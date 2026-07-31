<!--
  DecisionEditor — the expanded-row editor for one decision (Sprint 15).

  Parses the decision block + its potential/allow/effect sub-blocks through
  parse_script_block_with_edits (which folds the PENDING queue onto the file, so
  the tree reflects unsaved state), and re-parses whenever the queue changes. The
  three sub-blocks each feed a 14.2 ScriptTreeEditor; the host wraps every emitted
  edit batch into one composite and pushes it (decisions are date-agnostic).

  Edits:
    • title/desc loc → LocOverride
    • major toggle   → setScalar (key present) / insertStatement (key absent)
    • potential+allow+effect trees → the ScriptTreeEditor emission contract
    • ai_will_do     → shown raw-preserved (read-only, advanced)
    • availability   → evaluate_decision (potential ∧ allow), jump to a country
-->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { ScriptTreeEditor } from "$lib/components/script";
  import type { KnownKey, ScriptBlock, TreeNode } from "$lib/components/script";
  import type { DropdownItem } from "$lib/components/ui";
  import type { EditQueue, TypedEdit } from "$lib/edits.svelte";
  import type { DecisionEntry, DecisionAvailability } from "./decisionsTypes";

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
  }: {
    entry: DecisionEntry;
    installPath: string;
    modPath?: string | null;
    selectedDate?: string | null;
    queue: EditQueue;
    triggers: KnownKey[];
    effects: KnownKey[];
    countries?: DropdownItem[];
    onjumpcountry: (tag: string) => void;
  } = $props();

  const countryByTag = $derived(new Map(countries.map((c) => [c.key, c])));

  // --- Parsed blocks (re-fetched on queue change) --------------------------
  let decBlock = $state<ScriptBlock | null>(null);
  let potentialBlock = $state<ScriptBlock | null>(null);
  let allowBlock = $state<ScriptBlock | null>(null);
  let effectBlock = $state<ScriptBlock | null>(null);
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
  // Reload the decision + sub-blocks whenever the session or the queue changes.
  // Every dependency is read SYNCHRONOUSLY so the effect tracks it (the actual
  // fetch is async and untracked).
  $effect(() => {
    void installPath;
    void modPath;
    void entry.file;
    queue.version; // pending edits re-parse the tree
    const token = ++loadToken;
    void reload(token);
  });

  async function reload(token: number) {
    parseError = null;
    const dec = await parseBlock(entry.path);
    if (token !== loadToken) return;
    decBlock = dec;
    if (!dec) {
      parseError = "Could not parse this decision (it may not be saved yet).";
      potentialBlock = allowBlock = effectBlock = null;
      return;
    }
    const has = (k: string) => dec.nodes.some((n) => n.key === k);
    potentialBlock = has("potential") ? await parseBlock(entry.potentialPath) : null;
    allowBlock = has("allow") ? await parseBlock(entry.allowPath) : null;
    effectBlock = has("effect") ? await parseBlock(entry.effectPath) : null;
  }

  // --- Derived meta from the parsed decision block -------------------------
  const decNodes = $derived<TreeNode[]>(decBlock?.nodes ?? []);
  const majorNode = $derived(decNodes.find((n) => n.key === "major"));
  const effectiveMajor = $derived(majorNode?.value?.text?.toLowerCase() === "yes");
  const aiWillDoRaw = $derived(
    decNodes.find((n) => n.key === "ai_will_do")?.value?.text ??
      decNodes.find((n) => n.key === "ai_will_do")?.raw ??
      null,
  );

  // --- Edit emission -------------------------------------------------------
  function push(edits: TypedEdit[], label: string) {
    if (edits.length) queue.push({ label, edits });
  }

  function onTreeEdit(edits: TypedEdit[], label: string) {
    push(edits, label);
  }

  // Loc field current values (pending override wins, else the base loc value).
  const titleValue = $derived(queue.pendingLocOverride(entry.titleKey) ?? entry.titleLoc ?? "");
  const descValue = $derived(queue.pendingLocOverride(entry.descKey) ?? entry.descLoc ?? "");

  function commitLoc(key: string, value: string, current: string) {
    if (value === current) return;
    push([{ kind: "locOverride", key, value }], `Edit decision text`);
  }

  function toggleMajor() {
    const target = !effectiveMajor;
    const edit: TypedEdit = majorNode
      ? { kind: "setScalar", file: entry.file, path: [...entry.path, "major"], value: target ? "yes" : "no", quoted: false }
      : { kind: "insertStatement", file: entry.file, blockPath: entry.path, statement: `major = ${target ? "yes" : "no"}` };
    push([edit], target ? "Set decision major" : "Unset decision major");
  }

  function addBlock(name: "potential" | "allow" | "effect") {
    push(
      [{ kind: "insertStatement", file: entry.file, blockPath: entry.path, statement: `${name} = {\n}` }],
      `Add ${name} block`,
    );
  }

  // --- Availability (potential ∧ allow) ------------------------------------
  let availability = $state<DecisionAvailability | null>(null);
  let availLoading = $state(false);
  let availError = $state<string | null>(null);

  async function loadAvailability() {
    availError = null;
    availLoading = true;
    availability = null;
    try {
      availability = await invoke<DecisionAvailability>("evaluate_decision", {
        installPath,
        modPath,
        date: selectedDate,
        file: entry.file,
        potentialPath: entry.hasPotential ? entry.potentialPath : [],
        allowPath: entry.hasAllow ? entry.allowPath : [],
      });
    } catch (e) {
      availError = String(e);
    } finally {
      availLoading = false;
    }
  }

  const matched = $derived(availability?.verdicts.filter((v) => v.verdict === "yes") ?? []);
  const unknownCount = $derived(
    availability?.verdicts.filter((v) => v.verdict === "unknown").length ?? 0,
  );

  function labelFor(tag: string): string {
    return countryByTag.get(tag)?.label ?? tag;
  }
</script>

<div class="editor">
  {#if parseError}
    <p class="err">{parseError}</p>
  {/if}

  <!-- Loc + flags -->
  <div class="meta">
    <label class="fld">
      <span>Title</span>
      <input
        type="text"
        value={titleValue}
        onchange={(e) => commitLoc(entry.titleKey, e.currentTarget.value, titleValue)}
        placeholder={entry.titleKey}
      />
    </label>
    <label class="fld">
      <span>Description</span>
      <textarea
        rows="2"
        value={descValue}
        onchange={(e) => commitLoc(entry.descKey, e.currentTarget.value, descValue)}
        placeholder={entry.descKey}
      ></textarea>
    </label>
    <label class="chk">
      <input type="checkbox" checked={effectiveMajor} onchange={toggleMajor} />
      <span>Major decision</span>
    </label>
  </div>

  <!-- potential / allow / effect trees -->
  {#each [{ name: "potential", block: potentialBlock, has: entry.hasPotential, path: entry.potentialPath, reg: "triggers", known: triggers }, { name: "allow", block: allowBlock, has: entry.hasAllow, path: entry.allowPath, reg: "triggers", known: triggers }, { name: "effect", block: effectBlock, has: entry.hasEffect, path: entry.effectPath, reg: "effects", known: effects }] as sec (sec.name)}
    <section class="block">
      <h4>{sec.name}</h4>
      {#if sec.block}
        <ScriptTreeEditor
          file={entry.file}
          rootPath={sec.path}
          block={sec.block}
          registry={sec.reg as "triggers" | "effects"}
          known={sec.known}
          {countries}
          onedit={onTreeEdit}
        />
      {:else}
        <button class="add-block" onclick={() => addBlock(sec.name as "potential" | "allow" | "effect")}>
          ＋ Add {sec.name} block
        </button>
      {/if}
    </section>
  {/each}

  <!-- ai_will_do raw-preserved (advanced) -->
  {#if aiWillDoRaw}
    <section class="block">
      <h4>ai_will_do <span class="adv">advanced — raw-preserved</span></h4>
      <pre class="raw">{aiWillDoRaw}</pre>
    </section>
  {/if}

  <!-- Availability -->
  <section class="block">
    <h4>Availability</h4>
    {#if entry.pending}
      <p class="note">Save the project to evaluate availability for this new decision.</p>
    {:else}
      <button class="avail-btn" onclick={loadAvailability} disabled={availLoading}>
        {availLoading ? "Evaluating…" : "Who can take this?"}
      </button>
      {#if availError}
        <p class="err">{availError}</p>
      {/if}
      {#if availability}
        {#if unknownCount > 0}
          <p class="approx">
            Approximate — {availability.unevaluated.length} condition{availability.unevaluated
              .length === 1
              ? ""
              : "s"} not evaluated ({availability.unevaluated.join(", ")}); {unknownCount} country
            verdict{unknownCount === 1 ? "" : "s"} unknown.
          </p>
        {/if}
        <p class="count">{matched.length} matching countr{matched.length === 1 ? "y" : "ies"}</p>
        <div class="tags">
          {#each matched as v (v.tag)}
            <button class="tag" onclick={() => onjumpcountry(v.tag)} title="Show on the political map">
              {#if countryByTag.get(v.tag)?.icon}
                <img src={countryByTag.get(v.tag)?.icon} alt="" />
              {:else if countryByTag.get(v.tag)?.swatch}
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

  .chk {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    font-size: 0.85rem;
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

  .raw {
    margin: 0;
    background: var(--bg-0);
    border: 1px solid var(--border);
    color: var(--ok);
    font-size: 0.75rem;
    padding: 0.4rem 0.5rem;
    overflow-x: auto;
  }

  .add-block,
  .avail-btn {
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
    margin: 0;
    font-size: 0.8rem;
    color: var(--text-2);
  }

  .err {
    margin: 0;
    font-size: 0.8rem;
    color: var(--err);
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

  .tag img {
    width: 1rem;
    height: 0.7rem;
    object-fit: cover;
  }
</style>
