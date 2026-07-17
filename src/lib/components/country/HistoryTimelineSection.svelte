<!--
  HistoryTimelineSection — the country panel's History section (S3.2). Renders the
  country history file's dated blocks three ways, sharing ONE data source:

  * Historical Advisors — typed rows for every `advisor = { … }` dated block
    (name / type from the advisor-types registry / skill stepper / dates), plus
    an "Add advisor" form. Advisors are inherently dated, so a new one lands in
    the dated block for its own date (merge or date-ordered insert).
  * Rulers over time — every monarch/queen/heir dated block reuses the EXISTING
    ruler editor fields (CharacterCore) rather than a second implementation; each
    is an expander addressing its own occurrence-qualified dated block.
  * The reusable Timeline (SPRINT.md 2.3, same component the province panel uses)
    — the full chronological view of every dated block with generic add / edit /
    delete (government/religion/tech/tag-switch/elector scalars edit as text).

  All three read from the local `blocks` mirror of `details.dated_blocks`, folded
  optimistically on every edit so the view follows before save. Pending typed
  fields read through the shared character.ts queue projection; edits are
  occurrence-indexed and byte-surgical (mod_writer `Y.M.D#n` addressing).
-->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { SearchDropdown, DatePicker } from "$lib/components/ui";
  import type { DropdownItem } from "$lib/components/ui";
  import type { EditQueue } from "$lib/edits.svelte";
  import type { Calendar } from "$lib/calendar";
  import { compareDates } from "$lib/calendar";
  import Timeline from "$lib/components/Timeline.svelte";
  import type { TimelineBlock, TimelineIntent } from "$lib/components/timeline";
  import { intentToEdits } from "$lib/components/timeline";
  import { datedBlockSegment } from "$lib/editAtDate";
  import Stepper from "./Stepper.svelte";
  import CharacterCore from "./CharacterCore.svelte";
  import { charValue, setCharEdit } from "./character";
  import type { CountryDetails, RegistryEntry } from "./types";
  import {
    HOLDER_KEYS,
    personalityEffectFor,
    holderChar,
    advisorView,
    advisorBody,
    addAdvisorEdits,
    type CountryDatedBlock,
    type HolderKey,
  } from "./history";

  let {
    installPath,
    modPath,
    tag,
    details,
    queue,
    calendar = null,
    date = null,
    startDate = "1444.11.11",
    cultures,
    religions,
    personalityItems,
    onopenmechanics,
  }: {
    installPath: string;
    modPath: string | null;
    tag: string;
    details: CountryDetails;
    queue: EditQueue;
    calendar?: Calendar | null;
    /** Selected view/edit date (Sprint 12.2); null = effective start. */
    date?: string | null;
    /** The mod's effective start date (Sprint 12.3); the base-state baseline. */
    startDate?: string;
    cultures: DropdownItem[];
    religions: DropdownItem[];
    personalityItems: DropdownItem[];
    /** Open the Mechanics editor (advisor-types family) — edit/new affordance. */
    onopenmechanics?: (family: string, key?: string) => void;
  } = $props();

  const file = $derived(details.history_file ?? `history/countries/${tag} - ${details.name}.txt`);

  // Advisor-type registry (for the type picker). Loaded once per session.
  let advisorTypes = $state<DropdownItem[]>([]);
  $effect(() => {
    invoke<RegistryEntry[]>("get_registry", { name: "advisor_types", installPath, modPath })
      .then((v) => (advisorTypes = v.map((e) => ({ key: e.key, label: e.name }))))
      .catch(() => {});
  });

  // Local mirror of the backend dated blocks, folded optimistically on edits so
  // the view follows before save. Reset whenever details (re)loads.
  let blocks = $state<CountryDatedBlock[]>([]);
  $effect(() => {
    blocks = details.dated_blocks.map((b) => ({ ...b, entries: b.entries.map((e) => ({ ...e })) }));
  });

  function isPostStart(d: string): boolean {
    return compareDates(d, "1444.11.11") > 0;
  }

  // --- Advisors -------------------------------------------------------------
  interface AdvisorRow {
    block: CountryDatedBlock;
    entryIndex: number;
    view: ReturnType<typeof advisorView>;
  }
  const advisors = $derived<AdvisorRow[]>(
    blocks.flatMap((block) =>
      block.entries
        .map((e, entryIndex) => ({ e, entryIndex }))
        .filter(({ e }) => e.key === "advisor" && e.is_block)
        .map(({ e, entryIndex }) => ({ block, entryIndex, view: advisorView(block, e.value) })),
    ),
  );

  function advisorPath(v: AdvisorRow["view"]): string[] {
    return [datedBlockSegment(v.date, v.occurrenceIndex), "advisor"];
  }

  function advNameVal(r: AdvisorRow): string {
    return charValue(queue, file, advisorPath(r.view), "name", r.view.name) ?? "";
  }
  function advTypeVal(r: AdvisorRow): string | null {
    return charValue(queue, file, advisorPath(r.view), "type", r.view.type);
  }
  function advSkillVal(r: AdvisorRow): number {
    const v = charValue(queue, file, advisorPath(r.view), "skill", r.view.skill != null ? String(r.view.skill) : null);
    return v != null ? parseInt(v, 10) || 0 : 1;
  }

  function setAdvisor(r: AdvisorRow, key: string, value: string, present: boolean, quoted = false) {
    queue.push({
      label: `Edit advisor ${key} (${r.view.date}) of ${tag}`,
      edits: [setCharEdit(file, advisorPath(r.view), key, value, present, quoted)],
      ...(isPostStart(r.view.date) ? { date: r.view.date } : {}),
    });
  }

  function deleteAdvisor(r: AdvisorRow) {
    const seg = datedBlockSegment(r.view.date, r.view.occurrenceIndex);
    queue.push({
      label: `Delete advisor (${r.view.date}) of ${tag}`,
      edits: [{ kind: "removeStatement", file, blockPath: [seg], key: "advisor", value: null }],
      ...(isPostStart(r.view.date) ? { date: r.view.date } : {}),
    });
    // Optimistically drop the entry (and the whole block if it was its only one).
    blocks = blocks
      .map((b) =>
        b === r.block ? { ...b, entries: b.entries.filter((_, i) => i !== r.entryIndex) } : b,
      )
      .filter((b) => b.entries.length > 0);
  }

  // Add-advisor form.
  let adding = $state(false);
  let newName = $state("");
  let newType = $state<string | null>(null);
  let newSkill = $state(1);
  let newDate = $state("1444.11.11");
  let newDeath = $state("");
  function beginAdd() {
    adding = true;
    newName = "";
    newType = advisorTypes[0]?.key ?? null;
    newSkill = 1;
    newDate = date ?? startDate;
    newDeath = "";
  }
  function submitAdvisor() {
    const name = newName.trim();
    if (!name || !newType) return;
    const body = advisorBody({
      name,
      type: newType,
      skill: newSkill,
      date: newDate,
      deathDate: newDeath.trim() || undefined,
    });
    queue.push({
      label: `Add advisor "${name}" (${newDate}) to ${tag}`,
      edits: addAdvisorEdits(file, newDate, blocks, body),
      ...(isPostStart(newDate) ? { date: newDate } : {}),
    });
    // Optimistically fold the new advisor block in (value = the `{ … }` part).
    const entry = { key: "advisor", value: body.slice(body.indexOf("{")), is_block: true };
    const targets = blocks.map((b, i) => ({ b, i })).filter((x) => x.b.date === newDate);
    if (targets.length > 0) {
      const ti = targets[targets.length - 1].i;
      blocks = blocks.map((b, j) => (j === ti ? { ...b, entries: [...b.entries, entry] } : b));
    } else {
      blocks = [...blocks, { date: newDate, post_start: isPostStart(newDate), occurrence_index: 0, entries: [entry] }];
    }
    adding = false;
  }

  // --- Rulers over time (reuse CharacterCore) ------------------------------
  interface HolderRow {
    block: CountryDatedBlock;
    holder: HolderKey;
    char: ReturnType<typeof holderChar>;
  }
  const holders = $derived<HolderRow[]>(
    blocks.flatMap((block) =>
      block.entries
        .filter((e) => e.is_block && (HOLDER_KEYS as readonly string[]).includes(e.key))
        .map((e) => ({ block, holder: e.key as HolderKey, char: holderChar(block, e.key as HolderKey, e.value) })),
    ),
  );
  let openHolder = $state<string | null>(null);
  function holderKey(r: HolderRow): string {
    return `${r.char.date}:${r.holder}`;
  }

  // --- Generic timeline (all dated blocks) ----------------------------------
  const timelineBlocks = $derived<TimelineBlock[]>(
    blocks.map((b) => ({
      date: b.date,
      postStart: b.post_start,
      occurrenceIndex: b.occurrence_index,
      entries: b.entries.map((e) => ({ key: e.key, value: e.value, isBlock: e.is_block })),
    })),
  );

  function timelineLabel(intent: TimelineIntent): string {
    if (intent.kind === "addEntry") return `Add ${intent.date} entry to ${tag}`;
    if (intent.kind === "deleteEntry") return `Delete ${intent.key} from ${intent.date} (${tag})`;
    return `Edit ${intent.date} entry of ${tag}`;
  }

  function foldIntoBlocks(intent: TimelineIntent) {
    if (intent.kind === "addEntry") {
      const occ = blocks.filter((b) => b.date === intent.date).length;
      blocks = [
        ...blocks,
        {
          date: intent.date,
          post_start: isPostStart(intent.date),
          occurrence_index: occ,
          entries: [{ key: intent.key, value: intent.value, is_block: false }],
        },
      ];
      return;
    }
    const i = blocks.findIndex((b) => b.date === intent.date && b.occurrence_index === intent.occurrenceIndex);
    if (i < 0) return;
    const nb = blocks.map((b, j) => (j === i ? { ...b, entries: [...b.entries] } : b));
    const blk = nb[i];
    if (intent.kind === "editValue") blk.entries[intent.entryIndex] = { ...blk.entries[intent.entryIndex], value: intent.value };
    else if (intent.kind === "editEntry") blk.entries[intent.entryIndex] = { key: intent.key, value: intent.value, is_block: false };
    else if (intent.kind === "deleteEntry") blk.entries = blk.entries.filter((_, k) => k !== intent.entryIndex);
    blocks = nb;
  }

  function onTimeline(intent: TimelineIntent) {
    queue.push({
      label: timelineLabel(intent),
      edits: intentToEdits(file, intent),
      ...(isPostStart(intent.date) ? { date: intent.date } : {}),
    });
    foldIntoBlocks(intent);
  }
</script>

<section>
  <h3>History</h3>

  <!-- Historical advisors (typed) -->
  <div class="sub">
    <div class="sub-head">
      <span>Historical Advisors</span>
      {#if !adding}
        <button class="mini" onclick={beginAdd}>+ Add</button>
      {/if}
    </div>

    {#if adding}
      <div class="add-form">
        <input class="text" bind:value={newName} placeholder="Advisor name" />
        <div class="frow">
          <span class="lbl">Type</span>
          <SearchDropdown items={advisorTypes} value={newType} placeholder="Advisor type…" onselect={(k) => (newType = k)} />
          {#if onopenmechanics}
            {#if newType}
              <button class="mini" title="Edit this advisor type's definition" onclick={() => onopenmechanics?.("advisortypes", newType!)}>✎</button>
            {/if}
            <button class="mini" title="Create a new advisor type definition" onclick={() => onopenmechanics?.("advisortypes")}>＋ new…</button>
          {/if}
        </div>
        <div class="frow">
          <span class="lbl">Skill</span>
          <Stepper value={newSkill} min={1} max={5} onchange={(v) => (newSkill = v)} />
        </div>
        <div class="frow">
          <span class="lbl">Date</span>
          <DatePicker bind:value={newDate} min="1.1.1" max="9999.1.1" />
        </div>
        <div class="frow">
          <span class="lbl">Death (optional)</span>
          <DatePicker bind:value={newDeath} min="1.1.1" max="9999.1.1" />
        </div>
        <div class="add-actions">
          <button class="btn ok" onclick={submitAdvisor} disabled={!newName.trim() || !newType}>Add advisor</button>
          <button class="btn" onclick={() => (adding = false)}>Cancel</button>
        </div>
      </div>
    {/if}

    {#if advisors.length === 0 && !adding}
      <p class="empty">No historical advisors defined.</p>
    {/if}

    {#each advisors as r (`${r.view.date}#${r.view.occurrenceIndex}:${r.entryIndex}`)}
      <div class="advisor">
        <div class="advisor-head">
          <span class="date">{r.view.date}</span>
          {#if r.block.post_start}<span class="badge post">post-start</span>{/if}
          <button class="mini danger" title="Delete advisor" onclick={() => deleteAdvisor(r)}>🗑</button>
        </div>
        <div class="frow">
          <span class="lbl">Name</span>
          <input class="text" value={advNameVal(r)} onchange={(e) => setAdvisor(r, "name", e.currentTarget.value, r.view.present.has("name"), true)} />
        </div>
        <div class="frow">
          <span class="lbl">Type</span>
          <SearchDropdown
            items={advisorTypes}
            value={advTypeVal(r)}
            placeholder="Advisor type…"
            onselect={(k) => setAdvisor(r, "type", k, r.view.present.has("type"))}
          />
        </div>
        <div class="frow">
          <span class="lbl">Skill</span>
          <Stepper value={advSkillVal(r)} min={1} max={5} onchange={(v) => setAdvisor(r, "skill", String(v), r.view.present.has("skill"))} />
        </div>
      </div>
    {/each}
  </div>

  <!-- Rulers over time (reuse the existing ruler editor fields) -->
  {#if holders.length > 0}
    <div class="sub">
      <div class="sub-head"><span>Rulers over time</span></div>
      {#each holders as r (holderKey(r))}
        <div class="holder">
          <button
            class="holder-head"
            onclick={() => (openHolder = openHolder === holderKey(r) ? null : holderKey(r))}
          >
            <span class="chev">{openHolder === holderKey(r) ? "▾" : "▸"}</span>
            <span class="date">{r.char.date.split("#")[0]}</span>
            <span class="role">{r.holder}</span>
            <span class="who">{r.char.name ?? "(unnamed)"}</span>
            {#if r.block.post_start}<span class="badge post">post-start</span>{/if}
          </button>
          {#if openHolder === holderKey(r)}
            <div class="holder-body">
              <CharacterCore
                {installPath}
                {modPath}
                {tag}
                {queue}
                {file}
                holder={r.holder}
                label={r.holder === "monarch" ? "ruler" : r.holder}
                personalityEffect={personalityEffectFor(r.holder)}
                char={r.char}
                {cultures}
                {religions}
                {personalityItems}
              />
            </div>
          {/if}
        </div>
      {/each}
    </div>
  {/if}

  <!-- Full chronological timeline (reusable 2.3 component) -->
  <div class="sub">
    <div class="sub-head"><span>All dated history</span></div>
    <Timeline blocks={timelineBlocks} {calendar} {startDate} onchange={onTimeline} anchorLabel="Base state" />
  </div>
</section>

<style>
  section {
    margin-bottom: 1rem;
  }
  h3 {
    margin: 0 0 0.5rem;
    font-size: 0.8rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: #9ca3af;
  }
  .sub {
    margin-bottom: 0.8rem;
  }
  .sub-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: 0.72rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: #8a919c;
    margin-bottom: 0.35rem;
    border-bottom: 1px solid #1f242c;
    padding-bottom: 0.2rem;
  }
  .empty {
    margin: 0.2rem 0;
    color: #8a919c;
    font-style: italic;
    font-size: 0.8rem;
  }
  .advisor,
  .holder {
    border: 1px solid #1f242c;
    background: #21262e;
    padding: 0.4rem 0.5rem;
    margin-bottom: 0.35rem;
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
  }
  .advisor-head {
    display: flex;
    align-items: center;
    gap: 0.4rem;
  }
  .date {
    font-variant-numeric: tabular-nums;
    font-weight: 600;
    color: #dfe4ea;
    font-size: 0.82rem;
  }
  .badge.post {
    background: #a1662f;
    color: #fff;
    font-size: 0.62rem;
    padding: 0.05rem 0.3rem;
    text-transform: uppercase;
  }
  .frow {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
  }
  .lbl {
    font-size: 0.68rem;
    text-transform: uppercase;
    letter-spacing: 0.03em;
    color: #8a919c;
  }
  .text {
    background: #191d23;
    border: 1px solid #1f242c;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.85rem;
    padding: 0.2rem 0.4rem;
    outline: none;
  }
  .add-form {
    border: 1px solid #1f242c;
    background: #21262e;
    padding: 0.5rem;
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    margin-bottom: 0.4rem;
  }
  .add-actions {
    display: flex;
    gap: 0.35rem;
  }
  .btn {
    border: 1px solid #1f242c;
    background: #3f4855;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.8rem;
    padding: 0.25rem 0.6rem;
    cursor: pointer;
  }
  .btn:hover {
    background: #4a6da7;
    color: #fff;
  }
  .btn.ok {
    background: #3a5a86;
  }
  .btn:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .mini {
    border: 1px solid #1f242c;
    background: #2b323d;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.76rem;
    line-height: 1;
    padding: 0.12rem 0.4rem;
    cursor: pointer;
  }
  .mini:hover {
    background: #4a6da7;
    color: #fff;
  }
  .mini.danger {
    margin-left: auto;
  }
  .mini.danger:hover {
    background: #7a3f3f;
  }
  .holder-head {
    display: flex;
    align-items: center;
    gap: 0.45rem;
    width: 100%;
    border: none;
    background: transparent;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.82rem;
    cursor: pointer;
    text-align: left;
    padding: 0;
  }
  .holder-head:hover {
    color: #fff;
  }
  .chev {
    color: #9ca3af;
  }
  .role {
    font-size: 0.66rem;
    text-transform: uppercase;
    background: #303844;
    color: #9ab0d0;
    padding: 0.03rem 0.35rem;
  }
  .who {
    color: #cfd4db;
  }
  .holder-body {
    margin-top: 0.4rem;
    padding-top: 0.4rem;
    border-top: 1px solid #1f242c;
  }
</style>
