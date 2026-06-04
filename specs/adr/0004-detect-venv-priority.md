# ADR-0004 — Orden de detección del entorno virtual de Python

**Status**: accepted
**Date**: 2026-06-04
**Deciders**: @miglesias

## Context

El tool `python_run` debe ejecutar Python en el `.venv` del workspace.
Cada workspace puede tener su venv de muchas formas:

- `.venv/` (convención `uv` y reciente Python).
- `venv/` (convención histórica `python -m venv`).
- `.python-version` (pyenv).
- `pyproject.toml` con `[project] requires-python` (uv/poetry/pdm).
- `poetry.lock`, `pdm.lock`, `uv.lock` (lockfile que implica gestor).
- `conda-env.yml` / `environment.yml` (conda).

La detección tiene que ser:

1. **Determinista**: dado el mismo árbol de archivos, siempre devuelve
   la misma `VenvSpec`.
2. **Testeable**: cubrible por tests con fixtures.
3. **Rápida**: < 50 ms en el caso común (workspace con `.venv/`).
4. **Conservadora**: si no encuentra nada, **no crea nada**. El usuario
   decide explícitamente crear el venv desde la UI.
5. **Segura**: no escribe fuera del `root` del workspace, nunca sigue
   symlinks que apunten fuera.

## Decision

Orden de detección (el primer match gana, no se acumulan):

1. **`.venv/`** (más explícito, convención más reciente).
2. **`venv/`** (convención histórica).
3. **`.python-version`** (pyenv) → resolver interpreter vía `pyenv which`.
4. **`pyproject.toml`** con `[tool.uv]` o `[tool.poetry]` o `[project]` →
   usar el binario del venv del gestor (si existe) o
   `.venv/bin/python` si está.
5. **`uv.lock`** o **`poetry.lock`** → sugiere el venv del gestor; si
   no está, retorna `null` y se loguea `tracing::info!` con sugerencia.
6. **`conda-env.yml`** → **fuera de v1**: retorna `null` y log
   `tracing::warn!` "conda no soportado en v1; spec a actualizar".
7. Si nada matchea: retorna `null` (workspace sin venv).

## Status

`accepted`. La spec de `domains/workspace.md` referenciará este orden.

## Consequences

### Positivas
- **Determinismo** ⇒ tests reproducibles.
- **No-op por defecto** ⇒ workspace sin venv no crea nada raro.
- **Mensajes de log útiles** para el developer que use `uv`/`poetry`
  sin venv aún.
- **Cumple el principio "no hacer trabajo no pedido"** del proyecto.

### Negativas
- **Conda queda fuera de v1**. Quien lo use tiene que convertir a
  `uv` o `venv` estándar.
- **`.python-version` requiere `pyenv` instalado** para resolver
  interpreter. Si no, retorna `null` y warning.
- **El orden no es configurable** (v1). Si un usuario quiere otro
  orden, lo hace explícito en el path de la tool (`python_run(venv=…)`).

### Neutras
- En el futuro podemos añadir un override por workspace
  (`workspace.config.toml` → `venv.path = ...`) sin cambiar este ADR.

## Alternatives considered

### Alternative A: Auto-crear venv si no existe
- Pros: zero-config.
- Cons: el usuario explícitamente pidió **"si no, no se crea"**. Viola
  el principio de no sorprender.
- **Por qué se descartó**: el usuario debe crear el venv con acción
  explícita ("Crear venv aquí" en la UI).

### Alternative B: Usar siempre `python` del sistema
- Pros: simple, cero detección.
- Cons: rompe el requisito de "venv por workspace". Quien tenga
  paquetes en `.venv` no los vería.
- **Por qué se descartó**: contradice la decisión de proyecto.

### Alternative C: Soporte de conda en v1
- Pros: cuota de mercado de data scientists.
- Cons: integración conda ≠ `python -m venv`; añade un backend entero
  y tests con fixtures. Fuera de scope de v1.
- **Por qué se descartó**: tiempo y complejidad. Se deja como feature
  v2 explícita.

### Alternative D: Orden configurable por usuario
- Pros: máxima flexibilidad.
- Cons: tests más complejos, UX más enrevesada. La mayoría de los
  workspaces son estándar.
- **Por qué se descartó**: KISS. Se puede añadir más tarde con un
  override por workspace.

## References

- [project.md](../project.md) — goals y non-goals.
- [architecture.md](../architecture.md) — flujo de `python_run`.
- [ipc.md](../ipc.md) — Tauri command `workspace_detect_venv` y
  endpoints HTTP equivalentes.
- Spec de dominio: `domains/workspace.md` (pendiente).
