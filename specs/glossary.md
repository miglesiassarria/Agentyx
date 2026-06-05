# Glossary

**Status**: approved
**Owner**: @miglesias
**Last update**: 2026-06-05

> Vocabulario canónico del proyecto. Si un término no está aquí, definirlo
> antes de usarlo en una spec o en código.

---

## A

### Agent
Una **configuración de agente** ejecutable por el agent loop. Se modela como
`AgentSpec { id, mode, model, prompt, tools, permissions, description? }`
donde `mode` es `Primary | Subagent | Hidden`. v1 incluye 2 primary
(`build`, `plan`) y 1 subagent (`general`) built-in. La arquitectura
está preparada para añadir agentes custom definidos por el usuario
(ver [agents.md](./agents.md) y ADR por escribir).

### Agent loop
Bucle ReAct (Reason + Act) que ejecuta el agente. Recibe el input del
usuario, llama al provider LLM, detecta tool calls, ejecuta tools, registra
en el journal, y repite hasta `finish_reason == "stop"` o `max_steps`.
Ver [domains/agent-loop.md](./domains/agent-loop.md).

### Agent mode
Categoría del agente que define cómo se invoca:
- `primary` — el que el usuario controla directamente (cycle con Tab,
  el "active agent" de la sesión).
- `subagent` — invocado por un primary (vía tool `task`) o manualmente
  por el usuario con `@<agent-id>` en un mensaje.
- `hidden` — agente de sistema, no seleccionable en UI (p. ej. compaction,
  title generation). Se invoca automáticamente.

### Approval mode
Modo de aprobación para acciones del agente:
- `always` — ejecuta sin prompt.
- `ask` — prompt al usuario antes de acciones marcadas como "destructivas".
- `deny` — bloquea la acción y devuelve error al agente.

### Extra path
Directorio **fuera del `root_path`** del workspace que el usuario ha
autorizado explícitamente para que el agente lea y escriba. Cada workspace
puede tener 0..N `extra_paths`. Por defecto, el agente trabaja en el root;
un tool call puede tomar `path` apuntando a un extra path si está declarado
en la config del workspace. Ver [workspace.md](./domains/workspace.md) y
[ADR-0007](./adr/0007-extra-paths-per-workspace.md).

---

## C

### ChatEvent
Enum canónico que normaliza el streaming de cualquier provider LLM a una
sola forma (`MessageStart`, `ContentDelta`, `ToolUse`, `ToolResult`,
`MessageEnd`, `Error`). El frontend solo conoce `ChatEvent`, nunca shapes
específicos de provider. Ver [domains/providers.md](./domains/providers.md).

### Core (`agentyx-core`)
Librería Rust pura (`crates/agentyx-core/`) que contiene todo el dominio:
agent loop, providers LLM, tools, storage, workspaces, journal, permissions,
config, PTY. **Nunca depende de Tauri**. Es testeable sin GUI.

---

## D

### Diff
Cambio entre dos versiones de un archivo (unified diff). En la UI se
renderiza con CodeMirror 6 + `@codemirror/merge`. En el modelo, la tool
`apply_patch(diff)` aplica un diff unificado (formato similar a opencode).

### Discovered bugs
Sección al final de cada spec donde se listan los bugs post-aprobación
resueltos, con id, fecha, categoría y versión. Mantiene la spec sincronizada
con la realidad. Ver `AGENTS.md` §Gestión de bugs.

---

## E

### Event (streaming)
Mensaje unidireccional que el core emite al UI (vía Tauri `emit` o vía SSE
en HTTP). Tiene schema versionado (`chat.message.v1`, `pty.output.v1`, …).
Ver [ipc.md](./ipc.md).

---

## F

### Feature spec
Spec vertical que describe una funcionalidad de cara al usuario (UX, flow,
acceptance criteria). Vive en [features/](./features/) con naming
`F<NN>-slug.md`. Siempre referencia qué dominios toca (`Affects:`).

### Finish reason
Razón por la que el provider detuvo la generación:
`stop` (modelo terminó), `tool_use` (modelo pide ejecutar tool),
`length` (cortado por límite), `content_filter` (rechazado),
`error` (fallo del provider).

---

## H

### Hotfix
PR marcado como `hotfix` que puede saltarse el ciclo de aprobación de
specs en `draft` cuando el bug es `blocker`. La spec se actualiza en
≤ 24 h después. Ver `AGENTS.md` §Gestión de bugs.

---

## I

### IPC
Inter-Process Communication. En Agentyx hay dos canales:
- **Tauri commands** (`#[tauri::command]`) — request/response síncrono.
- **Eventos Tauri** (`window.emit`) + **SSE HTTP** — streaming unidireccional.

Contratos definidos en [ipc.md](./ipc.md).

---

## J

### Journal
Log append-only de todas las acciones del agente. Cada entrada:
`id, session_id, ts, tool, args, result, duration_ms, permission_decision`.
Permite replay y debug post-mortem. Se persiste en SQLite (`journal` table)
o archivo rotado (`journal.jsonl`).

---

## L

### LLM Provider
Servicio externo de inferencia. Implementa el trait `Provider`
(`agentyx-core/src/llm/provider.rs`) y normaliza su salida a `ChatEvent`.
Soporte v1: **Ollama** (local, NDJSON), **Groq** (OpenAI-compatible) y
**Minimax** (Anthropic-compatible). Otros providers (OpenAI nativo,
Anthropic nativo, Bedrock, Vertex, Cohere) se difieren a v1.x / v2 según
demanda. Ver [providers.md](./domains/providers.md) y
[ADR-0008](./adr/0008-providers-v1-scope.md).

---

## M

### Max steps
Límite de iteraciones del agent loop. Por defecto 50. Cuando se alcanza,
el loop aborta con error y la sesión queda en estado `aborted`.

---

## P

### Permission decision
Decisión registrada cuando una tool requiere permiso:
`allow` (ejecutar), `ask` (prompt al usuario), `deny` (rechazar).
Se guarda en el journal junto al resultado de la tool.

### Provider
Ver [LLM Provider](#llm-provider).

### PTY
Pseudo-terminal. En Agentyx lo aporta la crate `portable-pty` (ConPTY en
Windows, openpty en macOS/Linux). Permite REPLs interactivos (`python -i`),
comandos con color, y shells que requieren TTY.

---

## R

### ReAct
Patrón **Re**ason + **Act** que sigue el agent loop: el LLM razona, propone
tool calls, ejecuta tools, observa resultados, y vuelve a razonar.

### Roadmap
Vista de features con dependencias y fases. Ver
[features/ROADMAP.md](./features/ROADMAP.md).

---

## S

### Sandbox (workspace)
Aislamiento lógico por workspace: su `.venv`, su config, su historial,
sus permisos. Path traversal bloqueado: toda I/O se canonicaliza contra
el `root` del workspace. v1: aislamiento lógico. v2: sandboxing nativo
del SO (Seatbelt macOS, Landlock Linux, AppContainer Windows).

### Session
Unidad de trabajo: un chat + un conjunto de tool calls + un journal,
asociados a un workspace. Persiste en `state.db` (SQLite del workspace).
Ciclo de vida: `idle` → `running` → `idle` (o `aborted` / `errored`).

### Spec
Documento versionado en `specs/` que describe diseño, contratos,
acceptance criteria. Es la fuente de verdad antes del código.

### SSE
Server-Sent Events. Mecanismo de streaming HTTP unidireccional usado por
el servidor embebido (axum) para empujar `ChatEvent` y `pty.output` al
navegador. Ver [ipc.md](./ipc.md).

### Status (de una spec)
Estado del ciclo de vida: `draft` → `review` → `approved` →
`implemented` → `deprecated`.

---

## T

### Tool
Capacidad invocable por el agente. Contrato en
[domains/tools.md](./domains/tools.md). Tools v1:
`read_file`, `write_file`, `edit_file`, `search`, `shell`,
`python_run`, `list_dir`, `apply_patch`.

### TUI
Terminal User Interface. **No en v1**. Si se añade en v2, sería con
`ratatui` compartiendo `agentyx-core`.

---

## U

### ULID
Identificador universal ordenable lexicográficamente por tiempo. Usado
para todos los IDs (`workspace_id`, `session_id`, `message_id`, etc.).
Crate: `ulid`.

### User prompt
Mensaje del usuario al agente dentro de una sesión. Se persiste como
un `Message` con `role: "user"`.

---

## V

### Venv
Entorno virtual de Python. Detectado por workspace según orden definido
en `domains/workspace.md` y ADR-0004. **Opt-in en v1**: un workspace
**no requiere** un `.venv`. La detección es pasiva (si existe `.venv/`
o `venv/`, se reporta en la UI del workspace como info, pero no es
bloqueante). Se crea solo si el usuario lo pide explícitamente
(`workspace_create_venv`) o si la tool `python_run` se invoca sin venv,
en cuyo caso retorna `invalid_input` con un mensaje claro y la acción
correctiva; **nunca** se auto-crea. Activación: ejecutar el binario
directamente (`.venv/bin/python` o `.venv\Scripts\python.exe`),
no `source activate`.

### VenvSpec
Estructura in-memory que describe un venv detectado: `{ kind: Uv|Venv,
path: PathBuf, python: PathBuf, version: String }`.

---

## W

### Workspace
Carpeta elegida por el usuario que el agente trata como unidad de
aislamiento. Un workspace tiene un `root_path` (path principal donde
el agente trabaja por defecto) y opcionalmente 0..N `extra_paths` con
acceso R/W explícito. Estado por workspace en
`~/.agentyx/workspaces/<id>/` con `config.toml`, `state.db`,
`journal.jsonl`. Ver [workspace.md](./domains/workspace.md).
