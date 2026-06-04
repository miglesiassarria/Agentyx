# Features — Roadmap

> Vista por features. Para vista global: [specs/README.md](../README.md).
> Para índice de ADRs: [specs/adr/README.md](../adr/README.md).
> Última actualización: 2026-06-04

## Leyenda

- **Status**: `draft` (spec en redacción) | `approved` (lista para implementar) | `in-progress` (código en marcha) | `shipped` (en release).
- **Phase**: orden aproximado de implementación dentro de la versión (no estricto; depende de la spec).
- **Depends on**: features cuyo spec debe estar al menos `approved` antes de empezar esta.
- **Affects**: specs de [dominio](../domains/) que la feature consume.

---

## v0.1 — Foundation (MVP)

> La app debe ser utilizable: abrir un workspace, configurar un
> provider, chatear, ejecutar tools básicas, gestionar `.venv`,
> stream LLM en la UI, todo persistido.

| ID | Feature | Status | Affects | Depends on | Phase |
|---|---|---|---|---|---|
| [F02](F02-multi-workspace.md) | Multi-workspace: list, open, delete, detectar .venv, crear venv explícito | draft | workspace | — | 1 |
| F05 | Settings: providers activos, modelos, keychain entry, approval_mode | draft | providers, permissions, config | F02 | 2 |
| F01 | Chat con streaming LLM (provider agnóstico) | draft | agent-loop, providers, session | F02, F05 | 3 |
| F03 | Python en .venv del workspace (badge UI, CTA "Crear venv", tool python_run) | draft | workspace, tools, pty | F02 | 4 |
| F04 | File diffs en UI (CodeMirror merge) tras edit_file / apply_patch | draft | tools, ui | F01, F02 | 5 |

### Acceptance de v0.1

- [ ] Abrir un workspace y ver su árbol de archivos.
- [ ] Ver el badge "🐍 .venv" si tiene venv; si no, "🐍 No venv" + CTA.
- [ ] Crear venv explícitamente desde la UI.
- [ ] Configurar al menos 1 provider (Ollama local) en Settings.
- [ ] Chatear con streaming visible.
- [ ] Cuando el modelo pide `read_file`, ver el diff/file en la UI.
- [ ] Cuando el modelo pide `python_run`, ver el output en la UI.
- [ ] Persistir mensajes y journal entre sesiones de la app.
- [ ] Cerrar y reabrir la app → conversaciones intactas.

---

## v0.2 — Productividad

> Sobre el MVP, añadimos capacidades que hacen la app útil en el
> día a día de un developer.

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
| F15 | Compaction de contexto cuando se acerca al límite del modelo | draft | agent-loop, providers | F01 | 10 |

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
- F37: Sub-agentes: el agente principal puede delegar a sub-agentes
  con system prompts distintos.
- F38: Soporte de providers no-OpenAI ni Anthropic (Bedrock, Vertex,
  Cohere, Mistral native).
- F39: Auto-summarization de sesiones largas (background).
- F40: Integración con git: commits automáticos, branches, PRs.

---

## Visualización de dependencias

```
v0.1 (Foundation)
  F02 (workspaces) ────► F05 (settings) ────► F01 (chat) ──┐
       │                                                    │
       └─────────────────► F03 (python venv)                │
                                                             │
                                          F04 (diffs) ◄────┘
                                              │
v0.2 (Productividad)                          │
  F01 ──► F06 (server LAN)                    │
              │                                │
              └──► F16 (browser UI) ──► v0.3   │
                                            F04 ──► F11 (apply_patch)
                                            F01 ──► F12 (permisos UI)
                                                       F13 (multi-session)
                                                       F14 (multimodal)
                                                       F15 (compaction)
  F02 ──► F07, F08, F10, F13, F18
  F01 ──► F09 (dashboard)

v1.0 (Polish) — independiente de v0.2/v0.3 mayormente.
  F20, F21 (release infra) son bloqueantes para F29.
  F23, F24, F25, F26, F27, F28 son UX/docs, no bloquean entre sí.
```

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
