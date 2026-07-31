<!--
  EstatesOverlay — View ▸ Estates… (Sprint 20).

  The Sprint-31 pilot workspace view with three content tabs (Estates / Privileges / Agendas),
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
  import { TabStrip } from "$lib/components/workspace";
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
    installPath,
    modPath = null,
    date = null,
    queue,
    focusKey = null,
    onfocused,
    onopencountry,
  }: {
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
    if (!focusKey || !data) return;
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

<div class="estates-view">
  <div class="view-toolbar">
    <input class="search" type="text" placeholder="Search…" bind:value={search} />
    <label class="modonly">
      <input type="checkbox" bind:checked={modOnly} />
      Mod only
    </label>
    <span class="counter">{shown.length}</span>
  </div>

  <div class="body">
    <TabStrip
      tier="content"
      tabs={TABS.map((t) => ({ id: t, label: KIND_SCHEMAS[t].label, count: data ? listOf(data, t).length : undefined }))}
      activeId={tab}
      onselect={(id) => switchTab(id as EstateKind)}
    />

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
</div>

<style>
  .search {
    background: var(--bg-1);
    border: 1px solid var(--border);
    color: var(--text-1);
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
    color: var(--text-1);
  }
  .counter {
    font-size: 0.8rem;
    color: var(--text-2);
  }
  .body {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  .newrow {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    flex-wrap: wrap;
  }
  .estsel,
  .newkey {
    background: var(--bg-1);
    border: 1px solid var(--border);
    color: var(--text-1);
    font-family: inherit;
    font-size: 0.83rem;
    padding: 0.25rem 0.4rem;
  }
  .newkey {
    width: 15rem;
  }
  .newbtn {
    border: 1px solid var(--border);
    background: var(--bg-3);
    color: var(--text-1);
    font-family: inherit;
    font-size: 0.82rem;
    padding: 0.28rem 0.7rem;
    cursor: pointer;
  }
  .newbtn:hover {
    background: var(--accent);
    color: var(--text-inverse);
  }
  .newerr {
    color: var(--err);
    font-size: 0.78rem;
  }
  .msg {
    margin: 0.2rem 0;
    font-size: 0.85rem;
    color: var(--text-2);
  }
  .msg.err {
    color: var(--err);
  }
  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
  }
  .row {
    border: 1px solid var(--border);
    border-bottom: none;
  }
  .row:last-child {
    border-bottom: 1px solid var(--border);
  }
  .row.expanded {
    background: var(--bg-2);
  }
  .rowmain {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    width: 100%;
    text-align: left;
    border: none;
    background: transparent;
    color: var(--text-1);
    font-family: inherit;
    font-size: 0.86rem;
    padding: 0.35rem 0.5rem;
    cursor: pointer;
  }
  .rowmain:hover {
    background: var(--bg-3);
  }
  .caret {
    color: var(--text-2);
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
    color: var(--ok);
    background: var(--bg-0);
    padding: 0 0.3rem;
    font-size: 0.76rem;
  }
  .badge {
    font-size: 0.68rem;
    text-transform: uppercase;
    letter-spacing: 0.03em;
    padding: 0.05rem 0.35rem;
    border: 1px solid var(--border);
  }
  .badge.origin.base {
    background: var(--bg-3);
    color: var(--text-1);
  }
  .badge.origin.mod {
    background: var(--ok);
    color: var(--text-inverse);
  }
  .file {
    margin-left: auto;
    color: var(--text-3);
    font-size: 0.72rem;
    white-space: nowrap;
  }
  .rowbody {
    padding: 0 0.6rem 0.4rem;
  }
  .estates-view { min-height: 100%; }
  .view-toolbar {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    margin-bottom: var(--sp-3);
  }
</style>
