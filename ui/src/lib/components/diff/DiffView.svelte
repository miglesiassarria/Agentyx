<script lang="ts">
  import type { DiffPayload, DiffSummaryDto } from '$lib/ipc-types';
  import DiffBody from './DiffBody.svelte';

  interface Props {
    summary: DiffSummaryDto;
    payload?: DiffPayload | null;
  }

  let { summary, payload = null }: Props = $props();

  let collapsed = $state(false);
  const storageKey = $derived(`diff-collapse:${summary.path}`);

  // Restore collapse state from localStorage on mount.
  $effect(() => {
    try {
      collapsed = localStorage.getItem(storageKey) === '1';
    } catch {
      // localStorage may be unavailable (private mode); ignore.
    }
  });

  function toggle() {
    collapsed = !collapsed;
    try {
      localStorage.setItem(storageKey, collapsed ? '1' : '0');
    } catch {
      // ignore
    }
  }
</script>

<div class="diff-view" class:collapsed data-kind={summary.kind}>
  <button class="header" type="button" onclick={toggle} aria-expanded={!collapsed}>
    <span class="caret" aria-hidden="true">{collapsed ? '▶' : '▼'}</span>
    <span class="path" title={summary.path}>{summary.path}</span>
    <span class="counts" aria-label="added and removed lines">
      <span class="add">+{summary.additions}</span>
      <span class="del">−{summary.deletions}</span>
    </span>
  </button>
  {#if !collapsed}
    <div class="body">
      {#if payload?.isBinary}
        <div class="notice binary">
          <span class="icon" aria-hidden="true">📦</span>
          Binary file changed
          {#if payload.mime}<span class="mime">({payload.mime})</span>{/if}
        </div>
      {:else if payload}
        <DiffBody
          before={payload.before}
          after={payload.after}
          beforeTruncated={payload.beforeTruncated}
          afterTruncated={payload.afterTruncated}
        />
      {:else}
        <div class="notice">Diff preview unavailable (cold start without journal payload).</div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .diff-view {
    border: 1px solid var(--ag-border, #2a2a2a);
    border-radius: 6px;
    background: var(--ag-surface, #181818);
    margin: 0.5rem 0;
    font-size: 0.875rem;
  }
  .header {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    width: 100%;
    padding: 0.5rem 0.75rem;
    background: transparent;
    border: 0;
    color: inherit;
    cursor: pointer;
    text-align: left;
    font: inherit;
  }
  .header:hover {
    background: var(--ag-surface-hover, #222);
  }
  .caret {
    font-size: 0.7rem;
    color: var(--ag-fg-muted, #888);
    width: 1ch;
  }
  .path {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: ui-monospace, 'SFMono-Regular', Menlo, monospace;
  }
  .counts {
    font-variant-numeric: tabular-nums;
    font-size: 0.8rem;
  }
  .counts .add {
    color: var(--ag-green, #4ade80);
    margin-right: 0.5rem;
  }
  .counts .del {
    color: var(--ag-red, #f87171);
  }
  .body {
    border-top: 1px solid var(--ag-border, #2a2a2a);
    padding: 0.5rem 0;
  }
  .notice {
    padding: 0.75rem 1rem;
    color: var(--ag-fg-muted, #888);
  }
  .notice.binary {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }
  .mime {
    font-family: ui-monospace, 'SFMono-Regular', Menlo, monospace;
    font-size: 0.8rem;
  }
</style>
