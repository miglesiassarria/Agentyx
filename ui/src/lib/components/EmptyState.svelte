<script lang="ts">
  interface Props {
    title: string;
    message?: string;
    actionLabel?: string;
    onaction?: () => void;
  }

  let { title, message, actionLabel, onaction }: Props = $props();
</script>

<div class="empty">
  <h2 class="title">{title}</h2>
  {#if message}
    <p class="message">{message}</p>
  {/if}
  {#if actionLabel && onaction}
    <button type="button" class="action" onclick={onaction}>
      {actionLabel}
    </button>
  {/if}
</div>

<style>
  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    gap: var(--space-3);
    padding: var(--space-6);
    color: var(--color-fg-muted);
    text-align: center;
  }

  .title {
    margin: 0;
    font-size: var(--font-size-xl);
    font-weight: 600;
    color: var(--color-fg);
  }

  .message {
    margin: 0;
    max-width: 36em;
    line-height: 1.5;
  }

  .action {
    margin-top: var(--space-3);
    padding: var(--space-2) var(--space-5);
    background: var(--color-primary);
    color: var(--color-primary-fg);
    border: 1px solid var(--color-primary);
    border-radius: var(--radius-md);
    font: inherit;
    cursor: pointer;
  }

  .action:hover {
    filter: brightness(1.1);
  }
</style>
