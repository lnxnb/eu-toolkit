<!--
  MapBrush — the brush size control revealed next to the bottom toolbar while a
  brush tool is armed (Phase 0.5 / SPRINT 1.4b). Logarithmic slider over the
  brush DIAMETER in map pixels (1 px point → several hundred px region-scale),
  with `[` / `]` keyboard nudge handled by the host (MapView). The circle
  outline cursor and the actual painting live in MapView; this is just the size
  chrome so the same component can front every future paint tool.
-->
<script lang="ts">
  import { sliderToSize, sizeToSlider } from "$lib/brush";

  let {
    size = $bindable(12),
  }: {
    /** Brush diameter in map pixels (bindable, persisted by the host). */
    size?: number;
  } = $props();

  // The <input type=range> works in slider-position space (0..1000) so the
  // logarithmic mapping is smooth; size is derived on input.
  let pos = $derived(Math.round(sizeToSlider(size) * 1000));

  function onInput(e: Event) {
    const p = Number((e.currentTarget as HTMLInputElement).value) / 1000;
    size = sliderToSize(p);
  }
</script>

<div class="brush">
  <span class="lbl">Brush</span>
  <input
    type="range"
    min="0"
    max="1000"
    value={pos}
    oninput={onInput}
    title="Brush size ({size} px) — [ and ] to resize"
    aria-label="Brush size"
  />
  <span class="val">{size}px</span>
</div>

<style>
  .brush {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    font-size: 0.82rem;
    color: var(--text-1);
  }

  .lbl {
    text-transform: uppercase;
    letter-spacing: 0.04em;
    font-size: 0.72rem;
    color: var(--text-2);
  }

  input[type="range"] {
    width: 9rem;
    accent-color: var(--accent);
    cursor: pointer;
  }

  .val {
    min-width: 3rem;
    text-align: right;
    font-variant-numeric: tabular-nums;
    color: var(--text-2);
  }

  /* Range bounds hint (referenced so tooling knows they exist). */
  input[type="range"]::-webkit-slider-runnable-track {
    background: var(--bg-2);
  }
</style>
