# Project — Agentyx

**Status**: approved
**Owner**: @miglesias
**Last update**: 2026-06-04

## Visión

Agentyx es una aplicación de escritorio **ligera, rápida y agentic** que permite
a un developer delegar trabajo de programación a un agente IA que opera sobre
uno o varios proyectos locales, con un modelo mental de **workspaces aislados**,
**sesiones reproducibles** y **journal de acciones** para auditoría y replay.

Inspirado en opencode (referencia arquitectónica) y Codex (UX y modelo de
"command center" para agentes), pero construido sobre **Tauri 2 + Rust** y un
**frontend Svelte 5** con un servidor HTTP embebido (axum) que permite operar
la misma UI desde el navegador y desde otros dispositivos en LAN.

## Goals (v1)

- **Multi-workspace** con `.venv` por workspace cuando aplique.
- **Multi-provider**: OpenAI, Anthropic, Ollama local, y cualquier endpoint
  OpenAI-compatible (Together, Groq, OpenRouter, …).
- **Agente agentic** con tools: lectura/escritura/edición de archivos, búsqueda,
  shell, `python_run` (usa el `.venv` del workspace), listar directorios, y
  `apply_patch` (diff unificado).
- **UI rica**: chat con streaming, diffs visuales (CodeMirror 6), visor PDF
  y DOCX (lazy-load), dashboards con métricas, terminal PTY embebido.
- **Servidor HTTP embebido** que expone la misma UI en `127.0.0.1` por defecto
  y, opt-in, en `0.0.0.0` (LAN) con auth por bearer token.
- **Journal append-only** de cada acción del agente para replay y debug.
- **Permisos** por workspace (matriz `allowed_tools` / `denied_paths`) con
  prompt de aprobación para acciones destructivas.
- **Binario final** < 20 MB instalado, arranque < 500 ms, RAM < 80 MB en reposo.

## Non-goals (v1)

- ❌ IDE completo tipo Cursor/VSCode (no reemplazamos al editor del usuario).
- ❌ Sincronización de workspaces en la nube.
- ❌ Colaboración multi-usuario en tiempo real sobre el mismo workspace.
- ❌ Marketplace de plugins o custom tools por el usuario.
- ❌ TUI en terminal (la v1 es GUI + servidor web; TUI queda para v2 si aplica).
- ❌ Túnel WAN (cloudflared/ngrok) para acceso externo a la LAN.
- ❌ Telemetría alguna enviada fuera del dispositivo (opt-in, granular, off
  por defecto).
- ❌ Soporte de providers que no sean OpenAI-compatibles o Anthropic
  (Bedrock, Vertex, etc. → v2 si hay demanda).
- ❌ Auto-updater en v1 (sí distribución manual de binarios firmados, no
  push de updates automáticos; updater queda como feature explícita para v1.x).

## Diferenciadores vs. alternativas

| vs. | Diferencia clave |
|---|---|
| **vs. opencode (referencia)** | Mismo modelo mental (workspaces, sesiones, journal) pero en **Rust + Tauri** en vez de Electron + TypeScript. Binario ~10× más pequeño, arranque ~3× más rápido. |
| **vs. Codex App** | Multi-provider (Codex está atado a OpenAI). Servidor web embebido para acceso desde navegador y otros dispositivos. Stack open source y modificable. |
| **vs. Cursor** | No es un IDE, es un command center para el agente. El developer usa su editor favorito. |

## Métricas de éxito (post v1)

- Binario < 20 MB instalado.
- Arranque frío < 500 ms en MacBook M1.
- RAM en reposo < 80 MB.
- 0 `unwrap()` / `expect()` en producción.
- 0 secrets logueados.
- Cobertura > 70 % en `agentyx-core`, > 50 % en el resto.
- Smoke test con Ollama local funciona en CI.

## Vías de evolución (post v1)

- **v1.x**: auto-updater firmado, notarización, métricas locales, tunnel WAN
  opt-in.
- **v2**: TUI mínima (ratatui) compartiendo `agentyx-core`, MCP server (la app
  como tool para otros agentes), sync básica read-only entre devices.

## Referencias

- [opencode-dev](../opencode-dev/) — referencia arquitectónica local (no
  fork, no se redistribuye, no se commitea al repo).
- [AGENTS.md](../AGENTS.md) — convenciones y reglas del proyecto.
- [architecture.md](./architecture.md) — diagrama de cajas y flujo de datos.
- [glossary.md](./glossary.md) — vocabulario del proyecto.
