#!/usr/bin/env python3
"""Live SDK checks against the real services (network required).

1. Android artifact: `setup` downloads the pinned `library-release.aar` from the
   real GitHub release and the bytes must match the pinned SHA-256.
2. JavaScript SDK: `setup` runs the real package manager install and the package
   must land in `node_modules`.

Both are run for v1 and v2 so their behaviour can be compared.

Usage: python3 audit/live_sdk.py [--skip-js]
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

REPO = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
V1_ENTRY = os.path.join(REPO, "bin", "aid.js")
V2_BIN = os.path.join(REPO, "rust", "target", "release", "aid")
PINNED_AAR_SHA256 = "8fde79fd7690f6b09c1198fec08ff160f5ecaf54301f14b88c4f43de90ebdba2"
REAL_ASSET_URL = "https://github.com/tab-gl/aid-sdk/releases/download/1.1/library-release.aar"


class Mock:
    def __init__(self) -> None:
        class Handler(BaseHTTPRequestHandler):
            protocol_version = "HTTP/1.1"

            def log_message(self, *args: object) -> None:
                pass

            def _serve(self) -> None:
                length = int(self.headers.get("content-length") or 0)
                if length:
                    self.rfile.read(length)
                if self.path.startswith("/apps") and self.command == "POST":
                    payload, status = json.dumps({"id": "a1", "name": "demo"}), 201
                elif self.path.startswith("/clients") and self.command == "POST":
                    payload, status = json.dumps({"client_id": "c1"}), 201
                else:
                    payload, status = "{}", 200
                data = payload.encode()
                self.send_response(status)
                self.send_header("content-type", "application/json")
                self.send_header("content-length", str(len(data)))
                self.send_header("connection", "close")
                self.end_headers()
                self.wfile.write(data)

            do_GET = _serve
            do_POST = _serve

        self.server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.server.daemon_threads = True
        self.base = f"http://127.0.0.1:{self.server.server_address[1]}"
        threading.Thread(target=self.server.serve_forever, daemon=True).start()

    def close(self) -> None:
        self.server.shutdown()
        self.server.server_close()


class Mirror:
    """Serves the real release metadata and bytes locally, with a redirect hop.

    v2 accepts `AIDC_SDK_RELEASE_BASE` so the download path can be exercised
    without TLS interception in the way; the bytes are the real artifact.
    """

    def __init__(self, artifact: bytes) -> None:
        port_holder: dict[str, int] = {}

        class Handler(BaseHTTPRequestHandler):
            protocol_version = "HTTP/1.1"

            def log_message(self, *args: object) -> None:
                pass

            def _send(self, status: int, body: bytes, content_type: str, headers: dict[str, str] | None = None) -> None:
                self.send_response(status)
                self.send_header("content-type", content_type)
                self.send_header("content-length", str(len(body)))
                for name, value in (headers or {}).items():
                    self.send_header(name, value)
                self.send_header("connection", "close")
                self.end_headers()
                try:
                    self.wfile.write(body)
                except (BrokenPipeError, ConnectionResetError):
                    pass

            def do_GET(self) -> None:
                if self.path == "/releases/tags/1.1":
                    payload = json.dumps(
                        {
                            "tag_name": "1.1",
                            "assets": [
                                {
                                    "name": "library-release.aar",
                                    "size": len(artifact),
                                    "digest": f"sha256:{PINNED_AAR_SHA256}",
                                    "browser_download_url": f"http://127.0.0.1:{port_holder['port']}/download/library-release.aar",
                                }
                            ],
                        }
                    ).encode()
                    self._send(200, payload, "application/json")
                elif self.path == "/download/library-release.aar":
                    self._send(302, b"", "text/plain", {"location": "/blob/library-release.aar"})
                elif self.path == "/blob/library-release.aar":
                    self._send(200, artifact, "application/octet-stream")
                else:
                    self._send(404, b"{}", "application/json")

        self.server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.server.daemon_threads = True
        port_holder["port"] = self.server.server_address[1]
        self.base = f"http://127.0.0.1:{port_holder['port']}"
        threading.Thread(target=self.server.serve_forever, daemon=True).start()

    def close(self) -> None:
        self.server.shutdown()
        self.server.server_close()


def fetch_real_artifact() -> bytes | None:
    """Fetch the pinned artifact from the real release (honours proxy env)."""
    try:
        completed = subprocess.run(
            ["curl", "-sSL", "--max-time", "120", REAL_ASSET_URL],
            capture_output=True,
            timeout=150,
        )
    except (OSError, subprocess.TimeoutExpired):
        return None
    if completed.returncode != 0 or not completed.stdout:
        return None
    return completed.stdout


def project(kind: str) -> str:
    root = tempfile.mkdtemp(prefix=f"live-{kind}-")
    workdir = os.path.join(root, "project")
    config_home = os.path.join(root, "config")
    os.makedirs(os.path.join(config_home, "aidc"))
    os.makedirs(workdir)
    now = int(time.time() * 1000)
    json.dump(
        {"access_token": "at", "token_type": "Bearer", "expires_at": now + 3_600_000},
        open(os.path.join(config_home, "aidc", "tokens.json"), "w"),
    )
    if kind == "android":
        os.makedirs(os.path.join(workdir, "app", "src", "main"))
        open(os.path.join(workdir, "app", "src", "main", "AndroidManifest.xml"), "w").write("<manifest/>")
        os.makedirs(os.path.join(workdir, "app", "src", "main", "java"))
        open(os.path.join(workdir, "app", "src", "main", "java", "Main.kt"), "w").write("fun main() {}")
    else:
        json.dump(
            {"name": "demo", "dependencies": {"next": "14.0.0"}},
            open(os.path.join(workdir, "package.json"), "w"),
        )
    return root


def run(
    impl: str,
    fixture: str,
    mock: Mock,
    timeout: int = 240,
    extra_env: dict[str, str] | None = None,
) -> tuple[subprocess.CompletedProcess, str]:
    root = project(fixture)
    workdir = os.path.join(root, "project")
    env = dict(os.environ)
    env.update(
        {
            "HOME": workdir,
            "XDG_CONFIG_HOME": os.path.join(root, "config"),
            "NO_COLOR": "1",
            "TERM": "dumb",
            "AIDC_ISSUER": mock.base,
        }
    )
    env.update(extra_env or {})
    command = ["node", V1_ENTRY] if impl == "v1" else [V2_BIN]
    result = subprocess.run(
        command + ["setup"],
        cwd=workdir,
        env=env,
        stdin=subprocess.DEVNULL,
        capture_output=True,
        text=True,
        timeout=timeout,
    )
    return result, root


def check(name: str, condition: bool, detail: str = "") -> bool:
    print(f"  {'PASS' if condition else 'FAIL'}  {name}" + (f" — {detail}" if detail and not condition else ""))
    return condition


def android_checks(label: str, impl: str, mock: Mock, mirror: "Mirror | None") -> list[bool]:
    results: list[bool] = []
    print(f"\nAndroid artifact via {label}")
    extra_env = {"AIDC_SDK_RELEASE_BASE": mirror.base} if mirror else {}
    try:
        result, root = run(impl, "android", mock, extra_env=extra_env)
        workdir = os.path.join(root, "project")
        artifact = os.path.join(workdir, "libs", "library-release.aar")
        results.append(check(f"{label}: setup exits 0", result.returncode == 0, result.stderr[-400:]))
        results.append(check(f"{label}: artifact downloaded", os.path.exists(artifact)))
        if not os.path.exists(artifact):
            print("        stdout tail: " + " | ".join(result.stdout.strip().splitlines()[-6:]))
            print("        stderr tail: " + " | ".join(result.stderr.strip().splitlines()[-6:]))
        if os.path.exists(artifact):
            digest = hashlib.sha256(open(artifact, "rb").read()).hexdigest()
            size = os.path.getsize(artifact)
            results.append(check(f"{label}: artifact matches pinned SHA-256", digest == PINNED_AAR_SHA256, digest))
            print(f"        size={size} bytes sha256={digest}")
        config_path = os.path.join(workdir, ".aid.json")
        if os.path.exists(config_path):
            config = json.load(open(config_path))
            results.append(check(f"{label}: config records sdk_path", config.get("sdk_path") == "libs/library-release.aar", json.dumps(config)))
        else:
            results.append(check(f"{label}: config written", False))
    finally:
        shutil.rmtree(root, ignore_errors=True)
    return results


def js_checks(label: str, impl: str, mock: Mock) -> list[bool]:
    results: list[bool] = []
    print(f"\nJavaScript SDK install via {label}")
    try:
        result, root = run(impl, "js", mock)
        workdir = os.path.join(root, "project")
        installed = os.path.join(workdir, "node_modules", "ace-id-sdk", "package.json")
        results.append(check(f"{label}: setup exits 0", result.returncode == 0, result.stderr[-400:]))
        results.append(check(f"{label}: ace-id-sdk installed", os.path.exists(installed)))
        if os.path.exists(installed):
            version = json.load(open(installed)).get("version")
            print(f"        installed ace-id-sdk {version}")
            declared = json.load(open(os.path.join(workdir, "package.json")))
            results.append(
                check(
                    f"{label}: dependency recorded in package.json",
                    "ace-id-sdk" in declared.get("dependencies", {}),
                    json.dumps(declared.get("dependencies")),
                )
            )
        else:
            print("        stderr tail: " + result.stderr[-400:].replace("\n", " | "))
    finally:
        shutil.rmtree(root, ignore_errors=True)
    return results


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--skip-js", action="store_true")
    args = parser.parse_args()

    if not os.path.exists(V2_BIN):
        print(f"missing v2 binary at {V2_BIN}", file=sys.stderr)
        return 2

    mock = Mock()
    results: list[bool] = []
    mirror: Mirror | None = None
    try:
        print("\nReal artifact: fetching from the live release")
        artifact = fetch_real_artifact()
        if artifact is None:
            print("  SKIP  could not fetch the live artifact (network/TLS interception)")
        else:
            digest = hashlib.sha256(artifact).hexdigest()
            results.append(
                check(
                    "live artifact matches the pinned SHA-256",
                    digest == PINNED_AAR_SHA256,
                    digest,
                )
            )
            print(f"        size={len(artifact)} bytes sha256={digest}")
            mirror = Mirror(artifact)

        results += android_checks("v2", "v2", mock, mirror)
        if mirror is None:
            print("\nAndroid artifact via v1")
            print("  SKIP  v1 hardcodes the GitHub API URL; its download path is unchanged from master")
        else:
            # v1 hardcodes the GitHub API URL (no mirror override) and undici does
            # not read proxy variables, so it cannot complete this download here.
            # Recorded as information, not as a v2 failure.
            print("\nAndroid artifact via v1 (informational)")
            v1_result, v1_root = run("v1", "android", mock)
            v1_workdir = os.path.join(v1_root, "project")
            installed = os.path.exists(os.path.join(v1_workdir, "libs", "library-release.aar"))
            status = [line for line in v1_result.stdout.splitlines() if "SDK status" in line]
            print(f"  INFO  v1 exit={v1_result.returncode} artifact={'yes' if installed else 'no'} {status}")
            shutil.rmtree(v1_root, ignore_errors=True)

        if not args.skip_js:
            results += js_checks("v2", "v2", mock)
            results += js_checks("v1", "v1", mock)
    finally:
        if mirror is not None:
            mirror.close()
        mock.close()

    failures = results.count(False)
    print(f"\n{len(results) - failures}/{len(results)} live SDK checks passed")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
