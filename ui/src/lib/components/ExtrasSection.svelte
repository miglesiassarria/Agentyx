<script lang="ts">
  import type { ExtraPathDto, WorkspaceDto } from '$lib/ipc-types';
  import { workspaceStore } from '$lib/stores/workspace.svelte';

  import ConfirmDialog from './ConfirmDialog.svelte';

  interface Props {
    workspace: WorkspaceDto;
  }

  let { workspace }: Props = $props();

  let confirmingExtra = $state<ExtraPathDto | null>(null);

  function handleAdd(): void {
    void workspaceStore.addExtraPathViaDialog();
  }

  function handleRemove(extra: ExtraPathDto): void {
    confirmingExtra = extra;
  }

  function confirmRemove(): void {
    if (confirmingExtra === null) return;
    const path = confirmingExtra.path;
    confirmingExtra = null;
    void workspaceStore.removeExtraPath(path);
  }
</script>

<section class="extras">
  <header>
    <h3>Extras ({workspace.extraPaths.length})</h3>
  </header>

  {#if workspace.extraPaths.length === 0}
    <p class="hint">
      No extra paths. Add a directory to give the agent access beyond the workspace root.
    </p>
  {:else}
    <ul>
      {#each workspace.extraPaths as extra (extra.path)}
        <li>
          <span class="label" title={extra.path}>{extra.label}</span>
          <button
            type="button"
            class="remove"
            onclick={() => handleRemove(extra)}
            aria-label="Remove {extra.label}"
            title="Remove"
          >
            ✕
          </button>
        </li>
      {/each}
    </ul>
  {/if}

  <button type="button" class="add" onclick={handleAdd} disabled={workspaceStore.mutating}>
    + Add directory
  </button>
</section>

<ConfirmDialog
  open={confirmingExtra !== null}
  title={confirmingExtra ? `Quitar "${confirmingExtra.label}" de "${workspace.name}"?` : ''}
  message={'El agente ya no podrá acceder a este directorio. Los archivos no se tocan.'}
  confirmLabel="Remove"
  cancelLabel="Cancel"
  destructive
  onconfirm={confirmRemove}
  oncancel={() => (confirmingExtra = null)}
/>

<style>
  .extras {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  header h3 {
    margin: 0;
    font-size: var(--font-size-sm);
    font-weight: 600;
    color: var(--color-fg-muted);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .hint {
    margin: 0;
    color: var(--color-fg-subtle);
    font-size: var(--font-size-xs);
    line-height: 1.4;
  }

  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  li {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-1) var(--space-2);
    border-radius: var(--radius-sm);
  }

  li:hover {
    background: var(--color-bg-subtle);
  }

  .label {
    flex: 1;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    font-size: var(--font-size-sm);
  }

  .remove {
    flex-shrink: 0;
    width: 1.5em;
    height: 1.5em;
    border: none;
    background: transparent;
    color: var(--color-fg-subtle);
    cursor: pointer;
    border-radius: var(--radius-sm);
    font-size: var(--font-size-md);
  }

  .remove:hover {
    background: var(--color-danger);
    color: var(--color-primary-fg);
  }

  .add {
    align-self: flex-start;
    background: transparent;
    color: var(--color-primary);
    border: 1px dashed var(--color-border);
    padding: var(--space-1) var(--space-3);
    border-radius: var(--radius-sm);
    font-size: var(--font-size-sm);
    cursor: pointer;
  }

  .add:hover:not(:disabled) {
    background: var(--color-bg-subtle);
    border-style: solid;
  }

  .add:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
</style>
