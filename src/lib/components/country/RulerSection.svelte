<!--
  RulerSection — the monarch-at-1444 editor (Sprint 1.2). Wraps CharacterCore with
  the ruler-only regent toggle, and handles countries with NO 1444 ruler (PU
  juniors like Sweden): it explains why and offers a "Create starting ruler" that
  queues a whole `1444.11.11 = { monarch = { … } }` dated block (3/3/3 default).
-->
<script lang="ts">
  import type { DropdownItem } from "$lib/components/ui";
  import type { EditQueue } from "$lib/edits.svelte";
  import CharacterCore from "./CharacterCore.svelte";
  import { charValue, setCharEdit, removeCharEdit, holderExists, createHolderEdit, createAtDate } from "./character";
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
  const ruler = $derived(details.ruler);

  // Regent toggle (ruler-only; presence of `regent = yes` in the monarch block).
  const regentBlockPath = $derived(ruler ? [ruler.date, "monarch"] : []);
  const regentVal = $derived(
    ruler ? charValue(queue, file, regentBlockPath, "regent", ruler.regent ? "yes" : null) : null,
  );
  const regentOn = $derived(regentVal === "yes");
  function toggleRegent() {
    if (!ruler) return;
    const edit = regentOn
      ? removeCharEdit(file, regentBlockPath, "regent")
      : setCharEdit(file, regentBlockPath, "regent", "yes", ruler.regent);
    queue.push({ label: `${regentOn ? "Clear" : "Set"} regent of ${tag}`, edits: [edit] });
  }

  // --- No-1444-ruler: create a starting ruler ---
  const rulerCreated = $derived(holderExists(queue, file, "monarch", ruler != null));
  let newName = $state("");
  let newDynasty = $state("");
  // The date a created ruler lands on: the selected view date (Sprint 12.3), else
  // the effective start (was hardwired 1444.11.11). Tag the composite so pending
  // folds gate it when it's a later-dated create.
  const createOn = $derived(createAtDate(date, startDate));
  const createLater = $derived(date != null && compareDates(date, startDate) > 0);
  function createRuler() {
    const name = newName.trim() || "New Ruler";
    const dyn = newDynasty.trim();
    const dynLine = dyn ? ` dynasty = "${dyn}"` : "";
    queue.push({
      label: `Create ${createLater ? createOn : "starting"} ruler for ${tag}`,
      edits: [createHolderEdit(file, createOn, "monarch", `name = "${name}"${dynLine} adm = 3 dip = 3 mil = 3`)],
      ...(createLater ? { date: createOn } : {}),
    });
    newName = "";
    newDynasty = "";
  }
</script>

<section>
  <h3>Ruler</h3>
  {#if ruler}
    <CharacterCore
      {installPath}
      {modPath}
      {tag}
      {queue}
      {file}
      holder="monarch"
      label="ruler"
      personalityEffect="add_ruler_personality"
      char={ruler}
      {cultures}
      {religions}
      {personalityItems}
    />
    <div class="row">
      <span class="lbl">Regent</span>
      <label class="check">
        <input type="checkbox" checked={regentOn} onchange={toggleRegent} />
        <span>{regentOn ? "Regency council" : "Full ruler"}</span>
      </label>
    </div>
  {:else if rulerCreated}
    <p class="ok">Starting ruler queued. Save and reopen the country to edit its fields.</p>
  {:else}
    <p class="reason">{details.ruler_reason ?? "No 1444 ruler."}</p>
    <div class="create">
      <input class="text" placeholder="Ruler name" bind:value={newName} />
      <input class="text" placeholder="Dynasty (optional)" bind:value={newDynasty} />
      <button class="btn" onclick={createRuler}>Create starting ruler (3/3/3)</button>
    </div>
  {/if}
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

  .check {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    font-size: 0.85rem;
    cursor: pointer;
  }

  .reason {
    color: var(--warn);
    font-size: 0.83rem;
    margin: 0 0 0.5rem;
  }

  .ok {
    color: var(--ok);
    font-size: 0.83rem;
  }

  .create {
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
  }

  .text {
    background: var(--bg-1);
    border: 1px solid var(--border);
    color: var(--text-1);
    font-family: inherit;
    font-size: 0.85rem;
    padding: 0.25rem 0.4rem;
    outline: none;
  }

  .btn {
    align-self: flex-start;
    border: 1px solid var(--border-strong);
    background: transparent;
    color: inherit;
    font-family: inherit;
    font-size: 0.82rem;
    padding: 0.28rem 0.6rem;
    cursor: pointer;
  }

  .btn:hover {
    border-color: var(--text-2);
    background: var(--accent);
    color: var(--text-inverse);
  }
</style>
