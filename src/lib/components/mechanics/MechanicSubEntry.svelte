<script lang="ts">
  // One child of a sub-group container (Sprint 27 Wave 3): an age objective
  // (whole body = a trigger tree) or an age ability (flat `modifier` block +
  // effect / ai_will_do scripts). Edits address the deeper path
  // [obj, container, childKey, …] through the same typed-edit vocabulary; the
  // child block itself is present (we iterate existing children), so trigger /
  // script editors resolve directly.
  import type { KnownKey } from "$lib/components/script";
  import type { DropdownItem, KnownModifier, ModifierRow } from "$lib/components/ui";
  import EstateModifierBlock from "$lib/components/estates/EstateModifierBlock.svelte";
  import type { EditQueue, TypedEdit } from "$lib/edits.svelte";
  import { modBlockValue, type ModifierBlock, type SubEntry } from "$lib/mechanics";
  import MechanicScriptBlock from "./MechanicScriptBlock.svelte";

  let {
    installPath,
    modPath,
    queue,
    file,
    containerPath,
    entry,
    childIsTrigger,
    known,
    triggers,
    effects,
    countries = [],
    onremove,
  }: {
    installPath: string;
    modPath: string | null;
    queue: EditQueue;
    file: string;
    /** Path to the container block, e.g. [ageKey, "objectives"]. */
    containerPath: string[];
    entry: SubEntry;
    childIsTrigger: boolean;
    known: KnownModifier[];
    triggers: KnownKey[];
    effects: KnownKey[];
    countries?: DropdownItem[];
    onremove: () => void;
  } = $props();

  // Path to the child block itself (ability body / objective trigger body).
  const childPath = $derived([...containerPath, entry.key]);

  function commitModifier(mb: ModifierBlock, rows: ModifierRow[]) {
    const body = modBlockValue(rows);
    const edit: TypedEdit = mb.present
      ? { kind: "setBlock", file, path: [...childPath, mb.name], value: body }
      : { kind: "insertStatement", file, blockPath: childPath, statement: `${mb.name} = { ${body} }` };
    queue.push({
      label: `Edit ${mb.name} of ${entry.key}`,
      edits: [edit],
      coalesceKey: `mecsub:${file}:${childPath.join(".")}:${mb.name}`,
    });
  }
</script>

<div class="sub">
  <div class="sub-head">
    <code class="sk">{entry.key}</code>
    {#if entry.name && entry.name !== entry.key}<span class="nm">{entry.name}</span>{/if}
    <span class="spacer"></span>
    <button class="mini danger" title="Delete" onclick={onremove}>✕</button>
  </div>

  {#if childIsTrigger}
    <!-- The whole child body is a trigger tree (age objective). -->
    <MechanicScriptBlock
      {installPath}
      {modPath}
      {queue}
      {file}
      basePath={containerPath}
      name={entry.key}
      registry="triggers"
      present={true}
      known={triggers}
      {countries}
    />
  {:else}
    {#each entry.modifierBlocks as mb (mb.name)}
      <div class="modblock">
        <div class="mb-head">
          <code>{mb.name}</code>
          {#if !mb.present}<span class="tag-abs">absent</span>{/if}
          {#if mb.present && !mb.flat}<span class="tag-raw">nested — read-only</span>{/if}
        </div>
        {#if mb.flat}
          <EstateModifierBlock base={mb.rows} {known} oncommit={(r) => commitModifier(mb, r)} />
        {:else}
          <p class="dim small">This block contains nested content; edit it in the raw file.</p>
        {/if}
      </div>
    {/each}
    {#each entry.scriptBlocks as sb (sb.name)}
      <MechanicScriptBlock
        {installPath}
        {modPath}
        {queue}
        {file}
        basePath={childPath}
        name={sb.name}
        registry={sb.registry as "triggers" | "effects"}
        present={sb.present}
        known={sb.registry === "triggers" ? triggers : effects}
        {countries}
      />
    {/each}
  {/if}

  {#if entry.rawExtra.length > 0}
    <div class="rawrow">
      <span class="dim small">preserved:</span>
      {#each entry.rawExtra as r (r)}<code class="idchip raw">{r}</code>{/each}
    </div>
  {/if}
</div>

<style>
  .sub { border: 1px solid var(--bg-1); padding: 0.3rem; display: flex; flex-direction: column; gap: 0.25rem; }
  .sub-head { display: flex; align-items: center; gap: 0.4rem; }
  .sub-head .sk { color: var(--ok); background: var(--bg-0); padding: 0 0.3rem; font-size: 0.76rem; }
  .nm { font-size: 0.76rem; color: var(--text-1); }
  .spacer { flex: 1; }
  .modblock { border: 1px solid var(--bg-1); padding: 0.25rem; }
  .mb-head { display: flex; align-items: center; gap: 0.4rem; margin-bottom: 0.2rem; }
  .mb-head code { color: var(--ok); background: var(--bg-0); padding: 0 0.3rem; font-size: 0.74rem; }
  .tag-abs, .tag-raw { font-size: 0.65rem; text-transform: uppercase; color: var(--text-2); }
  .tag-raw { color: var(--warn); }
  .rawrow { display: flex; flex-wrap: wrap; align-items: center; gap: 0.25rem; }
  .idchip { color: var(--text-1); background: var(--bg-0); padding: 0.05rem 0.3rem; font-size: 0.72rem; }
  .idchip.raw { color: var(--text-2); font-style: italic; }
  .mini { border: 1px solid var(--border-strong); background: var(--bg-2); color: var(--text-1); font-family: inherit; font-size: 0.72rem; padding: 0.05rem 0.4rem; cursor: pointer; }
  .mini:hover { border-color: var(--accent); background: var(--accent); color: var(--text-inverse); }
  .mini.danger { color: var(--err); border-color: var(--danger-bg); }
  .dim { color: var(--text-2); }
  .small { font-size: 0.74rem; }
</style>
