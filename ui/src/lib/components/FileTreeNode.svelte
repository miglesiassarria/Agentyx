<script lang="ts">
  import { workspaceStore } from '$lib/stores/workspace.svelte';
  import type { FileTreeNodeData } from '$lib/stores/workspace.svelte';

  import FileTreeNode from './FileTreeNode.svelte';

  interface Props {
    node: FileTreeNodeData;
    depth: number;
  }

  let { node, depth }: Props = $props();

  function handleClick(): void {
    if (node.isDir) {
      workspaceStore.toggleNode(node.path);
    }
  }

  function iconFor(n: FileTreeNodeData): string {
    if (n.isDir) return n.expanded ? '📂' : '📁';
    return '📄';
  }
</script>

<div
  class="row"
  style="--depth: {depth}"
  role="treeitem"
  aria-expanded={node.isDir ? node.expanded : undefined}
  aria-selected={false}
>
  <button type="button" class="btn" onclick={handleClick} title={node.path} disabled={!node.isDir}>
    <span class="icon" aria-hidden="true">{iconFor(node)}</span>
    <span class="name">{node.name}</span>
    {#if node.isDir && node.loading}
      <span class="hint">…</span>
    {/if}
  </button>

  {#if node.error}
    <div class="error" role="alert">{node.error}</div>
  {/if}

  {#if node.isDir && node.expanded && node.loaded}
    {#if node.children.length === 0}
      <div class="empty" style="--depth: {depth + 1}">(empty)</div>
    {:else}
      {#each node.children as child (child.path)}
        <FileTreeNode node={child} depth={depth + 1} />
      {/each}
    {/if}
  {/if}
</div>

<style>
  .row {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .btn {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-1) var(--space-2);
    padding-left: calc(var(--space-2) + var(--depth) * var(--space-3));
    background: transparent;
    border: none;
    color: var(--color-fg);
    cursor: pointer;
    text-align: left;
    font: inherit;
    border-radius: var(--radius-sm);
    width: 100%;
  }

  .btn:hover:not(:disabled) {
    background: var(--color-bg-subtle);
  }

  .btn:disabled {
    cursor: default;
  }

  .icon {
    flex-shrink: 0;
    width: 1.25em;
    text-align: center;
  }

  .name {
    flex: 1;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    font-size: var(--font-size-sm);
  }

  .hint {
    color: var(--color-fg-subtle);
    font-size: var(--font-size-sm);
  }

  .error {
    color: var(--color-danger);
    font-size: var(--font-size-xs);
    padding-left: calc(var(--space-2) + var(--depth) * var(--space-3) + var(--space-5));
  }

  .empty {
    color: var(--color-fg-subtle);
    font-size: var(--font-size-xs);
    padding-left: calc(var(--space-2) + var(--depth) * var(--space-3) + var(--space-5));
    font-style: italic;
  }
</style>
