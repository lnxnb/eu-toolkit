<!--
  ScriptedOverlay (View ▸ Scripted Triggers/Effects) — Sprint 28.

  Browses `common/scripted_triggers` + `common/scripted_effects` and edits each
  definition's body through the 14.2 tree editor (trigger-shaped for triggers,
  effect-shaped for effects). `$PARAMETER$` meta-script tokens are surfaced
  read-only. This is ALSO the jump target for scripted-name links in every other
  14.2 tree (see scripted.svelte.ts / ScriptNode): opening it with `focusName`
  selects that definition.
-->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import OverlaySurface from "./OverlaySurface.svelte";
  import ScriptTreeEditor from "./ScriptTreeEditor.svelte";
  import type { KnownKey, ScriptBlock } from "./scriptTypes";
  import { LoadingState, type DropdownItem } from "$lib/components/ui";
  import type { EditQueue, TypedEdit } from "$lib/edits.svelte";
  import { scriptedDefs, loadScriptedDefs, type ScriptedDef } from "$lib/scripted.svelte";

  let {
    open = $bindable(false),
    focusName = $bindable<string | null>(null),
    installPath,
    modPath,
    queue,
    countries = [],
  }: {
    open?: boolean;
    focusName?: string | null;
    installPath: string;
    modPath: string | null;
    queue: EditQueue;
    countries?: DropdownItem[];
  } = $props();

  let query = $state("");
  let kindFilter = $state<"all" | "trigger" | "effect">("all");
  let selected = $state<ScriptedDef | null>(null);
  let knownTriggers = $state<KnownKey[]>([]);
  let knownEffects = $state<KnownKey[]>([]);

  const defs = $derived(scriptedDefs());
  const filtered = $derived(
    defs
      .filter((d) => kindFilter === "all" || d.kind === kindFilter)
      .filter((d) => d.name.toLowerCase().includes(query.trim().toLowerCase()))
      .slice()
      .sort((a, b) => a.name.localeCompare(b.name)),
  );

  // Fetch the known-key registries once.
  $effect(() => {
    void invoke<KnownKey[]>("get_known_triggers").then((k) => (knownTriggers = k));
    void invoke<KnownKey[]>("get_known_effects").then((k) => (knownEffects = k));
  });

  // Honour a focus request (a jump-link) — select that definition when open.
  $effect(() => {
    if (open && focusName) {
      const hit = defs.find((d) => d.name === focusName);
      if (hit) selected = hit;
      focusName = null;
    }
  });

  // --- Selected definition body (14.2 tree, reflects pending edits) ----------
  let block = $state<ScriptBlock | null>(null);
  let error = $state<string | null>(null);
  let token = 0;
  const known = $derived(selected?.kind === "effect" ? knownEffects : knownTriggers);
  const registry = $derived<"triggers" | "effects">(
    selected?.kind === "effect" ? "effects" : "triggers",
  );

  $effect(() => {
    void installPath;
    void modPath;
    queue.version;
    const sel = selected;
    const t = ++token;
    if (!open || !sel) {
      block = null;
      return;
    }
    void reload(sel, t);
  });

  async function reload(sel: ScriptedDef, t: number) {
    error = null;
    try {
      const b = await invoke<ScriptBlock>("parse_script_block_with_edits", {
        installPath,
        modPath,
        file: sel.file,
        path: sel.path,
        edits: queue.serialize(),
      });
      if (t !== token) return;
      block = b;
    } catch (e) {
      if (t !== token) return;
      block = null;
      error = String(e);
    }
  }

  function onTreeEdit(edits: TypedEdit[], label: string) {
    if (edits.length) queue.push({ label, edits });
  }

  // --- Create ---------------------------------------------------------------
  let creating = $state(false);
  let newKind = $state<"trigger" | "effect">("trigger");
  let newName = $state("");
  let createError = $state<string | null>(null);

  async function doCreate() {
    createError = null;
    const name = newName.trim();
    if (!name) return;
    if (defs.some((d) => d.name === name)) {
      createError = "A scripted definition with that name already exists.";
      return;
    }
    try {
      const sc = await invoke<{ file: string; statement: string }>("scaffold_scripted", {
        kind: newKind,
        name,
      });
      // Insert into the toolkit file if it already holds defs, else create it.
      const fileHasDefs = defs.some((d) => d.file === sc.file && d.origin === "mod");
      const edit: TypedEdit = fileHasDefs
        ? { kind: "insertStatement", file: sc.file, blockPath: [], statement: sc.statement }
        : { kind: "createFile", file: sc.file, text: sc.statement + "\n" };
      queue.push({ label: `Create scripted ${newKind} ${name}`, edits: [edit] });
      await loadScriptedDefs(installPath, modPath);
      const hit = scriptedDefs().find((d) => d.name === name);
      if (hit) selected = hit;
      creating = false;
      newName = "";
    } catch (e) {
      createError = String(e);
    }
  }
</script>

<OverlaySurface bind:open title="Scripted Triggers & Effects">
  <div class="wrap">
    <div class="list">
      <div class="list-head">
        <input class="search" placeholder="Search…" bind:value={query} />
        <div class="filters">
          {#each ["all", "trigger", "effect"] as k}
            <button class="chip" class:on={kindFilter === k} onclick={() => (kindFilter = k as typeof kindFilter)}>
              {k}
            </button>
          {/each}
        </div>
        <button class="new-btn" onclick={() => { creating = true; createError = null; }}>＋ New…</button>
        <div class="count">{filtered.length} of {defs.length}</div>
      </div>
      <ul class="defs">
        {#each filtered as d (d.kind + d.name + d.file)}
          <li>
            <button class="def" class:sel={selected?.name === d.name && selected?.file === d.file} onclick={() => (selected = d)}>
              <span class="kind" class:effect={d.kind === "effect"}>{d.kind === "trigger" ? "T" : "E"}</span>
              <span class="dname">{d.name}</span>
              {#if d.origin === "mod"}<span class="mod">mod</span>{/if}
              {#if d.params.length}<span class="pcount" title={d.params.join(", ")}>${d.params.length}</span>{/if}
            </button>
          </li>
        {/each}
        {#if filtered.length === 0}
          <li class="empty">No definitions.</li>
        {/if}
      </ul>
    </div>

    <div class="editor">
      {#if creating}
        <div class="create">
          <h3>New scripted definition</h3>
          <div class="crow">
            <label><input type="radio" bind:group={newKind} value="trigger" /> Trigger</label>
            <label><input type="radio" bind:group={newKind} value="effect" /> Effect</label>
          </div>
          <input class="cname" placeholder="definition_name" bind:value={newName} />
          {#if createError}<p class="err">{createError}</p>{/if}
          <div class="cbtns">
            <button class="ok" onclick={doCreate} disabled={!newName.trim()}>Create</button>
            <button class="cancel" onclick={() => (creating = false)}>Cancel</button>
          </div>
        </div>
      {:else if selected}
        <div class="ed-head">
          <span class="kind" class:effect={selected.kind === "effect"}>{selected.kind}</span>
          <code>{selected.name}</code>
          <span class="file">{selected.file}</span>
        </div>
        {#if selected.params.length}
          <div class="params">
            <span class="plabel">Parameters (read-only):</span>
            {#each selected.params as p}<code class="ptok">${p}$</code>{/each}
          </div>
        {/if}
        {#if error}
          <p class="err">{error}</p>
        {:else if block}
          <ScriptTreeEditor
            file={selected.file}
            rootPath={selected.path}
            {block}
            {registry}
            {known}
            {countries}
            onedit={onTreeEdit}
          />
        {:else}
          <LoadingState label="Loading scripted definitions…" />
        {/if}
      {:else}
        <p class="dim center">Select a scripted trigger or effect to edit its body.</p>
      {/if}
    </div>
  </div>
</OverlaySurface>

<style>
  .wrap { display: flex; gap: 0.6rem; height: 100%; min-height: 0; }
  .list { flex: none; width: 22rem; display: flex; flex-direction: column; min-height: 0; border: 1px solid var(--border); background: var(--bg-1); }
  .list-head { display: flex; flex-wrap: wrap; gap: 0.3rem; padding: 0.35rem; border-bottom: 1px solid var(--border); align-items: center; }
  .search { flex: 1; min-width: 8rem; background: var(--bg-0); border: 1px solid var(--border); color: var(--text-1); padding: 0.25rem 0.4rem; font-family: inherit; font-size: 0.8rem; }
  .filters { display: flex; gap: 0.2rem; }
  .chip { border: 1px solid var(--bg-3); background: var(--bg-2); color: var(--text-1); font-family: inherit; font-size: 0.7rem; padding: 0.1rem 0.4rem; cursor: pointer; text-transform: capitalize; }
  .chip.on { background: var(--accent); border-color: var(--accent); color: var(--text-inverse); }
  .new-btn { border: 1px solid var(--bg-3); background: var(--bg-2); color: var(--text-1); font-family: inherit; font-size: 0.72rem; padding: 0.1rem 0.45rem; cursor: pointer; }
  .new-btn:hover { background: var(--accent); color: var(--text-inverse); }
  .count { flex: 1 0 100%; font-size: 0.68rem; color: var(--text-2); }
  .defs { list-style: none; margin: 0; padding: 0; overflow-y: auto; flex: 1; min-height: 0; }
  .def { display: flex; align-items: center; gap: 0.35rem; width: 100%; text-align: left; border: none; background: transparent; color: var(--text-1); font-family: inherit; font-size: 0.8rem; padding: 0.2rem 0.4rem; cursor: pointer; }
  .def:hover { background: var(--bg-2); }
  .def.sel { background: var(--accent); color: var(--text-inverse); }
  .kind { flex: none; font-size: 0.6rem; font-weight: 700; background: var(--ok); color: var(--text-inverse); padding: 0 0.28rem; }
  .kind.effect { background: var(--warn); }
  .dname { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; flex: 1; }
  .mod { flex: none; font-size: 0.6rem; color: var(--ok); border: 1px solid var(--ok); padding: 0 0.2rem; }
  .pcount { flex: none; font-size: 0.62rem; color: var(--warn); }
  .empty { color: var(--text-2); font-size: 0.78rem; padding: 0.4rem; }
  .editor { flex: 1; min-width: 0; min-height: 0; overflow-y: auto; }
  .ed-head { display: flex; align-items: center; gap: 0.5rem; padding: 0.2rem 0.1rem 0.4rem; }
  .ed-head code { color: var(--ok); background: var(--bg-0); padding: 0.05rem 0.35rem; font-size: 0.85rem; }
  .file { font-size: 0.7rem; color: var(--text-2); margin-left: auto; }
  .params { display: flex; flex-wrap: wrap; align-items: center; gap: 0.3rem; padding: 0.2rem 0 0.4rem; }
  .plabel { font-size: 0.72rem; color: var(--text-2); }
  .ptok { background: var(--bg-1); color: var(--warn); padding: 0 0.3rem; font-size: 0.74rem; }
  .err { color: var(--err); font-size: 0.78rem; }
  .dim { color: var(--text-2); font-size: 0.8rem; }
  .center { text-align: center; padding: 2rem; }
  .create { max-width: 24rem; }
  .create h3 { margin: 0 0 0.5rem; font-size: 0.95rem; }
  .crow { display: flex; gap: 1rem; margin-bottom: 0.5rem; font-size: 0.82rem; }
  .cname { width: 100%; background: var(--bg-0); border: 1px solid var(--border); color: var(--text-1); padding: 0.3rem 0.4rem; font-family: inherit; font-size: 0.85rem; }
  .cbtns { display: flex; gap: 0.5rem; margin-top: 0.6rem; }
  .ok { background: var(--accent); border: 1px solid var(--accent); color: var(--text-inverse); font-family: inherit; padding: 0.25rem 0.8rem; cursor: pointer; }
  .ok:disabled { opacity: 0.5; cursor: default; }
  .cancel { background: var(--bg-2); border: 1px solid var(--bg-3); color: var(--text-1); font-family: inherit; padding: 0.25rem 0.8rem; cursor: pointer; }
</style>
