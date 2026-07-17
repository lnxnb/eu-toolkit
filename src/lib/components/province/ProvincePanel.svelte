<!--
  ProvincePanel — the full province editor (Sprint 2.2/2.3). Header (editable
  localized name → PROV<id> loc override, id, capital city name, owner
  click-through, geography breadcrumb) + Political / Economy / Culture&Religion /
  Buildings / Discovery / Geography sections + the reusable dated-history Timeline
  + a read-only "Advanced" raw-remainder block (preserve-unknown).

  Water/wasteland provinces get a trimmed read-only panel (header + geography +
  advanced) — most history fields don't apply.

  Effective-1444 re-derive: the sections read a base snapshot that is the backend
  `effective_1444` until a pre-start dated-block edit lands, after which it is
  re-derived locally (deriveEffective) so the shown state follows the edit.
-->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { SidePanel } from "$lib/components/ui";
  import type { DropdownItem } from "$lib/components/ui";
  import Timeline from "$lib/components/Timeline.svelte";
  import type { TimelineBlock, TimelineIntent } from "$lib/components/timeline";
  import { parseModeData } from "$lib/mapmode";
  import type { EditQueue } from "$lib/edits.svelte";
  import type { Calendar } from "$lib/calendar";
  import type { ProvinceDetails, DatedBlock, GeoOptions } from "./types";
  import { deriveEffective } from "./types";
  import { intentToEdits, scalarEdit, pushAtDate, type DateCtx } from "./fields";
  import PoliticalSection from "./PoliticalSection.svelte";
  import EconomySection from "./EconomySection.svelte";
  import CultureReligionSection from "./CultureReligionSection.svelte";
  import BuildingsSection from "./BuildingsSection.svelte";
  import DiscoverySection from "./DiscoverySection.svelte";
  import GeographySection from "./GeographySection.svelte";
  import RebelsSection from "./RebelsSection.svelte";
  import MonumentsSection from "./MonumentsSection.svelte";
  import MercenariesSection from "./MercenariesSection.svelte";
  import ProvinceNamesReverseSection from "./ProvinceNamesReverseSection.svelte";

  let {
    installPath,
    modPath,
    id,
    queue,
    calendar = null,
    date = null,
    startDate = "1444.11.11",
    onclose,
    onopencountry,
    onopenculture,
    onopenmechanics,
    scrollTo,
  }: {
    installPath: string;
    modPath: string | null;
    id: number;
    queue: EditQueue;
    /** The mod's calendar (Sprint 12.4) so timeline dates show custom months. */
    calendar?: Calendar | null;
    /** View/edit date (Sprint 12.2); null = effective start. Re-fetches details. */
    date?: string | null;
    /** The mod's effective start date (Sprint 12.3); the base-state baseline. */
    startDate?: string;
    onclose: () => void;
    /** Open a country in political mode (owner click-through). */
    onopencountry?: (tag: string) => void;
    /** Jump to a culture (cultures mode + select) from the reverse names view. */
    onopenculture?: (key: string) => void;
    /** Open the Mechanics editor (Sprint 27 W4) at a family — the Economy
     *  section's Centre-of-Trade control links to the centers_of_trade editor. */
    onopenmechanics?: (family: string, key?: string) => void;
    /** Section anchor to scroll into view on open (e.g. "development" → the
     * Economy section, opened from Development map mode's click-to-edit, 9.1b). */
    scrollTo?: string;
  } = $props();

  let devAnchor: HTMLDivElement | null = $state(null);
  // Scroll the requested section into view once details (and thus the section)
  // have rendered. Runs on open / id change.
  $effect(() => {
    if (scrollTo === "development" && details && devAnchor) {
      const el = devAnchor;
      requestAnimationFrame(() => el.scrollIntoView({ block: "start", behavior: "auto" }));
    }
  });

  let details = $state<ProvinceDetails | null>(null);
  let error = $state("");
  let blocks = $state<DatedBlock[]>([]);
  let blocksEdited = $state(false);

  // Option sources (province-independent; loaded once, refreshed on session).
  let countries = $state<DropdownItem[]>([]);
  let religions = $state<DropdownItem[]>([]);
  let cultures = $state<DropdownItem[]>([]);
  let techGroups = $state<DropdownItem[]>([]);
  let buildings = $state<DropdownItem[]>([]);
  let goods = $state<DropdownItem[]>([]);
  let eventModifiers = $state<DropdownItem[]>([]);
  let triggeredModifiers = $state<DropdownItem[]>([]);
  let rebelFactions = $state<DropdownItem[]>([]);
  let geo = $state<GeoOptions | null>(null);

  function css(c: [number, number, number] | null): string | undefined {
    return c ? `rgb(${c[0]}, ${c[1]}, ${c[2]})` : undefined;
  }
  interface Grouped { key: string; name: string; group_name: string; color: [number, number, number] | null; }
  function grouped(rows: Grouped[]): DropdownItem[] {
    return rows
      .slice()
      .sort((a, b) => a.group_name.localeCompare(b.group_name) || a.name.localeCompare(b.name))
      .map((r) => ({ key: r.key, label: `${r.name} — ${r.group_name}`, swatch: css(r.color) }));
  }

  // Load option sources once per session (installPath/modPath).
  $effect(() => {
    const ip = installPath, mp = modPath;
    const reg = (name: string) =>
      invoke<{ key: string; name: string }[]>("get_registry", { name, installPath: ip, modPath: mp })
        .then((r) => r.map((e) => ({ key: e.key, label: e.name })));
    invoke<{ tag: string; name: string; color: [number, number, number] | null }[]>("list_countries", { installPath: ip, modPath: mp })
      .then((v) => (countries = v.map((c) => ({ key: c.tag, label: c.name, swatch: css(c.color) })))).catch(() => {});
    invoke<Grouped[]>("list_religions", { installPath: ip, modPath: mp }).then((v) => (religions = grouped(v))).catch(() => {});
    invoke<Grouped[]>("list_cultures", { installPath: ip, modPath: mp }).then((v) => (cultures = grouped(v))).catch(() => {});
    reg("technology_groups").then((v) => (techGroups = v)).catch(() => {});
    reg("buildings").then((v) => (buildings = v)).catch(() => {});
    reg("event_modifiers").then((v) => (eventModifiers = v)).catch(() => {});
    reg("province_triggered_modifiers").then((v) => (triggeredModifiers = v)).catch(() => {});
    reg("rebel_types").then((v) => (rebelFactions = v)).catch(() => {});
    // Trade goods from mode-data groups (colored swatches).
    invoke<ArrayBuffer>("get_mode_data", { installPath: ip, modPath: mp, mode: "trade_goods" })
      .then((buf) => { goods = parseModeData(buf).groups.map((g) => ({ key: g.key, label: g.label, swatch: css(g.color) })); })
      .catch(() => {});
    // New command (needs registration in lib.rs); degrade gracefully if absent.
    invoke<GeoOptions>("get_geo_options", { installPath: ip, modPath: mp }).then((v) => (geo = v)).catch(() => (geo = null));
  });

  // Load province details when id changes.
  $effect(() => {
    const cur = id;
    const at = date;
    details = null;
    error = "";
    blocksEdited = false;
    invoke<ProvinceDetails>("get_province_details", { installPath, modPath, id: cur, date: at })
      .then((d) => {
        if (cur !== id) return;
        details = d;
        blocks = [...d.dated_blocks];
      })
      .catch((e) => { if (cur === id) error = String(e); });
  });

  const file = $derived(details?.file ?? "");
  // Effective base at the selected date: backend value until a dated edit
  // re-derives it locally. The cutoff is the selected view date (Sprint 12.3) so
  // a block added at that date updates the shown state; falls back to the start.
  const effective = $derived(
    details
      ? blocksEdited
        ? deriveEffective(details.top_level, blocks, date ?? startDate)
        : details.effective_1444
      : null,
  );

  // Sprint 12.3 date context handed to the section writers: at a later date their
  // writes go into the dated block for that date and fold into `blocks` here so
  // `effective` (above) follows. Rebuilt reactively as `blocks`/date change.
  const dateCtx = $derived<DateCtx>({
    file,
    selectedDate: date,
    startDate,
    blocks,
    foldStatements: foldStatementsIntoBlocks,
  });

  /** Vanilla start ordinal for the post-start badge, mirroring foldIntoBlocks. */
  function isPostStart(d: string): boolean {
    const [y, m, dd] = d.split(".").map((s) => parseInt(s, 10) || 0);
    return y * 10000 + m * 100 + dd > 1444 * 10000 + 11 * 100 + 11;
  }

  /** Fold `key = value` statements written at the selected date into the local
   *  blocks (merge into the last block for that date, else add one) so the shown
   *  effective state and timeline follow the pending write. Mirrors editAtDate. */
  function foldStatementsIntoBlocks(statements: string[]) {
    if (date == null || statements.length === 0) return;
    const d = date;
    const entries = statements.map((s) => {
      const eq = s.indexOf("=");
      const value = s.slice(eq + 1).trim();
      return { key: s.slice(0, eq).trim(), value, is_block: value.startsWith("{") };
    });
    const targets = blocks.map((b, i) => ({ b, i })).filter((x) => x.b.date === d);
    if (targets.length > 0) {
      const ti = targets[targets.length - 1].i;
      blocks = blocks.map((b, j) => (j === ti ? { ...b, entries: [...b.entries, ...entries] } : b));
    } else {
      blocks = [...blocks, { date: d, post_start: isPostStart(d), occurrence_index: 0, entries }];
    }
    blocksEdited = true;
  }

  // Header: localized name (PROV<id> loc override) + capital city name.
  let locKey = $derived(`PROV${id}`);
  let pendingName = $derived(queue.pendingLocOverride(locKey));
  let titleName = $derived(pendingName ?? details?.localized_name ?? `Province ${id}`);
  let editingName = $state(false);
  let nameDraft = $state("");
  function startName() { nameDraft = titleName; editingName = true; }
  function applyName() {
    const v = nameDraft.trim();
    if (v && v !== (details?.localized_name ?? "")) queue.push({ label: `Rename province #${id}`, edits: [{ kind: "locOverride", key: locKey, value: v }] });
    editingName = false;
  }

  let capField = $derived(queue.pendingField(file, "capital"));
  let capitalCity = $derived(capField !== undefined ? (capField.value ?? "") : (details?.top_level.capital ?? ""));
  let editingCap = $state(false);
  let capDraft = $state("");
  function startCap() { capDraft = capitalCity; editingCap = true; }
  function applyCap() {
    const v = capDraft.trim();
    if (details && v !== (details.top_level.capital ?? "")) {
      pushAtDate(
        queue,
        dateCtx,
        `Set capital city of #${id}`,
        [scalarEdit(file, "capital", v, details.top_level.capital != null, true)],
        [`capital = "${v}"`],
      );
    }
    editingCap = false;
  }

  let ownerTag = $derived(effective?.owner ?? null);
  function ownerLabel(tag: string): string { return countries.find((c) => c.key === tag)?.label ?? tag; }
  function ownerColor(tag: string): string | undefined { return countries.find((c) => c.key === tag)?.swatch; }

  const water = $derived(details?.geography.water === true);
  // Sprint 20 (thin): the legacy province-side `estate = <key>` assignment (used
  // by pre-1.26-targeting mods like Anbennar). No modern province-side estate keys
  // exist in vanilla, so this is a read-only surface — the value also round-trips
  // via raw_remainder/Advanced. Shown here as a labeled line for discoverability.
  const estateAssignment = $derived(
    details?.raw_remainder.find((r) => r.key === "estate")?.value ?? null,
  );

  // Timeline: map local blocks (file order) to the component's shape.
  let timelineBlocks = $derived<TimelineBlock[]>(
    blocks.map((b) => ({
      date: b.date,
      postStart: b.post_start,
      occurrenceIndex: b.occurrence_index,
      entries: b.entries.map((e) => ({ key: e.key, value: e.value, isBlock: e.is_block })),
    })),
  );

  function timelineLabel(intent: TimelineIntent): string {
    if (intent.kind === "addEntry") return `Add ${intent.date} entry to #${id}`;
    if (intent.kind === "deleteEntry") return `Delete ${intent.key} from ${intent.date} (#${id})`;
    return `Edit ${intent.date} entry of #${id}`;
  }

  // Optimistically fold an intent into the local blocks so the view follows.
  function foldIntoBlocks(intent: TimelineIntent) {
    if (intent.kind === "addEntry") {
      const occ = blocks.filter((b) => b.date === intent.date).length;
      const post = (() => {
        const [y, m, d] = intent.date.split(".").map((s) => parseInt(s, 10) || 0);
        return y * 10000 + m * 100 + d > 1444 * 10000 + 11 * 100 + 11;
      })();
      blocks = [...blocks, { date: intent.date, post_start: post, occurrence_index: occ, entries: [{ key: intent.key, value: intent.value, is_block: false }] }];
      return;
    }
    const i = blocks.findIndex((b) => b.date === intent.date && b.occurrence_index === intent.occurrenceIndex);
    if (i < 0) return;
    const nb = blocks.map((b, j) => (j === i ? { ...b, entries: [...b.entries] } : b));
    const blk = nb[i];
    if (intent.kind === "editValue") blk.entries[intent.entryIndex] = { ...blk.entries[intent.entryIndex], value: intent.value };
    else if (intent.kind === "editEntry") blk.entries[intent.entryIndex] = { key: intent.key, value: intent.value, is_block: false };
    else if (intent.kind === "deleteEntry") blk.entries = blk.entries.filter((_, k) => k !== intent.entryIndex);
    blocks = nb;
  }

  function onTimeline(intent: TimelineIntent) {
    if (!details) return;
    const edits = intentToEdits(file, intent);
    queue.push({ label: timelineLabel(intent), edits });
    foldIntoBlocks(intent);
    blocksEdited = true;
  }

  let showAdvanced = $state(false);
</script>

<SidePanel title={titleName} {onclose}>
  {#snippet header()}
    <div class="head">
      <div class="name-row">
        {#if editingName}
          <input class="hin" bind:value={nameDraft} onkeydown={(e) => e.key === "Enter" && applyName()} />
          <button class="mini" onclick={applyName}>✓</button>
          <button class="mini" onclick={() => (editingName = false)}>×</button>
        {:else}
          <span class="pid">#{id}</span>
          <button class="edit-btn" title="Rename province" onclick={startName}>✎</button>
          {#if pendingName}<span class="edited">edited</span>{/if}
        {/if}
      </div>
      {#if !water}
        <div class="cap-row">
          <span class="lbl">Capital city:</span>
          {#if editingCap}
            <input class="hin" bind:value={capDraft} onkeydown={(e) => e.key === "Enter" && applyCap()} />
            <button class="mini" onclick={applyCap}>✓</button>
            <button class="mini" onclick={() => (editingCap = false)}>×</button>
          {:else}
            <span class="capname">{capitalCity || "—"}</span>
            <button class="edit-btn" title="Edit capital city name" onclick={startCap}>✎</button>
            {#if capField !== undefined}<span class="edited">edited</span>{/if}
          {/if}
        </div>
      {/if}
      {#if ownerTag}
        <button class="owner" onclick={() => onopencountry?.(ownerTag!)} title="Open in political mode">
          <span class="swatch" style="background: {ownerColor(ownerTag) ?? 'transparent'}"></span>
          {ownerLabel(ownerTag)} ↗
        </button>
      {:else}
        <span class="uncol">Uncolonized</span>
      {/if}
    </div>
  {/snippet}

  {#if error}
    <p class="error">{error}</p>
  {:else if !details || !effective}
    <p class="dim">Loading…</p>
  {:else}
    <!-- Geography breadcrumb -->
    <div class="crumb">
      {details.geography.area?.name ?? "no area"} ·
      {details.geography.region?.name ?? "no region"} ·
      {details.geography.continent?.name ?? "no continent"}
      {#if details.geography.trade_node} · node: {details.geography.trade_node.name}{/if}
      {#if details.geography.water}<span class="wtag">water</span>{/if}
      {#if details.geography.impassable}<span class="wtag">wasteland</span>{/if}
    </div>

    <ProvinceNamesReverseSection
      {installPath}
      {modPath}
      {id}
      onjumpculture={onopenculture}
      onjumpcountry={onopencountry}
    />

    {#if water}
      <p class="dim water-note">Water province — most history fields don't apply. Trade-node membership and geography are editable below.</p>
      <GeographySection {installPath} {modPath} {details} {queue} {geo} />
    {:else}
      <PoliticalSection {details} {effective} {file} {queue} {countries} {dateCtx} />
      <!-- Development lives at the top of the Economy section (base tax/prod/man
           steppers); 9.1b scrolls here from the Development map mode. -->
      <div bind:this={devAnchor} id="prov-development-anchor">
        <EconomySection {details} {effective} {file} {queue} {goods} {eventModifiers} {triggeredModifiers} {dateCtx} {onopenmechanics} />
      </div>
      <CultureReligionSection {details} {effective} {file} {queue} {cultures} {religions} {dateCtx} />
      <BuildingsSection {details} {effective} {file} {queue} {buildings} {dateCtx} />
      <DiscoverySection {details} {effective} {file} {queue} {techGroups} {dateCtx} />
      <RebelsSection {details} {file} {queue} factions={rebelFactions} {dateCtx} />
      <MonumentsSection {installPath} {modPath} {id} {queue} {countries} />
      <MercenariesSection {installPath} {modPath} {id} {queue} {countries} />
      <GeographySection {installPath} {modPath} {details} {queue} {geo} />
      {#if estateAssignment}
        <section class="estate-assign">
          <h3>Estate</h3>
          <p class="dim">
            Assigned to <code>{estateAssignment}</code> (legacy province-side
            <code>estate =</code> key, read-only).
          </p>
        </section>
      {/if}
    {/if}

    <!-- Dated history timeline (Sprint 2.3) -->
    <section>
      <h3>History Timeline</h3>
      <Timeline blocks={timelineBlocks} {calendar} onchange={onTimeline} />
    </section>

    <!-- Preserve-unknown: unmodeled statements, read-only -->
    {#if details.raw_remainder.length > 0}
      <section>
        <button class="adv-toggle" onclick={() => (showAdvanced = !showAdvanced)}>
          {showAdvanced ? "▾" : "▸"} Advanced ({details.raw_remainder.length} unmodeled)
        </button>
        {#if showAdvanced}
          <pre class="raw">{#each details.raw_remainder as r}{r.key ? `${r.key} = ` : ""}{r.value}
{/each}</pre>
        {/if}
      </section>
    {/if}
  {/if}
</SidePanel>

<style>
  .head { display: flex; flex-direction: column; gap: 0.35rem; }
  .name-row, .cap-row { display: flex; align-items: center; gap: 0.4rem; }
  .pid { font-size: 0.8rem; color: #9ca3af; }
  .lbl { font-size: 0.72rem; text-transform: uppercase; color: #8a919c; }
  .capname { font-size: 0.85rem; }
  .hin { flex: 1; min-width: 0; background: #21262e; border: 1px solid #1f242c; color: #cfd4db; font-family: inherit; font-size: 0.85rem; padding: 0.15rem 0.4rem; outline: none; }
  .edit-btn { border: none; background: transparent; color: #9ca3af; cursor: pointer; font-size: 0.85rem; padding: 0 0.15rem; }
  .edit-btn:hover { color: #fff; }
  .mini { border: 1px solid #1f242c; background: #2b323d; color: #cfd4db; cursor: pointer; font-size: 0.8rem; padding: 0.1rem 0.35rem; }
  .mini:hover { background: #4a6da7; color: #fff; }
  .edited { background: rgba(234, 179, 8, 0.22); color: #fde68a; font-size: 0.66rem; padding: 0.02rem 0.4rem; }
  .owner { display: inline-flex; align-items: center; gap: 0.35rem; align-self: flex-start; border: 1px solid #1f242c; background: #21262e; color: #cfd4db; font-family: inherit; font-size: 0.82rem; padding: 0.15rem 0.5rem; cursor: pointer; }
  .owner:hover { background: #4a6da7; color: #fff; }
  .swatch { width: 0.8rem; height: 0.8rem; border: 1px solid #1f242c; display: inline-block; }
  .uncol { font-size: 0.8rem; color: #8a919c; }
  .crumb { font-size: 0.75rem; color: #9ca3af; margin-bottom: 0.7rem; line-height: 1.4; }
  .wtag { background: #3a5a86; color: #fff; font-size: 0.62rem; padding: 0.05rem 0.3rem; margin-left: 0.25rem; }
  .water-note { margin: 0 0 0.6rem; font-size: 0.8rem; }
  section { margin-bottom: 1rem; }
  h3 { margin: 0 0 0.4rem; font-size: 0.8rem; text-transform: uppercase; letter-spacing: 0.05em; color: #9ca3af; }
  .adv-toggle { border: none; background: transparent; color: #9ca3af; font-family: inherit; font-size: 0.78rem; cursor: pointer; padding: 0.2rem 0; text-transform: uppercase; letter-spacing: 0.03em; }
  .adv-toggle:hover { color: #cfd4db; }
  .raw { margin: 0.3rem 0 0; padding: 0.5rem; background: #191d23; border: 1px solid #1f242c; color: #9ab0d0; font-size: 0.72rem; white-space: pre-wrap; word-break: break-word; max-height: 16rem; overflow-y: auto; }
  .dim { color: #8a919c; }
  .error { color: #fca5a5; }
</style>
