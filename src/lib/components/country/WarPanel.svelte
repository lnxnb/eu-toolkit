<!--
  WarPanel — the inline war editor opened from the Diplomacy tab's Wars section
  (Sprint 13.2). Edits are byte-surgical TypedEdits pushed to the shared queue;
  the Wars list re-folds them live (see wars.ts projectWars). Covers:
    * name (literal quoted string → SetScalar),
    * war goal: type + casus_belli dropdowns (registries) + a target province /
      tag picker per the goal type's derived target kind,
    * two participant columns (attackers | defenders): flag + name + join/leave
      dates (DatePicker), add via country picker, remove = set a leave date,
    * battles: "N battles recorded" (read-only, never edited),
    * delete: toolkit wars → DeleteFile; base wars → confirm + empty shadow.
-->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { SearchDropdown, DatePicker } from "$lib/components/ui";
  import type { DropdownItem } from "$lib/components/ui";
  import { getFlagUrl } from "$lib/flagCache";
  import { formatDate, type Calendar } from "$lib/calendar";
  import type { EditQueue } from "$lib/edits.svelte";
  import type { War, Participant, TargetKind, WargoalRegistryEntry } from "./wars";
  import {
    ATTACKER,
    DEFENDER,
    targetKindOf,
    engineTypeOf,
    isToolkitWar,
    renameWarEdit,
    warGoalScalarEdit,
    warGoalTargetEdit,
    warGoalRemoveTargetEdit,
    addParticipantEdit,
    leaveParticipantEdit,
    removePartStatementEdit,
    deleteWarEdit,
    shadowDeleteBaseWarEdit,
  } from "./wars";

  let {
    installPath,
    modPath,
    war,
    tag,
    queue,
    calendar = null,
    countries,
    startDate,
    onclose,
    ondeleted,
    onopencountry,
  }: {
    installPath: string;
    modPath: string | null;
    war: War;
    tag: string;
    queue: EditQueue;
    calendar?: Calendar | null;
    countries: DropdownItem[];
    /** Effective start date; default join date for new participants. */
    startDate: string;
    onclose: () => void;
    ondeleted: () => void;
    onopencountry?: (tag: string) => void;
  } = $props();

  const warName = $derived(
    war.name ?? war.file.slice(war.file.lastIndexOf("/") + 1).replace(/\.txt$/i, ""),
  );

  // --- registries (war goal + casus belli) --------------------------------
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
  // The current goal type's derived target kind (province / tag / none).
  const kind = $derived.by<TargetKind>(() => {
    const gt = war.war_goal?.goal_type ?? null;
    const entry = wargoalTypes.find((e) => e.key === gt);
    return targetKindOf(entry ? engineTypeOf(entry) : gt);
  });

  // --- name edit ----------------------------------------------------------
  let nameDraft = $state("");
  $effect(() => {
    nameDraft = warName;
  });
  function commitName() {
    const v = nameDraft.trim();
    if (!v || v === warName) return;
    queue.push({ label: `Rename war to "${v}"`, edits: [renameWarEdit(war, v)] });
  }

  // --- war goal edits -----------------------------------------------------
  function setGoalType(v: string | null) {
    if (!v || v === war.war_goal?.goal_type) return;
    const present = war.war_goal?.goal_type != null;
    const edits = [warGoalScalarEdit(war, "type", v, present)];
    // If the target kind changes, drop the now-stale target key.
    const oldKind = kind;
    const entry = wargoalTypes.find((e) => e.key === v);
    const newKind = targetKindOf(entry ? engineTypeOf(entry) : v);
    if (oldKind === "province" && newKind !== "province" && war.war_goal?.province != null) {
      edits.push(warGoalRemoveTargetEdit(war, "province"));
    } else if (oldKind === "tag" && newKind !== "tag" && war.war_goal?.tag != null) {
      edits.push(warGoalRemoveTargetEdit(war, "tag"));
    }
    queue.push({ label: `Set war goal type ${v}`, edits });
  }
  function setCasusBelli(v: string | null) {
    if (!v || v === war.war_goal?.casus_belli) return;
    const present = war.war_goal?.casus_belli != null;
    queue.push({ label: `Set casus belli ${v}`, edits: [warGoalScalarEdit(war, "casus_belli", v, present)] });
  }
  let provinceDraft = $state("");
  $effect(() => {
    provinceDraft = war.war_goal?.province != null ? String(war.war_goal.province) : "";
  });
  function commitProvinceTarget() {
    const n = parseInt(provinceDraft.trim(), 10);
    if (!Number.isFinite(n) || n === war.war_goal?.province) return;
    const present = war.war_goal?.province != null;
    queue.push({ label: `Set war goal province ${n}`, edits: [warGoalTargetEdit(war, "province", String(n), present)] });
  }
  function setTagTarget(v: string | null) {
    if (!v || v === war.war_goal?.tag) return;
    const present = war.war_goal?.tag != null;
    queue.push({ label: `Set war goal target ${v}`, edits: [warGoalTargetEdit(war, "tag", v, present)] });
  }

  // --- participants -------------------------------------------------------
  const attackers = $derived(war.participants.filter((p) => p.side === ATTACKER));
  const defenders = $derived(war.participants.filter((p) => p.side === DEFENDER));

  function nameOf(t: string): string {
    return countries.find((c) => c.key === t)?.label ?? t;
  }
  function showDate(d: string | null): string {
    if (!d) return "—";
    return calendar ? formatDate(d, calendar) : d;
  }

  // Add-participant drafts, one per side.
  let addSide = $state<string | null>(null);
  let addTag = $state<string | null>(null);
  let addDate = $state("1444.11.11");
  function openAdd(side: string) {
    addSide = addSide === side ? null : side;
    addTag = null;
    addDate = startDate;
  }
  const addItems = $derived(
    countries.filter((c) => !war.participants.some((p) => p.tag === c.key)),
  );
  function confirmAdd() {
    if (!addSide || !addTag) return;
    queue.push({
      label: `Add ${addTag} to ${addSide}s`,
      edits: [addParticipantEdit(war, addSide, addTag, addDate)],
    });
    addSide = null;
    addTag = null;
  }

  function changeJoin(p: Participant, value: string) {
    if (!p.join_date || value === p.join_date) return;
    queue.push({
      label: `Change ${p.tag} join date`,
      edits: [
        removePartStatementEdit(war, p.side, p.tag, true, p.join_date),
        addParticipantEdit(war, p.side, p.tag, value),
      ],
    });
  }
  function setLeave(p: Participant, value: string) {
    if (value === p.leave_date) return;
    const edits = [];
    if (p.leave_date) edits.push(removePartStatementEdit(war, p.side, p.tag, false, p.leave_date));
    edits.push(leaveParticipantEdit(war, p.side, p.tag, value));
    queue.push({ label: `Set ${p.tag} leave date`, edits });
  }
  function clearLeave(p: Participant) {
    if (!p.leave_date) return;
    queue.push({
      label: `${p.tag} rejoins (clear leave)`,
      edits: [removePartStatementEdit(war, p.side, p.tag, false, p.leave_date)],
    });
  }

  // --- delete -------------------------------------------------------------
  let confirmingDelete = $state(false);
  const toolkit = $derived(isToolkitWar(war));
  function doDelete() {
    queue.push({
      label: `Delete war ${warName}`,
      edits: [toolkit ? deleteWarEdit(war) : shadowDeleteBaseWarEdit(war)],
    });
    confirmingDelete = false;
    ondeleted();
  }

  // Per-participant flag cache.
  const flags = $state<Record<string, string>>({});
  $effect(() => {
    for (const p of war.participants) {
      if (flags[p.tag] === undefined) {
        getFlagUrl(installPath, modPath, p.tag).then((u) => {
          if (u) flags[p.tag] = u;
        });
      }
    }
  });
</script>

<div class="war-panel">
  <div class="head">
    <input class="war-name" bind:value={nameDraft} onblur={commitName} onkeydown={(e) => e.key === "Enter" && commitName()} />
    <button class="x" title="Close war" onclick={onclose}>✕</button>
  </div>

  <!-- War goal -->
  <section class="goal">
    <h4>War goal</h4>
    <div class="grid">
      <label><span>Type</span>
        <select value={war.war_goal?.goal_type ?? ""} onchange={(e) => setGoalType((e.currentTarget as HTMLSelectElement).value)}>
          <option value="" disabled>—</option>
          {#each goalTypeItems as t}<option value={t.key}>{t.label}</option>{/each}
        </select>
      </label>
      <label><span>Casus belli</span>
        <select value={war.war_goal?.casus_belli ?? ""} onchange={(e) => setCasusBelli((e.currentTarget as HTMLSelectElement).value)}>
          <option value="" disabled>—</option>
          {#each cbTypes as c}<option value={c.key}>{c.label}</option>{/each}
        </select>
      </label>
    </div>
    {#if kind === "province"}
      <label class="target"><span>Target province</span>
        <input class="prov" type="number" min="1" bind:value={provinceDraft} onblur={commitProvinceTarget} onkeydown={(e) => e.key === "Enter" && commitProvinceTarget()} />
      </label>
    {:else if kind === "tag"}
      <label class="target"><span>Target country</span>
        <SearchDropdown items={countries} value={war.war_goal?.tag ?? null} onselect={(k) => setTagTarget(k)} placeholder="Pick a country…" />
      </label>
    {:else}
      <p class="note">This war goal has no province/country target.</p>
    {/if}
  </section>

  <!-- Participants -->
  <div class="cols">
    {#each [ATTACKER, DEFENDER] as side}
      {@const list = side === ATTACKER ? attackers : defenders}
      <section class="col">
        <h4 class:atk={side === ATTACKER} class:def={side === DEFENDER}>
          {side === ATTACKER ? "Attackers" : "Defenders"} ({list.length})
        </h4>
        {#each list as p (p.tag)}
          <div class="part">
            <button class="who" onclick={() => onopencountry?.(p.tag)} title="Open {nameOf(p.tag)}">
              {#if flags[p.tag]}<img class="flag" src={flags[p.tag]} alt="" />{:else}<span class="flag ph"></span>{/if}
              <span class="pn">{nameOf(p.tag)}</span>
            </button>
            <div class="pdates">
              <label><span>Joined</span><DatePicker value={p.join_date ?? startDate} onchange={(v) => changeJoin(p, v)} /></label>
              <label class="leave">
                <span>Left</span>
                {#if p.leave_date}
                  <DatePicker value={p.leave_date} onchange={(v) => setLeave(p, v)} />
                  <button class="mini" title="Clear leave date (rejoins)" onclick={() => clearLeave(p)}>↺</button>
                {:else}
                  <span class="still" title="Still in the war">in war</span>
                  <button class="mini danger" title="Set a leave date" onclick={() => setLeave(p, startDate)}>✕</button>
                {/if}
              </label>
            </div>
          </div>
        {/each}
        {#if addSide === side}
          <div class="addbox">
            <SearchDropdown items={addItems} bind:value={addTag} placeholder="Pick a country…" />
            <label class="jd"><span>Joins</span><DatePicker bind:value={addDate} /></label>
            <div class="addactions">
              <button class="confirm" disabled={!addTag} onclick={confirmAdd}>Add</button>
              <button class="cancel" onclick={() => (addSide = null)}>Cancel</button>
            </div>
          </div>
        {:else}
          <button class="add" onclick={() => openAdd(side)}>+ Add {side === ATTACKER ? "attacker" : "defender"}</button>
        {/if}
      </section>
    {/each}
  </div>

  <!-- Battles + delete -->
  <div class="foot">
    <span class="battles" title="Battle blocks are preserved byte-exactly, never edited">
      {war.battle_count} battle{war.battle_count === 1 ? "" : "s"} recorded
    </span>
    <span class="spacer"></span>
    {#if confirmingDelete}
      <span class="delnote">
        {#if toolkit}Delete this toolkit war file?{:else}Base war: writes an empty project shadow (in-game tolerance unverified).{/if}
      </span>
      <button class="mini danger" onclick={doDelete}>Delete</button>
      <button class="mini" onclick={() => (confirmingDelete = false)}>Cancel</button>
    {:else}
      <button class="mini danger" onclick={() => (confirmingDelete = true)}>Delete war</button>
    {/if}
  </div>
</div>

<style>
  .war-panel {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    border: 1px solid #1f242c;
    background: #262c35;
    padding: 0.5rem;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 0.4rem;
  }
  .war-name {
    flex: 1;
    background: #21262e;
    border: 1px solid #1f242c;
    color: #e6e9ee;
    font-family: inherit;
    font-size: 0.9rem;
    font-weight: 600;
    padding: 0.25rem 0.4rem;
    outline: none;
  }
  .x {
    border: 1px solid #1f242c;
    background: #2b323d;
    color: #cfd4db;
    cursor: pointer;
    font-size: 0.75rem;
    padding: 0.15rem 0.4rem;
  }
  .x:hover {
    background: #c0392b;
    color: #fff;
  }
  h4 {
    margin: 0 0 0.3rem;
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: #8a919c;
  }
  h4.atk {
    color: #e08b80;
  }
  h4.def {
    color: #86a9d6;
  }
  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.4rem;
  }
  .goal label,
  .target {
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
  }
  .goal span {
    font-size: 0.65rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: #8a919c;
  }
  .target {
    margin-top: 0.4rem;
  }
  select,
  .prov {
    background: #21262e;
    border: 1px solid #1f242c;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.82rem;
    padding: 0.25rem 0.3rem;
    outline: none;
  }
  .note {
    margin: 0.3rem 0 0;
    font-size: 0.72rem;
    color: #8a919c;
  }
  .cols {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.5rem;
  }
  .col {
    border: 1px solid #1f242c;
    background: #21262e;
    padding: 0.4rem;
  }
  .part {
    border-bottom: 1px solid #1f242c;
    padding: 0.2rem 0;
  }
  .who {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    border: none;
    background: transparent;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.82rem;
    cursor: pointer;
    padding: 0.1rem 0;
    min-width: 0;
  }
  .who:hover {
    color: #fff;
  }
  .flag {
    width: 1.1rem;
    height: 1.1rem;
    object-fit: cover;
    border: 1px solid #1f242c;
    flex: none;
  }
  .flag.ph {
    display: inline-block;
    background: #3a4150;
  }
  .pn {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 7rem;
  }
  .pdates {
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
    margin: 0.15rem 0 0.1rem;
  }
  .pdates label {
    display: inline-flex;
    align-items: center;
    gap: 0.25rem;
    font-size: 0.68rem;
    color: #8a919c;
  }
  .leave {
    flex-wrap: wrap;
  }
  .still {
    font-size: 0.72rem;
    color: #7fae7f;
  }
  .mini {
    border: 1px solid #1f242c;
    background: #2b323d;
    color: #cfd4db;
    cursor: pointer;
    font-size: 0.7rem;
    padding: 0.05rem 0.3rem;
  }
  .mini:hover {
    background: #4a6da7;
    color: #fff;
  }
  .mini.danger:hover {
    background: #c0392b;
  }
  .add {
    display: block;
    width: 100%;
    margin-top: 0.3rem;
    border: 1px dashed #3a4150;
    background: transparent;
    color: #9ca3af;
    font-family: inherit;
    font-size: 0.75rem;
    padding: 0.2rem;
    cursor: pointer;
  }
  .add:hover {
    background: #2b323d;
    color: #fff;
  }
  .addbox {
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
    margin-top: 0.3rem;
    padding: 0.3rem;
    border: 1px solid #1f242c;
    background: #262c35;
  }
  .jd {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    font-size: 0.68rem;
    color: #8a919c;
  }
  .addactions {
    display: flex;
    gap: 0.3rem;
  }
  .confirm,
  .cancel {
    border: 1px solid #1f242c;
    background: #3f4855;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.76rem;
    padding: 0.2rem 0.6rem;
    cursor: pointer;
  }
  .confirm:hover:not(:disabled) {
    background: #4a6da7;
    color: #fff;
  }
  .confirm:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .cancel:hover {
    background: #4a6da7;
    color: #fff;
  }
  .foot {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    flex-wrap: wrap;
  }
  .battles {
    font-size: 0.72rem;
    color: #9ca3af;
  }
  .spacer {
    flex: 1;
  }
  .delnote {
    font-size: 0.68rem;
    color: #fca5a5;
    max-width: 16rem;
  }
</style>
