<script lang="ts">
  import { tick } from 'svelte';

  import { sessionStore } from '$lib/stores/session.svelte';

  let listEl: HTMLDivElement | null = $state(null);
  let autoScroll = $state<boolean>(true);

  // Auto-scroll to bottom on new messages, unless the user has
  // scrolled up. We detect the latter by checking if the bottom
  // is in view at the time of mutation.
  $effect(() => {
    // Re-run on messages or status changes.
    void sessionStore.messages.length;
    void sessionStore.runStatus;
    if (!autoScroll) return;
    void scrollToBottom();
  });

  async function scrollToBottom(): Promise<void> {
    await tick();
    if (listEl === null) return;
    listEl.scrollTop = listEl.scrollHeight;
  }

  function onScroll(): void {
    if (listEl === null) return;
    const distanceFromBottom = listEl.scrollHeight - (listEl.scrollTop + listEl.clientHeight);
    autoScroll = distanceFromBottom < 32;
  }
</script>

<div
  class="list"
  bind:this={listEl}
  onscroll={onScroll}
  role="log"
  aria-live="polite"
  aria-label="Chat messages"
>
  {#if sessionStore.hydrating && sessionStore.messages.length === 0}
    <p class="placeholder">Loading messages…</p>
  {:else if sessionStore.messages.length === 0}
    <p class="placeholder">
      Send a message to start the conversation. The agent will respond in real time.
    </p>
  {:else}
    {#each sessionStore.messages as message (message.id)}
      <article class="message" data-role={message.role} data-status={message.status}>
        <div class="bubble">
          <pre class="content">{message.content}</pre>
          {#if message.isStreaming}
            <span class="cursor" aria-hidden="true">▍</span>
          {/if}
        </div>
        <div class="meta">
          <span class="role">{message.role}</span>
          {#if message.status !== 'complete'}
            <span class="status">· {message.status}</span>
          {/if}
        </div>
      </article>
    {/each}
  {/if}
</div>

{#if !autoScroll}
  <button type="button" class="jump-latest" onclick={scrollToBottom}>↓ Jump to latest</button>
{/if}

<style>
  .list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: var(--space-4) var(--space-5);
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .placeholder {
    margin: auto;
    color: var(--color-fg-subtle);
    font-size: var(--font-size-sm);
    text-align: center;
    max-width: 32em;
  }

  .message {
    display: flex;
    flex-direction: column;
    max-width: 80%;
  }

  .message[data-role='user'] {
    align-self: flex-end;
    align-items: flex-end;
  }

  .message[data-role='assistant'] {
    align-self: flex-start;
    align-items: flex-start;
  }

  .message[data-role='system'],
  .message[data-role='tool_result'] {
    align-self: center;
    align-items: center;
    max-width: 100%;
  }

  .bubble {
    position: relative;
    padding: var(--space-2) var(--space-4);
    border-radius: var(--radius-md);
    border: 1px solid var(--color-border-subtle);
    background: var(--color-bg-elevated);
    word-break: break-word;
  }

  .message[data-role='user'] .bubble {
    background: var(--color-primary);
    color: var(--color-primary-fg);
    border-color: var(--color-primary);
  }

  .message[data-status='streaming'] .bubble {
    border-color: var(--color-primary);
  }
  .message[data-status='aborted'] .bubble {
    border-color: var(--color-warning);
  }
  .message[data-status='error'] .bubble {
    border-color: var(--color-danger);
  }

  .content {
    margin: 0;
    font-family: var(--font-sans);
    font-size: var(--font-size-md);
    line-height: 1.5;
    white-space: pre-wrap;
    background: transparent;
    border: 0;
    padding: 0;
  }

  .cursor {
    display: inline-block;
    color: var(--color-primary);
    animation: blink 1s steps(1) infinite;
  }

  @keyframes blink {
    50% {
      opacity: 0;
    }
  }

  .meta {
    margin-top: var(--space-1);
    font-size: var(--font-size-xs);
    color: var(--color-fg-subtle);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .message[data-role='user'] .meta {
    text-align: right;
  }

  .role {
    font-weight: 600;
  }

  .status {
    margin-left: var(--space-1);
    text-transform: lowercase;
  }

  .jump-latest {
    position: absolute;
    bottom: var(--space-24);
    left: 50%;
    transform: translateX(-50%);
    padding: var(--space-1) var(--space-3);
    background: var(--color-bg-elevated);
    color: var(--color-fg);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-full);
    font-size: var(--font-size-xs);
    cursor: pointer;
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.05);
  }

  .jump-latest:hover {
    background: var(--color-bg-subtle);
  }
</style>
