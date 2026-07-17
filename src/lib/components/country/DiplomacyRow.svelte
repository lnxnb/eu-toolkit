<!--
  DiplomacyRow — one relation row in the Diplomacy tab (Sprint 3.3). Partner flag
  (session-cached) + localized name, subject-type badge for dependencies, date
  range, expired/future/pending badges. Click the partner to jump to its panel
  (keeping the Diplomacy tab open). Expand to edit the two dates inline, or delete
  with a confirm step. Pending (unsaved) relations are shown read-only with a badge
  — edit/undo them from the queue instead.
-->
<script lang="ts">
  import { DatePicker } from "$lib/components/ui";
  import { getFlagUrl } from "$lib/flagCache";
  import type { DropdownItem } from "$lib/components/ui";
  import { formatDate, type Calendar } from "$lib/calendar";
  import type { Relation } from "./diplomacy";
  import { relationTiming } from "./diplomacy";

  let {
    installPath,
    modPath,
    relation,
    partnerTag,
    subLabel = null,
    countries,
    calendar = null,
    onopenpartner,
    ondateedit,
    ondelete,
  }: {
    installPath: string;
    modPath: string | null;
    relation: Relation;
    partnerTag: string;
    /** Extra role/subject-type label (e.g. "vassal", "guarantor"). */
    subLabel?: string | null;
    countries: DropdownItem[];
    /** The mod's calendar (Sprint 12.4): the date range renders its months. */
    calendar?: Calendar | null;
    onopenpartner: (tag: string) => void;
    ondateedit: (which: "start" | "end", value: string) => void;
    ondelete: () => void;
  } = $props();

  /** A relation date shown with the mod's calendar; "—" when unset. */
  function showDate(d: string | null | undefined): string {
    if (!d) return "—";
    return calendar ? formatDate(d, calendar) : d;
  }

  let flagUrl = $state<string | null>(null);
  $effect(() => {
    let alive = true;
    getFlagUrl(installPath, modPath, partnerTag).then((u) => {
      if (alive) flagUrl = u;
    });
    return () => {
      alive = false;
    };
  });

  const partner = $derived(countries.find((c) => c.key === partnerTag));
  const partnerName = $derived(partner?.label ?? partnerTag);
  const timing = $derived(relationTiming(relation));

  let expanded = $state(false);
  let confirming = $state(false);
  let startDraft = $state("1444.11.11");
  let endDraft = $state("9999.1.1");

  function toggleEdit() {
    if (!expanded) {
      startDraft = relation.start_date ?? "1444.11.11";
      endDraft = relation.end_date ?? "9999.1.1";
    }
    expanded = !expanded;
    confirming = false;
  }
</script>

<div class="row" class:pending={relation.pending}>
  <div class="main">
    <button class="partner" onclick={() => onopenpartner(partnerTag)} title="Open {partnerName}">
      {#if flagUrl}
        <img class="flag" src={flagUrl} alt="" />
      {:else}
        <span class="flag ph" style="background: {partner?.swatch ?? '#3a4150'}"></span>
      {/if}
      <span class="name">{partnerName}</span>
      <span class="arrow">↗</span>
    </button>

    {#if subLabel}
      <span class="badge type">{subLabel}</span>
    {/if}
    {#if timing === "expired"}
      <span class="badge expired">expired</span>
    {:else if timing === "future"}
      <span class="badge future">future</span>
    {/if}
    {#if relation.pending}
      <span class="badge pend">pending save</span>
    {/if}

    <span class="spacer"></span>

    <span class="dates" title="{relation.start_date ?? '—'} → {relation.end_date ?? '—'}">
      {showDate(relation.start_date)} → {showDate(relation.end_date)}
    </span>

    {#if !relation.pending}
      <button class="mini" title="Edit dates" onclick={toggleEdit}>✎</button>
      <button class="mini danger" title="Delete relation" onclick={() => (confirming = true)}>✕</button>
    {/if}
  </div>

  {#if confirming && !relation.pending}
    <div class="confirm">
      <span>Delete this relation?</span>
      <button class="mini danger" onclick={() => { ondelete(); confirming = false; }}>Delete</button>
      <button class="mini" onclick={() => (confirming = false)}>Cancel</button>
    </div>
  {/if}

  {#if expanded && !relation.pending}
    <div class="editor">
      <label>
        <span>Start</span>
        <DatePicker bind:value={startDraft} onchange={(v) => ondateedit("start", v)} />
      </label>
      <label>
        <span>End</span>
        <DatePicker bind:value={endDraft} onchange={(v) => ondateedit("end", v)} />
      </label>
    </div>
  {/if}
</div>

<style>
  .row {
    border-bottom: 1px solid #1f242c;
    padding: 0.15rem 0.1rem;
  }
  .row:last-child {
    border-bottom: none;
  }
  .row.pending {
    background: rgba(74, 109, 167, 0.12);
  }
  .main {
    display: flex;
    align-items: center;
    gap: 0.3rem;
  }
  .partner {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    border: none;
    background: transparent;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.82rem;
    padding: 0.1rem 0.15rem;
    cursor: pointer;
    min-width: 0;
  }
  .partner:hover {
    color: #fff;
  }
  .flag {
    width: 1.2rem;
    height: 1.2rem;
    object-fit: cover;
    border: 1px solid #1f242c;
    flex: none;
  }
  .flag.ph {
    display: inline-block;
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 7rem;
  }
  .arrow {
    color: #8a919c;
    font-size: 0.7rem;
  }
  .spacer {
    flex: 1;
  }
  .dates {
    font-size: 0.72rem;
    color: #9ca3af;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .badge {
    font-size: 0.62rem;
    padding: 0.02rem 0.3rem;
    border: 1px solid rgba(0, 0, 0, 0.35);
    color: #fff;
    text-transform: uppercase;
    letter-spacing: 0.03em;
  }
  .badge.type {
    background: #4a6da7;
    text-transform: none;
    letter-spacing: 0;
  }
  .badge.expired {
    background: #8a5a2b;
  }
  .badge.future {
    background: #566;
  }
  .badge.pend {
    background: #6b4aa7;
  }
  .mini {
    border: 1px solid #1f242c;
    background: #2b323d;
    color: #cfd4db;
    cursor: pointer;
    font-size: 0.72rem;
    padding: 0.05rem 0.3rem;
    flex: none;
  }
  .mini:hover {
    background: #4a6da7;
    color: #fff;
  }
  .mini.danger:hover {
    background: #c0392b;
  }
  .confirm,
  .editor {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.3rem 0.15rem 0.15rem;
    font-size: 0.75rem;
    color: #cfd4db;
    flex-wrap: wrap;
  }
  .editor label {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
  }
  .editor span {
    font-size: 0.7rem;
    color: #8a919c;
    text-transform: uppercase;
  }
</style>
