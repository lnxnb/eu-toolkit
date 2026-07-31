<!--
  MechanicFamilyList — a generic list + create + MechanicObjectEditor for one
  (hidden) mechanics family, reused by the Empires overlay for decrees and
  imperial incidents (Sprint 29). An optional `extra` snippet renders extra
  per-object UI in the expanded body (the incidents' numbered AI-weight options).
-->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import type { Snippet } from "svelte";
  import type { KnownKey } from "$lib/components/script";
  import type { DropdownItem, KnownModifier } from "$lib/components/ui";
  import type { EditQueue, TypedEdit } from "$lib/edits.svelte";
  import MechanicObjectEditor from "$lib/components/mechanics/MechanicObjectEditor.svelte";
  import {
    foldMechanics,
    isValidKey,
    slugify,
    type MechanicsData,
    type MechanicObject,
    type Scaffold,
  } from "$lib/mechanics";

  let {
    installPath,
    modPath,
    date = null,
    queue,
    data,
    known,
    triggers,
    effects,
    countries,
    pickerItems,
    createLabel,
    onopenevents,
    extra,
  }: {
    installPath: string;
    modPath: string | null;
    date?: string | null;
    queue: EditQueue;
    data: MechanicsData;
    known: KnownModifier[];
    triggers: KnownKey[];
    effects: KnownKey[];
    countries: DropdownItem[];
    pickerItems: Record<string, DropdownItem[]>;
    createLabel: string;
    onopenevents?: (id: string) => void;
    extra?: Snippet<[MechanicObject]>;
  } = $props();

  const meta = $derived(data.meta);
  const folded = $derived<MechanicsData>((queue.version, foldMechanics(data, queue.serialize())));
  const objects = $derived<MechanicObject[]>(folded.objects);
  const keySet = $derived(new Set(objects.map((o) => o.key)));

  let expandedKey = $state<string | null>(null);
  let newName = $state("");
  let newError = $state<string | null>(null);

  function nameOf(o: MechanicObject): string {
    return queue.pendingLocOverride(o.nameKey) ?? o.name;
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

  function removeObject(o: MechanicObject) {
    if (!confirm(`Delete "${o.key}"?`)) return;
    queue.push({ label: `Delete ${o.key}`, edits: [{ kind: "removeStatement", file: o.file, blockPath: [], key: o.key }] });
    if (expandedKey === o.key) expandedKey = null;
  }

  async function createObject() {
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
    let sc: Scaffold;
    try {
      sc = await invoke<Scaffold>("scaffold_mechanic", { family: meta.id, key });
    } catch (e) {
      newError = String(e);
      return;
    }
    const edits: TypedEdit[] = [
      wrapperExists(meta.projectFile)
        ? { kind: "appendText", file: meta.projectFile, text: "\n" + sc.text + "\n" }
        : { kind: "createFile", file: meta.projectFile, text: sc.text + "\n" },
    ];
    for (const le of sc.locEntries) edits.push({ kind: "locOverride", key: le.key, value: le.value });
    queue.push({ label: `Create ${key}`, edits });
    newName = "";
    expandedKey = key;
  }
</script>

<div class="ml">
  <div class="newrow">
    <input class="newkey" type="text" placeholder="New name…" bind:value={newName} onkeydown={(e) => e.key === "Enter" && createObject()} />
    <button class="newbtn" onclick={createObject}>＋ New {createLabel}</button>
    {#if newError}<span class="newerr">{newError}</span>{/if}
  </div>
  {#if objects.length === 0}<p class="msg">Nothing here yet.</p>{/if}
  <ul class="list">
    {#each objects as o (o.file + "::" + o.key)}
      <li class="row" class:expanded={expandedKey === o.key}>
        <button class="rowmain" onclick={() => toggle(o.key)}>
          <span class="caret">{expandedKey === o.key ? "▾" : "▸"}</span>
          <span class="title">{nameOf(o)}</span>
          <code class="key">{o.key}</code>
          <span class="badge origin {o.origin}">{o.origin}</span>
          <span class="file">{o.file.split("/").pop()}</span>
        </button>
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
              onremove={() => removeObject(o)}
              {onopenevents}
            />
            {#if extra}{@render extra(o)}{/if}
          </div>
        {/if}
      </li>
    {/each}
  </ul>
</div>

<style>
  .ml { display: flex; flex-direction: column; gap: 0.5rem; }
  .newrow { display: flex; align-items: center; gap: 0.5rem; flex-wrap: wrap; }
  .newkey { background: var(--bg-1); border: 1px solid var(--border); color: var(--text-1); font-family: inherit; font-size: 0.83rem; padding: 0.25rem 0.4rem; width: 16rem; }
  .newbtn { border: 1px solid var(--border); background: var(--bg-3); color: var(--text-1); font-family: inherit; font-size: 0.82rem; padding: 0.28rem 0.7rem; cursor: pointer; }
  .newbtn:hover { background: var(--accent); color: var(--text-inverse); }
  .newerr { color: var(--err); font-size: 0.78rem; }
  .msg { color: var(--text-2); font-size: 0.85rem; margin: 0.2rem 0; }
  .list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; }
  .row { border: 1px solid var(--border); border-bottom: none; }
  .row:last-child { border-bottom: 1px solid var(--border); }
  .row.expanded { background: var(--bg-2); }
  .rowmain { display: flex; align-items: center; gap: 0.5rem; width: 100%; text-align: left; border: none; background: transparent; color: var(--text-1); font-family: inherit; font-size: 0.86rem; padding: 0.35rem 0.5rem; cursor: pointer; }
  .rowmain:hover { background: var(--bg-3); }
  .caret { color: var(--text-2); width: 0.8rem; flex: none; }
  .title { font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 18rem; }
  .key { color: var(--ok); background: var(--bg-0); padding: 0 0.3rem; font-size: 0.76rem; }
  .badge { font-size: 0.68rem; text-transform: uppercase; letter-spacing: 0.03em; padding: 0.05rem 0.35rem; border: 1px solid var(--border); }
  .badge.origin.base { background: var(--bg-3); color: var(--text-1); }
  .badge.origin.mod { background: var(--ok); color: var(--text-inverse); }
  .file { margin-left: auto; color: var(--text-3); font-size: 0.72rem; white-space: nowrap; }
  .rowbody { padding: 0 0.6rem 0.4rem; }
</style>
