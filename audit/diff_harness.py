#!/usr/bin/env python3
"""Differential audit harness: run the v1 (Node) and v2 (Rust) CLIs against the
same mock API with identical inputs, then compare stdout, stderr, exit code and
the resulting configuration files byte for byte.

Usage: python3 diff_harness.py [--json out.json] [-v]
"""

from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import threading
import time
from dataclasses import dataclass, field
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from typing import Any

REPO = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
V1_ENTRY = os.path.join(REPO, "bin", "aid.js")
V2_BIN = os.path.join(REPO, "rust", "target", "release", "aid")


# ── Mock server ──────────────────────────────────────────────────────────────


@dataclass
class Route:
    method: str
    path: str
    status: int = 200
    body: str = ""
    content_type: str = "application/json"
    headers: dict[str, str] = field(default_factory=dict)
    delay: float = 0.0


class Scenario:
    def __init__(self, routes: list[Route]):
        self.routes = routes
        self.requests: list[dict[str, Any]] = []
        self._lock = threading.Lock()
        self.server = ThreadingHTTPServer(("127.0.0.1", 0), self._handler())
        self.server.daemon_threads = True
        self.base = f"http://127.0.0.1:{self.server.server_address[1]}"
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)

    def __enter__(self) -> "Scenario":
        self.thread.start()
        return self

    def __exit__(self, *exc: object) -> None:
        self.server.shutdown()
        self.server.server_close()

    def reset(self) -> None:
        with self._lock:
            self.requests.clear()

    def _handler(self):
        scenario = self

        class Handler(BaseHTTPRequestHandler):
            protocol_version = "HTTP/1.1"

            def log_message(self, *args: object) -> None:  # silence
                pass

            def _serve(self) -> None:
                length = int(self.headers.get("content-length") or 0)
                body = self.rfile.read(length) if length else b""
                path = self.path.split("?")[0]
                with scenario._lock:
                    scenario.requests.append(
                        {
                            "method": self.command,
                            "path": path,
                            "query": self.path.partition("?")[2],
                            "headers": dict(self.headers),
                            "body": body.decode("utf-8", "replace"),
                        }
                    )
                route = next(
                    (
                        r
                        for r in scenario.routes
                        if r.path == path and r.method.upper() == self.command.upper()
                    ),
                    None,
                )
                if route is None:
                    payload = f"no route for {self.command} {path}".encode()
                    self.send_response(404)
                    self.send_header("content-type", "text/plain")
                    self.send_header("content-length", str(len(payload)))
                    self.send_header("connection", "close")
                    self.end_headers()
                    self.wfile.write(payload)
                    return
                if route.delay:
                    time.sleep(route.delay)
                payload = route.body.encode()
                self.send_response(route.status)
                self.send_header("content-type", route.content_type)
                self.send_header("content-length", str(len(payload)))
                for name, value in route.headers.items():
                    self.send_header(name, value)
                self.send_header("connection", "close")
                self.end_headers()
                try:
                    self.wfile.write(payload)
                except (BrokenPipeError, ConnectionResetError):
                    pass

            do_GET = _serve
            do_POST = _serve
            do_PATCH = _serve
            do_DELETE = _serve

        return Handler


APPS = json.dumps(
    [
        {"id": "a1", "name": "Alpha", "description": "first"},
        {"id": "a2", "name": "Beta", "description": None},
    ]
)
APP = json.dumps(
    {
        "id": "a1",
        "name": "Alpha",
        "description": "first",
        "created_at": "2026-01-01T00:00:00.000Z",
    }
)
CLIENTS = json.dumps(
    [
        {
            "client_id": "c1",
            "name": "CLI One",
            "public": True,
            "app_id": "a1",
            "redirect_uris": ["http://localhost:3000/callback"],
            "scopes": ["openid", "email"],
            "created_at": "2026-01-01T00:00:00.000Z",
        },
        {"client_id": "c2", "name": "Orphan", "public": False, "app_id": ""},
    ]
)
ME = json.dumps(
    {"name": "Yash Gupta", "email": "yash@example.com", "sub": "user-1"}
)


def ok_scenario() -> Scenario:
    return Scenario(
        [
            Route("GET", "/me", 200, ME),
            Route("GET", "/apps", 200, APPS),
            Route("POST", "/apps", 201, json.dumps({"id": "a9", "name": "New App"})),
            Route("GET", "/apps/a1", 200, APP),
            Route("PATCH", "/apps/a1", 200, json.dumps({"id": "a1", "name": "Renamed"})),
            Route("DELETE", "/apps/a1", 204, ""),
            Route("DELETE", "/apps/a2", 204, ""),
            Route("DELETE", "/apps/missing", 404, json.dumps({"message": "gone"})),
            Route("GET", "/clients", 200, CLIENTS),
            Route(
                "POST",
                "/clients",
                201,
                json.dumps(
                    {
                        "client_id": "c9",
                        "name": "New Client",
                        "public": False,
                        "client_secret": "s3cret",
                    }
                ),
            ),
            Route("GET", "/clients/c1", 200, json.dumps({"client_id": "c1", "app_id": "a1"})),
            Route("DELETE", "/clients/c1", 204, ""),
            Route(
                "POST",
                "/clients/c1/rotate-secret",
                200,
                json.dumps({"client_id": "c1", "client_secret": "rotated"}),
            ),
            Route(
                "POST",
                "/token",
                200,
                json.dumps(
                    {
                        "access_token": "at-1",
                        "refresh_token": "rt-1",
                        "token_type": "Bearer",
                        "expires_in": 3600,
                    }
                ),
            ),
        ]
    )


def error_scenario() -> Scenario:
    return Scenario(
        [
            Route("GET", "/me", 401, json.dumps({"message": "expired"})),
            Route("GET", "/apps", 400, json.dumps({"message": "bad access_token=leak-me"})),
            Route("GET", "/clients", 429, "slow down"),
            Route("GET", "/apps/a1", 404, "<!DOCTYPE html><html><body>nope</body></html>"),
        ]
    )


def redirect_scenario() -> Scenario:
    return Scenario(
        [
            Route(
                "GET",
                "/apps",
                302,
                "",
                headers={"location": "http://evil.example/steal"},
            )
        ]
    )


def clients_error_scenario() -> Scenario:
    return Scenario(
        [
            Route("GET", "/apps", 200, APPS),
            Route("GET", "/clients", 429, "slow down"),
        ]
    )


def badjson_scenario() -> Scenario:
    return Scenario([Route("GET", "/apps", 200, "not json at all")])


def slow_scenario() -> Scenario:
    return Scenario([Route("GET", "/apps", 200, APPS, delay=1.2)])


# ── Runner ───────────────────────────────────────────────────────────────────


@dataclass
class Case:
    name: str
    args: list[str]
    scenario: str = "ok"
    tokens: bool = True
    config: str | None = None  # ".aid.json" content template
    files: dict[str, str] = field(default_factory=dict)
    env: dict[str, str] = field(default_factory=dict)
    compare_files: tuple[str, ...] = (".aid.json",)
    # A deliberate, documented improvement: v2's stderr may contain this text
    # where v1 emitted a generic message. Everything else must still match.
    intended_stderr: str = ""
    # v1 issues these reads concurrently, so their arrival order is racy.
    unordered_requests: bool = False
    # Transport-level noise (aborted requests) is not CLI behaviour.
    skip_request_trace: bool = False


@dataclass
class Result:
    name: str
    ok: bool
    detail: str
    v1: dict[str, Any]
    v2: dict[str, Any]


def normalise(text: str, base: str, port: str) -> str:
    text = text.replace("\r\n", "\n")
    text = re.sub(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z", "<TIMESTAMP>", text)
    # Version strings differ by design (v1 0.1.4, v2 2.0.0).
    for version in ("0.1.4", "2.0.0"):
        text = text.replace(version, "<VERSION>")
    text = text.replace(base, "<ISSUER>")
    text = text.replace(port, "<PORT>")
    return text


def seed_tokens(config_home: str) -> None:
    directory = os.path.join(config_home, "aidc")
    os.makedirs(directory, exist_ok=True)
    now_ms = int(time.time() * 1000)
    document = {
        "access_token": "at-seeded",
        "refresh_token": "rt-seeded",
        "token_type": "Bearer",
        "scope": "openid profile email",
        "expires_at": now_ms + 3_600_000,
    }
    with open(os.path.join(directory, "tokens.json"), "w", encoding="utf-8") as handle:
        json.dump(document, handle, indent=2)


def run_one(
    binary_kind: str, case: Case, base: str, port: str, workdir: str, config_home: str
) -> dict[str, Any]:
    # Start from the ambient environment: Node's undici needs it to initialise
    # (a stripped env makes its llhttp WASM instantiation fail in this sandbox).
    env = dict(os.environ)
    env.update({
        "HOME": workdir,
        "XDG_CONFIG_HOME": config_home,
        "NO_COLOR": "1",
        "TERM": "dumb",
        "AIDC_ISSUER": base,
        "BROWSER": "true",
        "AIDC_PORT": port,
    })
    env.update(case.env)
    if binary_kind == "v1":
        command = ["node", V1_ENTRY] + case.args
    else:
        command = [V2_BIN] + case.args
    completed = subprocess.run(
        command,
        cwd=workdir,
        env=env,
        stdin=subprocess.DEVNULL,
        capture_output=True,
        timeout=60,
    )
    return {
        "exit": completed.returncode,
        "stdout": completed.stdout.decode("utf-8", "replace"),
        "stderr": completed.stderr.decode("utf-8", "replace"),
    }


def prepare_project(case: Case, base: str, root: str) -> None:
    if case.config:
        with open(os.path.join(root, ".aid.json"), "w", encoding="utf-8") as handle:
            handle.write(case.config.replace("__ISSUER__", base))
    for name, content in case.files.items():
        path = os.path.join(root, name)
        os.makedirs(os.path.dirname(path), exist_ok=True) if os.path.dirname(path) else None
        with open(path, "w", encoding="utf-8") as handle:
            handle.write(content)


def read_files(root: str, names: tuple[str, ...]) -> dict[str, str]:
    out: dict[str, str] = {}
    for name in names:
        path = os.path.join(root, name)
        if os.path.exists(path):
            with open(path, encoding="utf-8") as handle:
                out[name] = handle.read()
    return out


def compare(case: Case, scenarios: dict[str, Scenario], verbose: bool) -> Result:
    scenario = scenarios[case.scenario]
    base = scenario.base
    port = str(int(base.rsplit(":", 1)[1]))

    outputs: dict[str, dict[str, Any]] = {}
    files: dict[str, dict[str, str]] = {}
    for kind in ("v1", "v2"):
        root = tempfile.mkdtemp(prefix=f"audit-{kind}-")
        workdir = os.path.join(root, "project")
        config_home = os.path.join(root, "config")
        os.makedirs(workdir)
        os.makedirs(config_home)
        prepare_project(case, base, workdir)
        if case.tokens:
            seed_tokens(config_home)
        scenario.reset()
        try:
            result = run_one(kind, case, base, port, workdir, config_home)
            result["requests"] = [
                {"method": r["method"], "path": r["path"], "query": r["query"], "body": r["body"]}
                for r in scenario.requests
            ]
            result["tokens"] = read_files(config_home, ("aidc/tokens.json",))
            result["files"] = read_files(workdir, case.compare_files)
            outputs[kind] = result
            files[kind] = result["files"]
        finally:
            shutil.rmtree(root, ignore_errors=True)

    v1, v2 = outputs["v1"], outputs["v2"]
    problems: list[str] = []
    intended: list[str] = []

    for stream in ("stdout", "stderr"):
        left = normalise(v1[stream], base, port)
        right = normalise(v2[stream], base, port)
        if left == right:
            continue
        if (
            stream == "stderr"
            and case.intended_stderr
            and case.intended_stderr in right
        ):
            intended.append(f"stderr: {case.intended_stderr!r} (documented improvement)")
            continue
        problems.append(f"{stream} differs:\n--- v1 ---\n{left}\n--- v2 ---\n{right}")

    if v1["exit"] != v2["exit"]:
        problems.append(f"exit code differs: v1={v1['exit']} v2={v2['exit']}")

    for name in case.compare_files:
        left = files["v1"].get(name, "<absent>")
        right = files["v2"].get(name, "<absent>")
        if normalise(left, base, port) != normalise(right, base, port):
            problems.append(f"{name} differs:\n--- v1 ---\n{left}\n--- v2 ---\n{right}")

    tokens_left = v1["tokens"].get("aidc/tokens.json", "<absent>")
    tokens_right = v2["tokens"].get("aidc/tokens.json", "<absent>")
    if (tokens_left == "<absent>") != (tokens_right == "<absent>"):
        problems.append(
            f"token file presence differs: v1={'present' if tokens_left != '<absent>' else 'absent'} "
            f"v2={'present' if tokens_right != '<absent>' else 'absent'}"
        )

    # Request traces must match: same endpoints, same bodies.
    left_reqs = [(r["method"], r["path"], r["body"]) for r in v1["requests"]]
    right_reqs = [(r["method"], r["path"], r["body"]) for r in v2["requests"]]
    if case.unordered_requests:
        left_reqs, right_reqs = sorted(left_reqs), sorted(right_reqs)
    if not case.skip_request_trace and left_reqs != right_reqs:
        problems.append(f"request trace differs:\n--- v1 ---\n{left_reqs}\n--- v2 ---\n{right_reqs}")

    if intended and not problems:
        return Result(case.name, True, "; ".join(intended), v1, v2)

    if verbose and problems:
        print(f"\n### {case.name} ({' '.join(case.args) or '<none>'})")
        for problem in problems:
            print(problem)
    return Result(case.name, not problems, "\n".join(problems), v1, v2)


def build_cases() -> list[Case]:
    linked = '{"issuer":"__ISSUER__","app_id":"a1","client_id":"c1","redirect_uri":"http://localhost:3000/callback","app_name":"Old","custom":"keep"}'
    drift = '{"issuer":"__ISSUER__","app_id":"a1","project_type":"react","framework":"React","sdk":"ace-id-sdk"}'
    complete = '{"issuer":"__ISSUER__","app_id":"a1","client_id":"c1","project_type":"vanilla","framework":"Vanilla JavaScript","sdk":"ace-id-sdk"}'
    pkg_next = '{"name":"demo","dependencies":{"next":"14.0.0","ace-id-sdk":"1.0.0"}}'
    pkg_next_nosdk = '{"name":"demo","dependencies":{"next":"14.0.0"}}'

    return [
        # surface
        Case("help", ["--help"]),
        Case("help-flag", ["-h"]),
        Case("help-word", ["help"]),
        Case("no-args", []),
        Case("version", ["--version"]),
        Case("version-short", ["-v"]),
        Case("version-word", ["version"]),
        Case("unknown", ["definitely-not-a-command"]),
        Case("alias-prefix", ["ace-id", "--version"]),
        Case("chain-ok", ["--version", ";", "--version"]),
        Case("chain-fail", ["nope", ";", "--version"]),
        # apps
        Case("apps-bare", ["apps"]),
        Case("apps-help", ["apps", "help"]),
        Case("apps-unknown-sub", ["apps", "nope"]),
        Case("apps-list", ["apps", "list"]),
        Case("apps-list-json", ["apps", "list", "--json"]),
        Case("apps-list-search", ["apps", "list", "--search", "alpha"]),
        Case("apps-list-limit", ["apps", "list", "--limit", "1"]),
        Case("apps-list-limit-bad", ["apps", "list", "--limit", "0"]),
        Case("apps-list-limit-alpha", ["apps", "list", "--limit", "abc"]),
        Case("apps-list-extra", ["apps", "list", "extra"]),
        Case("apps-create", ["apps", "create", "-n", "New App"]),
        Case("apps-create-desc", ["apps", "create", "--name", "New App", "--description", "d"]),
        Case("apps-create-noname", ["apps", "create"]),
        Case("apps-get", ["apps", "get", "a1"]),
        Case("apps-get-json", ["apps", "get", "a1", "--json"]),
        Case("apps-get-noid", ["apps", "get"]),
        Case("apps-update", ["apps", "update", "a1", "-n", "Renamed"]),
        Case("apps-update-nothing", ["apps", "update", "a1"]),
        Case("apps-delete", ["apps", "delete", "a1"]),
        Case("apps-delete-many", ["apps", "delete", "a1", "a2"]),
        Case("apps-delete-missing", ["apps", "delete", "missing"]),
        Case("apps-delete-noid", ["apps", "delete"]),
        # shorthand aliases
        Case("short-list", ["list"]),
        Case("short-create-app", ["create", "app", "-n", "New App"]),
        Case("app-alias", ["app", "list"]),
        # clients
        Case("clients-bare", ["clients"]),
        Case("clients-help", ["clients", "--help"]),
        Case("clients-unknown-sub", ["clients", "nope"]),
        Case("clients-list", ["clients", "list"], config=linked, unordered_requests=True),
        Case(
            "clients-list-json",
            ["clients", "list", "--json"],
            config=linked,
            unordered_requests=True,
        ),
        Case("clients-create", ["clients", "create", "-n", "New Client", "-r", "http://x/cb"]),
        Case(
            "clients-create-full",
            ["clients", "create", "-n", "New Client", "-r", "http://x/cb", "-s", "openid", "-p"],
        ),
        Case("clients-create-noname", ["clients", "create"]),
        Case("clients-create-noredirect", ["clients", "create", "-n", "x"]),
        Case("clients-get", ["clients", "get", "c1"]),
        Case("clients-get-json", ["clients", "get", "c1", "--json"]),
        Case("clients-delete", ["clients", "delete", "c1"]),
        Case("clients-rotate", ["clients", "rotate-secret", "c1"]),
        Case("clients-rotate-noid", ["clients", "rotate-secret"]),
        # account
        Case("whoami", ["whoami"]),
        Case("whoami-json", ["whoami", "--json"]),
        Case("whoami-bad", ["whoami", "--nope"]),
        Case("test-auth", ["test-auth"]),
        Case("test-auth-json", ["test-auth", "--json"]),
        Case("whoami-anon", ["whoami"], tokens=False),
        Case("logout", ["logout"]),
        # link / sync
        Case("link", ["link", "a1"]),
        Case("link-client", ["link", "a1", "--client", "c1"]),
        Case("link-existing", ["link", "a1"], config=linked),
        Case("link-force", ["link", "a1", "--force"], config=linked),
        Case("link-noid", ["link"]),
        Case("sync-dry", ["sync", "--dry-run"], config=linked, unordered_requests=True),
        Case("sync-apply", ["sync"], config=linked, unordered_requests=True),
        Case("sync-noop", ["sync"], config=complete, unordered_requests=True),
        Case("sync-noconfig", ["sync"]),
        Case("sync-badargs", ["sync", "a1", "--dry-run"]),
        # setup
        Case("setup-vanilla", ["setup"], compare_files=(".aid.json", ".aid-setup-recovery.json")),
        Case(
            "setup-next",
            ["setup"],
            files={"package.json": pkg_next},
            compare_files=(".aid.json",),
        ),
        Case(
            "setup-existing",
            ["setup"],
            config=complete,
            compare_files=(".aid.json",),
        ),
        Case(
            "setup-drift",
            ["setup"],
            config=drift,
            compare_files=(".aid.json",),
        ),
        Case("setup-recover-none", ["setup", "--recover"]),
        Case(
            "setup-flags",
            ["setup", "--name", "Flagged", "--redirect", "http://localhost:4000/cb", "--scope", "openid", "--scope", "email"],
            compare_files=(".aid.json",),
        ),
        Case("setup-badflag", ["setup", "--nope"]),
        # error scenarios
        Case("err-apps-400", ["apps", "list"], scenario="error"),
        Case("err-me-401", ["whoami"], scenario="error"),
        Case(
            "err-clients-429",
            ["clients", "list"],
            scenario="clients_error",
            unordered_requests=True,
            # v1 fires both reads concurrently, so /apps is still in flight when
            # /clients fails; v2 stops after the first failure (one request fewer,
            # identical output).
            skip_request_trace=True,
        ),
        Case("err-apps-html", ["apps", "get", "a1"], scenario="error"),
        Case(
            "err-redirect",
            ["apps", "list"],
            scenario="redirect",
            # v1 falls through to its generic transport message here (its
            # /redirect/i branch is unreachable); v2 names the cause.
            intended_stderr="API request was redirected unexpectedly",
        ),
        Case("err-badjson", ["apps", "list"], scenario="badjson"),
        Case(
            "err-timeout",
            ["apps", "list"],
            scenario="slow",
            env={"AIDC_REQUEST_TIMEOUT_MS": "300"},
            skip_request_trace=True,
        ),
    ]


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--json", default="")
    parser.add_argument("--filter", default="", help="only run cases whose name contains this")
    parser.add_argument("-v", "--verbose", action="store_true")
    args = parser.parse_args()

    if not os.path.exists(V2_BIN):
        print(f"missing v2 binary at {V2_BIN}", file=sys.stderr)
        return 2

    scenarios = {
        "ok": ok_scenario().__enter__(),
        "error": error_scenario().__enter__(),
        "redirect": redirect_scenario().__enter__(),
        "badjson": badjson_scenario().__enter__(),
        "clients_error": clients_error_scenario().__enter__(),
        "slow": slow_scenario().__enter__(),
    }
    cases = build_cases()
    if args.filter:
        cases = [case for case in cases if args.filter in case.name]
    results: list[Result] = []
    try:
        for index, case in enumerate(cases, 1):
            result = compare(case, scenarios, args.verbose)
            results.append(result)
            print(f"[{index}/{len(cases)}] {case.name}: {'ok' if result.ok else 'DIFF'}", flush=True)
    finally:
        for scenario in scenarios.values():
            scenario.__exit__(None, None, None)

    failures = [r for r in results if not r.ok]
    print(f"\n{'=' * 70}")
    print(f"cases: {len(results)}   identical: {len(results) - len(failures)}   divergent: {len(failures)}")
    if failures:
        print("\nDIVERGENT CASES")
        for failure in failures:
            first_line = failure.detail.splitlines()[0] if failure.detail else ""
            print(f"  - {failure.name}: {first_line}")

    if args.json:
        with open(args.json, "w", encoding="utf-8") as handle:
            json.dump(
                [
                    {
                        "name": r.name,
                        "ok": r.ok,
                        "detail": r.detail,
                        "v1_exit": r.v1["exit"],
                        "v2_exit": r.v2["exit"],
                    }
                    for r in results
                ],
                handle,
                indent=2,
            )
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
