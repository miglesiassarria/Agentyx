# ADR-0006 — SQLite con `rusqlite` (bundled) en vez de `sqlx`

**Status**: accepted
**Date**: 2026-06-04
**Deciders**: @miglesias

## Context

Necesitamos persistencia local para:

- Estado global: `config`, mappings de providers, settings.
- Estado por workspace: sesiones, mensajes, journal, índices.
- Métricas locales: `stats.db` con tokens consumidos, latencias.

Los datos son **relacionales pero pequeños** (decenas de miles de filas
por workspace, no millones), accedidos por la app de un solo proceso
(un solo writer).

Opciones:

- **`rusqlite` (bundled)** — wrapper síncrono sobre SQLite C library,
  embebido en el binario (sin dependencia externa en runtime).
- **`sqlx`** — async, con macros `query!` que validan SQL contra una
  base en tiempo de compilación. Requiere `tokio`/`async-std`.
- **`diesel`** — ORM pesado, migrations propias.
- **Postgres / MySQL / Redis** — fuera de scope, contradice el goal
  de "sin DB externa en v1".

## Decision

**Adoptamos `rusqlite` con `bundled` feature** (la librería SQLite
C se compila y embebe en el binario, sin necesidad de un `libsqlite3`
externo).

Para las migraciones: archivos `.sql` versionados en
`crates/agentyx-core/src/storage/migrations/`, aplicados por un
runner interno en el arranque con un tracking de versión en
`__migrations` (id, applied_at).

## Status

`accepted`.

## Consequences

### Positivas
- **Cero dependencias en runtime**: el binario lleva SQLite dentro.
  No hay `libsqlite3` que falte en distros minimalistas.
- **API simple y predecible**: `let conn = Connection::open(path)?;`.
- **No necesitas compilar contra una DB de desarrollo** (al contrario
  que `sqlx` con `query!`).
- **Funciona bien con `tokio::task::spawn_blocking`** para no bloquear
  el runtime async.
- **Migraciones triviales**: archivos `.sql` en orden lexicográfico.

### Negativas
- **No es async**: hay que envolver con `spawn_blocking` o similar.
  Es el patrón estándar.
- **Sin validación de SQL en compile-time** (a diferencia de `sqlx`).
  Compensa con tests de integración que ejercitan las queries.
- **No hay tipos generados automáticamente** del schema. Hay que
  definirlos a mano en Rust o con `serde` desde JSON.

### Neutras
- **Connection pool**: para un solo proceso con un solo writer, no
  hace falta `r2d2` ni `deadpool`. Una conexión por workspace
  abierta con un `Mutex` o `RwLock` es suficiente.
- **WAL mode activado por defecto** (`PRAGMA journal_mode=WAL`) para
  concurrencia razonable entre lector (UI) y writer (core).

## Alternatives considered

### Alternative A: `sqlx` con `sqlite` y `runtime-tokio`
- Pros: async nativo, `query!` macros que validan SQL en compile-time.
- Cons: requiere DB de desarrollo (o `sqlx prepare`) para compilar,
  añade ~3 crates pesados, async-enforced puede chocar con APIs
  síncronas en bordes.
- **Por qué se descartó**: nuestro perfil (un solo writer, datos
  pequeños) no necesita async DB. `rusqlite` es más simple y ligero.

### Alternative B: `diesel`
- Pros: ORM maduro, typesafe queries, migraciones integradas.
- Cons: API verbose, compilaciones lentas, DSL propia.
- **Por qué se descartó**: la mayoría de queries en este proyecto son
  CRUD simple; un ORM completo es overkill.

### Alternative C: DB externa (Postgres/Redis)
- Pros: escalable.
- Cons: contradice el goal de "ligero" y "sin DB externa en v1".
  Obliga al usuario a instalar/correr un server.
- **Por qué se descartó**: explícitamente fuera de scope.

## Implementation notes

- Wrapper en `crates/agentyx-core/src/storage/db.rs`:
  - `Db::open(path) -> Db` (abre o crea, aplica migraciones pendientes).
  - `Db::conn() -> MutexGuard<Connection>`.
- Migraciones: `migrations/0001_initial.sql`, `0002_journal.sql`, ….
- Tracking en tabla `__migrations(id INTEGER PRIMARY KEY, applied_at INTEGER NOT NULL)`.
- PRAGMAs al abrir: `journal_mode=WAL`, `foreign_keys=ON`,
  `synchronous=NORMAL` (balance durabilidad/rendimiento).

## References

- [architecture.md](../architecture.md) — capa `storage::*`.
- [project.md](../project.md) — non-goal "sin DB externa en v1".
- Spec de dominio: `domains/storage.md` (pendiente).
- Web: <https://docs.rs/rusqlite>.
