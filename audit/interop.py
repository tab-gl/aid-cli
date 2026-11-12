#!/usr/bin/env python3
"""v1 (Node) <-> v2 (Rust) interoperability checks.

1. Both CLIs can use one stored session, in both directions.
2. A refresh is performed exactly once when both CLIs race on an expired token.
3. File modes written by v2 are what v1 expects (0600), and v1 can delete them.

Usage: python3 audit/interop.py
"""

from __future__ import annotations

import json
import os
import re
import shutil
import socket
import stat
import subprocess
import sys
import tempfile
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

REPO = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
V1_ENTRY = os.path.join(REPO, "bin", "aid.js")
V2_BIN = os.path.join(REPO, "rust", "target", "release", "aid")

TOKEN_OK = json.dumps(
    {
        "access_token": "at-1",
        "refresh_token": "rt-1",
        "token_type": "Bearer",
        "expires_in": 3600,
    }
)
ME = json.dumps({"name": "Yash Gupta", "email": "yash@example.com", "sub": "user-1"})


class Mock:
    def __init__(self, token_delay: float = 0.0):
        self.requests: list[tuple[str, str]] = []
        self.lock = threading.Lock()
        mock = self

        class Handler(BaseHTTPRequestHandler):
            protocol_version = "HTTP/1.1"

            def log_message(self, *args: object) -> None:
                pass

            def _serve(self) -> None:
                length = int(self.headers.get("content-length") or 0)
                body = self.rfile.read(length) if length else b""
                path = self.path.split("?")[0]
                with mock.lock:
                    mock.requests.append((self.command, path))
                if path == "/token":
                    time.sleep(token_delay)
                    payload, status = TOKEN_OK, 200
                elif path == "/me":
                    payload, status = ME, 200
                else:
                    payload, status = "{}", 200
                data = payload.encode()
                self.send_response(status)
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

    def count(self, method: str, path: str) -> int:
        with self.lock:
            return sum(1 for m, p in self.requests if m == method and p == path)

    def reset(self) -> None:
        with self.lock:
            self.requests.clear()

    def close(self) -> None:
        self.server.shutdown()
        self.server.server_close()


def env_for(workdir: str, config_home: str, issuer: str) -> dict[str, str]:
    env = dict(os.environ)
    env.update(
        {
            "HOME": workdir,
            "XDG_CONFIG_HOME": config_home,
            "NO_COLOR": "1",
            "TERM": "dumb",
            "AIDC_ISSUER": issuer,
            "BROWSER": "true",
        }
    )
    return env


def seed(config_home: str, expires_in_ms: int) -> None:
    directory = os.path.join(config_home, "aidc")
    os.makedirs(directory, exist_ok=True)
    now = int(time.time() * 1000)
    document = {
        "access_token": "at-seeded",
        "refresh_token": "rt-seeded",
        "token_type": "Bearer",
        "expires_at": now + expires_in_ms,
    }
    path = os.path.join(directory, "tokens.json")
    with open(path, "w", encoding="utf-8") as handle:
        json.dump(document, handle, indent=2)
    os.chmod(path, 0o600)


def free_port() -> int:
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


def v2_login(mock: Mock, workdir: str, config_home: str) -> subprocess.CompletedProcess:
    """Drive `aid login` end to end: read the authorize URL, then play the browser."""
    port = free_port()
    env = env_for(workdir, config_home, mock.base)
    env["AIDC_PORT"] = str(port)
    process = subprocess.Popen(
        [V2_BIN, "login"],
        cwd=workdir,
        env=env,
        stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    authorize = ""
    assert process.stdout is not None
    for line in process.stdout:
        if "/auth?" in line:
            authorize = line.strip()
            break
    match = re.search(r"state=([^&]+)", authorize)
    if not match:
        process.kill()
        raise RuntimeError(f"no state in authorize URL: {authorize!r}")
    state = match.group(1)
    with socket.create_connection(("127.0.0.1", port), timeout=5) as sock:
        sock.sendall(
            f"GET /callback?state={state}&code=code-1 HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n".encode()
        )
        response = b""
        while True:
            chunk = sock.recv(4096)
            if not chunk:
                break
            response += chunk
    if b"200" not in response.split(b"\r\n")[0]:
        process.kill()
        raise RuntimeError(f"callback rejected: {response[:120]!r}")
    out, err = process.communicate(timeout=30)
    return subprocess.CompletedProcess(process.args, process.returncode, out, err)


def check(name: str, condition: bool, detail: str = "") -> bool:
    print(f"  {'PASS' if condition else 'FAIL'}  {name}" + (f" — {detail}" if detail and not condition else ""))
    return condition


def main() -> int:
    if not os.path.exists(V2_BIN):
        print(f"missing v2 binary at {V2_BIN}", file=sys.stderr)
        return 2

    results: list[bool] = []

    print("\n1. v2 login -> v1 reads the same session")
    mock = Mock()
    root = tempfile.mkdtemp(prefix="interop-")
    workdir = os.path.join(root, "project")
    config_home = os.path.join(root, "config")
    os.makedirs(workdir)
    os.makedirs(config_home)
    try:
        login = v2_login(mock, workdir, config_home)
        results.append(check("v2 login succeeds", login.returncode == 0, login.stderr.strip()))

        tokens_path = os.path.join(config_home, "aidc", "tokens.json")
        mode = stat.S_IMODE(os.stat(tokens_path).st_mode)
        results.append(check("token file mode is 0600", mode == 0o600, oct(mode)))

        stored = json.load(open(tokens_path))
        results.append(
            check(
                "session records its issuer",
                stored.get("issuer") == mock.base,
                json.dumps(stored),
            )
        )

        mock.reset()
        v1 = subprocess.run(
            ["node", V1_ENTRY, "whoami"],
            cwd=workdir,
            env=env_for(workdir, config_home, mock.base),
            stdin=subprocess.DEVNULL,
            capture_output=True,
            text=True,
            timeout=60,
        )
        results.append(check("v1 whoami works with the v2 session", v1.returncode == 0, v1.stderr[-300:]))
        results.append(check("v1 sees the account", "Yash Gupta" in v1.stdout, v1.stdout))

        mock.reset()
        v1_logout = subprocess.run(
            ["node", V1_ENTRY, "logout"],
            cwd=workdir,
            env=env_for(workdir, config_home, mock.base),
            stdin=subprocess.DEVNULL,
            capture_output=True,
            text=True,
            timeout=60,
        )
        results.append(
            check(
                "v1 logout removes the v2 session",
                v1_logout.returncode == 0 and not os.path.exists(tokens_path),
                v1_logout.stderr[-300:],
            )
        )
    finally:
        shutil.rmtree(root, ignore_errors=True)
        mock.close()

    print("\n2. concurrent refresh: v1 and v2 race on one expired session")
    mock = Mock(token_delay=0.4)
    root = tempfile.mkdtemp(prefix="interop-race-")
    workdir = os.path.join(root, "project")
    config_home = os.path.join(root, "config")
    os.makedirs(workdir)
    os.makedirs(config_home)
    try:
        seed(config_home, -1_000)
        env = env_for(workdir, config_home, mock.base)
        v1 = subprocess.Popen(
            ["node", V1_ENTRY, "whoami"],
            cwd=workdir,
            env=env,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        v2 = subprocess.Popen(
            [V2_BIN, "whoami"],
            cwd=workdir,
            env=env,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        v1_out, v1_err = v1.communicate(timeout=90)
        v2_out, v2_err = v2.communicate(timeout=90)
        results.append(check("v1 exits 0", v1.returncode == 0, v1_err[-300:]))
        results.append(check("v2 exits 0", v2.returncode == 0, v2_err[-300:]))
        refreshes = mock.count("POST", "/token")
        results.append(check("exactly one refresh happened", refreshes == 1, f"{refreshes} refreshes"))
        stored = json.load(open(os.path.join(config_home, "aidc", "tokens.json")))
        results.append(
            check("both processes used the refreshed token", stored.get("access_token") == "at-1", json.dumps(stored))
        )
    finally:
        shutil.rmtree(root, ignore_errors=True)
        mock.close()

    print("\n3. v1 writes, v2 reads")
    mock = Mock()
    root = tempfile.mkdtemp(prefix="interop-v1write-")
    workdir = os.path.join(root, "project")
    config_home = os.path.join(root, "config")
    os.makedirs(workdir)
    os.makedirs(config_home)
    try:
        seed(config_home, 3_600_000)
        v1 = subprocess.run(
            ["node", V1_ENTRY, "whoami"],
            cwd=workdir,
            env=env_for(workdir, config_home, mock.base),
            stdin=subprocess.DEVNULL,
            capture_output=True,
            text=True,
            timeout=60,
        )
        results.append(check("v1 whoami works with a v1-shaped session", v1.returncode == 0, v1.stderr[-300:]))
        v2 = subprocess.run(
            [V2_BIN, "whoami"],
            cwd=workdir,
            env=env_for(workdir, config_home, mock.base),
            stdin=subprocess.DEVNULL,
            capture_output=True,
            text=True,
            timeout=60,
        )
        results.append(check("v2 whoami works with the same session", v2.returncode == 0, v2.stderr[-300:]))
    finally:
        shutil.rmtree(root, ignore_errors=True)
        mock.close()

    failures = results.count(False)
    print(f"\n{len(results) - failures}/{len(results)} interop checks passed")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
