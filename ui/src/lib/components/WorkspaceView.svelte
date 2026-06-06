<script lang="ts">
  import type { WorkspaceDto } from '$lib/ipc-types';
  import { workspaceStore } from '$lib/stores/workspace.svelte';

  import FileTree from './FileTree.svelte';
  import VenvBadge from './VenvBadge.svelte';

  interface Props {
    workspace: WorkspaceDto;
  }

  let { workspace }: Props = $props();
</script>

<main class="view">
  <header class="header">
    <div class="title-row">
      <span class="icon" aria-hidden="true">📁</span>
      <div class="title-block">
        <h1>{workspace.name}</h1>
        <p class="path" title={workspace.rootPath}>{workspace.rootPath}</p>
      </div>
    </div>
    <div class="badges">
      {#if workspaceStore.venvLoading}
        <span class="venv-checking">Checking venv…</span>
      {:else if workspaceStore.venv}
        <VenvBadge venv={workspaceStore.venv} />
      {/if}
    </div>
  </header>

  <section class="content">
    <h2 class="section-title">Files</h2>
    <FileTree />
  </section>

  <footer class="footer">
    <button type="button" class="open-chat" disabled title="Coming in F01 (chat streaming)">
      Open chat
    </button>
    <span class="hint">Chat with the agent (F01)</span>
  </footer>
</main>

<style>
  .view {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
  }

  .header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-4);
    padding: var(--space-4) var(--space-5);
    border-bottom: 1px solid var(--color-border-subtle);
  }

  .title-row {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    min-width: 0;
  }

  .icon {
    font-size: var(--font-size-xl);
  }

  .title-block {
    min-width: 0;
  }

  .title-block h1 {
    margin: 0;
    font-size: var(--font-size-lg);
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .path {
    margin: 0;
    color: var(--color-fg-subtle);
    font-family: var(--font-mono);
    font-size: var(--font-size-xs);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .badges {
    flex-shrink: 0;
  }

  .venv-checking {
    color: var(--color-fg-subtle);
    font-size: var(--font-size-sm);
  }

  .content {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-4) var(--space-5);
    overflow: auto;
  }

  .section-title {
    margin: 0;
    font-size: var(--font-size-sm);
    font-weight: 600;
    color: var(--color-fg-muted);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .footer {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-5);
    border-top: 1px solid var(--color-border-subtle);
    background: var(--color-bg-elevated);
  }

  .open-chat {
    background: var(--color-primary);
    color: var(--color-primary-fg);
    border: 1px solid var(--color-primary);
    padding: var(--space-2) var(--space-4);
    border-radius: var(--radius-md);
    font: inherit;
    cursor: pointer;
  }

  .open-chat:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .hint {
    color: var(--color-fg-subtle);
    font-size: var(--font-size-xs);
  }
</style>
