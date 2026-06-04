# ADR-0002 — Rust core + Svelte 5 UI

**Status**: accepted
**Date**: 2026-06-04
**Deciders**: @miglesias

## Context

El proyecto necesita separar la **lógica de negocio** de la **presentación**.
Las opciones sobre la mesa para el frontend eran:

- **Svelte 5** (runes), compilado a JS mínimo.
- **HTML + CSS + JS vanilla + Alpine.js** (sin build step).
- **Vue 3** con `<script setup>`.
- **Leptos / Yew** (Rust → WASM, sin JS en la UI).
- **Preact** (alternativa ligera a React).

Y para el backend/core: **Rust** (decidido en [0001-tauri-vs-electron.md](0001-tauri-vs-electron.md)).

## Decision

- **Core**: Rust puro en `crates/agentyx-core/`, **sin Tauri**.
- **UI**: **Svelte 5** (runes) + Vite + TypeScript estricto.

`Svelte 5` se elige sobre las alternativas por:
- Compila a JS mínimo (mejor que Vue 3 y Preact para peso de bundle).
- Runes (`$state`, `$derived`, `$effect`) son explícitos y trazables.
- Ecosistema maduro de componentes (CodeMirror bindings, etc.).
- DX excelente con Vite + HMR.

## Status

`accepted`.

## Consequences

### Positivas
- **Bundle de UI mínimo**: Svelte 5 runtime ~3-5 KB, mejor que cualquier
  framework con virtual DOM.
- **Tipado end-to-end en TS estricto**: `noUncheckedIndexedAccess`,
  sin `any` (ver `AGENTS.md` §4.2).
- **Runes explícitos** ⇒ estado trazable, sin magia reactiva oculta.
- **HMR rápido** con Vite → iteración veloz en la UI.

### Negativas
- **Svelte tiene una capa de magia de compilación** que a veces dificulta
  el debug (vs. JS plano). Pero runes en Svelte 5 son más explícitas que
  la reactividad de Svelte 3/4.
- **Hay que mantener dos lenguajes** (Rust + TS). El IPC los acota y
  el `lib/ipc.ts` abstrae el transporte (ver [ipc.md](../ipc.md) §6).
- **TypeScript strict discipline**: el equipo (humano o IA) debe
  resistir la tentación de `any` en bordes de FFI.

### Neutras
- La elección de **HTML/JS vanilla + Alpine** quedó como **plan B**:
  si en algún momento la complejidad de Svelte no compensa, podemos
  rehacer la UI en plano con `lib/ipc.ts` igual.
- Necesitamos disciplina: el código de UI **nunca** toca Node/Electron
  APIs, solo `lib/ipc.ts`.

## Alternatives considered

### Alternative A: HTML + CSS + JS vanilla + Alpine.js
- Pros: cero build step, máxima simplicidad, ~15 KB de runtime.
- Cons: para diffs visuales, dashboards, vista de PDF/DOCX, la cantidad
  de código "pegamento" en vanilla JS crece.
- **Por qué se descartó (para v1)**: Svelte nos da HMR, componentes
  reutilizables y type-safety sin pagar bundle enorme. Se mantiene
  como plan B.

### Alternative B: Vue 3 (`<script setup>`)
- Pros: similar DX a Svelte, ecosistema grande.
- Cons: runtime mayor (~30 KB), virtual DOM añade overhead.
- **Por qué se descartó**: Svelte 5 produce bundles más pequeños con
  la misma DX.

### Alternative C: Leptos / Yew (Rust → WASM, sin JS)
- Pros: cero JS en la UI, todo en Rust.
- Cons: integrarse con librerías web (PDF.js, mammoth, CodeMirror)
  es más costoso; ecosistema UI más limitado.
- **Por qué se descartó**: los requisitos del UI (diffs, PDF, Word,
  dashboards) se montan trivialmente con librerías web; rehacerlas en
  Rust no aporta.

### Alternative D: Preact
- Pros: ~3 KB, virtual DOM.
- Cons: virtual DOM innecesario para nuestro caso; comunidad más
  pequeña que Svelte.
- **Por qué se descartó**: Svelte compila a JS imperativo, más
  eficiente que virtual DOM para nuestro perfil de uso.

## References

- [project.md](../project.md)
- [architecture.md](../architecture.md)
- [ipc.md](../ipc.md) — abstracción del transporte.
- Web: <https://svelte.dev/docs/svelte/overview> (Svelte 5 runes).
