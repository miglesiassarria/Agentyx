<script lang="ts">
  interface Props {
    open: boolean;
    title: string;
    message: string;
    confirmLabel?: string;
    cancelLabel?: string;
    destructive?: boolean;
    onconfirm: () => void;
    oncancel: () => void;
  }

  let {
    open,
    title,
    message,
    confirmLabel = 'Confirm',
    cancelLabel = 'Cancel',
    destructive = false,
    onconfirm,
    oncancel,
  }: Props = $props();
</script>

{#if open}
  <div
    class="backdrop"
    role="presentation"
    onclick={oncancel}
    onkeydown={(e) => {
      if (e.key === 'Escape') oncancel();
    }}
  >
    <div
      class="dialog"
      role="dialog"
      aria-modal="true"
      aria-labelledby="confirm-title"
      tabindex={-1}
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
    >
      <h2 id="confirm-title" class="title">{title}</h2>
      <p class="message">{message}</p>
      <div class="actions">
        <button type="button" class="btn btn-cancel" onclick={oncancel}>
          {cancelLabel}
        </button>
        <button
          type="button"
          class="btn"
          class:btn-danger={destructive}
          class:btn-primary={!destructive}
          onclick={onconfirm}
        >
          {confirmLabel}
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.5);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: var(--z-modal);
  }

  .dialog {
    background: var(--color-bg-elevated);
    color: var(--color-fg);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    padding: var(--space-5);
    max-width: 480px;
    width: calc(100% - var(--space-6));
    box-shadow: 0 12px 48px rgba(0, 0, 0, 0.4);
  }

  .title {
    margin: 0 0 var(--space-3);
    font-size: var(--font-size-lg);
    font-weight: 600;
  }

  .message {
    margin: 0 0 var(--space-5);
    color: var(--color-fg-muted);
    line-height: 1.5;
    white-space: pre-line;
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
  }

  .btn {
    font: inherit;
    padding: var(--space-2) var(--space-4);
    border-radius: var(--radius-md);
    border: 1px solid var(--color-border);
    background: var(--color-bg);
    color: var(--color-fg);
    cursor: pointer;
  }

  .btn:hover {
    background: var(--color-bg-subtle);
  }

  .btn-primary {
    background: var(--color-primary);
    color: var(--color-primary-fg);
    border-color: var(--color-primary);
  }

  .btn-primary:hover {
    filter: brightness(1.1);
    background: var(--color-primary);
  }

  .btn-danger {
    background: var(--color-danger);
    color: var(--color-primary-fg);
    border-color: var(--color-danger);
  }

  .btn-danger:hover {
    filter: brightness(1.1);
    background: var(--color-danger);
  }

  .btn-cancel {
    background: transparent;
  }
</style>
