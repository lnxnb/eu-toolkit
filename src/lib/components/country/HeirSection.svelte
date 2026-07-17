<!--
  HeirSection — the heir editor (Sprint 1.2). Optional: may not exist. Wraps
  CharacterCore with heir-only monarch_name (regnal), death date, and claim
  (0-100), plus Add / Remove heir (whole `heir = { … }` block in the ruler's dated
  block). Client-side, non-blocking validation: an heir needs a ruler, and dates
  must be coherent (born after the ruler, death after birth).
-->
<script lang="ts">
  import { DatePicker } from "$lib/components/ui";
  import type { DropdownItem } from "$lib/components/ui";
  import type { EditQueue, TypedEdit } from "$lib/edits.svelte";
  import CharacterCore from "./CharacterCore.svelte";
  import Stepper from "./Stepper.svelte";
  import { charValue, charEdited, setCharEdit, holderExists, createHolderEdit, createAtDate } from "./character";
  import { compareDates } from "$lib/calendar";
  import type { CountryDetails } from "./types";

  let {
    installPath,
    modPath,
    tag,
    queue,
    details,
    date = null,
    startDate = "1444.11.11",
    cultures,
    religions,
    personalityItems,
  }: {
    installPath: string;
    modPath: string | null;
    tag: string;
    queue: EditQueue;
    details: CountryDetails;
    /** Selected view/edit date (Sprint 12.3); null = effective start. */
    date?: string | null;
    /** The mod's effective start date; the base-state baseline. */
    startDate?: string;
    cultures: DropdownItem[];
    religions: DropdownItem[];
    personalityItems: DropdownItem[];
  } = $props();

  const file = $derived(details.history_file ?? `history/countries/${tag} - ${details.name}.txt`);
  const heir = $derived(details.heir);
  const exists = $derived(holderExists(queue, file, "heir", heir != null));
  const bp = $derived(heir ? [heir.date, "heir"] : []);

  const monarchName = $derived(heir ? charValue(queue, file, bp, "monarch_name", heir.monarch_name) ?? "" : "");
  function setMonarchName(v: string) {
    if (!heir) return;
    const t = v.trim();
    queue.push({
      label: `Set heir regnal name of ${tag}`,
      edits: [setCharEdit(file, bp, "monarch_name", t, heir.monarch_name != null, true)],
      coalesceKey: `heir:${tag}:monarch_name`,
    });
  }

  const birthVal = $derived(heir ? charValue(queue, file, bp, "birth_date", heir.birth_date) : null);
  const deathVal = $derived(heir ? charValue(queue, file, bp, "death_date", heir.death_date) ?? "1500.1.1" : "1500.1.1");
  const deathEdited = $derived(heir ? charEdited(queue, file, bp, "death_date", heir.death_date) : false);
  function setDeath(v: string) {
    if (!heir) return;
    queue.push({
      label: `Set heir death date of ${tag}`,
      edits: [setCharEdit(file, bp, "death_date", v, heir.death_date != null)],
    });
  }

  const claimVal = $derived(heir ? charValue(queue, file, bp, "claim", heir.claim != null ? String(heir.claim) : null) : null);
  function setClaim(v: number) {
    if (!heir) return;
    queue.push({
      label: `Set heir claim of ${tag}`,
      edits: [setCharEdit(file, bp, "claim", String(v), heir.claim != null)],
    });
  }

  // --- Validation (non-blocking) ---
  function ord(d: string | null | undefined): number | null {
    if (!d) return null;
    const p = d.split(".").map(Number);
    return p.length === 3 && p.every(Number.isFinite) ? p[0] * 10000 + p[1] * 100 + p[2] : null;
  }
  const warnings = $derived.by(() => {
    const w: string[] = [];
    if (heir && !details.ruler) w.push("An heir requires a ruler — none is defined at 1444.");
    const hb = ord(birthVal);
    const rb = ord(details.ruler?.birth_date);
    const hd = ord(deathVal);
    if (hb != null && rb != null && hb < rb) w.push("Heir is born before the ruler's birth date.");
    if (hb != null && hd != null && hd < hb) w.push("Heir's death date precedes their birth date.");
    return w;
  });

  const createOn = $derived(createAtDate(date, startDate));
  const createLater = $derived(date != null && compareDates(date, startDate) > 0);
  function addHeir() {
    const fields = `name = "Heir" monarch_name = "Heir I" claim = 50 birth_date = 1430.1.1 adm = 2 dip = 2 mil = 2`;
    // Anchor to the ruler's dated block if present; else a fresh block at the
    // selected date (Sprint 12.3: was hardwired 1444.11.11).
    const edit: TypedEdit = details.ruler
      ? { kind: "insertStatement", file, blockPath: [details.ruler.date], statement: `heir = { ${fields} }` }
      : createHolderEdit(file, createOn, "heir", fields);
    queue.push({ label: `Add heir to ${tag}`, edits: [edit], ...(createLater ? { date: createOn } : {}) });
  }
  function removeHeir() {
    if (!heir) return;
    queue.push({
      label: `Remove heir of ${tag}`,
      edits: [{ kind: "removeStatement", file, blockPath: [heir.date], key: "heir", value: null }],
    });
  }
</script>

<section>
  <div class="head">
    <h3>Heir</h3>
    {#if heir}
      <button class="mini danger" onclick={removeHeir}>Remove</button>
    {/if}
  </div>

  {#if warnings.length}
    <ul class="warn">
      {#each warnings as w (w)}<li>{w}</li>{/each}
    </ul>
  {/if}

  {#if heir}
    <div class="row">
      <span class="lbl">Regnal Name (monarch_name)</span>
      <input class="text" value={monarchName} onchange={(e) => setMonarchName(e.currentTarget.value)} />
    </div>
    <CharacterCore
      {installPath}
      {modPath}
      {tag}
      {queue}
      {file}
      holder="heir"
      label="heir"
      personalityEffect="add_heir_personality"
      char={heir}
      {cultures}
      {religions}
      {personalityItems}
    />
    <div class="row">
      <span class="lbl">Death Date {#if deathEdited}<span class="e">•</span>{/if}</span>
      <DatePicker value={deathVal} onchange={setDeath} />
    </div>
    <div class="row">
      <span class="lbl">Claim (0-100)</span>
      <Stepper value={claimVal != null ? parseInt(claimVal, 10) : null} min={0} max={100} onchange={setClaim} />
    </div>
  {:else if exists}
    <p class="ok">Heir queued. Save and reopen the country to edit its fields.</p>
  {:else}
    <p class="dim">No heir defined.</p>
    <button class="btn" onclick={addHeir} disabled={!details.ruler} title={details.ruler ? "" : "An heir requires a ruler"}>
      Add heir…
    </button>
  {/if}
</section>

<style>
  section {
    margin-bottom: 1rem;
  }

  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  h3 {
    margin: 0 0 0.5rem;
    font-size: 0.8rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: #9ca3af;
  }

  .row {
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
    margin-top: 0.4rem;
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

  .warn {
    margin: 0 0 0.5rem;
    padding: 0.35rem 0.5rem 0.35rem 1.4rem;
    background: rgba(234, 179, 8, 0.12);
    border: 1px solid rgba(234, 179, 8, 0.3);
    color: #e6cd82;
    font-size: 0.78rem;
  }

  .dim {
    color: #8a919c;
    font-size: 0.83rem;
    margin: 0 0 0.4rem;
  }

  .ok {
    color: #86c58a;
    font-size: 0.83rem;
  }

  .btn,
  .mini {
    border: 1px solid #4b5563;
    background: transparent;
    color: inherit;
    font-family: inherit;
    cursor: pointer;
  }

  .btn {
    align-self: flex-start;
    font-size: 0.82rem;
    padding: 0.28rem 0.6rem;
  }

  .btn:hover:not(:disabled) {
    border-color: #9ca3af;
    background: #4a6da7;
    color: #fff;
  }

  .btn:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .mini {
    font-size: 0.72rem;
    padding: 0.1rem 0.45rem;
    margin-bottom: 0.5rem;
  }

  .mini.danger:hover {
    background: #7a2e2e;
    border-color: #a13636;
    color: #fff;
  }
</style>
