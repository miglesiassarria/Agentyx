<script lang="ts">
  import { onMount } from 'svelte';

  import { sessionStore } from '$lib/stores/session.svelte';
  import type { WorkspaceId } from '$lib/ipc-types';

  import Composer from './Composer.svelte';
  import DiffsSidePanel from './diff/DiffsSidePanel.svelte';
  import MessageList from './MessageList.svelte';
  import AgentChip from './agents/AgentChip.svelte';
  import { installTabCycle } from '$lib/stores/tab-cycle.svelte';

  onMount(() => {
    installTabCycle();
  });

  interface Props {
    workspaceId: WorkspaceId;
  }

  let { workspaceId }: Props = $props();

  onMount(async () => {
    await sessionStore.attach(workspaceId);
    // Auto-create a session on first mount (single-session per workspace in v0.1).
    if (sessionStore.activeSession === null) {
      await sessionStore.createSession();
    } else {
      await sessionStore.loadHistory(sessionStore.activeSession.id);
    }
  });
</script>

<section class="chat" aria-label="Chat">
  <header class="header">
    <div class="title-block">
      <AgentChip />
      <h2 class="title">{sessionStore.activeSession?.title ?? 'New session'}</h2>
    </div>

    <div class="actions">
      <DiffsSidePanel
        sessionId={sessionStore.activeSession?.id ?? ''}
        onJump={(id) => {
          // Scroll to the diff in MessageList. The store keeps
          // a list of recent tool calls; we dispatch a custom
          // event so the list can highlight it.
          window.dispatchEvent(
            new CustomEvent('agentyx:jump-to-diff', { detail: { toolCallId: id } }),
          );
        }}
      />
      <span class="status" data-status={sessionStore.runStatus}>
        {sessionStore.runStatus}
      </span>
    </div>
  </header>

  <MessageList />

  <Composer />
</section>

<style>
  .chat {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    background: var(--color-bg);
  }

  .header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-4);
    border-bottom: 1px solid var(--color-border-subtle);
    background: var(--color-bg-elevated);
  }

  .title-block {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    min-width: 0;
  }

  .title {
    margin: 0;
    font-size: var(--font-size-md);
    font-weight: 600;
    color: var(--color-fg);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .actions {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex-shrink: 0;
  }

  .status {
    font-size: var(--font-size-xs);
    color: var(--color-fg-subtle);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    padding: var(--space-1) var(--space-2);
    border-radius: var(--radius-sm);
    background: var(--color-bg-subtle);
  }

  .status[data-status='running'],
  .status[data-status='starting'] {
    color: var(--color-primary);
  }
  .status[data-status='error'] {
    color: var(--color-danger);
  }
  .status[data-status='completed'] {
    color: var(--color-success);
  }
  .status[data-status='aborted'] {
    color: var(--color-warning);
  }

  @media (max-width: 760px) {
    .header {
      align-items: flex-start;
      flex-direction: column;
      gap: var(--space-2);
      padding: var(--space-2) var(--space-3);
    }

    .title-block,
    .actions {
      width: 100%;
    }

    .actions {
      justify-content: space-between;
    }

    .title {
      font-size: var(--font-size-sm);
    }

    .status {
      letter-spacing: 0;
    }
  }
</style>
