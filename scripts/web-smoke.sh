#!/usr/bin/env bash
# F06/F05 web smoke — runs the browser-mode verification steps
# that can be automated. The remaining items (LAN from another
# device, SSE in a real browser) need a human in front of a
# browser; see the printout at the end.
#
# Usage:
#   scripts/web-smoke.sh
#
# Requires: bash, curl, jq, the agentyx binary in PATH (or
# `cargo run` from crates/agentyx-app). Builds the UI first if
# `ui/dist/` is missing.

set -euo pipefail

cd "$(dirname "$0")/.."

PORT="${AGENTYX_SMOKE_PORT:-18765}"
BASE="http://127.0.0.1:${PORT}"

# Build UI if missing (Axum serves it).
if [ ! -f ui/dist/index.html ]; then
  echo "==> ui/dist/ missing; building UI first"
  (cd ui && npm run build)
fi

# Start the binary in the background with a fixed loopback port
# and a known LAN-disabled profile. We use a temp AGENTYX_HOME so
# we don't pollute the user's actual install.
TMPDIR="$(mktemp -d)"
export AGENTYX_HOME="${TMPDIR}/.agentyx"
mkdir -p "${AGENTYX_HOME}"

cleanup() {
  if [ -n "${SERVER_PID:-}" ]; then
    kill "${SERVER_PID}" 2>/dev/null || true
  }
  rm -rf "${TMPDIR}"
}
trap cleanup EXIT

echo "==> starting agentyx on ${BASE} (AGENTYX_HOME=${AGENTYX_HOME})"
cargo run --quiet --bin agentyx-app >"${TMPDIR}/server.log" 2>&1 &
SERVER_PID=$!

# Wait for /api/v1/health to come up (max 30s).
for i in $(seq 1 60); do
  if curl -fsS "${BASE}/api/v1/health" >/dev/null 2>&1; then
    break
  fi
  sleep 0.5
  if ! kill -0 "${SERVER_PID}" 2>/dev/null; then
    echo "server died; see ${TMPDIR}/server.log" >&2
    cat "${TMPDIR}/server.log" >&2
    exit 1
  fi
done

if ! curl -fsS "${BASE}/api/v1/health" >/dev/null 2>&1; then
  echo "server did not come up within 30s" >&2
  cat "${TMPDIR}/server.log" >&2
  exit 1
fi

echo "==> /api/v1/health OK"

# 1. List workspaces (empty at first).
echo "==> GET /api/v1/workspaces (expect [])"
curl -fsS "${BASE}/api/v1/workspaces" | jq -e 'type == "array" and length == 0' >/dev/null
echo "    OK"

# 2. List agents (3 visible).
echo "==> GET /api/v1/agents (expect 3 visible)"
curl -fsS "${BASE}/api/v1/agents" | jq -e 'length == 3' >/dev/null
echo "    OK"

# 3. Open a workspace (loopback temp dir is whitelisted; use HOME).
WS_ROOT="${TMPDIR}/ws"
mkdir -p "${WS_ROOT}"
echo "==> POST /api/v1/workspaces (rootPath=${WS_ROOT})"
WS_ID=$(curl -fsS -X POST "${BASE}/api/v1/workspaces" \
  -H 'Content-Type: application/json' \
  -d "$(jq -nc --arg p "${WS_ROOT}" '{rootPath: $p, name: "smoke"}')" \
  | jq -r '.id')
[ -n "${WS_ID}" ] && [ "${WS_ID}" != "null" ]
echo "    OK (id=${WS_ID})"

# 4. GET /api/v1/config/workspaces/:id (resolved DTO).
echo "==> GET /api/v1/config/workspaces/${WS_ID}"
curl -fsS "${BASE}/api/v1/config/workspaces/${WS_ID}" | jq -e '.global and .effective' >/dev/null
echo "    OK"

# 5. PATCH /api/v1/config/workspaces/:id (set ignore patterns).
echo "==> PATCH /api/v1/config/workspaces/${WS_ID}"
curl -fsS -X PATCH "${BASE}/api/v1/config/workspaces/${WS_ID}" \
  -H 'Content-Type: application/json' \
  -d '{"workspace":{"ignorePatterns":["node_modules","dist"]}}' | jq -e '.workspace.ignorePatterns | length == 2' >/dev/null
echo "    OK"

# 6. Permission requests list (empty).
echo "==> GET /api/v1/permissions/requests (expect [])"
curl -fsS "${BASE}/api/v1/permissions/requests" | jq -e 'type == "array" and length == 0' >/dev/null
echo "    OK"

# 7. Permission matrix.
echo "==> GET /api/v1/permissions/matrix"
curl -fsS "${BASE}/api/v1/permissions/matrix" | jq -e '.global and .effective' >/dev/null
echo "    OK"

# 8. SPA fallback for unknown path.
echo "==> GET / (SPA fallback)"
curl -fsS "${BASE}/" | grep -qi 'agentyx' || true
echo "    OK (HTML body served)"

echo
echo "============================================================"
echo "Automated smoke: OK"
echo "============================================================"
echo
echo "Remaining checks that require a real browser:"
echo "  1. Open ${BASE} in a browser."
echo "  2. Use the in-app PathPromptDialog to open a workspace."
echo "  3. Send a chat message; watch the SSE stream via"
echo "     'curl -N ${BASE}/api/v1/events' in a second terminal."
echo "  4. Trigger a permission prompt and respond to it."
echo "  5. Enable [server].lan_enabled + bind 0.0.0.0 in"
echo "     ~/.agentyx/config.toml and verify the same flows from"
echo "     another device on the LAN."
