<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { SidePanel, ColorPicker, SearchDropdown, NewGroupModal, NEW_GROUP_KEY } from "$lib/components/ui";
  import type { DropdownItem, RGB, GroupScaffold, NewGroupResult } from "$lib/components/ui";
  import type { EditQueue, TypedEdit } from "$lib/edits.svelte";
  import FieldRow from "../country/FieldRow.svelte";
  import NamePoolEditor from "./NamePoolEditor.svelte";
  import ProvinceNamesSection from "../ProvinceNamesSection.svelte";
  import DynastyModal from "../DynastyModal.svelte";
  import { poolBlockValue, type CultureDetails, type CultureGroupEntry } from "./types";

  let {
    installPath,
    modPath,
    cultureKey,
    queue,
    seed = null,
    provNamePick = null,
    onclose,
    oncolor,
    onjumpcountry,
    onjumpprovince,
    onarmprovnamepick,
    onprovnamepickconsumed,
  }: {
    installPath: string;
    modPath: string | null;
    cultureKey: string;
    queue: EditQueue;
    /** Details for a pending-created culture not yet on disk (fetch will 404). */
    seed?: CultureDetails | null;
    /** Province id picked on the map for a province-names section, or null. */
    provNamePick?: number | null;
    onclose: () => void;
    /** Live map recolor for the display-color override: (key, rgb|null). */
    oncolor?: (key: string, rgb: [number, number, number] | null) => void;
    onjumpcountry?: (tag: string) => void;
    onjumpprovince?: (id: number) => void;
    /** Arm the map "pick a province" tool for a province-names section. */
    onarmprovnamepick?: () => void;
    /** Reset the pick request once a section consumes it. */
    onprovnamepickconsumed?: () => void;
  } = $props();

  interface CountryBrief {
    tag: string;
    name: string;
    color: [number, number, number] | null;
  }
  interface RegistryEntry {
    key: string;
    name: string;
  }

  let details = $state<CultureDetails | null>(null);
  let error = $state("");
  let groups = $state<CultureGroupEntry[]>([]);
  let countries = $state<CountryBrief[]>([]);
  let graphicalCultures = $state<RegistryEntry[]>([]);

  // Group can change within the session (move-to-group); edits target the current.
  let movedGroup = $state<string | null>(null);
  let group = $derived(movedGroup ?? details?.group_key ?? "");
  let file = $derived(details?.source_file ?? "");

  // Seed-once local state (panel is remounted per culture by the parent {#key}).
  let seeded = false;
  let hasPrimary = $state(false);
  let primaryVal = $state<string | null>(null);
  let dynastyList = $state<string[]>([]);
  let dynastyRev = $state(0);
  let showGroup = $state(false);
  let dynModalOpen = $state(false);

  // Display-color override (toolkit DB, not the mod). Loaded per culture.
  let overrideColor = $state<[number, number, number] | null>(null);

  let pendingName = $derived(queue.pendingLocOverride(cultureKey));
  let titleName = $derived(pendingName ?? details?.localized_name ?? cultureKey);

  function css(c: [number, number, number] | null): string {
    return c ? `rgb(${c[0]}, ${c[1]}, ${c[2]})` : "transparent";
  }

  // --- Details fetch (staleness-guarded; falls back to the seed) ---
  $effect(() => {
    const current = cultureKey;
    details = null;
    error = "";
    seeded = false;
    invoke<CultureDetails>("get_culture_details", { installPath, modPath, key: current })
      .then((d) => {
        if (current === cultureKey) details = d;
      })
      .catch((e) => {
        if (current !== cultureKey) return;
        if (seed && seed.key === current) details = seed;
        else error = String(e);
      });
  });

  // Seed the local field state once details arrive.
  $effect(() => {
    if (details && !seeded) {
      seeded = true;
      hasPrimary = details.primary != null;
      primaryVal = details.primary;
      dynastyList = [...details.dynasty_names];
    }
  });

  // Load the display-color override for this culture.
  $effect(() => {
    const current = cultureKey;
    invoke<[number, number, number] | null>("get_culture_color_override", {
      modPath,
      key: current,
    })
      .then((c) => {
        if (current === cultureKey) overrideColor = c;
      })
      .catch(() => {});
  });

  // Shared option lists.
  $effect(() => {
    invoke<CultureGroupEntry[]>("list_culture_groups", { installPath, modPath })
      .then((v) => (groups = v))
      .catch(() => {});
    invoke<CountryBrief[]>("list_countries", { installPath, modPath })
      .then((v) => (countries = v))
      .catch(() => {});
    invoke<RegistryEntry[]>("get_registry", {
      name: "graphical_cultures",
      installPath,
      modPath,
    })
      .then((v) => (graphicalCultures = v))
      .catch(() => {});
  });

  // --- Name (loc override) ---
  function commitName(v: string) {
    queue.push({
      label: `Rename ${cultureKey}`,
      edits: [{ kind: "locOverride", key: cultureKey, value: v }],
      coalesceKey: `culname:${cultureKey}`,
    });
  }

  // --- Display color override (immediate; not part of the pending queue) ---
  let colorRGB = $derived<RGB>({
    r: overrideColor?.[0] ?? 150,
    g: overrideColor?.[1] ?? 150,
    b: overrideColor?.[2] ?? 150,
  });
  function commitColor(c: RGB) {
    overrideColor = [c.r, c.g, c.b];
    invoke("set_culture_color_override", {
      modPath,
      key: cultureKey,
      r: c.r,
      g: c.g,
      b: c.b,
    }).catch((e) => (error = String(e)));
    oncolor?.(cultureKey, [c.r, c.g, c.b]);
  }
  function clearColor() {
    overrideColor = null;
    invoke("clear_culture_color_override", { modPath, key: cultureKey }).catch(
      (e) => (error = String(e)),
    );
    oncolor?.(cultureKey, null);
  }

  // --- Primary nation (`primary = TAG`) ---
  let countryItems = $derived<DropdownItem[]>(
    countries.map((c) => ({
      key: c.tag,
      label: `${c.name} (${c.tag})`,
      swatch: c.color ? `rgb(${c.color[0]}, ${c.color[1]}, ${c.color[2]})` : undefined,
    })),
  );
  function setPrimary(tag: string) {
    const present = primaryVal != null;
    primaryVal = tag;
    hasPrimary = true;
    queue.push({
      label: `Set primary nation of ${cultureKey}`,
      edits: [
        present
          ? { kind: "setScalar", file, path: [group, cultureKey, "primary"], value: tag, quoted: false }
          : { kind: "insertStatement", file, blockPath: [group, cultureKey], statement: `primary = ${tag}` },
      ],
      coalesceKey: `culprimary:${cultureKey}`,
    });
  }
  function removePrimary() {
    hasPrimary = false;
    primaryVal = null;
    queue.push({
      label: `Remove primary nation of ${cultureKey}`,
      edits: [{ kind: "removeStatement", file, blockPath: [group, cultureKey], key: "primary" }],
    });
  }

  // --- Name pools (setBlock whole pool; insert when the block is absent) ---
  function poolEdit(
    blockPath: string[],
    poolKey: string,
    present: boolean,
    names: string[],
  ): TypedEdit {
    const value = poolBlockValue(names);
    return present
      ? { kind: "setBlock", file, path: [...blockPath, poolKey], value }
      : { kind: "insertStatement", file, blockPath, statement: `${poolKey} = { ${value} }` };
  }
  function commitCulturePool(
    poolKey: "male_names" | "female_names" | "dynasty_names",
    present: boolean,
    names: string[],
  ) {
    queue.push({
      label: `Edit ${poolKey.replace("_", " ")} of ${cultureKey}`,
      edits: [poolEdit([group, cultureKey], poolKey, present, names)],
      coalesceKey: `culpool:${cultureKey}:${poolKey}`,
    });
  }
  function commitGroupPool(
    poolKey: "male_names" | "female_names" | "dynasty_names",
    present: boolean,
    names: string[],
  ) {
    queue.push({
      label: `Edit group ${poolKey.replace("_", " ")} of ${group}`,
      edits: [poolEdit([group], poolKey, present, names)],
      coalesceKey: `culgrouppool:${group}:${poolKey}`,
    });
  }

  // Dynasty browse-append (DynastyModal in pick mode).
  function appendDynasty(name: string) {
    const n = name.trim();
    if (!n || dynastyList.includes(n)) return;
    dynastyList = [...dynastyList, n];
    dynastyRev++;
    commitCulturePool("dynasty_names", details?.dynasty_names_present ?? false, dynastyList);
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
    if (!details.raw_block_text) return; // pending-created culture: no faithful text to move
    queue.push({
      label: `Move ${cultureKey} to ${newGroup}`,
      edits: [
        { kind: "removeStatement", file, blockPath: [group], key: cultureKey },
        { kind: "insertStatement", file, blockPath: [newGroup], statement: details.raw_block_text },
      ],
    });
    movedGroup = newGroup;
  }

  // Create a new culture group (pending edit) and move this culture into it. The
  // group block goes into this culture's file so the move stays same-file and
  // composes on the evolving buffer (AGENTS.md list-creation ordering).
  async function createGroupAndMove(res: NewGroupResult) {
    if (!details || !details.raw_block_text) return;
    try {
      const scaffold = await invoke<GroupScaffold>("prepare_culture_group_scaffold", {
        installPath,
        modPath,
        siblingGroupKey: res.sibling,
        name: res.name,
        graphicalCulture: res.graphicalCulture ?? "",
        existingKeys: groups.map((g) => g.key),
      });
      queue.push({
        label: `Move ${cultureKey} to new group ${res.name}`,
        edits: [
          { kind: "insertStatement", file, blockPath: [], statement: scaffold.block },
          { kind: "locOverride", key: scaffold.group_key, value: scaffold.group_name },
          { kind: "removeStatement", file, blockPath: [group], key: cultureKey },
          { kind: "insertStatement", file, blockPath: [scaffold.group_key], statement: details.raw_block_text },
        ],
      });
      groups = [...groups, { key: scaffold.group_key, name: scaffold.group_name }];
      movedGroup = scaffold.group_key;
    } catch (e) {
      error = String(e);
    }
  }

  // --- Group-level graphical culture ---
  let gfxItems = $derived<DropdownItem[]>(
    graphicalCultures.map((g) => ({ key: g.key, label: g.name })),
  );
  let groupGfx = $state<string | null>(null);
  let groupGfxSeeded = false;
  $effect(() => {
    if (details && !groupGfxSeeded) {
      groupGfxSeeded = true;
      groupGfx = details.group_graphical_culture;
    }
  });
  function setGroupGfx(key: string) {
    const present = groupGfx != null;
    groupGfx = key;
    queue.push({
      label: `Set graphical culture of ${group}`,
      edits: [
        present
          ? { kind: "setScalar", file, path: [group, "graphical_culture"], value: key, quoted: false }
          : { kind: "insertStatement", file, blockPath: [group], statement: `graphical_culture = ${key}` },
      ],
      coalesceKey: `culgfx:${group}`,
    });
  }
</script>

<SidePanel title={titleName} {onclose}>
  {#snippet header()}
    <div class="head">
      <span class="swatch" class:auto={!overrideColor} style="background: {css(overrideColor)}"></span>
      <span class="key-chip">{cultureKey}{group ? ` · ${group}` : ""}</span>
    </div>
  {/snippet}

  {#if error}
    <p class="error">{error}</p>
  {:else if !details}
    <p class="dim">Loading…</p>
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
        <span class="mono">{cultureKey}</span>
      </FieldRow>
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
      <FieldRow label="Primary nation" edited={hasPrimary && primaryVal !== details.primary}>
        <div class="row">
          <SearchDropdown
            items={countryItems}
            value={primaryVal}
            placeholder="Pick a nation…"
            onselect={(k) => setPrimary(k)}
          />
          {#if hasPrimary}
            <button class="btn" onclick={removePrimary}>Clear</button>
          {/if}
        </div>
      </FieldRow>
    </section>

    <section>
      <h3>Display color</h3>
      <p class="dim small">
        Cultures have no color in the game files — the toolkit hashes one. An
        override is stored in the toolkit only (never written to your mod).
      </p>
      <FieldRow label="Override">
        <ColorPicker value={colorRGB} onchange={commitColor} />
        {#if overrideColor}
          <span class="mono">rgb({overrideColor[0]}, {overrideColor[1]}, {overrideColor[2]})</span>
          <button class="btn" onclick={clearColor}>Reset to hash</button>
        {:else}
          <span class="dim small">Auto (hashed)</span>
        {/if}
      </FieldRow>
    </section>

    <section>
      <h3>Male names</h3>
      <NamePoolEditor
        base={details.male_names}
        oncommit={(n) => commitCulturePool("male_names", details!.male_names_present, n)}
      />
    </section>

    <section>
      <h3>Female names</h3>
      <NamePoolEditor
        base={details.female_names}
        oncommit={(n) => commitCulturePool("female_names", details!.female_names_present, n)}
      />
    </section>

    <section>
      <h3>Dynasty names</h3>
      {#key dynastyRev}
        <NamePoolEditor
          base={dynastyList}
          placeholder="One dynasty per line (spaces allowed)…"
          oncommit={(n) => {
            dynastyList = n;
            commitCulturePool("dynasty_names", details!.dynasty_names_present, n);
          }}
        />
      {/key}
      <button class="link browse" onclick={() => (dynModalOpen = true)}>Browse dynasties…</button>
    </section>

    <ProvinceNamesSection
      {installPath}
      {modPath}
      {queue}
      fileKey={cultureKey}
      kindLabel="culture"
      pickRequest={provNamePick}
      onarmpick={onarmprovnamepick}
      onpickconsumed={onprovnamepickconsumed}
    />

    <section>
      <h3>
        Group: {details.group_name}
        <button class="link" onclick={() => (showGroup = !showGroup)}>
          {showGroup ? "Hide" : "Edit group…"}
        </button>
      </h3>
      {#if showGroup}
        <p class="dim small">
          Group-level settings apply to every culture in {details.group_name} that
          doesn't define its own.
        </p>
        <FieldRow label="Graphical culture">
          <SearchDropdown
            items={gfxItems}
            value={groupGfx}
            placeholder="Pick graphical culture…"
            onselect={(k) => setGroupGfx(k)}
          />
        </FieldRow>
        <div class="sub"><h4>Group male names</h4>
          <NamePoolEditor
            base={details.group_male_names}
            oncommit={(n) => commitGroupPool("male_names", details!.group_male_names_present, n)}
          />
        </div>
        <div class="sub"><h4>Group female names</h4>
          <NamePoolEditor
            base={details.group_female_names}
            oncommit={(n) => commitGroupPool("female_names", details!.group_female_names_present, n)}
          />
        </div>
        <div class="sub"><h4>Group dynasty names</h4>
          <NamePoolEditor
            base={details.group_dynasty_names}
            oncommit={(n) => commitGroupPool("dynasty_names", details!.group_dynasty_names_present, n)}
          />
        </div>
        <div class="sub">
          <ProvinceNamesSection
            {installPath}
            {modPath}
            {queue}
            fileKey={group}
            kindLabel="culture group"
            pickRequest={provNamePick}
            onarmpick={onarmprovnamepick}
            onpickconsumed={onprovnamepickconsumed}
          />
        </div>
      {/if}
    </section>

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
      <FieldRow label="Primary of">
        <span>{details.primary_count} countr{details.primary_count === 1 ? "y" : "ies"}</span>
      </FieldRow>
      {#if details.primary_tags.length > 0}
        <div class="jumps">
          {#each details.primary_tags as t (t)}
            <button class="link" onclick={() => onjumpcountry?.(t)}>{t}</button>
          {/each}
        </div>
      {/if}
      <FieldRow label="Accepted by">
        <span>{details.accepted_count} countr{details.accepted_count === 1 ? "y" : "ies"}</span>
      </FieldRow>
      {#if details.accepted_tags.length > 0}
        <div class="jumps">
          {#each details.accepted_tags as t (t)}
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

<DynastyModal
  bind:open={dynModalOpen}
  mode="pick"
  {installPath}
  {modPath}
  onpick={appendDynasty}
/>

<NewGroupModal
  bind:open={newGroupOpen}
  kind="culture"
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
    border: 1px solid #1f242c;
  }
  .swatch.auto {
    background: repeating-linear-gradient(
      45deg,
      #4b5563,
      #4b5563 3px,
      #2b323d 3px,
      #2b323d 6px
    );
  }
  .key-chip {
    font-size: 0.8rem;
    color: #9ca3af;
  }
  section {
    padding: 0.4rem 0 0.6rem;
    border-bottom: 1px solid #232a33;
  }
  h3 {
    margin: 0 0 0.4rem;
    font-size: 0.8rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: #9ca3af;
    display: flex;
    align-items: center;
    gap: 0.5rem;
    justify-content: space-between;
  }
  h4 {
    margin: 0.4rem 0 0.2rem;
    font-size: 0.72rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: #8a919c;
  }
  .sub {
    margin-top: 0.3rem;
  }
  .text,
  .mono {
    font-size: 0.85rem;
  }
  .text {
    width: 100%;
    background: #14181d;
    border: 1px solid #4b5563;
    color: #cfd4db;
    font-family: inherit;
    padding: 0.2rem 0.4rem;
  }
  .mono {
    font-family: ui-monospace, monospace;
    color: #9ca3af;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    flex: 1;
  }
  .btn {
    border: 1px solid #4b5563;
    background: transparent;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.8rem;
    padding: 0.2rem 0.6rem;
    cursor: pointer;
    white-space: nowrap;
  }
  .btn:hover {
    border-color: #9ca3af;
  }
  .raw {
    list-style: none;
    margin: 0;
    padding: 0;
    font-size: 0.8rem;
    color: #cfd4db;
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
    border: 1px solid #4b5563;
    background: #2b323d;
    color: #9cc7ea;
    font-family: inherit;
    font-size: 0.75rem;
    padding: 0.1rem 0.4rem;
    cursor: pointer;
  }
  .link:hover {
    border-color: #4a6da7;
    color: #ffffff;
  }
  .browse {
    margin-top: 0.4rem;
  }
  .dim {
    color: #9ca3af;
  }
  .small {
    font-size: 0.75rem;
  }
  .error {
    color: #fca5a5;
  }
</style>
