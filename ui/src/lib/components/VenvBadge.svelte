<script lang="ts">
  import type { VenvSpec } from '$lib/ipc-types';

  interface Props {
    venv: VenvSpec;
  }

  let { venv }: Props = $props();
</script>

<span class="badge" title={venv.path}>
  <span class="icon" aria-hidden="true">🐍</span>
  <span class="text">.venv · {venv.version}</span>
</span>

<style>
  .badge {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    padding: var(--space-1) var(--space-3);
    background: var(--color-bg-subtle);
    color: var(--color-fg-muted);
    border: 1px solid var(--color-border-subtle);
    border-radius: var(--radius-full);
    font-size: var(--font-size-sm);
    font-family: var(--font-mono);
  }

  .icon {
    font-size: var(--font-size-md);
  }
</style>
