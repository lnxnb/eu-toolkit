<!--
  TradeGoodEditor — the 7.3 inline editor for one trade good (expands under its
  row). Edits are byte-surgical TypedEdits pushed to the shared queue using the
  recipes documented in the backend `tradegoods.rs` header:
    • name   → locOverride (bare good key)
    • color  → setBlock [key,"color"] as space-joined 0-1 floats (2dp)
    • price  → setScalar [key,"base_price"] in the good's price_file
    • modifier / province → setBlock [key,<sub>] "k1 = v1 k2 = v2"
    • chance factor → setScalar [key,"chance","factor"] (conditionals preserved)
    • is_latent → insert/remove statement
  Continuous edits (color/price/factor/modifiers) coalesce into one undo unit.
  raw_extra + the conditional-weights note are read-only.
-->
<script lang="ts">
  import { untrack } from "svelte";
  import { ColorPicker, ModifierEditor, IconImportButton } from "$lib/components/ui";
  import type { KnownModifier, ModifierRow, RGB } from "$lib/components/ui";
  import type { EditQueue } from "$lib/edits.svelte";
  import type { Rgb } from "$lib/mapmode";
  import {
    colorFloatsToRgb,
    rgbToColorFloats,
    type TradeGood,
  } from "./types";

  let {
    good,
    queue,
    installPath,
    modPath,
    known = [],
    priceFileFallback,
    oncolor,
  }: {
    good: TradeGood;
    queue: EditQueue;
    installPath: string;
    modPath: string | null;
    known?: KnownModifier[];
    /** File to write prices into when the good has no existing price_file. */
    priceFileFallback: string;
    /** Live map repaint: (goodKey, rgb|null). */
    oncolor?: (key: string, rgb: Rgb | null) => void;
  } = $props();

  const file = $derived(good.sourceFile);
  const priceFile = $derived(good.priceFile ?? priceFileFallback);

  // --- Color (live repaint) ---
  let pendingColorStr = $derived(queue.pendingBlockValue(file, [good.key, "color"]));
  let colorEdited = $derived(pendingColorStr !== undefined);
  let effectiveColor = $derived<Rgb | null>(
    (pendingColorStr ? colorFloatsToRgb(pendingColorStr.split(/\s+/)) : null) ??
      good.rgb ??
      null,
  );
  let colorRGB = $derived<RGB>({
    r: effectiveColor?.[0] ?? 128,
    g: effectiveColor?.[1] ?? 128,
    b: effectiveColor?.[2] ?? 128,
  });

  // Push the pending color to the map while the editor is open; clear on unmount.
  $effect(() => {
    if (colorEdited && effectiveColor) oncolor?.(good.key, effectiveColor);
    else oncolor?.(good.key, null);
  });
  $effect(() => {
    const k = good.key;
    return () => oncolor?.(k, null);
  });

  function commitColor(c: RGB) {
    const rgb: Rgb = [c.r, c.g, c.b];
    queue.push({
      label: `Set color of ${good.key}`,
      edits: [
        { kind: "setBlock", file, path: [good.key, "color"], value: rgbToColorFloats(rgb).join(" ") },
      ],
      coalesceKey: `tgcolor:${good.key}`,
    });
  }

  // --- Name (loc override) ---
  let pendingName = $derived(queue.pendingLocOverride(good.key));
  let nameVal = $derived(pendingName ?? good.localizedName);
  function commitName(v: string) {
    queue.push({
      label: `Rename ${good.key}`,
      edits: [{ kind: "locOverride", key: good.key, value: v }],
      coalesceKey: `tgname:${good.key}`,
    });
  }

  // --- Base price ---
  let pendingPrice = $derived(queue.pendingScalar(priceFile, [good.key, "base_price"]));
  let priceVal = $derived(pendingPrice ?? good.basePrice ?? "");
  function commitPrice(v: string) {
    const t = v.trim();
    if (t === "") return;
    queue.push({
      label: `Set base price of ${good.key}`,
      edits: [{ kind: "setScalar", file: priceFile, path: [good.key, "base_price"], value: t, quoted: false }],
      coalesceKey: `tgprice:${good.key}`,
    });
  }

  // --- Modifier blocks (seed-once; component is remounted per good by {#key}) ---
  let countryMods = $state<ModifierRow[]>(
    untrack(() => good.modifierRows.map((r) => ({ key: r.key, value: r.value }))),
  );
  let provinceMods = $state<ModifierRow[]>(
    untrack(() => good.provinceRows.map((r) => ({ key: r.key, value: r.value }))),
  );

  function blockValue(rows: ModifierRow[]): string {
    return rows.map((r) => `${r.key} = ${r.value}`).join(" ");
  }
  function commitMod(sub: "modifier" | "province", rows: ModifierRow[]) {
    queue.push({
      label: `Edit ${sub === "modifier" ? "country" : "province"} modifiers of ${good.key}`,
      edits: [{ kind: "setBlock", file, path: [good.key, sub], value: blockValue(rows) }],
      coalesceKey: `tgmod:${good.key}:${sub}`,
    });
  }

  // --- Chance base factor (bulk editor is the No-trade-good panel, 7.5) ---
  let pendingFactor = $derived(queue.pendingScalar(file, [good.key, "chance", "factor"]));
  let factorVal = $derived(pendingFactor ?? good.chance.base_factor ?? "");
  function commitFactor(v: string) {
    const t = v.trim();
    if (t === "") return;
    queue.push({
      label: `Set colonization chance of ${good.key}`,
      edits: [{ kind: "setScalar", file, path: [good.key, "chance", "factor"], value: t, quoted: false }],
      coalesceKey: `tgfactor:${good.key}`,
    });
  }

  // --- is_latent toggle (seed-once) ---
  let latent = $state(untrack(() => good.isLatent));
  function toggleLatent(on: boolean) {
    latent = on;
    queue.push({
      label: `${on ? "Mark" : "Unmark"} ${good.key} latent`,
      edits: [
        on
          ? { kind: "insertStatement", file, blockPath: [good.key], statement: "is_latent = yes" }
          : { kind: "removeStatement", file, blockPath: [good.key], key: "is_latent" },
      ],
      coalesceKey: `tglatent:${good.key}`,
    });
  }
</script>

<div class="editor">
  <label class="field">
    <span class="lbl">Name{#if pendingName !== undefined}<span class="dot">•</span>{/if}</span>
    <input class="text" value={nameVal} oninput={(e) => commitName((e.target as HTMLInputElement).value)} />
  </label>

  <label class="field">
    <span class="lbl">Color{#if colorEdited}<span class="dot">•</span>{/if}</span>
    <span class="ctl">
      <ColorPicker value={colorRGB} onchange={commitColor} />
      <span class="mono">rgb({colorRGB.r}, {colorRGB.g}, {colorRGB.b})</span>
    </span>
  </label>

  <label class="field">
    <span class="lbl">Base price{#if pendingPrice !== undefined}<span class="dot">•</span>{/if}</span>
    <input
      class="num"
      type="number"
      step="any"
      value={priceVal}
      oninput={(e) => commitPrice((e.target as HTMLInputElement).value)}
    />
  </label>

  <div class="field">
    <span class="lbl">Icon</span>
    <IconImportButton
      {installPath}
      {modPath}
      {queue}
      kind="trade_goods"
      frame={good.index}
      label="Import art…"
    />
  </div>

  <div class="block">
    <span class="lbl">Trading-in country modifiers</span>
    <ModifierEditor bind:modifiers={countryMods} {known} onchange={(r) => commitMod("modifier", r)} />
  </div>

  <div class="block">
    <span class="lbl">Province production modifiers</span>
    <ModifierEditor bind:modifiers={provinceMods} {known} onchange={(r) => commitMod("province", r)} />
  </div>

  {#if good.chance.base_factor !== null}
    <label class="field">
      <span class="lbl">Colonization chance{#if pendingFactor !== undefined}<span class="dot">•</span>{/if}</span>
      <input
        class="num"
        type="number"
        step="any"
        value={factorVal}
        oninput={(e) => commitFactor((e.target as HTMLInputElement).value)}
      />
    </label>
    {#if good.chance.has_conditional_modifiers}
      <p class="note">
        + {good.chance.conditional_count} conditional weight{good.chance.conditional_count === 1 ? "" : "s"}
        (climate/region) preserved — the base factor here is the unconditional weight.
      </p>
    {/if}
    <p class="note dim">Bulk-edit all goods' chances in the "No trade good" panel.</p>
  {/if}

  <label class="field checkbox">
    <input type="checkbox" checked={latent} onchange={(e) => toggleLatent((e.target as HTMLInputElement).checked)} />
    <span class="lbl">Latent good (e.g. coal — spawns on discovery)</span>
  </label>

  {#if good.rawExtra.length > 0}
    <div class="block">
      <span class="lbl">Advanced (read-only)</span>
      <p class="note dim">Unmodeled content — preserved untouched on save.</p>
      <ul class="raw">
        {#each good.rawExtra as r (r.key)}
          <li><span class="mono">{r.key}</span> = {r.value}</li>
        {/each}
      </ul>
    </div>
  {/if}
</div>

<style>
  .editor {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    padding: 0.55rem 0.5rem 0.7rem;
    background: var(--bg-1);
    border-top: 1px solid var(--border);
  }

  .field {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .field.checkbox {
    cursor: pointer;
  }

  .block {
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
  }

  .lbl {
    font-size: 0.78rem;
    color: var(--text-2);
    min-width: 6rem;
  }

  .block .lbl {
    min-width: 0;
    color: var(--text-1);
    font-weight: 600;
  }

  .dot {
    color: var(--warn);
    margin-left: 0.2rem;
  }

  .ctl {
    display: inline-flex;
    align-items: center;
    gap: 0.5rem;
  }

  .text,
  .num {
    background: var(--bg-0);
    border: 1px solid var(--border);
    color: var(--text-1);
    font-family: inherit;
    font-size: 0.85rem;
    padding: 0.25rem 0.4rem;
    outline: none;
  }

  .text {
    flex: 1;
    min-width: 0;
  }

  .num {
    width: 6rem;
  }

  .mono {
    font-family: ui-monospace, monospace;
    font-size: 0.75rem;
    color: var(--text-2);
  }

  .note {
    margin: 0;
    font-size: 0.75rem;
    color: var(--text-1);
  }

  .note.dim {
    color: var(--text-2);
  }

  .raw {
    margin: 0;
    padding-left: 1rem;
    font-size: 0.78rem;
    color: var(--text-1);
  }
</style>
