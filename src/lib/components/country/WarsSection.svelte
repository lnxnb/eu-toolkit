<!--
  WarsSection — the Diplomacy tab's Wars block (Sprint 13.2). Lists the wars the
  selected country is in at the selected date (fixed-height ListSection, like the
  relations sections), with a "+ New war" flow, a "show all history" toggle, a
  wars-domain validation strip, and an inline War panel for the selected war.
  Displayed wars = backend payload folded with the pending queue (wars.ts
  projectWars) so scaffolds/renames/participant edits show before save; re-fetched
  on tag / date / queue.version change.
-->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { ListSection, SearchDropdown, DatePicker } from "$lib/components/ui";
  import type { DropdownItem } from "$lib/components/ui";
  import ValidationStrip from "$lib/components/ValidationStrip.svelte";
  import type { ValidationIssue, JumpTarget } from "$lib/components/ValidationStrip.svelte";
  import type { EditQueue } from "$lib/edits.svelte";
  import type { Calendar } from "$lib/calendar";
  import WarRow from "./WarRow.svelte";
  import WarPanel from "./WarPanel.svelte";
  import type { War, TargetKind, WargoalRegistryEntry } from "./wars";
  import {
    projectWars,
    targetKindOf,
    engineTypeOf,
    newWarEdit,
    newWarFile,
    type NewWarSpec,
  } from "./wars";

  let {
    installPath,
    modPath,
    tag,
    queue,
    calendar = null,
    date = null,
    startDate,
    countries,
    onopencountry,
    onjumpprovince,
    onopenmechanics,
  }: {
    installPath: string;
    modPath: string | null;
    tag: string;
    queue: EditQueue;
    calendar?: Calendar | null;
    date?: string | null;
    startDate: string;
    countries: DropdownItem[];
    onopencountry?: (tag: string) => void;
    onjumpprovince?: (id: number) => void;
    /** Jump into the Mechanics editor (Sprint 27 W2) at a family (+ optional key)
     *  — the CB / war-goal definition editor is reachable from the new-war form. */
    onopenmechanics?: (family: string, key?: string) => void;
  } = $props();

  const at = $derived(date ?? startDate);

  let backend = $state<War[]>([]);
  let issues = $state<ValidationIssue[]>([]);
  let showAll = $state(false);
  let selectedFile = $state<string | null>(null);

  // Backend wars for this country (re-fetched on tag/date change).
  $effect(() => {
    const cur = tag;
    const d = date;
    backend = [];
    invoke<War[]>("get_wars", { installPath, modPath, date: d, tag: cur })
      .then((r) => {
        if (cur === tag) backend = r;
      })
      .catch(() => {});
  });

  // Wars validation (whole-game; refreshes on save via queue.version).
  $effect(() => {
    installPath;
    modPath;
    const d = date;
    queue.version;
    invoke<ValidationIssue[]>("validate", { domain: "wars", installPath, modPath, date: d })
      .then((v) => (issues = v))
      .catch(() => (issues = []));
  });

  // Displayed wars = backend folded with the pending queue (tracks queue.version).
  const wars = $derived.by(() => {
    queue.version;
    return projectWars(tag, backend, queue.serialize(), at);
  });
  const visible = $derived(showAll ? wars : wars.filter((w) => w.active_at_date));
  const selectedWar = $derived<War | null>(
    selectedFile ? (wars.find((w) => w.file === selectedFile) ?? null) : null,
  );

  function onValidationJump(t: JumpTarget) {
    if (t.kind === "country") onopencountry?.(t.id);
    else if (t.kind === "province") onjumpprovince?.(t.id);
  }

  // --- new-war flow -------------------------------------------------------
  let adding = $state(false);
  let nwName = $state("New War");
  let nwAttacker = $state<string | null>(null);
  let nwDefender = $state<string | null>(null);
  let nwStart = $state("1444.11.11");
  let nwGoalType = $state<string | null>(null);
  let nwCb = $state<string | null>(null);
  let nwProvince = $state("");
  let nwTag = $state<string | null>(null);

  let wargoalTypes = $state<WargoalRegistryEntry[]>([]);
  let cbTypes = $state<{ key: string; label: string }[]>([]);
  $effect(() => {
    invoke<WargoalRegistryEntry[]>("get_registry", { name: "wargoal_types", installPath, modPath })
      .then((v) => (wargoalTypes = v))
      .catch(() => (wargoalTypes = []));
  });
  $effect(() => {
    invoke<{ key: string; name: string }[]>("get_registry", { name: "cb_types", installPath, modPath })
      .then((v) => (cbTypes = v.map((e) => ({ key: e.key, label: e.name }))))
      .catch(() => (cbTypes = []));
  });
  const goalTypeItems = $derived(wargoalTypes.map((e) => ({ key: e.key, label: e.name })));
  const nwKind = $derived.by<TargetKind>(() => {
    const entry = wargoalTypes.find((e) => e.key === nwGoalType);
    return targetKindOf(entry ? engineTypeOf(entry) : nwGoalType);
  });

  // Default the attacker to the selected country (its war, most likely).
  $effect(() => {
    if (adding && nwAttacker == null) nwAttacker = tag;
  });

  const nwError = $derived.by<string | null>(() => {
    if (!nwName.trim()) return "Name is required.";
    if (!nwAttacker || !nwDefender) return "Pick an attacker and a defender.";
    if (nwAttacker === nwDefender) return "Attacker and defender must differ.";
    if (!nwGoalType) return "Pick a war goal type.";
    if (!nwCb) return "Pick a casus belli.";
    return null;
  });

  function resetNew() {
    adding = false;
    nwName = "New War";
    nwAttacker = null;
    nwDefender = null;
    nwStart = startDate;
    nwGoalType = null;
    nwCb = null;
    nwProvince = "";
    nwTag = null;
  }
  function confirmNew() {
    if (nwError) return;
    const spec: NewWarSpec = {
      name: nwName.trim(),
      attacker: nwAttacker!,
      defender: nwDefender!,
      startDate: nwStart,
      goalType: nwGoalType!,
      casusBelli: nwCb!,
      targetKind: nwKind,
      targetProvince: nwKind === "province" ? parseInt(nwProvince, 10) || null : null,
      targetTag: nwKind === "tag" ? nwTag : null,
    };
    const file = newWarFile(spec.name);
    queue.push({ label: `New war "${spec.name}"`, edits: [newWarEdit(spec)] });
    selectedFile = file;
    resetNew();
  }
</script>

<div class="wars">
  <ValidationStrip {issues} onjump={onValidationJump} title="War checks" collapsed={issues.length === 0} />

  <div class="bar">
    <button class="add" onclick={() => (adding ? resetNew() : ((nwStart = startDate), (adding = true)))}>{adding ? "Cancel" : "+ New war"}</button>
    <label class="toggle"><input type="checkbox" bind:checked={showAll} /> Show all history</label>
  </div>

  {#if adding}
    <div class="new-form">
      <label class="field"><span>Name</span><input bind:value={nwName} /></label>
      <div class="row2">
        <label class="field"><span>Attacker</span><SearchDropdown items={countries} bind:value={nwAttacker} placeholder="Pick…" /></label>
        <label class="field"><span>Defender</span><SearchDropdown items={countries} bind:value={nwDefender} placeholder="Pick…" /></label>
      </div>
      <div class="row2">
        <label class="field"><span>War goal</span>
          <div class="defrow">
            <select bind:value={nwGoalType}>
              <option value={null} disabled selected>—</option>
              {#each goalTypeItems as t}<option value={t.key}>{t.label}</option>{/each}
            </select>
            {#if onopenmechanics}
              <button class="def" title="Edit this war goal type's definition" disabled={!nwGoalType} onclick={() => nwGoalType && onopenmechanics?.("wargoal_types", nwGoalType)}>✎</button>
              <button class="def" title="Create a new war goal type" onclick={() => onopenmechanics?.("wargoal_types")}>＋</button>
            {/if}
          </div>
        </label>
        <label class="field"><span>Casus belli</span>
          <div class="defrow">
            <select bind:value={nwCb}>
              <option value={null} disabled selected>—</option>
              {#each cbTypes as c}<option value={c.key}>{c.label}</option>{/each}
            </select>
            {#if onopenmechanics}
              <button class="def" title="Edit this casus belli's definition" disabled={!nwCb} onclick={() => nwCb && onopenmechanics?.("cb_types", nwCb)}>✎</button>
              <button class="def" title="Create a new casus belli type" onclick={() => onopenmechanics?.("cb_types")}>＋</button>
            {/if}
          </div>
        </label>
      </div>
      {#if nwKind === "province"}
        <label class="field"><span>Target province</span><input type="number" min="1" bind:value={nwProvince} /></label>
      {:else if nwKind === "tag"}
        <label class="field"><span>Target country</span><SearchDropdown items={countries} bind:value={nwTag} placeholder="Pick…" /></label>
      {/if}
      <label class="field"><span>Start date</span><DatePicker bind:value={nwStart} /></label>
      {#if nwError}<p class="err">{nwError}</p>{/if}
      <div class="actions">
        <button class="confirm" disabled={!!nwError} onclick={confirmNew}>Create war</button>
        <button class="cancel" onclick={resetNew}>Cancel</button>
      </div>
    </div>
  {/if}

  <ListSection title="Wars" count={visible.length}>
    {#if visible.length === 0}
      <p class="empty">None</p>
    {:else}
      {#each visible as w (w.file)}
        <WarRow
          {installPath}
          {modPath}
          war={w}
          {tag}
          {countries}
          {calendar}
          selected={w.file === selectedFile}
          onopen={() => (selectedFile = selectedFile === w.file ? null : w.file)}
        />
      {/each}
    {/if}
  </ListSection>

  {#if selectedWar}
    {#key selectedWar.file}
      <WarPanel
        {installPath}
        {modPath}
        war={selectedWar}
        {tag}
        {queue}
        {calendar}
        {countries}
        {startDate}
        onclose={() => (selectedFile = null)}
        ondeleted={() => (selectedFile = null)}
        {onopencountry}
      />
    {/key}
  {/if}
</div>

<style>
  .wars {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  .bar {
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
  .new-form {
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
  .field input,
  .field select {
    background: var(--bg-1);
    border: 1px solid var(--border);
    color: var(--text-1);
    font-family: inherit;
    font-size: 0.83rem;
    padding: 0.25rem 0.3rem;
    outline: none;
  }
  .row2 {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.5rem;
  }
  .defrow {
    display: flex;
    align-items: stretch;
    gap: 0.2rem;
  }
  .defrow select {
    flex: 1;
    min-width: 0;
  }
  .def {
    flex: none;
    border: 1px solid var(--border);
    background: var(--bg-2);
    color: var(--text-1);
    font-family: inherit;
    font-size: 0.78rem;
    padding: 0 0.35rem;
    cursor: pointer;
  }
  .def:hover:not(:disabled) {
    background: var(--accent);
    color: var(--text-inverse);
  }
  .def:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .err {
    margin: 0;
    font-size: 0.75rem;
    color: var(--err);
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
</style>
