#!/usr/bin/env bash
# Run the full test suite (Rust + UI).
# Run `bun run test` from the repo root.

set -euo pipefail

cd "$(dirname "$0")/.."

echo "==> cargo test --workspace"
cargo test --workspace

echo "==> vitest run"
cd ui
if command -v bun >/dev/null 2>&1; then
  bunx --bun vitest run
elif command -v npx >/dev/null 2>&1; then
  npx --yes vitest run
fi

echo "OK"
