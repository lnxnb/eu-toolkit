<!--
  TradeGoodsList — Sprint 7.1 right-side list, the picker + editor surface for the
  Trade Goods map mode (replaces the per-selection panel concept). One scrollable
  list of ALL goods in definition order:
    • subrow 1: icon (atlas frame, or a placeholder tile for a pending good),
      localized name, base price, province count
    • subrow 2: compact "trading in" + province modifier summaries
    • Edit button → inline 7.3 editor; click-to-select arms painting (7.2)
  Top: "+ New trade good" (7.4). Directly below it: the "No trade good" option
  (7.5) which expands into the colonization-probability editor.

  Selection/painting is driven from here; the map is the canvas. Windows-classic
  chrome to match the rest of the app.
-->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import type { KnownModifier } from "$lib/components/ui";
  import type { EditQueue } from "$lib/edits.svelte";
  import type { Atlas } from "$lib/overlay";
  import type { Rgb } from "$lib/mapmode";
  import StripIcon from "./StripIcon.svelte";
  import TradeGoodEditor from "./TradeGoodEditor.svelte";
  import ProbabilityEditor from "./ProbabilityEditor.svelte";
  import { modifierSummary, UNKNOWN_KEY, type TradeGood } from "./types";

  const PRICES_FILE = "common/prices/zz_eutoolkit_prices.txt";

  let {
    installPath,
    modPath,
    queue,
    goods,
    counts,
    selectedKey,
    atlas = null,
    atlasIndex,
    onselect,
    oncolor,
    oncreate,
  }: {
    installPath: string;
    modPath: string | null;
    queue: EditQueue;
    /** All goods in definition order (base payload + pending scaffolds appended). */
    goods: TradeGood[];
    /** Good key → province count (mode-data + pending fold). */
    counts: Map<string, number>;
    selectedKey: string | null;
    atlas?: Atlas | null;
    atlasIndex: Map<string, number>;
    /** Select a good (or the "No trade good" sentinel) for painting. */
    onselect: (key: string) => void;
    /** Live map repaint from the editor's color picker. */
    oncolor?: (key: string, rgb: Rgb | null) => void;
    /** Start the create-good flow with the entered name. */
    oncreate: (name: string) => void;
  } = $props();

  let known = $state<KnownModifier[]>([]);
  $effect(() => {
    invoke<KnownModifier[]>("get_known_modifiers")
      .then((k) => (known = k))
      .catch(() => {});
  });
  let labelMap = $derived(new Map(known.map((k) => [k.key, k.label])));

  // The "No trade good" sentinel is pulled out and rendered as its own option.
  let noGood = $derived(goods.find((g) => g.key === UNKNOWN_KEY) ?? null);
  let normalGoods = $derived(goods.filter((g) => g.key !== UNKNOWN_KEY));
  // Goods that participate in the colonization distribution (have a base factor).
  let chanceGoods = $derived(normalGoods.filter((g) => g.chance.base_factor !== null));

  // Which good's inline editor is open (independent of paint selection).
  let editingKey = $state<string | null>(null);

  // Create flow: inline name prompt at the top of the list.
  let creating = $state(false);
  let newName = $state("New Trade Good");
  let nameInput = $state<HTMLInputElement | null>(null);
  function startCreate() {
    creating = true;
    newName = "New Trade Good";
    queueMicrotask(() => {
      nameInput?.focus();
      nameInput?.select();
    });
  }
  function acceptCreate() {
    const n = newName.trim();
    if (n) oncreate(n);
    creating = false;
  }
  function cancelCreate() {
    creating = false;
  }

  function effectiveName(g: TradeGood): string {
    return queue.pendingLocOverride(g.key) ?? g.localizedName;
  }
  function effectivePrice(g: TradeGood): string {
    const pf = g.priceFile ?? PRICES_FILE;
    return queue.pendingScalar(pf, [g.key, "base_price"]) ?? g.basePrice ?? "—";
  }
  function toggleEdit(key: string) {
    editingKey = editingKey === key ? null : key;
    onselect(key);
  }
  function selectRow(key: string) {
    onselect(key);
  }
</script>

<aside class="tg-panel">
  <div class="chrome">
    <span class="title">Trade Goods</span>
    <span class="badge">{goods.length}</span>
  </div>

  <div class="create">
    {#if creating}
      <div class="name-prompt">
        <input
          bind:this={nameInput}
          class="text"
          bind:value={newName}
          onkeydown={(e) => {
            if (e.key === "Enter") acceptCreate();
            else if (e.key === "Escape") cancelCreate();
          }}
        />
        <button class="ok" onclick={acceptCreate} aria-label="Create">✓</button>
        <button class="x" onclick={cancelCreate} aria-label="Cancel">×</button>
      </div>
    {:else}
      <button class="new-btn" onclick={startCreate}>＋ New trade good</button>
    {/if}
  </div>

  <div class="scroll">
    <!-- No trade good (unknown) + probability editor (7.5) -->
    {#if noGood}
      <div class="row nogood" class:selected={selectedKey === UNKNOWN_KEY}>
        <button class="row-main" onclick={() => selectRow(UNKNOWN_KEY)}>
          <span class="swatch none" aria-hidden="true">∅</span>
          <span class="sub1">
            <span class="name">No trade good</span>
            <span class="meta">{counts.get(UNKNOWN_KEY) ?? 0} prov.</span>
          </span>
          <span class="sub2 dim">Paints <span class="mono">trade_goods = unknown</span> (uncolonized)</span>
        </button>
      </div>
      {#if selectedKey === UNKNOWN_KEY}
        <ProbabilityEditor
          {installPath}
          {modPath}
          {queue}
          goods={chanceGoods}
          {atlas}
          {atlasIndex}
        />
      {/if}
    {/if}

    <!-- All goods in definition order -->
    {#each normalGoods as g (g.key)}
      <div class="row" class:selected={selectedKey === g.key}>
        <button class="row-main" onclick={() => selectRow(g.key)}>
          <StripIcon
            {atlas}
            frame={g.pending ? -1 : (atlasIndex.get(g.key) ?? g.index)}
            size={26}
            placeholder={g.rgb}
          />
          <span class="sub1">
            <span class="name">
              {effectiveName(g)}
              {#if g.pending}<span class="tag new">new</span>{/if}
              {#if g.isLatent}<span class="tag latent">latent</span>{/if}
            </span>
            <span class="meta">
              <span class="price">${effectivePrice(g)}</span>
              <span class="cnt">{counts.get(g.key) ?? 0} prov.</span>
            </span>
          </span>
          {#if g.modifierRows.length > 0 || g.provinceRows.length > 0}
            <span class="sub2">
              {#if g.modifierRows.length > 0}
                <span class="mods">{modifierSummary(g.modifierRows, labelMap)}</span>
              {/if}
              {#if g.provinceRows.length > 0}
                <span class="mods prov">⛭ {modifierSummary(g.provinceRows, labelMap)}</span>
              {/if}
            </span>
          {/if}
        </button>
        <button
          class="edit-btn"
          class:on={editingKey === g.key}
          onclick={() => toggleEdit(g.key)}
        >
          Edit
        </button>
      </div>
      {#if editingKey === g.key}
        {#key g.key}
          <TradeGoodEditor good={g} {queue} {installPath} {modPath} {known} priceFileFallback={PRICES_FILE} {oncolor} />
        {/key}
      {/if}
    {/each}
  </div>
</aside>

<style>
  .tg-panel {
    position: absolute;
    top: 3rem;
    right: 0.75rem;
    bottom: 0.75rem;
    z-index: 10;
    width: 23rem;
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
    line-height: 1;
    padding: 0.12rem 0.4rem;
    background: #4a6da7;
    color: #fff;
    font-variant-numeric: tabular-nums;
  }

  .create {
    flex: none;
    padding: 0.4rem;
    border-bottom: 1px solid #1f242c;
    background: #262c35;
  }

  .new-btn {
    width: 100%;
    border: 1px dashed #4b5563;
    background: transparent;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.85rem;
    padding: 0.35rem;
    cursor: pointer;
  }

  .new-btn:hover {
    border-color: #4a6da7;
    background: #4a6da7;
    color: #fff;
  }

  .name-prompt {
    display: flex;
    align-items: center;
    gap: 0.3rem;
  }

  .name-prompt .text {
    flex: 1;
    min-width: 0;
    background: #14181d;
    border: 1px solid #1f242c;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.85rem;
    padding: 0.25rem 0.4rem;
    outline: none;
  }

  .name-prompt .ok,
  .name-prompt .x {
    flex: none;
    border: 1px solid #1f242c;
    background: #14181d;
    color: #cfd4db;
    font-size: 0.9rem;
    line-height: 1;
    padding: 0.25rem 0.4rem;
    cursor: pointer;
  }

  .name-prompt .ok:hover {
    background: #4a6da7;
    color: #fff;
  }

  .name-prompt .x:hover {
    background: #7a3f3f;
    color: #fff;
  }

  .scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }

  .row {
    display: flex;
    align-items: stretch;
    border-bottom: 1px solid #1f242c;
  }

  .row.selected {
    background: rgba(74, 109, 167, 0.28);
  }

  .row-main {
    flex: 1;
    min-width: 0;
    display: grid;
    grid-template-columns: auto 1fr;
    grid-template-rows: auto auto;
    align-items: center;
    gap: 0.1rem 0.5rem;
    border: none;
    background: transparent;
    color: inherit;
    font-family: inherit;
    text-align: left;
    padding: 0.4rem 0.5rem;
    cursor: pointer;
  }

  .row-main:hover {
    background: rgba(255, 255, 255, 0.04);
  }

  .swatch.none {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 26px;
    height: 26px;
    border: 1px solid #1f242c;
    background: #14181d;
    color: #8a919c;
    font-size: 0.9rem;
    grid-row: 1 / span 2;
  }

  .row-main :global(.strip-icon) {
    grid-row: 1 / span 2;
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

  .meta {
    flex: none;
    display: flex;
    gap: 0.5rem;
    font-size: 0.76rem;
    font-variant-numeric: tabular-nums;
    color: #9ca3af;
  }

  .price {
    color: #86efac;
  }

  .sub2 {
    grid-column: 2;
    display: flex;
    flex-direction: column;
    gap: 0.05rem;
    font-size: 0.73rem;
    color: #b9bec7;
    overflow: hidden;
  }

  .sub2 .mods {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .sub2 .prov {
    color: #9ca3af;
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

  .tag.new {
    background: #4a6da7;
    color: #fff;
  }

  .tag.latent {
    background: #6b5b95;
    color: #fff;
  }

  .edit-btn {
    flex: none;
    border: none;
    border-left: 1px solid #1f242c;
    background: transparent;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.78rem;
    padding: 0 0.7rem;
    cursor: pointer;
  }

  .edit-btn:hover,
  .edit-btn.on {
    background: #4a6da7;
    color: #fff;
  }

  .mono {
    font-family: ui-monospace, monospace;
    font-size: 0.72rem;
  }
</style>
