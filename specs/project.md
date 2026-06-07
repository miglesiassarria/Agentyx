# Project — Agentyx

**Status**: approved
**Owner**: @miglesias
**Last update**: 2026-06-05

## Visión

Agentyx es una **aplicación de escritorio agentic** — ligera, rápida y local —
que permite a un usuario delegar **tareas complejas sobre sus proyectos** a un
agente IA que opera sobre uno o varios **workspaces aislados**, con un modelo
mental de **sesiones reproducibles**, **journal append-only** para auditoría
y replay, y **multi-agent** como arquitectura base (no como feature futura
añadida encima).

El público objetivo **no** es solo developers: cualquier usuario con proyectos
locales que quiera delegar trabajo a un agente (organizar archivos, transformar
documentos, ejecutar scripts, mantener documentación, etc.). Como caso de uso
particular —y muy relevante— Agentyx también sirve como **herramienta de
desarrollo** al estilo opencode: la separación de workspaces, journal, IPC
tipado y permisos está tomada de opencode como **referencia arquitectónica**,
no como identidad de producto.

Construido sobre **Tauri 2 + Rust** con un frontend **Svelte 5** y un servidor
HTTP embebido (axum) que permite operar la misma UI desde el navegador y
desde otros dispositivos en LAN desde el MVP. **No sube archivos del usuario
a la nube**: todo el trabajo ocurre en local, contra los directorios que el
usuario explícitamente autorizó.

## Goals (v1)

- **Multi-workspace aislado**: cada proyecto del usuario es un workspace con
  su propio `root_path`, configuración, sesiones, journal y permisos. Por
  defecto todo se genera en el root.
- **Directorios extra (R/W)** por workspace: el usuario puede añadir 0..N
  directorios adicionales donde el agente puede leer y escribir. La UI lo
  expone como "Extras" del workspace.
- **Multi-provider**: **Ollama** (local, default, sin coste), **Groq**
  (OpenAI-compatible, rápido y barato) y **Minimax** (Anthropic-compatible,
  bueno para razonamiento). Los tres se configuran con la misma matriz
  `Provider`/`ChatEvent` (ver `domains/providers.md` y ADR-0008).
- **Python opt-in**: el `.venv` del workspace **no es obligatorio**. Un
  workspace sin `.venv` es perfectamente válido (no se crea nada al abrir).
  El venv se crea solo si el usuario lo pide explícitamente o si la tool
  `python_run` se invoca sin venv (en cuyo caso retorna `invalid_input` con
  mensaje claro, no auto-crea).
- **Agente agentic** con tools: lectura/escritura/edición de archivos, búsqueda,
  shell, `python_run` (usa el `.venv` del workspace si existe), listar
  directorios y `apply_patch` (diff unificado).
- **Sistema multi-agente desde el inicio**: la arquitectura de agentes
  (`specs/agents.md`) está modelada para soportar primary + subagents desde
  el día 1, aunque v1 solo incluya 1–2 primary y 1 subagent built-in. Esto
  evita refactors masivos cuando se añadan agentes custom.
- **UI rica**: chat con streaming, diffs visuales (CodeMirror 6), visor PDF
  y DOCX (lazy-load), dashboards con métricas, terminal PTY embebido.
- **Servidor HTTP embebido en el MVP** que expone la misma UI en
  `127.0.0.1` por defecto y, opt-in, en `0.0.0.0` (LAN) con auth
  obligatoria por bearer token. La UI del navegador usa HTTP + SSE,
  no APIs Tauri.
- **Journal append-only** de cada acción del agente para replay y debug.
- **Permisos** por workspace (matriz `allowed_tools` / `denied_paths`) con
  prompt de aprobación para acciones destructivas. Los `extra_paths` se
  rigen por la misma matriz.
- **Binario final** < 20 MB instalado, arranque < 500 ms, RAM < 80 MB en reposo.

## Non-goals (v1)

- ❌ IDE completo tipo Cursor/VSCode (no reemplazamos al editor del usuario).
- ❌ Sincronización de workspaces en la nube.
- ❌ Colaboración multi-usuario en tiempo real sobre el mismo workspace.
- ❌ Marketplace de plugins o custom tools por el usuario.
- ❌ TUI en terminal (la v1 es GUI + servidor web; TUI queda para v2 si aplica).
- ❌ Túnel WAN, relay cloud o exposición pública automática en v1. El MVP
  solo sirve en loopback/LAN; cualquier routing externo queda fuera
  del producto.
- ❌ Telemetría alguna enviada fuera del dispositivo (opt-in, granular, off
  por defecto).
- ❌ Subir el contenido de los workspaces a servidores externos. Todo el
  trabajo del agente ocurre local; el único tráfico de red saliente son las
  llamadas a los providers LLM que el usuario haya configurado.
- ❌ Soporte de providers distintos a Ollama/Groq/Minimax en v1 (OpenAI
  nativo, Anthropic nativo, Bedrock, Vertex, Cohere → v1.x / v2 según
  demanda; ver ADR-0008).
- ❌ Auto-crear `.venv` al abrir un workspace. Es opt-in (ver `python_run`).
- ❌ Auto-updater en v1 (sí distribución manual de binarios firmados, no
  push de updates automáticos; updater queda como feature explícita para v1.x).

## Diferenciadores vs. alternativas

| vs. | Diferencia clave |
|---|---|
| **vs. opencode (referencia arquitectónica)** | Mismo modelo mental (workspaces aislados, sesiones, journal, multi-agent), pero en **Rust + Tauri** en vez de Electron + TypeScript. Binario ~10× más pequeño, arranque ~3× más rápido. Mejor sandbox de archivos (`root + extras` explícito). |
| **vs. Codex App** | Multi-provider real (Codex está atado a OpenAI). Servidor web embebido para acceso desde navegador y otros dispositivos. Stack open source y modificable. |
| **vs. agentes cloud (Manus, Devin, …)** | Corre **100% local** sobre los directorios del usuario. Los archivos nunca salen de la máquina. Compatible con workflows offline si el provider es local (Ollama). |
| **vs. Cursor / Copilot** | No es un IDE, es un **command center** para el agente. El usuario usa su editor favorito. El agente opera sobre el árbol de archivos del proyecto como un par más, no como un pariente del editor. |

## Métricas de éxito (post v1)

- Binario < 20 MB instalado.
- Arranque frío < 500 ms en MacBook M1.
- RAM en reposo < 80 MB.
- 0 `unwrap()` / `expect()` en producción.
- 0 secrets logueados.
- Cobertura > 70 % en `agentyx-core`, > 50 % en el resto.
- Smoke test con Ollama local funciona en CI.
- El usuario puede abrir un workspace, añadir 1 directorio extra y trabajar
  sobre ambos en una sola sesión sin configuración adicional.
- El usuario puede cambiar entre los 3 agents default (1 primary activo, 1
  plan opcional) sin reiniciar la app.
- El usuario puede abrir la misma UI desde un navegador en la LAN, autenticarse
  con bearer token cuando el bind es `0.0.0.0`, y ejecutar el flujo básico de
  workspace + chat + streaming.

## Vías de evolución (post v1)

- **v1.x**: auto-updater firmado, notarización, métricas locales,
  F03 (Python venv opt-in), editor de agentes custom en UI.
- **v2**: TUI mínima (ratatui) compartiendo `agentyx-core`, MCP server (la app
  como tool para otros agentes), sync básica read-only entre devices, más
  providers (OpenAI nativo, Anthropic nativo, Bedrock, Vertex).

## Referencias

- [opencode-dev](../opencode-dev/) — referencia arquitectónica local (no
  fork, no se redistribuye, no se commitea al repo).
- [AGENTS.md](../AGENTS.md) — convenciones y reglas del proyecto.
- [architecture.md](./architecture.md) — diagrama de cajas y flujo de datos.
- [glossary.md](./glossary.md) — vocabulario del proyecto.
- [adr/0007-extra-paths-per-workspace.md](./adr/0007-extra-paths-per-workspace.md)
  — modelo `root + extra_paths`.
- [adr/0008-providers-v1-scope.md](./adr/0008-providers-v1-scope.md) —
  justificación de los 3 providers de v1.
