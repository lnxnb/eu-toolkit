<!--
  DecisionsOverlay — View ▸ Decisions… (Sprint 15).

  A full-screen OverlaySurface listing ALL country_decisions across the VFS
  (base + mod) with an origin badge, search (by key or localized title), and a
  "mod only" filter. Expanding a row opens the DecisionEditor (loc text, major
  toggle, potential/allow/effect 14.2 trees, ai_will_do raw, and the 14.3
  availability list). "＋ New decision" scaffolds an empty decision into the
  project's decisions/zz_eutoolkit_decisions.txt (+ title/desc loc).

  Purity: this component owns NO map/session state beyond what it's handed; it
  pushes composites to the shared `queue` and asks the parent to jump to a country
  (which also closes the overlay).
-->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { OverlaySurface } from "$lib/components/script";
  import type { KnownKey } from "$lib/components/script";
  import type { DropdownItem } from "$lib/components/ui";
  import type { EditQueue, TypedEdit } from "$lib/edits.svelte";
  import DecisionEditor from "./DecisionEditor.svelte";
  import { SCAFFOLD_FILE, type DecisionEntry } from "./decisionsTypes";

  let {
    open = $bindable(false),
    installPath,
    modPath = null,
    selectedDate = null,
    queue,
    onjumpcountry,
  }: {
    open?: boolean;
    installPath: string;
    modPath?: string | null;
    selectedDate?: string | null;
    queue: EditQueue;
    onjumpcountry: (tag: string) => void;
  } = $props();

  interface CountryBrief {
    tag: string;
    name: string;
    color: [number, number, number] | null;
  }

  let fetched = $state<DecisionEntry[]>([]);
  let pendingCreated = $state<DecisionEntry[]>([]);
  let triggers = $state<KnownKey[]>([]);
  let effects = $state<KnownKey[]>([]);
  let countries = $state<DropdownItem[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);

  let search = $state("");
  let modOnly = $state(false);
  let expandedKey = $state<string | null>(null);
  let newKey = $state("");
  let newKeyError = $state<string | null>(null);

  // Load everything when the overlay opens (or the session changes while open).
  $effect(() => {
    if (!open) return;
    void load(installPath, modPath);
  });

  async function load(install: string, mod: string | null) {
    loading = true;
    error = null;
    try {
      const [decs, trig, eff, ctys] = await Promise.all([
        invoke<DecisionEntry[]>("get_decisions", { installPath: install, modPath: mod }),
        invoke<KnownKey[]>("get_known_triggers"),
        invoke<KnownKey[]>("get_known_effects"),
        invoke<CountryBrief[]>("list_countries", { installPath: install, modPath: mod }),
      ]);
      fetched = decs;
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

  // Merged, searched, filtered list. Pending scaffolds appear first.
  const all = $derived<DecisionEntry[]>([...pendingCreated, ...fetched]);
  const allKeys = $derived(new Set(all.map((d) => d.key)));
  const shown = $derived(
    all
      .filter((d) => !modOnly || d.origin === "mod")
      .filter((d) => {
        const q = search.trim().toLowerCase();
        if (!q) return true;
        return d.key.toLowerCase().includes(q) || d.title.toLowerCase().includes(q);
      }),
  );

  function toggle(key: string) {
    expandedKey = expandedKey === key ? null : key;
  }

  function jump(tag: string) {
    open = false;
    onjumpcountry(tag);
  }

  // --- + New decision ------------------------------------------------------
  const KEY_RE = /^[a-z][a-z0-9_]*$/;

  const wrapperExists = $derived(
    fetched.some((d) => d.file === SCAFFOLD_FILE) ||
      queue.findLast((e) => e.kind === "createFile" && e.file === SCAFFOLD_FILE) != null,
  );

  function prettify(key: string): string {
    return key
      .split("_")
      .map((w) => (w ? w[0].toUpperCase() + w.slice(1) : w))
      .join(" ");
  }

  function createDecision() {
    const key = newKey.trim();
    newKeyError = null;
    if (!KEY_RE.test(key)) {
      newKeyError = "Use lowercase letters, digits and underscores (start with a letter).";
      return;
    }
    if (allKeys.has(key)) {
      newKeyError = `A decision named "${key}" already exists.`;
      return;
    }

    // The empty decision block (authored at column 0; the writer re-indents on
    // insert, and for a fresh file we wrap it ourselves).
    const decBody = `${key} = {\n\tpotential = {\n\t}\n\tallow = {\n\t}\n\teffect = {\n\t}\n\tai_will_do = {\n\t\tfactor = 1\n\t}\n}`;
    const edits: TypedEdit[] = [];
    if (wrapperExists) {
      edits.push({
        kind: "insertStatement",
        file: SCAFFOLD_FILE,
        blockPath: ["country_decisions"],
        statement: decBody,
      });
    } else {
      // First scaffold into a brand-new file: wrap in country_decisions.
      const wrapped = `country_decisions = {\n${decBody
        .split("\n")
        .map((l) => (l ? `\t${l}` : l))
        .join("\n")}\n}\n`;
      edits.push({ kind: "createFile", file: SCAFFOLD_FILE, text: wrapped });
    }
    edits.push({ kind: "locOverride", key: `${key}_title`, value: prettify(key) });
    edits.push({ kind: "locOverride", key: `${key}_desc`, value: "" });
    queue.push({ label: `Create decision ${key}`, edits });

    const path = ["country_decisions", key];
    const entry: DecisionEntry = {
      key,
      file: SCAFFOLD_FILE,
      origin: "mod",
      major: false,
      title: prettify(key),
      titleKey: `${key}_title`,
      descKey: `${key}_desc`,
      titleLoc: prettify(key),
      descLoc: "",
      aiWillDo: "{\n\t\tfactor = 1\n\t}",
      path,
      potentialPath: [...path, "potential"],
      allowPath: [...path, "allow"],
      effectPath: [...path, "effect"],
      hasPotential: true,
      hasAllow: true,
      hasEffect: true,
      pending: true,
    };
    pendingCreated = [entry, ...pendingCreated];
    newKey = "";
    expandedKey = key;
  }
</script>

<OverlaySurface bind:open title="Decisions">
  {#snippet toolbar()}
    <input class="search" type="text" placeholder="Search decisions…" bind:value={search} />
    <label class="modonly">
      <input type="checkbox" bind:checked={modOnly} />
      Mod only
    </label>
    <span class="counter">{shown.length}</span>
  {/snippet}

  <div class="body">
    <div class="newrow">
      <input
        class="newkey"
        type="text"
        placeholder="new_decision_key"
        bind:value={newKey}
        onkeydown={(e) => e.key === "Enter" && createDecision()}
      />
      <button class="newbtn" onclick={createDecision}>＋ New decision</button>
      {#if newKeyError}<span class="newerr">{newKeyError}</span>{/if}
    </div>

    {#if loading}
      <p class="msg">Loading decisions…</p>
    {:else if error}
      <p class="msg err">{error}</p>
    {:else if shown.length === 0}
      <p class="msg">No decisions match.</p>
    {/if}

    <ul class="list">
      {#each shown as d (d.file + "::" + d.key)}
        <li class="row" class:expanded={expandedKey === d.key}>
          <button class="rowhead" onclick={() => toggle(d.key)}>
            <span class="caret">{expandedKey === d.key ? "▾" : "▸"}</span>
            <span class="title">{d.title}</span>
            <code class="key">{d.key}</code>
            {#if d.major}<span class="badge major">major</span>{/if}
            <span class="badge origin {d.origin}">{d.origin}</span>
            {#if d.pending}<span class="badge pending">unsaved</span>{/if}
            <span class="file">{d.file}</span>
          </button>
          {#if expandedKey === d.key}
            <div class="rowbody">
              <DecisionEditor
                entry={d}
                {installPath}
                {modPath}
                {selectedDate}
                {queue}
                {triggers}
                {effects}
                {countries}
                onjumpcountry={jump}
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

  .newkey {
    background: var(--bg-1);
    border: 1px solid var(--border);
    color: var(--text-1);
    font-family: inherit;
    font-size: 0.83rem;
    padding: 0.25rem 0.4rem;
    width: 14rem;
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

  .rowhead {
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

  .rowhead:hover {
    background: var(--bg-3);
  }

  .caret {
    color: var(--text-2);
    width: 0.8rem;
    flex: none;
  }

  .rowhead .title {
    font-weight: 600;
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

  .badge.major {
    background: var(--accent-text);
    color: var(--text-inverse);
  }

  .badge.origin.base {
    background: var(--bg-3);
    color: var(--text-1);
  }

  .badge.origin.mod {
    background: var(--ok);
    color: var(--text-inverse);
  }

  .badge.pending {
    background: var(--warn);
    color: var(--text-inverse);
  }

  .file {
    margin-left: auto;
    color: var(--text-3);
    font-size: 0.72rem;
  }

  .rowbody {
    padding: 0 0.6rem 0.4rem;
  }
</style>
