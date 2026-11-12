#!/usr/bin/env bash
# Fresh-install check: clone the branch, build from source, install the binary on
# PATH, run it from an empty HOME, then uninstall and confirm it is gone.
#
# Usage: bash audit/fresh_install.sh [--reuse-target DIR]
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/aid-fresh-XXXXXX")"
REUSE_TARGET=""
if [[ "${1:-}" == "--reuse-target" ]]; then
  REUSE_TARGET="${2:?--reuse-target needs a directory}"
fi

cleanup() { rm -rf "$WORK"; }
trap cleanup EXIT

echo "== 1. clone the branch into a clean tree"
git clone --quiet --depth 1 "file://${REPO_ROOT}" "$WORK/src"
cd "$WORK/src"
git log --oneline -1

echo
echo "== 2. build from source"
cd rust
if [[ -n "$REUSE_TARGET" ]]; then
  CARGO_TARGET_DIR="$REUSE_TARGET" cargo build --release --quiet
  BIN="$REUSE_TARGET/release/aid"
else
  cargo build --release --quiet
  BIN="target/release/aid"
fi
echo "built: $(ls -l "$BIN" | awk '{print $5}') bytes"

echo
echo "== 3. install onto PATH (no ~/.local/bin yet)"
export HOME="$WORK/home"
mkdir -p "$HOME"
test ! -d "$HOME/.local/bin" && echo "confirmed: ~/.local/bin does not exist"
mkdir -p "$HOME/.local/bin"
install -m 0755 "$BIN" "$HOME/.local/bin/aid"
export PATH="$HOME/.local/bin:$PATH"
command -v aid

echo
echo "== 4. run from a clean HOME"
aid --version
aid --help | head -4
aid logout || true
aid whoami 2>&1 | tail -2 || true
aid definitely-not-a-command 2>&1 | tail -2 || true

echo
echo "== 5. uninstall"
rm -f "$HOME/.local/bin/aid"
hash -r 2>/dev/null || true
if command -v aid > /dev/null; then
  echo "FAIL: aid still on PATH"
  exit 1
fi
echo "confirmed: aid removed from PATH"

echo
echo "fresh install/uninstall checks passed"
