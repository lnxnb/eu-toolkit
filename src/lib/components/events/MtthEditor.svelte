<!--
  MtthEditor — the "likelihood" editor for an event's `mean_time_to_happen`
  block (Sprint 16): a base months/years/days stepper plus add/remove modifier
  rows (each factor + a trigger tree via ModifierRow).

  Fed the parsed MTTH ScriptBlock by the host; base + modifier nodes are derived
  from it. Modifier occurrence paths come straight from the block's node paths
  (`modifier`, `modifier#1`, …), so removal is a byte-surgical removeStatement on
  the occurrence-qualified segment; the host re-parses after every edit, so the
  indices stay valid across a sequence of removals.
-->
<script lang="ts">
  import type { KnownKey, ScriptBlock } from "$lib/components/script";
  import type { DropdownItem } from "$lib/components/ui";
  import type { EditQueue, TypedEdit } from "$lib/edits.svelte";
  import ModifierRow from "./ModifierRow.svelte";

  let {
    file,
    mtthPath,
    block,
    installPath,
    modPath = null,
    queue,
    triggers,
    countries = [],
    onedit,
  }: {
    file: string;
    mtthPath: string[];
    block: ScriptBlock;
    installPath: string;
    modPath?: string | null;
    queue: EditQueue;
    triggers: KnownKey[];
    countries?: DropdownItem[];
    onedit: (edits: TypedEdit[], label: string) => void;
  } = $props();

  const UNITS = ["months", "years", "days"] as const;
  type Unit = (typeof UNITS)[number];

  // The base unit node (months/years/days) — the first one present.
  const baseNode = $derived(block.nodes.find((n) => UNITS.includes(n.key as Unit)));
  const baseUnit = $derived<Unit>((baseNode?.key as Unit) ?? "months");
  const baseValue = $derived(baseNode?.value?.text ?? "");

  // Modifier nodes, in file order, with their occurrence-qualified paths.
  const modifierNodes = $derived(block.nodes.filter((n) => n.key === "modifier"));

  function commitBaseValue(value: string) {
    if (value === baseValue) return;
    const edit: TypedEdit = baseNode
      ? { kind: "setScalar", file, path: [...mtthPath, baseUnit], value, quoted: false }
      : { kind: "insertStatement", file, blockPath: mtthPath, statement: `${baseUnit} = ${value}` };
    onedit([edit], "Edit MTTH base");
  }

  function commitBaseUnit(next: Unit) {
    if (next === baseUnit) return;
    const value = baseValue || "1";
    const edits: TypedEdit[] = [];
    if (baseNode) {
      // Swap the unit: remove the old key, insert the new one carrying the value.
      edits.push({ kind: "removeStatement", file, blockPath: mtthPath, key: baseUnit, value: null });
    }
    edits.push({ kind: "insertStatement", file, blockPath: mtthPath, statement: `${next} = ${value}` });
    onedit(edits, "Change MTTH unit");
  }

  function addModifier() {
    onedit(
      [
        {
          kind: "insertStatement",
          file,
          blockPath: mtthPath,
          statement: "modifier = {\n\t\t\tfactor = 1\n\t\t}",
        },
      ],
      "Add MTTH modifier",
    );
  }

  function removeModifier(path: string[]) {
    // The final path segment is the occurrence-qualified `modifier#n`; the
    // backend remove splits the `#n` suffix.
    const key = path[path.length - 1];
    onedit([{ kind: "removeStatement", file, blockPath: mtthPath, key, value: null }], "Remove MTTH modifier");
  }
</script>

<div class="mtth">
  <div class="base">
    <span class="lbl">Base mean time:</span>
    <input
      class="baseval"
      type="number"
      step="1"
      min="0"
      placeholder="value"
      value={baseValue}
      onchange={(e) => commitBaseValue(e.currentTarget.value)}
    />
    <select class="unit" value={baseUnit} onchange={(e) => commitBaseUnit(e.currentTarget.value as Unit)}>
      {#each UNITS as u (u)}
        <option value={u}>{u}</option>
      {/each}
    </select>
    {#if !baseNode}
      <span class="hint">No base set — type a value to add one.</span>
    {/if}
  </div>

  <div class="mods">
    {#each modifierNodes as node, i (node.path.join("/"))}
      <ModifierRow
        {file}
        modifierPath={node.path}
        {installPath}
        {modPath}
        {queue}
        {triggers}
        {countries}
        index={i + 1}
        {onedit}
        onremove={() => removeModifier(node.path)}
      />
    {/each}
    <button class="addmod" onclick={addModifier}>＋ Add modifier</button>
  </div>
</div>

<style>
  .mtth {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .base {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    flex-wrap: wrap;
  }

  .lbl {
    font-size: 0.8rem;
    color: #9aa2ad;
  }

  .baseval {
    width: 6rem;
    background: #21262e;
    border: 1px solid #1f242c;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.85rem;
    padding: 0.2rem 0.35rem;
  }

  .unit {
    background: #21262e;
    border: 1px solid #1f242c;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.82rem;
    padding: 0.2rem 0.3rem;
  }

  .hint {
    font-size: 0.74rem;
    color: #8a919c;
  }

  .mods {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }

  .addmod {
    align-self: flex-start;
    border: 1px solid #1f242c;
    background: #3f4855;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.8rem;
    padding: 0.25rem 0.6rem;
    cursor: pointer;
  }

  .addmod:hover {
    background: #4a6da7;
    color: #fff;
  }
</style>
