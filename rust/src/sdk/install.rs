//! SDK installation.
//!
//! * JavaScript SDKs are installed with the project's own package manager.
//! * The Android artifact is fetched from a pinned GitHub release, its release
//!   digest and its downloaded bytes are both checked against the pinned SHA-256,
//!   and only then is it written into `libs/`.

use std::path::Path;
use std::process::{Command, Stdio};

use serde_json::Value;

use crate::api::errors::read_api_error;
use crate::config::{paths, storage, Config};
use crate::error::{AidError, Result};
use crate::http::{HttpError, Request};
use crate::project::{detect::read_package_json, Project, SdkMode};
use crate::sdk::verify::{sha256_bytes, ANDROID_SDK_SHA256};
use crate::terminal::warn;

pub const ANDROID_SDK_REPO: &str = "aceyash-dev/ace-id-sdk";
/// Base URL for the SDK release metadata. Overridable so air-gapped installs and
/// tests can point at a mirror; the pinned digest is still enforced.
pub const SDK_RELEASE_BASE_ENV: &str = "AIDC_SDK_RELEASE_BASE";
pub const ANDROID_SDK_RELEASE: &str = "1.1";
pub const ANDROID_SDK_ASSET: &str = "library-release.aar";
pub const ANDROID_SDK_DIR: &str = "libs";

/// Outcome labels shown in the setup summary (identical strings to v1).
pub const STATUS_INSTALLED: &str = "installed";
pub const STATUS_ALREADY_INSTALLED: &str = "already installed";
pub const STATUS_NOT_APPLICABLE: &str = "not applicable";
pub const STATUS_SKIPPED: &str = "skipped";
pub const STATUS_INSTALL_FAILED: &str = "install failed";

/// True when `name` appears in any dependency section of `package.json`.
pub fn has_dependency(root: &Path, name: &str) -> bool {
    read_package_json(root)
        .ok()
        .flatten()
        .map(|package| {
            ["dependencies", "devDependencies", "peerDependencies"]
                .iter()
                .any(|section| {
                    package
                        .get(*section)
                        .and_then(Value::as_object)
                        .map(|deps| deps.contains_key(name))
                        .unwrap_or(false)
                })
        })
        .unwrap_or(false)
}

/// Install a JavaScript SDK with the detected package manager.
///
/// An install failure is reported as `install failed` rather than aborting the
/// whole setup, matching v1's tolerant behaviour.
pub fn install_js_sdk(root: &Path, project: &Project) -> String {
    let manager = project
        .package_manager
        .clone()
        .unwrap_or_else(|| "npm".to_string());
    let (executable, args) = package_install_command(&manager, &project.sdk);
    let executable = executable_for_platform(&executable);

    match Command::new(&executable)
        .args(&args)
        .current_dir(root)
        .stdin(Stdio::null())
        .output()
    {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            if !stdout.trim().is_empty() {
                println!("{}", stdout.trim());
            }
            if !stderr.trim().is_empty() {
                eprintln!("{}", stderr.trim());
            }
            if output.status.success() {
                STATUS_INSTALLED.to_string()
            } else {
                warn(&format!(
                    "SDK install was skipped after an install error. {} exited with status {}",
                    executable,
                    output.status.code().unwrap_or(1)
                ));
                STATUS_INSTALL_FAILED.to_string()
            }
        }
        Err(err) => {
            warn(&format!(
                "SDK install was skipped after an install error. {err}"
            ));
            STATUS_INSTALL_FAILED.to_string()
        }
    }
}

fn package_install_command(manager: &str, package: &str) -> (String, Vec<String>) {
    match manager {
        "pnpm" => (
            "pnpm".to_string(),
            vec!["add".to_string(), package.to_string()],
        ),
        "yarn" => (
            "yarn".to_string(),
            vec!["add".to_string(), package.to_string()],
        ),
        "bun" => (
            "bun".to_string(),
            vec!["add".to_string(), package.to_string()],
        ),
        _ => (
            "npm".to_string(),
            vec!["install".to_string(), package.to_string()],
        ),
    }
}

fn executable_for_platform(executable: &str) -> String {
    if cfg!(windows) {
        format!("{executable}.cmd")
    } else {
        executable.to_string()
    }
}

/// Download, verify and install the pinned Android SDK artifact.
///
/// Returns the project-relative path of the installed artifact.
pub fn install_android_sdk(config: &Config, root: &Path) -> Result<String> {
    let base = std::env::var(SDK_RELEASE_BASE_ENV)
        .ok()
        .map(|value| value.trim().trim_end_matches('/').to_string())
        .filter(|value| value.starts_with("https://") || value.starts_with("http://127.0.0.1"))
        .unwrap_or_else(|| format!("https://api.github.com/repos/{ANDROID_SDK_REPO}"));
    let release_url = format!("{base}/releases/tags/{ANDROID_SDK_RELEASE}");
    // The GitHub API 301-redirects when a repository is renamed, and asset
    // downloads redirect to a CDN. These requests carry no credentials, so a
    // bounded redirect is followed; the pinned SHA-256 remains the control.
    let release = Request::new("GET", &release_url, config.request_timeout)
        .header("accept", "application/vnd.github+json")
        .header("user-agent", "ace-id-cli")
        .follow_redirects(5)
        .send()
        .map_err(|err| sdk_transport_error(config, err))?;

    if !(200..300).contains(&release.status) {
        return Err(AidError::new(format!(
            "Could not find Android SDK release {} on GitHub (HTTP {}).",
            ANDROID_SDK_RELEASE, release.status
        )));
    }

    let value: Value = serde_json::from_slice(&release.body).map_err(|_| {
        AidError::new("The Android SDK release metadata from GitHub was not valid JSON.")
    })?;
    let asset = value
        .get("assets")
        .and_then(Value::as_array)
        .and_then(|assets| {
            assets
                .iter()
                .find(|asset| asset.get("name").and_then(Value::as_str) == Some(ANDROID_SDK_ASSET))
        })
        .ok_or_else(|| {
            AidError::new(format!(
                "Ace ID SDK release {ANDROID_SDK_RELEASE} does not contain an .aar asset."
            ))
        })?;

    let download_url = asset
        .get("browser_download_url")
        .and_then(Value::as_str)
        .filter(|url| url.starts_with("https://") || url.starts_with("http://127.0.0.1"))
        .ok_or_else(|| {
            AidError::new(format!(
                "Ace ID SDK release {ANDROID_SDK_RELEASE} does not contain an .aar asset."
            ))
        })?;

    let expected_digest = format!("sha256:{ANDROID_SDK_SHA256}");
    if asset.get("digest").and_then(Value::as_str) != Some(expected_digest.as_str()) {
        return Err(AidError::new(format!(
            "Android SDK release {ANDROID_SDK_RELEASE} digest does not match the pinned artifact."
        )));
    }

    let download = Request::new("GET", download_url, config.request_timeout)
        .header("user-agent", "ace-id-cli")
        .follow_redirects(5)
        .send()
        .map_err(|err| sdk_transport_error(config, err))?;

    if !(200..300).contains(&download.status) {
        let detail = read_api_error(
            download.status,
            download.content_type.as_deref(),
            &download.body,
        );
        return Err(AidError::new(format!(
            "Failed to download {ANDROID_SDK_ASSET} (HTTP {}): {detail}",
            download.status
        )));
    }

    if sha256_bytes(&download.body) != ANDROID_SDK_SHA256 {
        return Err(AidError::new(format!(
            "Android SDK integrity check failed for {ANDROID_SDK_ASSET}. Installation was aborted."
        )));
    }

    let target = paths::ensure_within(
        root,
        Path::new(ANDROID_SDK_DIR).join(ANDROID_SDK_ASSET).as_path(),
    )?;
    storage::write_atomic(&target, &download.body, storage::PUBLIC_FILE_MODE)?;

    Ok(format!("{ANDROID_SDK_DIR}/{ANDROID_SDK_ASSET}"))
}

fn sdk_transport_error(config: &Config, err: HttpError) -> AidError {
    match err {
        HttpError::Timeout => AidError::new(format!(
            "SDK download request timed out after {}s.",
            config.request_timeout_label()
        )),
        HttpError::Redirect(reason) => {
            AidError::new(format!("SDK download request failed: {reason}."))
        }
        HttpError::Transport(message) => {
            AidError::new(format!("SDK download request failed: {message}"))
        }
    }
}

/// Install the SDK appropriate for the detected project, if any.
pub fn install_for_project(config: &Config, root: &Path, project: &mut Project) -> Result<String> {
    match project.sdk_mode {
        SdkMode::Android => {
            let relative = install_android_sdk(config, root)?;
            project.sdk_path = Some(relative);
            Ok(STATUS_INSTALLED.to_string())
        }
        SdkMode::Build => {
            if has_dependency(root, &project.sdk) {
                return Ok(STATUS_ALREADY_INSTALLED.to_string());
            }
            Ok(install_js_sdk(root, project))
        }
        SdkMode::Manual => Ok(STATUS_NOT_APPLICABLE.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(tag: &str) -> std::path::PathBuf {
        let mut random = [0u8; 6];
        getrandom::getrandom(&mut random).unwrap();
        let dir = std::env::temp_dir().join(format!(
            "ace-sdk-{tag}-{}-{}",
            std::process::id(),
            crate::util::hex(&random)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn dependency_detection_reads_all_sections() {
        let dir = fixture("dep");
        std::fs::write(
            dir.join("package.json"),
            r#"{"devDependencies":{"ace-id-sdk":"^1.0.0"}}"#,
        )
        .unwrap();
        assert!(has_dependency(&dir, "ace-id-sdk"));
        assert!(!has_dependency(&dir, "other-sdk"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn package_install_commands_match_v1() {
        assert_eq!(
            package_install_command("pnpm", "ace-id-sdk"),
            (
                "pnpm".to_string(),
                vec!["add".to_string(), "ace-id-sdk".to_string()]
            )
        );
        assert_eq!(
            package_install_command("unknown", "ace-id-sdk").0,
            "npm".to_string()
        );
    }

    #[test]
    fn manual_projects_skip_the_sdk_step() {
        let dir = fixture("manual");
        let config = Config::from_lookup(|_| None).unwrap();
        let mut project = Project {
            kind: "vanilla".to_string(),
            label: "Vanilla JavaScript".to_string(),
            sdk: crate::project::detect::SDK_PACKAGE.to_string(),
            sdk_mode: SdkMode::Manual,
            package_manager: Some("npm".to_string()),
            sdk_path: None,
            sdk_version: None,
        };
        let status = install_for_project(&config, &dir, &mut project).unwrap();
        assert_eq!(status, STATUS_NOT_APPLICABLE);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn already_declared_dependency_is_not_reinstalled() {
        let dir = fixture("declared");
        std::fs::write(
            dir.join("package.json"),
            r#"{"dependencies":{"ace-id-sdk":"1.0.0"}}"#,
        )
        .unwrap();
        let config = Config::from_lookup(|_| None).unwrap();
        let mut project = Project {
            kind: "next".to_string(),
            label: "Next.js".to_string(),
            sdk: crate::project::detect::SDK_PACKAGE.to_string(),
            sdk_mode: SdkMode::Build,
            package_manager: Some("npm".to_string()),
            sdk_path: None,
            sdk_version: None,
        };
        let status = install_for_project(&config, &dir, &mut project).unwrap();
        assert_eq!(status, STATUS_ALREADY_INSTALLED);
        std::fs::remove_dir_all(&dir).ok();
    }
}
