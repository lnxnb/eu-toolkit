<!--
  RebelsOverlay — View ▸ Rebels… (Sprint 21).

  A full-screen OverlaySurface: a searchable faction list (origin badge base/mod)
  → expand editor (RebelObjectEditor: loc name/title/desc, color, typed
  scalars/enums/flags, 14.2 trigger/effect/weight trees, start-date revolt
  context, preserve-unknown). "＋ New faction" scaffolds a project zz_ file entry
  + loc keys (name/title/desc/demand), minimal viable structure — zero-manual-fixes.
-->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { OverlaySurface } from "$lib/components/script";
  import type { KnownKey } from "$lib/components/script";
  import type { DropdownItem } from "$lib/components/ui";
  import type { EditQueue, TypedEdit } from "$lib/edits.svelte";
  import RebelObjectEditor from "./RebelObjectEditor.svelte";
  import {
    foldRebels,
    isValidKey,
    slugify,
    allKeys,
    REBELS_PROJECT_FILE,
    type RebelsData,
    type RebelFaction,
    type Scaffold,
  } from "$lib/rebels";

  let {
    open = $bindable(false),
    installPath,
    modPath = null,
    date = null,
    queue,
    onopenprovince,
  }: {
    open?: boolean;
    installPath: string;
    modPath?: string | null;
    date?: string | null;
    queue: EditQueue;
    onopenprovince?: (id: number) => void;
  } = $props();

  interface CountryBrief {
    tag: string;
    name: string;
    color: [number, number, number] | null;
  }

  let fetched = $state<RebelsData | null>(null);
  let triggers = $state<KnownKey[]>([]);
  let effects = $state<KnownKey[]>([]);
  let countries = $state<DropdownItem[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);

  let search = $state("");
  let modOnly = $state(false);
  let expandedKey = $state<string | null>(null);
  let newName = $state("");
  let newError = $state<string | null>(null);

  $effect(() => {
    if (!open) return;
    void load(installPath, modPath);
  });

  async function load(install: string, mod: string | null) {
    loading = true;
    error = null;
    try {
      const [data, trig, eff, ctys] = await Promise.all([
        invoke<RebelsData>("get_rebels", { installPath: install, modPath: mod }),
        invoke<KnownKey[]>("get_known_triggers"),
        invoke<KnownKey[]>("get_known_effects"),
        invoke<CountryBrief[]>("list_countries", { installPath: install, modPath: mod }),
      ]);
      fetched = data;
      triggers = trig;
      effects = eff;
      countries = ctys.map((c) => ({
        key: c.tag,
        label: c.name,
        swatch: c.color ? `rgb(${c.color[0]}, ${c.color[1]}, ${c.color[2]})` : undefined,
      }));
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  const data = $derived<RebelsData | null>(
    fetched ? ((queue.version, foldRebels(fetched, queue.serialize()))) : null,
  );
  const factions = $derived<RebelFaction[]>(data?.factions ?? []);
  const keys = $derived(data ? allKeys(data) : new Set<string>());

  function titleOf(o: RebelFaction): string {
    return queue.pendingLocOverride(o.titleKey) ?? o.title;
  }

  const shown = $derived(
    factions.filter((o) => {
      if (modOnly && o.origin !== "mod") return false;
      const q = search.trim().toLowerCase();
      if (!q) return true;
      return o.key.toLowerCase().includes(q) || titleOf(o).toLowerCase().includes(q);
    }),
  );

  function toggle(k: string) {
    expandedKey = expandedKey === k ? null : k;
  }

  // --- Delete ---
  function removeObject(o: RebelFaction) {
    if (!confirm(`Delete rebel faction "${o.key}"?`)) return;
    queue.push({
      label: `Delete faction ${o.key}`,
      edits: [{ kind: "removeStatement", file: o.file, blockPath: [], key: o.key }],
    });
    if (expandedKey === o.key) expandedKey = null;
  }

  // --- ＋ New … ---
  function wrapperExists(): boolean {
    return (
      (fetched ? fetched.factions.some((o) => o.file === REBELS_PROJECT_FILE) : false) ||
      queue.findLast((e) => (e.kind === "createFile" || e.kind === "appendText") && e.file === REBELS_PROJECT_FILE) != null
    );
  }

  async function createObject() {
    newError = null;
    const key = slugify(newName.trim());
    if (!isValidKey(key)) {
      newError = "Use lowercase letters, digits and underscores (start with a letter).";
      return;
    }
    if (keys.has(key)) {
      newError = `A faction named "${key}" already exists.`;
      return;
    }
    let scaffold: Scaffold;
    try {
      scaffold = await invoke<Scaffold>("scaffold_rebel_faction", { key });
    } catch (e) {
      newError = String(e);
      return;
    }
    const edits: TypedEdit[] = [];
    edits.push(
      wrapperExists()
        ? { kind: "appendText", file: REBELS_PROJECT_FILE, text: "\n" + scaffold.text + "\n" }
        : { kind: "createFile", file: REBELS_PROJECT_FILE, text: scaffold.text + "\n" },
    );
    for (const le of scaffold.locEntries) {
      edits.push({ kind: "locOverride", key: le.key, value: le.value });
    }
    queue.push({ label: `Create faction ${key}`, edits });
    newName = "";
    expandedKey = key;
  }
</script>

<OverlaySurface bind:open title="Rebel Factions">
  {#snippet toolbar()}
    <input class="search" type="text" placeholder="Search…" bind:value={search} />
    <label class="modonly">
      <input type="checkbox" bind:checked={modOnly} />
      Mod only
    </label>
    <span class="counter">{shown.length}</span>
  {/snippet}

  <div class="body">
    <div class="newrow">
      <input class="newkey" type="text" placeholder="New faction name…" bind:value={newName}
        onkeydown={(e) => e.key === "Enter" && createObject()} />
      <button class="newbtn" onclick={createObject}>＋ New faction</button>
      {#if newError}<span class="newerr">{newError}</span>{/if}
    </div>

    {#if loading}
      <p class="msg">Loading rebel factions…</p>
    {:else if error}
      <p class="msg err">{error}</p>
    {:else if shown.length === 0}
      <p class="msg">Nothing matches.</p>
    {/if}

    <ul class="list">
      {#each shown as o (o.file + "::" + o.key)}
        <li class="row" class:expanded={expandedKey === o.key} id={`reb-row-${o.key}`}>
          <button class="rowmain" onclick={() => toggle(o.key)}>
            <span class="caret">{expandedKey === o.key ? "▾" : "▸"}</span>
            {#if o.color}<span class="cswatch" style={`background: rgb(${o.color[0]}, ${o.color[1]}, ${o.color[2]})`}></span>{/if}
            <span class="title">{titleOf(o)}</span>
            <code class="key">{o.key}</code>
            <span class="badge origin {o.origin}">{o.origin}</span>
            <span class="file">{o.file.split("/").pop()}</span>
          </button>
          {#if expandedKey === o.key}
            <div class="rowbody">
              <RebelObjectEditor
                {installPath}
                {modPath}
                {date}
                {queue}
                obj={o}
                {triggers}
                {effects}
                {countries}
                onremove={() => removeObject(o)}
                {onopenprovince}
              />
            </div>
          {/if}
        </li>
      {/each}
    </ul>
  </div>
</OverlaySurface>

<style>
  .search { background: #21262e; border: 1px solid #1f242c; color: #cfd4db; font-family: inherit; font-size: 0.83rem; padding: 0.2rem 0.4rem; width: 16rem; }
  .modonly { display: flex; align-items: center; gap: 0.3rem; font-size: 0.8rem; color: #cfd4db; }
  .counter { font-size: 0.8rem; color: #8a919c; }
  .body { display: flex; flex-direction: column; gap: 0.5rem; }
  .newrow { display: flex; align-items: center; gap: 0.5rem; flex-wrap: wrap; }
  .newkey { background: #21262e; border: 1px solid #1f242c; color: #cfd4db; font-family: inherit; font-size: 0.83rem; padding: 0.25rem 0.4rem; width: 18rem; }
  .newbtn { border: 1px solid #1f242c; background: #3f4855; color: #cfd4db; font-family: inherit; font-size: 0.82rem; padding: 0.28rem 0.7rem; cursor: pointer; }
  .newbtn:hover { background: #4a6da7; color: #fff; }
  .newerr { color: #d9756b; font-size: 0.78rem; }
  .msg { margin: 0.2rem 0; font-size: 0.85rem; color: #8a919c; }
  .msg.err { color: #d9756b; }
  .list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; }
  .row { border: 1px solid #1f242c; border-bottom: none; }
  .row:last-child { border-bottom: 1px solid #1f242c; }
  .row.expanded { background: #262d37; }
  .rowmain { display: flex; align-items: center; gap: 0.5rem; width: 100%; text-align: left; border: none; background: transparent; color: #cfd4db; font-family: inherit; font-size: 0.86rem; padding: 0.35rem 0.5rem; cursor: pointer; }
  .rowmain:hover { background: #303844; }
  .caret { color: #8a919c; width: 0.8rem; flex: none; }
  .cswatch { width: 0.85rem; height: 0.85rem; border: 1px solid #1f242c; flex: none; }
  .title { font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 18rem; }
  .key { color: #9aecc0; background: #16191f; padding: 0 0.3rem; font-size: 0.76rem; }
  .badge { font-size: 0.68rem; text-transform: uppercase; letter-spacing: 0.03em; padding: 0.05rem 0.35rem; border: 1px solid #1f242c; }
  .badge.origin.base { background: #3f4855; color: #cfd4db; }
  .badge.origin.mod { background: #3f8a6d; color: #fff; }
  .file { margin-left: auto; color: #6d7683; font-size: 0.72rem; white-space: nowrap; }
  .rowbody { padding: 0 0.6rem 0.4rem; }
</style>
