# ADR-0008 — Scope de providers LLM en v1 (Ollama / Groq / Minimax)

**Status**: accepted
**Date**: 2026-06-05
**Deciders**: @miglesias

## Context

En el PRD original, los providers de v1 eran **OpenAI** (nativo),
**Anthropic** (nativo), **Ollama** (local) y **OpenAI-compatible
genérico** (Together, Groq, OpenRouter, …). Esta elección tenía
sentido como "max coverage con poco código", pero al repensar el
alcance del producto surgieron problemas:

1. **Identidad de Agentyx** ≠ "lo mismo que opencode pero en Rust".
   Agentyx es **agentic-first** (ver [`project.md`](../project.md) §Visión).
   El catálogo de providers debe reflejar los casos de uso del
   producto, no solo "lo que existe".
2. **Onboarding local-first**: el caso zero-friction es
   "descargar Agentyx → abrir un workspace → chatear con Ollama
   local". Esto exige que Ollama sea un first-class citizen, no
   "un provider más entre cuatro".
3. **Razonamiento de calidad sin pagar OpenAI**: hay modelos
   competitivos en coste/calidad fuera de OpenAI/Anthropic. Para
   un producto que quiere ser accesible, lock-in con OpenAI o
   Anthropic nativos es un antipatrón.
4. **Integración opencode ya probada**: opencode ya tiene
   integraciones estables de **Groq** y **Minimax** (en
   "minimax token plan" / `minimax-coding-plan`). Aprovechar
   ese conocimiento reduce riesgo de implementación.

## Decision

Los **3 providers de v1** son:

1. **Ollama** (local, default).
2. **Groq** (OpenAI-compatible, cloud, rápido y barato).
3. **Minimax** (Anthropic-compatible, cloud, bueno para razonamiento).

### Razones por provider

- **Ollama**:
  - Cero coste, sin API key, sin datos fuera de la máquina.
  - Cumple el principio **local-first** del producto.
  - Permite a usuarios probar Agentyx sin configurar nada.
  - Soporta tools desde 0.5+; capacidad de herramientas es
    transparente: `capabilities(model).tools` decide si se
    pasan o no.
  - **Default** (`default_provider = "ollama"`).

- **Groq**:
  - **OpenAI-compatible**: implementación trivial (mismo
    normalizador que el de openai_compat, solo cambia
    `base_url` y `api_key`).
  - Latencia muy baja (inferencia optimizada).
  - Modelos abiertos: `llama-3.3-70b-versatile`,
    `llama-3.1-8b-instant`, `mixtral-8x7b-32768`.
  - **Alternativa económica** para usuarios que quieran
    potencia sin pagar OpenAI.

- **Minimax**:
  - **Anthropic-compatible**: implementación ligeramente
    más compleja (bloques de content en el stream, no deltas
    simples), pero el código ya está validado en opencode.
  - Modelos `minimax-m2.7`, `minimax-m2.5` ya integrados en
    opencode como "minimax token plan"; API key se obtiene
    en `platform.minimax.io/login`.
  - Por defecto `base_url` apunta a `https://api.minimax.io/v1`
    (Anthropic-compatible); el usuario puede cambiar a
    `https://opencode.ai/zen/go/v1/messages` para usar el
    proxy de opencode.
  - **Opción de razonamiento** cuando Groq/Ollama no dan la
    talla.

### Providers NO incluidos en v1 (y por qué)

- **OpenAI nativo**: redundante con Groq (que ya es
  OpenAI-compatible). Si alguien lo pide, se añade en v1.x
  reutilizando el normalizador OpenAI-compatible con
  `base_url = "https://api.openai.com/v1"`. Sin coste de
  implementación significativo.
- **Anthropic nativo**: redundante con Minimax (Anthropic-
  compatible). Mismo razonamiento.
- **Bedrock, Vertex, Cohere, …**: cada uno requiere su
  propio adapter (Bedrock firma AWS SigV4, Vertex requiere
  GCP auth, Cohere tiene su propia API). v2 si hay demanda.
- **LM Studio, Jan, otros locales OpenAI-compatible**:
  pueden usar la base `OpenAI-compatible` cuando la
  reintroduzcamos en v1.x (ver más abajo).

### Hacia v1.x

- Cuando la base de usuarios lo pida, reintroducimos
  `openai_compat` genérico (un único provider con
  `base_url` + `api_key` configurables, modelo de
  capabilities hardcoded por nombre). Esto cubre
  Together, OpenRouter, LM Studio, Jan, y cualquier
  OpenAI-compatible nuevo que aparezca.
- OpenAI nativo y Anthropic nativo se añaden en v1.x
  también, compartiendo el normalizador de su gemelo
  compatible.

## Status

`accepted`. La spec de [`providers.md`](../domains/providers.md) se
reescribe en PR 3 de la reforma de scope.

## Consequences

### Positivas

- **Cobertura del 90 % de los casos** con 3 providers: local
  gratis (Ollama), cloud barato y rápido (Groq), cloud con
  razonamiento fuerte (Minimax).
- **Onboarding instantáneo**: usuario descarga Agentyx,
  abre Ollama, ya puede chatear. Sin keys, sin config.
- **Reducción de surface area**: 3 implementaciones
  (NDJSON, OpenAI-compatible, Anthropic-compatible) en
  vez de 4. Menos código, menos bugs, menos tests.
- **Alineación con opencode**: si opencode ya validó
  Groq y Minimax, tenemos un mapa de pitfalls conocido.
- **El usuario no se ata a un vendor**: 3 vendors
  distintos, 2 protocolos distintos, 1 opción local.

### Negativas

- **OpenAI nativo no es first-class**: si el usuario
  tiene una suscripción a OpenAI y quiere usarla,
  tiene que apuntar `base_url` al proxy de opencode
  (vía minimax) o esperar a v1.x. No es ideal para
  el early adopter de OpenAI.
- **Capacidades por modelo son hardcoded**: la lista
  de modelos hardcoded (Groq, Minimax) puede quedarse
  desfasada cuando un vendor saque un modelo nuevo.
  Mitigado: el usuario puede overridear `model_id`
  en el config; el LLM de fallback es "asume
  `tools: true, vision: false` si no conozco el
  modelo".
- **El usuario tiene que elegir provider al principio**:
  aunque Ollama es default, no todo el mundo tiene
  Ollama instalado. Necesitamos un onboarding que
  detecte "no tienes Ollama, instala o configura
  Groq/Minimax". Esto va en F23 (Onboarding) en v1.0.

### Neutras

- El trait `Provider` y el enum `ChatEvent` no cambian.
  Los cambios son **solo** en las implementaciones
  concretas (un archivo por provider).
- `domains/providers.md` se reescribe, pero las
  herramientas de testing (mock servers, fixtures)
  son las mismas.

## Alternatives considered

### Alternative A: Mantener los 4 originales (OpenAI, Anthropic, Ollama, openai_compat)

- Pros: máxima cobertura desde el día 1.
- Cons: 4 implementaciones a mantener, código
  duplicado (OpenAI y openai_compat son
  prácticamente el mismo normalizador), onboarding
  confuso ("¿cuál pongo?").
- **Por qué se descartó**: el producto no necesita
  4 vendors. Con 3 se cubre el 90 % de los casos.

### Alternative B: Solo Ollama (v1) y añadir el resto en v1.x

- Pros: ultra-simple. La app es local-first al 100 %.
- Cons: deja fuera a quien quiera potencia cloud.
  El producto se percibe como "juguete" hasta v1.x.
- **Por qué se descartó**: el usuario que paga por
  un agente espera poder elegir cloud. Ollama solo
  no es viable para usuarios con GPUs modestas o
  modelos grandes.

### Alternative C: OpenAI + Anthropic nativos (sin Ollama, sin Minimax)

- Pros: máxima calidad out-of-the-box.
- Cons: lock-in con dos vendors. Caro para
  experimentar. Pierde la propuesta local-first.
- **Por qué se descartó**: contradice el
  posicionamiento del producto (local-first,
  multi-provider real, sin lock-in).

### Alternative D: Esta (Ollama + Groq + Minimax)

- Pros: la decisión.
- Cons: las tres de arriba.
- **Por qué se eligió**: el mejor balance entre
  cobertura, coste de implementación y propuesta
  de valor.

## References

- [`../domains/providers.md`](../domains/providers.md) — spec
  reescrita con los 3 providers.
- [`../glossary.md`](../glossary.md) — `LLM Provider` actualizado.
- [`../project.md`](../project.md) — goals y non-goals.
- OpenCode como referencia de implementación:
  - Groq: `@ai-sdk/openai-compatible` con
    `https://api.groq.com/openai/v1`.
  - Minimax (token plan): `@ai-sdk/anthropic` con
    `https://opencode.ai/zen/go/v1/messages`, API key de
    `platform.minimax.io/login`.
  - Modelos actuales de minimax: `minimax-m2.7`, `minimax-m2.5`.
