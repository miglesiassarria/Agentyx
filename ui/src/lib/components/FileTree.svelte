<script lang="ts">
  import FileTreeNode from './FileTreeNode.svelte';
  import { workspaceStore } from '$lib/stores/workspace.svelte';

  let root = $derived.by(() => {
    const ws = workspaceStore.selected;
    if (ws === null) return null;
    return workspaceStore.fileTree[ws.rootPath] ?? null;
  });
</script>

<div class="tree" role="tree" aria-label="Workspace files">
  {#if workspaceStore.fileTreeLoading && root === null}
    <p class="loading">Loading files…</p>
  {:else if root === null}
    <p class="empty">No files to show.</p>
  {:else if root.error !== null}
    <p class="error" role="alert">Couldn't list directory: {root.error}</p>
  {:else if root.children.length === 0}
    <p class="empty">This workspace is empty. Drop some files in to get started.</p>
  {:else}
    <FileTreeNode node={root} depth={0} />
  {/if}
</div>

<style>
  .tree {
    border: 1px solid var(--color-border-subtle);
    border-radius: var(--radius-md);
    background: var(--color-bg-elevated);
    padding: var(--space-2);
    overflow: auto;
    max-height: 60vh;
  }

  .loading,
  .empty {
    color: var(--color-fg-subtle);
    font-size: var(--font-size-sm);
    margin: 0;
    padding: var(--space-3);
    text-align: center;
  }

  .error {
    color: var(--color-danger);
    font-size: var(--font-size-sm);
    margin: 0;
    padding: var(--space-3);
  }
</style>
