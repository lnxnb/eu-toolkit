<!--
  QueenSection — the queen/consort editor (Sprint 1.2). Optional: may not exist.
  Wraps CharacterCore with the queen-only country_of_origin (tag picker), death
  date, and regent toggle, plus Add / Remove consort (whole `queen = { … }` block
  inside the ruler's dated block).
-->
<script lang="ts">
  import { SearchDropdown, DatePicker } from "$lib/components/ui";
  import type { DropdownItem } from "$lib/components/ui";
  import type { EditQueue } from "$lib/edits.svelte";
  import CharacterCore from "./CharacterCore.svelte";
  import { charValue, charEdited, setCharEdit, removeCharEdit, holderExists, createHolderEdit, createAtDate } from "./character";
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
    countries,
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
    countries: DropdownItem[];
  } = $props();

  const file = $derived(details.history_file ?? `history/countries/${tag} - ${details.name}.txt`);
  const queen = $derived(details.queen);
  const exists = $derived(holderExists(queue, file, "queen", queen != null));

  const bp = $derived(queen ? [queen.date, "queen"] : []);

  const originVal = $derived(queen ? charValue(queue, file, bp, "country_of_origin", queen.country_of_origin) : null);
  function setOrigin(v: string) {
    if (!queen) return;
    queue.push({
      label: `Set consort origin of ${tag}`,
      edits: [setCharEdit(file, bp, "country_of_origin", v, queen.country_of_origin != null)],
    });
  }

  const deathVal = $derived(queen ? charValue(queue, file, bp, "death_date", queen.death_date) ?? "1500.1.1" : "1500.1.1");
  const deathEdited = $derived(queen ? charEdited(queue, file, bp, "death_date", queen.death_date) : false);
  function setDeath(v: string) {
    if (!queen) return;
    queue.push({
      label: `Set consort death date of ${tag}`,
      edits: [setCharEdit(file, bp, "death_date", v, queen.death_date != null)],
    });
  }

  const regentVal = $derived(queen ? charValue(queue, file, bp, "regent", queen.regent ? "yes" : null) : null);
  const regentOn = $derived(regentVal === "yes");
  function toggleRegent() {
    if (!queen) return;
    const edit = regentOn
      ? removeCharEdit(file, bp, "regent")
      : setCharEdit(file, bp, "regent", "yes", queen.regent);
    queue.push({ label: `${regentOn ? "Clear" : "Set"} consort regent of ${tag}`, edits: [edit] });
  }

  const createOn = $derived(createAtDate(date, startDate));
  const createLater = $derived(date != null && compareDates(date, startDate) > 0);
  function addConsort() {
    const fields = `name = "Consort" female = yes adm = 2 dip = 2 mil = 2`;
    // Anchor to the ruler's dated block if present; else a fresh block at the
    // selected date (Sprint 12.3: was hardwired 1444.11.11).
    if (details.ruler) {
      queue.push({
        label: `Add consort to ${tag}`,
        edits: [{ kind: "insertStatement", file, blockPath: [details.ruler.date], statement: `queen = { ${fields} }` }],
        ...(createLater ? { date: createOn } : {}),
      });
    } else {
      queue.push({
        label: `Add consort to ${tag}`,
        edits: [createHolderEdit(file, createOn, "queen", fields)],
        ...(createLater ? { date: createOn } : {}),
      });
    }
  }
  function removeConsort() {
    if (!queen) return;
    queue.push({
      label: `Remove consort of ${tag}`,
      edits: [{ kind: "removeStatement", file, blockPath: [queen.date], key: "queen", value: null }],
    });
  }
</script>

<section>
  <div class="head">
    <h3>Queen / Consort</h3>
    {#if queen}
      <button class="mini danger" onclick={removeConsort}>Remove</button>
    {/if}
  </div>

  {#if queen}
    <CharacterCore
      {installPath}
      {modPath}
      {tag}
      {queue}
      {file}
      holder="queen"
      label="consort"
      personalityEffect="add_queen_personality"
      char={queen}
      {cultures}
      {religions}
      {personalityItems}
    />
    <div class="row">
      <span class="lbl">Country of Origin</span>
      <SearchDropdown items={countries} value={originVal} placeholder="Origin tag…" onselect={setOrigin} />
    </div>
    <div class="row">
      <span class="lbl">Death Date {#if deathEdited}<span class="e">•</span>{/if}</span>
      <DatePicker value={deathVal} onchange={setDeath} />
    </div>
    <div class="row">
      <span class="lbl">Regent</span>
      <label class="check">
        <input type="checkbox" checked={regentOn} onchange={toggleRegent} />
        <span>{regentOn ? "Regency council" : "Consort"}</span>
      </label>
    </div>
  {:else if exists}
    <p class="ok">Consort queued. Save and reopen the country to edit its fields.</p>
  {:else}
    <p class="dim">No consort defined.</p>
    <button class="btn" onclick={addConsort}>Add consort…</button>
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
    color: var(--text-2);
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
    color: var(--text-2);
  }

  .e {
    color: var(--warn);
  }

  .check {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    font-size: 0.85rem;
    cursor: pointer;
  }

  .dim {
    color: var(--text-2);
    font-size: 0.83rem;
    margin: 0 0 0.4rem;
  }

  .ok {
    color: var(--ok);
    font-size: 0.83rem;
  }

  .btn,
  .mini {
    border: 1px solid var(--border-strong);
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

  .btn:hover {
    border-color: var(--text-2);
    background: var(--accent);
    color: var(--text-inverse);
  }

  .mini {
    font-size: 0.72rem;
    padding: 0.1rem 0.45rem;
    margin-bottom: 0.5rem;
  }

  .mini.danger:hover {
    background: var(--danger-bg);
    border-color: var(--err);
    color: var(--text-inverse);
  }
</style>
