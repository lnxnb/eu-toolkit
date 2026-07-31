<!--
  ClimatePanel — Sprint 11.1 two-slot climate selector (right-side, list-driven
  like the trade-goods list). A province has two INDEPENDENT slots: climate zone
  (tropical / arid / arctic / impassable, absence = temperate) and winter severity
  (mild / normal / severe, absence = none). Selecting an entry arms the paint
  brush for that slot; painting one slot never clobbers the other.

  Erasers are first-class entries: "Temperate" clears the zone slot, "No winter"
  clears the winter slot. Impassable carries a validation nudge (wastelands
  interact with areas/borders). In Climate mode a "Show winter tint" toggle
  overlays winter severity so both slots are visible at once.

  Windows-classic chrome to match the rest of the app.
-->
<script lang="ts">
  import type { ClimateModel, ClimateSlot, Rgb } from "$lib/climate";
  import { ZONE_COLORS, WINTER_COLORS, TEMPERATE_COLOR, WINTER_LAND } from "$lib/climate";

  let {
    model,
    counts,
    selSlot,
    selKey,
    mode,
    showWinterTint = $bindable(false),
    onselect,
  }: {
    model: ClimateModel;
    counts: Map<string, number>;
    /** The selected entry's slot, or null when nothing is selected. */
    selSlot: ClimateSlot | null;
    /** The selected entry's list key, or null for that slot's eraser entry. */
    selKey: string | null;
    mode: "climate" | "winter";
    showWinterTint?: boolean;
    onselect: (slot: ClimateSlot, key: string | null) => void;
  } = $props();

  interface Row {
    slot: ClimateSlot;
    key: string | null;
    label: string;
    color: Rgb;
    /** True for the "absence" eraser entries (Temperate / No winter). */
    eraser?: boolean;
  }

  const zoneRows: Row[] = [
    { slot: "zone", key: "tropical", label: "Tropical", color: ZONE_COLORS.tropical },
    { slot: "zone", key: "arid", label: "Arid", color: ZONE_COLORS.arid },
    { slot: "zone", key: "arctic", label: "Arctic", color: ZONE_COLORS.arctic },
    { slot: "zone", key: null, label: "Temperate (erase zone)", color: TEMPERATE_COLOR, eraser: true },
  ];
  const impassableRow: Row = {
    slot: "zone",
    key: "impassable",
    label: "Impassable (wasteland)",
    color: ZONE_COLORS.impassable,
  };
  const winterRows: Row[] = [
    { slot: "winter", key: "mild_winter", label: "Mild winter", color: WINTER_COLORS.mild_winter },
    { slot: "winter", key: "normal_winter", label: "Normal winter", color: WINTER_COLORS.normal_winter },
    { slot: "winter", key: "severe_winter", label: "Severe winter", color: WINTER_COLORS.severe_winter },
    { slot: "winter", key: null, label: "No winter (erase)", color: WINTER_LAND, eraser: true },
  ];

  function isSelected(r: Row): boolean {
    return selSlot === r.slot && selKey === r.key;
  }
  function count(r: Row): number {
    return r.key ? (counts.get(r.key) ?? 0) : 0;
  }
  function css(c: Rgb): string {
    return `rgb(${c[0]}, ${c[1]}, ${c[2]})`;
  }
</script>

<aside class="climate-panel">
  <div class="chrome">
    <span class="title">Climate</span>
    <span class="mode-tag">{mode === "climate" ? "zones" : "winter"}</span>
  </div>

  <div class="scroll">
    <section>
      <h4>Climate zones</h4>
      {#each zoneRows as r (r.label)}
        <button class="row" class:selected={isSelected(r)} class:eraser={r.eraser} onclick={() => onselect(r.slot, r.key)}>
          <span class="swatch" class:none={r.eraser} style="background: {css(r.color)}"></span>
          <span class="name">{r.label}</span>
          {#if r.key}<span class="cnt">{count(r)}</span>{/if}
        </button>
      {/each}
    </section>

    <section>
      <h4>Impassable</h4>
      <button class="row" class:selected={isSelected(impassableRow)} onclick={() => onselect(impassableRow.slot, impassableRow.key)}>
        <span class="swatch" style="background: {css(impassableRow.color)}"></span>
        <span class="name">{impassableRow.label}</span>
        <span class="cnt">{count(impassableRow)}</span>
      </button>
      {#if isSelected(impassableRow)}
        <p class="nudge">⚠ Painting impassable turns land into wasteland — it drops out of its area/region and changes borders. Re-check geography after.</p>
      {/if}
    </section>

    <section>
      <div class="winter-head">
        <h4>Winter severity</h4>
        {#if mode === "climate"}
          <label class="tint-toggle" title="Overlay winter severity as a tint over the climate zones">
            <input type="checkbox" bind:checked={showWinterTint} />
            Tint
          </label>
        {/if}
      </div>
      {#each winterRows as r (r.label)}
        <button class="row" class:selected={isSelected(r)} class:eraser={r.eraser} onclick={() => onselect(r.slot, r.key)}>
          <span class="swatch" class:none={r.eraser} style="background: {css(r.color)}"></span>
          <span class="name">{r.label}</span>
          {#if r.key}<span class="cnt">{count(r)}</span>{/if}
        </button>
      {/each}
    </section>

    <p class="hint dim">
      A province has two independent slots. Select an entry, then paint with the brush below — painting one slot never changes the other.
    </p>
  </div>
</aside>

<style>
  .climate-panel {
    position: absolute;
    top: 3rem;
    right: 0.75rem;
    bottom: 0.75rem;
    z-index: 10;
    width: 20rem;
    display: flex;
    flex-direction: column;
    background: var(--bg-2);
    border: 1px solid var(--border);
    color: var(--text-1);
    font-size: 0.9rem;
    box-shadow: 2px 3px 10px rgba(0, 0, 0, 0.4);
  }
  .chrome {
    flex: none;
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.4rem 0.6rem;
    background: var(--bg-3);
    border-bottom: 1px solid var(--border);
  }
  .title {
    font-weight: 700;
  }
  .mode-tag {
    font-size: 0.72rem;
    padding: 0.12rem 0.4rem;
    background: var(--accent);
    color: var(--text-inverse);
  }
  .scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 0.3rem 0;
  }
  section {
    padding: 0.2rem 0 0.4rem;
    border-bottom: 1px solid var(--bg-1);
  }
  h4 {
    margin: 0.3rem 0.6rem 0.3rem;
    font-size: 0.72rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-2);
  }
  .winter-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .tint-toggle {
    display: flex;
    align-items: center;
    gap: 0.3rem;
    margin-right: 0.6rem;
    font-size: 0.74rem;
    color: var(--text-1);
    cursor: pointer;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    width: 100%;
    border: none;
    background: transparent;
    color: inherit;
    font-family: inherit;
    font-size: 0.86rem;
    text-align: left;
    padding: 0.35rem 0.6rem;
    cursor: pointer;
  }
  .row:hover {
    background: rgba(255, 255, 255, 0.05);
  }
  .row.selected {
    background: rgba(74, 109, 167, 0.32);
    outline: 1px solid var(--accent);
    outline-offset: -1px;
  }
  .row.eraser .name {
    font-style: italic;
    color: var(--text-1);
  }
  .swatch {
    flex: none;
    width: 1rem;
    height: 1rem;
    border: 1px solid var(--border);
  }
  .swatch.none {
    background-image: linear-gradient(45deg, var(--bg-3) 25%, transparent 25%, transparent 75%, var(--bg-3) 75%);
  }
  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .cnt {
    flex: none;
    font-size: 0.74rem;
    font-variant-numeric: tabular-nums;
    color: var(--text-2);
  }
  .nudge {
    margin: 0.1rem 0.6rem 0.3rem;
    padding: 0.3rem 0.4rem;
    font-size: 0.74rem;
    color: var(--warn);
    background: rgba(216, 160, 32, 0.12);
    border-left: 3px solid var(--warn);
  }
  .hint {
    margin: 0.5rem 0.6rem 0.2rem;
    font-size: 0.76rem;
    line-height: 1.35;
  }
  .dim {
    color: var(--text-2);
  }
</style>
