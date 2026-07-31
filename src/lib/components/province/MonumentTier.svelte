<script lang="ts">
  // One tier (tier_0..tier_3) of a great project: upgrade-time months + cost
  // factor steppers, the province/area/country typed modifier blocks (flat-block
  // gated), the on_upgraded effect tree (14.2), and preserve-unknown for any
  // other tier sub-block (conditional_modifier, …). All edits are byte-surgical.
  import EstateModifierBlock from "$lib/components/estates/EstateModifierBlock.svelte";
  import ScriptBlockField from "./ScriptBlockField.svelte";
  import type { KnownModifier, ModifierRow, DropdownItem } from "$lib/components/ui";
  import type { KnownKey } from "$lib/components/script";
  import type { EditQueue, TypedEdit } from "$lib/edits.svelte";
  import { modBlockValue, type Tier, type Scalar, type ModifierBlockRef } from "$lib/monuments";

  let {
    installPath,
    modPath,
    queue,
    file,
    entryKey,
    tier,
    known,
    effects,
    countries = [],
  }: {
    installPath: string;
    modPath: string | null;
    queue: EditQueue;
    file: string;
    entryKey: string;
    tier: Tier;
    known: KnownModifier[];
    effects: KnownKey[];
    countries?: DropdownItem[];
  } = $props();

  let open = $state(false);

  const fullPath = (rel: string[]) => [entryKey, ...rel];

  // --- Scalars (setScalar when present, insertStatement into parent when absent) ---
  function liveScalar(s: Scalar): string {
    queue.version;
    const ps = queue.pendingScalar(file, fullPath(s.path));
    if (ps !== undefined) return ps;
    const parent = fullPath(s.path.slice(0, -1));
    const ins = queue.findLast(
      (e) =>
        e.kind === "insertStatement" &&
        e.file === file &&
        e.blockPath.length === parent.length &&
        e.blockPath.every((p, i) => p === parent[i]) &&
        e.statement.split("=")[0].trim() === s.key,
    );
    if (ins?.kind === "insertStatement") return ins.statement.split("=").slice(1).join("=").trim();
    return s.value;
  }
  function commitScalar(s: Scalar, value: string) {
    const parent = fullPath(s.path.slice(0, -1));
    const edit: TypedEdit = s.present
      ? { kind: "setScalar", file, path: fullPath(s.path), value, quoted: false }
      : { kind: "insertStatement", file, blockPath: parent, statement: `${s.key} = ${value}` };
    queue.push({
      label: `Edit ${entryKey} tier_${tier.index} ${s.key}`,
      edits: [edit],
      coalesceKey: `gptier:${file}:${entryKey}:${s.path.join(".")}`,
    });
  }

  // --- Modifier blocks ---
  function commitModifier(mb: ModifierBlockRef, rows: ModifierRow[]) {
    const body = modBlockValue(rows);
    const parent = fullPath(mb.path.slice(0, -1));
    const edit: TypedEdit = mb.present
      ? { kind: "setBlock", file, path: fullPath(mb.path), value: body }
      : { kind: "insertStatement", file, blockPath: parent, statement: `${mb.name} = { ${body} }` };
    queue.push({
      label: `Edit ${entryKey} tier_${tier.index} ${mb.name}`,
      edits: [edit],
      coalesceKey: `gptiermod:${file}:${entryKey}:${mb.path.join(".")}`,
    });
  }
</script>

<div class="tier">
  <button class="tier-head" onclick={() => (open = !open)}>
    <span class="caret">{open ? "▾" : "▸"}</span>
    <strong>Tier {tier.index}</strong>
    {#if !tier.present}<span class="absent">absent</span>{/if}
  </button>
  {#if open}
    <div class="tier-body">
      <!-- Scalars: upgrade time months + cost factor -->
      <div class="scalars">
        {#each tier.scalars as s (s.key)}
          <div class="scalar">
            <span class="sk">{s.key === "months" ? "upgrade months" : s.key === "factor" ? "cost factor" : s.key}</span>
            <input
              class="num"
              type="number"
              step={s.kind === "int" ? "1" : "any"}
              value={liveScalar(s)}
              oninput={(e) => commitScalar(s, (e.target as HTMLInputElement).value)}
            />
          </div>
        {/each}
      </div>

      <!-- Modifier blocks -->
      {#each tier.modifierBlocks as mb (mb.name)}
        <div class="modblock">
          <div class="mb-head">
            <code>{mb.name}</code>
            {#if !mb.present}<span class="tag-abs">absent</span>{/if}
            {#if mb.present && !mb.flat}<span class="tag-raw">nested — read-only</span>{/if}
          </div>
          {#if mb.flat}
            <EstateModifierBlock base={mb.rows} {known} oncommit={(r) => commitModifier(mb, r)} />
          {:else}
            <p class="dim small">Nested content — edit in the raw file to avoid data loss.</p>
          {/if}
        </div>
      {/each}

      <!-- on_upgraded effect tree -->
      {#each tier.scriptBlocks as sb (sb.name)}
        <ScriptBlockField
          {installPath}
          {modPath}
          {queue}
          {file}
          path={fullPath(sb.path)}
          registry={sb.registry as "triggers" | "effects"}
          present={sb.present}
          known={effects}
          {countries}
        />
      {/each}

      <!-- Preserve-unknown -->
      {#if tier.rawExtra.length > 0}
        <div class="raw-extra">
          <span class="lbl">Advanced (read-only):</span>
          {#each tier.rawExtra as r (r)}<code class="idchip">{r}</code>{/each}
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .tier { border: 1px solid var(--bg-1); margin-top: 0.3rem; }
  .tier-head {
    display: flex; align-items: center; gap: 0.4rem; width: 100%; text-align: left;
    border: none; background: var(--bg-1); color: var(--text-1); font-family: inherit;
    font-size: 0.8rem; padding: 0.2rem 0.35rem; cursor: pointer;
  }
  .caret { color: var(--text-2); width: 0.8rem; flex: none; }
  .absent { font-size: 0.65rem; text-transform: uppercase; color: var(--text-2); }
  .tier-body { padding: 0.35rem; display: flex; flex-direction: column; gap: 0.35rem; }
  .scalars { display: flex; flex-wrap: wrap; gap: 0.4rem 1rem; }
  .scalar { display: flex; align-items: center; gap: 0.4rem; }
  .sk { font-size: 0.74rem; color: var(--text-1); }
  .num {
    width: 5rem; background: var(--bg-0); border: 1px solid var(--border-strong); color: var(--text-1);
    font-family: inherit; font-size: 0.8rem; padding: 0.15rem 0.35rem;
  }
  .modblock { border: 1px solid var(--bg-1); padding: 0.3rem; }
  .mb-head { display: flex; align-items: center; gap: 0.4rem; margin-bottom: 0.25rem; }
  .mb-head code { color: var(--ok); background: var(--bg-0); padding: 0 0.3rem; font-size: 0.76rem; }
  .tag-abs, .tag-raw { font-size: 0.65rem; text-transform: uppercase; color: var(--text-2); }
  .tag-raw { color: var(--warn); }
  .raw-extra { display: flex; flex-wrap: wrap; align-items: center; gap: 0.25rem; }
  .lbl { font-size: 0.72rem; color: var(--text-2); }
  .idchip { color: var(--text-1); background: var(--bg-0); padding: 0.05rem 0.3rem; font-size: 0.72rem; }
  .dim { color: var(--text-2); }
  .small { font-size: 0.74rem; }
</style>
