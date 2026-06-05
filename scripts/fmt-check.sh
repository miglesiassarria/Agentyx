#!/usr/bin/env bash
# Check formatting without writing changes. Used in CI.
# Run `bun run fmt:check` from the repo root.

set -euo pipefail

cd "$(dirname "$0")/.."

echo "==> rustfmt --check"
cargo fmt --all -- --check

echo "==> prettier --check"
if command -v bun >/dev/null 2>&1; then
  bunx --bun prettier --check .
elif command -v npx >/dev/null 2>&1; then
  npx --yes prettier --check .
else
  echo "warning: neither bun nor npx found; skipping prettier" >&2
fi

echo "OK"
