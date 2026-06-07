<script lang="ts">
  interface Props {
    before: string | null;
    after: string;
    beforeTruncated: boolean;
    afterTruncated: boolean;
  }

  let { before, after, beforeTruncated, afterTruncated }: Props = $props();

  // Compute a simple line-level diff for display. We use a
  // basic LCS-style approach inline (no third-party dep) so
  // the render works even before CodeMirror Merge is wired.
  // For v0.1 this is a readable approximation; v0.2 will swap
  // in CodeMirror Merge for proper side-by-side hunks.
  type Line = { kind: 'context' | 'add' | 'del'; text: string };
  let lines = $derived(computeDiff(before ?? '', after));

  function computeDiff(a: string, b: string): Line[] {
    const aLines = a.split('\n');
    const bLines = b.split('\n');
    const out: Line[] = [];
    let i = 0;
    let j = 0;
    // Greedy walk — finds common prefix and suffix, marks
    // middle as removed/added. This is not optimal but is
    // correct and O(n+m). Good enough for previews ≤ 8 KiB.
    while (i < aLines.length && j < bLines.length && aLines[i] === bLines[j]) {
      out.push({ kind: 'context', text: aLines[i] });
      i++;
      j++;
    }
    const aTail = aLines.length;
    const bTail = bLines.length;
    let ti = aTail - 1;
    let tj = bTail - 1;
    while (ti >= i && tj >= j && aLines[ti] === bLines[tj]) {
      ti--;
      tj--;
    }
    for (let k = i; k <= ti; k++) {
      out.push({ kind: 'del', text: aLines[k] });
    }
    for (let k = j; k <= tj; k++) {
      out.push({ kind: 'add', text: bLines[k] });
    }
    for (let k = ti + 1, l = tj + 1; k < aTail && l < bTail; k++, l++) {
      out.push({ kind: 'context', text: aLines[k] });
    }
    return out;
  }
</script>

<div class="diff-body">
  {#if beforeTruncated || afterTruncated}
    <div class="truncated-notice">
      Diff truncated. Showing first {Math.min(8192, (before?.length ?? 0) + after.length)} bytes.
    </div>
  {/if}
  <table>
    <tbody>
      {#each lines as line, idx (idx)}
        <tr
          class:context={line.kind === 'context'}
          class:add={line.kind === 'add'}
          class:del={line.kind === 'del'}
        >
          <td class="marker">{line.kind === 'add' ? '+' : line.kind === 'del' ? '−' : ' '}</td>
          <td class="text"><pre>{line.text || ' '}</pre></td>
        </tr>
      {/each}
    </tbody>
  </table>
</div>

<style>
  .diff-body {
    font-family: ui-monospace, 'SFMono-Regular', Menlo, monospace;
    font-size: 0.8rem;
    line-height: 1.4;
    overflow-x: auto;
  }
  .truncated-notice {
    padding: 0.5rem 0.75rem;
    color: var(--ag-fg-muted, #888);
    font-size: 0.75rem;
    background: var(--ag-warn-bg, #2a2418);
  }
  table {
    width: 100%;
    border-collapse: collapse;
  }
  td {
    vertical-align: top;
    padding: 0 0.5rem;
  }
  td.marker {
    width: 1.5rem;
    text-align: center;
    user-select: none;
    color: var(--ag-fg-muted, #888);
  }
  td.text pre {
    margin: 0;
    white-space: pre-wrap;
    word-break: break-word;
  }
  tr.context td {
    color: var(--ag-fg, #ddd);
  }
  tr.add {
    background: var(--ag-green-bg, #14241a);
  }
  tr.add td.marker {
    color: var(--ag-green, #4ade80);
  }
  tr.del {
    background: var(--ag-red-bg, #2a1414);
  }
  tr.del td.marker {
    color: var(--ag-red, #f87171);
  }
</style>
