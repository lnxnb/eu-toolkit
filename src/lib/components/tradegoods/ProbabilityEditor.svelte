<!--
  ProbabilityEditor — the 7.5 colonization-chance editor that the "No trade good"
  option expands into. A scrollable sub-list of every good that has a base
  `chance.factor`, each shown as a percentage (base factors normalized to 100%),
  editable via a numeric % input + a lock pin (spreadsheet-style redistribution
  reusing the pure `redistribute` math from the UI kit's sliderMath).

  Apply → rebalance_chances(newPercentages) → the returned TypedEdits are pushed
  as one coalesced composite (so repeated Applies stay one undo unit). Pending
  until the project is saved; a badge reflects that. Conditional weights per good
  are noted but never touched (the 100% view is the base distribution).
-->
<script lang="ts">
  import { untrack } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { redistribute } from "$lib/components/ui";
  import type { EditQueue, TypedEdit } from "$lib/edits.svelte";
  import type { Atlas } from "$lib/overlay";
  import StripIcon from "./StripIcon.svelte";
  import { factorsToPercentages, type TradeGood } from "./types";

  let {
    installPath,
    modPath,
    queue,
    goods,
    atlas = null,
    atlasIndex,
  }: {
    installPath: string;
    modPath: string | null;
    queue: EditQueue;
    /** Goods that participate (those with a base chance.factor). */
    goods: TradeGood[];
    atlas?: Atlas | null;
    atlasIndex: Map<string, number>;
  } = $props();

  // Seed percentages from the effective (pending-aware) base factors, normalized
  // to 100. Seeded once on mount — the parent re-mounts this when it re-expands.
  let keys: string[] = untrack(() => goods.map((g) => g.key));
  function effectiveFactor(g: TradeGood): number {
    const pending = queue.pendingScalar(g.sourceFile, [g.key, "chance", "factor"]);
    const n = Number(pending ?? g.chance.base_factor ?? "0");
    return Number.isFinite(n) ? n : 0;
  }
  let values = $state<number[]>(untrack(() => factorsToPercentages(goods.map(effectiveFactor))));
  let locks = $state<boolean[]>(untrack(() => goods.map(() => false)));

  let applying = $state(false);
  let applied = $state(false);
  let sum = $derived(values.reduce((a, b) => a + b, 0));

  function setPct(i: number, raw: string) {
    const req = Number(raw);
    if (!Number.isFinite(req)) return;
    values = redistribute(values, i, req, locks, 100);
    applied = false;
  }

  function toggleLock(i: number) {
    const next = locks.slice();
    next[i] = !next[i];
    locks = next;
  }

  async function apply() {
    if (applying) return;
    applying = true;
    const newPercentages: Record<string, number> = {};
    keys.forEach((k, i) => (newPercentages[k] = values[i]));
    try {
      const edits = await invoke<TypedEdit[]>("rebalance_chances", {
        installPath,
        modPath,
        newPercentages,
      });
      if (edits.length > 0) {
        queue.push({
          label: "Rebalance colonization chances",
          edits,
          coalesceKey: "tradegood:rebalance",
        });
      }
      applied = true;
    } catch {
      /* leave applied=false; the strip shows nothing changed */
    }
    applying = false;
  }
</script>

<div class="prob">
  <div class="prob-head">
    <span class="title">Colonization chances</span>
    <span class="sum" class:off={Math.abs(sum - 100) > 0.5}>Σ {sum.toFixed(1)}%</span>
  </div>
  <p class="hint">
    Percentages of the base colonization distribution. Lock a row to pin it while
    the rest redistribute. Conditional (climate/region) weights are preserved.
  </p>

  <div class="rows">
    {#each goods as g, i (g.key)}
      <div class="prow">
        <button
          class="lock"
          class:on={locks[i]}
          title={locks[i] ? "Unlock" : "Lock"}
          onclick={() => toggleLock(i)}
        >
          {locks[i] ? "🔒" : "🔓"}
        </button>
        <StripIcon {atlas} frame={atlasIndex.get(g.key) ?? -1} size={18} placeholder={g.rgb} />
        <span class="name" title={g.key}>{g.localizedName}</span>
        {#if g.chance.has_conditional_modifiers}
          <span class="cond" title="{g.chance.conditional_count} conditional weight(s) preserved">＊</span>
        {/if}
        <input
          class="pct"
          type="number"
          min="0"
          max="100"
          step="any"
          value={values[i].toFixed(1)}
          disabled={locks[i]}
          oninput={(e) => setPct(i, (e.target as HTMLInputElement).value)}
        />
        <span class="pct-sign">%</span>
      </div>
    {/each}
  </div>

  <div class="apply-row">
    <button class="apply" onclick={apply} disabled={applying}>
      {applying ? "Applying…" : "Apply"}
    </button>
    {#if applied}<span class="pending">Pending — saves with the project</span>{/if}
  </div>
</div>

<style>
  .prob {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    padding: 0.5rem;
    background: var(--bg-1);
    border-top: 1px solid var(--border);
  }

  .prob-head {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .title {
    font-size: 0.78rem;
    font-weight: 600;
    color: var(--text-1);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .sum {
    margin-left: auto;
    font-size: 0.78rem;
    font-variant-numeric: tabular-nums;
    color: var(--ok);
  }

  .sum.off {
    color: var(--err);
  }

  .hint {
    margin: 0;
    font-size: 0.73rem;
    color: var(--text-2);
  }

  .rows {
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
    max-height: 16rem;
    overflow-y: auto;
  }

  .prow {
    display: flex;
    align-items: center;
    gap: 0.35rem;
  }

  .lock {
    flex: none;
    border: 1px solid var(--border);
    background: var(--bg-0);
    color: var(--text-1);
    font-size: 0.7rem;
    line-height: 1;
    padding: 0.15rem 0.25rem;
    cursor: pointer;
  }

  .lock.on {
    background: var(--accent);
  }

  .name {
    flex: 1;
    min-width: 0;
    font-size: 0.82rem;
    color: var(--text-1);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .cond {
    flex: none;
    color: var(--warn);
    font-size: 0.8rem;
  }

  .pct {
    flex: none;
    width: 4rem;
    background: var(--bg-0);
    border: 1px solid var(--border);
    color: var(--text-1);
    font-family: inherit;
    font-size: 0.82rem;
    text-align: right;
    padding: 0.2rem 0.3rem;
    outline: none;
  }

  .pct-sign {
    flex: none;
    font-size: 0.78rem;
    color: var(--text-2);
  }

  .apply-row {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    margin-top: 0.2rem;
  }

  .apply {
    border: 1px solid var(--border);
    background: var(--bg-3);
    color: var(--text-1);
    font-family: inherit;
    font-size: 0.82rem;
    padding: 0.3rem 0.9rem;
    cursor: pointer;
  }

  .apply:hover:not(:disabled) {
    background: var(--accent);
    color: var(--text-inverse);
  }

  .apply:disabled {
    opacity: 0.6;
    cursor: default;
  }

  .pending {
    font-size: 0.75rem;
    color: var(--warn);
  }
</style>
