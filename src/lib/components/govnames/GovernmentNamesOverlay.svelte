<!--
  GovernmentNamesOverlay — View ▸ Government names… (Sprint 19.3).

  A full-screen OverlaySurface listing ALL naming schemes across
  common/government_names (base + mod) with an origin badge, search (by key or
  resolved country name), and a "mod only" filter. File order = precedence (first
  valid match wins) and is REORDERABLE within a file via byte-surgical body swaps.
  Expanding a scheme opens the SchemeEditor (rank×role loc-cell table + 14.2
  trigger tree). "＋ New scheme" scaffolds a project zz_ file entry + loc keys for
  every cell (zero-manual-fixes: loads in game immediately).

  `focusKey` (set by the country-panel "jump to scheme" link) auto-expands and
  scrolls to that scheme when the overlay opens.
-->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { OverlaySurface } from "$lib/components/script";
  import type { KnownKey } from "$lib/components/script";
  import type { DropdownItem } from "$lib/components/ui";
  import type { EditQueue, TypedEdit } from "$lib/edits.svelte";
  import SchemeEditor from "./SchemeEditor.svelte";
  import {
    foldGovernmentNames,
    isValidSchemeKey,
    slugify,
    GOV_NAMES_PROJECT_FILE,
    type GovernmentNamesData,
    type GovNameScaffold,
    type GovNameScheme,
  } from "$lib/governmentNames";

  let {
    open = $bindable(false),
    installPath,
    modPath = null,
    queue,
    focusKey = null,
    onfocused,
  }: {
    open?: boolean;
    installPath: string;
    modPath?: string | null;
    queue: EditQueue;
    /** A scheme key to auto-expand + scroll to when opening (jump-to-scheme). */
    focusKey?: string | null;
    /** Called once the focus request has been consumed. */
    onfocused?: () => void;
  } = $props();

  interface CountryBrief {
    tag: string;
    name: string;
    color: [number, number, number] | null;
  }

  let fetched = $state<GovernmentNamesData | null>(null);
  let triggers = $state<KnownKey[]>([]);
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
      const [data, trig, ctys] = await Promise.all([
        invoke<GovernmentNamesData>("get_government_names", { installPath: install, modPath: mod }),
        invoke<KnownKey[]>("get_known_triggers"),
        invoke<CountryBrief[]>("list_countries", { installPath: install, modPath: mod }),
      ]);
      fetched = data;
      triggers = trig;
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

  // Effective schemes = fetched folded with the pending queue (create/delete/
  // reorder/trigger toggle appear live; undo/redo revert them).
  const data = $derived<GovernmentNamesData | null>(
    fetched ? ((queue.version, foldGovernmentNames(fetched, queue.serialize()))) : null,
  );
  const schemes = $derived<GovNameScheme[]>(data?.schemes ?? []);
  const allKeys = $derived(new Set(schemes.map((s) => s.key)));

  // A scheme's human title = its rank-2 (Kingdom) country name, else rank-1, else key.
  function titleOf(s: GovNameScheme): string {
    const c =
      s.cells.find((c) => c.role === "rank" && c.rank === 2) ??
      s.cells.find((c) => c.role === "rank" && c.rank === 1);
    if (!c) return s.key;
    return queue.pendingLocOverride(c.loc_key) ?? c.resolved;
  }

  const shown = $derived(
    schemes.filter((s) => {
      if (modOnly && s.origin !== "mod") return false;
      const q = search.trim().toLowerCase();
      if (!q) return true;
      return s.key.toLowerCase().includes(q) || titleOf(s).toLowerCase().includes(q);
    }),
  );

  // Index of each scheme in the full (unfiltered) list — for reorder bounds and
  // for reading adjacent schemes' raw bodies during a swap.
  function indexOf(key: string): number {
    return schemes.findIndex((s) => s.key === key);
  }

  function toggle(key: string) {
    expandedKey = expandedKey === key ? null : key;
  }

  // Consume the focus request once the list is loaded.
  $effect(() => {
    if (!open || !focusKey || !fetched) return;
    const target = focusKey;
    expandedKey = target;
    onfocused?.();
    // Scroll after the row renders.
    queueMicrotask(() => {
      const el = document.getElementById(`gov-scheme-${target}`);
      el?.scrollIntoView({ block: "center" });
    });
  });

  // --- Reorder (byte-surgical body swap between adjacent schemes IN THE SAME FILE) ---
  function innerOf(raw: string): string {
    const s = raw.indexOf("{");
    const e = raw.lastIndexOf("}");
    return s >= 0 && e > s ? raw.slice(s + 1, e) : raw;
  }
  function canMove(s: GovNameScheme, delta: number): boolean {
    const i = indexOf(s.key);
    const j = i + delta;
    return j >= 0 && j < schemes.length && schemes[j].file === s.file;
  }
  function move(s: GovNameScheme, delta: number) {
    const i = indexOf(s.key);
    const j = i + delta;
    if (j < 0 || j >= schemes.length) return;
    const other = schemes[j];
    if (other.file !== s.file) return; // only reorder within one file
    queue.push({
      label: `Reorder naming schemes in ${s.file.split("/").pop()}`,
      edits: [
        { kind: "setBlock", file: s.file, path: [s.key], value: innerOf(other.raw) },
        { kind: "setBlock", file: s.file, path: [other.key], value: innerOf(s.raw) },
      ],
    });
  }

  // --- Delete ---
  function removeScheme(s: GovNameScheme) {
    if (!confirm(`Delete naming scheme "${s.key}"? Countries it matched fall through to the next scheme.`)) return;
    queue.push({
      label: `Delete naming scheme ${s.key}`,
      edits: [{ kind: "removeStatement", file: s.file, blockPath: [], key: s.key }],
    });
    if (expandedKey === s.key) expandedKey = null;
  }

  // --- ＋ New scheme ---
  const wrapperExists = $derived(
    (fetched?.schemes.some((s) => s.file === GOV_NAMES_PROJECT_FILE) ?? false) ||
      queue.findLast(
        (e) =>
          (e.kind === "createFile" || e.kind === "appendText") && e.file === GOV_NAMES_PROJECT_FILE,
      ) != null,
  );

  async function createScheme() {
    newError = null;
    const key = slugify(newName.trim());
    if (!isValidSchemeKey(key)) {
      newError = "Use lowercase letters, digits and underscores (start with a letter).";
      return;
    }
    if (allKeys.has(key)) {
      newError = `A scheme named "${key}" already exists.`;
      return;
    }
    let scaffold: GovNameScaffold;
    try {
      scaffold = await invoke<GovNameScaffold>("scaffold_government_name", { key });
    } catch (e) {
      newError = String(e);
      return;
    }
    const edits: TypedEdit[] = [];
    edits.push(
      wrapperExists
        ? { kind: "appendText", file: GOV_NAMES_PROJECT_FILE, text: scaffold.text }
        : { kind: "createFile", file: GOV_NAMES_PROJECT_FILE, text: scaffold.text + "\n" },
    );
    for (const c of scaffold.cells) {
      edits.push({ kind: "locOverride", key: c.loc_key, value: c.resolved });
    }
    queue.push({ label: `Create naming scheme ${key}`, edits });
    newName = "";
    expandedKey = key;
  }
</script>

<OverlaySurface bind:open title="Government names">
  {#snippet toolbar()}
    <input class="search" type="text" placeholder="Search schemes…" bind:value={search} />
    <label class="modonly">
      <input type="checkbox" bind:checked={modOnly} />
      Mod only
    </label>
    <span class="counter">{shown.length}</span>
  {/snippet}

  <div class="body">
    <p class="lede">
      Each scheme sets a country's localized name (by rank) and ruler / consort titles for the countries its
      condition matches. <strong>The first matching scheme in file order wins</strong> — reorder to change precedence.
    </p>

    <div class="newrow">
      <input
        class="newkey"
        type="text"
        placeholder="New scheme name…"
        bind:value={newName}
        onkeydown={(e) => e.key === "Enter" && createScheme()}
      />
      <button class="newbtn" onclick={createScheme}>＋ New scheme</button>
      {#if newError}<span class="newerr">{newError}</span>{/if}
    </div>

    {#if loading}
      <p class="msg">Loading government names…</p>
    {:else if error}
      <p class="msg err">{error}</p>
    {:else if shown.length === 0}
      <p class="msg">No schemes match.</p>
    {/if}

    <ul class="list">
      {#each shown as s (s.file + "::" + s.key)}
        <li class="row" class:expanded={expandedKey === s.key} id={`gov-scheme-${s.key}`}>
          <div class="rowhead">
            <button class="rowmain" onclick={() => toggle(s.key)}>
              <span class="caret">{expandedKey === s.key ? "▾" : "▸"}</span>
              <span class="pos">#{indexOf(s.key) + 1}</span>
              <span class="title">{titleOf(s)}</span>
              <code class="key">{s.key}</code>
              {#if !s.has_trigger}<span class="badge always">always</span>{/if}
              <span class="badge origin {s.origin}">{s.origin}</span>
              <span class="file">{s.file.split("/").pop()}</span>
            </button>
            <div class="ord">
              <button class="ico" disabled={!canMove(s, -1)} title="Move up (matched earlier)" aria-label="Move up" onclick={() => move(s, -1)}>▲</button>
              <button class="ico" disabled={!canMove(s, 1)} title="Move down" aria-label="Move down" onclick={() => move(s, 1)}>▼</button>
            </div>
          </div>
          {#if expandedKey === s.key}
            <div class="rowbody">
              <SchemeEditor
                {installPath}
                {modPath}
                {queue}
                scheme={s}
                {triggers}
                {countries}
                onremove={() => removeScheme(s)}
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
  .lede {
    margin: 0;
    font-size: 0.8rem;
    color: #9ca3af;
  }
  .newrow {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    flex-wrap: wrap;
  }
  .newkey {
    background: #21262e;
    border: 1px solid #1f242c;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.83rem;
    padding: 0.25rem 0.4rem;
    width: 16rem;
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
  .rowhead {
    display: flex;
    align-items: center;
  }
  .rowmain {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    flex: 1;
    min-width: 0;
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
  .pos {
    color: #8a919c;
    font-size: 0.72rem;
    width: 2rem;
    flex: none;
  }
  .title {
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 16rem;
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
  .badge.always {
    background: #6d5aa1;
    color: #fff;
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
  .ord {
    display: flex;
    gap: 0.15rem;
    padding: 0 0.4rem;
    flex: none;
  }
  .ico {
    border: 1px solid #4b5563;
    background: #2b323d;
    color: #cfd4db;
    font-size: 0.72rem;
    line-height: 1;
    padding: 0.15rem 0.3rem;
    cursor: pointer;
  }
  .ico:disabled {
    opacity: 0.35;
    cursor: default;
  }
  .ico:not(:disabled):hover {
    border-color: #4a6da7;
    background: #4a6da7;
    color: #fff;
  }
  .rowbody {
    padding: 0 0.6rem 0.4rem;
  }
</style>
