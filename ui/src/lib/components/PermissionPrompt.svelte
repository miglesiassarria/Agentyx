<script lang="ts">
  import type { PermissionRequestDto } from '$lib/ipc-types';

  interface Props {
    /** The pending request, or null when nothing is awaiting input. */
    request: PermissionRequestDto | null;
    /** Called when the user picks a decision. */
    onrespond: (requestId: string, response: PermissionResponse) => void;
  }

  type PermissionResponse =
    | { kind: 'allowOnce' }
    | { kind: 'allowSession' }
    | { kind: 'allowAlways'; tool: string }
    | { kind: 'deny' };

  let { request, onrespond }: Props = $props();

  function handle(key: 'allowOnce' | 'allowSession' | 'allowAlways' | 'deny') {
    if (request === null) return;
    if (key === 'allowAlways') {
      onrespond(request.requestId, { kind: 'allowAlways', tool: request.tool });
    } else {
      onrespond(request.requestId, { kind: key });
    }
  }
</script>

{#if request !== null}
  <div
    class="backdrop"
    role="presentation"
    onclick={() => handle('deny')}
    onkeydown={(e) => {
      if (e.key === 'Escape') handle('deny');
    }}
  >
    <div
      class="dialog"
      role="dialog"
      aria-modal="true"
      aria-labelledby="permission-title"
      tabindex={-1}
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
    >
      <header class="header">
        <h2 id="permission-title" class="title">Permission required</h2>
        <p class="subtitle">The agent wants to call a tool that may modify your workspace.</p>
      </header>

      <dl class="meta">
        <dt>Tool</dt>
        <dd><code class="tool-name">{request.tool}</code></dd>
        <dt>Reason</dt>
        <dd>{request.reason}</dd>
        <dt>Args</dt>
        <dd><code class="args">{request.argsSummary}</code></dd>
      </dl>

      <div class="actions">
        <button type="button" class="btn btn-deny" onclick={() => handle('deny')}>Deny</button>
        <button type="button" class="btn" onclick={() => handle('allowOnce')}> Allow once </button>
        <button type="button" class="btn" onclick={() => handle('allowSession')}>
          Allow for this run
        </button>
        <button type="button" class="btn btn-primary" onclick={() => handle('allowAlways')}>
          Allow always
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.55);
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
    max-width: 560px;
    width: calc(100% - var(--space-6));
    box-shadow: 0 12px 48px rgba(0, 0, 0, 0.5);
  }

  .header {
    margin-bottom: var(--space-4);
  }

  .title {
    margin: 0 0 var(--space-1);
    font-size: var(--font-size-lg);
    font-weight: 600;
  }

  .subtitle {
    margin: 0;
    color: var(--color-fg-muted);
    font-size: var(--font-size-sm);
    line-height: 1.4;
  }

  .meta {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: var(--space-2) var(--space-4);
    margin: 0 0 var(--space-5);
    font-size: var(--font-size-sm);
  }

  .meta dt {
    color: var(--color-fg-muted);
    font-weight: 500;
  }

  .meta dd {
    margin: 0;
    color: var(--color-fg);
  }

  .tool-name,
  .args {
    font-family: var(--font-mono, ui-monospace, monospace);
    font-size: 0.9em;
    background: var(--color-bg-subtle);
    padding: 0 var(--space-1);
    border-radius: var(--radius-sm);
  }

  .args {
    word-break: break-all;
    display: inline-block;
    padding: var(--space-1) var(--space-2);
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: var(--space-2);
  }

  .btn {
    font: inherit;
    padding: var(--space-2) var(--space-3);
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

  .btn-deny {
    color: var(--color-danger);
    border-color: var(--color-danger);
    background: transparent;
  }

  .btn-deny:hover {
    background: var(--color-bg-subtle);
  }
</style>
