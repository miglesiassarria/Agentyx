# AGENTS.md

> Convenciones, reglas y arquitectura de **Agentyx** — escritorio, rápido, ligero, agentic.
> Este documento es la fuente de verdad para cualquier agente (humano o IA) que trabaje en el repo.

---

## 1. Stack tecnológico

### 1.1 Shell y core
- **Tauri 2** (Rust) — desktop shell multiplataforma.
- **Rust** (edition 2021, MSRV `1.80+`).
- **Tokio** como runtime async.
- **axum** para el servidor HTTP/SSE embebido que sirve la UI por
  navegador en loopback/LAN desde el MVP.
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
11. **Desktop + web LAN en el MVP** — la app se usa desde Tauri y desde
    navegador en la LAN mediante el servidor embebido. `127.0.0.1`
    es el bind por defecto; `0.0.0.0` requiere auth bearer obligatoria.

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
- PRs abiertas por un agente: si el CI pasa completo y no hay bloqueo
  humano explícito, el mismo agente debe cerrar el ciclo fusionando la
  PR (`gh pr merge`) y verificando que `main` queda actualizado. No
  dejar PRs verdes abiertas salvo que el usuario pida revisión humana.
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
  api_key = "keychain:groq"           # referencio al keychain OS
  base_url = "https://api.groq.com/openai/v1"

  [providers.minimax]
  api_key = "keychain:minimax"        # también soportado
  base_url = "https://api.minimax.io/v1"

  default_provider = "ollama"
  default_model = "llama3.1:8b"
  ```
- **Nunca** guardar API keys en texto plano en el repo. Opciones:
  - `keychain:<account>` — referencio al keychain del SO (recomendado, ya implementado)
  - `env:VAR_NAME` — variable de entorno
- La app resuelve `keychain:...` y `env:...` al cargar y cachea en memoria. No loguea keys.
- El keychain usa servicio `"agentyx"` (macOS Keychain, Windows Credential Manager, Linux Secret Service).

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
- [ ] **Spec sync (regla §17.5)**: si el PR toca comportamiento
      cubierto por pitch/spec, el diff incluye:
      1. Pitch/spec afectado actualizado solo cuando cambian
         alcance, contratos, ACs o estado.
      2. `specs/STATUS.md` actualizado solo si cambia el status
         de una spec/pitch o el board queda objetivamente obsoleto.
      3. Sección `## Spec status changes` del cuerpo del PR con
         cada pitch/spec tocado, o `N/A` con motivo.
- [ ] **PR lifecycle**: si el CI del PR está en verde y no hay bloqueo
      humano explícito, el agente que abrió la PR debe fusionarla y
      confirmar después que el estado del proyecto refleja el resultado
      real post-merge.

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

## 17. Pitch-Driven SDD Lite (OBLIGATORIO)

Este proyecto sigue **Pitch-Driven SDD Lite**: suficiente diseño antes
de implementar, pero con documentos cortos y lectura selectiva para no
quemar contexto. La fuente de verdad vive en [`specs/`](specs/), junto
al código, pero no todo cambio necesita una spec larga.

### 17.1 Cuándo hace falta pitch/spec

Un PR **debe** referenciar o actualizar un pitch/spec cuando toca al
menos una de estas superficies:

1. Comportamiento visible de usuario o una feature vertical.
2. API de Tauri command, endpoint HTTP, evento streaming o error code.
3. Persistencia, migraciones, journal, permisos, sandbox, providers,
   agentes, PTY o ejecución de tools.
4. Seguridad, secretos, paths, red o capabilities Tauri.
5. Decisiones de stack/arquitectura difíciles de revertir.

No hace falta pitch/spec nuevo para refactors internos, cambios de
estilo, renombres locales, tests que cubren comportamiento existente,
docs operativas o fixes pequeños que no cambian contrato ni UX. En esos
casos el PR usa `Refs: N/A — <motivo>`.

### 17.1.1 Reglas duras

1. **PROHIBIDO** implementar una feature nueva si no existe un pitch en
   `specs/features/F<NN>-<slug>.md` con `Status` `ready` o `approved`
   y acceptance criteria definidos.
2. **PROHIBIDO** cambiar la API de un Tauri command, endpoint HTTP,
   evento streaming o error code sin actualizar [`specs/ipc.md`](specs/ipc.md)
   y el pitch/spec afectado en el mismo PR.
3. **OBLIGATORIO** que cada AC implementado tenga al menos un test cuyo
   nombre derive del AC cuando el AC sea automatizable. Si no lo es, el
   pitch debe indicar la verificación manual esperada en `Test Map`.
4. **OBLIGATORIO** que cada decisión nueva de stack/arquitectura difícil
   de revertir tenga un ADR en [`specs/adr/`](specs/adr/) antes de
   implementarse. No crear ADRs para detalles reversibles.
5. **OBLIGATORIO** que cualquier agente (humano o IA) lea primero
   [`specs/README.md`](specs/README.md) y después solo los pitches/specs
   directamente afectados. Debe mencionar qué ACs cubre o explicar
   `Refs: N/A`.

### 17.1.2 Forma estándar de un pitch

Las features nuevas usan la plantilla ligera
[`specs/templates/feature-spec.md`](specs/templates/feature-spec.md).
Objetivo: 120-180 líneas, salvo features excepcionalmente grandes.
Debe contener: `Problem`, `Appetite`, `Solution Shape`, `Contracts`,
`Acceptance Criteria`, `No-gos`, `Risks / Rabbit holes` y `Test Map`.

Las specs largas existentes siguen siendo válidas. Cuando se toquen,
preferir añadir o actualizar un bloque corto `## Agent context` arriba
antes que expandir narrativa histórica.

### 17.2 Estado de las specs

Ver [`specs/STATUS.md`](specs/STATUS.md). Transiciones preferidas:

```
proposed → ready → shipped → deprecated
```

Los estados históricos `draft`, `review`, `approved` e `implemented`
siguen aceptados para specs antiguas. Para trabajo nuevo, usar
`proposed` mientras se perfila, `ready` cuando el pitch ya tiene ACs y
contratos suficientes, y `shipped` cuando el código y tests están
mergeados.

### 17.3 Navegación rápida

- Índice maestro: [`specs/README.md`](specs/README.md)
- Roadmap de features: [`specs/features/ROADMAP.md`](specs/features/ROADMAP.md)
- Índice de ADRs: [`specs/adr/README.md`](specs/adr/README.md)
- Status board: [`specs/STATUS.md`](specs/STATUS.md)
- Plantillas: [`specs/templates/`](specs/templates/)

### 17.4 Violaciones

Si un PR viola las reglas duras de §17.1.1:

- Se rechaza en review con la plantilla:
  > "Bloqueado por AGENTS.md §17. Actualiza/crea el pitch/spec correspondiente antes de mergear."
- No se discute en el PR; se discute en el pitch/spec.

### 17.5 Disciplina de status ligera (OBLIGATORIO)

> **Esta regla existe porque ya fallamos aquí.** F02 fue mergeado
> en PRs #5 y #6 mientras el spec seguía en `review`, y nadie
> actualizó `specs/STATUS.md` para reflejar la implementación del
> backend. La spec quedó desincronizada del código durante semanas.
>
La corrección se mantiene: el status no puede mentir. Lo que cambia es
el alcance: no se duplica implementación en varios documentos si no
cambió el estado real.

#### 17.5.1 Lo que se actualiza atómicamente con cada PR

Cualquier PR que toque código cubierto por pitch/spec **debe**, en el
mismo PR, actualizar estos archivos **solo cuando aplique**:

1. **`specs/STATUS.md`** — mover la spec/pitch si cambió de estado
   (`proposed` → `ready`, `ready` → `shipped`, etc.) o si el board
   queda obsoleto. Actualizar la fecha del board en ese caso.
2. **El pitch/spec afectado** (`specs/features/F<NN>-<slug>.md` o
   `specs/domains/<x>.md`) si cambian alcance, contratos, ACs o estado:
   - Cambiar el `Status` en la cabecera.
   - Marcar ACs cubiertos con `[x]`; dejar `[ ]` los pendientes.
   - Mantener `## Test Map` coherente con los tests/verificación.
   - Añadir `## Implementation notes` solo si hay información útil para
     futuros cambios; máximo 5 bullets.
   - Añadir entradas en `## Discovered bugs (post-approval)` si
     el PR descubrió gaps (categoría A) o se desvió de la spec
     (categoría B) — ver §18.
3. **El cuerpo del PR**: `## Refs` lista specs/ACs tocados o `N/A`;
   `## Spec status changes` lista cambios de estado o `N/A`.

#### 17.5.2 Cuándo aplica

| Tipo de cambio en el PR | STATUS.md | Pitch/spec afectado |
|---|---|---|
| Promueve `proposed` → `ready` | ✅ mover de sección | ✅ cambiar Status + ACs completos |
| Implementa todos los ACs de un pitch | ✅ mover a `shipped` | ✅ marcar ACs + Test Map |
| Implementa parte de un pitch sin cambiar status | ❌ salvo board obsoleto | ✅ marcar ACs si aplica |
| Cambia IPC/contratos | ✅ solo si cambia status/board | ✅ actualizar `Contracts` + `specs/ipc.md` |
| Spec-wrong (categoría A) vuelve a diseño | ✅ mover a `proposed`/`review` | ✅ cambiar Status + Discovered bugs |
| Deprecated una spec | ✅ mover a Deprecated | ✅ cambiar Status + link al ADR si aplica |
| PR sin cambio cubierto por SDD | ❌ no aplica | ❌ no aplica |

#### 17.5.3 Cómo verificarlo

- **Pre-merge (checklist §15)**: el PR no se aprueba si el diff
  de `specs/` no refleja cambios reales de alcance, contratos, ACs o
  status cuando el código cae bajo pitch/spec.
- **CI** (futuro, v0.1.x): un job de CI parsea la sección
  `## Spec status changes` del cuerpo del PR y comprueba que
  los `Refs:` apunten a specs que existen cuando no son `N/A`.
- **Post-merge**: el board debe quedar coherente con el estado real.
  No abrir PRs posteriores solo para sincronizar status salvo drift
  heredado que se esté corrigiendo explícitamente.

#### 17.5.4 Ejemplos

**Bien** (un PR pequeño con pitch ligero):

```
feat(ui): add provider health badge
├── código: componente + ipc wrapper
├── tests: f05_ac3_provider_health_badge
├── specs/features/F05-settings.md: AC3 marcado [x], Test Map actualizado
└── PR: Refs F05.AC3; Spec status changes: N/A — status sigue ready
```

**Mal** (lo que NO se debe hacer):

```
feat(app): change chat event payload              ← cambia contrato
Refs: N/A                                         ← incorrecto
specs/ipc.md sin actualizar                       ← incorrecto
```

#### 17.5.5 Recuperación de drift (caso F02)

Si se detecta que una spec quedó desincronizada del código (como
pasó con F02), el PR correctivo debe:

1. Recalificar la spec a su estado real (`shipped`/`implemented` si el
   código cumple ACs; `ready`/`approved` si el diseño es sólido pero
   falta código; `proposed`/`review` si no cubre el código actual).
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
4. **Spec-wrong** (categoría A): volver el pitch/spec a diseño
   (`ready` → `proposed`, o `approved` → `review` en specs históricas),
   corregir AC/contrato, luego fix código + test. **Un solo PR** si la
   corrección es pequeña; si no, dos PRs (diseño primero, código después).

### 18.4 Hotfixes (emergencias)

Para v0.x: si un bug es `blocker` y el pitch/spec está en `proposed`
o `draft`, se permite un **hotfix** que:

- Crea un issue con categoría A.
- Añade test de regresión.
- Fixa.
- PR marcado con etiqueta `hotfix`.
- La spec se actualiza en un PR separado en **≤ 24 h** después.

Esto evita que pitches/specs en diseño bloqueen emergencias.

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
   │                           → pitch/spec a `proposed` o `review`
   │                           → corregir AC/contrato
   │                           → pitch/spec a `ready` o `approved`
   │                           → fix código + test
   │                           → un solo PR (o dos si la corrección es grande)
   │
   └─► siempre: cerrar issue, apuntar en spec#discovered-bugs
```

---

> **Última actualización**: Pitch-Driven SDD Lite habilitado. Cualquier desviación de este documento requiere PR con justificación.
