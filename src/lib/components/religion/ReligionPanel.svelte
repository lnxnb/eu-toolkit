<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { SidePanel, ColorPicker, DatePicker, SearchDropdown, NewGroupModal, IconImportButton, AtlasIcon, LoadingState, NEW_GROUP_KEY } from "$lib/components/ui";
  import type { DropdownItem, KnownModifier, ModifierRow, GroupScaffold, NewGroupResult } from "$lib/components/ui";
  import type { EditQueue, TypedEdit } from "$lib/edits.svelte";
  import FieldRow from "../country/FieldRow.svelte";
  import ModifierBlock from "./ModifierBlock.svelte";
  import StringListEditor from "./StringListEditor.svelte";
  import IconPicker from "./IconPicker.svelte";
  import {
    featureLabel,
    FEATURE_KEYS,
    type ReligionDetails,
    type ReligionGroupEntry,
  } from "./types";

  let {
    installPath,
    modPath,
    religionKey,
    queue,
    seed = null,
    onclose,
    oncolor,
    onjumpcountry,
    onjumpprovince,
    onopenmechanics,
  }: {
    installPath: string;
    modPath: string | null;
    religionKey: string;
    queue: EditQueue;
    /** Details for a pending-created religion not yet on disk (fetch will 404). */
    seed?: ReligionDetails | null;
    onclose: () => void;
    /** Live map repaint: (religionKey, rgb|null) — null clears the override. */
    oncolor?: (key: string, rgb: [number, number, number] | null) => void;
    /** Jump to a country in political mode. */
    onjumpcountry?: (tag: string) => void;
    /** Jump to a province in provinces mode. */
    onjumpprovince?: (id: number) => void;
    /** Open the Sprint 26 mechanics overlay on a given family (advanced section). */
    onopenmechanics?: (family: string) => void;
  } = $props();

  // --- Sprint 26 advanced mechanics: which sub-mechanic families this religion
  //     uses (derived from its feature flags + group), each opening the
  //     config-driven Mechanics overlay filtered to that family. ---
  const MECH_FAMILY_LABELS: Record<string, string> = {
    personal_deities: "Personal deities",
    church_aspects: "Church aspects",
    fetishist_cults: "Fetishist cults",
    fervor: "Fervor aspects",
    isolationism: "Isolationism tiers",
    incidents: "Incidents (Shinto)",
    religious_reforms: "Religious reforms",
    religious_schools: "Religious schools",
    holy_orders: "Holy orders",
    golden_bulls: "Golden bulls",
  };
  const relevantMechanics = $derived.by<string[]>(() => {
    const feats = new Set(details?.features ?? []);
    const grp = group;
    const out: string[] = [];
    const add = (f: string) => {
      if (!out.includes(f)) out.push(f);
    };
    if (feats.has("personal_deity")) add("personal_deities");
    if (feats.has("uses_church_power") || feats.has("uses_anglican_power") || feats.has("uses_hussite_power") || feats.has("uses_judaism_power")) add("church_aspects");
    if (feats.has("fetishist_cult")) add("fetishist_cults");
    if (feats.has("fervor")) add("fervor");
    if (feats.has("uses_isolationism")) { add("isolationism"); add("incidents"); }
    if (feats.has("religious_reforms")) add("religious_reforms");
    if (religionKey === "shinto") add("incidents");
    if (religionKey === "catholic") add("golden_bulls");
    if (grp === "muslim") add("religious_schools");
    if (grp === "christian") add("holy_orders");
    return out;
  });

  let details = $state<ReligionDetails | null>(null);
  let error = $state("");
  let groups = $state<ReligionGroupEntry[]>([]);
  let known = $state<KnownModifier[]>([]);

  // Group can change within the session (move-to-group); edits target the current.
  let movedGroup = $state<string | null>(null);
  let group = $derived(movedGroup ?? details?.group_key ?? "");
  let file = $derived(details?.source_file ?? "");

  // Seed-once local state for the toggle/date editors (panel is remounted per
  // religion by the parent {#key}, so this resets on selection change).
  let seeded = false;
  let hasDate = $state(false);
  let dateVal = $state("1444.11.11");
  let featureSet = $state<Set<string>>(new Set());

  let pendingName = $derived(queue.pendingLocOverride(religionKey));
  let titleName = $derived(pendingName ?? details?.localized_name ?? religionKey);

  function parseTriple(s: string): [number, number, number] | null {
    const p = s.trim().split(/\s+/).map(Number);
    return p.length >= 3 && p.every((n) => Number.isFinite(n)) ? [p[0], p[1], p[2]] : null;
  }
  function css(c: [number, number, number] | null): string {
    return c ? `rgb(${c[0]}, ${c[1]}, ${c[2]})` : "transparent";
  }

  // --- Details fetch (staleness-guarded; falls back to the seed for a pending
  //     religion that isn't on disk yet) ---
  $effect(() => {
    const current = religionKey;
    details = null;
    error = "";
    seeded = false;
    invoke<ReligionDetails>("get_religion_details", { installPath, modPath, key: current })
      .then((d) => {
        if (current === religionKey) details = d;
      })
      .catch((e) => {
        if (current !== religionKey) return;
        if (seed && seed.key === current) details = seed;
        else error = String(e);
      });
  });

  // Seed the local toggle/date state once details arrive.
  $effect(() => {
    if (details && !seeded) {
      seeded = true;
      hasDate = details.enable_date != null;
      dateVal = details.enable_date ?? "1444.11.11";
      featureSet = new Set(details.features);
    }
  });

  // Shared option lists.
  $effect(() => {
    invoke<ReligionGroupEntry[]>("list_religion_groups", { installPath, modPath })
      .then((v) => (groups = v))
      .catch(() => {});
    invoke<KnownModifier[]>("get_known_modifiers")
      .then((k) => (known = k))
      .catch(() => {});
  });

  // --- Color (live map repaint) ---
  let pendingColorStr = $derived(details ? queue.pendingBlockValue(file, [group, religionKey, "color"]) : undefined);
  let colorEdited = $derived(pendingColorStr !== undefined);
  let effectiveColor = $derived<[number, number, number] | null>(
    (pendingColorStr ? parseTriple(pendingColorStr) : null) ?? details?.color ?? null,
  );
  let colorRGB = $derived({
    r: effectiveColor?.[0] ?? 128,
    g: effectiveColor?.[1] ?? 128,
    b: effectiveColor?.[2] ?? 128,
  });

  // Push the pending color up to the map (queue stays the source of truth). A
  // just-created religion (seed) needs its color painted even before an edit.
  $effect(() => {
    if (!details) return;
    const isSeedPending = seed != null && seed.key === religionKey;
    if (colorEdited) oncolor?.(religionKey, effectiveColor);
    else if (isSeedPending) oncolor?.(religionKey, effectiveColor);
    else oncolor?.(religionKey, null);
  });
  // Clear the override on unmount.
  $effect(() => {
    const k = religionKey;
    return () => oncolor?.(k, null);
  });

  function commitColor(c: { r: number; g: number; b: number }) {
    queue.push({
      label: `Set color of ${religionKey}`,
      edits: [{ kind: "setBlock", file, path: [group, religionKey, "color"], value: `${c.r} ${c.g} ${c.b}` }],
      coalesceKey: `relcolor:${religionKey}`,
    });
  }

  // --- Name ---
  function commitName(v: string) {
    queue.push({
      label: `Rename ${religionKey}`,
      edits: [{ kind: "locOverride", key: religionKey, value: v }],
      coalesceKey: `relname:${religionKey}`,
    });
  }

  // --- Icon ---
  let pendingIconStr = $derived(details ? queue.pendingScalar(file, [group, religionKey, "icon"]) : undefined);
  let effectiveIcon = $derived(
    pendingIconStr != null ? parseInt(pendingIconStr, 10) : (details?.icon ?? null),
  );
  function pickIcon(frame: number) {
    const value = String(frame + 1);
    const present = details?.icon != null;
    queue.push({
      label: `Set icon of ${religionKey}`,
      edits: [
        present
          ? { kind: "setScalar", file, path: [group, religionKey, "icon"], value, quoted: false }
          : { kind: "insertStatement", file, blockPath: [group, religionKey], statement: `icon = ${value}` },
      ],
      coalesceKey: `relicon:${religionKey}`,
    });
  }

  // --- Group move ---
  // The dropdown gets a trailing "＋ New group…" sentinel that opens NewGroupModal.
  let groupItems = $derived<DropdownItem[]>([
    ...groups.map((g) => ({ key: g.key, label: g.name })),
    { key: NEW_GROUP_KEY, label: "＋ New group…" },
  ]);
  let newGroupOpen = $state(false);
  // Bumped to remount the group dropdown so a "＋ New group…" pick (a sentinel,
  // not a real value) never sticks in its display.
  let groupPickerNonce = $state(0);
  function moveToGroup(newGroup: string) {
    if (newGroup === NEW_GROUP_KEY) {
      groupPickerNonce++;
      if (details?.raw_block_text) newGroupOpen = true;
      return;
    }
    if (!details || newGroup === group) return;
    if (!details.raw_block_text) return; // pending-created religion: no faithful text to move
    queue.push({
      label: `Move ${religionKey} to ${newGroup}`,
      edits: [
        { kind: "removeStatement", file, blockPath: [group], key: religionKey },
        { kind: "insertStatement", file, blockPath: [newGroup], statement: details.raw_block_text },
      ],
    });
    movedGroup = newGroup;
  }

  // Create a new group (pending edit) and move this religion into it. The group
  // block is inserted into this religion's file so the move stays same-file and
  // composes on the evolving buffer (list-creation ordering rule).
  async function createGroupAndMove(res: NewGroupResult) {
    if (!details || !details.raw_block_text) return;
    try {
      const scaffold = await invoke<GroupScaffold>("prepare_religion_group_scaffold", {
        installPath,
        modPath,
        siblingGroupKey: res.sibling,
        name: res.name,
        existingKeys: groups.map((g) => g.key),
      });
      queue.push({
        label: `Move ${religionKey} to new group ${res.name}`,
        edits: [
          { kind: "insertStatement", file, blockPath: [], statement: scaffold.block },
          { kind: "locOverride", key: scaffold.group_key, value: scaffold.group_name },
          { kind: "removeStatement", file, blockPath: [group], key: religionKey },
          { kind: "insertStatement", file, blockPath: [scaffold.group_key], statement: details.raw_block_text },
        ],
      });
      groups = [...groups, { key: scaffold.group_key, name: scaffold.group_name }];
      movedGroup = scaffold.group_key;
    } catch (e) {
      error = String(e);
    }
  }

  // --- Modifier blocks ---
  function blockValue(rows: ModifierRow[]): string {
    return rows.map((r) => `${r.key} = ${r.value}`).join(" ");
  }
  function commitMod(sub: "country" | "province", rows: ModifierRow[]) {
    queue.push({
      label: `Edit ${sub} modifiers of ${religionKey}`,
      edits: [{ kind: "setBlock", file, path: [group, religionKey, sub], value: blockValue(rows) }],
      coalesceKey: `relmod:${religionKey}:${sub}`,
    });
  }

  // --- Heretics ---
  function commitHeretics(items: string[]) {
    queue.push({
      label: `Edit heretics of ${religionKey}`,
      edits: [{ kind: "setBlock", file, path: [group, religionKey, "heretic"], value: items.join(" ") }],
      coalesceKey: `relheretic:${religionKey}`,
    });
  }

  // --- Enable date ---
  function setDate(v: string) {
    dateVal = v;
    const present = details?.enable_date != null;
    queue.push({
      label: `Set enable date of ${religionKey}`,
      edits: [
        present
          ? { kind: "setScalar", file, path: [group, religionKey, "date"], value: v, quoted: false }
          : { kind: "insertStatement", file, blockPath: [group, religionKey], statement: `date = ${v}` },
      ],
      coalesceKey: `reldate:${religionKey}`,
    });
  }
  function addDate() {
    hasDate = true;
    setDate(dateVal || "1444.11.11");
  }
  function removeDate() {
    hasDate = false;
    queue.push({
      label: `Remove enable date of ${religionKey}`,
      edits: [{ kind: "removeStatement", file, blockPath: [group, religionKey], key: "date" }],
    });
  }

  // --- Feature toggles ---
  function toggleFeature(k: string, on: boolean) {
    const next = new Set(featureSet);
    if (on) next.add(k);
    else next.delete(k);
    featureSet = next;
    queue.push({
      label: `${on ? "Enable" : "Disable"} ${featureLabel(k)} for ${religionKey}`,
      edits: [
        on
          ? { kind: "insertStatement", file, blockPath: [group, religionKey], statement: `${k} = yes` }
          : ({ kind: "removeStatement", file, blockPath: [group, religionKey], key: k } as TypedEdit),
      ],
    });
  }

  // Feature keys to show: the curated known set, plus any present feature outside it.
  let featureRows = $derived.by(() => {
    const set = new Set([...FEATURE_KEYS, ...featureSet]);
    return [...set];
  });
</script>

<SidePanel title={titleName} {onclose}>
  {#snippet header()}
    <div class="head">
      {#if details?.icon}<AtlasIcon {installPath} {modPath} kind="religions" frame={Math.max(0, details.icon - 1)} size={32} label={`${titleName} icon`} />{/if}
      <span class="swatch" style="background: {css(effectiveColor)}"></span>
      <span class="key-chip">{religionKey}{group ? ` · ${group}` : ""}</span>
    </div>
  {/snippet}

  {#if error}
    <p class="error">{error}</p>
  {:else if !details}
    <LoadingState label="Loading religion…" />
  {:else}
    <section>
      <h3>Identity</h3>
      <FieldRow label="Name" edited={pendingName !== undefined}>
        <input
          class="text"
          value={titleName}
          oninput={(e) => commitName((e.target as HTMLInputElement).value)}
        />
      </FieldRow>
      <FieldRow label="Key">
        <span class="mono">{religionKey}</span>
      </FieldRow>
      <FieldRow label="Color" edited={colorEdited}>
        <ColorPicker value={colorRGB} onchange={commitColor} />
        <span class="mono">rgb({colorRGB.r}, {colorRGB.g}, {colorRGB.b})</span>
      </FieldRow>
      <FieldRow label="Icon" edited={pendingIconStr !== undefined}>
        <div class="icon-row">
          <IconPicker
            {installPath}
            {modPath}
            current={effectiveIcon != null ? effectiveIcon - 1 : null}
            onpick={pickIcon}
          />
          <IconImportButton
            {installPath}
            {modPath}
            {queue}
            kind="religions"
            frame={effectiveIcon != null ? effectiveIcon - 1 : -1}
            label="Import art…"
          />
        </div>
      </FieldRow>
      {#if effectiveIcon == null}
        <p class="dim small">Pick an icon slot above before importing custom art.</p>
      {/if}
      <FieldRow label="Group" edited={movedGroup !== null}>
        {#key groupPickerNonce}
          <SearchDropdown
            items={groupItems}
            value={group}
            placeholder="Move to group…"
            onselect={(k) => moveToGroup(k)}
          />
        {/key}
      </FieldRow>
    </section>

    <section>
      <h3>Country modifiers</h3>
      <ModifierBlock base={details.country_modifiers} {known} oncommit={(r) => commitMod("country", r)} />
    </section>

    <section>
      <h3>Province modifiers</h3>
      <ModifierBlock base={details.province_modifiers} {known} oncommit={(r) => commitMod("province", r)} />
    </section>

    <section>
      <h3>Heretics</h3>
      <StringListEditor base={details.heretics} placeholder="Add heretic (e.g. LOLLARD)…" oncommit={commitHeretics} />
    </section>

    <section>
      <h3>Enable date</h3>
      {#if hasDate}
        <div class="row">
          <DatePicker value={dateVal} onchange={setDate} />
          <button class="btn" onclick={removeDate}>Remove</button>
        </div>
      {:else}
        <button class="btn" onclick={addDate}>Add enable date…</button>
      {/if}
    </section>

    <section>
      <h3>Features</h3>
      <div class="features">
        {#each featureRows as k (k)}
          <label class="feat">
            <input
              type="checkbox"
              checked={featureSet.has(k)}
              onchange={(e) => toggleFeature(k, (e.target as HTMLInputElement).checked)}
            />
            {featureLabel(k)}
          </label>
        {/each}
      </div>
    </section>

    {#if onopenmechanics}
      <section>
        <h3>Advanced mechanics</h3>
        <p class="dim small">Edit this religion's sub-mechanics (typed editors, not raw).</p>
        <div class="mechlinks">
          {#each relevantMechanics as fam (fam)}
            <button class="mechlink used" onclick={() => onopenmechanics?.(fam)} title="Used by this religion">
              {MECH_FAMILY_LABELS[fam] ?? fam}
            </button>
          {/each}
          {#each Object.keys(MECH_FAMILY_LABELS).filter((f) => !relevantMechanics.includes(f)) as fam (fam)}
            <button class="mechlink" onclick={() => onopenmechanics?.(fam)}>
              {MECH_FAMILY_LABELS[fam]}
            </button>
          {/each}
        </div>
      </section>
    {/if}

    {#if details.raw_remainder.length > 0}
      <section>
        <h3>Advanced (read-only)</h3>
        <p class="dim small">Unmodeled content — preserved untouched on save.</p>
        <ul class="raw">
          {#each details.raw_remainder as r (r.key)}
            <li><span class="mono">{r.key}</span> = {r.value}</li>
          {/each}
        </ul>
      </section>
    {/if}

    <section>
      <h3>Usage at 1444</h3>
      <FieldRow label="Countries">
        <span>{details.country_count}</span>
      </FieldRow>
      {#if details.sample_tags.length > 0}
        <div class="jumps">
          {#each details.sample_tags as t (t)}
            <button class="link" onclick={() => onjumpcountry?.(t)}>{t}</button>
          {/each}
        </div>
      {/if}
      <FieldRow label="Provinces">
        <span>{details.province_count}</span>
      </FieldRow>
      {#if details.sample_provinces.length > 0}
        <div class="jumps">
          {#each details.sample_provinces as id (id)}
            <button class="link" onclick={() => onjumpprovince?.(id)}>#{id}</button>
          {/each}
        </div>
      {/if}
    </section>
  {/if}
</SidePanel>

<NewGroupModal
  bind:open={newGroupOpen}
  kind="religion"
  {installPath}
  {modPath}
  {groups}
  defaultSibling={group}
  entityLabel={titleName}
  onconfirm={createGroupAndMove}
  oncancel={() => (newGroupOpen = false)}
/>

<style>
  .head {
    display: flex;
    align-items: center;
    gap: 0.6rem;
  }
  .swatch {
    width: 1rem;
    height: 1rem;
    display: inline-block;
    border: 1px solid var(--border);
  }
  .key-chip {
    font-size: 0.8rem;
    color: var(--text-2);
  }
  section {
    padding: 0.4rem 0 0.6rem;
    border-bottom: 1px solid var(--bg-1);
  }
  h3 {
    margin: 0 0 0.4rem;
    font-size: 0.8rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-2);
  }
  .text,
  .mono {
    font-size: 0.85rem;
  }
  .text {
    width: 100%;
    background: var(--bg-0);
    border: 1px solid var(--border-strong);
    color: var(--text-1);
    font-family: inherit;
    padding: 0.2rem 0.4rem;
  }
  .mono {
    font-family: ui-monospace, monospace;
    color: var(--text-2);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }
  .btn {
    border: 1px solid var(--border-strong);
    background: transparent;
    color: var(--text-1);
    font-family: inherit;
    font-size: 0.8rem;
    padding: 0.2rem 0.6rem;
    cursor: pointer;
  }
  .btn:hover {
    border-color: var(--text-2);
  }
  .mechlinks {
    display: flex;
    flex-wrap: wrap;
    gap: 0.3rem;
  }
  .mechlink {
    border: 1px solid var(--bg-3);
    background: var(--bg-1);
    color: var(--text-1);
    font-family: inherit;
    font-size: 0.74rem;
    padding: 0.12rem 0.4rem;
    cursor: pointer;
  }
  .mechlink:hover {
    border-color: var(--accent);
    background: var(--bg-3);
    color: var(--text-inverse);
  }
  .mechlink.used {
    border-color: var(--ok);
    color: var(--ok);
  }
  .features {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.2rem 0.6rem;
  }
  .feat {
    display: flex;
    align-items: center;
    gap: 0.3rem;
    font-size: 0.8rem;
    color: var(--text-1);
  }
  .raw {
    list-style: none;
    margin: 0;
    padding: 0;
    font-size: 0.8rem;
    color: var(--text-1);
  }
  .raw li {
    padding: 0.1rem 0;
  }
  .jumps {
    display: flex;
    flex-wrap: wrap;
    gap: 0.3rem;
    margin: 0.2rem 0 0.4rem;
  }
  .link {
    border: 1px solid var(--border-strong);
    background: var(--bg-2);
    color: var(--accent-text);
    font-family: inherit;
    font-size: 0.75rem;
    padding: 0.1rem 0.4rem;
    cursor: pointer;
  }
  .link:hover {
    border-color: var(--accent);
    color: var(--text-inverse);
  }
  .icon-row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    flex-wrap: wrap;
  }
  .dim {
    color: var(--text-2);
  }
  .small {
    font-size: 0.75rem;
  }
  .error {
    color: var(--err);
  }
</style>
