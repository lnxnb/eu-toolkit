<!--
  GovernmentSection — the country panel's Government & identity block (Sprint 1.2):
  government type, rank, religion, primary culture, tech group, unit type, national
  focus, mercantilism, elector, government reforms, accepted cultures, capital, and
  historical rivals/friends. All write to history/countries/<TAG - Name>.txt (except
  the read-only capital display, edited via the map's Set Capital tool). Every edit
  is a composite on the shared queue and reads its pending value back via projection.

  Grouped dropdowns (religion, culture) append the group name to each row label —
  SearchDropdown has no header slot, so grouping is shown inline and made searchable.
-->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { SearchDropdown } from "$lib/components/ui";
  import type { DropdownItem } from "$lib/components/ui";
  import type { EditQueue } from "$lib/edits.svelte";
  import FieldRow from "./FieldRow.svelte";
  import {
    scalarEdit,
    removeEdit,
    listAdd,
    listRemove,
    pushAtDate,
    pendingHistField,
    pendingHistList,
    type CountryDateCtx,
  } from "./fields";
  import type { CountryDetails, RegistryEntry, GroupedEntry, CountryBrief } from "./types";

  let {
    installPath,
    modPath,
    tag,
    details,
    queue,
    date = null,
    startDate = "1444.11.11",
    onopennaming,
    onopenmechanics,
  }: {
    installPath: string;
    modPath: string | null;
    tag: string;
    details: CountryDetails;
    queue: EditQueue;
    /** Selected view/edit date (Sprint 12) — the naming-rules preview evaluates
     *  the country's state at this date. Null = effective start. */
    date?: string | null;
    /** The mod's effective start date — the pushAtDate write routing needs it. */
    startDate?: string;
    /** Open the Government-names editor (Sprint 19.3), optionally scrolled to a
     *  scheme. MapView owns the overlay. */
    onopennaming?: (schemeKey?: string) => void;
    /** Open the Mechanics editor (Sprint 27) at the government_reforms family,
     *  optionally focused on a reform key (edit… / new… affordance). */
    onopenmechanics?: (family: string, key?: string) => void;
  } = $props();

  interface GovNamePreview {
    tag: string;
    rank: number;
    matchedKey: string | null;
    countryName: string | null;
    rulerName: string | null;
    approximate: boolean;
    skipped: string[];
  }

  // --- Naming-rules preview (19.3): which scheme this country resolves to ---
  let namePreview = $state<GovNamePreview | null>(null);
  $effect(() => {
    const current = tag;
    const at = date;
    namePreview = null;
    invoke<GovNamePreview>("preview_government_name", { installPath, modPath, date: at, tag: current })
      .then((p) => {
        if (current === tag) namePreview = p;
      })
      .catch(() => {
        if (current === tag) namePreview = null;
      });
  });

  const hf = $derived(
    details.history_file ?? `history/countries/${tag} - ${details.name}.txt`,
  );

  // --- Option sources (loaded once per session; cheap registries) ---
  let governments = $state<DropdownItem[]>([]);
  let reforms = $state<DropdownItem[]>([]);
  let techGroups = $state<DropdownItem[]>([]);
  let religions = $state<DropdownItem[]>([]);
  let cultures = $state<DropdownItem[]>([]);
  let countries = $state<DropdownItem[]>([]);

  function css(c: [number, number, number] | null): string | undefined {
    return c ? `rgb(${c[0]}, ${c[1]}, ${c[2]})` : undefined;
  }
  function grouped(rows: GroupedEntry[]): DropdownItem[] {
    return rows
      .slice()
      .sort((a, b) =>
        a.group_name.localeCompare(b.group_name) || a.name.localeCompare(b.name),
      )
      .map((r) => ({ key: r.key, label: `${r.name} — ${r.group_name}`, swatch: css(r.color) }));
  }

  $effect(() => {
    const reg = (name: string) =>
      invoke<RegistryEntry[]>("get_registry", { name, installPath, modPath }).then((r) =>
        r.map((e) => ({ key: e.key, label: e.name })),
      );
    reg("governments").then((v) => (governments = v)).catch(() => {});
    reg("government_reforms").then((v) => (reforms = v)).catch(() => {});
    reg("technology_groups").then((v) => (techGroups = v)).catch(() => {});
    invoke<GroupedEntry[]>("list_religions", { installPath, modPath })
      .then((v) => (religions = grouped(v)))
      .catch(() => {});
    invoke<GroupedEntry[]>("list_cultures", { installPath, modPath })
      .then((v) => (cultures = grouped(v)))
      .catch(() => {});
    invoke<CountryBrief[]>("list_countries", { installPath, modPath })
      .then((v) => (countries = v.map((c) => ({ key: c.tag, label: c.name, swatch: css(c.color) }))))
      .catch(() => {});
  });

  // Date routing (Sprint 12.3 / timeline mods): writes go through `pushAtDate`,
  // which lands them top-level only when the top level is authoritative for the
  // written key at the selected date — else in a dated block.
  const ctx = $derived<CountryDateCtx>({
    file: hf,
    selectedDate: date,
    startDate,
    blocks: details.dated_blocks,
  });

  // --- Single scalar field helper: reads pending, writes set/insert/remove ---
  function fieldValue(key: string, base: string | null): string | null {
    const p = pendingHistField(queue, hf, key, date);
    return p !== undefined ? p.value : base;
  }
  function isEdited(key: string, base: string | null): boolean {
    const p = pendingHistField(queue, hf, key, date);
    return p !== undefined && p.value !== base;
  }
  function setField(key: string, base: string | null, value: string, label: string) {
    if (value === base) return;
    pushAtDate(queue, ctx, label, [scalarEdit(hf, key, value, base != null)], [
      `${key} = ${value}`,
    ]);
  }

  // Government type
  let govValue = $derived(fieldValue("government", details.government));
  // Rank
  const RANKS: DropdownItem[] = [
    { key: "1", label: "Duchy" },
    { key: "2", label: "Kingdom" },
    { key: "3", label: "Empire" },
  ];
  let rankBase = $derived(details.government_rank != null ? String(details.government_rank) : null);
  let rankValue = $derived(fieldValue("government_rank", rankBase));
  // Religion / culture / tech / unit type
  let religionValue = $derived(fieldValue("religion", details.religion));
  let cultureValue = $derived(fieldValue("primary_culture", details.primary_culture));
  let techValue = $derived(fieldValue("technology_group", details.technology_group));
  let unitValue = $derived(fieldValue("unit_type", details.unit_type));

  // National focus (none = key absent)
  const FOCI: DropdownItem[] = [
    { key: "NONE", label: "None" },
    { key: "ADM", label: "Administrative (ADM)" },
    { key: "DIP", label: "Diplomatic (DIP)" },
    { key: "MIL", label: "Military (MIL)" },
  ];
  let focusValue = $derived(fieldValue("national_focus", details.national_focus) ?? "NONE");
  let focusEdited = $derived(
    pendingHistField(queue, hf, "national_focus", date) !== undefined,
  );
  function setFocus(key: string) {
    const present = details.national_focus != null;
    if (key === "NONE") {
      if (!present && pendingHistField(queue, hf, "national_focus", date)?.value == null) return;
      // A scalar clear has no dated-block form — it edits the baseline at any
      // date (same rule as the province panel's `clear`).
      queue.push({ label: `Clear national focus of ${tag}`, edits: [removeEdit(hf, "national_focus")] });
      return;
    }
    setField("national_focus", details.national_focus, key, `Set national focus of ${tag}`);
  }

  // Mercantilism (numeric)
  let mercBase = $derived(details.mercantilism != null ? String(details.mercantilism) : null);
  let mercValue = $derived(fieldValue("mercantilism", mercBase));
  let mercEdited = $derived(isEdited("mercantilism", mercBase));
  function setMerc(raw: string) {
    const v = raw.trim();
    if (v === "" || v === mercBase) return;
    pushAtDate(
      queue,
      ctx,
      `Set mercantilism of ${tag}`,
      [scalarEdit(hf, "mercantilism", v, details.mercantilism != null)],
      [`mercantilism = ${v}`],
    );
  }

  // Elector toggle (presence of `elector = yes`)
  let electorField = $derived(pendingHistField(queue, hf, "elector", date));
  let electorOn = $derived(
    electorField !== undefined ? electorField.value === "yes" : details.elector,
  );
  let electorEdited = $derived(electorField !== undefined && (electorField.value === "yes") !== details.elector);
  function toggleElector() {
    const next = !electorOn;
    // Top-level shape keeps the presence semantics (`elector = yes` / key
    // absent); the dated shape assigns `elector = yes|no` so the fold at the
    // selected date sees the change even when earlier blocks set it.
    const startEdits = [next ? listAdd(hf, "elector", "yes") : removeEdit(hf, "elector")];
    pushAtDate(queue, ctx, `${next ? "Set" : "Clear"} ${tag} as elector`, startEdits, [
      `elector = ${next ? "yes" : "no"}`,
    ]);
  }

  // --- Membership lists (reforms / accepted cultures / rivals / friends) ---
  let reformList = $derived(pendingHistList(queue, hf, "add_government_reform", details.government_reforms, date));
  let acceptedList = $derived(
    pendingHistList(queue, hf, "add_accepted_culture", details.accepted_cultures, date, "remove_accepted_culture"),
  );
  let rivalList = $derived(pendingHistList(queue, hf, "historical_rival", details.historical_rivals, date));
  let friendList = $derived(pendingHistList(queue, hf, "historical_friend", details.historical_friends, date));

  function labelFor(items: DropdownItem[], key: string): string {
    return items.find((i) => i.key === key)?.label ?? key;
  }
  function addTo(key: string, value: string, human: string) {
    if (!value) return;
    pushAtDate(queue, ctx, `Add ${human} to ${tag}`, [listAdd(hf, key, value)], [
      `${key} = ${value}`,
    ]);
  }
  function removeFrom(key: string, value: string, human: string) {
    if (key === "add_accepted_culture") {
      // `remove_accepted_culture` is real history vocabulary (vanilla uses it),
      // so a removal that can't surgically strike the top level (the culture
      // came from a dated block, or the date is past the start) writes the
      // dated inverse instead.
      pushAtDate(queue, ctx, `Remove ${human} from ${tag}`, [listRemove(hf, key, value)], [
        `remove_accepted_culture = ${value}`,
      ]);
      return;
    }
    // No dated inverse exists for the other lists — the removal edits the
    // baseline statement in place at any date.
    queue.push({ label: `Remove ${human} from ${tag}`, edits: [listRemove(hf, key, value)] });
  }

  // Capital display (edited via the map's Set Capital tool)
  let capitalField = $derived(pendingHistField(queue, hf, "capital", date));
  let capitalId = $derived(
    capitalField?.value != null ? capitalField.value : (details.capital != null ? String(details.capital) : null),
  );
  let capitalEdited = $derived(capitalField !== undefined);
</script>

<section>
  <h3>Government &amp; Identity</h3>

  <div class="naming">
    <div class="naming-head">
      <span class="naming-label">Naming rules</span>
      <button class="naming-btn" onclick={() => onopennaming?.()}>Naming rules…</button>
    </div>
    {#if namePreview}
      <div class="naming-preview">
        {#if namePreview.matchedKey}
          <span class="resolves">Resolves to</span>
          <button
            class="scheme-jump"
            title="Open this scheme in the Government-names editor"
            onclick={() => onopennaming?.(namePreview!.matchedKey!)}
          >
            <strong>{namePreview.countryName ?? "?"}</strong>
            {#if namePreview.rulerName}<span class="ruler">· {namePreview.rulerName}</span>{/if}
            <span class="scheme-key">{namePreview.matchedKey}</span> ↗
          </button>
          {#if namePreview.approximate}
            <span class="approx" title="An earlier scheme used a condition the toolkit can't evaluate (e.g. has_reform), so the in-game result may differ.">may not be exact</span>
          {/if}
        {:else}
          <span class="dim small">No naming scheme matches this country (uses the game default).</span>
        {/if}
      </div>
    {/if}
  </div>

  <FieldRow label="Government" edited={isEdited("government", details.government)}>
    <SearchDropdown
      items={governments}
      value={govValue}
      placeholder="Government…"
      onselect={(k) => setField("government", details.government, k, `Set government of ${tag}`)}
    />
  </FieldRow>

  <FieldRow label="Government Rank" edited={isEdited("government_rank", rankBase)}>
    <SearchDropdown
      items={RANKS}
      value={rankValue}
      placeholder="Rank…"
      onselect={(k) => setField("government_rank", rankBase, k, `Set rank of ${tag}`)}
    />
  </FieldRow>

  <FieldRow label="Religion" edited={isEdited("religion", details.religion)}>
    <SearchDropdown
      items={religions}
      value={religionValue}
      placeholder="Religion…"
      onselect={(k) => setField("religion", details.religion, k, `Set religion of ${tag}`)}
    />
  </FieldRow>

  <FieldRow label="Primary Culture" edited={isEdited("primary_culture", details.primary_culture)}>
    <SearchDropdown
      items={cultures}
      value={cultureValue}
      placeholder="Culture…"
      onselect={(k) => setField("primary_culture", details.primary_culture, k, `Set culture of ${tag}`)}
    />
  </FieldRow>

  <FieldRow label="Technology Group" edited={isEdited("technology_group", details.technology_group)}>
    <SearchDropdown
      items={techGroups}
      value={techValue}
      placeholder="Tech group…"
      onselect={(k) => setField("technology_group", details.technology_group, k, `Set tech group of ${tag}`)}
    />
  </FieldRow>

  <FieldRow label="Unit Type" edited={isEdited("unit_type", details.unit_type)}>
    <SearchDropdown
      items={techGroups}
      value={unitValue}
      placeholder="(follows tech group)"
      onselect={(k) => setField("unit_type", details.unit_type, k, `Set unit type of ${tag}`)}
    />
  </FieldRow>

  <FieldRow label="National Focus" edited={focusEdited}>
    <SearchDropdown items={FOCI} value={focusValue} onselect={setFocus} />
  </FieldRow>

  <FieldRow label="Mercantilism" edited={mercEdited}>
    <input
      class="num"
      type="number"
      min="0"
      max="100"
      step="1"
      value={mercValue ?? ""}
      placeholder="(none)"
      onchange={(e) => setMerc(e.currentTarget.value)}
    />
  </FieldRow>

  <FieldRow label="Elector (HRE)" edited={electorEdited}>
    <label class="check">
      <input type="checkbox" checked={electorOn} onchange={toggleElector} />
      <span>{electorOn ? "Elector" : "Not an elector"}</span>
    </label>
  </FieldRow>

  <FieldRow label="Capital" edited={capitalEdited}>
    <span class="capital">
      {#if capitalId}
        {capitalEdited ? `#${capitalId}` : (details.capital_name ?? "?")}
        <span class="dim">(#{capitalId})</span>
      {:else}
        <span class="dim">none</span>
      {/if}
    </span>
    <span class="tool-hint">Set via the map's ★ Set Capital tool</span>
  </FieldRow>

  <div class="list-field">
    <div class="list-label">
      Government Reforms
      {#if onopenmechanics}
        <button class="edit-def" title="Create a new government reform definition" onclick={() => onopenmechanics?.("government_reforms")}>＋ new…</button>
      {/if}
    </div>
    {#each reformList as r (r)}
      <span class="chip">
        {labelFor(reforms, r)}
        {#if onopenmechanics}
          <button class="def" title="Edit this reform's definition" onclick={() => onopenmechanics?.("government_reforms", r)}>✎</button>
        {/if}
        <button class="x" onclick={() => removeFrom("add_government_reform", r, "reform")} aria-label="Remove">×</button>
      </span>
    {/each}
    <SearchDropdown
      items={reforms}
      value={null}
      placeholder="Add reform…"
      onselect={(k) => addTo("add_government_reform", k, "reform")}
    />
  </div>

  <div class="list-field">
    <div class="list-label">Accepted Cultures</div>
    {#each acceptedList as c (c)}
      <span class="chip">
        {labelFor(cultures, c)}
        <button class="x" onclick={() => removeFrom("add_accepted_culture", c, "accepted culture")} aria-label="Remove">×</button>
      </span>
    {/each}
    <SearchDropdown
      items={cultures}
      value={null}
      placeholder="Add accepted culture…"
      onselect={(k) => addTo("add_accepted_culture", k, "accepted culture")}
    />
  </div>

  <div class="list-field">
    <div class="list-label">Historical Rivals</div>
    {#each rivalList as t (t)}
      <span class="chip">
        {labelFor(countries, t)}
        <button class="x" onclick={() => removeFrom("historical_rival", t, "rival")} aria-label="Remove">×</button>
      </span>
    {/each}
    <SearchDropdown
      items={countries}
      value={null}
      placeholder="Add rival…"
      onselect={(k) => addTo("historical_rival", k, "rival")}
    />
  </div>

  <div class="list-field">
    <div class="list-label">Historical Friends</div>
    {#each friendList as t (t)}
      <span class="chip">
        {labelFor(countries, t)}
        <button class="x" onclick={() => removeFrom("historical_friend", t, "friend")} aria-label="Remove">×</button>
      </span>
    {/each}
    <SearchDropdown
      items={countries}
      value={null}
      placeholder="Add friend…"
      onselect={(k) => addTo("historical_friend", k, "friend")}
    />
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
    color: var(--text-2);
  }

  .num {
    width: 4rem;
    background: var(--bg-1);
    border: 1px solid var(--border);
    color: var(--text-1);
    font-family: inherit;
    font-size: 0.85rem;
    padding: 0.25rem 0.4rem;
    outline: none;
  }

  .check {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    font-size: 0.85rem;
    cursor: pointer;
  }

  .capital {
    font-size: 0.85rem;
  }

  .dim {
    color: var(--text-2);
  }

  .tool-hint {
    font-size: 0.68rem;
    color: var(--text-2);
  }

  .list-field {
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
    margin-bottom: 0.7rem;
  }

  .list-label {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    font-size: 0.75rem;
    text-transform: uppercase;
    letter-spacing: 0.03em;
    color: var(--text-2);
  }

  .edit-def {
    border: 1px solid var(--border-strong);
    background: var(--bg-2);
    color: var(--text-1);
    font-family: inherit;
    font-size: 0.68rem;
    text-transform: none;
    letter-spacing: 0;
    padding: 0.05rem 0.4rem;
    cursor: pointer;
  }
  .edit-def:hover {
    border-color: var(--accent);
    background: var(--accent);
    color: var(--text-inverse);
  }

  .def {
    border: none;
    background: transparent;
    color: var(--text-2);
    cursor: pointer;
    font-size: 0.8rem;
    line-height: 1;
    padding: 0 0.1rem;
  }
  .def:hover {
    color: var(--ok);
  }

  .chip {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    align-self: flex-start;
    background: var(--bg-1);
    border: 1px solid var(--border);
    color: var(--text-1);
    font-size: 0.8rem;
    padding: 0.12rem 0.2rem 0.12rem 0.45rem;
  }

  .x {
    border: none;
    background: transparent;
    color: var(--text-2);
    cursor: pointer;
    font-size: 0.95rem;
    line-height: 1;
    padding: 0 0.2rem;
  }

  .x:hover {
    color: var(--err);
  }

  .naming {
    margin-bottom: 0.7rem;
    padding: 0.35rem 0.45rem;
    background: var(--bg-1);
    border: 1px solid var(--border);
  }
  .naming-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
  }
  .naming-label {
    font-size: 0.72rem;
    text-transform: uppercase;
    letter-spacing: 0.03em;
    color: var(--text-2);
  }
  .naming-btn {
    border: 1px solid var(--border-strong);
    background: var(--bg-2);
    color: var(--text-1);
    font-family: inherit;
    font-size: 0.74rem;
    padding: 0.1rem 0.45rem;
    cursor: pointer;
  }
  .naming-btn:hover {
    border-color: var(--accent);
    background: var(--accent);
    color: var(--text-inverse);
  }
  .naming-preview {
    margin-top: 0.35rem;
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 0.35rem;
    font-size: 0.82rem;
  }
  .resolves {
    color: var(--text-2);
    font-size: 0.74rem;
  }
  .scheme-jump {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    border: none;
    background: transparent;
    color: var(--text-1);
    font-family: inherit;
    font-size: 0.82rem;
    padding: 0;
    cursor: pointer;
    text-align: left;
  }
  .scheme-jump:hover {
    color: var(--text-inverse);
  }
  .scheme-jump strong {
    color: var(--text-1);
  }
  .ruler {
    color: var(--text-2);
  }
  .scheme-key {
    color: var(--ok);
    background: var(--bg-0);
    padding: 0 0.3rem;
    font-size: 0.72rem;
  }
  .approx {
    color: var(--warn);
    font-size: 0.7rem;
    border: 1px solid var(--warn);
    padding: 0.02rem 0.3rem;
  }
  .dim {
    color: var(--text-2);
  }
  .small {
    font-size: 0.76rem;
  }
</style>
