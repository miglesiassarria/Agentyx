<script lang="ts">
  import { uiStore } from '$lib/stores/ui.svelte';
  import { workspaceStore } from '$lib/stores/workspace.svelte';

  import WorkspaceListItem from './WorkspaceListItem.svelte';

  function handleAdd(): void {
    void workspaceStore.openViaDialog();
  }
</script>

<aside class="sidebar" aria-label="Workspaces">
  <header>
    <h2>Workspaces</h2>
    <button
      type="button"
      class="add"
      onclick={handleAdd}
      disabled={workspaceStore.mutating}
      title="Open a folder as a workspace"
    >
      + Add
    </button>
  </header>

  <button
    type="button"
    class="settings-link"
    class:active={uiStore.activeView === 'settings'}
    onclick={() => uiStore.showSettings()}
  >
    Settings
  </button>

  <div class="list">
    {#if workspaceStore.loadingList && workspaceStore.list.length === 0}
      <p class="loading">Loading…</p>
    {:else if workspaceStore.list.length === 0}
      <p class="empty-list">No workspaces yet.</p>
    {:else}
      {#each workspaceStore.list as workspace (workspace.id)}
        <WorkspaceListItem {workspace} />
      {/each}
    {/if}
  </div>

  {#if workspaceStore.lastError}
    <div class="error" role="alert">
      <span class="error-label">Error:</span>
      {workspaceStore.lastError}
    </div>
  {/if}
</aside>

<style>
  .sidebar {
    display: flex;
    flex-direction: column;
    width: 280px;
    border-right: 1px solid var(--color-border-subtle);
    background: var(--color-bg-elevated);
    overflow: hidden;
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: var(--space-3) var(--space-4);
    border-bottom: 1px solid var(--color-border-subtle);
  }

  header h2 {
    margin: 0;
    font-size: var(--font-size-sm);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--color-fg-muted);
  }

  .add {
    background: var(--color-primary);
    color: var(--color-primary-fg);
    border: 1px solid var(--color-primary);
    border-radius: var(--radius-sm);
    padding: var(--space-1) var(--space-3);
    font-size: var(--font-size-sm);
    cursor: pointer;
  }

  .add:hover:not(:disabled) {
    filter: brightness(1.1);
  }

  .add:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .list {
    flex: 1;
    overflow: auto;
    padding: var(--space-2);
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }

  .settings-link {
    margin: var(--space-2);
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--color-border-subtle);
    border-radius: var(--radius-md);
    background: transparent;
    color: var(--color-fg-muted);
    text-align: left;
  }

  .settings-link:hover,
  .settings-link.active {
    background: var(--color-bg-subtle);
    color: var(--color-fg);
  }

  .loading,
  .empty-list {
    color: var(--color-fg-subtle);
    font-size: var(--font-size-sm);
    text-align: center;
    padding: var(--space-4);
    margin: 0;
  }

  .error {
    margin: var(--space-2);
    padding: var(--space-2) var(--space-3);
    background: var(--color-bg-subtle);
    color: var(--color-danger);
    border: 1px solid var(--color-border-subtle);
    border-radius: var(--radius-sm);
    font-size: var(--font-size-xs);
    word-break: break-word;
  }

  .error-label {
    font-weight: 600;
    margin-right: var(--space-1);
  }
</style>
