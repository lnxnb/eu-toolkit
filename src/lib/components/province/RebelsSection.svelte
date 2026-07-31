<!--
  RebelsSection (Sprint 21) — the province-panel Rebels editor.

  Edits the `revolt = { type size leader name }` block. "No revolt" = the block is
  absent; adding creates it, clearing removes it. Works top-level (cutoff ≤ start)
  and dated (a later selected date writes into that date's block via pushAtDate,
  and the History Timeline below gives full per-entry control). `unrest` is NOT
  here — it is already editable in the Economy section.

  Top-level write mode mirrors latent_trade_goods: setBlock when the block is
  projected-present (on disk, or already inserted this session), insert otherwise,
  remove to clear. No coalescing — projected-present flips to true after the first
  insert, so subsequent edits use setBlock and never stack duplicate blocks.
-->
<script lang="ts">
  import { SearchDropdown } from "$lib/components/ui";
  import type { DropdownItem } from "$lib/components/ui";
  import FieldRow from "$lib/components/country/FieldRow.svelte";
  import type { EditQueue, TypedEdit } from "$lib/edits.svelte";
  import type { ProvinceDetails } from "./types";
  import { writesDatedBlock, pushAtDate, type DateCtx } from "./fields";
  import { parseRevolt, revoltEmpty, revoltBody, type Revolt } from "$lib/rebels";
  import { compareDates } from "$lib/calendar";

  let {
    details,
    file,
    queue,
    factions,
    dateCtx,
  }: {
    details: ProvinceDetails;
    file: string;
    queue: EditQueue;
    /** Rebel faction registry (rebel_types) for the type picker. */
    factions: DropdownItem[];
    dateCtx?: DateCtx;
  } = $props();

  const id = $derived(details.id);
  const cutoff = $derived(dateCtx?.selectedDate ?? dateCtx?.startDate ?? "1444.11.11");
  const onDiskTopPresent = $derived(details.raw_remainder.some((r) => r.key === "revolt"));

  function stmtKey(s: string): string {
    const eq = s.indexOf("=");
    return eq < 0 ? "" : s.slice(0, eq).trim();
  }
  function stmtVal(s: string): string {
    const eq = s.indexOf("=");
    return eq < 0 ? "" : s.slice(eq + 1).trim();
  }

  // The last structural top-level revolt edit in the queue (setBlock/insert/remove).
  function lastTopEdit(): TypedEdit | undefined {
    queue.version;
    return queue.findLast(
      (e) =>
        (e.kind === "setBlock" && e.file === file && e.path.length === 1 && e.path[0] === "revolt") ||
        (e.kind === "insertStatement" && e.file === file && e.blockPath.length === 0 && stmtKey(e.statement) === "revolt") ||
        (e.kind === "removeStatement" && e.file === file && e.blockPath.length === 0 && e.key === "revolt" && e.value == null),
    );
  }

  // Projected top-level revolt body ("{ … }" or null), folding pending edits.
  const topBody = $derived.by<string | null>(() => {
    const hit = lastTopEdit();
    if (hit) {
      if (hit.kind === "removeStatement") return null;
      if (hit.kind === "setBlock") return `{ ${hit.value} }`;
      if (hit.kind === "insertStatement") return stmtVal(hit.statement);
    }
    const disk = details.raw_remainder.find((r) => r.key === "revolt");
    return disk ? disk.value : null;
  });

  // Will the top-level block exist at the point the next edit applies?
  const projectedTopPresent = $derived.by<boolean>(() => {
    const hit = lastTopEdit();
    return hit ? hit.kind !== "removeStatement" : onDiskTopPresent;
  });

  // Effective revolt at the cutoff date: top-level, then dated blocks ≤ cutoff in
  // file order (an empty `revolt = {}` clears it). `dateCtx.blocks` already carries
  // pending dated folds.
  const effective = $derived.by<Revolt | null>(() => {
    let body: string | null = topBody;
    for (const b of dateCtx?.blocks ?? []) {
      if (compareDates(b.date, cutoff) > 0) continue;
      for (const e of b.entries) {
        if (e.key === "revolt") body = e.value;
      }
    }
    if (body == null || revoltEmpty(body)) return null;
    return parseRevolt(body);
  });

  const present = $derived(effective != null);
  /** Display-only: does a revolt write at this date go into a dated block? */
  const later = $derived(writesDatedBlock(dateCtx, ["revolt = {}"]));

  // --- Writers ---
  function writeRevolt(next: Revolt) {
    const inner = revoltBody(next).replace(/^\{\s*/, "").replace(/\s*\}$/, "");
    const full = `revolt = { ${inner} }`;
    if (writesDatedBlock(dateCtx, [full])) {
      pushAtDate(queue, dateCtx, `Set revolt of #${id}`, [{ kind: "insertStatement", file, blockPath: [], statement: full }], [full]);
      return;
    }
    const edit: TypedEdit = projectedTopPresent
      ? { kind: "setBlock", file, path: ["revolt"], value: inner }
      : { kind: "insertStatement", file, blockPath: [], statement: full };
    queue.push({ label: `Set revolt of #${id}`, edits: [edit] });
  }

  function edit(patch: Partial<Revolt>) {
    const base: Revolt = effective ?? { type: factions[0]?.key ?? null, size: "1", leader: null, name: null };
    writeRevolt({ ...base, ...patch });
  }

  function addRevolt() {
    writeRevolt({ type: factions[0]?.key ?? "nationalist_rebels", size: "1", leader: null, name: null });
  }

  function clearRevolt() {
    if (writesDatedBlock(dateCtx, ["revolt = {}"])) {
      // Dated clear = an empty `revolt = {}` entry that ends the revolt at this date.
      pushAtDate(queue, dateCtx, `Clear revolt of #${id}`, [{ kind: "insertStatement", file, blockPath: [], statement: "revolt = {}" }], ["revolt = {}"]);
      return;
    }
    // Top-level clear: remove the block. When it exists only as a pending insert,
    // the insert+remove pair nets to nothing at save (no on-disk statement left).
    queue.push({ label: `Clear revolt of #${id}`, edits: [{ kind: "removeStatement", file, blockPath: [], key: "revolt" }] });
  }
</script>

<section>
  <h3>Rebels</h3>

  {#if !present}
    <p class="dim">No revolt {later ? "at this date" : "at start"}. Adding creates a <code>revolt</code> block.</p>
    <button class="add-btn" onclick={addRevolt}>+ Add revolt</button>
  {:else}
    {@const r = effective as Revolt}
    <FieldRow label="Faction">
      <SearchDropdown items={factions} value={r.type ?? ""} placeholder="Rebel type…" onselect={(k) => edit({ type: k })} />
    </FieldRow>
    <FieldRow label="Size">
      <input class="num" type="number" min="0" step="1" value={r.size ?? ""} placeholder="0"
        onchange={(e) => edit({ size: e.currentTarget.value.trim() || "0" })} />
    </FieldRow>
    <FieldRow label="Leader">
      <input class="txt" value={r.leader ?? ""} placeholder="(none)"
        onchange={(e) => edit({ leader: e.currentTarget.value.trim() || null })} />
    </FieldRow>
    <FieldRow label="Name">
      <input class="txt" value={r.name ?? ""} placeholder="(none)"
        onchange={(e) => edit({ name: e.currentTarget.value.trim() || null })} />
    </FieldRow>
    <button class="clr-btn" onclick={clearRevolt}>Clear revolt</button>
    {#if later}
      <p class="dim note">Writing into the {cutoff} history block. Use the History Timeline below for finer control of dated entries.</p>
    {/if}
  {/if}
</section>

<style>
  section { margin-bottom: 1rem; }
  h3 { margin: 0 0 0.5rem; font-size: 0.8rem; text-transform: uppercase; letter-spacing: 0.05em; color: var(--text-2); }
  .dim { color: var(--text-2); font-size: 0.8rem; margin: 0 0 0.4rem; }
  .note { margin-top: 0.4rem; }
  code { color: var(--ok); background: var(--bg-0); padding: 0 0.25rem; font-size: 0.76rem; }
  .num { width: 4rem; background: var(--bg-1); border: 1px solid var(--border); color: var(--text-1); font-family: inherit; font-size: 0.85rem; padding: 0.25rem 0.4rem; }
  .txt { width: 14rem; max-width: 100%; background: var(--bg-1); border: 1px solid var(--border); color: var(--text-1); font-family: inherit; font-size: 0.85rem; padding: 0.25rem 0.4rem; }
  .add-btn { border: 1px solid var(--border); background: var(--bg-3); color: var(--text-1); font-family: inherit; font-size: 0.8rem; padding: 0.2rem 0.6rem; cursor: pointer; }
  .add-btn:hover { background: var(--accent); color: var(--text-inverse); }
  .clr-btn { margin-top: 0.4rem; border: 1px solid var(--danger-bg); background: transparent; color: var(--err); font-family: inherit; font-size: 0.78rem; padding: 0.2rem 0.6rem; cursor: pointer; }
  .clr-btn:hover { background: var(--danger-bg); color: var(--text-inverse); }
</style>
