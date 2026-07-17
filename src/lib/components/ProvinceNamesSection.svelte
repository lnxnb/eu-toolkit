<script lang="ts">
  // Province names section (Sprint 24): a searchable, editable view of one
  // common/province_names/<key>.txt rename list — hosted from the culture panel
  // (culture and culture-group keys) and the country panel (TAG key). Entries are
  // literal Windows-1252 strings; edits are byte-surgical on existing files and a
  // whole-file CreateFile scaffold for a key with no file yet.
  import { invoke } from "@tauri-apps/api/core";
  import type { EditQueue, TypedEdit } from "$lib/edits.svelte";
  import {
    entryStatement,
    buildFileText,
    editEntryEdits,
    normCapital,
    type ProvinceNameEntry,
    type ProvinceNamesFile,
  } from "$lib/provinceNames";

  let {
    installPath,
    modPath,
    fileKey,
    kindLabel,
    queue,
    pickRequest = null,
    onarmpick,
    onpickconsumed,
  }: {
    installPath: string;
    modPath: string | null;
    /** The file key: a culture, culture group, or TAG. */
    fileKey: string;
    /** Human label for the key kind, e.g. "culture" / "culture group" / "country". */
    kindLabel: string;
    queue: EditQueue;
    /** A province id picked on the map for this section, or null. */
    pickRequest?: number | null;
    /** Arm the map "pick a province" tool for this section. */
    onarmpick?: () => void;
    /** Tell the host the pick was consumed (disarm/reset). */
    onpickconsumed?: () => void;
  } = $props();

  let data = $state<ProvinceNamesFile | null>(null);
  let error = $state("");
  let file = $derived(data?.source_file ?? `common/province_names/${fileKey}.txt`);
  // Surgical (existing file) vs whole-file rebuild (a brand-new file we own).
  let surgical = $derived(data?.exists ?? false);

  // Local desired state + per-id buffer bookkeeping (the section is remounted per
  // key by the host, so this seeds once). `buf` tracks each id's current
  // on-buffer presence + shape so an edit picks SetScalar/SetBlock vs Remove+Insert.
  let entries = $state<ProvinceNameEntry[]>([]);
  let buf = new Map<number, { onBuffer: boolean; hasCap: boolean }>();
  let seeded = false;

  $effect(() => {
    const key = fileKey;
    data = null;
    error = "";
    seeded = false;
    invoke<ProvinceNamesFile>("get_province_names", { installPath, modPath, key })
      .then((d) => {
        if (key !== fileKey) return;
        data = d;
        if (!seeded) {
          seeded = true;
          entries = d.entries.map((e) => ({ ...e }));
          buf = new Map(d.entries.map((e) => [e.id, { onBuffer: true, hasCap: e.capital != null }]));
        }
      })
      .catch((e) => {
        if (key === fileKey) error = String(e);
      });
  });

  // --- Search ---
  let query = $state("");
  let filtered = $derived.by(() => {
    const q = query.trim().toLowerCase();
    const list = [...entries].sort((a, b) => a.id - b.id);
    if (!q) return list;
    return list.filter(
      (e) =>
        String(e.id).includes(q) ||
        e.name.toLowerCase().includes(q) ||
        (e.capital?.toLowerCase().includes(q) ?? false),
    );
  });

  // --- Whole-file rebuild (regime A: new file) ---
  function rebuild() {
    const edits: TypedEdit[] = [{ kind: "createFile", file, text: buildFileText(entries) }];
    queue.push({
      label: `Edit province names for ${fileKey}`,
      edits,
      coalesceKey: `pnames-file:${fileKey}`,
    });
  }

  // --- Commit one entry's value (add or edit) ---
  function commit(id: number, name: string, capitalRaw: string | null) {
    const nm = name.trim();
    if (!nm) return;
    const capital = normCapital(capitalRaw);
    const idx = entries.findIndex((e) => e.id === id);
    const next: ProvinceNameEntry = { id, name: nm, capital };
    if (idx >= 0) entries[idx] = next;
    else entries = [...entries, next];

    if (!surgical) {
      rebuild();
      return;
    }
    const st = buf.get(id);
    if (!st || !st.onBuffer) {
      // ADD: one Insert, emitted exactly once per id (its own coalesce key so a
      // later value edit never clobbers it).
      queue.push({
        label: `Name province #${id} in ${fileKey}`,
        edits: [{ kind: "insertStatement", file, blockPath: [], statement: entryStatement(next) }],
        coalesceKey: `pnames-add:${fileKey}:${id}`,
      });
      buf.set(id, { onBuffer: true, hasCap: capital != null });
    } else {
      // EDIT: idempotent SetScalar/SetBlock, or Remove+Insert on a shape change.
      queue.push({
        label: `Rename province #${id} in ${fileKey}`,
        edits: editEntryEdits(file, next, st.hasCap),
        coalesceKey: `pnames-edit:${fileKey}:${id}`,
      });
      buf.set(id, { onBuffer: true, hasCap: capital != null });
    }
  }

  function removeEntry(id: number) {
    entries = entries.filter((e) => e.id !== id);
    if (!surgical) {
      rebuild();
    } else if (buf.get(id)?.onBuffer) {
      queue.push({
        label: `Remove province #${id} name in ${fileKey}`,
        edits: [{ kind: "removeStatement", file, blockPath: [], key: String(id) }],
        coalesceKey: `pnames-rm:${fileKey}:${id}`,
      });
    }
    buf.set(id, { onBuffer: false, hasCap: false });
    if (editingId === id) editingId = null;
  }

  // --- Inline edit of an existing row ---
  let editingId = $state<number | null>(null);
  let editName = $state("");
  let editCap = $state("");
  function startEdit(e: ProvinceNameEntry) {
    editingId = e.id;
    editName = e.name;
    editCap = e.capital ?? "";
  }
  function applyEdit() {
    if (editingId == null) return;
    commit(editingId, editName, editCap || null);
    editingId = null;
  }

  // --- Add a new entry ---
  let adding = $state(false);
  let newId = $state("");
  let newName = $state("");
  let newCap = $state("");
  let addError = $state("");
  function openAdd(id?: number) {
    adding = true;
    addError = "";
    if (id != null) newId = String(id);
    newName = "";
    newCap = "";
  }
  function applyAdd() {
    const id = parseInt(newId.trim(), 10);
    if (!Number.isFinite(id) || id <= 0) {
      addError = "Enter a valid province id.";
      return;
    }
    if (entries.some((e) => e.id === id)) {
      addError = `Province #${id} already has a name here.`;
      return;
    }
    if (!newName.trim()) {
      addError = "Enter a name.";
      return;
    }
    commit(id, newName, newCap || null);
    adding = false;
    newId = "";
    newName = "";
    newCap = "";
  }

  // Map pick → prefill the add form with the clicked province id. Only the
  // section that armed the pick consumes it (two sections can be mounted in one
  // panel: a culture and its group).
  let armed = false;
  function armPick() {
    armed = true;
    onarmpick?.();
  }
  $effect(() => {
    const id = pickRequest;
    if (id == null || !armed) return;
    armed = false;
    openAdd(id);
    onpickconsumed?.();
  });
</script>

<section>
  <h3>
    Province names
    <span class="cnt">{entries.length}</span>
  </h3>
  <p class="dim small">
    Renames provinces for this {kindLabel} (<span class="mono">{file.replace("common/province_names/", "")}</span>).
    Literal names, not localisation keys.
  </p>

  {#if error}
    <p class="err">{error}</p>
  {:else if !data}
    <p class="dim small">Loading…</p>
  {:else}
    <div class="toolbar">
      <input class="search" placeholder="Search id or name…" bind:value={query} />
      <button class="btn" onclick={() => openAdd()}>＋ Add</button>
      <button class="btn" onclick={armPick} title="Click a province on the map">Pick on map</button>
    </div>

    {#if adding}
      <div class="add">
        <input class="num" placeholder="id" bind:value={newId} />
        <input class="nm" placeholder="Name" bind:value={newName} />
        <input class="nm" placeholder="Capital (optional)" bind:value={newCap} />
        <button class="mini ok" onclick={applyAdd}>✓</button>
        <button class="mini" onclick={() => (adding = false)}>×</button>
      </div>
      {#if addError}<p class="err small">{addError}</p>{/if}
    {/if}

    {#if filtered.length === 0}
      <p class="dim small">{query ? "No matches." : "No province names yet."}</p>
    {:else}
      <ul class="rows">
        {#each filtered as e (e.id)}
          <li>
            {#if editingId === e.id}
              <span class="id">#{e.id}</span>
              <input class="nm" bind:value={editName} onkeydown={(ev) => ev.key === "Enter" && applyEdit()} />
              <input class="nm" placeholder="Capital" bind:value={editCap} onkeydown={(ev) => ev.key === "Enter" && applyEdit()} />
              <button class="mini ok" onclick={applyEdit}>✓</button>
              <button class="mini" onclick={() => (editingId = null)}>×</button>
            {:else}
              <span class="id">#{e.id}</span>
              <span class="nm-txt">{e.name}</span>
              {#if e.capital}<span class="cap-txt" title="Capital city name">⌂ {e.capital}</span>{/if}
              <button class="mini" title="Edit" onclick={() => startEdit(e)}>✎</button>
              <button class="mini" title="Remove" onclick={() => removeEntry(e.id)}>🗑</button>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  {/if}
</section>

<style>
  section {
    padding: 0.4rem 0 0.6rem;
    border-bottom: 1px solid #232a33;
  }
  h3 {
    margin: 0 0 0.3rem;
    font-size: 0.8rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: #9ca3af;
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }
  .cnt {
    font-size: 0.72rem;
    color: #6b7280;
    background: #1a1f27;
    border: 1px solid #2b323d;
    padding: 0 0.35rem;
    border-radius: 2px;
  }
  .toolbar {
    display: flex;
    gap: 0.3rem;
    margin: 0.3rem 0;
  }
  .search {
    flex: 1;
  }
  input {
    background: #14181d;
    border: 1px solid #4b5563;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.8rem;
    padding: 0.2rem 0.4rem;
    min-width: 0;
  }
  .btn {
    border: 1px solid #4b5563;
    background: transparent;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.78rem;
    padding: 0.2rem 0.5rem;
    cursor: pointer;
    white-space: nowrap;
  }
  .btn:hover {
    border-color: #9ca3af;
  }
  .add {
    display: flex;
    gap: 0.3rem;
    align-items: center;
    margin: 0.3rem 0;
  }
  .add .num {
    width: 3.5rem;
  }
  .add .nm {
    flex: 1;
  }
  .rows {
    list-style: none;
    margin: 0;
    padding: 0;
    max-height: 16rem;
    overflow-y: auto;
  }
  .rows li {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    padding: 0.12rem 0;
    font-size: 0.82rem;
  }
  .rows li .nm {
    flex: 1;
  }
  .id {
    color: #6b7280;
    font-family: ui-monospace, monospace;
    font-size: 0.72rem;
    min-width: 2.6rem;
  }
  .nm-txt {
    flex: 1;
    color: #cfd4db;
  }
  .cap-txt {
    color: #9cc7ea;
    font-size: 0.74rem;
  }
  .mono {
    font-family: ui-monospace, monospace;
    color: #9ca3af;
  }
  .mini {
    border: 1px solid #3a424e;
    background: #2b323d;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.72rem;
    padding: 0.05rem 0.35rem;
    cursor: pointer;
    line-height: 1.4;
  }
  .mini:hover {
    border-color: #9ca3af;
  }
  .mini.ok {
    color: #86efac;
  }
  .dim {
    color: #9ca3af;
  }
  .small {
    font-size: 0.75rem;
  }
  .err {
    color: #fca5a5;
    font-size: 0.78rem;
  }
</style>
