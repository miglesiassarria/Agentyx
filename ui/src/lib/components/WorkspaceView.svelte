<script lang="ts">
  import type { WorkspaceDto } from '$lib/ipc-types';

  import ChatPanel from './ChatPanel.svelte';
  import FileTree from './FileTree.svelte';
  import PermissionPrompt from './PermissionPrompt.svelte';
  import VenvBadge from './VenvBadge.svelte';
  import { sessionStore } from '$lib/stores/session.svelte';
  import { workspaceStore } from '$lib/stores/workspace.svelte';

  interface Props {
    workspace: WorkspaceDto;
  }

  let { workspace }: Props = $props();

  async function respondToPermission(
    requestId: string,
    response:
      | { kind: 'allowOnce' }
      | { kind: 'allowSession' }
      | { kind: 'allowAlways'; tool: string }
      | { kind: 'deny' },
  ): Promise<void> {
    await sessionStore.respondToPermission(requestId, response);
  }
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

  <div class="split">
    <aside class="files">
      <h2 class="section-title">Files</h2>
      <FileTree />
    </aside>

    <section class="chat">
      <ChatPanel workspaceId={workspace.id} />
    </section>
  </div>

  <PermissionPrompt request={sessionStore.currentPermission} onrespond={respondToPermission} />
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

  .section-title {
    margin: 0;
    font-size: var(--font-size-sm);
    font-weight: 600;
    color: var(--color-fg-muted);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .split {
    flex: 1;
    display: grid;
    grid-template-columns: minmax(220px, 320px) 1fr;
    gap: 0;
    min-height: 0;
    overflow: hidden;
  }

  .files {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-4) var(--space-5);
    overflow: auto;
    border-right: 1px solid var(--color-border-subtle);
  }

  .chat {
    min-height: 0;
    overflow: hidden;
  }
</style>
