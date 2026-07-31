<!--
  EventEditor — the expanded-row editor for one event (Sprint 16).

  Parses the event block + its trigger / mean_time_to_happen sub-blocks through
  parse_script_block_with_edits (folding the PENDING queue, so the tree reflects
  unsaved state) and re-parses whenever the queue changes. Sub-editors:
    • title/desc loc            → LocOverride
    • picture                   → SpritePicker (14.4), filtered to *_eventPicture
    • flags (fire_only_once /   → setScalar (present) / insertStatement (absent)
      hidden / major / is_triggered_only)
    • trigger                   → ScriptTreeEditor (triggers registry)
    • likelihood (MTTH)         → MtthEditor (base stepper + modifier rows)
    • options                   → OptionEditor each (add / remove / move-to-end)
    • "can happen to"           → evaluate_event (skipped for is_triggered_only,
      which instead shows "fired by script" + the referenced-from-N list)

  Kind (country vs province) is shown read-only: switching it re-scopes the whole
  event (province vs country scope) and isn't expressible as a byte-surgical edit,
  so it's chosen at creation time only — mirroring the out-of-scope id rename.

  Reorder note: InsertStatement is append-to-end, so arbitrary up/down reordering
  can't be expressed without a full-block rewrite (which would lose the clean
  byte-surgical diff). "Move to end" (remove + re-append the option's raw) is the
  one reorder the wire vocabulary expresses cleanly, so that is what's offered.
-->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { ScriptTreeEditor, SpritePicker } from "$lib/components/script";
  import type { KnownKey, ScriptBlock, TreeNode } from "$lib/components/script";
  import type { DropdownItem } from "$lib/components/ui";
  import type { EditQueue, TypedEdit } from "$lib/edits.svelte";
  import MtthEditor from "./MtthEditor.svelte";
  import OptionEditor from "./OptionEditor.svelte";
  import {
    EVENT_PICTURE_SUFFIX,
    type EventEntry,
    type EventEvaluation,
    type EventReference,
  } from "./eventsTypes";

  let {
    entry,
    installPath,
    modPath = null,
    selectedDate = null,
    queue,
    triggers,
    effects,
    countries = [],
    onjumpcountry,
    onjumpfile,
  }: {
    entry: EventEntry;
    installPath: string;
    modPath?: string | null;
    selectedDate?: string | null;
    queue: EditQueue;
    triggers: KnownKey[];
    effects: KnownKey[];
    countries?: DropdownItem[];
    onjumpcountry: (tag: string) => void;
    onjumpfile: (file: string) => void;
  } = $props();

  const countryByTag = $derived(new Map(countries.map((c) => [c.key, c])));

  // --- Parsed blocks (re-fetched on queue change) --------------------------
  let eventBlock = $state<ScriptBlock | null>(null);
  let triggerBlock = $state<ScriptBlock | null>(null);
  let mtthBlock = $state<ScriptBlock | null>(null);
  let parseError = $state<string | null>(null);

  async function parseBlock(path: string[]): Promise<ScriptBlock | null> {
    try {
      return await invoke<ScriptBlock>("parse_script_block_with_edits", {
        installPath,
        modPath,
        file: entry.file,
        path,
        edits: queue.serialize(),
      });
    } catch {
      return null;
    }
  }

  let loadToken = 0;
  $effect(() => {
    void installPath;
    void modPath;
    void entry.file;
    queue.version;
    const token = ++loadToken;
    void reload(token);
  });

  async function reload(token: number) {
    parseError = null;
    const ev = await parseBlock(entry.path);
    if (token !== loadToken) return;
    eventBlock = ev;
    if (!ev) {
      parseError = "Could not parse this event (it may not be saved yet).";
      triggerBlock = mtthBlock = null;
      return;
    }
    const has = (k: string) => ev.nodes.some((n) => n.key === k);
    triggerBlock = has("trigger") ? await parseBlock(entry.triggerPath) : null;
    if (token !== loadToken) return;
    mtthBlock = has("mean_time_to_happen") ? await parseBlock(entry.mtthPath) : null;
  }

  // --- Derived meta from the parsed event block ----------------------------
  const evNodes = $derived<TreeNode[]>(eventBlock?.nodes ?? []);
  const nodeFor = (k: string) => evNodes.find((n) => n.key === k);

  const pictureNode = $derived(nodeFor("picture"));
  const pictureValue = $derived(pictureNode?.value?.text ?? entry.picture ?? "");

  const FLAGS = ["is_triggered_only", "fire_only_once", "hidden", "major"] as const;
  type Flag = (typeof FLAGS)[number];
  const flagOn = (f: Flag) => nodeFor(f)?.value?.text?.toLowerCase() === "yes";
  const isTriggeredOnly = $derived(flagOn("is_triggered_only"));

  // Option nodes in file order (block-valued leaves keyed `option`).
  const optionNodes = $derived(evNodes.filter((n) => n.key === "option"));

  // --- Edit emission -------------------------------------------------------
  function push(edits: TypedEdit[], label: string) {
    if (edits.length) queue.push({ label, edits });
  }
  function onTreeEdit(edits: TypedEdit[], label: string) {
    push(edits, label);
  }

  const titleValue = $derived(
    entry.titleKey ? (queue.pendingLocOverride(entry.titleKey) ?? entry.titleLoc ?? "") : "",
  );
  const descValue = $derived(
    entry.descKey ? (queue.pendingLocOverride(entry.descKey) ?? entry.descLoc ?? "") : "",
  );
  function commitLoc(key: string | null, value: string, current: string) {
    if (!key || value === current) return;
    push([{ kind: "locOverride", key, value }], "Edit event text");
  }

  function toggleFlag(f: Flag) {
    const node = nodeFor(f);
    const target = !flagOn(f);
    const edit: TypedEdit = node
      ? { kind: "setScalar", file: entry.file, path: [...entry.path, f], value: target ? "yes" : "no", quoted: false }
      : { kind: "insertStatement", file: entry.file, blockPath: entry.path, statement: `${f} = ${target ? "yes" : "no"}` };
    push([edit], `${target ? "Set" : "Unset"} ${f}`);
  }

  // --- Picture picker ------------------------------------------------------
  let pickingPicture = $state(false);
  function commitPicture(name: string) {
    const edit: TypedEdit = pictureNode
      ? { kind: "setScalar", file: entry.file, path: [...entry.path, "picture"], value: name, quoted: false }
      : { kind: "insertStatement", file: entry.file, blockPath: entry.path, statement: `picture = ${name}` };
    push([edit], "Set event picture");
    pickingPicture = false;
  }

  // --- trigger / MTTH add when absent --------------------------------------
  function addTrigger() {
    push([{ kind: "insertStatement", file: entry.file, blockPath: entry.path, statement: "trigger = {\n}" }], "Add trigger block");
  }
  function addMtth() {
    push([{ kind: "insertStatement", file: entry.file, blockPath: entry.path, statement: "mean_time_to_happen = {\n\t\tdays = 1\n\t}" }], "Add MTTH block");
  }

  // --- options add / remove / move-to-end ----------------------------------
  function addOption() {
    const stmt = `option = {\n\t\tname = "${entry.id}.opt"\n\t}`;
    push([{ kind: "insertStatement", file: entry.file, blockPath: entry.path, statement: stmt }], "Add option");
  }
  function removeOption(node: TreeNode) {
    const key = node.path[node.path.length - 1]; // occurrence-qualified `option#n`
    push([{ kind: "removeStatement", file: entry.file, blockPath: entry.path, key, value: null }], "Remove option");
  }
  function moveOptionToEnd(node: TreeNode) {
    const key = node.path[node.path.length - 1];
    push(
      [
        { kind: "removeStatement", file: entry.file, blockPath: entry.path, key, value: null },
        { kind: "insertStatement", file: entry.file, blockPath: entry.path, statement: node.raw },
      ],
      "Move option to end",
    );
  }

  // --- "Can happen to" (trigger evaluation) --------------------------------
  let evaluation = $state<EventEvaluation | null>(null);
  let evalLoading = $state(false);
  let evalError = $state<string | null>(null);
  async function loadEvaluation() {
    evalError = null;
    evalLoading = true;
    evaluation = null;
    try {
      evaluation = await invoke<EventEvaluation>("evaluate_event", {
        installPath,
        modPath,
        date: selectedDate,
        file: entry.file,
        triggerPath: entry.hasTrigger ? entry.triggerPath : [],
      });
    } catch (e) {
      evalError = String(e);
    } finally {
      evalLoading = false;
    }
  }
  const matched = $derived(evaluation?.verdicts.filter((v) => v.verdict === "yes") ?? []);
  const unknownCount = $derived(evaluation?.verdicts.filter((v) => v.verdict === "unknown").length ?? 0);
  const labelFor = (tag: string) => countryByTag.get(tag)?.label ?? tag;

  // --- References (is_triggered_only) --------------------------------------
  let refs = $state<EventReference[] | null>(null);
  let refsLoading = $state(false);
  async function loadRefs() {
    refsLoading = true;
    try {
      refs = await invoke<EventReference[]>("find_event_references", { installPath, modPath, id: entry.id });
    } catch {
      refs = [];
    } finally {
      refsLoading = false;
    }
  }
  // Auto-load references when an is_triggered_only event is expanded.
  $effect(() => {
    void entry.id;
    if (isTriggeredOnly && !entry.pending) void loadRefs();
    else refs = null;
  });
</script>

<div class="editor">
  {#if parseError}
    <p class="err">{parseError}</p>
  {/if}

  <!-- Identity + loc + kind -->
  <div class="meta">
    <div class="idrow">
      <code class="id">{entry.id}</code>
      <span class="badge kind {entry.kind}">{entry.kind}</span>
      <span class="badge origin {entry.origin}">{entry.origin}</span>
    </div>
    <label class="fld">
      <span>Title</span>
      <input
        type="text"
        value={titleValue}
        onchange={(e) => commitLoc(entry.titleKey, e.currentTarget.value, titleValue)}
        placeholder={entry.titleKey ?? "(no title key)"}
        disabled={!entry.titleKey}
      />
    </label>
    <label class="fld">
      <span>Description</span>
      <textarea
        rows="2"
        value={descValue}
        onchange={(e) => commitLoc(entry.descKey, e.currentTarget.value, descValue)}
        placeholder={entry.descKey ?? "(no desc key)"}
        disabled={!entry.descKey}
      ></textarea>
    </label>

    <!-- Picture -->
    <div class="fld">
      <span>Picture</span>
      <div class="picrow">
        <code class="picname">{pictureValue || "(none)"}</code>
        <button class="mini" onclick={() => (pickingPicture = !pickingPicture)}>
          {pickingPicture ? "Close" : "Change…"}
        </button>
      </div>
      {#if pickingPicture}
        <div class="picker">
          <SpritePicker
            {installPath}
            {modPath}
            contains={EVENT_PICTURE_SUFFIX}
            value={pictureValue || null}
            onselect={commitPicture}
          />
        </div>
      {/if}
    </div>

    <!-- Flags -->
    <div class="flags">
      {#each FLAGS as f (f)}
        <label class="chk">
          <input type="checkbox" checked={flagOn(f)} onchange={() => toggleFlag(f)} />
          <span>{f}</span>
        </label>
      {/each}
    </div>
  </div>

  <!-- Trigger -->
  <section class="block">
    <h4>Trigger</h4>
    {#if triggerBlock}
      <ScriptTreeEditor
        file={entry.file}
        rootPath={entry.triggerPath}
        block={triggerBlock}
        registry="triggers"
        known={triggers}
        {countries}
        onedit={onTreeEdit}
      />
    {:else}
      <button class="add-block" onclick={addTrigger}>＋ Add trigger block</button>
    {/if}
  </section>

  <!-- Likelihood (MTTH) -->
  <section class="block">
    <h4>Likelihood <span class="adv">mean time to happen</span></h4>
    {#if mtthBlock}
      <MtthEditor
        file={entry.file}
        mtthPath={entry.mtthPath}
        block={mtthBlock}
        {installPath}
        {modPath}
        {queue}
        {triggers}
        {countries}
        onedit={onTreeEdit}
      />
    {:else}
      <button class="add-block" onclick={addMtth}>＋ Add mean_time_to_happen</button>
    {/if}
  </section>

  <!-- Options -->
  <section class="block">
    <h4>Options</h4>
    <div class="options">
      {#each optionNodes as node, i (node.path.join("/"))}
        <div class="optwrap">
          <OptionEditor
            file={entry.file}
            optionPath={node.path}
            {installPath}
            {modPath}
            {queue}
            {effects}
            {countries}
            index={i + 1}
            onedit={onTreeEdit}
            onremove={() => removeOption(node)}
          />
          {#if optionNodes.length > 1 && i < optionNodes.length - 1}
            <button class="moveend" title="Move this option to the end" onclick={() => moveOptionToEnd(node)}>
              Move to end ▾
            </button>
          {/if}
        </div>
      {/each}
      <button class="add-block" onclick={addOption}>＋ Add option</button>
    </div>
  </section>

  <!-- Can happen to / fired by script -->
  <section class="block">
    <h4>Can happen to</h4>
    {#if isTriggeredOnly}
      <p class="note">This event is <strong>fired by script</strong> (is_triggered_only) — it has no trigger of its own.</p>
      {#if entry.pending}
        <p class="note">Save the project to scan for references.</p>
      {:else if refsLoading}
        <p class="note">Scanning references…</p>
      {:else if refs}
        <p class="count">Referenced from {refs.length} call site{refs.length === 1 ? "" : "s"}</p>
        <div class="reflist">
          {#each refs as r, i (r.file + i)}
            {#if r.location === "events"}
              <button class="ref jump" title="Show this file in the events list" onclick={() => onjumpfile(r.file)}>
                <span class="badge kind {r.kind}">{r.kind}</span>
                {r.file}
              </button>
            {:else}
              <span class="ref" title="Open the {r.location} editor to see this call">
                <span class="badge loc">{r.location}</span>
                {r.file}
              </span>
            {/if}
          {/each}
        </div>
      {/if}
    {:else if entry.pending}
      <p class="note">Save the project to evaluate which countries this can happen to.</p>
    {:else}
      <button class="avail-btn" onclick={loadEvaluation} disabled={evalLoading}>
        {evalLoading ? "Evaluating…" : "Who can this happen to?"}
      </button>
      {#if evalError}<p class="err">{evalError}</p>{/if}
      {#if evaluation}
        {#if unknownCount > 0}
          <p class="approx">
            Approximate — {evaluation.unevaluated.length} condition{evaluation.unevaluated.length === 1 ? "" : "s"}
            not evaluated ({evaluation.unevaluated.join(", ")}); {unknownCount} verdict{unknownCount === 1 ? "" : "s"} unknown.
          </p>
        {/if}
        <p class="count">{matched.length} matching countr{matched.length === 1 ? "y" : "ies"}</p>
        <div class="tags">
          {#each matched as v (v.tag)}
            <button class="tag" onclick={() => onjumpcountry(v.tag)} title="Show on the political map">
              {#if countryByTag.get(v.tag)?.swatch}
                <span class="sw" style:background={countryByTag.get(v.tag)?.swatch}></span>
              {/if}
              {labelFor(v.tag)}
            </button>
          {/each}
        </div>
      {/if}
    {/if}
  </section>
</div>

<style>
  .editor {
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
    padding: 0.5rem 0.2rem;
  }

  .meta {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }

  .idrow {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .id {
    color: var(--ok);
    background: var(--bg-0);
    padding: 0.05rem 0.35rem;
    font-size: 0.8rem;
  }

  .fld {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
    font-size: 0.78rem;
    color: var(--text-2);
  }

  .fld input,
  .fld textarea {
    background: var(--bg-1);
    border: 1px solid var(--border);
    color: var(--text-1);
    font-family: inherit;
    font-size: 0.85rem;
    padding: 0.25rem 0.35rem;
    resize: vertical;
  }

  .fld input:disabled,
  .fld textarea:disabled {
    opacity: 0.5;
  }

  .picrow {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .picname {
    color: var(--text-1);
    background: var(--bg-0);
    padding: 0.05rem 0.35rem;
    font-size: 0.76rem;
  }

  .mini {
    border: 1px solid var(--border);
    background: var(--bg-3);
    color: var(--text-1);
    font-family: inherit;
    font-size: 0.76rem;
    padding: 0.12rem 0.5rem;
    cursor: pointer;
  }

  .mini:hover {
    background: var(--accent);
    color: var(--text-inverse);
  }

  .picker {
    height: 22rem;
    margin-top: 0.3rem;
  }

  .flags {
    display: flex;
    flex-wrap: wrap;
    gap: 0.8rem;
  }

  .chk {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    font-size: 0.82rem;
    color: var(--text-1);
  }

  .block {
    border-top: 1px solid var(--border);
    padding-top: 0.4rem;
  }

  .block h4 {
    margin: 0 0 0.35rem;
    font-size: 0.8rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-2);
  }

  .adv {
    text-transform: none;
    letter-spacing: 0;
    color: var(--text-3);
    font-size: 0.72rem;
  }

  .options {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .optwrap {
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
  }

  .moveend {
    align-self: flex-end;
    border: 1px solid var(--border);
    background: var(--bg-2);
    color: var(--text-2);
    font-family: inherit;
    font-size: 0.72rem;
    padding: 0.1rem 0.5rem;
    cursor: pointer;
  }

  .moveend:hover {
    background: var(--bg-3);
    color: var(--text-inverse);
  }

  .add-block,
  .avail-btn {
    align-self: flex-start;
    border: 1px solid var(--border);
    background: var(--bg-3);
    color: var(--text-1);
    font-family: inherit;
    font-size: 0.8rem;
    padding: 0.28rem 0.6rem;
    cursor: pointer;
  }

  .add-block:hover,
  .avail-btn:hover {
    background: var(--accent);
    color: var(--text-inverse);
  }

  .avail-btn:disabled {
    opacity: 0.6;
    cursor: default;
  }

  .approx {
    margin: 0.4rem 0 0.2rem;
    font-size: 0.76rem;
    color: var(--warn);
  }

  .count {
    margin: 0.3rem 0;
    font-size: 0.8rem;
    color: var(--text-2);
  }

  .note {
    margin: 0.2rem 0;
    font-size: 0.8rem;
    color: var(--text-2);
  }

  .err {
    margin: 0;
    font-size: 0.8rem;
    color: var(--err);
  }

  .reflist {
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
  }

  .ref {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    border: 1px solid var(--border);
    background: var(--bg-1);
    color: var(--text-1);
    font-family: inherit;
    font-size: 0.76rem;
    padding: 0.15rem 0.4rem;
    text-align: left;
  }

  .ref.jump {
    cursor: pointer;
  }

  .ref.jump:hover {
    background: var(--accent);
    color: var(--text-inverse);
  }

  .tags {
    display: flex;
    flex-wrap: wrap;
    gap: 0.3rem;
  }

  .tag {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    border: 1px solid var(--border);
    background: var(--bg-1);
    color: var(--text-1);
    font-family: inherit;
    font-size: 0.78rem;
    padding: 0.15rem 0.4rem;
    cursor: pointer;
  }

  .tag:hover {
    background: var(--accent);
    color: var(--text-inverse);
  }

  .tag .sw {
    width: 0.7rem;
    height: 0.7rem;
    border: 1px solid var(--bg-0);
  }

  .badge {
    font-size: 0.68rem;
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

  .badge.origin.base {
    background: var(--bg-3);
    color: var(--text-1);
  }

  .badge.origin.mod {
    background: var(--ok);
    color: var(--text-inverse);
  }

  .badge.loc {
    background: var(--text-3);
    color: var(--text-inverse);
  }
</style>
