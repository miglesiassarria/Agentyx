#!/usr/bin/env bash
# Run linters across the workspace (clippy + eslint + tsc).
# Run `bun run lint` from the repo root.

set -euo pipefail

cd "$(dirname "$0")/.."

echo "==> cargo clippy"
cargo clippy --workspace --all-targets --all-features -- -D warnings

echo "==> cargo deny check"
cargo deny check

echo "==> tsc --noEmit"
cd ui
if command -v bun >/dev/null 2>&1; then
  bunx --bun tsc --noEmit
elif command -v npx >/dev/null 2>&1; then
  npx --yes tsc --noEmit
fi

echo "==> eslint ."
if command -v bun >/dev/null 2>&1; then
  bunx --bun eslint .
elif command -v npx >/dev/null 2>&1; then
  npx --yes eslint .
fi

echo "OK"
