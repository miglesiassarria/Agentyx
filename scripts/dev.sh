#!/usr/bin/env bash
# Start the dev environment: Vite UI in foreground, Tauri shell spawning
# the Rust binary. Run `bun run dev` from the repo root.

set -euo pipefail

cd "$(dirname "$0")/.."

if command -v bun >/dev/null 2>&1; then
  bunx --bun tauri dev
elif command -v npx >/dev/null 2>&1; then
  npx --yes tauri dev
else
  cargo install tauri-cli --version "^2.0" --locked
  cargo tauri dev
fi
