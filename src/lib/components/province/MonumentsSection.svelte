<!--
  MonumentsSection (Sprint 23.1) — the province-panel Great Projects editor.

  Lists great projects with `start = <this province id>` (monuments + canals),
  each expandable into a full typed editor (MonumentEditor). "+ Add great
  project" scaffolds a new monument anchored to this province, copying the tier
  structure and the gfx sprite binding from a user-picked existing monument, so
  it renders in game immediately (zero-manual-fixes).

  STATIC common files — no date threading. The base snapshot is fetched with the
  pending edit queue applied (so create/delete survive remounts); per-field edits
  fold live through the queue helpers inside the editors.
-->
<script lang="ts">
  import { untrack } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { LoadingState, SearchDropdown } from "$lib/components/ui";
  import type { DropdownItem, KnownModifier } from "$lib/components/ui";
  import type { KnownKey } from "$lib/components/script";
  import type { EditQueue, TypedEdit } from "$lib/edits.svelte";
  import {
    GP_PROJECT_FILE,
    isValidKey,
    slugify,
    type GreatProject,
    type GreatProjectScaffold,
    type MonumentBrief,
    type ProvinceMonuments,
  } from "$lib/monuments";
  import MonumentEditor from "./MonumentEditor.svelte";

  let {
    installPath,
    modPath,
    id,
    queue,
    countries = [],
  }: {
    installPath: string;
    modPath: string | null;
    id: number;
    queue: EditQueue;
    countries?: DropdownItem[];
  } = $props();

  let data = $state<ProvinceMonuments | null>(null);
  let error = $state("");
  let reloadToken = $state(0);
  let expandedKey = $state<string | null>(null);

  // Known registries (14.1/14.2/14.4) loaded once, lazily on first render.
  let known = $state<KnownModifier[]>([]);
  let triggers = $state<KnownKey[]>([]);
  let effects = $state<KnownKey[]>([]);
  let knownLoaded = $state(false);
  $effect(() => {
    if (knownLoaded) return;
    knownLoaded = true;
    invoke<KnownModifier[]>("get_known_modifiers").then((v) => (known = v)).catch(() => {});
    invoke<KnownKey[]>("get_known_triggers").then((v) => (triggers = v)).catch(() => {});
    invoke<KnownKey[]>("get_known_effects").then((v) => (effects = v)).catch(() => {});
  });

  // Base snapshot (with pending edits applied for create/delete folding).
  $effect(() => {
    const cur = id;
    void reloadToken;
    data = null;
    error = "";
    // Read the queue untracked: refresh on id / create / delete only, not on
    // every field edit (those fold live inside the editors via queue helpers).
    const edits = untrack(() => queue.serialize());
    invoke<ProvinceMonuments>("get_province_monuments", {
      installPath,
      modPath,
      id: cur,
      edits,
    })
      .then((d) => {
        if (cur === id) data = d;
      })
      .catch((e) => {
        if (cur === id) error = String(e);
      });
  });

  const monuments = $derived(data?.monuments ?? []);
  const projectFile = $derived(data?.projectFile ?? GP_PROJECT_FILE);

  function toggle(k: string) {
    expandedKey = expandedKey === k ? null : k;
  }

  function removeProject(p: GreatProject) {
    if (!confirm(`Delete great project "${p.key}"?`)) return;
    queue.push({
      label: `Delete great project ${p.key}`,
      edits: [{ kind: "removeStatement", file: p.file, blockPath: [], key: p.key }],
    });
    if (expandedKey === p.key) expandedKey = null;
    reloadToken++;
  }

  // --- Add ---
  let adding = $state(false);
  let sourceOptions = $state<DropdownItem[]>([]);
  let sourceKey = $state<string | null>(null);
  let newName = $state("");
  let newError = $state<string | null>(null);

  function openAdd() {
    adding = true;
    newError = null;
    if (sourceOptions.length === 0) {
      invoke<MonumentBrief[]>("list_monuments", { installPath, modPath })
        .then((v) => {
          sourceOptions = v.map((m) => ({ key: m.key, label: `${m.name} (#${m.start})` }));
        })
        .catch((e) => (newError = String(e)));
    }
  }

  function wrapperExists(): boolean {
    return (
      monuments.some((m) => m.file === projectFile) ||
      queue.findLast(
        (e) => (e.kind === "createFile" || e.kind === "appendText") && e.file === projectFile,
      ) != null
    );
  }

  async function createProject() {
    newError = null;
    const key = slugify(newName.trim());
    if (!isValidKey(key)) {
      newError = "Use lowercase letters, digits and underscores (start with a letter).";
      return;
    }
    if (monuments.some((m) => m.key === key)) {
      newError = `A great project named "${key}" already exists here.`;
      return;
    }
    if (!sourceKey) {
      newError = "Pick a monument to copy the tier structure and gfx from.";
      return;
    }
    let scaffold: GreatProjectScaffold;
    try {
      scaffold = await invoke<GreatProjectScaffold>("scaffold_great_project_cmd", {
        installPath,
        modPath,
        sourceKey,
        newKey: key,
        provinceId: id,
      });
    } catch (e) {
      newError = String(e);
      return;
    }
    const edits: TypedEdit[] = [];
    edits.push(
      wrapperExists()
        ? { kind: "appendText", file: projectFile, text: "\n" + scaffold.text + "\n" }
        : { kind: "createFile", file: projectFile, text: scaffold.text + "\n" },
    );
    // The copied gfx sprite binding (own file per monument → always createFile).
    edits.push({ kind: "createFile", file: scaffold.gfxFile, text: scaffold.gfxText });
    for (const le of scaffold.locEntries) {
      edits.push({ kind: "locOverride", key: le.key, value: le.value });
    }
    queue.push({ label: `Create great project ${key}`, edits });
    newName = "";
    adding = false;
    reloadToken++;
    expandedKey = key;
  }
</script>

<section>
  <h3>Great Projects</h3>

  {#if error}
    <p class="err">{error}</p>
  {:else if !data}
    <LoadingState label="Loading great projects…" />
  {:else}
    {#if monuments.length === 0}
      <p class="dim">No great project anchored to this province.</p>
    {:else}
      <div class="list">
        {#each monuments as p (p.key)}
          <div class="row">
            <button class="row-head" onclick={() => toggle(p.key)}>
              <span class="caret">{expandedKey === p.key ? "▾" : "▸"}</span>
              <span class="rname">{p.nameLoc ?? p.key}</span>
              <span class="type">{p.projectType}</span>
              {#if p.origin === "mod"}<span class="badge">mod</span>{/if}
            </button>
            {#if expandedKey === p.key}
              <MonumentEditor
                {installPath}
                {modPath}
                {queue}
                project={p}
                {known}
                {triggers}
                {effects}
                {countries}
                onremove={() => removeProject(p)}
              />
            {/if}
          </div>
        {/each}
      </div>
    {/if}

    {#if adding}
      <div class="add-form">
        <div class="af-row">
          <span class="lbl">Copy from</span>
          <SearchDropdown items={sourceOptions} value={sourceKey} placeholder="Existing monument…" onselect={(k) => (sourceKey = k)} />
        </div>
        <div class="af-row">
          <span class="lbl">Name</span>
          <input class="txt" bind:value={newName} placeholder="My Monument" onkeydown={(e) => e.key === "Enter" && createProject()} />
        </div>
        {#if newError}<p class="err">{newError}</p>{/if}
        <div class="af-actions">
          <button class="btn primary" onclick={createProject}>Create</button>
          <button class="btn" onclick={() => (adding = false)}>Cancel</button>
        </div>
      </div>
    {:else}
      <button class="add-btn" onclick={openAdd}>+ Add great project</button>
    {/if}
  {/if}
</section>

<style>
  section { margin-bottom: 1rem; }
  h3 { margin: 0 0 0.5rem; font-size: 0.8rem; text-transform: uppercase; letter-spacing: 0.05em; color: var(--text-2); }
  .dim { color: var(--text-2); font-size: 0.8rem; margin: 0 0 0.4rem; }
  .err { color: var(--err); font-size: 0.78rem; margin: 0.2rem 0; }
  .list { display: flex; flex-direction: column; gap: 0.25rem; margin-bottom: 0.4rem; }
  .row { border: 1px solid var(--bg-1); }
  .row-head {
    display: flex; align-items: center; gap: 0.4rem; width: 100%; text-align: left;
    border: none; background: var(--bg-1); color: var(--text-1); font-family: inherit;
    font-size: 0.82rem; padding: 0.25rem 0.4rem; cursor: pointer;
  }
  .caret { color: var(--text-2); width: 0.8rem; flex: none; }
  .rname { flex: 1; }
  .type { font-size: 0.66rem; text-transform: uppercase; color: var(--text-2); }
  .badge { font-size: 0.6rem; text-transform: uppercase; background: var(--bg-1); color: var(--ok); padding: 0.02rem 0.3rem; }
  .add-btn { border: 1px solid var(--border); background: var(--bg-3); color: var(--text-1); font-family: inherit; font-size: 0.8rem; padding: 0.2rem 0.6rem; cursor: pointer; }
  .add-btn:hover { background: var(--accent); color: var(--text-inverse); }
  .add-form { border: 1px solid var(--bg-1); padding: 0.4rem; display: flex; flex-direction: column; gap: 0.35rem; }
  .af-row { display: flex; align-items: center; gap: 0.5rem; }
  .lbl { width: 5rem; flex: none; font-size: 0.76rem; color: var(--text-2); }
  .txt { flex: 1; min-width: 0; background: var(--bg-0); border: 1px solid var(--border-strong); color: var(--text-1); font-family: inherit; font-size: 0.8rem; padding: 0.15rem 0.35rem; }
  .af-actions { display: flex; gap: 0.4rem; }
  .btn { border: 1px solid var(--border-strong); background: var(--bg-2); color: var(--text-1); font-family: inherit; font-size: 0.78rem; padding: 0.2rem 0.6rem; cursor: pointer; }
  .btn.primary { background: var(--accent); border-color: var(--accent); color: var(--text-inverse); }
  .btn:hover { border-color: var(--accent); }
</style>
