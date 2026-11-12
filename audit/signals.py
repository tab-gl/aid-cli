#!/usr/bin/env python3
"""Signal and interrupted-write checks.

1. SIGINT while waiting for the browser: exits promptly, writes nothing, leaves no
   temp files and releases the callback port.
2. SIGKILL during a token refresh: the token file stays valid, the stale lock is
   recovered by the next command.
3. SIGINT during a slow API call: exits without writing a partial project file.

Usage: python3 audit/signals.py
"""

from __future__ import annotations

import json
import os
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

REPO = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
V2_BIN = os.path.join(REPO, "rust", "target", "release", "aid")


class Mock:
    def __init__(self, token_delay: float = 0.0, apps_delay: float = 0.0) -> None:
        class Handler(BaseHTTPRequestHandler):
            protocol_version = "HTTP/1.1"

            def log_message(self, *args: object) -> None:
                pass

            def _serve(self) -> None:
                length = int(self.headers.get("content-length") or 0)
                if length:
                    self.rfile.read(length)
                path = self.path.split("?")[0]
                if path == "/token":
                    time.sleep(token_delay)
                    payload = json.dumps(
                        {"access_token": "at-2", "token_type": "Bearer", "expires_in": 3600}
                    )
                elif path == "/apps":
                    time.sleep(apps_delay)
                    payload = json.dumps([{"id": "a1", "name": "Alpha"}])
                elif path == "/me":
                    payload = json.dumps({"name": "Yash Gupta", "sub": "user-1"})
                else:
                    payload = "{}"
                data = payload.encode()
                self.send_response(200)
                self.send_header("content-type", "application/json")
                self.send_header("content-length", str(len(data)))
                self.send_header("connection", "close")
                self.end_headers()
                try:
                    self.wfile.write(data)
                except (BrokenPipeError, ConnectionResetError):
                    pass

            do_GET = _serve
            do_POST = _serve

        self.server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.server.daemon_threads = True
        self.base = f"http://127.0.0.1:{self.server.server_address[1]}"
        threading.Thread(target=self.server.serve_forever, daemon=True).start()

    def close(self) -> None:
        self.server.shutdown()
        self.server.server_close()


def make_env(
    mock: Mock,
    expires_in_ms: int = 3_600_000,
    refresh: bool = True,
    seed: bool = True,
) -> tuple[str, str, str, dict[str, str]]:
    root = tempfile.mkdtemp(prefix="aid-signals-")
    workdir = os.path.join(root, "project")
    config_home = os.path.join(root, "config")
    os.makedirs(os.path.join(config_home, "aidc"))
    os.makedirs(workdir)
    now = int(time.time() * 1000)
    if not seed:
        env = dict(os.environ)
        env.update(
            {
                "HOME": workdir,
                "XDG_CONFIG_HOME": config_home,
                "NO_COLOR": "1",
                "TERM": "dumb",
                "AIDC_ISSUER": mock.base,
                "BROWSER": "true",
            }
        )
        return root, workdir, config_home, env
    document: dict[str, object] = {
        "access_token": "at-1",
        "token_type": "Bearer",
        "expires_at": now + expires_in_ms,
    }
    if refresh:
        document["refresh_token"] = "rt-1"
    path = os.path.join(config_home, "aidc", "tokens.json")
    with open(path, "w", encoding="utf-8") as handle:
        json.dump(document, handle, indent=2)
    os.chmod(path, 0o600)
    env = dict(os.environ)
    env.update(
        {
            "HOME": workdir,
            "XDG_CONFIG_HOME": config_home,
            "NO_COLOR": "1",
            "TERM": "dumb",
            "AIDC_ISSUER": mock.base,
            "BROWSER": "true",
        }
    )
    return root, workdir, config_home, env


def check(name: str, condition: bool, detail: str = "") -> bool:
    print(f"  {'PASS' if condition else 'FAIL'}  {name}" + (f" — {detail}" if detail and not condition else ""))
    return condition


def free_port() -> int:
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


def temp_files(config_home: str) -> list[str]:
    directory = os.path.join(config_home, "aidc")
    if not os.path.isdir(directory):
        return []
    return [
        name
        for name in os.listdir(directory)
        if name.endswith(".tmp")
    ]


def main() -> int:
    if not os.path.exists(V2_BIN):
        print(f"missing v2 binary at {V2_BIN}", file=sys.stderr)
        return 2
    results: list[bool] = []

    print("\n1. SIGINT while waiting for the browser")
    mock = Mock()
    root, workdir, config_home, env = make_env(mock, seed=False)
    port = free_port()
    env["AIDC_PORT"] = str(port)
    try:
        process = subprocess.Popen(
            [V2_BIN, "login"],
            cwd=workdir,
            env=env,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        time.sleep(0.8)  # listener bound, browser "opened", waiting
        started = time.time()
        process.send_signal(signal.SIGINT)
        try:
            process.communicate(timeout=10)
        except subprocess.TimeoutExpired:
            process.kill()
        elapsed = time.time() - started
        results.append(check("login exits on SIGINT", process.returncode is not None, str(process.returncode)))
        results.append(check("exit is non-zero", (process.returncode or 0) != 0, str(process.returncode)))
        results.append(check("exits promptly", elapsed < 5, f"{elapsed:.1f}s"))
        results.append(check("no token file written", not os.path.exists(os.path.join(config_home, "aidc", "tokens.json"))))
        results.append(check("no temp files left", temp_files(config_home) == [], str(temp_files(config_home))))
        # The callback port must be released.
        with socket.socket() as sock:
            sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
            try:
                sock.bind(("127.0.0.1", port))
                released = True
            except OSError:
                released = False
        results.append(check("callback port released", released))
    finally:
        shutil.rmtree(root, ignore_errors=True)
        mock.close()

    print("\n2. SIGKILL during a refresh")
    mock = Mock(token_delay=3.0)
    root, workdir, config_home, env = make_env(mock, expires_in_ms=-1_000)
    token_path = os.path.join(config_home, "aidc", "tokens.json")
    try:
        process = subprocess.Popen(
            [V2_BIN, "whoami"],
            cwd=workdir,
            env=env,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        time.sleep(0.6)  # refresh in flight, lock probably held
        process.kill()
        process.wait(timeout=10)
        results.append(check("killed process is gone", process.returncode is not None))

        try:
            with open(token_path, encoding="utf-8") as handle:
                stored = json.load(handle)
            valid = isinstance(stored, dict) and "access_token" in stored
        except Exception as error:  # noqa: BLE001
            valid = False
            stored = {"error": str(error)}
        results.append(check("token file still valid JSON", valid, json.dumps(stored)))
        results.append(check("no temp files left", temp_files(config_home) == [], str(temp_files(config_home))))

        # The next command must recover any stale lock left by the kill.
        recovery_mock = Mock()
        env["AIDC_ISSUER"] = recovery_mock.base
        try:
            follow_up = subprocess.run(
                [V2_BIN, "whoami"],
                cwd=workdir,
                env=env,
                stdin=subprocess.DEVNULL,
                capture_output=True,
                text=True,
                timeout=60,
            )
        finally:
            recovery_mock.close()
        results.append(
            check(
                "next command recovers from the crash",
                follow_up.returncode == 0,
                follow_up.stderr.strip()[-200:],
            )
        )
    finally:
        shutil.rmtree(root, ignore_errors=True)
        mock.close()

    print("\n3. SIGINT during a slow API call")
    mock = Mock(apps_delay=3.0)
    root, workdir, config_home, env = make_env(mock)
    try:
        process = subprocess.Popen(
            [V2_BIN, "setup"],
            cwd=workdir,
            env=env,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        time.sleep(0.8)
        process.send_signal(signal.SIGINT)
        try:
            process.communicate(timeout=10)
        except subprocess.TimeoutExpired:
            process.kill()
        results.append(check("setup exits on SIGINT", process.returncode is not None))
        results.append(
            check(
                "no partial project config written",
                not os.path.exists(os.path.join(workdir, ".aid.json")),
            )
        )
        results.append(
            check(
                "no recovery journal left behind",
                not os.path.exists(os.path.join(workdir, ".aid-setup-recovery.json")),
            )
        )
    finally:
        shutil.rmtree(root, ignore_errors=True)
        mock.close()

    failures = results.count(False)
    print(f"\n{len(results) - failures}/{len(results)} signal checks passed")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
