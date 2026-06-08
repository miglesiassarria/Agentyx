<script lang="ts">
  import type { WorkspaceDto } from '$lib/ipc-types';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { workspaceStore } from '$lib/stores/workspace.svelte';

  import ConfirmDialog from './ConfirmDialog.svelte';
  import ExtrasSection from './ExtrasSection.svelte';

  interface Props {
    workspace: WorkspaceDto;
    onselect?: () => void;
  }

  let { workspace, onselect }: Props = $props();

  let menuOpen = $state(false);
  let confirmingDelete = $state(false);

  function handleSelect(): void {
    uiStore.showWorkspace();
    void workspaceStore.select(workspace.id);
    onselect?.();
  }

  function toggleMenu(): void {
    menuOpen = !menuOpen;
  }

  function askDelete(): void {
    menuOpen = false;
    confirmingDelete = true;
  }

  function confirmDelete(): void {
    confirmingDelete = false;
    void workspaceStore.deleteSelected();
  }
</script>

<div class="item" class:selected={workspaceStore.selectedId === workspace.id}>
  <button type="button" class="row" onclick={handleSelect} title={workspace.rootPath}>
    <span class="icon" aria-hidden="true">📁</span>
    <span class="name">{workspace.name}</span>
    {#if workspace.hasVenv}
      <span class="venv-dot" title="Detected .venv" aria-label="venv detected">🐍</span>
    {/if}
  </button>
  <button
    type="button"
    class="menu-btn"
    onclick={toggleMenu}
    aria-label="Actions for {workspace.name}"
    aria-haspopup="true"
    aria-expanded={menuOpen}
  >
    ⋯
  </button>

  {#if menuOpen}
    <div
      class="menu"
      role="menu"
      tabindex={-1}
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
    >
      <button
        type="button"
        class="menu-item danger"
        role="menuitem"
        onclick={askDelete}
        disabled={workspaceStore.mutating}
      >
        Delete workspace…
      </button>
    </div>
  {/if}

  {#if workspaceStore.selectedId === workspace.id}
    <div class="extras-wrap">
      <ExtrasSection {workspace} />
    </div>
  {/if}
</div>

<ConfirmDialog
  open={confirmingDelete}
  title={confirmingDelete ? `¿Borrar workspace "${workspace.name}"?` : ''}
  message={'Esto eliminará:\n  • ~/.agentyx/workspaces/<id>/\n  • Su historial de chat\n  • Su journal\n  • Sus extra paths (de la config)\n\nLos archivos del proyecto NO se tocan (ni el root ni los extras).'}
  confirmLabel="Delete"
  cancelLabel="Cancel"
  destructive
  onconfirm={confirmDelete}
  oncancel={() => (confirmingDelete = false)}
/>

<style>
  .item {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: 0;
  }

  .row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-3);
    background: transparent;
    border: none;
    color: var(--color-fg);
    cursor: pointer;
    text-align: left;
    font: inherit;
    border-radius: var(--radius-sm);
    width: 100%;
    flex: 1;
    min-width: 0;
  }

  .row:hover {
    background: var(--color-bg-subtle);
  }

  .item.selected .row {
    background: var(--color-bg-subtle);
    color: var(--color-fg);
  }

  .icon {
    flex-shrink: 0;
  }

  .name {
    flex: 1;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    font-size: var(--font-size-sm);
  }

  .venv-dot {
    font-size: var(--font-size-sm);
  }

  .menu-btn {
    flex-shrink: 0;
    width: 1.75em;
    background: transparent;
    border: none;
    color: var(--color-fg-muted);
    cursor: pointer;
    border-radius: var(--radius-sm);
    font-size: var(--font-size-md);
  }

  .menu-btn:hover {
    background: var(--color-bg);
  }

  .menu {
    position: absolute;
    margin-top: var(--space-1);
    background: var(--color-bg-elevated);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.2);
    min-width: 180px;
    z-index: var(--z-dropdown);
  }

  .menu-item {
    display: block;
    width: 100%;
    padding: var(--space-2) var(--space-3);
    background: transparent;
    border: none;
    color: var(--color-fg);
    text-align: left;
    font: inherit;
    cursor: pointer;
  }

  .menu-item:hover:not(:disabled) {
    background: var(--color-bg-subtle);
  }

  .menu-item.danger {
    color: var(--color-danger);
  }

  .menu-item:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .extras-wrap {
    padding: var(--space-2) var(--space-3) var(--space-3);
    border-left: 2px solid var(--color-border-subtle);
    margin-left: var(--space-4);
  }
</style>
