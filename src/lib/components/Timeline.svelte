<!--
  Timeline — reusable dated-history view (SPRINT.md 2.3). Renders a chronological
  list of dated blocks under a "1444 state" anchor, and emits typed edit INTENTS
  (never touches the pending-edit queue itself). First consumer: the province
  panel (Sprint 2); reused later for country history (rulers/heirs over time).

  ## Host contract
  Props:
  - `blocks: TimelineBlock[]`  — the dated blocks (map the backend `dated_blocks`;
        `date`, `postStart`, `occurrenceIndex`, `entries[{key,value,isBlock}]`).
        Display is sorted by date automatically (duplicate dates keep file order
        via `occurrenceIndex`); the prop itself need not be pre-sorted.
  - `startDate = "1444.11.11"` — default date for new entries; the post-start
        boundary is carried per-block by `postStart` (host computes it).
  - `editable = true` — set false for read-only (country history preview, etc.).
  - `anchorLabel = "1444 state"` — label of the anchor the timeline hangs under.
  - `anchor?` (snippet)       — optional content rendered inside the anchor row
        (e.g. the effective-1444 summary). Omit for just the label.
  - `onchange?: (intent: TimelineIntent) => void` — the ONLY output. The host maps
        each intent to `TypedEdit`s per the recipe in timeline.ts. Intents:
        addEntry | editValue | editEntry | deleteEntry, each carrying full
        addressing (date + occurrenceIndex + entryIndex + key/value).

  The component owns only ephemeral UI state (which row is being edited, the
  add-entry draft, delete confirmation). It re-renders purely from `blocks`, so
  after the host applies an intent and updates its data, the view follows.
-->
<script lang="ts">
  import { DatePicker } from "$lib/components/ui";
  import { formatDate, type Calendar } from "$lib/calendar";
  import {
    type TimelineBlock,
    type TimelineIntent,
    compareBlocks,
  } from "./timeline";

  let {
    blocks,
    startDate = "1444.11.11",
    editable = true,
    anchorLabel = "1444 state",
    calendar = null,
    anchor,
    onchange,
  }: {
    blocks: TimelineBlock[];
    startDate?: string;
    editable?: boolean;
    anchorLabel?: string;
    /** The mod's calendar (Sprint 12.4): dated-block headers render its months. */
    calendar?: Calendar | null;
    anchor?: import("svelte").Snippet;
    onchange?: (intent: TimelineIntent) => void;
  } = $props();

  // Chronological display order; duplicate dates keep file order.
  let sorted = $derived([...blocks].sort(compareBlocks));

  // --- Per-row edit state (keyed by a stable block+entry signature) ----------
  // We identify a row by date|occurrenceIndex|entryIndex so re-sorts don't lose
  // the active editor.
  function rowKey(date: string, occ: number, i: number): string {
    return `${date}#${occ}#${i}`;
  }

  let editingRow = $state<string | null>(null);
  let editKey = $state("");
  let editValue = $state("");
  let confirmDelete = $state<string | null>(null);

  function startEdit(date: string, occ: number, i: number, key: string, value: string) {
    editingRow = rowKey(date, occ, i);
    editKey = key;
    editValue = value;
    confirmDelete = null;
  }

  function cancelEdit() {
    editingRow = null;
  }

  function saveEdit(
    date: string,
    occ: number,
    i: number,
    oldKey: string,
    oldValue: string,
  ) {
    const key = editKey.trim();
    const value = editValue.trim();
    if (!key) return;
    if (key === oldKey && value === oldValue) {
      editingRow = null;
      return;
    }
    if (key === oldKey) {
      emit({
        kind: "editValue",
        date,
        occurrenceIndex: occ,
        entryIndex: i,
        key,
        oldValue,
        value,
      });
    } else {
      emit({
        kind: "editEntry",
        date,
        occurrenceIndex: occ,
        entryIndex: i,
        oldKey,
        oldValue,
        key,
        value,
      });
    }
    editingRow = null;
  }

  function askDelete(date: string, occ: number, i: number) {
    confirmDelete = rowKey(date, occ, i);
    editingRow = null;
  }

  function doDelete(date: string, occ: number, i: number, key: string, value: string) {
    emit({ kind: "deleteEntry", date, occurrenceIndex: occ, entryIndex: i, key, value });
    confirmDelete = null;
  }

  // --- Add-entry flow --------------------------------------------------------
  let adding = $state(false);
  // Reset from `startDate` on open (beginAdd); the literal default is only used
  // before the form is ever shown.
  let addDate = $state("1444.11.11");
  let addKey = $state("");
  let addValue = $state("");

  function beginAdd() {
    adding = true;
    addDate = startDate;
    addKey = "";
    addValue = "";
  }

  function submitAdd() {
    const key = addKey.trim();
    const value = addValue.trim();
    if (!key) return;
    emit({ kind: "addEntry", date: addDate, key, value });
    adding = false;
  }

  function emit(intent: TimelineIntent) {
    onchange?.(intent);
  }

  // Post-start boundary check for the add-flow draft (visual hint only).
  import { parseDate } from "./timeline";
  let addIsPostStart = $derived.by(() => {
    const [ay, am, ad] = parseDate(addDate);
    const [sy, sm, sd] = parseDate(startDate);
    return ay * 10000 + am * 100 + ad > sy * 10000 + sm * 100 + sd;
  });
</script>

<div class="timeline">
  <!-- 1444 state anchor -->
  <div class="anchor">
    <span class="anchor-dot" aria-hidden="true"></span>
    <div class="anchor-body">
      <span class="anchor-label">{anchorLabel}</span>
      {#if anchor}
        <div class="anchor-content">{@render anchor()}</div>
      {/if}
    </div>
  </div>

  {#if sorted.length === 0}
    <p class="empty">No dated history entries.</p>
  {/if}

  {#each sorted as block (rowKey(block.date, block.occurrenceIndex, -1))}
    <div class="block" class:post-start={block.postStart}>
      <div class="block-head">
        <span class="date" title={block.date}>
          {calendar ? formatDate(block.date, calendar) : block.date}
        </span>
        {#if block.postStart}
          <span class="badge post" title="After 1444.11.11 — does not affect the 1444 map"
            >post-start</span
          >
        {/if}
        {#if block.occurrenceIndex > 0}
          <span class="badge dup" title="Another block shares this date (file order #{block.occurrenceIndex})"
            >dup #{block.occurrenceIndex}</span
          >
        {/if}
      </div>

      <div class="entries">
        {#each block.entries as entry, i (rowKey(block.date, block.occurrenceIndex, i))}
          {@const key = rowKey(block.date, block.occurrenceIndex, i)}
          <div class="entry">
            {#if editingRow === key}
              <input
                class="edit-key"
                bind:value={editKey}
                aria-label="Entry key"
                placeholder="key"
              />
              <span class="eq">=</span>
              <input
                class="edit-val"
                bind:value={editValue}
                aria-label="Entry value"
                placeholder="value"
                disabled={entry.isBlock}
              />
              <button class="mini ok" onclick={() =>
                saveEdit(block.date, block.occurrenceIndex, i, entry.key, entry.value)}
                >✓</button
              >
              <button class="mini cancel" onclick={cancelEdit}>×</button>
            {:else if confirmDelete === key}
              <span class="confirm-text">Delete <code>{entry.key}</code>?</span>
              <button class="mini danger" onclick={() =>
                doDelete(block.date, block.occurrenceIndex, i, entry.key, entry.value)}
                >Yes</button
              >
              <button class="mini cancel" onclick={() => (confirmDelete = null)}>No</button>
            {:else}
              <span class="k">{entry.key}</span>
              <span class="eq">=</span>
              <span class="v" class:block-val={entry.isBlock}>{entry.value}</span>
              {#if editable}
                <span class="row-actions">
                  <button
                    class="mini"
                    title="Edit"
                    onclick={() =>
                      startEdit(
                        block.date,
                        block.occurrenceIndex,
                        i,
                        entry.key,
                        entry.value,
                      )}>✎</button
                  >
                  <button
                    class="mini"
                    title="Delete"
                    onclick={() => askDelete(block.date, block.occurrenceIndex, i)}>🗑</button
                  >
                </span>
              {/if}
            {/if}
          </div>
        {/each}
      </div>
    </div>
  {/each}

  {#if editable}
    <div class="add-zone">
      {#if !adding}
        <button class="add-btn" onclick={beginAdd}>+ Add dated entry</button>
      {:else}
        <div class="add-form">
          <div class="add-row">
            <span class="add-label">Date</span>
            <DatePicker bind:value={addDate} min="1.1.1" max="9999.1.1" />
            {#if addIsPostStart}
              <span class="badge post">post-start</span>
            {/if}
          </div>
          <div class="add-row">
            <input class="edit-key" bind:value={addKey} placeholder="key (e.g. religion)" />
            <span class="eq">=</span>
            <input class="edit-val" bind:value={addValue} placeholder="value (e.g. protestant)" />
          </div>
          <div class="add-actions">
            <button class="add-btn ok" onclick={submitAdd} disabled={!addKey.trim()}>Add</button>
            <button class="add-btn" onclick={() => (adding = false)}>Cancel</button>
          </div>
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .timeline {
    display: flex;
    flex-direction: column;
    font-size: 0.82rem;
    color: #cfd4db;
  }

  .anchor {
    display: flex;
    align-items: flex-start;
    gap: 0.5rem;
    padding: 0.4rem 0.5rem;
    background: #2b323d;
    border: 1px solid #1f242c;
    border-left: 3px solid #4a6da7;
  }

  .anchor-dot {
    width: 0.6rem;
    height: 0.6rem;
    border-radius: 50%;
    background: #4a6da7;
    margin-top: 0.15rem;
    flex: none;
  }

  .anchor-body {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    flex: 1;
  }

  .anchor-label {
    font-size: 0.72rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: #9aa2ad;
    font-weight: 600;
  }

  .empty {
    margin: 0.4rem 0.5rem;
    color: #8a919c;
    font-style: italic;
  }

  .block {
    border: 1px solid #1f242c;
    border-top: none;
    background: #21262e;
  }

  .block.post-start {
    opacity: 0.92;
  }

  .block-head {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.25rem 0.5rem;
    background: #303844;
    border-bottom: 1px solid #1f242c;
  }

  .date {
    font-variant-numeric: tabular-nums;
    font-weight: 600;
    color: #dfe4ea;
  }

  .badge {
    font-size: 0.66rem;
    line-height: 1;
    padding: 0.1rem 0.35rem;
    color: #fff;
    text-transform: uppercase;
    letter-spacing: 0.03em;
  }

  .badge.post {
    background: #a1662f;
  }

  .badge.dup {
    background: #6b46c1;
  }

  .entries {
    display: flex;
    flex-direction: column;
  }

  .entry {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    padding: 0.2rem 0.5rem;
    border-top: 1px solid #191d23;
  }

  .entry:first-child {
    border-top: none;
  }

  .k {
    color: #9ab0d0;
  }

  .eq {
    color: #6c7480;
  }

  .v {
    color: #cfd4db;
    word-break: break-word;
  }

  .v.block-val {
    color: #8a919c;
    font-style: italic;
  }

  .row-actions {
    margin-left: auto;
    display: flex;
    gap: 0.15rem;
    opacity: 0;
    transition: opacity 0.1s;
  }

  .entry:hover .row-actions {
    opacity: 1;
  }

  .mini {
    border: 1px solid #1f242c;
    background: #2b323d;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.78rem;
    line-height: 1;
    padding: 0.15rem 0.35rem;
    cursor: pointer;
  }

  .mini:hover {
    background: #4a6da7;
    color: #fff;
  }

  .mini.danger:hover {
    background: #7a3f3f;
  }

  .confirm-text {
    color: #e0b0b0;
  }

  .confirm-text code {
    color: #cfd4db;
    background: #191d23;
    padding: 0 0.25rem;
  }

  .edit-key,
  .edit-val {
    background: #191d23;
    border: 1px solid #1f242c;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.8rem;
    padding: 0.15rem 0.3rem;
    outline: none;
  }

  .edit-key {
    width: 8rem;
  }

  .edit-val {
    flex: 1;
    min-width: 6rem;
  }

  .edit-val:disabled {
    opacity: 0.5;
  }

  .add-zone {
    margin-top: 0.4rem;
  }

  .add-btn {
    border: 1px solid #1f242c;
    background: #3f4855;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.8rem;
    padding: 0.25rem 0.6rem;
    cursor: pointer;
  }

  .add-btn:hover {
    background: #4a6da7;
    color: #fff;
  }

  .add-btn.ok {
    background: #3a5a86;
  }

  .add-btn:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .add-form {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    padding: 0.5rem;
    border: 1px solid #1f242c;
    background: #21262e;
  }

  .add-row {
    display: flex;
    align-items: center;
    gap: 0.4rem;
  }

  .add-label {
    font-size: 0.72rem;
    color: #8a919c;
    text-transform: uppercase;
  }

  .add-actions {
    display: flex;
    gap: 0.35rem;
  }
</style>
