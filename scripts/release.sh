#!/usr/bin/env bash
# Build the release artifacts for the current platform.
# Run `bun run release` from the repo root.

set -euo pipefail

cd "$(dirname "$0")/.."

VERSION="${1:-}"
if [ -z "$VERSION" ]; then
  echo "usage: $0 <version> (e.g. 0.1.0)" >&2
  exit 1
fi

echo "==> running tests"
scripts/test.sh

echo "==> running lints"
scripts/lint.sh

echo "==> tauri build"
if command -v bun >/dev/null 2>&1; then
  bunx --bun tauri build
elif command -v npx >/dev/null 2>&1; then
  npx --yes tauri build
else
  cargo install tauri-cli --version "^2.0" --locked
  cargo tauri build
fi

echo "Release v$VERSION built. Artifacts under src-tauri/target/release/bundle/."
