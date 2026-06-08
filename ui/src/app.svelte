<script lang="ts">
  import { onDestroy, onMount } from 'svelte';

  import EmptyState from './lib/components/EmptyState.svelte';
  import PathPromptHost from './lib/components/PathPromptHost.svelte';
  import SettingsView from './lib/components/settings/SettingsView.svelte';
  import Sidebar from './lib/components/Sidebar.svelte';
  import WorkspaceView from './lib/components/WorkspaceView.svelte';
  import { uiStore } from './lib/stores/ui.svelte';
  import { workspaceStore } from './lib/stores/workspace.svelte';

  let detach: (() => void) | null = null;
  let sidebarOpen = $state(false);
  const mobileTitle = $derived(
    uiStore.activeView === 'settings' ? 'Settings' : (workspaceStore.selected?.name ?? 'Agentyx'),
  );

  onMount(async () => {
    detach = await workspaceStore.attach();
    await workspaceStore.loadList();
  });

  onDestroy(() => {
    detach?.();
  });

  function handleOpen(): void {
    void workspaceStore.openViaDialog();
    sidebarOpen = false;
  }

  function closeSidebar(): void {
    sidebarOpen = false;
  }
</script>

<div class="app-shell" class:sidebar-open={sidebarOpen}>
  <div class="sidebar-panel">
    <Sidebar onnavigate={closeSidebar} />
  </div>
  {#if sidebarOpen}
    <button
      type="button"
      class="sidebar-backdrop"
      aria-label="Close workspace navigation"
      onclick={closeSidebar}
    ></button>
  {/if}
  <section class="main">
    <header class="mobile-topbar">
      <button
        type="button"
        class="menu-button"
        aria-label="Open workspace navigation"
        aria-expanded={sidebarOpen}
        onclick={() => (sidebarOpen = true)}
      >
        ☰
      </button>
      <h1>{mobileTitle}</h1>
    </header>

    <div class="content">
      {#if uiStore.activeView === 'settings'}
        <SettingsView workspace={workspaceStore.selected} />
      {:else if workspaceStore.selected === null}
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
    </div>
  </section>
</div>

<PathPromptHost />

<style>
  .app-shell {
    display: grid;
    grid-template-columns: auto 1fr;
    height: 100dvh;
    background: var(--color-bg);
    color: var(--color-fg);
  }

  .sidebar-panel {
    min-height: 0;
  }

  .main {
    overflow: hidden;
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
  }

  .content {
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }

  .mobile-topbar,
  .sidebar-backdrop {
    display: none;
  }

  @media (max-width: 760px) {
    .app-shell {
      display: block;
      position: relative;
    }

    .sidebar-panel {
      position: fixed;
      inset: 0 auto 0 0;
      z-index: var(--z-drawer, 40);
      transform: translateX(-100%);
      transition: transform 140ms ease;
    }

    .app-shell.sidebar-open .sidebar-panel {
      transform: translateX(0);
    }

    .sidebar-backdrop {
      display: block;
      position: fixed;
      inset: 0;
      z-index: calc(var(--z-drawer, 40) - 1);
      border: 0;
      background: rgba(0, 0, 0, 0.45);
    }

    .main {
      height: 100dvh;
    }

    .mobile-topbar {
      display: flex;
      align-items: center;
      gap: var(--space-3);
      flex-shrink: 0;
      min-height: 52px;
      padding: env(safe-area-inset-top) var(--space-3) 0;
      border-bottom: 1px solid var(--color-border-subtle);
      background: var(--color-bg-elevated);
    }

    .mobile-topbar h1 {
      min-width: 0;
      margin: 0;
      font-size: var(--font-size-md);
      font-weight: 600;
      white-space: nowrap;
      overflow: hidden;
      text-overflow: ellipsis;
    }

    .menu-button {
      display: inline-flex;
      align-items: center;
      justify-content: center;
      flex: 0 0 40px;
      width: 40px;
      height: 40px;
      border: 1px solid var(--color-border-subtle);
      border-radius: var(--radius-md);
      background: var(--color-bg);
      color: var(--color-fg);
      font: inherit;
      font-size: var(--font-size-lg);
    }
  }
</style>
