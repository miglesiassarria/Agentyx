<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    children?: Snippet;
  }

  let { children }: Props = $props();
</script>

<main class="app-shell">
  <header class="app-header">
    <h1>Agentyx</h1>
    <span class="version">v0.1.0</span>
  </header>

  <section class="placeholder">
    <p>Bootstrap OK. La UI se implementa en Fase D.</p>
    <p class="hint">
      Tauri commands y eventos streaming se montan en F01-F05.
    </p>
    {@render children?.()}
  </section>
</main>

<style>
  .app-shell {
    display: grid;
    grid-template-rows: auto 1fr;
    height: 100%;
    background: var(--color-bg);
    color: var(--color-fg);
  }

  .app-header {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-4);
    border-bottom: 1px solid var(--color-border-subtle);
    background: var(--color-bg-elevated);
  }

  .app-header h1 {
    margin: 0;
    font-size: var(--font-size-lg);
    font-weight: 600;
  }

  .version {
    color: var(--color-fg-subtle);
    font-size: var(--font-size-sm);
    font-family: var(--font-mono);
  }

  .placeholder {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: var(--space-6);
    gap: var(--space-2);
  }

  .placeholder p {
    margin: 0;
  }

  .hint {
    color: var(--color-fg-muted);
    font-size: var(--font-size-sm);
  }
</style>
