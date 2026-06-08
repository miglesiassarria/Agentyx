<script lang="ts">
  import { pathPromptStore, type PathPromptResult } from '$lib/stores/path-prompt.svelte';

  function isAbsolutePath(value: string): boolean {
    if (value.length === 0) return false;
    if (value.startsWith('/')) return true;
    if (/^[A-Za-z]:[\\/]/.test(value)) return true;
    if (value.startsWith('~')) return true;
    return false;
  }

  let pathValue = $state<string>(pathPromptStore.pending?.defaultPath ?? '');
  let labelValue = $state<string>(pathPromptStore.pending?.defaultLabel ?? '');
  let inputEl: HTMLInputElement | null = $state(null);
  let error = $state<string | null>(null);

  $effect(() => {
    const pending = pathPromptStore.pending;
    if (pending !== null) {
      pathValue = pending.defaultPath ?? '';
      labelValue = pending.defaultLabel ?? '';
      error = null;
      // Focus the input on the next tick so jsdom-style tests can
      // reason about it, and so the real browser focuses the
      // primary field immediately.
      queueMicrotask(() => inputEl?.focus());
    }
  });

  const trimmedPath = $derived(pathValue.trim());
  const trimmedLabel = $derived(labelValue.trim());
  const isAbsolute = $derived(isAbsolutePath(trimmedPath));
  const canSubmit = $derived(trimmedPath.length > 0 && isAbsolute);

  function submit(event: Event): void {
    event.preventDefault();
    if (!canSubmit) {
      error =
        pathPromptStore.pending?.requireAbsolute === true
          ? 'Enter an absolute path (e.g. /Users/you/project or C:\\Users\\you\\project).'
          : 'Path is required.';
      return;
    }
    const result: PathPromptResult = {
      path: trimmedPath,
      label:
        pathPromptStore.pending?.showLabel === true && trimmedLabel.length > 0
          ? trimmedLabel
          : null,
    };
    pathPromptStore.submit(result);
  }

  function cancel(): void {
    pathPromptStore.cancel();
  }

  function handleKeydown(event: KeyboardEvent): void {
    if (event.key === 'Escape') {
      event.preventDefault();
      cancel();
    }
  }
</script>

{#if pathPromptStore.pending !== null}
  <div class="backdrop" role="presentation" onclick={cancel} onkeydown={handleKeydown}>
    <div
      class="dialog"
      role="dialog"
      aria-modal="true"
      aria-labelledby="path-prompt-title"
      tabindex={-1}
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
    >
      <form onsubmit={submit} class="form">
        <h2 id="path-prompt-title" class="title">{pathPromptStore.pending.title}</h2>
        <p class="hint">{pathPromptStore.pending.hint}</p>

        <label class="field">
          <span class="label">Path</span>
          <input
            bind:this={inputEl}
            bind:value={pathValue}
            type="text"
            inputmode="text"
            autocomplete="off"
            spellcheck="false"
            placeholder="/absolute/path/to/folder"
            aria-invalid={!isAbsolute && trimmedPath.length > 0}
            data-testid="path-prompt-input"
          />
        </label>

        {#if pathPromptStore.pending.showLabel}
          <label class="field">
            <span class="label">Label <span class="optional">(optional)</span></span>
            <input
              bind:value={labelValue}
              type="text"
              autocomplete="off"
              placeholder="e.g. shared lib"
              data-testid="path-prompt-label"
            />
          </label>
        {/if}

        {#if error}
          <p class="error" role="alert" data-testid="path-prompt-error">{error}</p>
        {/if}

        <div class="actions">
          <button type="button" class="btn btn-cancel" onclick={cancel}>Cancel</button>
          <button
            type="submit"
            class="btn btn-primary"
            disabled={!canSubmit}
            data-testid="path-prompt-submit"
          >
            Confirm
          </button>
        </div>
      </form>
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
    max-width: 520px;
    width: calc(100% - var(--space-6));
    box-shadow: 0 12px 48px rgba(0, 0, 0, 0.4);
  }

  .form {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .title {
    margin: 0;
    font-size: var(--font-size-lg);
    font-weight: 600;
  }

  .hint {
    margin: 0;
    color: var(--color-fg-muted);
    font-size: var(--font-size-sm);
    line-height: 1.5;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }

  .label {
    font-size: var(--font-size-sm);
    color: var(--color-fg-muted);
    font-weight: 500;
  }

  .optional {
    color: var(--color-fg-subtle);
    font-weight: 400;
  }

  input[type='text'] {
    font: inherit;
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-bg);
    color: var(--color-fg);
  }

  input[type='text']:focus {
    outline: 2px solid var(--color-primary);
    outline-offset: -1px;
    border-color: var(--color-primary);
  }

  input[type='text'][aria-invalid='true'] {
    border-color: var(--color-danger);
  }

  .error {
    margin: 0;
    color: var(--color-danger);
    font-size: var(--font-size-sm);
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
    margin-top: var(--space-2);
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

  .btn:hover:not(:disabled) {
    background: var(--color-bg-subtle);
  }

  .btn-primary {
    background: var(--color-primary);
    color: var(--color-primary-fg);
    border-color: var(--color-primary);
  }

  .btn-primary:hover:not(:disabled) {
    filter: brightness(1.1);
  }

  .btn-cancel {
    background: transparent;
  }

  .btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  @media (max-width: 760px) {
    .backdrop {
      align-items: flex-end;
      padding: var(--space-3);
    }

    .dialog {
      width: 100%;
      max-width: none;
      max-height: calc(100dvh - var(--space-6));
      overflow: auto;
      padding: var(--space-4);
      border-radius: var(--radius-lg) var(--radius-lg) 0 0;
    }

    input[type='text'] {
      font-size: 16px;
    }

    .actions {
      flex-direction: column-reverse;
    }

    .btn {
      width: 100%;
    }
  }
</style>
