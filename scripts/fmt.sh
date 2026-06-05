#!/usr/bin/env bash
# Format all sources (Rust + UI) in the workspace.
# Run `bun run fmt` from the repo root.

set -euo pipefail

cd "$(dirname "$0")/.."

echo "==> rustfmt"
cargo fmt --all

echo "==> prettier"
if command -v bun >/dev/null 2>&1; then
  bunx --bun prettier --write .
elif command -v npx >/dev/null 2>&1; then
  npx --yes prettier --write .
else
  echo "warning: neither bun nor npx found; skipping prettier" >&2
fi

echo "OK"
