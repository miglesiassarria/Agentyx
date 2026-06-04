# ADR-0001 — Tauri 2 vs Electron como shell de escritorio

**Status**: accepted
**Date**: 2026-06-04
**Deciders**: @miglesias

## Context

Necesitamos un shell de escritorio multiplataforma (macOS, Windows, Linux)
para una aplicación que debe ser **ligera y rápida**. Los dos contendientes
razonables en 2026 son:

- **Electron** (usado por opencode-dev, Codex App, Cursor, VSCode fork).
- **Tauri 2** (usado por Agentyx objetivo).

Cargas que tendrá que soportar el shell:
- Webview para la UI (Svelte 5).
- IPC con la lógica de negocio (Rust puro en `agentyx-core`).
- Spawn de subprocesses (`python`, `uv`, `git`, …) y PTYs.
- File watcher (`notify`).
- HTTP server embebido (axum) para acceso desde navegador/LAN.
- Auto-updater (futuro).

## Decision

**Adoptamos Tauri 2** como shell de escritorio.

## Status

`accepted`. Cualquier migración a Electron u otro shell requiere un ADR
nuevo que lo superseda.

## Consequences

### Positivas
- **Binario instalado ~10× más pequeño**: Tauri usa el webview del SO
  (WebKit/WKWebView, WebView2, WebKitGTK) en vez de embebir Chromium.
  Objetivo: < 20 MB instalados (Electron suele > 150 MB).
- **Arranque más rápido**: < 500 ms objetivo (Electron típico ~1.5-2 s).
- **Consumo de RAM menor**: objetivo < 80 MB en reposo (Electron típico
  ~250-400 MB).
- **Alineado con el requisito de "ligero y rápido"** del proyecto.

### Negativas
- **Ecosistema de plugins Tauri más pequeño** que Electron, pero los
  necesarios (`shell`, `fs`, `dialog`, `updater`, `deep-link`, `os`,
  `log`) están todos en `tauri-plugin-*` oficiales.
- **Capabilities por ventana** añaden complejidad de configuración
  vs. el modelo "todo permitido" de Electron. **Es una ventaja** desde
  el punto de vista de seguridad.
- **El webview del SO no es Chromium**: diferencias de comportamiento
  (CSS, APIs modernas) requieren tests específicos por plataforma.
- **Sin `withGlobalTauri`**: la UI **no** puede usar `window.__TAURI__`
  directo. Solo vía `lib/ipc.ts` con `invoke`/`listen` explícitos.

### Neutras
- Las **APIs de Tauri 2 son estables y la documentación es buena**.
- Necesitamos aprender el modelo de **capabilities + permissions**.
- El tooling (`tauri-cli`) es `cargo install`-able; no hay un daemon
  oculto.

## Alternatives considered

### Alternative A: Electron + TypeScript
- Pros: ecosistema enorme, mismo lenguaje que opencode-dev (referencia
  arquitectónica), debugger de Chrome funciona out-of-the-box.
- Cons: binario > 150 MB, RAM alta, contradice el requisito "ligero".
- **Por qué se descartó**: contradice el goal explícito de
  `project.md` ("Binario final < 20 MB instalado, arranque < 500 ms,
  RAM < 80 MB en reposo").

### Alternative B: WebView2 directo en Windows + WKWebView en macOS + WebKitGTK en Linux (sin Tauri)
- Pros: aún más ligero (sin runtime Tauri).
- Cons: reinventamos IPC, capabilities, updater, deep links, menús,
  window state. Coste de mantenimiento enorme.
- **Por qué se descartó**: Tauri ya nos da todo eso multiplataforma.
  No hay razón para reinventarlo.

### Alternative C: Slint / Iced / egui (UI 100% nativa, sin webview)
- Pros: binario más pequeño todavía, look nativo real.
- Cons: para renderizar PDF, DOCX, dashboards y un diff visual
  decente, el coste de implementación en Rust nativo es **enorme**.
  Web ecosystem tiene todo eso resuelto (PDF.js, mammoth, CodeMirror).
- **Por qué se descartó**: los requisitos del UI (diffs, PDF, Word,
  artefactos web, dashboards) hacen inviable UI nativa pura.

### Alternative D: Flutter
- Pros: cross-platform consistente.
- Cons: bundle grande, no encaja con "ligero y rápido", usar Skia
  cuando ya hay webview del SO disponible.
- **Por qué se descartó**: misma razón que Electron (peso, objetivo
  < 20 MB).

## References

- [project.md](../project.md) — goals y non-goals.
- [architecture.md](../architecture.md) — diagrama de procesos.
- Discusión previa en sesión de planning (este repo).
- Web: <https://tauri.app/> (oficial Tauri 2).
- Comparativas de tamaño: Tauri ~3-15 MB vs Electron ~150-200 MB en
  apps comparables.
