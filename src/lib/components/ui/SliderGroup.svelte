<!--
  SliderGroup — N sliders that always sum to a fixed total (100% or 1.0). Dragging one
  redistributes the others proportionally; a per-row lock pin freezes that row during
  redistribution (spreadsheet-style). Consumers: trade-good colonization chances (7.5)
  and the dev tax/production/manpower mix (9.2).

  All redistribution math lives in ./sliderMath.ts (pure, unit-tested). Invariants:
  sum stays === total; locked rows never move; if every *other* row is locked, dragging
  the remaining one is a no-op.
-->
<script lang="ts">
  import { redistribute } from "./sliderMath";

  let {
    values = $bindable([]),
    labels = [],
    locks = $bindable([]),
    total = 100,
    orientation = "horizontal",
    showLocks = true,
    format,
    onchange,
  }: {
    /** Bindable values; must already sum to `total`. */
    values?: number[];
    labels?: string[];
    /** Bindable per-row lock flags. */
    locks?: boolean[];
    total?: number;
    orientation?: "horizontal" | "vertical";
    showLocks?: boolean;
    /** Optional value formatter for the readout (defaults to rounded number). */
    format?: (v: number) => string;
    onchange?: (values: number[]) => void;
  } = $props();

  function fmt(v: number): string {
    return format ? format(v) : String(Math.round(v));
  }

  function drag(index: number, raw: string) {
    const next = redistribute(values, index, Number(raw), locks, total);
    values = next;
    onchange?.(next);
  }

  function toggleLock(index: number) {
    const next = locks.slice();
    while (next.length < values.length) next.push(false);
    next[index] = !next[index];
    locks = next;
  }
</script>

<div class="slider-group" class:vertical={orientation === "vertical"}>
  {#each values as v, i (i)}
    <div class="row">
      {#if showLocks}
        <button
          class="lock"
          class:locked={locks[i]}
          aria-pressed={locks[i] === true}
          title={locks[i] ? "Unlock row" : "Lock row"}
          onclick={() => toggleLock(i)}
        >
          {locks[i] ? "🔒" : "🔓"}
        </button>
      {/if}
      <span class="label">{labels[i] ?? `#${i + 1}`}</span>
      <input
        type="range"
        min="0"
        max={total}
        step="any"
        value={v}
        disabled={locks[i]}
        aria-label={labels[i] ?? `Slider ${i + 1}`}
        oninput={(e) => drag(i, e.currentTarget.value)}
      />
      <span class="readout">{fmt(v)}</span>
    </div>
  {/each}
</div>

<style>
  .slider-group {
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
  }

  .slider-group.vertical {
    flex-direction: row;
    align-items: flex-end;
    gap: 0.75rem;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 0.4rem;
  }

  .slider-group.vertical .row {
    flex-direction: column;
    gap: 0.3rem;
  }

  .lock {
    flex: none;
    border: 1px solid var(--border);
    background: var(--bg-1);
    color: var(--text-1);
    font-size: 0.75rem;
    line-height: 1;
    padding: 0.15rem 0.3rem;
    cursor: pointer;
  }

  .lock.locked {
    background: var(--accent);
  }

  .label {
    flex: none;
    width: 5.5rem;
    font-size: 0.82rem;
    color: var(--text-1);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .slider-group.vertical .label {
    width: auto;
    order: 3;
    text-align: center;
  }

  input[type="range"] {
    flex: 1;
    min-width: 0;
    accent-color: var(--accent);
  }

  .slider-group.vertical input[type="range"] {
    /* Vertical orientation: rotate a horizontal range and give it height. */
    writing-mode: vertical-lr;
    direction: rtl;
    width: 1.2rem;
    height: 9rem;
    flex: none;
  }

  .readout {
    flex: none;
    width: 3rem;
    text-align: right;
    font-size: 0.8rem;
    font-variant-numeric: tabular-nums;
    color: var(--text-1);
  }

  .slider-group.vertical .readout {
    width: auto;
    order: 2;
  }
</style>
