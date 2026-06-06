<script lang="ts">
  import { onDestroy, onMount } from 'svelte';

  import EmptyState from './lib/components/EmptyState.svelte';
  import Sidebar from './lib/components/Sidebar.svelte';
  import WorkspaceView from './lib/components/WorkspaceView.svelte';
  import { workspaceStore } from './lib/stores/workspace.svelte';

  let detach: (() => void) | null = null;

  onMount(async () => {
    detach = await workspaceStore.attach();
    await workspaceStore.loadList();
  });

  onDestroy(() => {
    detach?.();
  });

  function handleOpen(): void {
    void workspaceStore.openViaDialog();
  }
</script>

<div class="app-shell">
  <Sidebar />
  <section class="main">
    {#if workspaceStore.selected === null}
      {#if workspaceStore.list.length === 0 && !workspaceStore.loadingList}
        <EmptyState
          title="No workspace open"
          message="Pick a folder to start. Agentyx gives the agent read/write access to that folder (and any extras you add)."
          actionLabel="+ Open workspace"
          onaction={handleOpen}
        />
      {:else}
        <EmptyState
          title="Select a workspace"
          message="Pick a folder from the sidebar to see its files and extra paths."
        />
      {/if}
    {:else}
      <WorkspaceView workspace={workspaceStore.selected} />
    {/if}
  </section>
</div>

<style>
  .app-shell {
    display: grid;
    grid-template-columns: auto 1fr;
    height: 100%;
    background: var(--color-bg);
    color: var(--color-fg);
  }

  .main {
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }
</style>
