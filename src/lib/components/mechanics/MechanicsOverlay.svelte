<!--
  MechanicsOverlay — View ▸ Mechanics… (Sprint 26).

  One config-driven OverlaySurface hosting all 14 country-interior mechanics
  families (disasters, parliament issues/bribes, court factions, and the religion
  sub-mechanics: personal deities, church aspects, fetishist cults, holy orders,
  fervor aspects, isolationism tiers, golden bulls, religious schools, religious
  reforms, and Shinto incidents). A family selector switches the list; each row
  expands into the shared MechanicObjectEditor (typed keys + 14.2 trees + loc +
  preserve-unknown). "＋ New" scaffolds a project zz_ file entry (or, for the
  group-nested schools family, inserts into a chosen religion group) + loc keys.
-->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { OverlaySurface } from "$lib/components/script";
  import type { KnownKey } from "$lib/components/script";
  import type { DropdownItem, KnownModifier } from "$lib/components/ui";
  import type { EditQueue, TypedEdit } from "$lib/edits.svelte";
  import MechanicObjectEditor from "./MechanicObjectEditor.svelte";
  import {
    foldMechanics,
    isValidKey,
    slugify,
    allKeys,
    type FamilyMeta,
    type MechanicsData,
    type MechanicObject,
    type Scaffold,
  } from "$lib/mechanics";

  let {
    open = $bindable(false),
    family = $bindable<string | null>(null),
    focusKey = $bindable<string | null>(null),
    installPath,
    modPath = null,
    date = null,
    queue,
    onopenevents,
    onopennaming,
  }: {
    open?: boolean;
    family?: string | null;
    /** Auto-expand this object key once the family loads (edit… jump). */
    focusKey?: string | null;
    installPath: string;
    modPath?: string | null;
    date?: string | null;
    queue: EditQueue;
    onopenevents?: (id: string) => void;
    /** Government ranks → Government Names overlay (ranks index naming tables). */
    onopennaming?: () => void;
  } = $props();

  interface CountryBrief {
    tag: string;
    name: string;
    color: [number, number, number] | null;
  }
  interface RegistryBrief {
    key: string;
    name: string;
  }

  let families = $state<FamilyMeta[]>([]);
  let selected = $state<string>("disasters");
  let fetched = $state<MechanicsData | null>(null);
  let known = $state<KnownModifier[]>([]);
  let triggers = $state<KnownKey[]>([]);
  let effects = $state<KnownKey[]>([]);
  let countries = $state<DropdownItem[]>([]);
  let pickerItems = $state<Record<string, DropdownItem[]>>({});
  let loading = $state(false);
  let error = $state<string | null>(null);

  let search = $state("");
  let modOnly = $state(false);
  let expandedKey = $state<string | null>(null);
  let newName = $state("");
  let newGroup = $state("");
  let newError = $state<string | null>(null);

  // When launched targeting a specific family (e.g. from the religion panel /
  // country-panel reform edit affordance).
  $effect(() => {
    if (family) {
      selected = family;
      family = null;
    }
  });

  // Auto-expand a requested object once its family list has loaded.
  $effect(() => {
    if (!open || !focusKey) return;
    const want = focusKey;
    if (fetched && fetched.objects.some((o) => o.key === want)) {
      expandedKey = want;
      focusKey = null;
    }
  });

  // Load the family catalog + shared registries once when opened.
  $effect(() => {
    if (!open) return;
    void loadStatic(installPath, modPath);
  });

  async function loadStatic(install: string, mod: string | null) {
    try {
      const [fams, kmods, trig, eff, ctys, blds, goods, subj, wgoals, treaties] = await Promise.all([
        invoke<FamilyMeta[]>("get_mechanic_families"),
        invoke<KnownModifier[]>("get_known_modifiers"),
        invoke<KnownKey[]>("get_known_triggers"),
        invoke<KnownKey[]>("get_known_effects"),
        invoke<CountryBrief[]>("list_countries", { installPath: install, modPath: mod }),
        invoke<RegistryBrief[]>("get_registry", { name: "buildings", installPath: install, modPath: mod }),
        invoke<RegistryBrief[]>("get_registry", { name: "trade_goods", installPath: install, modPath: mod }),
        // Sprint 27 W2 picker sources: subject types, war goals, peace treaties.
        invoke<RegistryBrief[]>("get_registry", { name: "subject_types", installPath: install, modPath: mod }),
        invoke<RegistryBrief[]>("get_registry", { name: "wargoal_types", installPath: install, modPath: mod }),
        invoke<RegistryBrief[]>("get_registry", { name: "peace_treaties", installPath: install, modPath: mod }),
      ]);
      families = fams;
      known = kmods;
      triggers = trig;
      effects = eff;
      countries = ctys.map((c) => ({
        key: c.tag,
        label: c.name,
        swatch: c.color ? `rgb(${c.color[0]}, ${c.color[1]}, ${c.color[2]})` : undefined,
      }));
      pickerItems = {
        building: blds.map((b) => ({ key: b.key, label: b.name })),
        trade_good: goods.map((g) => ({ key: g.key, label: g.name })),
        subject_type: subj.map((s) => ({ key: s.key, label: s.name })),
        wargoal_type: wgoals.map((w) => ({ key: w.key, label: w.name })),
        peace_treaty: treaties.map((t) => ({ key: t.key, label: t.name })),
      };
    } catch (e) {
      error = String(e);
    }
  }

  // Load objects whenever the selected family changes (while open).
  $effect(() => {
    if (!open) return;
    const fam = selected;
    void loadFamily(installPath, modPath, fam);
  });

  async function loadFamily(install: string, mod: string | null, fam: string) {
    loading = true;
    error = null;
    expandedKey = null;
    try {
      fetched = await invoke<MechanicsData>("get_mechanics", { installPath: install, modPath: mod, family: fam });
    } catch (e) {
      error = String(e);
      fetched = null;
    } finally {
      loading = false;
    }
  }

  const data = $derived<MechanicsData | null>(
    fetched ? ((queue.version, foldMechanics(fetched, queue.serialize()))) : null,
  );
  const meta = $derived<FamilyMeta | null>(data?.meta ?? null);
  const objects = $derived<MechanicObject[]>(data?.objects ?? []);
  const keys = $derived(data ? allKeys(data) : new Set<string>());
  const schoolGroups = $derived(
    meta?.groupNested ? Array.from(new Set(objects.map((o) => o.group).filter((g): g is string => !!g))).sort() : [],
  );

  function nameOf(o: MechanicObject): string {
    return queue.pendingLocOverride(o.nameKey) ?? o.name;
  }

  const shown = $derived(
    objects.filter((o) => {
      if (modOnly && o.origin !== "mod") return false;
      const q = search.trim().toLowerCase();
      if (!q) return true;
      return o.key.toLowerCase().includes(q) || nameOf(o).toLowerCase().includes(q);
    }),
  );

  function toggle(k: string) {
    expandedKey = expandedKey === k ? null : k;
  }

  function selectFamily(id: string) {
    if (id === selected) return;
    selected = id;
    search = "";
    newName = "";
    newError = null;
  }

  // --- Delete ---
  function removeObject(o: MechanicObject) {
    if (!confirm(`Delete "${o.key}"?`)) return;
    const blockPath = o.group ? [o.group, "religious_schools"] : [];
    queue.push({
      label: `Delete ${o.key}`,
      edits: [{ kind: "removeStatement", file: o.file, blockPath, key: o.key }],
    });
    if (expandedKey === o.key) expandedKey = null;
  }

  // --- ＋ New … ---
  function wrapperExists(projectFile: string): boolean {
    return (
      (fetched ? fetched.objects.some((o) => o.file === projectFile) : false) ||
      queue.findLast((e) => (e.kind === "createFile" || e.kind === "appendText") && e.file === projectFile) != null
    );
  }

  async function createObject() {
    newError = null;
    if (!meta) return;
    const key = slugify(newName.trim());
    if (!isValidKey(key)) {
      newError = "Use lowercase letters, digits and underscores (start with a letter).";
      return;
    }
    if (keys.has(key)) {
      newError = `"${key}" already exists.`;
      return;
    }
    const group = meta.groupNested ? newGroup || schoolGroups[0] : undefined;
    if (meta.groupNested && !group) {
      newError = "Pick a religion group for the new school.";
      return;
    }
    let scaffold: Scaffold;
    try {
      scaffold = await invoke<Scaffold>("scaffold_mechanic", { family: meta.id, key, group });
    } catch (e) {
      newError = String(e);
      return;
    }
    const edits: TypedEdit[] = [];
    if (meta.groupNested && group) {
      // Insert into the religion group's existing religious_schools block.
      edits.push({ kind: "insertStatement", file: meta.projectFile, blockPath: [group, "religious_schools"], statement: scaffold.text });
    } else {
      edits.push(
        wrapperExists(meta.projectFile)
          ? { kind: "appendText", file: meta.projectFile, text: "\n" + scaffold.text + "\n" }
          : { kind: "createFile", file: meta.projectFile, text: scaffold.text + "\n" },
      );
    }
    // Named-sprite emission (buildings/institutions): append a spriteType so the
    // created entity's icon resolves in game (icon-strip gotcha — resolved by
    // NAME here, not positional index).
    if (scaffold.gfxFile && scaffold.gfxText) {
      const gfxFile = scaffold.gfxFile;
      edits.push(
        wrapperExists(gfxFile)
          ? { kind: "appendText", file: gfxFile, text: "\n" + scaffold.gfxText + "\n" }
          : { kind: "createFile", file: gfxFile, text: scaffold.gfxText + "\n" },
      );
    }
    for (const le of scaffold.locEntries) {
      edits.push({ kind: "locOverride", key: le.key, value: le.value });
    }
    queue.push({ label: `Create ${key}`, edits });
    newName = "";
    expandedKey = key;
  }
</script>

<OverlaySurface bind:open title="Country Mechanics">
  {#snippet toolbar()}
    <select class="famsel" value={selected} onchange={(e) => selectFamily((e.target as HTMLSelectElement).value)}>
      {#each families as f (f.id)}<option value={f.id}>{f.label}</option>{/each}
    </select>
    <input class="search" type="text" placeholder="Search…" bind:value={search} />
    <label class="modonly"><input type="checkbox" bind:checked={modOnly} /> Mod only</label>
    <span class="counter">{shown.length}</span>
  {/snippet}

  <div class="body">
    {#if meta?.allowCreate ?? true}
      <div class="newrow">
        <input class="newkey" type="text" placeholder="New name…" bind:value={newName} onkeydown={(e) => e.key === "Enter" && createObject()} />
        {#if meta?.groupNested}
          <select class="grpsel" bind:value={newGroup}>
            <option value="">(religion group)</option>
            {#each schoolGroups as g (g)}<option value={g}>{g}</option>{/each}
          </select>
        {/if}
        <button class="newbtn" onclick={createObject}>＋ New {meta?.label ?? ""}</button>
        {#if newError}<span class="newerr">{newError}</span>{/if}
      </div>
    {:else}
      <div class="nocreate">This family is hardcoded by the game — existing entries are editable, but new ones can't be added.</div>
    {/if}

    {#if loading}
      <p class="msg">Loading…</p>
    {:else if error}
      <p class="msg err">{error}</p>
    {:else if shown.length === 0}
      <p class="msg">Nothing matches.</p>
    {/if}

    <ul class="list">
      {#each shown as o (o.file + "::" + (o.group ?? "") + "::" + o.key)}
        <li class="row" class:expanded={expandedKey === o.key}>
          <button class="rowmain" onclick={() => toggle(o.key)}>
            <span class="caret">{expandedKey === o.key ? "▾" : "▸"}</span>
            {#if o.color}<span class="cswatch" style={`background: rgb(${o.color[0]}, ${o.color[1]}, ${o.color[2]})`}></span>{/if}
            <span class="title">{nameOf(o)}</span>
            <code class="key">{o.key}</code>
            {#if o.group}<span class="grp">{o.group}</span>{/if}
            <span class="badge origin {o.origin}">{o.origin}</span>
            <span class="file">{o.file.split("/").pop()}</span>
          </button>
          {#if expandedKey === o.key && meta}
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
                {onopennaming}
              />
            </div>
          {/if}
        </li>
      {/each}
    </ul>
  </div>
</OverlaySurface>

<style>
  .famsel { background: #21262e; border: 1px solid #1f242c; color: #cfd4db; font-family: inherit; font-size: 0.83rem; padding: 0.2rem 0.4rem; }
  .search { background: #21262e; border: 1px solid #1f242c; color: #cfd4db; font-family: inherit; font-size: 0.83rem; padding: 0.2rem 0.4rem; width: 14rem; }
  .modonly { display: flex; align-items: center; gap: 0.3rem; font-size: 0.8rem; color: #cfd4db; }
  .counter { font-size: 0.8rem; color: #8a919c; }
  .body { display: flex; flex-direction: column; gap: 0.5rem; }
  .newrow { display: flex; align-items: center; gap: 0.5rem; flex-wrap: wrap; }
  .nocreate { color: #9aa3af; font-size: 0.82rem; font-style: italic; padding: 0.15rem 0; }
  .newkey { background: #21262e; border: 1px solid #1f242c; color: #cfd4db; font-family: inherit; font-size: 0.83rem; padding: 0.25rem 0.4rem; width: 16rem; }
  .grpsel { background: #21262e; border: 1px solid #1f242c; color: #cfd4db; font-family: inherit; font-size: 0.82rem; padding: 0.22rem 0.35rem; }
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
  .grp { color: #c9a978; background: #201d16; padding: 0 0.3rem; font-size: 0.72rem; }
  .badge { font-size: 0.68rem; text-transform: uppercase; letter-spacing: 0.03em; padding: 0.05rem 0.35rem; border: 1px solid #1f242c; }
  .badge.origin.base { background: #3f4855; color: #cfd4db; }
  .badge.origin.mod { background: #3f8a6d; color: #fff; }
  .file { margin-left: auto; color: #6d7683; font-size: 0.72rem; white-space: nowrap; }
  .rowbody { padding: 0 0.6rem 0.4rem; }
</style>
