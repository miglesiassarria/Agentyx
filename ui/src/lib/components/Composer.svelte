<script lang="ts">
  import { sessionStore } from '$lib/stores/session.svelte';
  import AtMentionPopover from './agents/AtMentionPopover.svelte';

  let content = $state<string>('');
  let textareaEl: HTMLTextAreaElement | null = $state(null);
  let submitting = $state<boolean>(false);
  let mentionOpen = $state<boolean>(false);
  let mentionQuery = $state<string>('');
  let mentionStart = $state<number>(-1);
  let popoverRef: AtMentionPopover | null = $state(null);

  const MAX_USER_MSG_BYTES = 1024 * 1024;

  function onKeydown(e: KeyboardEvent): void {
    // AtMention popover takes priority when open.
    if (mentionOpen && popoverRef !== null && popoverRef.handleKeydown(e)) {
      return;
    }
    // Enter submits; Shift+Enter inserts a newline.
    if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) {
      e.preventDefault();
      void submit();
      return;
    }
    // Tab cycles the primary agent (delegated to the store).
    if (e.key === 'Tab' && !e.shiftKey && !e.isComposing) {
      e.preventDefault();
      sessionStore.cyclePrimary();
    }
  }

  function onInput(e: Event): void {
    const ta = e.target as HTMLTextAreaElement;
    content = ta.value;
    // Detect `@` trigger.
    const cursor = ta.selectionStart ?? content.length;
    const before = content.slice(0, cursor);
    const atIdx = before.lastIndexOf('@');
    if (atIdx < 0) {
      mentionOpen = false;
      return;
    }
    // Only open if @ is at start or preceded by whitespace.
    const prev = atIdx === 0 ? ' ' : before[atIdx - 1];
    if (!(prev === ' ' || prev === '\n' || prev === '\t')) {
      mentionOpen = false;
      return;
    }
    const fragment = before.slice(atIdx + 1);
    // Close if a whitespace appears in the fragment.
    if (/\s/.test(fragment)) {
      mentionOpen = false;
      return;
    }
    mentionStart = atIdx;
    mentionQuery = fragment;
    mentionOpen = true;
  }

  function selectMention(agentId: string): void {
    if (mentionStart < 0) return;
    const before = content.slice(0, mentionStart);
    const after = content.slice(mentionStart + 1 + mentionQuery.length);
    content = `${before}@${agentId} ${after}`;
    mentionOpen = false;
    mentionStart = -1;
    mentionQuery = '';
    // Restore focus.
    queueMicrotask(() => textareaEl?.focus());
  }

  function closeMention(): void {
    mentionOpen = false;
    mentionStart = -1;
    mentionQuery = '';
  }

  async function submit(): Promise<void> {
    if (submitting) return;
    const value = content;
    if (value.trim().length === 0) return;
    if (value.length > MAX_USER_MSG_BYTES) {
      sessionStore.lastError = {
        code: 'input_too_large',
        message: `Message exceeds the ${MAX_USER_MSG_BYTES}-byte limit.`,
        retryable: false,
        at: new Date().toISOString(),
      };
      return;
    }
    submitting = true;
    try {
      await sessionStore.send(value);
      content = '';
      autoresize();
    } catch {
      // Error is already set on the store. Keep the draft so the
      // user can edit and retry.
    } finally {
      submitting = false;
    }
  }

  function autoresize(): void {
    if (textareaEl === null) return;
    textareaEl.style.height = 'auto';
    textareaEl.style.height = `${Math.min(textareaEl.scrollHeight, 240)}px`;
  }

  $effect(() => {
    void content;
    autoresize();
  });

  async function onStop(): Promise<void> {
    await sessionStore.abort();
  }
</script>

<div class="composer" role="group" aria-label="Message composer">
  {#if sessionStore.lastError !== null}
    <div class="error" role="alert">
      <span class="error-code">{sessionStore.lastError.code}</span>
      <span class="error-message">{sessionStore.lastError.message}</span>
      <button
        type="button"
        class="dismiss"
        onclick={() => {
          sessionStore.lastError = null;
        }}
        aria-label="Dismiss error"
      >
        ×
      </button>
    </div>
  {/if}

  <form
    class="row"
    onsubmit={(e) => {
      e.preventDefault();
      void submit();
    }}
  >
    <div class="textarea-wrap">
      {#if mentionOpen}
        <AtMentionPopover
          bind:this={popoverRef}
          subagents={sessionStore.subagents}
          query={mentionQuery}
          onSelect={selectMention}
          onClose={closeMention}
        />
      {/if}
      <textarea
        bind:this={textareaEl}
        bind:value={content}
        onkeydown={onKeydown}
        oninput={onInput}
        placeholder="Type a message… (Enter to send, Shift+Enter for newline, Tab to switch agent, @ to mention a subagent)"
        rows="1"
        disabled={sessionStore.composerDisabled && !submitting}
        aria-label="Message"
      ></textarea>
    </div>

    {#if sessionStore.composerDisabled}
      <button type="button" class="stop" onclick={onStop} aria-label="Stop the current run">
        Stop
      </button>
    {:else}
      <button
        type="submit"
        class="send"
        disabled={content.trim().length === 0 || submitting}
        aria-label="Send message"
      >
        Send
      </button>
    {/if}
  </form>
</div>

<style>
  .composer {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-3) var(--space-4);
    border-top: 1px solid var(--color-border-subtle);
    background: var(--color-bg-elevated);
  }

  .error {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-3);
    background: var(--color-bg-subtle);
    color: var(--color-danger);
    border: 1px solid var(--color-danger);
    border-radius: var(--radius-sm);
    font-size: var(--font-size-sm);
  }

  .error-code {
    font-family: var(--font-mono);
    font-weight: 600;
  }
  .error-message {
    flex: 1;
  }
  .dismiss {
    background: transparent;
    border: 0;
    color: var(--color-danger);
    font-size: var(--font-size-lg);
    cursor: pointer;
    line-height: 1;
  }

  .row {
    display: flex;
    gap: var(--space-2);
    align-items: flex-end;
  }

  .textarea-wrap {
    flex: 1;
    position: relative;
    display: flex;
  }

  textarea {
    flex: 1;
    min-height: 36px;
    max-height: 240px;
    padding: var(--space-2) var(--space-3);
    background: var(--color-bg);
    color: var(--color-fg);
    border: 1px solid var(--color-border-subtle);
    border-radius: var(--radius-md);
    font: inherit;
    font-size: var(--font-size-md);
    line-height: 1.4;
    resize: none;
    overflow-y: auto;
  }

  textarea:focus {
    outline: 2px solid var(--color-primary);
    outline-offset: -1px;
  }

  textarea:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .send,
  .stop {
    flex-shrink: 0;
    height: 36px;
    padding: 0 var(--space-4);
    border-radius: var(--radius-md);
    font: inherit;
    font-weight: 600;
    cursor: pointer;
    border: 1px solid transparent;
  }

  .send {
    background: var(--color-primary);
    color: var(--color-primary-fg);
    border-color: var(--color-primary);
  }

  .send:hover:not(:disabled) {
    filter: brightness(1.1);
  }

  .send:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .stop {
    background: var(--color-bg);
    color: var(--color-danger);
    border-color: var(--color-danger);
  }

  .stop:hover {
    background: var(--color-bg-subtle);
  }
</style>
