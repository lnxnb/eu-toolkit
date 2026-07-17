<!--
  MercenariesSection (Sprint 23.2) — the province-panel Mercenaries editor.

  Lists mercenary companies with `home_province = <this province id>`, each
  expandable into a typed editor. "+ Add company" scaffolds a new company
  anchored to this province (zero-manual-fixes; base sprite pack). STATIC common
  files — the base snapshot is fetched with the pending queue applied so
  create/delete survive remounts; per-field edits fold live via queue helpers.
-->
<script lang="ts">
  import { untrack } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import type { DropdownItem, KnownModifier } from "$lib/components/ui";
  import type { KnownKey } from "$lib/components/script";
  import type { EditQueue, TypedEdit } from "$lib/edits.svelte";
  import {
    MERC_PROJECT_FILE,
    isValidKey,
    slugify,
    type MercenaryCompany,
    type MercenaryScaffold,
    type ProvinceMercenaries,
  } from "$lib/mercenaries";
  import MercenaryEditor from "./MercenaryEditor.svelte";

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

  let data = $state<ProvinceMercenaries | null>(null);
  let error = $state("");
  let reloadToken = $state(0);
  let expandedKey = $state<string | null>(null);

  let known = $state<KnownModifier[]>([]);
  let triggers = $state<KnownKey[]>([]);
  let knownLoaded = $state(false);
  $effect(() => {
    if (knownLoaded) return;
    knownLoaded = true;
    invoke<KnownModifier[]>("get_known_modifiers").then((v) => (known = v)).catch(() => {});
    invoke<KnownKey[]>("get_known_triggers").then((v) => (triggers = v)).catch(() => {});
  });

  $effect(() => {
    const cur = id;
    void reloadToken;
    data = null;
    error = "";
    // Read the queue untracked: refresh on id / create / delete only, not on
    // every field edit (those fold live inside the editors via queue helpers).
    const edits = untrack(() => queue.serialize());
    invoke<ProvinceMercenaries>("get_province_mercenaries", {
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

  const companies = $derived(data?.companies ?? []);
  const projectFile = $derived(data?.projectFile ?? MERC_PROJECT_FILE);

  function toggle(k: string) {
    expandedKey = expandedKey === k ? null : k;
  }

  function removeCompany(c: MercenaryCompany) {
    if (!confirm(`Delete mercenary company "${c.key}"?`)) return;
    queue.push({
      label: `Delete company ${c.key}`,
      edits: [{ kind: "removeStatement", file: c.file, blockPath: [], key: c.key }],
    });
    if (expandedKey === c.key) expandedKey = null;
    reloadToken++;
  }

  // --- Add ---
  let adding = $state(false);
  let newName = $state("");
  let newError = $state<string | null>(null);

  function wrapperExists(): boolean {
    return (
      companies.some((c) => c.file === projectFile) ||
      queue.findLast(
        (e) => (e.kind === "createFile" || e.kind === "appendText") && e.file === projectFile,
      ) != null
    );
  }

  async function createCompany() {
    newError = null;
    const key = slugify(newName.trim());
    if (!isValidKey(key)) {
      newError = "Use lowercase letters, digits and underscores (start with a letter).";
      return;
    }
    if (companies.some((c) => c.key === key)) {
      newError = `A company named "${key}" already exists here.`;
      return;
    }
    let scaffold: MercenaryScaffold;
    try {
      scaffold = await invoke<MercenaryScaffold>("scaffold_mercenary_company", { key, provinceId: id });
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
    for (const le of scaffold.locEntries) {
      edits.push({ kind: "locOverride", key: le.key, value: le.value });
    }
    queue.push({ label: `Create company ${key}`, edits });
    newName = "";
    adding = false;
    reloadToken++;
    expandedKey = key;
  }
</script>

<section>
  <h3>Mercenaries</h3>

  {#if error}
    <p class="err">{error}</p>
  {:else if !data}
    <p class="dim">Loading…</p>
  {:else}
    {#if companies.length === 0}
      <p class="dim">No mercenary company homed in this province.</p>
    {:else}
      <div class="list">
        {#each companies as c (c.key)}
          <div class="row">
            <button class="row-head" onclick={() => toggle(c.key)}>
              <span class="caret">{expandedKey === c.key ? "▾" : "▸"}</span>
              <span class="rname">{c.nameLoc ?? c.key}</span>
              {#if c.origin === "mod"}<span class="badge">mod</span>{/if}
            </button>
            {#if expandedKey === c.key}
              <MercenaryEditor
                {installPath}
                {modPath}
                {queue}
                company={c}
                {known}
                {triggers}
                {countries}
                onremove={() => removeCompany(c)}
              />
            {/if}
          </div>
        {/each}
      </div>
    {/if}

    {#if adding}
      <div class="add-form">
        <div class="af-row">
          <span class="lbl">Name</span>
          <input class="txt" bind:value={newName} placeholder="My Company" onkeydown={(e) => e.key === "Enter" && createCompany()} />
        </div>
        {#if newError}<p class="err">{newError}</p>{/if}
        <div class="af-actions">
          <button class="btn primary" onclick={createCompany}>Create</button>
          <button class="btn" onclick={() => (adding = false)}>Cancel</button>
        </div>
      </div>
    {:else}
      <button class="add-btn" onclick={() => { adding = true; newError = null; }}>+ Add company</button>
    {/if}
  {/if}
</section>

<style>
  section { margin-bottom: 1rem; }
  h3 { margin: 0 0 0.5rem; font-size: 0.8rem; text-transform: uppercase; letter-spacing: 0.05em; color: #9ca3af; }
  .dim { color: #8a919c; font-size: 0.8rem; margin: 0 0 0.4rem; }
  .err { color: #fca5a5; font-size: 0.78rem; margin: 0.2rem 0; }
  .list { display: flex; flex-direction: column; gap: 0.25rem; margin-bottom: 0.4rem; }
  .row { border: 1px solid #232a33; }
  .row-head {
    display: flex; align-items: center; gap: 0.4rem; width: 100%; text-align: left;
    border: none; background: #21262e; color: #cfd4db; font-family: inherit;
    font-size: 0.82rem; padding: 0.25rem 0.4rem; cursor: pointer;
  }
  .caret { color: #8a919c; width: 0.8rem; flex: none; }
  .rname { flex: 1; }
  .badge { font-size: 0.6rem; text-transform: uppercase; background: #2f3b2f; color: #9ece9e; padding: 0.02rem 0.3rem; }
  .add-btn { border: 1px solid #1f242c; background: #3f4855; color: #cfd4db; font-family: inherit; font-size: 0.8rem; padding: 0.2rem 0.6rem; cursor: pointer; }
  .add-btn:hover { background: #4a6da7; color: #fff; }
  .add-form { border: 1px solid #232a33; padding: 0.4rem; display: flex; flex-direction: column; gap: 0.35rem; }
  .af-row { display: flex; align-items: center; gap: 0.5rem; }
  .lbl { width: 5rem; flex: none; font-size: 0.76rem; color: #9ca3af; }
  .txt { flex: 1; min-width: 0; background: #14181d; border: 1px solid #4b5563; color: #cfd4db; font-family: inherit; font-size: 0.8rem; padding: 0.15rem 0.35rem; }
  .af-actions { display: flex; gap: 0.4rem; }
  .btn { border: 1px solid #4b5563; background: #2b323d; color: #cfd4db; font-family: inherit; font-size: 0.78rem; padding: 0.2rem 0.6rem; cursor: pointer; }
  .btn.primary { background: #4a6da7; border-color: #4a6da7; color: #fff; }
  .btn:hover { border-color: #4a6da7; }
</style>
