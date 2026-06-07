<script lang="ts">
  import { sessionStore } from '$lib/stores/session.svelte';
  import type { AgentInfoDto } from '$lib/ipc-types';
  import AgentPickerMenu from './AgentPickerMenu.svelte';

  interface Props {
    /** When true, the chip is rendered in a compact style for the
     * composer header. Defaults to true. */
    compact?: boolean;
  }

  let { compact = true }: Props = $props();

  let menuOpen = $state(false);
  let anchorEl: HTMLButtonElement | null = $state(null);

  function toggleMenu() {
    menuOpen = !menuOpen;
  }

  function closeMenu() {
    menuOpen = false;
  }

  function handleSelect(agent: AgentInfoDto) {
    void sessionStore.setActiveAgent(agent.id);
    closeMenu();
  }

  function colorFor(id: string | undefined): string {
    switch (id) {
      case 'build':
        return 'var(--ag-blue, #60a5fa)';
      case 'plan':
        return 'var(--ag-amber, #fbbf24)';
      case 'general':
        return 'var(--ag-green, #4ade80)';
      default:
        return 'var(--ag-fg-muted, #888)';
    }
  }

  function iconFor(id: string | undefined): string {
    switch (id) {
      case 'build':
        return '🔨';
      case 'plan':
        return '📋';
      case 'general':
        return '🤖';
      default:
        return '•';
    }
  }
</script>

<div class="agent-chip-wrapper" class:compact>
  <button
    bind:this={anchorEl}
    type="button"
    class="chip"
    style:--chip-color={colorFor(sessionStore.activeAgent?.id)}
    onclick={toggleMenu}
    aria-haspopup="menu"
    aria-expanded={menuOpen}
    title={sessionStore.activeAgent?.description ?? 'Active agent'}
  >
    <span class="icon" aria-hidden="true">{iconFor(sessionStore.activeAgent?.id)}</span>
    <span class="id">{sessionStore.activeAgent?.id ?? 'build'}</span>
    <span class="caret" aria-hidden="true">▾</span>
  </button>
  {#if menuOpen}
    <AgentPickerMenu
      agents={sessionStore.primaryAgents}
      activeId={sessionStore.activeAgent?.id ?? null}
      onSelect={handleSelect}
      onClose={closeMenu}
    />
  {/if}
</div>

<style>
  .agent-chip-wrapper {
    position: relative;
    display: inline-flex;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.25rem 0.5rem;
    border: 1px solid var(--chip-color, var(--ag-fg-muted, #888));
    background: transparent;
    border-radius: 4px;
    color: var(--chip-color, var(--ag-fg, #ddd));
    cursor: pointer;
    font: inherit;
    font-size: 0.85rem;
  }
  .chip:hover {
    background: color-mix(in srgb, var(--chip-color, transparent) 12%, transparent);
  }
  .icon {
    font-size: 0.95rem;
  }
  .caret {
    font-size: 0.7rem;
    opacity: 0.7;
  }
  .compact .id {
    font-weight: 500;
  }
</style>
