<script lang="ts">
  import type { AgentInfoDto } from '$lib/ipc-types';

  interface Props {
    /** Subagents to show (mode === 'subagent' and !hidden). */
    subagents: AgentInfoDto[];
    /** Current query text after the `@` (e.g. `ge` for `@ge`). */
    query: string;
    /** Called with the selected agent id. */
    onSelect: (agentId: string) => void;
    /** Called when the user dismisses the popover (Esc, click outside). */
    onClose: () => void;
  }

  let { subagents, query, onSelect, onClose }: Props = $props();

  let activeIndex = $state(0);

  let filtered = $derived(
    subagents.filter((a) => a.id.toLowerCase().startsWith(query.toLowerCase())),
  );

  $effect(() => {
    // Reset selection when the filtered list changes.
    if (activeIndex >= filtered.length) {
      activeIndex = 0;
    }
  });

  export function handleKeydown(e: KeyboardEvent): boolean {
    if (filtered.length === 0) return false;
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      activeIndex = (activeIndex + 1) % filtered.length;
      return true;
    }
    if (e.key === 'ArrowUp') {
      e.preventDefault();
      activeIndex = (activeIndex - 1 + filtered.length) % filtered.length;
      return true;
    }
    if (e.key === 'Enter' || e.key === 'Tab') {
      e.preventDefault();
      onSelect(filtered[activeIndex].id);
      return true;
    }
    if (e.key === 'Escape') {
      e.preventDefault();
      onClose();
      return true;
    }
    return false;
  }
</script>

<div class="at-mention-popover" role="listbox" aria-label="Subagent suggestions">
  <div class="header">Subagents</div>
  {#if filtered.length === 0}
    <div class="empty">No subagents{query ? ` match "@${query}"` : ' available'}.</div>
  {:else}
    {#each filtered as agent, idx (agent.id)}
      <button
        type="button"
        class="item"
        class:active={idx === activeIndex}
        role="option"
        aria-selected={idx === activeIndex}
        onclick={() => onSelect(agent.id)}
        title={agent.description ?? ''}
      >
        <span class="dot" aria-hidden="true">🤖</span>
        <span class="name">{agent.id}</span>
        {#if agent.description}
          <span class="desc">{agent.description}</span>
        {/if}
      </button>
    {/each}
  {/if}
</div>

<style>
  .at-mention-popover {
    position: absolute;
    bottom: calc(100% + 0.25rem);
    left: 0.5rem;
    z-index: 20;
    min-width: 18rem;
    max-width: 24rem;
    background: var(--ag-surface, #181818);
    border: 1px solid var(--ag-border, #2a2a2a);
    border-radius: 6px;
    box-shadow: 0 6px 18px rgba(0, 0, 0, 0.4);
    padding: 0.25rem 0;
    font-size: 0.85rem;
  }
  .header {
    padding: 0.4rem 0.75rem;
    color: var(--ag-fg-muted, #888);
    font-size: 0.75rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    border-bottom: 1px solid var(--ag-border, #2a2a2a);
  }
  .empty {
    padding: 0.75rem;
    color: var(--ag-fg-muted, #888);
  }
  .item {
    display: flex;
    align-items: baseline;
    gap: 0.5rem;
    width: 100%;
    padding: 0.4rem 0.75rem;
    background: transparent;
    border: 0;
    color: inherit;
    cursor: pointer;
    text-align: left;
    font: inherit;
  }
  .item.active,
  .item:hover {
    background: var(--ag-surface-hover, #222);
  }
  .dot {
    font-size: 0.95rem;
  }
  .name {
    font-weight: 500;
  }
  .desc {
    color: var(--ag-fg-muted, #888);
    font-size: 0.75rem;
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
