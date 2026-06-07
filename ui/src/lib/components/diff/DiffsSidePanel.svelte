<script lang="ts">
  import { diffs } from '$lib/ipc';
  import type { DiffSummaryDto, SessionId } from '$lib/ipc-types';

  interface Props {
    sessionId: SessionId;
    onJump?: (toolCallId: string) => void;
  }

  let { sessionId, onJump }: Props = $props();

  let items = $state<DiffSummaryDto[]>([]);
  let filter = $state('');
  let loading = $state(false);
  let error = $state<string | null>(null);
  let open = $state(true);

  $effect(() => {
    if (!sessionId) return;
    void reload();
  });

  async function reload() {
    loading = true;
    error = null;
    try {
      items = await diffs.listPending(sessionId);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
      items = [];
    } finally {
      loading = false;
    }
  }

  let visible = $derived(
    filter ? items.filter((d) => d.path.toLowerCase().includes(filter.toLowerCase())) : items,
  );
</script>

<aside class="diffs-side-panel" class:open aria-label="Pending diffs">
  <button
    class="toggle"
    type="button"
    onclick={() => (open = !open)}
    aria-expanded={open}
    title="Toggle diffs side panel"
  >
    <span aria-hidden="true">≡</span>
    Diffs{items.length > 0 ? ` (${items.length})` : ''}
  </button>
  {#if open}
    <div class="panel">
      <input
        type="search"
        placeholder="Search diffs..."
        bind:value={filter}
        aria-label="Filter diffs by path"
      />
      {#if loading}
        <div class="state">Loading…</div>
      {:else if error}
        <div class="state error">{error}</div>
      {:else if visible.length === 0}
        <div class="state">No diffs{filter ? ' match filter' : ' yet'}.</div>
      {:else}
        <ul>
          {#each visible as d (d.toolCallId)}
            <li>
              <button
                type="button"
                class="row"
                onclick={() => onJump?.(d.toolCallId)}
                title={d.path}
              >
                <span class="path">{d.path}</span>
                <span class="counts">
                  <span class="add">+{d.additions}</span>
                  <span class="del">−{d.deletions}</span>
                </span>
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    </div>
  {/if}
</aside>

<style>
  .diffs-side-panel {
    position: relative;
    font-size: 0.85rem;
  }
  .toggle {
    background: var(--ag-surface, #181818);
    color: var(--ag-fg, #ddd);
    border: 1px solid var(--ag-border, #2a2a2a);
    border-radius: 4px;
    padding: 0.35rem 0.6rem;
    cursor: pointer;
    font: inherit;
  }
  .toggle:hover {
    background: var(--ag-surface-hover, #222);
  }
  .panel {
    position: absolute;
    right: 0;
    top: calc(100% + 0.25rem);
    width: 22rem;
    max-height: 24rem;
    background: var(--ag-surface, #181818);
    border: 1px solid var(--ag-border, #2a2a2a);
    border-radius: 6px;
    box-shadow: 0 6px 18px rgba(0, 0, 0, 0.4);
    display: flex;
    flex-direction: column;
    overflow: hidden;
    z-index: 10;
  }
  .panel input[type='search'] {
    border: 0;
    border-bottom: 1px solid var(--ag-border, #2a2a2a);
    background: transparent;
    color: inherit;
    padding: 0.5rem 0.75rem;
    font: inherit;
    outline: none;
  }
  .panel ul {
    list-style: none;
    margin: 0;
    padding: 0.25rem 0;
    overflow-y: auto;
  }
  .panel li {
    padding: 0;
  }
  .row {
    display: flex;
    align-items: center;
    width: 100%;
    padding: 0.35rem 0.75rem;
    background: transparent;
    border: 0;
    color: inherit;
    cursor: pointer;
    text-align: left;
    font: inherit;
    gap: 0.5rem;
  }
  .row:hover {
    background: var(--ag-surface-hover, #222);
  }
  .path {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: ui-monospace, 'SFMono-Regular', Menlo, monospace;
    font-size: 0.8rem;
  }
  .counts {
    font-variant-numeric: tabular-nums;
    font-size: 0.75rem;
  }
  .counts .add {
    color: var(--ag-green, #4ade80);
    margin-right: 0.4rem;
  }
  .counts .del {
    color: var(--ag-red, #f87171);
  }
  .state {
    padding: 0.75rem;
    color: var(--ag-fg-muted, #888);
    font-size: 0.8rem;
  }
  .state.error {
    color: var(--ag-red, #f87171);
  }
</style>
