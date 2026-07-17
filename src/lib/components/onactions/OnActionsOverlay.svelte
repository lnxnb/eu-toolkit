<!--
  OnActionsOverlay (View ▸ On Actions) — Sprint 28.

  Browses `common/on_actions` engine hooks and edits each hook's effect body via
  the 14.2 tree editor, plus dedicated typed rows for its `events` (unconditional)
  and `random_events` (weighted) event-firing lists. Feeds the events editor's
  "referenced from" scan (backend side).
-->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { OverlaySurface, ScriptTreeEditor } from "$lib/components/script";
  import type { KnownKey, ScriptBlock } from "$lib/components/script";
  import type { EditQueue, TypedEdit } from "$lib/edits.svelte";

  interface WeightedEvent { weight: string; id: string }
  interface OnActionHook {
    hook: string;
    file: string;
    origin: "base" | "mod";
    path: string[];
    eventsPath: string[];
    randomEventsPath: string[];
    events: string[];
    randomEvents: WeightedEvent[];
    hasEvents: boolean;
    hasRandomEvents: boolean;
    effectCount: number;
  }

  let {
    open = $bindable(false),
    installPath,
    modPath,
    queue,
  }: {
    open?: boolean;
    installPath: string;
    modPath: string | null;
    queue: EditQueue;
  } = $props();

  let hooks = $state<OnActionHook[]>([]);
  let query = $state("");
  let selected = $state<OnActionHook | null>(null);
  let knownEffects = $state<KnownKey[]>([]);
  let eventIds = $state<string[]>([]);

  const filtered = $derived(
    hooks.filter((h) => h.hook.toLowerCase().includes(query.trim().toLowerCase())),
  );

  $effect(() => {
    void invoke<KnownKey[]>("get_known_effects").then((k) => (knownEffects = k));
  });

  // Load hooks (and refresh after saves so a new hook/edit is reflected).
  $effect(() => {
    void installPath;
    void modPath;
    void open;
    if (!open) return;
    void invoke<OnActionHook[]>("get_on_actions", { installPath, modPath }).then((h) => {
      hooks = h;
      // Re-bind the selection to the fresh object (paths/lists may have shifted).
      if (selected) {
        selected = h.find((x) => x.hook === selected!.hook && x.file === selected!.file) ?? null;
      }
    });
    // Event ids for the pickers (best-effort).
    void invoke<{ id: string }[]>("get_events", { installPath, modPath })
      .then((evs) => (eventIds = evs.map((e) => e.id)))
      .catch(() => (eventIds = []));
  });

  // --- Hook body (14.2 tree, pending-aware) ---------------------------------
  let block = $state<ScriptBlock | null>(null);
  let error = $state<string | null>(null);
  let token = 0;

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

  async function reload(sel: OnActionHook, t: number) {
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

  // --- events list editing --------------------------------------------------
  let newEventId = $state("");
  function addEvent() {
    const sel = selected;
    const id = newEventId.trim();
    if (!sel || !id) return;
    const edits: TypedEdit[] = [];
    if (!sel.hasEvents) {
      edits.push({ kind: "insertStatement", file: sel.file, blockPath: sel.path, statement: "events = {\n}" });
    }
    edits.push({ kind: "addId", file: sel.file, listPath: sel.eventsPath, id });
    queue.push({ label: `Add event ${id} to ${sel.hook}`, edits });
    newEventId = "";
    // Reflect the new list membership optimistically.
    sel.events = [...sel.events, id];
    sel.hasEvents = true;
  }
  function removeEvent(id: string) {
    const sel = selected;
    if (!sel) return;
    queue.push({
      label: `Remove event ${id} from ${sel.hook}`,
      edits: [{ kind: "removeId", file: sel.file, listPath: sel.eventsPath, id }],
    });
    sel.events = sel.events.filter((e) => e !== id);
  }

  // --- random_events editing ------------------------------------------------
  let newWeight = $state("100");
  let newRandomId = $state("");
  function addRandomEvent() {
    const sel = selected;
    const id = newRandomId.trim();
    const w = newWeight.trim();
    if (!sel || !id || !w) return;
    const edits: TypedEdit[] = [];
    if (!sel.hasRandomEvents) {
      edits.push({ kind: "insertStatement", file: sel.file, blockPath: sel.path, statement: "random_events = {\n}" });
    }
    edits.push({ kind: "insertStatement", file: sel.file, blockPath: sel.randomEventsPath, statement: `${w} = ${id}` });
    queue.push({ label: `Add weighted event ${id} to ${sel.hook}`, edits });
    sel.randomEvents = [...sel.randomEvents, { weight: w, id }];
    sel.hasRandomEvents = true;
    newRandomId = "";
  }
  function removeRandomEvent(entry: WeightedEvent) {
    const sel = selected;
    if (!sel) return;
    queue.push({
      label: `Remove weighted event ${entry.id} from ${sel.hook}`,
      edits: [{ kind: "removeStatement", file: sel.file, blockPath: sel.randomEventsPath, key: entry.weight, value: entry.id }],
    });
    sel.randomEvents = sel.randomEvents.filter((r) => r !== entry);
  }

  // --- create ---------------------------------------------------------------
  let creating = $state(false);
  let newHook = $state("");
  let createError = $state<string | null>(null);
  async function doCreate() {
    createError = null;
    const hook = newHook.trim();
    if (!hook) return;
    try {
      const sc = await invoke<{ file: string; statement: string }>("scaffold_on_action", { hook });
      const fileHasHooks = hooks.some((h) => h.file === sc.file && h.origin === "mod");
      const edit: TypedEdit = fileHasHooks
        ? { kind: "insertStatement", file: sc.file, blockPath: [], statement: sc.statement }
        : { kind: "createFile", file: sc.file, text: sc.statement + "\n" };
      queue.push({ label: `Create on_action ${hook}`, edits: [edit] });
      creating = false;
      newHook = "";
      hooks = await invoke<OnActionHook[]>("get_on_actions", { installPath, modPath });
      selected = hooks.find((h) => h.hook === hook && h.file === sc.file) ?? null;
    } catch (e) {
      createError = String(e);
    }
  }
</script>

<OverlaySurface bind:open title="On Actions (engine hooks)">
  <div class="wrap">
    <div class="list">
      <div class="list-head">
        <input class="search" placeholder="Search hooks…" bind:value={query} />
        <button class="new-btn" onclick={() => { creating = true; createError = null; }}>＋ New…</button>
        <div class="count">{filtered.length} of {hooks.length} hooks</div>
      </div>
      <ul class="hooks">
        {#each filtered as h (h.hook + h.file)}
          <li>
            <button class="hook" class:sel={selected?.hook === h.hook && selected?.file === h.file} onclick={() => (selected = h)}>
              <span class="hname">{h.hook}</span>
              {#if h.origin === "mod"}<span class="mod">mod</span>{/if}
              {#if h.events.length}<span class="tag ev" title="events">{h.events.length}</span>{/if}
              {#if h.randomEvents.length}<span class="tag rev" title="random_events">~{h.randomEvents.length}</span>{/if}
            </button>
          </li>
        {/each}
        {#if filtered.length === 0}<li class="empty">No hooks.</li>{/if}
      </ul>
    </div>

    <div class="editor">
      {#if creating}
        <div class="create">
          <h3>New on_action hook</h3>
          <input class="cname" placeholder="on_startup" bind:value={newHook} />
          {#if createError}<p class="err">{createError}</p>{/if}
          <div class="cbtns">
            <button class="ok" onclick={doCreate} disabled={!newHook.trim()}>Create</button>
            <button class="cancel" onclick={() => (creating = false)}>Cancel</button>
          </div>
        </div>
      {:else if selected}
        <div class="ed-head">
          <code>{selected.hook}</code>
          <span class="file">{selected.file}</span>
        </div>

        <datalist id="onaction-event-ids">
          {#each eventIds.slice(0, 3000) as id}<option value={id}></option>{/each}
        </datalist>

        <section class="fires">
          <h4>events <span class="hint">(fired unconditionally)</span></h4>
          <ul class="idlist">
            {#each selected.events as id (id)}
              <li><code>{id}</code><button class="x" onclick={() => removeEvent(id)}>×</button></li>
            {/each}
            {#if selected.events.length === 0}<li class="dim">none</li>{/if}
          </ul>
          <div class="addrow">
            <input list="onaction-event-ids" placeholder="event.id" bind:value={newEventId} />
            <button class="mini" onclick={addEvent} disabled={!newEventId.trim()}>＋ add</button>
          </div>
        </section>

        <section class="fires">
          <h4>random_events <span class="hint">(weighted; 0 = nothing)</span></h4>
          <ul class="idlist">
            {#each selected.randomEvents as r (r.weight + r.id)}
              <li><span class="w">{r.weight}</span><code>{r.id}</code><button class="x" onclick={() => removeRandomEvent(r)}>×</button></li>
            {/each}
            {#if selected.randomEvents.length === 0}<li class="dim">none</li>{/if}
          </ul>
          <div class="addrow">
            <input class="wt" type="number" placeholder="weight" bind:value={newWeight} />
            <input list="onaction-event-ids" placeholder="event.id" bind:value={newRandomId} />
            <button class="mini" onclick={addRandomEvent} disabled={!newRandomId.trim()}>＋ add</button>
          </div>
        </section>

        <h4>Effect body</h4>
        {#if error}
          <p class="err">{error}</p>
        {:else if block}
          <ScriptTreeEditor
            file={selected.file}
            rootPath={selected.path}
            {block}
            registry="effects"
            known={knownEffects}
            onedit={onTreeEdit}
          />
        {:else}
          <p class="dim">Loading…</p>
        {/if}
      {:else}
        <p class="dim center">Select an engine hook to edit its effect body and event lists.</p>
      {/if}
    </div>
  </div>
</OverlaySurface>

<style>
  .wrap { display: flex; gap: 0.6rem; height: 100%; min-height: 0; }
  .list { flex: none; width: 20rem; display: flex; flex-direction: column; min-height: 0; border: 1px solid #1f242c; background: #21262e; }
  .list-head { display: flex; flex-wrap: wrap; gap: 0.3rem; padding: 0.35rem; border-bottom: 1px solid #1f242c; align-items: center; }
  .search { flex: 1; min-width: 8rem; background: #16191f; border: 1px solid #1f242c; color: #cfd4db; padding: 0.25rem 0.4rem; font-family: inherit; font-size: 0.8rem; }
  .new-btn { border: 1px solid #3a434f; background: #2b323d; color: #cfd4db; font-family: inherit; font-size: 0.72rem; padding: 0.1rem 0.45rem; cursor: pointer; }
  .new-btn:hover { background: #4a6da7; color: #fff; }
  .count { flex: 1 0 100%; font-size: 0.68rem; color: #8a919c; }
  .hooks { list-style: none; margin: 0; padding: 0; overflow-y: auto; flex: 1; min-height: 0; }
  .hook { display: flex; align-items: center; gap: 0.35rem; width: 100%; text-align: left; border: none; background: transparent; color: #cfd4db; font-family: inherit; font-size: 0.78rem; padding: 0.2rem 0.4rem; cursor: pointer; }
  .hook:hover { background: #262c35; }
  .hook.sel { background: #4a6da7; color: #fff; }
  .hname { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; flex: 1; }
  .mod { flex: none; font-size: 0.6rem; color: #9aecc0; border: 1px solid #2f5f48; padding: 0 0.2rem; }
  .tag { flex: none; font-size: 0.62rem; padding: 0 0.22rem; }
  .tag.ev { background: #2f4a6b; color: #bcd; }
  .tag.rev { background: #4a3b2f; color: #d8b45a; }
  .empty, .dim { color: #8a919c; font-size: 0.78rem; padding: 0.3rem; }
  .editor { flex: 1; min-width: 0; min-height: 0; overflow-y: auto; }
  .ed-head { display: flex; align-items: center; gap: 0.5rem; padding: 0.1rem 0 0.4rem; }
  .ed-head code { color: #9aecc0; background: #16191f; padding: 0.05rem 0.35rem; font-size: 0.9rem; }
  .file { font-size: 0.7rem; color: #8a919c; margin-left: auto; }
  h4 { margin: 0.6rem 0 0.2rem; font-size: 0.82rem; color: #cfd4db; }
  .hint { font-weight: 400; font-size: 0.7rem; color: #8a919c; }
  .fires { border: 1px solid #232a33; padding: 0.3rem 0.4rem; margin-bottom: 0.3rem; }
  .fires h4 { margin-top: 0; }
  .idlist { list-style: none; margin: 0.2rem 0; padding: 0; display: flex; flex-direction: column; gap: 0.1rem; }
  .idlist li { display: flex; align-items: center; gap: 0.4rem; font-size: 0.78rem; }
  .idlist code { color: #cfd4db; background: #16191f; padding: 0 0.3rem; }
  .w { flex: none; color: #d8b45a; font-variant-numeric: tabular-nums; width: 2.5rem; text-align: right; }
  .x { border: none; background: transparent; color: #8a919c; cursor: pointer; font-size: 0.95rem; padding: 0 0.2rem; }
  .x:hover { color: #fca5a5; }
  .addrow { display: flex; gap: 0.3rem; margin-top: 0.25rem; }
  .addrow input { background: #16191f; border: 1px solid #1f242c; color: #cfd4db; padding: 0.2rem 0.35rem; font-family: inherit; font-size: 0.78rem; }
  .addrow input:not(.wt) { flex: 1; }
  .wt { width: 4.5rem; }
  .mini { border: 1px solid #3a434f; background: #2b323d; color: #cfd4db; font-family: inherit; font-size: 0.72rem; padding: 0.1rem 0.5rem; cursor: pointer; flex: none; }
  .mini:hover:not(:disabled) { background: #4a6da7; color: #fff; }
  .mini:disabled { opacity: 0.5; cursor: default; }
  .err { color: #fca5a5; font-size: 0.78rem; }
  .center { text-align: center; padding: 2rem; }
  .create { max-width: 22rem; }
  .create h3 { margin: 0 0 0.5rem; font-size: 0.95rem; }
  .cname { width: 100%; background: #16191f; border: 1px solid #1f242c; color: #cfd4db; padding: 0.3rem 0.4rem; font-family: inherit; font-size: 0.85rem; }
  .cbtns { display: flex; gap: 0.5rem; margin-top: 0.6rem; }
  .ok { background: #4a6da7; border: 1px solid #4a6da7; color: #fff; font-family: inherit; padding: 0.25rem 0.8rem; cursor: pointer; }
  .ok:disabled { opacity: 0.5; cursor: default; }
  .cancel { background: #2b323d; border: 1px solid #3a434f; color: #cfd4db; font-family: inherit; padding: 0.25rem 0.8rem; cursor: pointer; }
</style>
