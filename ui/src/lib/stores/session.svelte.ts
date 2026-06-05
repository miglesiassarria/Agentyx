/// Stub for the Svelte 5 rune-based session store.
/// F01 implementation lands in Fase D.

export const sessionPlaceholder = $state({ ready: false });

export function sessionState() {
  return sessionPlaceholder;
}
