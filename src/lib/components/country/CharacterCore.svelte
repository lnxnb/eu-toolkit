<!--
  CharacterCore — the editable fields shared by the Ruler, Queen, and Heir
  sections (Sprint 1.2): name, dynasty (DynastyModal), ADM/DIP/MIL steppers,
  birth date, female toggle, culture/religion overrides, ruler personalities, and
  leader stats (with a "Make a general" scaffold). Every field is a composite on
  the shared queue, addressed at [<date>, <holder>, <key>] in the history file,
  read back through the character.ts projection so dirty/undo/save "just work".

  Section-specific fields (regent, monarch_name, claim, country_of_origin, dates)
  live in the wrapping section, not here.
-->
<script lang="ts">
  import { SearchDropdown, DatePicker } from "$lib/components/ui";
  import type { DropdownItem } from "$lib/components/ui";
  import type { EditQueue } from "$lib/edits.svelte";
  import Stepper from "./Stepper.svelte";
  import DynastyModal from "../DynastyModal.svelte";
  import {
    charValue,
    charEdited,
    setCharEdit,
    removeCharEdit,
    effectivePersonalities,
    addPersonalityEdit,
    removePersonalityEdit,
  } from "./character";
  import type { LeaderInfo, Personality } from "./types";

  interface CharCommon {
    date: string;
    name: string | null;
    dynasty: string | null;
    adm: number | null;
    dip: number | null;
    mil: number | null;
    birth_date: string | null;
    female: boolean;
    culture: string | null;
    religion: string | null;
    personalities: Personality[];
    leader: LeaderInfo | null;
  }

  let {
    installPath,
    modPath,
    tag,
    queue,
    file,
    holder,
    label,
    personalityEffect,
    char,
    cultures,
    religions,
    personalityItems,
  }: {
    installPath: string;
    modPath: string | null;
    tag: string;
    queue: EditQueue;
    file: string;
    /** "monarch" | "queen" | "heir". */
    holder: string;
    /** Human noun for edit labels, e.g. "ruler". */
    label: string;
    /** "add_ruler_personality" | "add_queen_personality" | "add_heir_personality". */
    personalityEffect: string;
    char: CharCommon;
    cultures: DropdownItem[];
    religions: DropdownItem[];
    personalityItems: DropdownItem[];
  } = $props();

  const blockPath = $derived([char.date, holder]);

  // --- Name ---
  const nameVal = $derived(charValue(queue, file, blockPath, "name", char.name) ?? "");
  function setName(v: string) {
    const t = v.trim();
    if (!t) return;
    queue.push({
      label: `Rename ${label} of ${tag}`,
      edits: [setCharEdit(file, blockPath, "name", t, char.name != null, true)],
      coalesceKey: `char:${tag}:${holder}:name`,
    });
  }

  // --- Dynasty (modal) ---
  let dynastyOpen = $state(false);
  const dynastyVal = $derived(charValue(queue, file, blockPath, "dynasty", char.dynasty));
  function pickDynasty(name: string) {
    queue.push({
      label: `Set ${label} dynasty of ${tag}`,
      edits: [setCharEdit(file, blockPath, "dynasty", name, char.dynasty != null, true)],
    });
    dynastyOpen = false;
  }

  // --- ADM / DIP / MIL ---
  function setStat(key: "adm" | "dip" | "mil", base: number | null, v: number) {
    queue.push({
      label: `Set ${label} ${key.toUpperCase()} of ${tag}`,
      edits: [setCharEdit(file, blockPath, key, String(v), base != null)],
    });
  }
  const admVal = $derived(charValue(queue, file, blockPath, "adm", char.adm != null ? String(char.adm) : null));
  const dipVal = $derived(charValue(queue, file, blockPath, "dip", char.dip != null ? String(char.dip) : null));
  const milVal = $derived(charValue(queue, file, blockPath, "mil", char.mil != null ? String(char.mil) : null));

  // --- Birth date ---
  const birthVal = $derived(charValue(queue, file, blockPath, "birth_date", char.birth_date) ?? "1400.1.1");
  const birthEdited = $derived(charEdited(queue, file, blockPath, "birth_date", char.birth_date));
  function setBirth(v: string) {
    queue.push({
      label: `Set ${label} birth date of ${tag}`,
      edits: [setCharEdit(file, blockPath, "birth_date", v, char.birth_date != null)],
    });
  }

  // --- Female toggle (presence of `female = yes`) ---
  const femaleVal = $derived(charValue(queue, file, blockPath, "female", char.female ? "yes" : null));
  const femaleOn = $derived(femaleVal === "yes");
  function toggleFemale() {
    const edit = femaleOn
      ? removeCharEdit(file, blockPath, "female")
      : setCharEdit(file, blockPath, "female", "yes", char.female);
    queue.push({ label: `${femaleOn ? "Clear" : "Set"} ${label} female of ${tag}`, edits: [edit] });
  }

  // --- Optional culture / religion overrides ---
  const cultureVal = $derived(charValue(queue, file, blockPath, "culture", char.culture));
  const religionVal = $derived(charValue(queue, file, blockPath, "religion", char.religion));
  function setOverride(key: "culture" | "religion", base: string | null, v: string, items: DropdownItem[]) {
    if (!v) {
      queue.push({ label: `Clear ${label} ${key} of ${tag}`, edits: [removeCharEdit(file, blockPath, key)] });
      return;
    }
    queue.push({
      label: `Set ${label} ${key} of ${tag}`,
      edits: [setCharEdit(file, blockPath, key, v, base != null)],
    });
  }

  // --- Personalities ---
  const personalities = $derived(effectivePersonalities(queue, file, personalityEffect, char.personalities));
  function personalityLabel(key: string): string {
    return personalityItems.find((p) => p.key === key)?.label ?? key;
  }
  function addPersonality(key: string) {
    if (!key || personalities.includes(key)) return;
    queue.push({
      label: `Add ${label} personality to ${tag}`,
      edits: [addPersonalityEdit(file, char.date, personalityEffect, key)],
    });
  }
  function removePersonality(key: string) {
    // Remove from the dated block it lives in (base) or, for a pending-added one,
    // the character's own date (where the insert put it).
    const base = char.personalities.find((p) => p.key === key);
    queue.push({
      label: `Remove ${label} personality from ${tag}`,
      edits: [removePersonalityEdit(file, base?.date ?? char.date, personalityEffect, key)],
    });
  }

  // --- Leader stats (fire/shock/manuever/siege) ---
  const leaderPath = $derived([char.date, holder, "leader"]);
  const hasLeader = $derived(char.leader != null);
  function setLeaderStat(key: "fire" | "shock" | "manuever" | "siege", base: number | null, v: number) {
    queue.push({
      label: `Set ${label} leader ${key} of ${tag}`,
      edits: [setCharEdit(file, leaderPath, key, String(v), base != null)],
    });
  }
  function leaderStatVal(key: "fire" | "shock" | "manuever" | "siege", base: number | null): number {
    const p = charValue(queue, file, leaderPath, key, base != null ? String(base) : null);
    return p != null ? parseInt(p, 10) || 0 : 0;
  }
  function makeGeneral() {
    // Scaffold a leader block inside the character block.
    const name = nameVal || char.name || label;
    queue.push({
      label: `Make ${label} of ${tag} a general`,
      edits: [
        {
          kind: "insertStatement",
          file,
          blockPath,
          statement: `leader = { name = "${name}" type = general fire = 0 shock = 0 manuever = 0 siege = 0 }`,
        },
      ],
    });
  }
</script>

<div class="rows">
  <div class="row">
    <span class="lbl">Name</span>
    <input class="text" value={nameVal} onchange={(e) => setName(e.currentTarget.value)} />
  </div>

  <div class="row">
    <span class="lbl">Dynasty</span>
    <button class="dyn" onclick={() => (dynastyOpen = true)}>
      {dynastyVal || "(none)"}
    </button>
  </div>

  <div class="row stats">
    <span class="lbl">ADM / DIP / MIL</span>
    <div class="triple">
      <Stepper value={admVal != null ? parseInt(admVal, 10) : null} onchange={(v) => setStat("adm", char.adm, v)} />
      <Stepper value={dipVal != null ? parseInt(dipVal, 10) : null} onchange={(v) => setStat("dip", char.dip, v)} />
      <Stepper value={milVal != null ? parseInt(milVal, 10) : null} onchange={(v) => setStat("mil", char.mil, v)} />
    </div>
  </div>

  <div class="row">
    <span class="lbl">Birth Date {#if birthEdited}<span class="e">•</span>{/if}</span>
    <DatePicker value={birthVal} onchange={setBirth} />
  </div>

  <div class="row">
    <span class="lbl">Female</span>
    <label class="check">
      <input type="checkbox" checked={femaleOn} onchange={toggleFemale} />
      <span>{femaleOn ? "Female" : "Male"}</span>
    </label>
  </div>

  <div class="row">
    <span class="lbl">Culture</span>
    <SearchDropdown
      items={cultures}
      value={cultureVal}
      placeholder="(inherit country)"
      onselect={(k) => setOverride("culture", char.culture, k, cultures)}
    />
  </div>

  <div class="row">
    <span class="lbl">Religion</span>
    <SearchDropdown
      items={religions}
      value={religionVal}
      placeholder="(inherit country)"
      onselect={(k) => setOverride("religion", char.religion, k, religions)}
    />
  </div>

  <div class="list-field">
    <div class="lbl">Personalities</div>
    {#each personalities as p (p)}
      <span class="chip">
        {personalityLabel(p)}
        <button class="x" onclick={() => removePersonality(p)} aria-label="Remove">×</button>
      </span>
    {/each}
    <SearchDropdown
      items={personalityItems}
      value={null}
      placeholder="Add personality…"
      onselect={addPersonality}
    />
  </div>

  <div class="row">
    <span class="lbl">Leader (general)</span>
    {#if hasLeader}
      <div class="triple leader">
        <label>F<Stepper value={leaderStatVal("fire", char.leader?.fire ?? null)} min={0} max={9} onchange={(v) => setLeaderStat("fire", char.leader?.fire ?? null, v)} /></label>
        <label>S<Stepper value={leaderStatVal("shock", char.leader?.shock ?? null)} min={0} max={9} onchange={(v) => setLeaderStat("shock", char.leader?.shock ?? null, v)} /></label>
        <label>M<Stepper value={leaderStatVal("manuever", char.leader?.manuever ?? null)} min={0} max={9} onchange={(v) => setLeaderStat("manuever", char.leader?.manuever ?? null, v)} /></label>
        <label>Si<Stepper value={leaderStatVal("siege", char.leader?.siege ?? null)} min={0} max={9} onchange={(v) => setLeaderStat("siege", char.leader?.siege ?? null, v)} /></label>
      </div>
    {:else}
      <button class="btn" onclick={makeGeneral}>Make a general…</button>
    {/if}
  </div>
</div>

<DynastyModal
  bind:open={dynastyOpen}
  mode="pick"
  {installPath}
  {modPath}
  onpick={pickDynasty}
/>

<style>
  .rows {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }

  .row {
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
  }

  .lbl {
    font-size: 0.72rem;
    text-transform: uppercase;
    letter-spacing: 0.03em;
    color: #8a919c;
  }

  .e {
    color: #fde68a;
  }

  .text {
    background: #21262e;
    border: 1px solid #1f242c;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.85rem;
    padding: 0.25rem 0.4rem;
    outline: none;
  }

  .dyn {
    align-self: flex-start;
    border: 1px solid #4b5563;
    background: #21262e;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.85rem;
    padding: 0.25rem 0.6rem;
    cursor: pointer;
  }

  .dyn:hover {
    border-color: #9ca3af;
  }

  .triple {
    display: flex;
    gap: 0.5rem;
  }

  .triple.leader label {
    display: inline-flex;
    align-items: center;
    gap: 0.2rem;
    font-size: 0.72rem;
    color: #8a919c;
  }

  .check {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    font-size: 0.85rem;
    cursor: pointer;
  }

  .list-field {
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
  }

  .chip {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    align-self: flex-start;
    background: #21262e;
    border: 1px solid #1f242c;
    color: #cfd4db;
    font-size: 0.8rem;
    padding: 0.12rem 0.2rem 0.12rem 0.45rem;
  }

  .x {
    border: none;
    background: transparent;
    color: #9ca3af;
    cursor: pointer;
    font-size: 0.95rem;
    line-height: 1;
    padding: 0 0.2rem;
  }

  .x:hover {
    color: #fca5a5;
  }

  .btn {
    align-self: flex-start;
    border: 1px solid #4b5563;
    background: transparent;
    color: inherit;
    font-family: inherit;
    font-size: 0.8rem;
    padding: 0.25rem 0.6rem;
    cursor: pointer;
  }

  .btn:hover {
    border-color: #9ca3af;
    background: #4a6da7;
    color: #fff;
  }
</style>
