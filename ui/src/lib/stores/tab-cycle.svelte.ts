/// Tab-cycle handler — global keyboard shortcut to cycle
/// between primary agents.
///
/// Per `specs/features/F-agents-ui.md` §Cycle con Tab (Cmd+[/Cmd+]):
/// - Cmd+[ = previous primary agent
/// - Cmd+] = next primary agent
/// - Only fires when focus is **not** in an input/textarea/contenteditable
/// - Only cycles if there is more than 1 primary visible
/// - Cycle during a run returns `conflict` from the backend; the
///   store surfaces this as a toast/error.

import { sessionStore } from './session.svelte';

let installed = false;

function isEditableTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  const tag = target.tagName;
  if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return true;
  if (target.isContentEditable) return true;
  return false;
}

function cycle(direction: 'prev' | 'next'): void {
  const ids = sessionStore.primaryAgents.map((a) => a.id);
  if (ids.length <= 1) return;
  const current = sessionStore.activeAgent?.id ?? ids[0];
  if (current === undefined) return;
  const idx = ids.indexOf(current);
  const nextIdx =
    direction === 'next' ? (idx + 1) % ids.length : (idx - 1 + ids.length) % ids.length;
  const nextId = ids[nextIdx];
  if (nextId === undefined) return;
  void sessionStore.setActiveAgent(nextId);
}

function handleKeydown(e: KeyboardEvent): void {
  if (!e.metaKey && !e.ctrlKey) return;
  if (e.altKey || e.shiftKey) return;
  if (isEditableTarget(e.target)) return;
  if (e.key === '[') {
    e.preventDefault();
    cycle('prev');
  } else if (e.key === ']') {
    e.preventDefault();
    cycle('next');
  }
}

/** Install the global keydown listener. Idempotent. */
export function installTabCycle(): void {
  if (installed) return;
  installed = true;
  window.addEventListener('keydown', handleKeydown);
}

/** Remove the global keydown listener. Useful for tests. */
export function uninstallTabCycle(): void {
  if (!installed) return;
  installed = false;
  window.removeEventListener('keydown', handleKeydown);
}
