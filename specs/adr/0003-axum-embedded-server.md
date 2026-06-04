# ADR-0003 — Servidor HTTP embebido (axum) en el mismo proceso Tauri

**Status**: accepted
**Date**: 2026-06-04
**Deciders**: @miglesias

## Context

Necesitamos que la app sea accesible desde:

1. El **webview nativo** de Tauri (modo principal).
2. Un **navegador** en el mismo dispositivo (modo dev, tests e2e,
   depuración con DevTools).
3. **Otros dispositivos en la LAN** (iPad, otro laptop, móvil en la
   misma WiFi) — uso ocasional y opt-in.

Los tres casos exigen exponer la misma UI web y la misma API de negocio
a través de HTTP. Las opciones:

- **A. Servidor HTTP embebido en el mismo proceso Tauri** (axum).
- **B. Sidecar separado** (opencode-style: el server es un binario
  aparte que Tauri lanza vía `utilityProcess`).
- **C. Solo Tauri webview nativo**, sin HTTP.
- **D. Generar binarios "headless" distintos** (server vs desktop).

## Decision

**A. Servidor HTTP embebido (axum) en el mismo proceso Tauri.**

El binario de Agentyx es **un solo proceso Rust** que:
- Aloja el runtime de Tauri (webview nativo + IPC).
- Aloja un servidor axum en un puerto local (por defecto `127.0.0.1:0`,
  aleatorio).
- Sirve el `ui/dist/` estático (embebido vía `rust-embed`).
- Expone la misma API que los Tauri commands, vía REST + SSE.
- Acepta bind a `0.0.0.0` **opt-in** desde Settings, exigiendo bearer
  token.

## Status

`accepted`.

## Consequences

### Positivas
- **Un solo binario, un solo proceso** → SQLite compartido, journal
  compartido, estado consistente sin IPC extra.
- **Tests e2e en navegador triviales**: Playwright apunta a
  `http://127.0.0.1:<puerto>`, sin WebDriver de Tauri.
- **Multi-device en LAN** con cero infra extra.
- **Misma build de Svelte** sirve webview nativo y navegador: un solo
  `lib/ipc.ts` decide el transporte.
- **Camino natural hacia MCP server** (la app expone tools a otros
  agentes) en v2.
- **Misma fuente de verdad para la API** que los Tauri commands: los
  handlers HTTP son thin wrappers sobre las mismas funciones de
  `agentyx-core`.

### Negativas
- **+1 crate** (`axum` ~500 KB-1 MB, `tower`, `tower-http`,
  `rust-embed`).
- **CORS y CSP a configurar bien**: la UI servida por el server debe
  tener `Content-Security-Policy` estricta.
- **Superficie de red aumenta** ⇒ más auditoría de seguridad. Pero
  esto es exactamente lo que queremos: un HTTP local con auth clara es
  **más auditable** que un canal IPC opaco.
- **Port random en loopback** dificulta saber el puerto desde fuera.
  Solución: el command Tauri `server_get_url` lo expone a la UI; el
  `~/.agentyx/server.url` lo persiste entre arranques.

### Neutras
- **No hay túnel WAN en v1**. El acceso externo (fuera de LAN) queda
  como feature de v1.x con cloudflared opt-in.
- El server arranca con la app; no es opcional apagarlo en v1 (es
  siempre local). Apagarlo es una feature menor que se puede añadir
  si hay queja.

## Alternatives considered

### Alternative B: Sidecar separado (estilo opencode)
- Pros: el server puede correr en una máquina sin GUI (CI, servidor).
- Cons: dos binarios, dos procesos, IPC entre ellos, dos fuentes de
  verdad para el puerto y la config. Para nuestro caso (app local de
  un developer) la complejidad no compensa.
- **Por qué se descartó**: el requisito de v1 es desktop local; el
  caso "server headless" lo cubrimos con `cargo run --bin agentyx-app`
  + variable `AGENTYX_NO_TAURI=1` cuando lo necesitemos (futuro).

### Alternative C: Solo Tauri webview nativo, sin HTTP
- Pros: menos superficie, menos código.
- Cons: perdemos tests e2e fáciles, perdemos acceso desde navegador,
  perdemos multi-device.
- **Por qué se descartó**: el usuario explícitamente pidió esta
  capacidad en la sesión de planning.

### Alternative D: Binarios headless distintos
- Pros: separación clara de responsabilidades.
- Cons: doble trabajo de packaging, doble superficie de config, doble
  espacio en disco para el usuario.
- **Por qué se descartó**: no aporta valor en v1; un solo binario es
  más simple y mantiene el principio de ligereza.

## Implementation notes (no decisiones)

- Bind por defecto: `127.0.0.1:0` (puerto aleatorio). Persistir puerto
  en `~/.agentyx/state.json` solo si el usuario lo expone a LAN.
- Token bearer: generado al primer arranque con `rand` (32 bytes
  base64), guardado en keychain (`keyring` crate).
- CORS: allowlist cerrado: solo el propio origen (`http://127.0.0.1:<puerto>`)
  y `null` (file://). Nunca `*`.
- Static: `rust-embed` con `include_dir!`-style del `ui/dist/`
  resultante del build de Svelte.
- Health endpoint: `GET /api/v1/health` → `200 { ok: true, version }`.
- Heartbeat SSE cada 15 s (`event: ping`).

## References

- [architecture.md](../architecture.md) — diagrama de procesos.
- [ipc.md](../ipc.md) — contratos HTTP, REST y SSE.
- Web: <https://docs.rs/axum> · <https://docs.rs/rust-embed>.
