# Features — Roadmap

> Vista por features. Para vista global: [specs/README.md](../README.md).
> Para índice de ADRs: [specs/adr/README.md](../adr/README.md).
> Última actualización: 2026-06-06

## Agent context

- Para trabajo MVP, leer primero esta tabla y luego el bloque
  `## Agent context` de la feature/dominio afectado.
- Specs compactadas con contexto rápido: F01, F02, F04, F05,
  F-agents-ui, `agents.md`, `domains/config.md`, `domains/journal.md`.
- No usar este roadmap como contrato de IPC o ACs; ir al pitch/spec
  concreto para contratos y tests.

## Leyenda

- **Status**: preferido `proposed` | `ready` | `shipped`; histórico
  `draft` | `approved` | `in-progress` | `implemented`.
- **Phase**: orden aproximado de implementación dentro de la versión (no estricto; depende de la spec).
- **Depends on**: features cuyo spec debe estar al menos `approved` antes de empezar esta.
- **Affects**: specs de [dominio](../domains/) que la feature consume.

---

## v0.1 — Foundation (MVP)

> La app debe ser utilizable: abrir un workspace (con o sin Python,
> con 0..N directorios extra), configurar un provider, chatear con
> el agente (con multi-agent desde el día 1: build + plan + general),
> ejecutar tools básicas, stream LLM en la UI, todo persistido.

| ID | Feature | Status | Affects | Depends on | Phase |
|---|---|---|---|---|---|
| [F02](F02-multi-workspace.md) | Multi-workspace: list, open, delete, **extra paths**, badge venv pasivo | review | workspace, tools, permissions | — | 1 |
| [F05](F05-settings.md) | Settings: providers activos (Ollama/Groq/Minimax), modelos, keychain entry, approval_mode | draft (2026-06-05) | providers, permissions, **config** | F02 | 2 |
| [F01](F01-chat-streaming.md) | Chat con streaming LLM (provider agnóstico, multi-agent: build/plan) | draft (2026-06-05) | agent-loop, providers, session, **agents**, **journal** | F02, F05 | 3 |
| [F04](F04-file-diffs.md) | File diffs en UI (CodeMirror merge) tras edit_file / apply_patch — **read-only en v0.1** | draft (2026-06-05) | tools, ui | F01, F02 | 4 |
| [F-agents-ui](F-agents-ui.md) | UI multi-agent: cycle con Cmd+[/] entre build/plan, @mention popover, SessionTree en sidebar | draft (2026-06-05) | ui, agent-loop, **agents**, session | F01 | 5 |

> **F03 (Python en `.venv`) se difiere a v0.1.x** (ver §v0.1.x más
> abajo). En v0.1, un workspace sin venv es perfectamente válido y
> la tool `python_run` retorna `invalid_input` con mensaje claro si
> se invoca sin venv. La creación de venv se hace en v0.1.x.

### Especs de dominio nuevas en Fase B (2026-06-05)

> Las 6 specs escritas en Fase B (este commit de docs) son
> prerrequisito de las features de arriba. Aún en `draft`,
> pendientes de promoción a `review` / `approved`:

- [`domains/journal.md`](../domains/journal.md) — log append-only en
  SQLite puro (16 ACs). Bloqueante de F01.
- [`domains/config.md`](../domains/config.md) — TOML global + workspace
  con `SecretRef` (env / keychain), sin secretos en disco (18 ACs).
  Bloqueante de F05.
- [`features/F05-settings.md`](F05-settings.md) — Tabs
  Providers/Models/Approval/Workspace con `secrets_set` que escribe
  al keychain del SO (15 ACs).
- [`features/F01-chat-streaming.md`](F01-chat-streaming.md) — chat
  con eventos `chat.*.v1`, batching de deltas (50ms), persistencia
  batch (500ms o por tool_call), permission prompts, abort
  (15 ACs).
- [`features/F04-file-diffs.md`](F04-file-diffs.md) — CodeMirror
  Merge con `DiffPayload` enriquecido en `chat.tool_call.v1`,
  `DiffsSidePanel`, read-only en v0.1 (12 ACs).
- [`features/F-agents-ui.md`](F-agents-ui.md) — `AgentChip`,
  `Cmd+[` / `Cmd+]` para cycle, `@mention` popover, `SessionTree`
  con child sessions (15 ACs).

### Acceptance de v0.1

- [ ] Abrir un workspace y ver su árbol de archivos.
- [ ] Si el workspace tiene venv, ver el badge "🐍 .venv X.Y".
- [ ] Si el workspace no tiene venv, **no** se muestra badge ni CTA
  (es válido).
- [ ] Añadir 1 directorio extra al workspace desde la UI y verlo en
  la sección "Extras" del sidebar.
- [ ] El agente puede leer y escribir en el extra path añadido.
- [ ] Quitar un extra path con confirmación.
- [ ] Configurar al menos 1 provider (Ollama local) en Settings.
- [ ] Chatear con streaming visible.
- [ ] Cambiar entre primary `build` y `plan` con Tab y ver cómo
  cambia el system prompt y las tools disponibles.
- [ ] Cuando el modelo pide `read_file`, ver el archivo en la UI.
- [ ] Persistir mensajes y journal entre sesiones de la app.
- [ ] Cerrar y reabrir la app → workspaces, sesiones y extra paths
  intactos.

---

## v0.1.x — F03 Python opt-in (post-MVP)

> Lo que sale del MVP porque no es bloqueante para el agente
> agentic genérico (muchos workspaces no necesitan Python), pero
> que entra rápido en v0.1.x para los que sí.

| ID | Feature | Status | Affects | Depends on | Phase |
|---|---|---|---|---|---|
| F03 | Python en `.venv` del workspace (UI de creación con `uv`/`venv`, tool `python_run` mejorada) | draft | workspace, tools, pty | F02 | 1.x.1 |
| F-extra-paths-tree | Árbol de archivos de un extra path expandible en la UI (con `ignore` patterns) | draft | tools, ui | F02 | 1.x.2 |
| F-extra-paths-cap | Cap configurable de N extra paths (default 20) | draft | workspace, ui | F02 | 1.x.3 |

## v0.2 — Productividad

> Sobre el MVP, añadimos capacidades que hacen la app útil en el
> día a día de un usuario.

| ID | Feature | Status | Affects | Depends on | Phase |
|---|---|---|---|---|---|
| F06 | Servidor web embebido (LAN) — bind opt-in 0.0.0.0, bearer token | draft | server, ipc | F01 | 6 |
| F07 | Visor PDF (PDF.js, lazy) — abrir PDF dentro del workspace | draft | tools, ui | F02 | 7 |
| F08 | Visor DOCX (mammoth.js, lazy) — abrir .docx renderizado | draft | tools, ui | F02 | 7 |
| F09 | Dashboard con métricas — tokens consumidos, latencia providers, tiempo en tools | draft | storage, ui, providers | F01 | 8 |
| F10 | Búsqueda ripgrep-style en workspace (tool `search` mejorada, UI) | draft | tools, ui | F02 | 8 |
| F11 | Aplicar patch unificado (tool `apply_patch` con dry-run) | draft | tools, agent-loop, ui | F04 | 8 |
| F12 | Permisos en UI: prompt "ask" con detalles, remember decision | draft | permissions, ui, agent-loop | F01 | 9 |
| F13 | Múltiples sesiones concurrentes en el mismo workspace (sidebar de sesiones) | draft | session, ui, agent-loop | F01 | 9 |
| F14 | Mensaje multimodal: imágenes y archivos adjuntos | draft | providers, ui | F01 | 10 |
| F15 | Compaction de contexto cuando se acerca al límite del modelo (agente `compaction`) | draft | agent-loop, providers, **agents** | F01, F-agents-ui | 10 |

### Acceptance de v0.2

- [ ] Acceder desde otro device en LAN (iPad, móvil) con bearer token.
- [ ] Abrir un PDF y un DOCX desde el workspace.
- [ ] Ver dashboard con consumo de tokens del día.
- [ ] Buscar en el workspace con regex, glob, case-insensitive.
- [ ] Aplicar un patch con dry-run y luego commit del cambio.
- [ ] Ver un prompt "Allow this tool?" con la tool y los args.
- [ ] Tener varias sesiones en paralelo en el sidebar.
- [ ] Adjuntar una imagen a un mensaje.

---

## v0.3 — Multi-device y colaboración

> De single-device a multi-device. Web como cliente de primera
> clase. Base para sync y remote.

| ID | Feature | Status | Affects | Depends on | Phase |
|---|---|---|---|---|---|
| F16 | UI desde navegador (no solo Tauri webview) — `lib/ipc.ts` con transporte http+SSE | draft | server, ui, ipc | F06 | 11 |
| F17 | Sincronización read-only entre devices (cambios en workspace A visibles en B) | draft | storage, server, session | F06 | 12 |
| F18 | Notificaciones de cambios (file_changed) propagadas a otros clients | draft | server, ui, workspace | F16 | 12 |
| F19 | Tunnel WAN opt-in (cloudflared) — un click y se expone públicamente con URL ephemeral | draft | server | F06 | 13 |

### Acceptance de v0.3

- [ ] Abrir la UI en un navegador (no Tauri) y chatear con el mismo
  provider que en el desktop.
- [ ] Empezar un chat en Mac, ver el progreso en el iPad.
- [ ] Cambiar un archivo del workspace desde Finder y verlo refrescado
  en la UI.
- [ ] Exponer públicamente con un click y compartir el link a alguien.

---

## v1.0 — Polish y release

> Lo que separa "internal dogfood" de "public release".

| ID | Feature | Status | Affects | Depends on | Phase |
|---|---|---|---|---|---|
| F20 | Auto-updater firmado (tauri-plugin-updater) con canal `stable/beta/dev` | draft | updater, app | — | 14 |
| F21 | Notarización macOS y signing Windows | draft | app, ci | — | 14 |
| F22 | Métricas locales de uso (stats.db) + UI de "mi semana" | draft | storage, ui | F09 | 14 |
| F23 | Onboarding: primer workspace, primer provider, primer chat | draft | ui, config | F01, F05 | 14 |
| F24 | Keyboard shortcuts (Cmd+K, Cmd+Shift+P, etc.) | draft | ui | F01 | 15 |
| F25 | i18n: inglés + español (mínimo) | draft | ui | — | 15 |
| F26 | Logging estructurado visible en la UI (Settings → Logs) | draft | app, ui | — | 15 |
| F27 | Crash reporter local (no telemetría) con UI de "compartir" | draft | app, ui | — | 15 |
| F28 | Documentación de usuario (Getting Started, FAQ, troubleshooting) | draft | docs | F01–F05 | 16 |
| F29 | Release pipeline: builds firmados, instaladores, channel feed | draft | ci, app | F20, F21 | 16 |

### Acceptance de v1.0

- [ ] El usuario descarga el .dmg/.exe/.deb/.AppImage desde un sitio web.
- [ ] El binario está firmado y notarizado (Gatekeeper pasa sin
  warnings en macOS).
- [ ] El updater ofrece nuevas versiones en canales separados.
- [ ] La UI soporta EN y ES.
- [ ] Hay un "Getting Started" que un user nuevo entiende en < 5 min.
- [ ] Crash logs locales se pueden ver y copiar desde la UI.

---

## Backlog (no comprometidas)

> Features fuera de roadmap firme. Se priorizan tras v1.0 según
> feedback de users.

- F30: TUI mínima (ratatui) compartiendo `agentyx-core` — para
  entornos sin GUI.
- F31: MCP server (la app expuesta como tool para otros agentes).
- F32: Custom tools definidas por el usuario (YAML/JSON).
- F33: Marketplace de providers y tools.
- F34: Voice input en el composer (Whisper local).
- F35: Compartir sesión vía link (read-only).
- F36: Modo headless / CI: ejecutar el agente sin GUI desde CLI.
- F-extra-agents: Custom agents definidos por el usuario en
  `~/.agentyx/agents/*.md` (ver [agents.md §Custom agents](../agents.md)).
- F-extra-providers: Reintroducir `openai_compat` genérico
  (Together, OpenRouter, LM Studio, Jan) + OpenAI nativo + Anthropic
  nativo (ver [ADR-0008](../adr/0008-providers-v1-scope.md)).
- F39: Auto-summarization de sesiones largas (background; agente
  `summary`, ver [agents.md](../agents.md)).
- F40: Integración con git: commits automáticos, branches, PRs.
- F41: Workspace rootless (lista pura de paths, sin `root_path`
  obligatorio; ver [ADR-0007 §Consequences](../adr/0007-extra-paths-per-workspace.md)).

---

## Visualización de dependencias

```
v0.1 (Foundation) — incluyendo dominios nuevos (Fase B 2026-06-05)

  Dominios fundamentales:
    journal.md ──┐
                 ├──► F05 (settings) ──► F01 (chat + multi-agent) ──┐
    config.md ───┘                                  │               │
                                                   │   F04 (diffs) │
                                                   │       │       │
                                                   │       ▼       │
                                                   │  F-agents-ui ◄┘
                                                   │
  v0.1.x                                            │
    F03 (python venv opt-in)                        │
    F-extra-paths-tree, F-extra-paths-cap           │
                                                   │
v0.2 (Productividad)                               │
  F01 ──► F06 (server LAN)                          │
              │                                    │
              └──► F16 (browser UI) ──► v0.3       │
                                            F04 ──► F11 (apply_patch)
                                            F01 ──► F12 (permisos UI)
                                            F01 ──► F-agents-ui ◄──┐
                                                        F13 (multi-session)
                                                        F14 (multimodal)
                                            F01, F-agents-ui ──► F15 (compaction)
  F02 ──► F07, F08, F10, F13, F18
  F01 ──► F09 (dashboard)

v1.0 (Polish) — independiente de v0.2/v0.3 mayormente.
  F20, F21 (release infra) son bloqueantes para F29.
  F23, F24, F25, F26, F27, F28 son UX/docs, no bloquean entre sí.
```

> **Notas del grafo**:
> - `journal.md` y `config.md` son **dominios fundamentales** del MVP,
>   no features. Se modelan aparte en [`../STATUS.md`](../STATUS.md).
> - `F-agents-ui` depende de F01 (no al revés). Se renderiza
>   encima de los eventos que F01 ya emite.
> - `F04` (diffs) **read-only en v0.1**: depende de F01 pero no
>   requiere lógica de "apply/reject" (eso es v0.2 con F12).

---

## Cómo se crea una feature spec

1. Copiar [`../templates/feature-spec.md`](../templates/feature-spec.md)
   a `F<NN>-<slug>.md` con el siguiente número libre.
2. Rellenar `User story`, `Scope`, `UX/UI`, `Flow`, `Affected
   domains`, `Acceptance criteria`, `Tests`.
3. Poner `Status: draft`. Pasar a `approved` cuando haya consenso.
4. Añadir a este roadmap (esta fila).

## Reglas de status

- `draft` → `approved`: revisión y consenso.
- `approved` → `in-progress`: alguien empieza a codear.
- `in-progress` → `shipped`: mergeada y en release.
- `shipped` no vuelve atrás (si hay que rehacer, abrir nueva feature
  con `supersedes: F<NN>`).
