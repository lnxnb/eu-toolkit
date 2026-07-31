<script module lang="ts">
  // Tab selection persists across country switches (module-level, one panel at a
  // time). Sprint 3.1: clicking between countries keeps the active tab.
  let activeTab = $state("overview");
</script>

<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { SidePanel, LoadingState, EditableHeading } from "$lib/components/ui";
  import type { DropdownItem } from "$lib/components/ui";
  import type { EditQueue, TypedEdit } from "$lib/edits.svelte";
  import type { Calendar } from "$lib/calendar";
  import type { CountryBlastRadius } from "./country/delete";
  import IdentitySection from "./country/IdentitySection.svelte";
  import GovernmentSection from "./country/GovernmentSection.svelte";
  import RulerSection from "./country/RulerSection.svelte";
  import QueenSection from "./country/QueenSection.svelte";
  import HeirSection from "./country/HeirSection.svelte";
  import IdeasSection from "./country/IdeasSection.svelte";
  import NamePoolsSection from "./country/NamePoolsSection.svelte";
  import ProvinceNamesSection from "./ProvinceNamesSection.svelte";
  import HistoricalSection from "./country/HistoricalSection.svelte";
  import HistoryTimelineSection from "./country/HistoryTimelineSection.svelte";
  import { EstatesSection } from "./estates";
  import DiplomacyTab from "./country/DiplomacyTab.svelte";
  import FieldRow from "./country/FieldRow.svelte";
  import { scalarEdit } from "./country/fields";
  import type {
    CountryDetails,
    CountryCreateSeed,
    GroupedEntry,
    RegistryEntry,
    CountryBrief,
  } from "./country/types";

  let {
    installPath,
    modPath,
    tag,
    queue,
    calendar = null,
    date = null,
    startDate = "1444.11.11",
    seed = null,
    onclose,
    oncolor,
    capitalRequest = null,
    oncapitalapplied,
    provNamePick = null,
    onarmprovnamepick,
    onprovnamepickconsumed,
    onopencountry,
    onopenprovince,
    onremovepending,
    ondeleted,
    onopennaming,
    onopenestates,
    onzoomtoprovince,
    onopenmechanics,
    embedded = false,
    contentTab,
    onopenmissions, onopenevents, onopendecisions,
  }: {
    installPath: string;
    modPath: string | null;
    tag: string;
    queue: EditQueue;
    /** The mod's calendar (Sprint 12.4) so diplomacy dates show custom months. */
    calendar?: Calendar | null;
    /** View/edit date (Sprint 12.2); null = effective start. Re-fetches details. */
    date?: string | null;
    /** The mod's effective start date (Sprint 12.3); the base-state baseline. */
    startDate?: string;
    /** Display seed for a pending-created country not on disk yet (Sprint 4.1). */
    seed?: CountryCreateSeed | null;
    onclose: () => void;
    /** Live map-color preview: (tag, rgb|null) — null clears the override. */
    oncolor?: (tag: string, rgb: [number, number, number] | null) => void;
    /** Province id chosen by the map's Set Capital tool, or null. */
    capitalRequest?: number | null;
    /** Called once the capital edit has been queued, so the map disarms. */
    oncapitalapplied?: () => void;
    /** Province id picked on the map for the province-names section, or null. */
    provNamePick?: number | null;
    /** Arm the map "pick a province" tool for the province-names section. */
    onarmprovnamepick?: () => void;
    /** Reset the pick request once the section consumes it. */
    onprovnamepickconsumed?: () => void;
    /**
     * Jump to another country's panel (Diplomacy-tab partner / rival-friend
     * links). MapView owns selection; thread its `openCountryInPolitical` here.
     * ORCHESTRATOR: add `onopencountry={openCountryInPolitical}` to the
     * <CountryPanel> in MapView.svelte (single line) so partner links work.
     */
    onopencountry?: (tag: string) => void;
    /** Jump to a province (war-validation occupation links). */
    onopenprovince?: (id: number) => void;
    /** Delete a pending-created (unsaved) country: MapView drops its create
     *  composite from the queue. Only called for a not-yet-saved scaffold. */
    onremovepending?: (tag: string) => void;
    /** A saved country's deletion composite was queued (panel closes after). */
    ondeleted?: (tag: string) => void;
    /** Open the Government-names editor (Sprint 19.3), optionally at a scheme. */
    onopennaming?: (schemeKey?: string) => void;
    /** Open the Estates editor (Sprint 20), optionally at an estate/priv/agenda. */
    onopenestates?: (key?: string) => void;
    /** Centre the map on a province — the header flag's zoom-to-capital click. */
    onzoomtoprovince?: (id: number) => void;
    /** Open the Mechanics editor (Sprint 27) at a family, optionally focused on a
     *  key — the government-reform edit…/new… affordance. */
    onopenmechanics?: (family: string, key?: string) => void;
    embedded?: boolean;
    contentTab?: "overview" | "rulers" | "ideas" | "diplomacy" | "estates" | "history" | "names";
    onopenmissions?: (tag: string) => void; onopenevents?: (tag: string) => void; onopendecisions?: (tag: string) => void;
  } = $props();

  $effect(() => { if (contentTab) activeTab = contentTab; });

  let details = $state<CountryDetails | null>(null);
  let flagUrl = $state<string | null>(null);
  let error = $state("");
  // A pending-created country has no files yet: get_country_details 404s, so we
  // render a read-only scaffold view from `seed` instead (mirrors ReligionPanel).
  let pendingMode = $state(false);

  // Shared option lists loaded once per session, passed to the character sections
  // (cultures/religions overrides, personality picker, consort origin tags).
  let cultures = $state<DropdownItem[]>([]);
  let religions = $state<DropdownItem[]>([]);
  let personalityItems = $state<DropdownItem[]>([]);
  let countries = $state<DropdownItem[]>([]);

  let pendingName = $derived(queue.pendingLocOverride(tag));

  // Renaming from the headline shares IdentitySection's coalesce key, so the
  // two affordances collapse into one composite instead of stacking edits.
  function commitHeaderName(next: string) {
    queue.push({
      label: `Rename ${tag} to ${next}`,
      edits: [{ kind: "locOverride", key: tag, value: next }],
      coalesceKey: `name:${tag}`,
    });
  }

  // The capital the header flag zooms to: a pending Set-Capital edit wins over
  // disk, and a not-yet-saved country has only its scaffold seed.
  let capitalId = $derived.by(() => {
    if (pendingMode) return seed?.capitalId ?? null;
    if (!details) return null;
    const hf = details.history_file ?? `history/countries/${tag} - ${details.name}.txt`;
    const pending = queue.pendingField(hf, "capital")?.value;
    const raw = pending != null ? Number(pending) : details.capital;
    return raw != null && Number.isFinite(Number(raw)) ? Number(raw) : null;
  });
  let titleName = $derived(pendingName ?? details?.localized_name ?? seed?.name ?? tag);
  // Adjective + color, pending-aware (seed backs the pending-scaffold view).
  let effectiveAdjective = $derived(
    queue.pendingLocOverride(`${tag}_ADJ`) ?? details?.adjective ?? seed?.adjective ?? null,
  );
  let effectiveColor = $derived<[number, number, number] | null>(
    details?.color ?? (pendingMode ? (seed?.color ?? null) : null),
  );

  const tabs = [
    { id: "overview", label: "Overview" },
    { id: "rulers", label: "Rulers" },
    { id: "ideas", label: "Ideas" },
    { id: "diplomacy", label: "Diplomacy" },
    { id: "estates", label: "Estates" },
    { id: "history", label: "History" },
    { id: "names", label: "Names" },
  ];

  // Rivals/friends folded with pending edits, for the Diplomacy tab's read view.
  const histFile = $derived(details?.history_file ?? "");
  const rivals = $derived(
    details ? queue.pendingList(histFile, "historical_rival", details.historical_rivals) : [],
  );
  const friends = $derived(
    details ? queue.pendingList(histFile, "historical_friend", details.historical_friends) : [],
  );

  function css(c: [number, number, number] | null): string | undefined {
    return c ? `rgb(${c[0]}, ${c[1]}, ${c[2]})` : undefined;
  }
  function grouped(rows: GroupedEntry[]): DropdownItem[] {
    return rows
      .slice()
      .sort((a, b) => a.group_name.localeCompare(b.group_name) || a.name.localeCompare(b.name))
      .map((r) => ({ key: r.key, label: `${r.name} — ${r.group_name}`, swatch: css(r.color) }));
  }

  $effect(() => {
    invoke<GroupedEntry[]>("list_cultures", { installPath, modPath })
      .then((v) => (cultures = grouped(v)))
      .catch(() => {});
    invoke<GroupedEntry[]>("list_religions", { installPath, modPath })
      .then((v) => (religions = grouped(v)))
      .catch(() => {});
    invoke<RegistryEntry[]>("get_registry", { name: "ruler_personalities", installPath, modPath })
      .then((v) => (personalityItems = v.map((e) => ({ key: e.key, label: e.name }))))
      .catch(() => {});
    invoke<CountryBrief[]>("list_countries", { installPath, modPath })
      .then((v) => (countries = v.map((c) => ({ key: c.tag, label: c.name, swatch: css(c.color) }))))
      .catch(() => {});
  });

  $effect(() => {
    const current = tag;
    const at = date;
    details = null;
    error = "";
    pendingMode = false;
    invoke<CountryDetails>("get_country_details", { installPath, modPath, tag: current, date: at })
      .then((d) => {
        if (current === tag) details = d;
      })
      .catch((e) => {
        if (current !== tag) return;
        // A pending-created country isn't on disk yet — show the scaffold seed.
        if (seed && seed.tag === current) pendingMode = true;
        else error = String(e);
      });

    let revoked: string | null = null;
    invoke<ArrayBuffer>("get_country_flag", { installPath, modPath, tag: current })
      .then((buf) => {
        const url = URL.createObjectURL(new Blob([buf], { type: "image/png" }));
        if (current === tag) {
          flagUrl = url;
          revoked = url;
        } else {
          URL.revokeObjectURL(url);
        }
      })
      .catch(() => {
        if (current === tag) flagUrl = null;
      });

    return () => {
      if (revoked) URL.revokeObjectURL(revoked);
      flagUrl = null;
    };
  });

  // Clear the map color override when this panel unmounts (selection change).
  $effect(() => {
    const t = tag;
    return () => oncolor?.(t, null);
  });

  // Apply a capital chosen by the map's Set Capital tool.
  $effect(() => {
    const id = capitalRequest;
    if (id == null || !details) return;
    const hf = details.history_file ?? `history/countries/${tag} - ${details.name}.txt`;
    queue.push({
      label: `Set capital of ${tag} to #${id}`,
      edits: [scalarEdit(hf, "capital", String(id), details.capital != null)],
    });
    oncapitalapplied?.();
  });

  // --- Delete country (Sprint S2.1) ---------------------------------------
  // A saved country opens a confirm dialog listing the blast radius, then queues
  // the deletion composite (one undo unit). A pending-created country (not on
  // disk yet — pendingMode) is deleted by dropping its create composite instead.
  let deleteOpen = $state(false);
  let blast = $state<CountryBlastRadius | null>(null);
  let blastError = $state("");
  let transferTag = $state(""); // "" = make provinces uncolonized
  let deleting = $state(false);
  const canDelete = $derived((details != null && !error) || (pendingMode && seed != null));
  const transferItems = $derived(countries.filter((c) => c.key !== tag));

  function openDelete() {
    deleteOpen = true;
    transferTag = "";
    blast = null;
    blastError = "";
    deleting = false;
    if (pendingMode) return; // unsaved: no backend blast to compute
    const cur = tag;
    invoke<CountryBlastRadius>("get_country_blast_radius", { installPath, modPath, tag: cur, date })
      .then((b) => {
        if (cur === tag) blast = b;
      })
      .catch((e) => {
        if (cur === tag) blastError = String(e);
      });
  }
  function closeDelete() {
    deleteOpen = false;
  }
  async function confirmDelete() {
    // Pending-created: just drop the create composite from the queue.
    if (pendingMode) {
      onremovepending?.(tag);
      deleteOpen = false;
      return;
    }
    deleting = true;
    let edits: TypedEdit[];
    try {
      edits = await invoke<TypedEdit[]>("prepare_country_deletion", {
        installPath,
        modPath,
        tag,
        date,
        transferTo: transferTag || null,
      });
    } catch (e) {
      blastError = String(e);
      deleting = false;
      return;
    }
    queue.push({ label: `Delete country ${titleName}`, edits });
    deleting = false;
    deleteOpen = false;
    ondeleted?.(tag);
    onclose();
  }
  function jumpCountry(t: string | null | undefined) {
    if (!t) return;
    deleteOpen = false;
    onopencountry?.(t);
  }
  function jumpProvince(id: number) {
    deleteOpen = false;
    onopenprovince?.(id);
  }
  function cname(t: string | null | undefined): string {
    if (!t) return "?";
    return countries.find((c) => c.key === t)?.label ?? t;
  }
</script>

<SidePanel title={titleName} {tabs} bind:activeTab {onclose} {embedded}>
  {#snippet header()}
    <div class="head">
      {#if flagUrl}
        {#if onzoomtoprovince && capitalId != null}
          <button
            class="flag-btn"
            title="Zoom to {titleName}'s capital (#{capitalId})"
            onclick={() => onzoomtoprovince?.(capitalId!)}
          >
            <img class="flag" src={flagUrl} alt="Flag of {titleName}" />
          </button>
        {:else}
          <img class="flag" src={flagUrl} alt="Flag of {titleName}" />
        {/if}
      {/if}
      <div class="ident">
        <EditableHeading
          value={titleName}
          label="Country name"
          edited={pendingName !== undefined}
          readonly={pendingMode}
          oncommit={commitHeaderName}
        />
        <span class="tag-chip">
          <span class="swatch" style="background: {css(effectiveColor) ?? 'transparent'}"></span>
          {tag}
        </span>
      </div>
    </div>
  {/snippet}

  {#if pendingMode && seed}
    <section class="pending-scaffold">
      <p class="note">New country — save the project to edit all its fields.</p>
      <FieldRow label="Tag"><span class="mono">{tag}</span></FieldRow>
      <FieldRow label="Name"><span>{titleName}</span></FieldRow>
      <FieldRow label="Adjective"><span>{effectiveAdjective ?? "—"}</span></FieldRow>
      <FieldRow label="Color">
        <span class="swatch" style="background: {css(effectiveColor) ?? 'transparent'}"></span>
        <span class="mono">
          {effectiveColor ? `rgb(${effectiveColor[0]}, ${effectiveColor[1]}, ${effectiveColor[2]})` : "—"}
        </span>
      </FieldRow>
      <FieldRow label="Capital"><span class="mono">#{seed.capitalId}</span></FieldRow>
    </section>
  {:else if error}
    <p class="error">{error}</p>
  {:else if !details}
    <LoadingState label="Loading country…" />
  {:else if activeTab === "diplomacy"}
    <DiplomacyTab
      {installPath}
      {modPath}
      {tag}
      {queue}
      {calendar}
      {date}
      {startDate}
      {countries}
      historicalRivals={rivals}
      historicalFriends={friends}
      {onopencountry}
      {onopenprovince}
      {onopenmechanics}
    />
  {:else if activeTab === "overview"}
    <nav class="jump-chips" aria-label="Related country tools">
      <button onclick={() => onopenmissions?.(tag)}>Missions ↗</button><button onclick={() => onopenevents?.(tag)}>Events ↗</button><button onclick={() => onopendecisions?.(tag)}>Decisions ↗</button>
    </nav>
    <IdentitySection
      {installPath}
      {modPath}
      {tag}
      {details}
      {queue}
      oncolor={(rgb) => oncolor?.(tag, rgb)}
    />

    <GovernmentSection {installPath} {modPath} {tag} {details} {queue} {date} {onopennaming} {onopenmechanics} />
  {:else if activeTab === "rulers"}
    <RulerSection {installPath} {modPath} {tag} {details} {queue} {date} {startDate} {cultures} {religions} {personalityItems} />

    <QueenSection {installPath} {modPath} {tag} {details} {queue} {date} {startDate} {cultures} {religions} {personalityItems} {countries} />

    <HeirSection {installPath} {modPath} {tag} {details} {queue} {date} {startDate} {cultures} {religions} {personalityItems} />
    <HistoryTimelineSection {installPath} {modPath} {tag} {details} {queue} {calendar} {date} {startDate} {cultures} {religions} {personalityItems} {onopenmechanics} />
  {:else if activeTab === "ideas"}
    <IdeasSection {installPath} {modPath} {tag} {details} {queue} />
  {:else if activeTab === "estates"}
    <EstatesSection {installPath} {modPath} {tag} {queue} {date} {startDate} {onopenestates} />
  {:else if activeTab === "names"}
    <NamePoolsSection {tag} {details} {queue} />

    <ProvinceNamesSection
      {installPath}
      {modPath}
      {queue}
      fileKey={tag}
      kindLabel="country"
      pickRequest={provNamePick}
      onarmpick={onarmprovnamepick}
      onpickconsumed={onprovnamepickconsumed}
    />

  {:else if activeTab === "history"}
    <HistoricalSection {installPath} {modPath} {tag} {details} {queue} />
    <HistoryTimelineSection
      {installPath}
      {modPath}
      {tag}
      {details}
      {queue}
      {calendar}
      {date}
      {startDate}
      {cultures}
      {religions}
      {personalityItems}
      {onopenmechanics}
    />
  {/if}

  {#if canDelete}
    <div class="danger-zone">
      <button class="delete-btn" onclick={openDelete}>Delete country…</button>
    </div>
  {/if}
</SidePanel>

{#if deleteOpen}
  <div class="modal-scrim" role="presentation" onclick={closeDelete}>
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div class="modal" role="dialog" aria-modal="true" tabindex="-1" onclick={(e) => e.stopPropagation()}>
      <h2 class="modal-title">Delete {titleName}?</h2>

      {#if pendingMode}
        <p class="modal-warn">
          This country hasn't been saved yet. Deleting it discards the pending
          creation and everything queued with it.
        </p>
      {:else if blastError}
        <p class="error">{blastError}</p>
      {:else if !blast}
        <p class="dim">Computing impact…</p>
      {:else}
        <p class="modal-lede">This queues one undoable delete. Nothing is written until you Save.</p>

        <ul class="impact">
          <li>
            <strong>{blast.ownedProvinces.length}</strong> owned province{blast.ownedProvinces.length === 1 ? "" : "s"}
            {#if transferTag}→ transferred to {cname(transferTag)}{:else}→ made uncolonized{/if}
          </li>
          {#if blast.relations.length > 0}
            <li><strong>{blast.relations.length}</strong> diplomacy relation{blast.relations.length === 1 ? "" : "s"} removed</li>
          {/if}
          {#if blast.coreReferences.length > 0}
            <li class="danger-text">
              <strong>{blast.coreReferences.length}</strong> province{blast.coreReferences.length === 1 ? "" : "s"}
              keep a dangling core for this tag
            </li>
          {/if}
          {#if blast.tribalOwnerReferences.length > 0}
            <li class="danger-text"><strong>{blast.tribalOwnerReferences.length}</strong> province(s) name it as tribal owner</li>
          {/if}
          {#if blast.toolkitFiles.length > 0}
            <li><strong>{blast.toolkitFiles.length}</strong> toolkit-created file(s) deleted (country, history, flag)</li>
          {:else}
            <li class="dim">Base-game files become unreferenced (not deleted)</li>
          {/if}
        </ul>

        {#if blast.relations.length > 0}
          <div class="impact-block">
            <div class="impact-head">Relations (jump to partner)</div>
            {#each blast.relations as r}
              <button class="jump-row" onclick={() => jumpCountry(r.partner)}>
                <span class="role-badge" class:orphan={r.role === "overlord" || r.role === "subject"}>{r.role}</span>
                {cname(r.partner)}{#if r.subjectType} ({r.subjectType}){/if}
                {#if !r.active}<span class="hist">historical</span>{/if} ↗
              </button>
            {/each}
          </div>
        {/if}

        {#if blast.activeWars.length > 0 || blast.historicalWars.length > 0}
          <div class="impact-block">
            <div class="impact-head danger-text">
              ⚠ Referenced by {blast.activeWars.length} active + {blast.historicalWars.length} historical war(s) —
              these are NOT removed; the war will reference a country that no longer exists.
            </div>
            {#each blast.activeWars as w}
              <button class="jump-row" onclick={() => jumpCountry(w.enemy)}>
                <span class="war-badge active">active</span>
                {w.name ?? w.file}{#if w.enemy} — vs {cname(w.enemy)}{/if} ↗
              </button>
            {/each}
            {#each blast.historicalWars as w}
              <button class="jump-row" onclick={() => jumpCountry(w.enemy)}>
                <span class="war-badge">past</span>
                {w.name ?? w.file}{#if w.enemy} — vs {cname(w.enemy)}{/if} ↗
              </button>
            {/each}
          </div>
        {/if}

        {#if blast.coreReferences.length > 0}
          <div class="impact-block">
            <div class="impact-head">Dangling cores (jump to province)</div>
            <div class="prov-chips">
              {#each blast.coreReferences.slice(0, 40) as pid}
                <button class="chip" onclick={() => jumpProvince(pid)}>#{pid}</button>
              {/each}
              {#if blast.coreReferences.length > 40}<span class="dim">+{blast.coreReferences.length - 40} more</span>{/if}
            </div>
          </div>
        {/if}

        <label class="transfer">
          <span>Owned provinces</span>
          <select bind:value={transferTag}>
            <option value="">— Make uncolonized —</option>
            {#each transferItems as c}
              <option value={c.key}>Transfer to {c.label}</option>
            {/each}
          </select>
        </label>
      {/if}

      <div class="modal-actions">
        <button class="cancel" onclick={closeDelete}>Cancel</button>
        <button
          class="danger"
          disabled={deleting || (!pendingMode && !blast)}
          onclick={confirmDelete}
        >
          {deleting ? "Deleting…" : "Delete country"}
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .head {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    min-width: 0;
  }
  /* Headline over tag+colour: the name is what identifies a country at a
     glance, the tag is the lookup key underneath it. */
  .ident {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }
  .jump-chips { display:flex; flex-wrap:wrap; gap:var(--sp-2); margin-bottom:var(--sp-4); }
  .jump-chips button { border:1px solid var(--border); border-radius:999px; background:var(--bg-3); color:var(--text-2); padding:var(--sp-1) var(--sp-3); cursor:pointer; }
  .jump-chips button:hover { background:var(--bg-hover); color:var(--text-1); }

  .flag-btn {
    display: block;
    padding: 0;
    border: 1px solid transparent;
    border-radius: var(--r-1);
    background: none;
    cursor: zoom-in;
    line-height: 0;
  }

  .flag-btn:hover {
    border-color: var(--accent);
  }

  .flag {
    width: 2.6rem;
    height: 2.6rem;
    object-fit: cover;
    border: 1px solid var(--border);
  }

  .tag-chip {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    font-size: 0.8rem;
    color: var(--text-2);
  }

  .swatch {
    width: 0.75rem;
    height: 0.75rem;
    display: inline-block;
    border: 1px solid var(--border);
  }

  .dim {
    color: var(--text-2);
  }

  .error {
    color: var(--err);
  }

  .pending-scaffold {
    padding: 0.4rem 0 0.6rem;
  }

  .note {
    margin: 0 0 0.6rem;
    padding: 0.4rem 0.6rem;
    background: rgba(240, 180, 41, 0.12);
    border-left: 3px solid var(--warn);
    color: var(--text-1);
    font-size: 0.82rem;
  }

  .mono {
    font-family: ui-monospace, monospace;
    font-size: 0.85rem;
    color: var(--text-1);
  }

  /* --- Delete country (danger zone + confirm dialog) --- */
  .danger-zone {
    margin-top: 0.9rem;
    padding-top: 0.7rem;
    border-top: 1px solid var(--border);
  }
  .delete-btn {
    width: 100%;
    border: 1px solid var(--danger-bg);
    background: var(--bg-1);
    color: var(--err);
    font-family: inherit;
    font-size: 0.82rem;
    padding: 0.35rem 0.6rem;
    cursor: pointer;
  }
  .delete-btn:hover {
    background: var(--danger-bg);
    color: var(--text-inverse);
  }

  .modal-scrim {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.5);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: var(--z-modal);
  }
  .modal {
    width: min(28rem, 92vw);
    max-height: 84vh;
    overflow-y: auto;
    background: var(--bg-2);
    border: 1px solid var(--danger-bg);
    box-shadow: 0 8px 30px rgba(0, 0, 0, 0.5);
    padding: 0.9rem 1rem;
    display: flex;
    flex-direction: column;
    gap: 0.55rem;
  }
  .modal-title {
    margin: 0;
    font-size: 1rem;
    color: var(--text-inverse);
  }
  .modal-lede {
    margin: 0;
    font-size: 0.78rem;
    color: var(--text-2);
  }
  .modal-warn {
    margin: 0;
    padding: 0.45rem 0.6rem;
    background: rgba(240, 180, 41, 0.12);
    border-left: 3px solid var(--warn);
    color: var(--text-1);
    font-size: 0.82rem;
  }
  .impact {
    margin: 0;
    padding-left: 1.1rem;
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
    font-size: 0.82rem;
    color: var(--text-1);
  }
  .danger-text {
    color: var(--err);
  }
  .impact-block {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
    border: 1px solid var(--border);
    background: var(--bg-2);
    padding: 0.4rem 0.45rem;
  }
  .impact-head {
    font-size: 0.72rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-2);
  }
  .jump-row {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    width: 100%;
    border: none;
    background: transparent;
    color: var(--text-1);
    font-family: inherit;
    font-size: 0.8rem;
    padding: 0.15rem 0.1rem;
    cursor: pointer;
    text-align: left;
  }
  .jump-row:hover {
    color: var(--text-inverse);
  }
  .role-badge,
  .war-badge {
    font-size: 0.62rem;
    padding: 0.02rem 0.3rem;
    color: var(--text-inverse);
    background: var(--accent);
    text-transform: uppercase;
  }
  .role-badge.orphan {
    background: var(--warn);
  }
  .war-badge {
    background: var(--text-3);
  }
  .war-badge.active {
    background: var(--err);
  }
  .hist {
    font-size: 0.66rem;
    color: var(--text-2);
  }
  .prov-chips {
    display: flex;
    flex-wrap: wrap;
    gap: 0.25rem;
  }
  .chip {
    border: 1px solid var(--border);
    background: var(--bg-1);
    color: var(--text-1);
    font-family: ui-monospace, monospace;
    font-size: 0.72rem;
    padding: 0.05rem 0.3rem;
    cursor: pointer;
  }
  .chip:hover {
    background: var(--accent);
    color: var(--text-inverse);
  }
  .transfer {
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
  }
  .transfer > span {
    font-size: 0.68rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-2);
  }
  .transfer select {
    background: var(--bg-1);
    border: 1px solid var(--border);
    color: var(--text-1);
    font-family: inherit;
    font-size: 0.83rem;
    padding: 0.25rem 0.3rem;
    outline: none;
  }
  .modal-actions {
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
    margin-top: 0.3rem;
  }
  .modal-actions .cancel {
    border: 1px solid var(--border);
    background: var(--bg-3);
    color: var(--text-1);
    font-family: inherit;
    font-size: 0.82rem;
    padding: 0.3rem 0.8rem;
    cursor: pointer;
  }
  .modal-actions .cancel:hover {
    background: var(--accent);
    color: var(--text-inverse);
  }
  .modal-actions .danger {
    border: 1px solid var(--danger-bg);
    background: var(--danger-bg);
    color: var(--text-inverse);
    font-family: inherit;
    font-size: 0.82rem;
    padding: 0.3rem 0.8rem;
    cursor: pointer;
  }
  .modal-actions .danger:hover:not(:disabled) {
    background: var(--err);
  }
  .modal-actions .danger:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
</style>
