<!--
  SimpleTerrainList — Sprint 11.2 right-side terrain list (trade-goods layout).
  One row per terrain category in map/terrain.txt definition order: color swatch,
  localized name, effective province count, and a subrow with the gameplay
  modifier summary (movement / defence / dev cost / supply — combat width has no
  per-category source, so it is omitted). Select a row to paint terrain_override
  with the brush; the "Auto (from terrain.bmp)" entry at the top is the eraser
  that removes the override so the province reverts to its raster class.

  Hover/click status (effective terrain + override-vs-auto) shows at the bottom.
  Windows-classic chrome to match the rest of the app.
-->
<script lang="ts">
  import { AUTO_KEY, terrainModifierSummary, type TerrainCategory, type Rgb } from "./types";
  import type { ModifierRow } from "$lib/components/ui";
  import TerrainPropertiesEditor from "./TerrainPropertiesEditor.svelte";

  let {
    categories,
    counts,
    selectedKey,
    hover,
    onselect,
    oncommitModifiers,
  }: {
    categories: TerrainCategory[];
    /** Category key → effective province count (mode-data + pending fold). */
    counts: Map<string, number>;
    /** Selected category key, or AUTO_KEY for the eraser, or null. */
    selectedKey: string | null;
    /** The province under the cursor / last clicked, for the status footer. */
    hover: { id: number; terrain: string; name: string; isOverride: boolean } | null;
    onselect: (key: string) => void;
    /** Commit edited gameplay modifiers for a category (S2.7). */
    oncommitModifiers?: (cat: TerrainCategory, rows: ModifierRow[]) => void;
  } = $props();

  function css(c: Rgb): string {
    return `rgb(${c[0]}, ${c[1]}, ${c[2]})`;
  }

  // The selected real category (not the Auto eraser / nothing) whose properties
  // the S2.7 editor targets.
  const selectedCategory = $derived(
    selectedKey && selectedKey !== AUTO_KEY
      ? (categories.find((c) => c.key === selectedKey) ?? null)
      : null,
  );
</script>

<aside class="terrain-panel">
  <div class="chrome">
    <span class="title">Simple Terrain</span>
    <span class="badge">{categories.length}</span>
  </div>

  <div class="scroll">
    <!-- Auto (from terrain.bmp) eraser: removes the override -->
    <button class="row auto" class:selected={selectedKey === AUTO_KEY} onclick={() => onselect(AUTO_KEY)}>
      <span class="swatch none" aria-hidden="true">▦</span>
      <span class="sub1">
        <span class="name">Auto (from terrain.bmp)</span>
      </span>
      <span class="sub2 dim">Removes the override — reverts to the raster class</span>
    </button>

    {#each categories as c (c.key)}
      <button class="row" class:selected={selectedKey === c.key} onclick={() => onselect(c.key)}>
        <span class="swatch" style="background: {css(c.color)}"></span>
        <span class="sub1">
          <span class="name">
            {c.name}
            {#if c.isWater}<span class="tag water">water</span>{/if}
          </span>
          <span class="cnt">{counts.get(c.key) ?? 0}</span>
        </span>
        {#if terrainModifierSummary(c)}
          <span class="sub2">{terrainModifierSummary(c)}</span>
        {/if}
      </button>
    {/each}
  </div>

  {#if selectedCategory && oncommitModifiers}
    <div class="props-panel">
      <div class="props-head">
        <span class="props-title">{selectedCategory.name} properties</span>
      </div>
      <div class="props-body">
        {#key selectedCategory.key}
          <TerrainPropertiesEditor
            category={selectedCategory}
            oncommit={(rows) => oncommitModifiers(selectedCategory, rows)}
          />
        {/key}
      </div>
    </div>
  {/if}

  <div class="status">
    {#if hover}
      <span class="s-id">#{hover.id}</span>
      <span class="s-name">{hover.name}</span>
      <span class="s-flag" class:override={hover.isOverride}>
        {hover.isOverride ? "override" : "auto"}
      </span>
    {:else}
      <span class="dim">Hover a province for its effective terrain</span>
    {/if}
  </div>
</aside>

<style>
  .terrain-panel {
    position: absolute;
    top: 3rem;
    right: 0.75rem;
    bottom: 0.75rem;
    z-index: 10;
    width: 21rem;
    display: flex;
    flex-direction: column;
    background: #2b323d;
    border: 1px solid #1f242c;
    color: #cfd4db;
    font-size: 0.9rem;
    box-shadow: 2px 3px 10px rgba(0, 0, 0, 0.4);
  }
  .chrome {
    flex: none;
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.4rem 0.6rem;
    background: #3f4855;
    border-bottom: 1px solid #1f242c;
  }
  .title {
    font-weight: 700;
  }
  .badge {
    font-size: 0.72rem;
    padding: 0.12rem 0.4rem;
    background: #4a6da7;
    color: #fff;
    font-variant-numeric: tabular-nums;
  }
  .scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }
  .row {
    display: grid;
    grid-template-columns: auto 1fr;
    grid-template-rows: auto auto;
    align-items: center;
    gap: 0.1rem 0.5rem;
    width: 100%;
    border: none;
    border-bottom: 1px solid #1f242c;
    background: transparent;
    color: inherit;
    font-family: inherit;
    text-align: left;
    padding: 0.4rem 0.5rem;
    cursor: pointer;
  }
  .row:hover {
    background: rgba(255, 255, 255, 0.04);
  }
  .row.selected {
    background: rgba(74, 109, 167, 0.28);
    outline: 1px solid #4a6da7;
    outline-offset: -1px;
  }
  .row.auto {
    background: #262c35;
  }
  .row.auto.selected {
    background: rgba(74, 109, 167, 0.32);
  }
  .swatch {
    width: 22px;
    height: 22px;
    border: 1px solid #1f242c;
    grid-row: 1 / span 2;
  }
  .swatch.none {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    background: #14181d;
    color: #8a919c;
  }
  .sub1 {
    display: flex;
    align-items: baseline;
    gap: 0.5rem;
    min-width: 0;
  }
  .name {
    flex: 1;
    min-width: 0;
    font-size: 0.88rem;
    color: #e5e7eb;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .cnt {
    flex: none;
    font-size: 0.76rem;
    font-variant-numeric: tabular-nums;
    color: #9ca3af;
  }
  .sub2 {
    grid-column: 2;
    font-size: 0.72rem;
    color: #b9bec7;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .sub2.dim {
    color: #8a919c;
  }
  .tag {
    font-size: 0.62rem;
    padding: 0 0.25rem;
    margin-left: 0.25rem;
    vertical-align: middle;
  }
  .tag.water {
    background: #35506b;
    color: #cfe3f5;
  }
  .props-panel {
    flex: none;
    max-height: 45%;
    overflow-y: auto;
    border-top: 1px solid #1f242c;
    background: #262c35;
  }
  .props-head {
    padding: 0.35rem 0.6rem;
    background: #3f4855;
    border-bottom: 1px solid #1f242c;
  }
  .props-title {
    font-size: 0.78rem;
    font-weight: 700;
    color: #e5e7eb;
  }
  .props-body {
    padding: 0.45rem 0.6rem;
  }
  .status {
    flex: none;
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.35rem 0.6rem;
    background: #262c35;
    border-top: 1px solid #1f242c;
    font-size: 0.78rem;
  }
  .s-id {
    color: #9ca3af;
    font-variant-numeric: tabular-nums;
  }
  .s-name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: #e5e7eb;
  }
  .s-flag {
    flex: none;
    font-size: 0.68rem;
    padding: 0.05rem 0.35rem;
    background: #3a4453;
    color: #b9bec7;
  }
  .s-flag.override {
    background: #4a6da7;
    color: #fff;
  }
  .dim {
    color: #8a919c;
  }
</style>
