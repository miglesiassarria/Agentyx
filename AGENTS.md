# AGENTS.md

> Convenciones, reglas y arquitectura de **Agentyx** — escritorio, rápido, ligero, agentic.
> Este documento es la fuente de verdad para cualquier agente (humano o IA) que trabaje en el repo.

---

## 1. Stack tecnológico

### 1.1 Shell y core
- **Tauri 2** (Rust) — desktop shell multiplataforma.
- **Rust** (edition 2021, MSRV `1.80+`).
- **Tokio** como runtime async.
- **reqwest** con `stream` y `rustls-tls` para HTTP/SSE.
- **serde** + **serde_json** + **toml** para serialización y config.
- **rusqlite** (bundled) para persistencia local. Sin servidor SQL externo.
- **portable-pty** para PTY multiplataforma.
- **notify** para file watcher.
- **tracing** + **tracing-subscriber** para logs estructurados.
- **anyhow** (binario/app) / **thiserror** (librerías).
- **ulid** para identificadores ordenables.

### 1.2 Frontend
- **Svelte 5** (runes) + **Vite**.
- **TypeScript** estricto.
- **CSS** plano o **Tailwind v4** (opt-in, evitar siempre que no aporte).
- **CodeMirror 6** + `@codemirror/merge` para diffs y edición de código.
- **PDF.js** (lazy-load) para PDFs.
- **mammoth.js** (lazy-load) para `.docx`.
- **uPlot** o **Chart.js** para dashboards.
- **marked** + **DOMPurify** para markdown seguro.
- **shiki** para syntax highlight (lazy).

### 1.3 Targets
- **macOS** (desarrollo principal), **Windows 10/11**, **Linux** (Ubuntu/Debian, Fedora, Arch).
- Binario instalado objetivo: **< 20 MB**.
- Arranque frío objetivo: **< 500 ms**.
- RAM en reposo objetivo: **< 80 MB**.

### 1.4 Lo que NO usamos
- ❌ Electron, ❌ Node.js embebido en producción, ❌ Chromium, ❌ Tauri con `withGlobalTauri` salvo necesidad justificada.
- ❌ Frameworks pesados de UI (React, Vue, Angular) — Svelte es el límite.
- ❌ Bases de datos externas (Postgres, Redis, etc.) en v1.
- ❌ Bundlers distintos de Vite en el frontend.
- ❌ TypeScript con `any` salvo puente justificado en bordes de FFI.

---

## 2. Principios de diseño

1. **Ligero y rápido** — cada dependencia se justifica por valor; cada KB cuenta.
2. **Lógica de negocio en Rust** — la UI es solo presentación.
3. **IPC tipado y explícito** — nada de strings mágicas en eventos.
4. **Streaming por defecto** — LLM, PTY y logs streamean eventos al frontend.
5. **Sandbox por workspace** — cada workspace es una jaula lógica: su
   config, su historial, sus permisos, sus `extra_paths`. El sandbox
   de paths es `root_path ∪ extra_paths` del workspace, **no** solo
   el `root_path` (ver ADR-0007).
6. **Reversible y reproducible** — toda acción del agente queda en un journal append-only.
7. **Fail loudly, fail early** — errores con contexto suficiente, no silenciar nunca.
8. **DRY/KISS/SOLID** — sin abstracciones especulativas; separar cuando aporte.
9. **Multi-agent desde el inicio** — la arquitectura de agentes
   modela `Primary | Subagent | Hidden` desde v1 (ver
   [`specs/agents.md`](specs/agents.md)). Aunque v1 solo traiga
   1–2 primary y 1 subagent built-in, no debe haber una ruta de
   código que asuma "un único agente por sesión". Esto evita
   refactors cuando se añadan agentes custom en v1.x.
10. **Local-first** — ningún archivo del workspace sale del
    dispositivo. El único tráfico de red saliente son las llamadas
    a los providers LLM que el usuario haya configurado
    explícitamente.

---

## 3. Estructura del proyecto

```
agentyx/
├── AGENTS.md                       # este archivo
├── README.md
├── LICENSE
├── package.json                    # scripts orquestadores (bun/npm)
├── bunfig.toml                     # opcional, si usamos bun
├── .gitignore
├── .gitattributes
├── .editorconfig
├── .nvmrc                          # pin de Node si se usa en tooling
│
├── crates/                         # workspace Rust (cargo)
│   ├── Cargo.toml                  # [workspace]
│   ├── rustfmt.toml
│   ├── clippy.toml
│   ├── deny.toml                   # cargo-deny: licencias + advisories
│   │
    │   ├── agentyx-core/               # librería pura: dominio, sin Tauri
    │   │   ├── Cargo.toml
    │   │   └── src/
    │   │       ├── lib.rs
    │   │       ├── agent/              # loop, tools, prompts
    │   │       │   ├── mod.rs
    │   │       │   ├── loop.rs
    │   │       │   ├── tool.rs
    │   │       │   └── prompt.rs
    │   │       ├── agents/             # AgentSpec, registry, prompts
    │   │       │   ├── mod.rs
    │   │       │   ├── spec.rs         # struct + enums
    │   │       │   ├── registry.rs     # carga built-in + custom
    │   │       │   └── prompt.rs       # prompts embebidos
    │   │       ├── llm/                # providers (ollama, groq, minimax)
│   │       │   ├── mod.rs
│   │       │   ├── provider.rs     # trait Provider
│   │       │   ├── openai.rs
│   │       │   ├── anthropic.rs
│   │       │   ├── ollama.rs
│   │       │   ├── streaming.rs    # SSE → stream de eventos
│   │       │   └── types.rs
│   │       ├── workspace/          # workspaces, .venv, paths
│   │       │   ├── mod.rs
│   │       │   ├── venv.rs
│   │       │   └── detect.rs
│   │       ├── pty/                # wrapper sobre portable-pty
│   │       │   └── mod.rs
│   │       ├── tools/              # tools que el agente invoca
│   │       │   ├── mod.rs
│   │       │   ├── read_file.rs
│   │       │   ├── write_file.rs
│   │       │   ├── edit_file.rs
│   │       │   ├── search.rs
│   │       │   ├── shell.rs
│   │       │   └── python.rs
│   │       ├── storage/            # SQLite, migraciones, repos
│   │       │   ├── mod.rs
│   │       │   ├── db.rs
│   │       │   ├── migrations/
│   │       │   ├── sessions.rs
│   │       │   ├── messages.rs
│   │       │   └── audit.rs
│   │       ├── journal/            # log append-only de acciones
│   │       │   └── mod.rs
│   │       ├── permissions/        # matriz de permisos por tool
│   │       │   └── mod.rs
│   │       ├── config/             # carga/validación de config
│   │       │   ├── mod.rs
│   │       │   └── schema.rs
│   │       ├── error.rs            # AppError + From impls
│   │       └── ids.rs
│   │
│   ├── agentyx-app/                # binario Tauri (entrypoint desktop)
│   │   ├── Cargo.toml
│   │   ├── build.rs
│   │   ├── tauri.conf.json
│   │   ├── capabilities/
│   │   │   └── default.json        # permisos Tauri por ventana
│   │   ├── icons/                  # iconos multiplataforma
│   │   └── src/
│   │       ├── main.rs
│   │       ├── commands/           # #[tauri::command] handlers
│   │       │   ├── mod.rs
│   │       │   ├── workspace.rs
│   │       │   ├── llm.rs
│   │       │   ├── pty.rs
│   │       │   └── session.rs
│   │       ├── events.rs           # canales → window.emit
│   │       ├── state.rs            # AppState (Arc<Mutex<...>>)
│   │       ├── ipc.rs              # tipos compartidos
│   │       ├── window.rs
│   │       ├── menu.rs
│   │       ├── updater.rs
│   │       └── deep_link.rs
│   │
│   └── agentyx-sdk/                # SDK Rust reutilizable (futuro)
│       ├── Cargo.toml
│       └── src/lib.rs
│
├── ui/                             # frontend Svelte 5 + Vite
│   ├── package.json
│   ├── tsconfig.json
│   ├── svelte.config.js
│   ├── vite.config.ts
│   ├── index.html
│   └── src/
│       ├── main.ts                 # entrypoint
│       ├── app.css                 # estilos globales (mínimos)
│       ├── app.svelte              # root component
│       ├── lib/
│       │   ├── ipc.ts              # wrapper tipado de invoke()/listen()
│       │   ├── ipc-types.ts        # tipos compartidos con Rust
│       │   ├── stores/             # estado global con runes
│       │   │   ├── session.svelte.ts
│       │   │   ├── workspace.svelte.ts
│       │   │   └── ui.svelte.ts
│       │   ├── components/
│       │   │   ├── ChatPanel.svelte
│       │   │   ├── MessageList.svelte
│       │   │   ├── Composer.svelte
│       │   │   ├── DiffView.svelte         # CodeMirror merge
│       │   │   ├── Editor.svelte           # CodeMirror base
│       │   │   ├── FileTree.svelte
│       │   │   ├── PdfViewer.svelte        # lazy
│       │   │   ├── DocxViewer.svelte       # lazy
│       │   │   ├── WebArtifact.svelte      # iframe sandbox
│       │   │   ├── Dashboard.svelte        # uPlot
│       │   │   ├── PtyTerminal.svelte      # xterm.js opcional
│       │   │   ├── VenvStatus.svelte
│       │   │   └── ProviderPicker.svelte
│       │   ├── routes/             # navegación (hash router o svelte-routing)
│       │   │   ├── Home.svelte
│       │   │   ├── Workspace.svelte
│       │   │   └── Settings.svelte
│       │   └── utils/
│       │       ├── format.ts
│       │       └── markdown.ts
│       └── assets/
│
├── scripts/                        # tooling del repo (no del binario)
│   ├── fmt.sh
│   ├── lint.sh
│   ├── test.sh
│   └── release.sh
│
└── docs/
    ├── architecture.md
    ├── ipc.md
    ├── providers.md
    └── security.md
```

### 3.1 Reglas de organización
- `crates/agentyx-core` **nunca** depende de Tauri. Es la librería pura.
- `crates/agentyx-app` es el único crate con `tauri = ...`.
- Todo handler Tauri va en `crates/agentyx-app/src/commands/`.
- Toda comunicación con el frontend pasa por `commands/` (request) o `events.rs` (stream).
- Componentes de UI **nunca** llaman a APIs de Node/Electron — solo a `lib/ipc.ts`.

---

## 4. Convenciones de código

### 4.1 Rust
- `rustfmt` y `clippy` limpios, sin warnings, antes de commit.
- `cargo deny` limpio (licencias y advisories).
- Sin `unwrap()` en código de producción. Usar `?` + `anyhow::Context` o `thiserror`.
- Sin `unsafe` salvo en bordes justificados (FFI, hot paths). Documentar el `// SAFETY: ...`.
- Preferir `&str` sobre `String`, `&[T]` sobre `Vec<T>` en firmas.
- Errores via `Result<T, AppError>` en `agentyx-core`; `Result<T, anyhow::Error>` solo en `agentyx-app/commands`.
- Tests al lado del código (`#[cfg(test)] mod tests`) o en `tests/` (integration).
- Nombrar: `snake_case` (funciones, variables), `PascalCase` (tipos, traits), `SCREAMING_SNAKE_CASE` (constantes).
- Módulos: `mod.rs` solo cuando hace falta sub-módulos privados; preferir archivos planos.

### 4.2 TypeScript / Svelte
- `tsconfig` con `strict: true`, `noUncheckedIndexedAccess: true`.
- Sin `any`. Usar `unknown` + type guards.
- Componentes `.svelte` con `<script lang="ts">`.
- Estado compartido con **runes** (`$state`, `$derived`, `$effect`), no stores legacy.
- Una responsabilidad por componente. Helpers puros en `lib/utils/`.
- Props tipadas con `interface Props` en cada componente.
- CSS scoped por componente, salvo tokens globales en `app.css`.

### 4.3 Estilo general
- Comentarios solo donde aporten contexto (no qué hace, sino por qué).
- Mensajes de commit: conventional commits (`feat:`, `fix:`, `refactor:`, `docs:`, `test:`, `chore:`).
- PRs: una preocupación por PR. Diff pequeño y revisable.
- Ramas: `feat/<scope>`, `fix/<scope>`, `chore/<scope>`.

---

## 5. IPC y contratos

### 5.1 Reglas
- Todo `#[tauri::command]` está en `crates/agentyx-app/src/commands/<scope>.rs`.
- Los argumentos y retornos son **tipos serializables** (serde). No `Value` opaco.
- Nombres en `snake_case` (Rust) y en `camelCase` (TS) — usar `#[serde(rename_all = "camelCase")]` en el lado Rust.
- Errores devueltos como `{ code, message, context? }`. La UI nunca parsea mensajes de error; usa el `code`.
- Eventos (stream) tienen un **schema versionado**: `chat.message.v1`, `pty.output.v1`, etc.
- La UI escucha eventos solo a través de `lib/ipc.ts`. Nunca `window.__TAURI__` directo.

### 5.2 Convenciones
- `commands/`: request/response (esperar resultado).
- `events`: push unidireccional (streaming, progreso, logs).
- Un canal por concern. No multiplexar.

---

## 6. Estructura del agente (loop agentic)

El agente sigue el patrón **ReAct con tools y journal**, similar a opencode pero simplificado. La arquitectura de agentes es **multi-agent desde el inicio** (ver [`specs/agents.md`](specs/agents.md)):

- v1 incluye **2 primary agents** (`build` con todas las tools, `plan` read-only) y **1 subagent** (`general`) built-in. El usuario puede cycle entre los primary con la tecla `Tab` (UX借鉴 opencode).
- Los **subagents** son invocados por un primary vía tool `task` o manualmente con `@<agent-id>` en un mensaje. Cada subagent corre en su propia child session, con su propio journal.
- Los **hidden agents** (`compaction`, `title`, `summary`) están reservados en v1 para no tener que refactorear cuando se implementen en v1.x.

El loop concreto:

```
user message
   │
   ▼
[session.send]
   │
   ▼
[agent loop]
   │
   ├──► provider.chat (streaming) ──► tokens al UI (evento)
   │
   ├──► tool call detectado ──► permission check
   │                              │
   │                              ├── DENY  → pedir confirmación al usuario
   │                              ├── ASK   → mostrar UI de aprobación
   │                              └── ALLOW → ejecutar tool
   │                                       │
   │                                       ▼
   │                                  tool.run(args)
   │                                       │
   │                                       ├── resultado ──► journal.append
   │                                       └── stdout/err ──► evento
   │
   └──► repetir hasta finish_reason == "stop" o max_steps
```

### 6.1 Tools mínimas (v1)
- `read_file(path)` — lee archivo de texto, con offset/limit.
- `write_file(path, content)` — escribe o crea.
- `edit_file(path, old_text, new_text)` — edición quirúrgica.
- `search(query, path?, glob?)` — búsqueda ripgrep-style.
- `shell(command, cwd?, timeout?)` — ejecución no-PTY.
- `python_run(code, venv?)` — ejecuta Python en el `.venv` del workspace.
- `list_dir(path, depth?)` — listado de directorio.
- `apply_patch(diff)` — aplica un diff unificado (formato similar a opencode).

### 6.2 Permisos
- **Matriz por workspace**: cada workspace declara `allowed_tools` y `denied_paths`.
- **Prompt de aprobación** para: escritura fuera del workspace, shell con comandos destructivos, acceso a red.
- **Settings globales**: usuario puede marcar tools como "always allow" o "always deny".
- **Sin bypass**: ni siquiera para el propio agente. Toda acción se registra.

### 6.3 Journal
- Append-only, en SQLite (`journal` table) o archivo rotado (`journal.jsonl`).
- Cada entrada: `id, session_id, ts, tool, args, result, duration_ms, permission_decision`.
- Permite replay y debug post-mortem.

---

## 7. Workspaces, extra paths y .venv

### 7.1 Workspace
- Un workspace = un proyecto del usuario. Tiene:
  - **`root_path`**: carpeta principal, donde el agente trabaja por defecto.
  - **0..N `extra_paths`**: directorios adicionales con R/W que el
    usuario autoriza explícitamente. Ver
    [ADR-0007](specs/adr/0007-extra-paths-per-workspace.md).
- Estado por workspace en `~/.agentyx/workspaces/<id>/`:
  - `config.toml` — config del workspace (provider, modelo, venv path,
    ignore patterns, `[[extra_paths]]`).
  - `state.db` — SQLite local del workspace (sesiones, mensajes, índice,
    `extra_paths_json`).
  - `journal.jsonl` — log append-only (alternativa a DB).
- Cache e índices en `~/.agentyx/cache/<workspace-hash>/`.
- **Sandbox de paths**: `root_path ∪ extra_paths`. El path traversal
  contra el `root` se mantiene; además, todo path fuera de
  `root ∪ extras` retorna `path_outside_workspace`.

### 7.2 Python y .venv (opt-in en v1)
- **El `.venv` NO es obligatorio**. Un workspace sin venv es
  perfectamente válido y no se crea nada al abrir.
- Detección (pasiva): buscar `.venv/`, `venv/`, `.python-version` (pyenv), `pyproject.toml` (uv/poetry/pdm).
  Si existe, se muestra como info en la UI; si no, no se muestra nada
  (no hay CTA "Crear venv aquí" en F02; eso se difiere a F03 en v0.1.x).
- Si la tool `python_run` se invoca con `venv: "auto"` y el workspace
  no tiene venv, retorna `invalid_input` con mensaje claro
  sugiriendo `workspace_create_venv` o usar `venv: "system"`. **No**
  auto-crea.
- Activación: ejecutar el binario directamente (`.venv/bin/python` o
  `.venv\Scripts\python.exe`). No usar `source activate` (no es
  multiplataforma).
- `uv` como backend preferido si está disponible; fallback a `python -m venv`.
- Respetar `pyproject.toml` si existe (leer `[project] requires-python`).

---

## 8. Providers LLM

### 8.1 Trait común
```rust
#[async_trait]
pub trait Provider: Send + Sync {
    fn id(&self) -> &'static str;
    fn models(&self) -> &[ModelInfo];
    async fn chat(&self, req: ChatRequest) -> Result<ChatStream, AppError>;
    fn capabilities(&self, model: &str) -> ModelCapabilities;
}
```

### 8.2 Soportados (v1)
- **Ollama** (local, NDJSON; endpoint configurable, default
  `http://127.0.0.1:11434`). **Default**.
- **Groq** (OpenAI-compatible; rápido y barato).
- **Minimax** (Anthropic-compatible; bueno para razonamiento;
  integración estilo opencode "token plan"; API key de
  `platform.minimax.io`).

> **Ver [ADR-0008](specs/adr/0008-providers-v1-scope.md)** para la
> justificación de los 3 elegidos. OpenAI nativo, Anthropic nativo,
> Bedrock, Vertex, Cohere, y un `openai_compat` genérico se difieren
> a v1.x / v2.

### 8.3 Config
- En `~/.agentyx/config.toml`:
  ```toml
  [providers.ollama]
  base_url = "http://127.0.0.1:11434"

  [providers.groq]
  api_key = "env:GROQ_API_KEY"
  base_url = "https://api.groq.com/openai/v1"

  [providers.minimax]
  api_key = "env:MINIMAX_API_KEY"
  base_url = "https://api.minimax.io/v1"

  default_provider = "ollama"
  default_model = "llama3.1:8b"
  ```
- **Nunca** guardar API keys en texto plano en el repo. Usar `env:VAR_NAME` o keychain del SO.
- La app resuelve `env:...` al cargar y cachea en memoria. No loguea keys.

### 8.4 Streaming
- Todos los providers streamean via SSE (o NDJSON para Ollama si no soporta SSE).
- Normalizar a un único enum `ChatEvent`:
  ```rust
  pub enum ChatEvent {
      MessageStart { id: String, model: String },
      ContentDelta { text: String },
      ToolUse { id: String, name: String, args: serde_json::Value },
      ToolResult { id: String, output: String, is_error: bool },
      MessageEnd { usage: Usage, finish_reason: FinishReason },
      Error { code: String, message: String },
  }
  ```
- El frontend solo conoce `ChatEvent`. No provider-specific shapes.

---

## 9. Seguridad

### 9.1 Principios
- **Mínimo privilegio** — capabilities Tauri ajustadas al mínimo por ventana.
- **Secrets nunca en disco plano** — keychain del SO (Keychain en macOS, Credential Manager en Windows, Secret Service en Linux via `keyring` crate).
- **No ejecutar comandos arbitrarios sin permiso explícito del usuario.**
- **Aislamiento por workspace** — tools restringidas al
  `root_path ∪ extra_paths` del workspace (no solo el root). El
  path traversal contra el root se mantiene; además, todo path
  fuera de `root ∪ extras` retorna `path_outside_workspace`.
- **Sin telemetría por defecto** — opt-in, granular, off by default.
- **Updates firmados** — `tauri-plugin-updater` con firma criptográfica.
- **Content Security Policy estricta** — `script-src 'self'`, sin `unsafe-inline` en producción.

### 9.2 Reglas concretas
- ❌ No leer variables de entorno con prefijo de secrets fuera del resolver de config.
- ❌ No loguear `Authorization`, cookies, ni bodies de request que contengan tokens.
- ❌ No persistir contenido de archivos binarios en el journal (solo path + hash).
- ✅ Validar toda entrada de tool con schemas (zod-like en Rust: `validator` o derive).
- ✅ Sanitizar todo output de tool que vaya a renderizarse como HTML (`DOMPurify`).
- ✅ Rate-limiting en providers locales (Ollama) para no saturar GPU.
- ✅ Timeouts agresivos en toda operación de red o subprocess (default 30s, ajustable).
- ✅ Path traversal bloqueado: resolver y canonicalizar antes de cualquier I/O.

### 9.3 Sandboxing nativo (futuro)
- macOS: `sandbox-exec` con profile restrictivo.
- Linux: namespaces + seccomp.
- Windows: AppContainer / Job Objects.
- v1: aislamiento lógico por workspace. Sandboxing real en v2.

---

## 10. Testing

### 10.1 Niveles
- **Unit** (Rust): al lado del código, con `#[cfg(test)]`. Mocks solo en bordes de FFI.
- **Unit** (TS/Svelte): `vitest` + `@testing-library/svelte` para componentes.
- **Integration** (Rust): `tests/` por módulo, con DB temporal y PTY fake.
- **E2E** (Tauri): `tauri-driver` (WebDriver) para flujos críticos.
- **E2E** (manual): smoke test con Ollama local en CI.

### 10.2 Fixtures
- Workspaces de prueba en `crates/agentyx-core/tests/fixtures/`.
- Providers mockeados con respuestas grabadas (snapshots de SSE).
- PTY: usar `script` Unix o `winpty` para tests reproducibles.

### 10.3 Reglas
- Sin mocks espurios. Si algo es difícil de testear, probablemente está mal estructurado.
- Cobertura objetivo: `> 70%` en `agentyx-core`, `> 50%` en el resto.
- Tests de provider son contratos: si Ollama cambia la API, fallamos antes de runtime.

---

## 11. Logging y observabilidad

- `tracing` con niveles por módulo (`RUST_LOG=agentyx_core::llm=debug`).
- UI muestra solo `info` y superior por defecto; `debug` con toggle.
- File watcher de `journal.jsonl` permite ver en vivo desde la UI.
- Métricas locales (no se envían a ningún lado) en `~/.agentyx/stats.db`:
  - tokens consumidos por sesión/día
  - latencia de providers
  - tiempo en tools

---

## 12. Versionado y releases

- **Semver** estricto.
- Tags: `vMAJOR.MINOR.PATCH`.
- Changelog autogenerado desde conventional commits.
- Binarios firmados y notarizados (macOS notarization, Windows signing).
- Canales: `stable`, `beta`, `dev` (como opencode).
- `tauri-plugin-updater` con manifests por canal.

---

## 13. Dependencias — política

- **Core Rust**: solo crates con mantenimiento activo, sin `unsafe` salvo justificación.
- **Frontend**: cada dep adicional debe justificarse en el PR. Evitar `moment`, `lodash`, `axios` (usar `fetch` nativo).
- **Auditorías**: `cargo audit` y `cargo deny` en CI.
- **Pin de versiones** en `Cargo.lock` y `package-lock.json`. Commiteados.
- **Renovación**: Dependabot o Renovate, agrupado por tipo, PRs semanales.

---

## 14. Anti-patrones prohibidos

- ❌ `unwrap()` / `expect()` en código de producción.
- ❌ `panic!` para errores recuperables.
- ❌ `tokio::spawn` sin `JoinHandle` manejado o documentado.
- ❌ `unsafe` sin comentario `// SAFETY:`.
- ❌ `any` en TypeScript.
- ❌ Strings mágicas en IPC. Todo enum o constante.
- ❌ Dependencias circulares entre módulos.
- ❌ Lógica de negocio en componentes Svelte (extraer a `lib/`).
- ❌ Estado global mutable implícito (usar runes explícitos).
- ❌ Network calls desde el renderer (siempre via Rust commands).
- ❌ `eval`, `Function()`, `innerHTML` sin sanitizar.
- ❌ Hardcodear URLs de providers — siempre via config.

---

## 15. Checklist antes de merge

- [ ] `cargo fmt --all -- --check` pasa.
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` pasa.
- [ ] `cargo test` pasa al 100%.
- [ ] `cargo deny check` pasa.
- [ ] `pnpm lint` (o `bun run lint`) pasa.
- [ ] `pnpm typecheck` pasa.
- [ ] `pnpm test` pasa.
- [ ] Smoke test manual con Ollama local.
- [ ] Sin secretos nuevos en el diff.
- [ ] CHANGELOG actualizado si hay cambio de cara al usuario.
- [ ] **Spec sync (regla §17.5)**: si el PR toca código cubierto
      por una spec, el diff incluye:
      1. `specs/STATUS.md` actualizado (spec movida a su nueva
         sección, fecha "Última actualización" refrescada).
      2. La spec afectada con `Status` cambiado, ACs marcados
         con `[x]`, y (si aplica) sección `## Implementation
         status` y/o `## Discovered bugs` actualizadas.
      3. Sección `## Spec status changes` del cuerpo del PR
         listando cada spec tocada y su nuevo estado.

---

## 16. Referencias y convenciones seguidas

- Patrones de proyecto inspirados en **opencode-dev** (estructura
  monorepo, separación core/app, IPC tipado, journal, **modelo
  multi-agent con primary + subagent + child sessions, ciclo con
  Tab, integración con providers como Minimax token plan**).
- opencode es **referencia arquitectónica**, no el público objetivo.
  Agentyx es **agentic-first** (ver
  [`specs/project.md`](specs/project.md) §Visión): el usuario
  quiere delegar tareas complejas sobre sus proyectos, no solo
  programar. opencode se cita como ejemplo de UX de editor; las
  decisiones se adaptan a un público más amplio.
- Patrones de Tauri: docs oficiales de Tauri 2, `tauri-plugin-*` oficiales.
- Patrones de Svelte 5: runes y stores explícitos.
- Patrones de Codex: workspace por proyecto, multiple agents, tools, permisos.

---

## 17. Spec-Driven Development (OBLIGATORIO)

Este proyecto sigue **Spec-Driven Development**. Es una **regla dura**, no una recomendación. Toda la lógica que cruza más de un archivo debe estar respaldada por una spec en [`specs/`](specs/) **antes** de codearse.

### 17.1 Reglas inquebrantables

1. **PROHIBIDO** tocar código de un dominio si no existe `specs/domains/<x>.md` con `Status` ≥ `draft`.
2. **PROHIBIDO** implementar una feature si no existe `specs/features/F<NN>-<slug>.md` con `Status` ≥ `approved` y acceptance criteria definidos.
3. **PROHIBIDO** mergear un PR que no referencie explícitamente las specs que implementa. Formato obligatorio en el cuerpo del PR:
   `Refs: specs/domains/agent-loop.md#AC3, specs/features/F03-python-venv.md#F03.AC1`
4. **PROHIBIDO** cambiar la API de un Tauri command, endpoint HTTP o evento streaming sin actualizar [`specs/ipc.md`](specs/ipc.md) y la spec de dominio afectada en el mismo PR.
5. **OBLIGATORIO** que cada acceptance criterion tenga al menos un test cuyo nombre derive del AC (ej: `AC3` → test `ac3_<short>`; `F03.AC1` → test `f03_ac1_<short>`).
6. **OBLIGATORIO** que cada decisión de stack/arquitectura tenga un ADR en [`specs/adr/`](specs/adr/) **antes** de implementarse.
7. **OBLIGATORIO** que cualquier agente (humano o IA) que proponga un cambio lea primero [`specs/README.md`](specs/README.md) + las specs afectadas, y mencione explícitamente qué ACs cubre.

### 17.2 Estado de las specs

Ver [`specs/STATUS.md`](specs/STATUS.md). Transiciones:

```
draft → review → approved → implemented → deprecated
```

Una spec en `draft` **puede** codearse, pero el código **no se mergea** hasta que la spec esté `approved`.

### 17.3 Navegación rápida

- Índice maestro: [`specs/README.md`](specs/README.md)
- Roadmap de features: [`specs/features/ROADMAP.md`](specs/features/ROADMAP.md)
- Índice de ADRs: [`specs/adr/README.md`](specs/adr/README.md)
- Status board: [`specs/STATUS.md`](specs/STATUS.md)
- Plantillas: [`specs/templates/`](specs/templates/)

### 17.4 Violaciones

Si un PR viola las reglas 1-4 de §17.1:

- Se rechaza en review con la plantilla:
  > "Bloqueado por AGENTS.md §17. Actualiza/crea la spec correspondiente antes de mergear."
- No se discute en el PR; se discute en la spec.

### 17.5 Disciplina de status: STATUS.md se actualiza en el mismo PR (OBLIGATORIO)

> **Esta regla existe porque ya fallamos aquí.** F02 fue mergeado
> en PRs #5 y #6 mientras el spec seguía en `review`, y nadie
> actualizó `specs/STATUS.md` para reflejar la implementación del
> backend. La spec quedó desincronizada del código durante semanas.
>
> Regla dura, sin excepciones fuera de hotfixes blocker (§18.4).

#### 17.5.1 Lo que se actualiza atómicamente con cada PR

Cualquier PR que toque código cubierto por una spec **debe**, en
el **mismo PR** (mismo commit, no follow-up), actualizar **todos**
estos archivos cuando aplique:

1. **`specs/STATUS.md`** — mover la spec a su nueva sección si
   cambió de estado (`review` → `approved`, `approved` →
   `implemented`, etc.). Actualizar la fecha "Última actualización"
   del board.
2. **La spec afectada** (`specs/features/F<NN>-<slug>.md` o
   `specs/domains/<x>.md`):
   - Cambiar el `Status` en la cabecera.
   - Marcar ACs cubiertos con `[x]`; dejar `[ ]` los pendientes.
   - Añadir o actualizar una sección `## Implementation status`
     con snapshot de cobertura (backend / IPC / UI, % ACs).
   - Añadir entradas en `## Discovered bugs (post-approval)` si
     el PR descubrió gaps (categoría A) o se desvió de la spec
     (categoría B) — ver §18.
3. **El template del PR** (`.github/PULL_REQUEST_TEMPLATE.md`):
   la sección `## Spec status changes` debe listar cada spec
   tocada y el nuevo estado.

#### 17.5.2 Cuándo aplica

| Tipo de cambio en el PR | STATUS.md | Spec afectada |
|---|---|---|
| Promueve spec `draft` → `review` / `approved` | ✅ mover de sección | ✅ cambiar Status + marcar ACs |
| Implementa ACs (código mergeado + tests pasando) | ✅ mover a ✅ Implemented (o nota de parcial) | ✅ marcar ACs con `[x]` + Implementation status |
| Cambia un dominio / IPC | ✅ añadir nota en la sección correspondiente | ✅ actualizar Status / Edge cases / ACs |
| Spec-wrong (categoría A) → revocar a `review` | ✅ mover a Review | ✅ cambiar Status + sección Discovered bugs |
| Deprecated una spec | ✅ mover a Deprecated | ✅ cambiar Status + link al ADR de deprecation |
| PR solo de código (no toca specs) | ❌ no aplica | ❌ no aplica |

#### 17.5.3 Cómo verificarlo

- **Pre-merge (checklist §15)**: el PR no se aprueba si el diff
  de `specs/` no refleja la nueva realidad cuando hay código
  tocado que cae bajo una spec.
- **CI** (futuro, v0.1.x): un job de CI parsea la sección
  `## Spec status changes` del cuerpo del PR y comprueba que
  los `Refs:` apunten a specs que existen y cuyo status en
  `STATUS.md` es coherente con la sección `## Implementation
  status` del spec (si dice "8/18 ACs", STATUS.md debe tener
  la spec en `Approved` con nota, no en `Implemented`).
- **Post-merge**: el autor actualiza el board en el mismo commit.
  **Nunca** se abre un PR "fix(status): sync STATUS.md" — eso
  es síntoma de que se saltó la regla.

#### 17.5.4 Ejemplos

**Bien** (un solo PR cubre todo):

```
feat(core,app): implement F02 backend (PR #5)
├── código: WorkspaceService + 9 Tauri commands
├── tests: 34 en core + 18 en app
├── specs/STATUS.md: F02 movido a ✅ Implemented con nota de parcial
├── specs/features/F02-multi-workspace.md: Status: implemented,
│   8 ACs marcados [x], sección Implementation status añadida
└── PR template: "## Spec status changes" lista F02 → implemented
```

**Mal** (lo que NO se debe hacer):

```
feat(core,app): implement F02 backend (PR #5)     ← código
feat(specs): F02 → implemented                    ← follow-up semanas después
chore(status): sync STATUS.md                     ← parcheo manual
```

#### 17.5.5 Recuperación de drift (caso F02)

Si se detecta que una spec quedó desincronizada del código (como
pasó con F02), el PR correctivo debe:

1. Recalificar la spec a su estado real (`approved` si la spec
   es sólida y el código cumple ACs; `review` si la spec no
   cubre el código actual; `draft` si hay que reescribir).
2. Añadir una entrada en `## Discovered bugs` con categoría
   "A. Spec gap (proceso)" y la causa raíz.
3. Endurecer las reglas si el gap es de proceso (como este PR).

---

## 18. Gestión de bugs

Todo bug se reporta como **issue en GitHub** (nunca solo como conversación) y se cierra siempre vía PR que referencia tanto el issue como la spec afectada.

### 18.1 Categorías

Solo dos categorías (se simplificaron en sesión de planning):

| Categoría | Significado | Acción |
|---|---|---|
| **A. Spec gap** | El código respeta la spec, pero la spec no cubría el caso (incluye edge cases no anticipados y spec wrong — spec aprobada con un comportamiento incorrecto). | Actualizar spec (añadir/cambiar AC) + añadir test que cubre el AC + fix código. **Un solo PR**. |
| **B. Implementation bug** | El código se desvió de la spec. | Fix código + añadir test de regresión cuyo nombre derive del AC. PR pequeño, no toca la spec. |

### 18.2 Plantilla de issue

```markdown
## Bug report

**ID**: BUG-<NN>
**Title**: <resumen>
**Severity**: blocker | major | minor | cosmetic
**Affected specs**: <lista de paths a specs>
**Category**: A. Spec gap | B. Implementation bug

### Reproduction
<pasos mínimos>

### Expected (según spec)
<cita del AC o de la sección de la spec>

### Actual
<lo que pasa>

### Root cause hypothesis
<opcional>

### Proposed resolution
- [ ] Spec change: <path> §<sección>
- [ ] Test: <nombre del test>
- [ ] Code fix: <archivo>
```

### 18.3 Reglas

1. **Ningún bug sin `Affected specs`**. Si no hay spec afectada:
   - Crear la spec (caso A).
   - Si el bug es trivial y aislado, **se permite** abrir issue sin spec referenciada, pero el PR de fix debe añadir al menos un test de regresión con nombre `bug_<NN>_*`.
2. **El PR del fix cierra el issue** con `Closes #NN` y referencia `specs/...` igual que cualquier feature.
3. **Spec-gaps y edge cases se acumulan** en una sección `## Discovered bugs (post-approval)` al final de cada spec afectada, con id, fecha, categoría y versión de resolución. Esto mantiene la spec sincronizada con la realidad.
4. **Spec-wrong** (categoría A): revocar `approved` → `review`, corregir la spec, volver a `approved`, luego fix código + test. **Un solo PR** si la corrección es pequeña; si no, dos PRs (spec primero, código después).

### 18.4 Hotfixes (emergencias)

Para v0.x: si un bug es `blocker` y la spec está en `draft`, se permite un **hotfix** que:

- Crea un issue con categoría A.
- Añade test de regresión.
- Fixa.
- PR marcado con etiqueta `hotfix`.
- La spec se actualiza en un PR separado en **≤ 24 h** después.

Esto evita que specs `draft` bloqueen emergencias.

### 18.5 Flujo

```
bug encontrado
   │
   ├─► ¿La spec lo cubre?
   │     │
   │     ├─ NO  → Categoría A
   │     │         → actualizar spec (nuevo/cambiado AC)
   │     │         → añadir test que cubre el AC
   │     │         → fix código
   │     │         → un solo PR
   │     │
   │     └─ SÍ  → ¿el código respeta la spec?
   │                 │
   │                 ├─ NO  → Categoría B
   │                 │         → fix código
   │                 │         → añadir test de regresión
   │                 │         → un solo PR
   │                 │
   │                 └─ SÍ  → Categoría A (raro: spec wrong)
   │                           → spec a `review`
   │                           → corregir spec
   │                           → spec a `approved`
   │                           → fix código + test
   │                           → un solo PR (o dos si la corrección es grande)
   │
   └─► siempre: cerrar issue, apuntar en spec#discovered-bugs
```

---

> **Última actualización**: Bloque 1 de Spec-Driven Development aplicado (specs/ + ADRs 0001-0006 + §17/§18 en este AGENTS.md). Cualquier desviación de este documento requiere PR con justificación.
