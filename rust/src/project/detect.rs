//! Project detection.
//!
//! Port of `src/project/detect.js`. Detection is intentionally scoped to the
//! current directory: v2 never walks upward or infers a project from an
//! enclosing workspace.

use std::path::Path;

use serde_json::Value;

use crate::config::storage;
use crate::error::{AidError, Result};

pub const SDK_PACKAGE: &str = "ace-id-sdk";
pub const ANDROID_SDK_PACKAGE: &str = "ace-id-sdk-android";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SdkMode {
    /// SDK is a build dependency installed by the package manager.
    Build,
    /// SDK must be wired manually (vanilla JavaScript).
    Manual,
    /// Native Android artifact installed into `libs/`.
    Android,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    pub kind: String,
    pub label: String,
    pub sdk: String,
    pub sdk_mode: SdkMode,
    pub package_manager: Option<String>,
    /// Populated after a native SDK install.
    pub sdk_path: Option<String>,
    pub sdk_version: Option<String>,
}

/// Detect the project in the current working directory.
pub fn detect_project() -> Result<Project> {
    let root = std::env::current_dir()
        .map_err(|err| AidError::new(format!("Could not inspect project directory: {err}")))?;
    detect_project_in(&root)
}

/// Detect the project rooted at `root`.
pub fn detect_project_in(root: &Path) -> Result<Project> {
    let package = read_package_json(root)?;
    let entries = read_dir_names(root)?;
    let has = |name: &str| entries.iter().any(|entry| entry == name);

    if is_android_project(root, &has)? {
        let kotlin = android_uses_kotlin(root);
        return Ok(Project {
            kind: if kotlin {
                "android-kotlin"
            } else {
                "android-java"
            }
            .to_string(),
            label: if kotlin {
                "Android (Kotlin)"
            } else {
                "Android (Java)"
            }
            .to_string(),
            sdk: ANDROID_SDK_PACKAGE.to_string(),
            sdk_mode: SdkMode::Android,
            package_manager: None,
            sdk_path: None,
            sdk_version: None,
        });
    }

    let dependency = |name: &str| -> bool {
        package
            .as_ref()
            .map(|package| has_dependency_entry(package, name))
            .unwrap_or(false)
    };

    let package_manager = Some(detect_package_manager(&has));

    if dependency("next")
        || has("next.config.js")
        || has("next.config.mjs")
        || has("next.config.ts")
    {
        return Ok(Project {
            kind: "next".to_string(),
            label: "Next.js".to_string(),
            sdk: SDK_PACKAGE.to_string(),
            sdk_mode: SdkMode::Build,
            package_manager,
            sdk_path: None,
            sdk_version: None,
        });
    }

    if dependency("vite")
        || has("vite.config.js")
        || has("vite.config.mjs")
        || has("vite.config.ts")
    {
        return Ok(Project {
            kind: "vite".to_string(),
            label: if dependency("react") {
                "React + Vite"
            } else {
                "Vite"
            }
            .to_string(),
            sdk: SDK_PACKAGE.to_string(),
            sdk_mode: SdkMode::Build,
            package_manager,
            sdk_path: None,
            sdk_version: None,
        });
    }

    if dependency("react") || dependency("react-dom") || (has("src") && has_react_source(root)) {
        return Ok(Project {
            kind: "react".to_string(),
            label: "React".to_string(),
            sdk: SDK_PACKAGE.to_string(),
            sdk_mode: SdkMode::Build,
            package_manager,
            sdk_path: None,
            sdk_version: None,
        });
    }

    Ok(Project {
        kind: "vanilla".to_string(),
        label: "Vanilla JavaScript".to_string(),
        sdk: SDK_PACKAGE.to_string(),
        sdk_mode: SdkMode::Manual,
        package_manager,
        sdk_path: None,
        sdk_version: None,
    })
}

fn has_dependency_entry(package: &Value, name: &str) -> bool {
    ["dependencies", "devDependencies", "peerDependencies"]
        .iter()
        .any(|section| {
            package
                .get(*section)
                .and_then(Value::as_object)
                .map(|deps| deps.contains_key(name))
                .unwrap_or(false)
        })
}

fn detect_package_manager(has: &dyn Fn(&str) -> bool) -> String {
    if has("pnpm-lock.yaml") {
        "pnpm".to_string()
    } else if has("yarn.lock") {
        "yarn".to_string()
    } else if has("bun.lockb") || has("bun.lock") {
        "bun".to_string()
    } else {
        "npm".to_string()
    }
}

fn is_android_project(root: &Path, has: &dyn Fn(&str) -> bool) -> Result<bool> {
    if storage::exists(
        &root
            .join("app")
            .join("src")
            .join("main")
            .join("AndroidManifest.xml"),
    )? || has("gradlew")
    {
        return Ok(true);
    }

    for file in [
        "settings.gradle",
        "settings.gradle.kts",
        "build.gradle",
        "build.gradle.kts",
    ] {
        if let Some(content) = storage::read_to_string(&root.join(file))? {
            if content.contains("com.android.application")
                || content.contains("com.android.library")
                || content.contains("com.android.tools.build:gradle")
                || includes_app_module(&content)
            {
                return Ok(true);
            }
        }
    }

    Ok(false)
}

/// Matches `include(":app")` / `include ':app'` in Gradle settings files.
fn includes_app_module(content: &str) -> bool {
    let mut rest = content;
    while let Some(index) = rest.find("include") {
        rest = &rest[index + "include".len()..];
        let trimmed = rest.trim_start();
        let trimmed = trimmed.strip_prefix('(').unwrap_or(trimmed).trim_start();
        if trimmed.starts_with("':app") || trimmed.starts_with("\":app") {
            return true;
        }
    }
    false
}

fn android_uses_kotlin(root: &Path) -> bool {
    let mut found_java = false;
    let mut found_kotlin = false;

    let roots = [
        root.join("app").join("src").join("main"),
        root.join("src").join("main"),
    ];
    for start in roots {
        walk_sources(&start, &mut found_java, &mut found_kotlin);
        if found_java && found_kotlin {
            break;
        }
    }

    // v1 prefers Kotlin when both are present.
    found_kotlin || !found_java
}

fn walk_sources(dir: &Path, found_java: &mut bool, found_kotlin: &mut bool) {
    if *found_java && *found_kotlin {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(_) => continue,
        };
        if file_type.is_dir() {
            if ["build", ".gradle", ".git", "node_modules", ".idea"].contains(&name.as_str()) {
                continue;
            }
            walk_sources(&path, found_java, found_kotlin);
        } else if file_type.is_file() {
            if name.ends_with(".java") {
                *found_java = true;
            }
            if name.ends_with(".kt") || name.ends_with(".kts") {
                *found_kotlin = true;
            }
        }
        if *found_java && *found_kotlin {
            return;
        }
    }
}

fn has_react_source(root: &Path) -> bool {
    ["src/App.jsx", "src/App.tsx", "src/main.jsx", "src/main.tsx"]
        .iter()
        .any(|file| root.join(file).is_file())
}

fn read_dir_names(root: &Path) -> Result<Vec<String>> {
    match std::fs::read_dir(root) {
        Ok(entries) => Ok(entries
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .collect()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(err) => Err(AidError::new(format!(
            "Could not inspect project directory: {err}"
        ))),
    }
}

/// Read and validate `package.json`, returning `None` when it is absent.
pub fn read_package_json(root: &Path) -> Result<Option<Value>> {
    let file = root.join("package.json");
    let raw = match storage::read_to_string(&file)? {
        Some(raw) => raw,
        None => return Ok(None),
    };
    let value: Value = serde_json::from_str(&raw).map_err(|_| {
        AidError::new("package.json contains invalid JSON. Fix it before running setup.")
    })?;
    let object = value
        .as_object()
        .ok_or_else(|| AidError::new("package.json must contain a JSON object."))?;

    for key in ["dependencies", "devDependencies", "peerDependencies"] {
        if let Some(section) = object.get(key) {
            if !section.is_object() {
                return Err(AidError::new(format!(
                    "package.json field \"{key}\" must be an object."
                )));
            }
        }
    }
    Ok(Some(value))
}

/// Declared version of `name` in any dependency section.
pub fn declared_dependency_version(root: &Path, name: &str) -> Option<String> {
    let package = read_package_json(root).ok().flatten()?;
    for section in ["dependencies", "devDependencies", "peerDependencies"] {
        if let Some(version) = package
            .get(section)
            .and_then(Value::as_object)
            .and_then(|deps| deps.get(name))
            .and_then(Value::as_str)
        {
            return Some(version.to_string());
        }
    }
    None
}

/// Version of the SDK installed under `node_modules`, when present.
pub fn installed_sdk_version(root: &Path, sdk: &str) -> Option<String> {
    if sdk.is_empty() {
        return None;
    }
    let mut path = root.join("node_modules");
    for segment in sdk.split('/') {
        path = path.join(segment);
    }
    path = path.join("package.json");
    let raw = storage::read_to_string(&path).ok().flatten()?;
    let value: Value = serde_json::from_str(&raw).ok()?;
    value
        .get("version")
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// Project name used as the default application name.
pub fn project_name(root: &Path) -> String {
    if let Ok(Some(package)) = read_package_json(root) {
        if let Some(name) = package.get("name").and_then(Value::as_str) {
            if !name.trim().is_empty() {
                return name.trim().to_string();
            }
        }
    }
    root.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "Ace ID App".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(tag: &str) -> std::path::PathBuf {
        let mut random = [0u8; 6];
        getrandom::getrandom(&mut random).unwrap();
        let dir = std::env::temp_dir().join(format!(
            "ace-detect-{tag}-{}-{}",
            std::process::id(),
            crate::util::hex(&random)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(path: &Path, content: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }

    #[test]
    fn detects_next_project_from_dependency() {
        let dir = fixture("next");
        write(
            &dir.join("package.json"),
            r#"{"name":"my-app","dependencies":{"next":"14.0.0"}}"#,
        );
        write(&dir.join("pnpm-lock.yaml"), "");
        let project = detect_project_in(&dir).unwrap();
        assert_eq!(project.kind, "next");
        assert_eq!(project.label, "Next.js");
        assert_eq!(project.package_manager.as_deref(), Some("pnpm"));
        assert_eq!(project.sdk, SDK_PACKAGE);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn detects_react_vite() {
        let dir = fixture("vite");
        write(
            &dir.join("package.json"),
            r#"{"dependencies":{"vite":"5.0.0","react":"18.0.0"}}"#,
        );
        let project = detect_project_in(&dir).unwrap();
        assert_eq!(project.kind, "vite");
        assert_eq!(project.label, "React + Vite");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn detects_react_from_source_files() {
        let dir = fixture("react-src");
        write(&dir.join("src/App.tsx"), "export default () => null;");
        let project = detect_project_in(&dir).unwrap();
        assert_eq!(project.kind, "react");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn falls_back_to_vanilla() {
        let dir = fixture("vanilla");
        let project = detect_project_in(&dir).unwrap();
        assert_eq!(project.kind, "vanilla");
        assert_eq!(project.sdk_mode, SdkMode::Manual);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn detects_android_kotlin_and_java() {
        let kotlin = fixture("android-kotlin");
        write(
            &kotlin.join("app/src/main/AndroidManifest.xml"),
            "<manifest/>",
        );
        write(&kotlin.join("app/src/main/java/Main.kt"), "fun main() {}");
        let project = detect_project_in(&kotlin).unwrap();
        assert_eq!(project.kind, "android-kotlin");
        assert_eq!(project.sdk_mode, SdkMode::Android);
        std::fs::remove_dir_all(&kotlin).ok();

        let java = fixture("android-java");
        write(&java.join("settings.gradle"), "include ':app'");
        write(&java.join("app/src/main/java/Main.java"), "class Main {}");
        let project = detect_project_in(&java).unwrap();
        assert_eq!(project.kind, "android-java");
        std::fs::remove_dir_all(&java).ok();
    }

    #[test]
    fn invalid_package_json_is_reported() {
        let dir = fixture("invalid");
        write(&dir.join("package.json"), "{ not json");
        let err = detect_project_in(&dir).unwrap_err();
        assert!(err.message().contains("invalid JSON"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn dependency_sections_must_be_objects() {
        let dir = fixture("deps");
        write(&dir.join("package.json"), r#"{"dependencies":[]}"#);
        let err = detect_project_in(&dir).unwrap_err();
        assert!(err.message().contains("must be an object"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn project_name_prefers_package_name() {
        let dir = fixture("name");
        write(&dir.join("package.json"), r#"{"name":"  spaced  "}"#);
        assert_eq!(project_name(&dir), "spaced");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn installed_version_reads_node_modules() {
        let dir = fixture("installed");
        write(
            &dir.join("node_modules/ace-id-sdk/package.json"),
            r#"{"version":"1.2.3"}"#,
        );
        assert_eq!(
            installed_sdk_version(&dir, SDK_PACKAGE).as_deref(),
            Some("1.2.3")
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
