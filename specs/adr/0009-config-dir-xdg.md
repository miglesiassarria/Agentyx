# ADR-0009 — Config directory en macOS/Linux (`~/.config/agentix`)

**Status**: accepted
**Date**: 2026-06-13
**Deciders**: @miglesias

## Context

El PRD original ubicaba todo el estado de Agentyx en `~/.agentyx/`:

```
~/.agentyx/
├── config.toml
├── state.json
├── workspaces/
│   └── <id>/
│       ├── config.toml
│       └── state.db
├── cache/
│   └── <hash>/
└── locks/
```

Esta convención es habitual en herramientas Unix legacy, pero hoy es
considerada mala práctica en macOS y Linux:

1. **Directorio home saturado**: `~` se llena de directorios ocultos.
   El estándar [XDG Base Directory Specification](https://specifications.freedesktop.org/basedir-spec/basedir-spec-latest.html)
   nació precisamente para evitarlo.
2. **XDG compliance**: macOS y Linux modernos siguen XDG
   (`$XDG_CONFIG_HOME` → `~/.config` por defecto). Los usuarios
   que usan `xdg-user-dirs` o herramientas que respetan XDG esperan
   encontrar configs en `~/.config`.
3. **Windows es diferente**: en Windows el estándar es `%APPDATA%`
   (ya implementado como `dirs::data_dir()`), así que el cambio
   solo aplica a macOS/Linux.

## Decision

- **macOS / Linux**: usar `~/.config/agentix/` como root de
  configuración en lugar de `~/.agentyx/`.
- **Windows**: sigue usando `%APPDATA%\agentyx` (sin cambios).

La migración es automática: si existe `~/.agentyx/` y no existe
`~/.config/agentix/`, la app copia el contenido al nuevo path y
borra el viejo (o lo deja como fallback con warning en logs).

## Consequences

### Positivas

- **Directorio home limpio**: sin saturar `~` con archivos ocultos.
- **XDG compliant**: alineado con expectativas de usuarios
  avanzados y herramientas modernas.
- **Windows sin cambios**: la convención Windows (`%APPDATA%`)
  ya es correcta.

### Negativas

- **Migración de datos existentes**: los usuarios que ya tienen
  `~/.agentyx/` necesitan que la app migre. El código de migración
  debe ser robusto (copia antes de borrar, rollback si falla).
- **Documentación existente**: cualquier referencia a `~/.agentyx/`
  en docs o logs debe actualizarse.

### Neutras

- El nombre del directorio (`agentix`) no cambia.
- Los subdirectorios (`workspaces/`, `cache/`, `locks/`) se crean
  bajo el nuevo root sin cambios de estructura interna.

## Alternatives considered

### Alternative A: Mantener `~/.agentyx/` en macOS/Linux

- Pros: cero migración, backward compatible.
- Cons: directorio home sigue saturado; viola XDG.
- **Por qué se descartó**: el usuario explícitamente prefiere
  `~/.config/agentix`.

### Alternative B: Usar `~/.config/agentyx/` (sin `i`)

- Pros: coherente con el nombre del proyecto (sin abreviar).
- Cons: cambio de nombre en todos lados; más riesgo de bugs.
- **Por qué se descarta**: el usuario pidió `agentix` (con `i`).

### Alternative C: Variable de entorno `AGENTYX_HOME` overrideable

- Pros: máximo control para usuarios power.
- Cons: complejidad extra en código y documentación.
- **Por qué se descarta**: overkill para v1. Se puede añadir en
  v1.x si alguien lo pide.

## References

- [XDG Base Directory Specification](https://specifications.freedesktop.org/basedir-spec/basedir-spec-latest.html)
- `crates/agentyx-app/src/state.rs` — función `agentyx_home()`
- `crates/agentyx-core/src/config/service.rs` — `ServiceConfigPaths`
