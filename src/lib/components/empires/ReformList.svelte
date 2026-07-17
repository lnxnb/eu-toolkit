<!--
  ReformList — imperial reforms for ONE empire (hre | celestial_empire), Sprint 29.
  File order = progression order. Each row expands into the shared
  MechanicObjectEditor (typed scope-modifier blocks emperor/member/elector/all/
  province/emperor_per_prince, potential/trigger/on_effect/off_effect trees, loc,
  preserve-unknown gui/art keys). `required_reform` renders as a jump link to the
  prerequisite reform. "＋ New reform" scaffolds a chain-appended reform
  (required_reform = current tail) so it loads with zero manual fixes.
-->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import type { KnownKey } from "$lib/components/script";
  import type { DropdownItem, KnownModifier } from "$lib/components/ui";
  import type { EditQueue, TypedEdit } from "$lib/edits.svelte";
  import MechanicObjectEditor from "$lib/components/mechanics/MechanicObjectEditor.svelte";
  import { foldMechanics, isValidKey, slugify, type FamilyMeta, type MechanicsData, type MechanicObject } from "$lib/mechanics";
  import type { ReformScaffold } from "$lib/empires";

  let {
    installPath,
    modPath,
    empire,
    date = null,
    queue,
    data,
    known,
    triggers,
    effects,
    countries,
    pickerItems,
    onopenevents,
  }: {
    installPath: string;
    modPath: string | null;
    empire: "hre" | "celestial_empire";
    date?: string | null;
    queue: EditQueue;
    /** The full imperial_reforms MechanicsData (both empires); filtered here. */
    data: MechanicsData;
    known: KnownModifier[];
    triggers: KnownKey[];
    effects: KnownKey[];
    countries: DropdownItem[];
    pickerItems: Record<string, DropdownItem[]>;
    onopenevents?: (id: string) => void;
  } = $props();

  const meta = $derived<FamilyMeta>(data.meta);
  const folded = $derived<MechanicsData>((queue.version, foldMechanics(data, queue.serialize())));
  const reforms = $derived<MechanicObject[]>(
    folded.objects.filter((o) => o.scalars.some((s) => s.key === "empire" && s.value === empire)),
  );
  const keySet = $derived(new Set(folded.objects.map((o) => o.key)));

  let expandedKey = $state<string | null>(null);
  let newName = $state("");
  let newError = $state<string | null>(null);

  function reqOf(o: MechanicObject): string | null {
    const s = o.scalars.find((x) => x.key === "required_reform" && x.present);
    return s ? s.value : null;
  }
  function nameOf(o: MechanicObject): string {
    return queue.pendingLocOverride(o.nameKey) ?? o.name;
  }
  function jumpTo(key: string) {
    if (reforms.some((o) => o.key === key)) expandedKey = key;
  }
  function toggle(k: string) {
    expandedKey = expandedKey === k ? null : k;
  }

  function wrapperExists(file: string): boolean {
    return (
      data.objects.some((o) => o.file === file) ||
      queue.findLast((e) => (e.kind === "createFile" || e.kind === "appendText") && e.file === file) != null
    );
  }

  function removeReform(o: MechanicObject) {
    if (!confirm(`Delete reform "${o.key}"?`)) return;
    queue.push({ label: `Delete reform ${o.key}`, edits: [{ kind: "removeStatement", file: o.file, blockPath: [], key: o.key }] });
    if (expandedKey === o.key) expandedKey = null;
  }

  async function createReform() {
    newError = null;
    const key = slugify(newName.trim());
    if (!isValidKey(key)) {
      newError = "Use lowercase letters, digits and underscores.";
      return;
    }
    if (keySet.has(key)) {
      newError = `"${key}" already exists.`;
      return;
    }
    const tail = reforms.length > 0 ? reforms[reforms.length - 1].key : null;
    let sc: ReformScaffold;
    try {
      sc = await invoke<ReformScaffold>("scaffold_imperial_reform_chain", { empire, requiredReform: tail, key });
    } catch (e) {
      newError = String(e);
      return;
    }
    const edits: TypedEdit[] = [
      wrapperExists(sc.file)
        ? { kind: "appendText", file: sc.file, text: "\n" + sc.text + "\n" }
        : { kind: "createFile", file: sc.file, text: sc.text + "\n" },
    ];
    for (const le of sc.locEntries) edits.push({ kind: "locOverride", key: le.key, value: le.value });
    queue.push({ label: `Create reform ${key}`, edits });
    newName = "";
    expandedKey = key;
  }
</script>

<div class="rl">
  <div class="newrow">
    <input class="newkey" type="text" placeholder="New reform name…" bind:value={newName} onkeydown={(e) => e.key === "Enter" && createReform()} />
    <button class="newbtn" onclick={createReform}>＋ New reform</button>
    {#if newError}<span class="newerr">{newError}</span>{/if}
  </div>
  {#if reforms.length === 0}
    <p class="msg">No reforms for this empire.</p>
  {/if}
  <ol class="list">
    {#each reforms as o, i (o.file + "::" + o.key)}
      <li class="row" class:expanded={expandedKey === o.key}>
        <div class="rowhead">
          <button class="rowmain" onclick={() => toggle(o.key)}>
            <span class="ord">{i + 1}</span>
            <span class="caret">{expandedKey === o.key ? "▾" : "▸"}</span>
            <span class="title">{nameOf(o)}</span>
            <code class="key">{o.key}</code>
          </button>
          {#if reqOf(o)}
            <span class="req">requires
              <button class="link" onclick={() => jumpTo(reqOf(o)!)}>{reqOf(o)}</button>
            </span>
          {/if}
          <span class="badge origin {o.origin}">{o.origin}</span>
        </div>
        {#if expandedKey === o.key}
          <div class="rowbody">
            <MechanicObjectEditor
              {installPath}
              {modPath}
              {date}
              {queue}
              obj={o}
              {meta}
              {known}
              {triggers}
              {effects}
              {countries}
              {pickerItems}
              onremove={() => removeReform(o)}
              {onopenevents}
            />
          </div>
        {/if}
      </li>
    {/each}
  </ol>
</div>

<style>
  .rl { display: flex; flex-direction: column; gap: 0.5rem; }
  .newrow { display: flex; align-items: center; gap: 0.5rem; flex-wrap: wrap; }
  .newkey { background: #21262e; border: 1px solid #1f242c; color: #cfd4db; font-family: inherit; font-size: 0.83rem; padding: 0.25rem 0.4rem; width: 16rem; }
  .newbtn { border: 1px solid #1f242c; background: #3f4855; color: #cfd4db; font-family: inherit; font-size: 0.82rem; padding: 0.28rem 0.7rem; cursor: pointer; }
  .newbtn:hover { background: #4a6da7; color: #fff; }
  .newerr { color: #d9756b; font-size: 0.78rem; }
  .msg { color: #8a919c; font-size: 0.85rem; margin: 0.2rem 0; }
  .list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; counter-reset: r; }
  .row { border: 1px solid #1f242c; border-bottom: none; }
  .row:last-child { border-bottom: 1px solid #1f242c; }
  .row.expanded { background: #262d37; }
  .rowhead { display: flex; align-items: center; gap: 0.5rem; padding-right: 0.5rem; }
  .rowmain { display: flex; align-items: center; gap: 0.5rem; flex: 1; min-width: 0; text-align: left; border: none; background: transparent; color: #cfd4db; font-family: inherit; font-size: 0.86rem; padding: 0.35rem 0.5rem; cursor: pointer; }
  .rowmain:hover { background: #303844; }
  .ord { color: #6d7683; font-size: 0.72rem; width: 1.3rem; text-align: right; flex: none; }
  .caret { color: #8a919c; width: 0.8rem; flex: none; }
  .title { font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 16rem; }
  .key { color: #9aecc0; background: #16191f; padding: 0 0.3rem; font-size: 0.76rem; }
  .req { color: #8a919c; font-size: 0.74rem; }
  .link { background: none; border: none; color: #7fb2ff; cursor: pointer; font-family: monospace; font-size: 0.74rem; text-decoration: underline; padding: 0; }
  .badge { font-size: 0.68rem; text-transform: uppercase; letter-spacing: 0.03em; padding: 0.05rem 0.35rem; border: 1px solid #1f242c; margin-left: auto; }
  .badge.origin.base { background: #3f4855; color: #cfd4db; }
  .badge.origin.mod { background: #3f8a6d; color: #fff; }
  .rowbody { padding: 0 0.6rem 0.4rem; }
</style>
