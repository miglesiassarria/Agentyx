# ADR-0010 — Política de caching de CI: rust-cache + sccache como steps directos

**Status**: accepted
**Date**: 2026-06-14
**Deciders**: @miglesias

## Context

A 2026-06-13, el `cargo test` del CI de Agentyx tardaba **~15 minutos
wall-clock** por PR. La causa raíz no era la ejecución de los tests
(los 469 tests corren en ~1.2s con `cargo nextest run`) sino la
**recompilación** de las 673 dependencias de `Cargo.lock` en cada run
(matrix 3-OS: Ubuntu, macOS, Windows).

Desglose del run baseline (#27462625580, 9:14–9:29 UTC):

| Job | Duración | Por qué |
|---|---|---|
| `cargo test (windows-latest)` | 14m 42s | **Sin caché** (`if: runner.os != 'Windows'`) |
| `cargo test (ubuntu-latest)` | 8m 22s | `actions/cache@v4` con key solo de `Cargo.lock` |
| `cargo test (macos-latest)` | 1m 06s | Caché caliente, smoking gun |
| `clippy` | 6m 03s | Compilaba `--all-targets` desde cero |
| `cargo deny` | 2m 09s | `cargo install cargo-deny` desde source cada run |
| `cargo audit` | 3m 00s | `cargo install cargo-audit` desde source cada run |

Tres problemas concretos a resolver:

1. **Caché manual con key solo de `Cargo.lock`**: no invalida
   correctamente con `--all-features`, causa race conditions entre
   jobs paralelos del matrix 3-OS, y no maneja bien cambios en la
   feature matrix.
2. **Windows sin caché**: un `if: runner.os != 'Windows'` huérfano
   forzaba recompilación completa en cada run.
3. **Sin caché de compilador distribuido**: cada CI run recompilaba
   lo que no estuviera en `target/`, sin reuso de artefactos objeto
   entre PRs.

## Decision

Adoptamos la combinación estándar de la comunidad Rust para CI en
GitHub Actions:

1. **`Swatinem/rust-cache@v2`** reemplaza el `actions/cache@v4`
   casero. Calcula un fingerprint content-aware (lockfile + rustc
   version + features + target triple), almacena `target/` y el
   registry de cargo en un cache backend con
   `cache-on-failure: true` para no perder el cache cuando un job
   falla a mitad de build.

2. **`mozilla-actions/sccache-action@v0.0.7`** configura
   `RUSTC_WRAPPER=sccache` en los 4 jobs de Rust (clippy, test,
   deny, audit). sccache cachea artefactos objeto entre runs usando
   el backend del cache de GitHub Actions (sin S3, sin credenciales
   externas). En el primer run de la PR #37, sccache tuvo 0 hits
   (esperable, el cache distribuido está vacío); a partir del
   segundo run la hit rate sube a >50% y baja los tiempos cold de
   compilación a segundos.

3. **`actions/cache@v4`** para `~/.cargo/bin/cargo-deny` y
   `~/.cargo/bin/cargo-audit` con key dependiente del hash del
   workflow, evitando `cargo install --locked` desde source en cada
   run.

4. **Eliminamos el `if: runner.os != 'Windows'`** del step de caché
   de `rust-test`. Windows, macOS y Linux comparten ahora la misma
   estrategia de cache.

5. **`cargo nextest run` queda descartado** en CI tras el intento
   de la PR #38 cerrada sin mergear (ver
   [Alternativa rechazada A](#alternativa-rechazada-a-composite-action--cargo-nextest)).
   `cargo test --workspace` se mantiene como ejecutor de tests.

## Status

`accepted`. El estado actual de `ci.yml` (PR #37 mergeada) refleja
esta decisión. Cambios futuros requieren un ADR nuevo que superseda
este.

## Consequences

### Positivas

- **Wall-clock del PR bajó de ~15 min a ~2 min 30s** (medido en
  PR #37 run #27465321638): **mejora de ~6×**.
- **Windows**: 14m 42s → 2m 33s. El cambio más impactante porque
  antes no tenía cache en absoluto.
- **Linux**: 8m 22s → 1m 40s. Combinación de rust-cache hit + sccache hit.
- **macOS**: 1m 06s → 0m 50s. Marginal (ya estaba caliente).
- **clippy**: 6m 03s → 1m 17s. Reutiliza el `target/` que dejó
  rust-test si los jobs se solapan en el matrix.
- **cargo deny/audit**: 2-3 min → 0m 20s. Cache de binarios.
- **Coste mensual estimado** (100 PRs + 200 pushes/mes): ~35 h/mes
  → ~13 h/mes de CI-minutes. **~60% de ahorro**.

### Negativas

- **sccache con backend de GitHub Actions añade ~10-20s** por
  restore (mete los artefactos objeto en el cache de Actions). Es
  marginal pero existe.
- **Race condition posible** entre jobs paralelos del matrix
  cuando dos OSes intentan guardar la misma key
  (ej. `cargo-nextest` cache). Se mitiga con
  `cache-on-failure: true` en rust-cache; el primer job que termina
  gana y los demás hacen el install desde cero (especie de fallback).
  El log de la PR #37 muestra un `Failed to save: Unable to reserve
  cache` no fatal.
- **Dependencia de 2 acciones externas** (`Swatinem/rust-cache`,
  `mozilla-actions/sccache-action`). Si alguna deja de mantenerse,
  hay que migrar. Ambas son proyectos activos con miles de stars.
- **Windows + sccache + MSVC**: no se ha observado problema en
  producción (PR #37, 3 OSes verdes), pero mantener
  `if: runner.os != 'Windows'` en sccache como escape sigue siendo
  una opción si surge un issue con `link.exe` o `build.rs` que
  invoque `cl.exe` directamente.

### Neutras

- **`cargo test` se mantiene** como ejecutor de tests, no
  migramos a `cargo nextest` (ver alternativa rechazada A).
- **El composite action `setup-rust-ci` no se introduce**. Cada
  job de Rust sigue siendo plano en `ci.yml`. Trade-off: el
  workflow es ~50 líneas más largo que con composite action, pero
  los steps directos son más fáciles de debuggear y no rompen el
  env de sccache.
- **El comentario sobre `FORCE_JAVASCRIPT_ACTIONS_TO_NODE24`**
  sigue mencionando `actions/cache@v4` y `dtolnay/rust-toolchain`
  porque ambas se siguen usando (la primera para el cache de
  binarios cargo, la segunda para toolchain). No es obsoleto.

## Alternatives considered

### Alternative A: Mantener `actions/cache@v4` casero

- Pros: cero dependencias externas nuevas; código simple y
  entendible por cualquiera que conozca Actions.
- Cons: el key solo de `Cargo.lock` no captura cambios en
  `--all-features`, hay race conditions entre los 3 OSes del matrix
  (todos con la misma key), y no aprovecha la integración nativa
  con el runner de cargo.
- **Por qué se descartó**: el wall-clock medido (15 min) confirma
  que el approach casero no escala. El baseline muestra el
  problema: macOS con cache caliente tarda 1m, Linux sin
  fingerprinting adecuado tarda 8m, Windows sin cache 14m.

### Alternative B: Solo `rust-cache`, sin `sccache`

- Pros: una acción menos; menos complejidad; menos puntos de fallo.
- Cons: rust-cache cachea `target/` y registry, pero **no cachea
  artefactos objeto entre PRs distintas**. Cuando cambia el código
  fuente, rust-cache detecta el cambio e invalida el `target/`,
  forzando recompilación completa de las crates afectadas. sccache
  cachea por contenido del `.rs` recompilando solo lo que cambió
  (incremental entre PRs).
- **Por qué se descartó**: el `sccache --show-stats` de la PR #37
  muestra que sccache sí aporta hits en runs sucesivos, lo que
  confirma el valor del segundo nivel de cache.

### Alternative C: Imagen pre-baked con todo pre-instalado

- Pros: setup time = 0. El job empieza con toolchain + sccache +
  cargo-nextest + apt deps de Tauri ya instalados. Techo teórico:
  ~30-45s de wall-clock.
- Cons: mantenimiento de la imagen (rebuilds cuando cambian versiones
  de Rust, sccache, etc.), imágenes privadas de Docker Hub o GHCR
  con costes de almacenamiento, complejidad añadida al release flow.
- **Por qué se descarta (de momento)**: el ROI no está claro
  todavía. Cuando lleguemos a 5-10 min de wall-clock con el
  approach actual, reevaluar. Hoy estamos en 2m 30s, no es
  urgente.

### Alternativa rechazada A: composite action `setup-rust-ci` + `cargo nextest`

Probado en PR #38 (`chore/ci-setup-action-nextest`), cerrada sin
mergear tras detectar regresión:

- **sccache con 0% hit rate dentro de la composite action**. El
  log muestra `Cache location: Local disk: "/home/runner/.cache/sccache"`
  en vez del backend distribuido. Cuando `mozilla-actions/sccache-action`
  corre como step directo, el `GITHUB_TOKEN` y el env de GHA
  Actions se inyectan correctamente; dentro de la composite
  action el env no se propaga, sccache cae a local disk y se
  pierde al final del job.
- **`cargo install --locked cargo-nextest` desde cero tardó 3m 41s**
  en el primer run. En runs posteriores el cache de cargo-nextest
  ayudaría, pero con sccache caído la compilación seguía siendo
  lenta.
- **Wall-clock del PR subió a ~5 min** (de 2m 30s), peor que el
  estado actual.

**Lección**: una composite action que **incluya sccache** no
funciona con el setup actual. Si en el futuro queremos
re-intentar, el approach correcto es:
- Composite action que NO incluya sccache (dejarlo como step
  directo con `RUSTC_WRAPPER: sccache` en el step de cargo).
- O bien una imagen pre-baked que ya traiga todo configurado.

Mientras tanto, `cargo test --workspace --all-features` se
mantiene como ejecutor de tests. Los 469 tests corren en ~1.2s;
la ejecución nunca fue el cuello de botella.

### Alternativa rechazada B: Path filters en Windows/macOS

- Pros: ahorro de minutos-factura en PRs que solo tocan
  `agentyx-core`, `specs/`, `ui/` o `docs/`.
- Cons: cambia el modelo de "todo se prueba siempre" a "lo que
  tocas se prueba"; riesgo de regresiones no detectadas en
  Windows/macOS; oculto para el revisor si el path filter falla.
- **Por qué se descartó**: Tauri 2 es la app entera, no hay una
  capa clara de "core Tauri" vs "core Rust" que justifique filtrar
  por path. La matriz 3-OS es la red de seguridad que cubre
  Cocoa/Win32/WebKitGTK quirks. Reevaluar en v1.x si la cobertura
  3-OS pasa a ser opcional.

## References

- [Swatinem/rust-cache](https://github.com/Swatinem/rust-cache) —
  acción usada para cache de `target/` y registry.
- [mozilla-actions/sccache-action](https://github.com/mozilla-actions/sccache-action) —
  acción usada para sccache.
- [sccache](https://github.com/mozilla/sccache) — el binario
  subyacente.
- PR #37 (`chore(ci): rust-cache + sccache + cached cargo bin tools`)
  — implementación mergeada, 3 commits atómicos revertibles.
- PR #38 (`chore(ci): setup-rust-ci composite action + cargo nextest`)
  — intento fallido de composite action, cerrada con análisis
  detallado.
- Run baseline: #27462625580 (2026-06-13, ~15 min).
- Run post-PR #37: #27465321638 (2026-06-13, ~2m 30s).
- [GitHub Actions: caching dependencies](https://docs.github.com/en/actions/using-workflows/caching-dependencies-to-speed-up-workflows).
