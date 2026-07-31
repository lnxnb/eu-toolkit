<!--
  DiplomacyTab — the country panel's Diplomacy tab (Sprint 3.3/3.4). Fixed-height
  scrollable ListSections for Overlord / Subjects / Alliances / Royal marriages /
  Guarantees (given+received) / Warnings (given+received) / Rivals & friends. A
  "show all history" toggle switches between active-at-1444 and every relation. An
  add-relation flow (type + partner + dates) queues an appendText; date edits and
  deletes are byte-surgical on the specific block. A ValidationStrip fed by the
  "diplomacy" domain sits on top; its country jumps reuse the panel's open-country
  callback. Displayed relations are derived from the backend payload folded with
  the pending queue, so both partners' tabs stay in sync (see ./diplomacy.ts).
-->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { ListSection, SearchDropdown, DatePicker } from "$lib/components/ui";
  import type { DropdownItem } from "$lib/components/ui";
  import ValidationStrip from "$lib/components/ValidationStrip.svelte";
  import type { ValidationIssue, JumpTarget } from "$lib/components/ValidationStrip.svelte";
  import type { EditQueue } from "$lib/edits.svelte";
  import type { Calendar } from "$lib/calendar";
  import DiplomacyRow from "./DiplomacyRow.svelte";
  import WarsSection from "./WarsSection.svelte";
  import type { Relation } from "./diplomacy";
  import {
    projectRelations,
    addRelationEdit,
    dateEdit,
    deleteRelationEdit,
    computeActive,
    activeSubjects,
  } from "./diplomacy";
  import type { RegistryEntry } from "./types";

  let {
    installPath,
    modPath,
    tag,
    queue,
    calendar = null,
    date = null,
    startDate = "1444.11.11",
    countries,
    historicalRivals,
    historicalFriends,
    onopencountry,
    onopenprovince,
    onopenmechanics,
  }: {
    installPath: string;
    modPath: string | null;
    tag: string;
    queue: EditQueue;
    /** The mod's calendar (Sprint 12.4) so date ranges show custom months. */
    calendar?: Calendar | null;
    /** View date (Sprint 12.2); null = effective start. Re-filters active-at-date. */
    date?: string | null;
    /** The mod's effective start date (Sprint 13.2: default join-date for wars). */
    startDate?: string;
    countries: DropdownItem[];
    historicalRivals: string[];
    historicalFriends: string[];
    /** Jump to another country, keeping the Diplomacy tab open (from MapView). */
    onopencountry?: (tag: string) => void;
    /** Jump to a province (war validation jump links). */
    onopenprovince?: (id: number) => void;
    /** Open the Mechanics editor at a family (+ key) — CB / war-goal defs. */
    onopenmechanics?: (family: string, key?: string) => void;
  } = $props();

  let backend = $state<Relation[]>([]);
  let issues = $state<ValidationIssue[]>([]);
  let subjectTypes = $state<{ key: string; label: string }[]>([]);
  let showAll = $state(false);

  // Backend relations for this country (re-fetched on tag/session change).
  $effect(() => {
    const cur = tag;
    const at = date;
    backend = [];
    invoke<Relation[]>("get_diplomacy", { installPath, modPath, tag: cur, date: at })
      .then((r) => {
        if (cur === tag) backend = r;
      })
      .catch(() => {});
  });

  // Diplomacy validation (whole-game; reflects saved state — refreshes on save).
  $effect(() => {
    installPath;
    modPath;
    const at = date;
    queue.version; // re-validate after a save clears the queue
    invoke<ValidationIssue[]>("validate", { domain: "diplomacy", installPath, modPath, date: at })
      .then((v) => (issues = v))
      .catch(() => (issues = []));
  });

  $effect(() => {
    invoke<RegistryEntry[]>("get_registry", { name: "subject_types", installPath, modPath })
      .then((v) => (subjectTypes = v.filter((e) => e.key !== "default").map((e) => ({ key: e.key, label: e.name }))))
      .catch(() => {});
  });

  // Displayed relations = backend folded with the pending queue (tracks queue.version).
  const relations = $derived.by(() => {
    queue.version;
    return projectRelations(tag, backend, queue.serialize());
  });
  const visible = $derived(showAll ? relations : relations.filter((r) => r.active_at_start));

  function ofType(type: string, pred: (r: Relation) => boolean): Relation[] {
    return visible.filter((r) => r.relation_type === type && pred(r));
  }
  const overlord = $derived(ofType("dependency", (r) => r.second === tag));
  const subjects = $derived(ofType("dependency", (r) => r.first === tag));
  const alliances = $derived(ofType("alliance", (r) => r.first === tag || r.second === tag));
  const marriages = $derived(ofType("royal_marriage", (r) => r.first === tag || r.second === tag));
  const guaranteesGiven = $derived(ofType("guarantee", (r) => r.first === tag));
  const guaranteesReceived = $derived(ofType("guarantee", (r) => r.second === tag));
  const warningsGiven = $derived(ofType("warning", (r) => r.first === tag));
  const warningsReceived = $derived(ofType("warning", (r) => r.second === tag));

  function partnerOf(r: Relation): string {
    return r.first === tag ? (r.second ?? "?") : (r.first ?? "?");
  }
  function subjectLabel(key: string | null): string {
    if (!key) return "subject";
    return subjectTypes.find((s) => s.key === key)?.label ?? key;
  }

  // --- per-row edit/delete -------------------------------------------------

  function relLabel(r: Relation): string {
    return `${r.relation_type}${r.subject_type ? ` (${r.subject_type})` : ""} ${r.first}-${r.second}`;
  }
  function editDate(r: Relation, which: "start" | "end", value: string) {
    if ((which === "start" ? r.start_date : r.end_date) === value) return;
    queue.push({ label: `Edit ${which} date of ${relLabel(r)}`, edits: [dateEdit(r, which, value)] });
  }
  function deleteRel(r: Relation) {
    queue.push({ label: `Delete ${relLabel(r)}`, edits: [deleteRelationEdit(r)] });
  }
  function jump(t: string) {
    onopencountry?.(t);
  }

  // --- add-relation flow ---------------------------------------------------

  let adding = $state(false);
  let addType = $state("alliance");
  let addPartner = $state<string | null>(null);
  let addStart = $state("1444.11.11");
  let addEnd = $state("9999.1.1");

  const partnerItems = $derived(countries.filter((c) => c.key !== tag));

  // Parse the type selection into (relation_type, subject_type).
  const parsedType = $derived.by<{ relation_type: string; subject_type: string | null }>(() => {
    if (addType.startsWith("dep:")) return { relation_type: "dependency", subject_type: addType.slice(4) };
    return { relation_type: addType, subject_type: null };
  });

  // Client feedback (3.4): block self + exact duplicate; warn on inactive / subject-alliance.
  const subjectsNow = $derived(activeSubjects(relations));
  const blockError = $derived.by<string | null>(() => {
    if (!addPartner) return null;
    if (addPartner === tag) return "A country cannot have a relation with itself.";
    const { relation_type, subject_type } = parsedType;
    const symmetric = relation_type === "alliance" || relation_type === "royal_marriage";
    const dup = relations.find(
      (r) =>
        r.active_at_start &&
        r.relation_type === relation_type &&
        (r.subject_type ?? null) === subject_type &&
        (symmetric
          ? (r.first === tag && r.second === addPartner) || (r.first === addPartner && r.second === tag)
          : r.first === tag && r.second === addPartner),
    );
    return dup ? "An active relation of this type between these countries already exists." : null;
  });
  const addWarnings = $derived.by<string[]>(() => {
    if (!addPartner || blockError) return [];
    const out: string[] = [];
    if (!computeActive(addStart, addEnd)) out.push("This relation is not active at 1444.11.11 (it will show as expired/future).");
    if (parsedType.relation_type === "alliance" && (subjectsNow.has(tag) || subjectsNow.has(addPartner)))
      out.push("One side is a subject — the game ignores alliances involving subjects.");
    return out;
  });

  const typeItems = $derived([
    { key: "alliance", label: "Alliance" },
    { key: "royal_marriage", label: "Royal marriage" },
    { key: "guarantee", label: "Guarantee (you guarantee them)" },
    { key: "warning", label: "Warning (you warn them)" },
    ...subjectTypes.map((s) => ({ key: `dep:${s.key}`, label: `Subject: ${s.label}` })),
  ]);

  function directionNote(): string {
    const t = parsedType.relation_type;
    if (t === "dependency") return "You are the overlord; the picked country becomes your subject.";
    if (t === "guarantee") return "You are the guarantor.";
    if (t === "warning") return "You issue the warning.";
    return "";
  }

  function resetAdd() {
    adding = false;
    addPartner = null;
    addType = "alliance";
    addStart = "1444.11.11";
    addEnd = "9999.1.1";
  }
  function confirmAdd() {
    if (!addPartner || blockError) return;
    const { relation_type, subject_type } = parsedType;
    queue.push({
      label: `Add ${relation_type} ${tag}-${addPartner}`,
      edits: [addRelationEdit({ relation_type, subject_type, first: tag, second: addPartner, start_date: addStart, end_date: addEnd })],
    });
    resetAdd();
  }

  function onValidationJump(t: JumpTarget) {
    if (t.kind === "country") onopencountry?.(t.id);
  }
</script>

<div class="diplo">
  <ValidationStrip {issues} onjump={onValidationJump} title="Diplomacy checks" collapsed={issues.length === 0} />

  <div class="toolbar">
    <button class="add" onclick={() => (adding = !adding)}>{adding ? "Cancel" : "+ Add relation"}</button>
    <label class="toggle">
      <input type="checkbox" bind:checked={showAll} />
      Show all history
    </label>
  </div>

  {#if adding}
    <div class="add-form">
      <label class="field">
        <span>Type</span>
        <select bind:value={addType}>
          {#each typeItems as t}
            <option value={t.key}>{t.label}</option>
          {/each}
        </select>
      </label>
      <label class="field">
        <span>Partner</span>
        <SearchDropdown items={partnerItems} bind:value={addPartner} placeholder="Pick a country…" />
      </label>
      <div class="dates-row">
        <label class="field"><span>Start</span><DatePicker bind:value={addStart} /></label>
        <label class="field"><span>End</span><DatePicker bind:value={addEnd} /></label>
      </div>
      {#if directionNote()}<p class="note">{directionNote()}</p>{/if}
      {#if blockError}<p class="err">{blockError}</p>{/if}
      {#each addWarnings as w}<p class="warn">⚠ {w}</p>{/each}
      <div class="actions">
        <button class="confirm" disabled={!addPartner || !!blockError} onclick={confirmAdd}>Add</button>
        <button class="cancel" onclick={resetAdd}>Cancel</button>
      </div>
    </div>
  {/if}

  <WarsSection
    {installPath}
    {modPath}
    {tag}
    {queue}
    {calendar}
    {date}
    {startDate}
    {countries}
    {onopencountry}
    onjumpprovince={onopenprovince}
    {onopenmechanics}
  />

  {#snippet section(title: string, rows: Relation[], sub: (r: Relation) => string | null)}
    <ListSection {title} count={rows.length}>
      {#if rows.length === 0}
        <p class="empty">None</p>
      {:else}
        {#each rows as r (relKey(r))}
          <DiplomacyRow
            {installPath}
            {modPath}
            {calendar}
            relation={r}
            partnerTag={partnerOf(r)}
            subLabel={sub(r)}
            {countries}
            onopenpartner={jump}
            ondateedit={(which, value) => editDate(r, which, value)}
            ondelete={() => deleteRel(r)}
          />
        {/each}
      {/if}
    </ListSection>
  {/snippet}

  {#if overlord.length > 0}
    {@render section("Overlord", overlord, (r) => subjectLabel(r.subject_type))}
  {/if}
  {@render section("Subjects", subjects, (r) => subjectLabel(r.subject_type))}
  {@render section("Alliances", alliances, () => null)}
  {@render section("Royal marriages", marriages, () => null)}
  {@render section("Guarantees given", guaranteesGiven, () => null)}
  {#if guaranteesReceived.length > 0}
    {@render section("Guarantees received", guaranteesReceived, () => null)}
  {/if}
  {@render section("Warnings given", warningsGiven, () => null)}
  {#if warningsReceived.length > 0}
    {@render section("Warnings received", warningsReceived, () => null)}
  {/if}

  <ListSection title="Rivals & friends" count={historicalRivals.length + historicalFriends.length}>
    {#if historicalRivals.length + historicalFriends.length === 0}
      <p class="empty">None (edit on the Country tab)</p>
    {:else}
      {#each historicalRivals as rt}
        <button class="rf" onclick={() => jump(rt)}>
          <span class="rf-badge rival">rival</span>
          {countries.find((c) => c.key === rt)?.label ?? rt} ↗
        </button>
      {/each}
      {#each historicalFriends as ft}
        <button class="rf" onclick={() => jump(ft)}>
          <span class="rf-badge friend">friend</span>
          {countries.find((c) => c.key === ft)?.label ?? ft} ↗
        </button>
      {/each}
    {/if}
  </ListSection>
</div>

<script module lang="ts">
  import type { Relation as Rel } from "./diplomacy";
  // Stable key for a relation row across pending/backend derivations.
  function relKey(r: Rel): string {
    return r.pending
      ? `pend ${r.block_key} ${r.first} ${r.second} ${r.start_date} ${r.end_date}`
      : `${r.file} ${r.block_key} ${r.block_index}`;
  }
</script>

<style>
  .diplo {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  .toolbar {
    display: flex;
    align-items: center;
    gap: 0.6rem;
  }
  .add {
    border: 1px solid var(--border);
    background: var(--bg-3);
    color: var(--text-1);
    font-family: inherit;
    font-size: 0.8rem;
    padding: 0.25rem 0.6rem;
    cursor: pointer;
  }
  .add:hover {
    background: var(--accent);
    color: var(--text-inverse);
  }
  .toggle {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    font-size: 0.78rem;
    color: var(--text-2);
    cursor: pointer;
  }
  .add-form {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    border: 1px solid var(--border);
    background: var(--bg-2);
    padding: 0.5rem;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
  }
  .field > span {
    font-size: 0.68rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-2);
  }
  .field select {
    background: var(--bg-1);
    border: 1px solid var(--border);
    color: var(--text-1);
    font-family: inherit;
    font-size: 0.83rem;
    padding: 0.25rem 0.3rem;
    outline: none;
  }
  .dates-row {
    display: flex;
    gap: 0.6rem;
    flex-wrap: wrap;
  }
  .note {
    margin: 0;
    font-size: 0.72rem;
    color: var(--text-2);
  }
  .err {
    margin: 0;
    font-size: 0.75rem;
    color: var(--err);
  }
  .warn {
    margin: 0;
    font-size: 0.75rem;
    color: var(--warn);
  }
  .actions {
    display: flex;
    gap: 0.4rem;
  }
  .confirm,
  .cancel {
    border: 1px solid var(--border);
    background: var(--bg-3);
    color: var(--text-1);
    font-family: inherit;
    font-size: 0.8rem;
    padding: 0.25rem 0.7rem;
    cursor: pointer;
  }
  .confirm:hover:not(:disabled) {
    background: var(--accent);
    color: var(--text-inverse);
  }
  .confirm:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .cancel:hover {
    background: var(--accent);
    color: var(--text-inverse);
  }
  .empty {
    margin: 0;
    padding: 0.25rem 0.3rem;
    font-size: 0.76rem;
    color: var(--text-2);
  }
  .rf {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    width: 100%;
    border: none;
    border-bottom: 1px solid var(--border);
    background: transparent;
    color: var(--text-1);
    font-family: inherit;
    font-size: 0.82rem;
    padding: 0.2rem 0.15rem;
    cursor: pointer;
    text-align: left;
  }
  .rf:last-child {
    border-bottom: none;
  }
  .rf:hover {
    color: var(--text-inverse);
  }
  .rf-badge {
    font-size: 0.62rem;
    padding: 0.02rem 0.3rem;
    color: var(--text-inverse);
    text-transform: uppercase;
  }
  .rf-badge.rival {
    background: var(--err);
  }
  .rf-badge.friend {
    background: var(--ok);
  }
</style>
