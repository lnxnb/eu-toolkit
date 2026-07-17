<!--
  EmpiresOverlay — View ▸ Empires… (Sprint 29). Two tabs configuring each empire
  system fully:

  * HRE — emperor timeline (hre.txt, date-aware), electors aggregate (fold of
    `elector = yes` country history + add/remove via country picker), members
    (province count + "highlight on map"), imperial reforms (empire = hre;
    ordered, required_reform links, scope-modifier blocks, effect trees, create),
    imperial incidents (event/default_option/can_stop + numbered AI-weight
    options).
  * Mandate — celestial emperor timeline (celestial_empire.txt), reforms
    (empire = celestial_empire; SAME editor), decrees (cost/duration/modifier/
    triggers, create), and a cross-link to the religion panel for harmony/karma.

  Reforms / incidents / decrees reuse the config-driven mechanics object editor
  (the hidden empire families, mechanics.rs) — one implementation, filtered by
  empire key.
-->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { OverlaySurface } from "$lib/components/script";
  import type { KnownKey } from "$lib/components/script";
  import type { DropdownItem, KnownModifier } from "$lib/components/ui";
  import { SearchDropdown } from "$lib/components/ui";
  import type { EditQueue, TypedEdit } from "$lib/edits.svelte";
  import type { MechanicsData } from "$lib/mechanics";
  import {
    foldElectors,
    foldMembers,
    visibleAt,
    type ElectorsData,
    type Elector,
    type HreMembers,
  } from "$lib/empires";
  import EmperorTimelineSection from "./EmperorTimelineSection.svelte";
  import ReformList from "./ReformList.svelte";
  import MechanicFamilyList from "./MechanicFamilyList.svelte";
  import IncidentOptions from "./IncidentOptions.svelte";

  let {
    open = $bindable(false),
    installPath,
    modPath = null,
    date = null,
    startDate = "1444.11.11",
    queue,
    onhighlightmembers,
    onopenevents,
    onopenreligion,
  }: {
    open?: boolean;
    installPath: string;
    modPath?: string | null;
    date?: string | null;
    startDate?: string;
    queue: EditQueue;
    /** Highlight the given HRE member province ids on the political map (null =
     *  clear). */
    onhighlightmembers?: (ids: number[] | null) => void;
    onopenevents?: (id: string) => void;
    /** Cross-link: Mandate harmony/karma live in the religion panel (Sprint 26). */
    onopenreligion?: () => void;
  } = $props();

  interface CountryBrief { tag: string; name: string; color: [number, number, number] | null }

  let tab = $state<"hre" | "mandate">("hre");
  let known = $state<KnownModifier[]>([]);
  let triggers = $state<KnownKey[]>([]);
  let effects = $state<KnownKey[]>([]);
  let countries = $state<DropdownItem[]>([]);
  let reforms = $state<MechanicsData | null>(null);
  let incidents = $state<MechanicsData | null>(null);
  let decrees = $state<MechanicsData | null>(null);
  let electorsData = $state<ElectorsData | null>(null);
  let members = $state<HreMembers | null>(null);
  let electorPick = $state<string | null>(null);
  let error = $state<string | null>(null);
  let highlighted = $state(false);

  const pickerItems = {} as Record<string, DropdownItem[]>;

  $effect(() => {
    if (!open) return;
    void loadAll(installPath, modPath, date);
  });

  async function loadAll(install: string, mod: string | null, d: string | null) {
    try {
      const [kmods, trig, eff, ctys, rf, inc, dec, el, mem] = await Promise.all([
        invoke<KnownModifier[]>("get_known_modifiers"),
        invoke<KnownKey[]>("get_known_triggers"),
        invoke<KnownKey[]>("get_known_effects"),
        invoke<CountryBrief[]>("list_countries", { installPath: install, modPath: mod }),
        invoke<MechanicsData>("get_mechanics", { installPath: install, modPath: mod, family: "imperial_reforms" }),
        invoke<MechanicsData>("get_mechanics", { installPath: install, modPath: mod, family: "imperial_incidents" }),
        invoke<MechanicsData>("get_mechanics", { installPath: install, modPath: mod, family: "decrees" }),
        invoke<ElectorsData>("get_hre_electors", { installPath: install, modPath: mod, date: d }),
        invoke<HreMembers>("get_hre_members", { installPath: install, modPath: mod, date: d }),
      ]);
      known = kmods;
      triggers = trig;
      effects = eff;
      countries = ctys.map((c) => ({
        key: c.tag,
        label: c.name,
        swatch: c.color ? `rgb(${c.color[0]}, ${c.color[1]}, ${c.color[2]})` : undefined,
      }));
      reforms = rf;
      incidents = inc;
      decrees = dec;
      electorsData = el;
      members = mem;
      error = null;
    } catch (e) {
      error = String(e);
    }
  }

  const nameOf = (tag: string) => countries.find((c) => c.key === tag.toUpperCase())?.label ?? tag;

  // Electors folded over the pending queue (date-gated like the map overlays).
  const electors = $derived<Elector[]>(
    electorsData
      ? ((queue.version, foldElectors(electorsData.electors, queue.serializeVisibleAt(visibleAt(date)), nameOf)))
      : [],
  );
  const electorTags = $derived(new Set(electors.map((e) => e.tag)));
  const addCandidates = $derived<DropdownItem[]>(
    (electorsData?.candidates ?? [])
      .filter((c) => !electorTags.has(c.tag))
      .map((c) => ({ key: c.tag, label: c.name })),
  );

  const memberData = $derived<HreMembers | null>(
    members ? ((queue.version, foldMembers(members, queue.serializeVisibleAt(visibleAt(date))))) : null,
  );

  function addElector() {
    if (!electorPick || !electorsData) return;
    const cand = electorsData.candidates.find((c) => c.tag === electorPick);
    if (!cand) return;
    const edits: TypedEdit[] = [{ kind: "insertStatement", file: cand.historyFile, blockPath: [], statement: "elector = yes" }];
    queue.push({ label: `Set ${cand.tag} as elector`, edits });
    electorPick = null;
  }
  function removeElector(tag: string) {
    const cand = electorsData?.candidates.find((c) => c.tag === tag);
    const file = cand?.historyFile ?? `history/countries/${tag} - ${nameOf(tag)}.txt`;
    queue.push({
      label: `Clear ${tag} as elector`,
      edits: [{ kind: "removeStatement", file, blockPath: [], key: "elector" }],
    });
  }

  function toggleHighlight() {
    if (!memberData) return;
    highlighted = !highlighted;
    onhighlightmembers?.(highlighted ? memberData.provinceIds : null);
  }
  // Clear the highlight when the overlay closes.
  $effect(() => {
    if (!open && highlighted) {
      highlighted = false;
      onhighlightmembers?.(null);
    }
  });
</script>

<OverlaySurface bind:open title="Empires">
  {#snippet toolbar()}
    <div class="tabs">
      <button class:active={tab === "hre"} onclick={() => (tab = "hre")}>Holy Roman Empire</button>
      <button class:active={tab === "mandate"} onclick={() => (tab = "mandate")}>Mandate of Heaven</button>
    </div>
  {/snippet}

  <div class="body">
    {#if error}<p class="err">{error}</p>{/if}

    {#if tab === "hre"}
      <section class="sec">
        <h3>Emperor timeline</h3>
        <EmperorTimelineSection {installPath} {modPath} kind="hre" label="HRE" {queue} {countries} {date} />
      </section>

      <section class="sec">
        <h3>Electors <span class="ct">{electors.length}</span></h3>
        <ul class="chips">
          {#each electors as e (e.tag)}
            <li class="chip">
              <span class="cname">{e.name}</span><code>{e.tag}</code>
              <button class="x" title="Remove elector" onclick={() => removeElector(e.tag)}>✕</button>
            </li>
          {/each}
          {#if electors.length === 0}<li class="none">No electors at {date ?? startDate}.</li>{/if}
        </ul>
        <div class="addrow">
          <div class="pick"><SearchDropdown items={addCandidates} bind:value={electorPick} placeholder="add elector…" /></div>
          <button class="add" disabled={!electorPick} onclick={addElector}>＋ Add elector</button>
        </div>
      </section>

      <section class="sec">
        <h3>Members</h3>
        {#if memberData}
          <p class="members"><strong>{memberData.provinceCount}</strong> provinces in the Empire at {date ?? startDate}.</p>
          <button class="hl" class:on={highlighted} onclick={toggleHighlight}>
            {highlighted ? "Clear highlight" : "Highlight members on the map"}
          </button>
          <p class="note">Membership is edited per province (province panel HRE toggle) or with the HRE brush in Political mode.</p>
        {/if}
      </section>

      <section class="sec">
        <h3>Imperial reforms</h3>
        {#if reforms}
          <ReformList {installPath} {modPath} empire="hre" {date} {queue} data={reforms} {known} {triggers} {effects} {countries} {pickerItems} {onopenevents} />
        {/if}
      </section>

      <section class="sec">
        <h3>Imperial incidents</h3>
        {#if incidents}
          <MechanicFamilyList {installPath} {modPath} {date} {queue} data={incidents} {known} {triggers} {effects} {countries} {pickerItems} createLabel="incident" {onopenevents}>
            {#snippet extra(o)}
              <IncidentOptions {installPath} {modPath} {queue} obj={o} known={triggers} />
            {/snippet}
          </MechanicFamilyList>
        {/if}
      </section>
    {:else}
      <section class="sec">
        <h3>Celestial emperor timeline</h3>
        <EmperorTimelineSection {installPath} {modPath} kind="celestial" label="Celestial" {queue} {countries} {date} />
      </section>

      <section class="sec">
        <h3>Imperial reforms (Empire of China)</h3>
        {#if reforms}
          <ReformList {installPath} {modPath} empire="celestial_empire" {date} {queue} data={reforms} {known} {triggers} {effects} {countries} {pickerItems} {onopenevents} />
        {/if}
      </section>

      <section class="sec">
        <h3>Decrees</h3>
        {#if decrees}
          <MechanicFamilyList {installPath} {modPath} {date} {queue} data={decrees} {known} {triggers} {effects} {countries} {pickerItems} createLabel="decree" {onopenevents} />
        {/if}
      </section>

      <section class="sec xlink">
        <h3>Harmony &amp; Karma</h3>
        <p class="note">Confucian harmony and Buddhist karma religion mechanics are edited in the religion panel.</p>
        {#if onopenreligion}<button class="add" onclick={onopenreligion}>Open religion mechanics ↗</button>{/if}
      </section>
    {/if}
  </div>
</OverlaySurface>

<style>
  .tabs { display: flex; gap: 0.25rem; }
  .tabs button { border: 1px solid #1f242c; background: #21262e; color: #cfd4db; font-family: inherit; font-size: 0.82rem; padding: 0.25rem 0.7rem; cursor: pointer; }
  .tabs button.active { background: #4a6da7; color: #fff; }
  .body { display: flex; flex-direction: column; gap: 1rem; }
  .sec { display: flex; flex-direction: column; gap: 0.4rem; }
  .sec h3 { margin: 0; font-size: 0.9rem; color: #e3e7ec; border-bottom: 1px solid #1f242c; padding-bottom: 0.25rem; }
  .ct { color: #8a919c; font-weight: normal; font-size: 0.8rem; }
  .chips { list-style: none; margin: 0; padding: 0; display: flex; flex-wrap: wrap; gap: 0.35rem; }
  .chip { display: flex; align-items: center; gap: 0.3rem; border: 1px solid #1f242c; background: #21262e; padding: 0.12rem 0.35rem; font-size: 0.8rem; }
  .chip code { color: #9aecc0; background: #16191f; padding: 0 0.25rem; font-size: 0.72rem; }
  .chip .x { border: none; background: none; color: #8a919c; cursor: pointer; font-size: 0.75rem; }
  .chip .x:hover { color: #d9756b; }
  .none { color: #8a919c; font-size: 0.82rem; }
  .addrow { display: flex; align-items: center; gap: 0.5rem; }
  .pick { width: 16rem; }
  .add { border: 1px solid #1f242c; background: #3f4855; color: #cfd4db; font-family: inherit; font-size: 0.82rem; padding: 0.28rem 0.7rem; cursor: pointer; }
  .add:hover:not(:disabled) { background: #4a6da7; color: #fff; }
  .add:disabled { opacity: 0.5; cursor: default; }
  .members { font-size: 0.88rem; color: #cfd4db; margin: 0; }
  .hl { border: 1px solid #1f242c; background: #3f4855; color: #cfd4db; font-family: inherit; font-size: 0.82rem; padding: 0.3rem 0.8rem; cursor: pointer; align-self: flex-start; }
  .hl:hover { background: #4a6da7; color: #fff; }
  .hl.on { background: #4a6da7; color: #fff; }
  .note { font-size: 0.78rem; color: #6d7683; margin: 0.1rem 0 0; }
  .xlink { opacity: 0.95; }
  .err { color: #d9756b; font-size: 0.85rem; }
</style>
