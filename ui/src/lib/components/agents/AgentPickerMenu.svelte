<script lang="ts">
  import type { AgentInfoDto } from '$lib/ipc-types';

  interface Props {
    agents: AgentInfoDto[];
    activeId: string | null;
    onSelect: (agent: AgentInfoDto) => void;
    onClose: () => void;
  }

  let { agents, activeId, onSelect, onClose }: Props = $props();

  function handleClickOutside(e: MouseEvent) {
    const target = e.target as HTMLElement;
    if (target.closest('.agent-picker-menu')) return;
    onClose();
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      onClose();
    }
  }
</script>

<svelte:window onclick={handleClickOutside} onkeydown={handleKeydown} />

<div class="agent-picker-menu" role="menu" aria-label="Pick primary agent">
  <div class="header">Primary agents</div>
  {#if agents.length === 0}
    <div class="empty">No primary agents available.</div>
  {:else}
    {#each agents as agent (agent.id)}
      <button
        type="button"
        class="item"
        class:active={agent.id === activeId}
        role="menuitem"
        onclick={() => onSelect(agent)}
        title={agent.description ?? ''}
      >
        <span class="dot" class:on={agent.id === activeId} aria-hidden="true"></span>
        <span class="name">{agent.id}</span>
        {#if agent.description}
          <span class="desc">{agent.description}</span>
        {/if}
      </button>
    {/each}
  {/if}
</div>

<style>
  .agent-picker-menu {
    position: absolute;
    top: calc(100% + 0.25rem);
    left: 0;
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
  .item:hover,
  .item:focus {
    background: var(--ag-surface-hover, #222);
    outline: none;
  }
  .item.active {
    color: var(--ag-fg, #ddd);
  }
  .dot {
    width: 0.5rem;
    height: 0.5rem;
    border-radius: 50%;
    background: transparent;
    border: 1px solid var(--ag-fg-muted, #888);
    flex-shrink: 0;
  }
  .dot.on {
    background: var(--ag-fg, #ddd);
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
