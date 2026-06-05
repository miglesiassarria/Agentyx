#!/usr/bin/env bash
# Clean build artifacts (target/, node_modules/, dist/, .svelte-kit/).

set -euo pipefail

cd "$(dirname "$0")/.."

echo "==> cargo clean"
cargo clean

echo "==> rm -rf ui/node_modules"
rm -rf ui/node_modules

echo "==> rm -rf ui/dist ui/.svelte-kit"
rm -rf ui/dist ui/.svelte-kit

echo "OK"
