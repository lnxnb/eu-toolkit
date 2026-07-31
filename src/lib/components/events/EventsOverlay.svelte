<!--
  EventsOverlay — View ▸ Events… (Sprint 16).

  A full-screen OverlaySurface listing ALL events across the VFS (base + mod),
  grouped by namespace into collapsible groups. Search matches id / title / file;
  a country-vs-province badge and an origin badge sit on each row; a "mod only"
  filter narrows to project events. Expanding a row opens the EventEditor (loc,
  picture, flags, trigger, MTTH, options, and the can-happen-to / references list).
  "＋ New event" scaffolds a minimal is_triggered_only event (one option) into
  events/zz_eutoolkit_events.txt, declaring its namespace first if the file/
  namespace is new (vanilla uses `namespace =`, never `add_namespace`).

  Purity: owns no session/map state beyond what it's handed; it pushes composites
  to the shared `queue` and asks the parent to jump to a country (also closing).
-->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { OverlaySurface } from "$lib/components/script";
  import type { KnownKey } from "$lib/components/script";
  import type { DropdownItem } from "$lib/components/ui";
  import type { EditQueue, TypedEdit } from "$lib/edits.svelte";
  import EventEditor from "./EventEditor.svelte";
  import { SCAFFOLD_FILE, DEFAULT_NAMESPACE, type EventEntry } from "./eventsTypes";

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

  let fetched = $state<EventEntry[]>([]);
  let pendingCreated = $state<EventEntry[]>([]);
  let triggers = $state<KnownKey[]>([]);
  let effects = $state<KnownKey[]>([]);
  let countries = $state<DropdownItem[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);

  let search = $state("");
  let modOnly = $state(false);
  let expandedId = $state<string | null>(null);
  let collapsed = $state<Record<string, boolean>>({});

  let newNs = $state(DEFAULT_NAMESPACE);
  let newKind = $state<"country" | "province">("country");
  let newError = $state<string | null>(null);

  $effect(() => {
    if (!open) return;
    void load(installPath, modPath);
  });

  async function load(install: string, mod: string | null) {
    loading = true;
    error = null;
    try {
      const [evs, trig, eff, ctys] = await Promise.all([
        invoke<EventEntry[]>("get_events", { installPath: install, modPath: mod }),
        invoke<KnownKey[]>("get_known_triggers"),
        invoke<KnownKey[]>("get_known_effects"),
        invoke<CountryBrief[]>("list_countries", { installPath: install, modPath: mod }),
      ]);
      fetched = evs;
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

  // Merged list; pending scaffolds appear first.
  const all = $derived<EventEntry[]>([...pendingCreated, ...fetched]);

  const shown = $derived(
    all
      .filter((e) => !modOnly || e.origin === "mod")
      .filter((e) => {
        const q = search.trim().toLowerCase();
        if (!q) return true;
        return (
          e.id.toLowerCase().includes(q) ||
          e.title.toLowerCase().includes(q) ||
          e.file.toLowerCase().includes(q)
        );
      }),
  );

  // Group by namespace, preserving first-seen order; sort events by number.
  const groups = $derived.by(() => {
    const map = new Map<string, EventEntry[]>();
    for (const e of shown) {
      const arr = map.get(e.namespace) ?? [];
      arr.push(e);
      map.set(e.namespace, arr);
    }
    return [...map.entries()].map(([ns, evs]) => ({
      ns,
      events: evs.slice().sort((a, b) => (a.number ?? 0) - (b.number ?? 0)),
    }));
  });

  function toggleRow(id: string) {
    expandedId = expandedId === id ? null : id;
  }
  function toggleGroup(ns: string) {
    collapsed = { ...collapsed, [ns]: !collapsed[ns] };
  }
  function jump(tag: string) {
    open = false;
    onjumpcountry(tag);
  }
  function jumpFile(file: string) {
    // Filter the list to the referenced file (search matches file paths).
    search = file;
    modOnly = false;
  }

  // --- + New event ---------------------------------------------------------
  const NS_RE = /^[a-z][a-z0-9_]*$/;

  function nextNumberFor(ns: string): number {
    let max = 0;
    for (const e of all) {
      if (e.namespace === ns && e.number != null && e.number > max) max = e.number;
    }
    return max + 1;
  }

  // Which namespaces already have an event in the scaffold file (so their
  // `namespace =` line is already declared there) — fetched + pending.
  const declaredInScaffold = $derived(
    new Set(all.filter((e) => e.file === SCAFFOLD_FILE).map((e) => e.namespace)),
  );
  const scaffoldFileExists = $derived(
    fetched.some((e) => e.file === SCAFFOLD_FILE) ||
      queue.findLast((e) => e.kind === "createFile" && e.file === SCAFFOLD_FILE) != null,
  );

  function createEvent() {
    newError = null;
    const ns = newNs.trim();
    if (!NS_RE.test(ns)) {
      newError = "Namespace: lowercase letters, digits and underscores (start with a letter).";
      return;
    }
    const n = nextNumberFor(ns);
    const id = `${ns}.${n}`;
    const kindKey = newKind === "country" ? "country_event" : "province_event";
    const titleKey = `${id}.t`;
    const descKey = `${id}.d`;
    const nameKey = `${id}.a`;

    // A minimal, console-fireable is_triggered_only event with one option.
    const evBody =
      `${kindKey} = {\n` +
      `\tid = ${id}\n` +
      `\ttitle = "${titleKey}"\n` +
      `\tdesc = "${descKey}"\n` +
      `\tpicture = ECONOMY_eventPicture\n` +
      `\tis_triggered_only = yes\n\n` +
      `\toption = {\n\t\tname = "${nameKey}"\n\t}\n` +
      `}`;

    const edits: TypedEdit[] = [];
    if (!scaffoldFileExists) {
      // Brand-new file: namespace FIRST (must precede the event), then the event.
      edits.push({ kind: "createFile", file: SCAFFOLD_FILE, text: `namespace = ${ns}\n\n${evBody}\n` });
    } else {
      // Existing file: declare the namespace once (before the event) if new here,
      // then append the event. Same-file inserts compose in queue order, so the
      // namespace line lands before the appended event.
      if (!declaredInScaffold.has(ns)) {
        edits.push({ kind: "insertStatement", file: SCAFFOLD_FILE, blockPath: [], statement: `namespace = ${ns}` });
      }
      edits.push({ kind: "insertStatement", file: SCAFFOLD_FILE, blockPath: [], statement: evBody });
    }
    edits.push({ kind: "locOverride", key: titleKey, value: "New Event" });
    edits.push({ kind: "locOverride", key: descKey, value: "" });
    edits.push({ kind: "locOverride", key: nameKey, value: "Option A" });
    queue.push({ label: `Create event ${id}`, edits });

    const path = [kindKey];
    const entry: EventEntry = {
      id,
      namespace: ns,
      number: n,
      file: SCAFFOLD_FILE,
      origin: "mod",
      kind: newKind,
      isTriggeredOnly: true,
      fireOnlyOnce: false,
      hidden: false,
      major: false,
      titleKey,
      descKey,
      title: "New Event",
      titleLoc: "New Event",
      descLoc: "",
      picture: "ECONOMY_eventPicture",
      mtthBaseUnit: null,
      mtthBaseValue: null,
      mtthModifierCount: 0,
      options: [{ nameKey, nameLoc: "Option A", path: [kindKey, "option"] }],
      path,
      triggerPath: [...path, "trigger"],
      mtthPath: [...path, "mean_time_to_happen"],
      hasTrigger: false,
      hasMtth: false,
      pending: true,
    };
    pendingCreated = [entry, ...pendingCreated];
    expandedId = id;
    collapsed = { ...collapsed, [ns]: false };
  }
</script>

<OverlaySurface bind:open title="Events">
  {#snippet toolbar()}
    <input class="search" type="text" placeholder="Search id / title / file…" bind:value={search} />
    <label class="modonly">
      <input type="checkbox" bind:checked={modOnly} />
      Mod only
    </label>
    <span class="counter">{shown.length}</span>
  {/snippet}

  <div class="body">
    <div class="newrow">
      <label class="nsfld">
        namespace
        <input class="newns" type="text" bind:value={newNs} onkeydown={(e) => e.key === "Enter" && createEvent()} />
      </label>
      <select class="newkind" bind:value={newKind}>
        <option value="country">country</option>
        <option value="province">province</option>
      </select>
      <button class="newbtn" onclick={createEvent}>＋ New event</button>
      {#if newError}<span class="newerr">{newError}</span>{/if}
    </div>

    {#if loading}
      <p class="msg">Loading events…</p>
    {:else if error}
      <p class="msg err">{error}</p>
    {:else if shown.length === 0}
      <p class="msg">No events match.</p>
    {/if}

    <div class="grouplist">
      {#each groups as g (g.ns)}
        <section class="group">
          <button class="grouphead" onclick={() => toggleGroup(g.ns)}>
            <span class="caret">{collapsed[g.ns] ? "▸" : "▾"}</span>
            <span class="ns">{g.ns}</span>
            <span class="gcount">{g.events.length}</span>
          </button>
          {#if !collapsed[g.ns]}
            <ul class="list">
              {#each g.events as e (e.file + "::" + e.id)}
                <li class="row" class:expanded={expandedId === e.id}>
                  <button class="rowhead" onclick={() => toggleRow(e.id)}>
                    <span class="caret">{expandedId === e.id ? "▾" : "▸"}</span>
                    <span class="title">{e.title}</span>
                    <code class="id">{e.id}</code>
                    <span class="badge kind {e.kind}">{e.kind}</span>
                    {#if e.isTriggeredOnly}<span class="badge tri">triggered</span>{/if}
                    <span class="badge origin {e.origin}">{e.origin}</span>
                    {#if e.pending}<span class="badge pending">unsaved</span>{/if}
                    <span class="file">{e.file}</span>
                  </button>
                  {#if expandedId === e.id}
                    <div class="rowbody">
                      <EventEditor
                        entry={e}
                        {installPath}
                        {modPath}
                        {selectedDate}
                        {queue}
                        {triggers}
                        {effects}
                        {countries}
                        onjumpcountry={jump}
                        onjumpfile={jumpFile}
                      />
                    </div>
                  {/if}
                </li>
              {/each}
            </ul>
          {/if}
        </section>
      {/each}
    </div>
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
    width: 18rem;
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

  .nsfld {
    display: flex;
    align-items: center;
    gap: 0.3rem;
    font-size: 0.78rem;
    color: var(--text-2);
  }

  .newns {
    background: var(--bg-1);
    border: 1px solid var(--border);
    color: var(--text-1);
    font-family: inherit;
    font-size: 0.83rem;
    padding: 0.25rem 0.4rem;
    width: 10rem;
  }

  .newkind {
    background: var(--bg-1);
    border: 1px solid var(--border);
    color: var(--text-1);
    font-family: inherit;
    font-size: 0.82rem;
    padding: 0.25rem 0.3rem;
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

  .grouplist {
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
  }

  .group {
    border: 1px solid var(--border);
  }

  .grouphead {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    width: 100%;
    text-align: left;
    border: none;
    background: var(--bg-2);
    color: var(--text-1);
    font-family: inherit;
    font-size: 0.85rem;
    padding: 0.3rem 0.5rem;
    cursor: pointer;
  }

  .grouphead:hover {
    background: var(--bg-3);
  }

  .ns {
    font-weight: 700;
    color: var(--ok);
  }

  .gcount {
    color: var(--text-2);
    font-size: 0.78rem;
  }

  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
  }

  .row {
    border-top: 1px solid var(--border);
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
    font-size: 0.85rem;
    padding: 0.3rem 0.5rem 0.3rem 1.2rem;
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

  .id {
    color: var(--ok);
    background: var(--bg-0);
    padding: 0 0.3rem;
    font-size: 0.74rem;
  }

  .badge {
    font-size: 0.66rem;
    text-transform: uppercase;
    letter-spacing: 0.03em;
    padding: 0.05rem 0.35rem;
    border: 1px solid var(--border);
  }

  .badge.kind.country {
    background: var(--accent-text);
    color: var(--text-inverse);
  }

  .badge.kind.province {
    background: var(--accent-text);
    color: var(--text-inverse);
  }

  .badge.tri {
    background: var(--warn);
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
    padding: 0 0.6rem 0.4rem 1.2rem;
  }
</style>
