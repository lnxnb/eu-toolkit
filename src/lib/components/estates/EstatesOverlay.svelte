<!--
  EstatesOverlay — View ▸ Estates… (Sprint 20).

  A full-screen OverlaySurface with three tabs (Estates / Privileges / Agendas),
  each a searchable list (origin badge base/mod) → expand editor. Expanding opens
  the shared EstateObjectEditor (loc, icon, typed scalars + modifier rows, 14.2
  trigger/effect trees, availability for privileges, preserve-unknown). "＋ New …"
  scaffolds a project zz_ file entry + loc keys; privileges/agendas reference their
  estate (a privilege registers into that estate's `privileges = {}` list — the
  actual EU4 reference direction; there is no `estates =` key on privileges).

  `focusKey` (from a country-panel jump link) auto-expands + scrolls to that object.
-->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { OverlaySurface } from "$lib/components/script";
  import type { KnownKey } from "$lib/components/script";
  import type { DropdownItem, KnownModifier } from "$lib/components/ui";
  import type { EditQueue, TypedEdit } from "$lib/edits.svelte";
  import EstateObjectEditor from "./EstateObjectEditor.svelte";
  import {
    foldEstates,
    isValidKey,
    slugify,
    allKeys,
    listOf,
    KIND_SCHEMAS,
    type EstatesData,
    type EstateKind,
    type EstateObject,
    type Scaffold,
  } from "$lib/estates";

  let {
    open = $bindable(false),
    installPath,
    modPath = null,
    date = null,
    queue,
    focusKey = null,
    onfocused,
    onopencountry,
  }: {
    open?: boolean;
    installPath: string;
    modPath?: string | null;
    date?: string | null;
    queue: EditQueue;
    focusKey?: string | null;
    onfocused?: () => void;
    onopencountry?: (tag: string) => void;
  } = $props();

  interface CountryBrief {
    tag: string;
    name: string;
    color: [number, number, number] | null;
  }

  let fetched = $state<EstatesData | null>(null);
  let known = $state<KnownModifier[]>([]);
  let triggers = $state<KnownKey[]>([]);
  let effects = $state<KnownKey[]>([]);
  let countries = $state<DropdownItem[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);

  let tab = $state<EstateKind>("estate");
  let search = $state("");
  let modOnly = $state(false);
  let expandedKey = $state<string | null>(null);
  let newName = $state("");
  let newEstate = $state("");
  let newError = $state<string | null>(null);

  $effect(() => {
    if (!open) return;
    void load(installPath, modPath);
  });

  async function load(install: string, mod: string | null) {
    loading = true;
    error = null;
    try {
      const [data, mods, trig, eff, ctys] = await Promise.all([
        invoke<EstatesData>("get_estates", { installPath: install, modPath: mod }),
        invoke<KnownModifier[]>("get_known_modifiers"),
        invoke<KnownKey[]>("get_known_triggers"),
        invoke<KnownKey[]>("get_known_effects"),
        invoke<CountryBrief[]>("list_countries", { installPath: install, modPath: mod }),
      ]);
      fetched = data;
      known = mods;
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

  const data = $derived<EstatesData | null>(
    fetched ? ((queue.version, foldEstates(fetched, queue.serialize()))) : null,
  );
  const objects = $derived<EstateObject[]>(data ? listOf(data, tab) : []);
  const keys = $derived(data ? allKeys(data) : new Set<string>());
  const estateList = $derived(data?.estates ?? []);

  const shown = $derived(
    objects.filter((o) => {
      if (modOnly && o.origin !== "mod") return false;
      const q = search.trim().toLowerCase();
      if (!q) return true;
      return (
        o.key.toLowerCase().includes(q) ||
        (queue.pendingLocOverride(o.locKey) ?? o.name).toLowerCase().includes(q)
      );
    }),
  );

  const TABS: EstateKind[] = ["estate", "privilege", "agenda"];
  function titleOf(o: EstateObject): string {
    return queue.pendingLocOverride(o.locKey) ?? o.name;
  }
  function toggle(k: string) {
    expandedKey = expandedKey === k ? null : k;
  }
  function switchTab(t: EstateKind) {
    tab = t;
    expandedKey = null;
    newName = "";
    newError = null;
  }

  // Consume the focus request once loaded (auto-detect which tab the key is in).
  $effect(() => {
    if (!open || !focusKey || !data) return;
    const target = focusKey;
    for (const t of TABS) {
      if (listOf(data, t).some((o) => o.key === target)) {
        tab = t;
        break;
      }
    }
    expandedKey = target;
    onfocused?.();
    queueMicrotask(() => {
      document.getElementById(`est-row-${target}`)?.scrollIntoView({ block: "center" });
    });
  });

  // --- Delete ---
  function removeObject(o: EstateObject) {
    if (!confirm(`Delete ${o.kind} "${o.key}"?`)) return;
    const edits: TypedEdit[] = [
      { kind: "removeStatement", file: o.file, blockPath: [], key: o.key },
    ];
    // Also unregister a privilege/agenda from its owning estate's list.
    if (o.kind === "privilege" || o.kind === "agenda") {
      const listName = o.kind === "privilege" ? "privileges" : "agendas";
      for (const est of estateList) {
        const list = o.kind === "privilege" ? est.privileges : est.agendas;
        if (list.includes(o.key)) {
          edits.push({ kind: "removeId", file: est.file, listPath: [est.key, listName], id: o.key });
        }
      }
    }
    queue.push({ label: `Delete ${o.kind} ${o.key}`, edits });
    if (expandedKey === o.key) expandedKey = null;
  }

  // --- ＋ New … ---
  function projectFile(kind: EstateKind): string {
    return KIND_SCHEMAS[kind].projectFile;
  }
  function wrapperExists(kind: EstateKind): boolean {
    const pf = projectFile(kind);
    return (
      (fetched ? listOf(fetched, kind).some((o) => o.file === pf) : false) ||
      queue.findLast((e) => (e.kind === "createFile" || e.kind === "appendText") && e.file === pf) != null
    );
  }

  async function createObject() {
    newError = null;
    let key = slugify(newName.trim());
    // Privileges/agendas are conventionally prefixed with their estate key.
    if ((tab === "privilege" || tab === "agenda") && newEstate && !key.startsWith(newEstate)) {
      key = `${newEstate}_${key}`;
    }
    if (!isValidKey(key)) {
      newError = "Use lowercase letters, digits and underscores (start with a letter).";
      return;
    }
    if (keys.has(key)) {
      newError = `An object named "${key}" already exists.`;
      return;
    }
    if ((tab === "privilege" || tab === "agenda") && !newEstate) {
      newError = "Pick the owning estate first.";
      return;
    }
    let scaffold: Scaffold;
    try {
      scaffold = await invoke<Scaffold>("scaffold_estate_object", {
        kind: tab,
        key,
        estate: newEstate || null,
      });
    } catch (e) {
      newError = String(e);
      return;
    }
    const pf = projectFile(tab);
    const edits: TypedEdit[] = [];
    edits.push(
      wrapperExists(tab)
        ? { kind: "appendText", file: pf, text: "\n" + scaffold.text }
        : { kind: "createFile", file: pf, text: scaffold.text + "\n" },
    );
    edits.push({ kind: "locOverride", key: scaffold.locNameKey, value: scaffold.locName });
    // Register a privilege/agenda in its estate's list so the estate offers it.
    if (tab === "privilege" || tab === "agenda") {
      const est = estateList.find((e) => e.key === newEstate);
      if (est) {
        const listName = tab === "privilege" ? "privileges" : "agendas";
        edits.push({ kind: "addId", file: est.file, listPath: [est.key, listName], id: key });
      }
    }
    queue.push({ label: `Create ${tab} ${key}`, edits });
    newName = "";
    expandedKey = key;
  }
</script>

<OverlaySurface bind:open title="Estates">
  {#snippet toolbar()}
    <input class="search" type="text" placeholder="Search…" bind:value={search} />
    <label class="modonly">
      <input type="checkbox" bind:checked={modOnly} />
      Mod only
    </label>
    <span class="counter">{shown.length}</span>
  {/snippet}

  <div class="body">
    <div class="tabs">
      {#each TABS as t (t)}
        <button class="tabbtn" class:active={tab === t} onclick={() => switchTab(t)}>
          {KIND_SCHEMAS[t].label}
          {#if data}<span class="tabn">{listOf(data, t).length}</span>{/if}
        </button>
      {/each}
    </div>

    <div class="newrow">
      {#if tab === "privilege" || tab === "agenda"}
        <select class="estsel" bind:value={newEstate}>
          <option value="">— owning estate —</option>
          {#each estateList as e (e.key)}<option value={e.key}>{titleOf(e)} ({e.key})</option>{/each}
        </select>
      {/if}
      <input
        class="newkey"
        type="text"
        placeholder={`New ${tab} name…`}
        bind:value={newName}
        onkeydown={(e) => e.key === "Enter" && createObject()}
      />
      <button class="newbtn" onclick={createObject}>＋ New {tab}</button>
      {#if newError}<span class="newerr">{newError}</span>{/if}
    </div>

    {#if loading}
      <p class="msg">Loading estates…</p>
    {:else if error}
      <p class="msg err">{error}</p>
    {:else if shown.length === 0}
      <p class="msg">Nothing matches.</p>
    {/if}

    <ul class="list">
      {#each shown as o (o.file + "::" + o.key)}
        <li class="row" class:expanded={expandedKey === o.key} id={`est-row-${o.key}`}>
          <button class="rowmain" onclick={() => toggle(o.key)}>
            <span class="caret">{expandedKey === o.key ? "▾" : "▸"}</span>
            <span class="title">{titleOf(o)}</span>
            <code class="key">{o.key}</code>
            <span class="badge origin {o.origin}">{o.origin}</span>
            <span class="file">{o.file.split("/").pop()}</span>
          </button>
          {#if expandedKey === o.key}
            <div class="rowbody">
              <EstateObjectEditor
                {installPath}
                {modPath}
                {date}
                {queue}
                obj={o}
                {known}
                {triggers}
                {effects}
                {countries}
                onremove={() => removeObject(o)}
                {onopencountry}
              />
            </div>
          {/if}
        </li>
      {/each}
    </ul>
  </div>
</OverlaySurface>

<style>
  .search {
    background: #21262e;
    border: 1px solid #1f242c;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.83rem;
    padding: 0.2rem 0.4rem;
    width: 16rem;
  }
  .modonly {
    display: flex;
    align-items: center;
    gap: 0.3rem;
    font-size: 0.8rem;
    color: #cfd4db;
  }
  .counter {
    font-size: 0.8rem;
    color: #8a919c;
  }
  .body {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  .tabs {
    display: flex;
    gap: 0.15rem;
    border-bottom: 1px solid #1f242c;
  }
  .tabbtn {
    border: 1px solid #1f242c;
    border-bottom: none;
    background: #2b323d;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.85rem;
    padding: 0.3rem 0.8rem;
    cursor: pointer;
    display: flex;
    align-items: center;
    gap: 0.35rem;
  }
  .tabbtn.active {
    background: #4a6da7;
    color: #fff;
  }
  .tabn {
    font-size: 0.7rem;
    color: #8a919c;
  }
  .tabbtn.active .tabn {
    color: #dbe4f0;
  }
  .newrow {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    flex-wrap: wrap;
  }
  .estsel,
  .newkey {
    background: #21262e;
    border: 1px solid #1f242c;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.83rem;
    padding: 0.25rem 0.4rem;
  }
  .newkey {
    width: 15rem;
  }
  .newbtn {
    border: 1px solid #1f242c;
    background: #3f4855;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.82rem;
    padding: 0.28rem 0.7rem;
    cursor: pointer;
  }
  .newbtn:hover {
    background: #4a6da7;
    color: #fff;
  }
  .newerr {
    color: #d9756b;
    font-size: 0.78rem;
  }
  .msg {
    margin: 0.2rem 0;
    font-size: 0.85rem;
    color: #8a919c;
  }
  .msg.err {
    color: #d9756b;
  }
  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
  }
  .row {
    border: 1px solid #1f242c;
    border-bottom: none;
  }
  .row:last-child {
    border-bottom: 1px solid #1f242c;
  }
  .row.expanded {
    background: #262d37;
  }
  .rowmain {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    width: 100%;
    text-align: left;
    border: none;
    background: transparent;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.86rem;
    padding: 0.35rem 0.5rem;
    cursor: pointer;
  }
  .rowmain:hover {
    background: #303844;
  }
  .caret {
    color: #8a919c;
    width: 0.8rem;
    flex: none;
  }
  .title {
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 18rem;
  }
  .key {
    color: #9aecc0;
    background: #16191f;
    padding: 0 0.3rem;
    font-size: 0.76rem;
  }
  .badge {
    font-size: 0.68rem;
    text-transform: uppercase;
    letter-spacing: 0.03em;
    padding: 0.05rem 0.35rem;
    border: 1px solid #1f242c;
  }
  .badge.origin.base {
    background: #3f4855;
    color: #cfd4db;
  }
  .badge.origin.mod {
    background: #3f8a6d;
    color: #fff;
  }
  .file {
    margin-left: auto;
    color: #6d7683;
    font-size: 0.72rem;
    white-space: nowrap;
  }
  .rowbody {
    padding: 0 0.6rem 0.4rem;
  }
</style>
