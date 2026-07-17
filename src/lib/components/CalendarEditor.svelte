<!--
  CalendarEditor — Sprint 12.4. The small panel behind the date selector's
  "Edit calendar…" row. Three sections, all queuing standard pending edits:

  - Month names: 12 fields prefilled from the resolved calendar loc (folded with
    pending edits by the host). Each change queues a `locOverride` on the plain
    month key (January..December) — the same 0.3 writer path as any rename.
  - Era/year label: the raw `WORLD_YEAR` template (with its `$YEAR$` token),
    editable → `locOverride`. This is the ONLY era/date-display key EU4 ships
    (audited: no `DATE_*`/`MONTH_n` family exists), so it is the whole story for
    "The world $YEAR$ AUC"-style branding.
  - Playable range: START_DATE / END_DATE from the effective defines, editable →
    `setDefine` edits (the project defines override, copy-on-write in the backend).

  The editor never touches the queue projection itself — it pushes composites and
  lets the host re-derive. Month/era edits coalesce per key so holding-and-typing
  in a field collapses to one undo unit.
-->
<script lang="ts">
  import { DatePicker } from "$lib/components/ui";
  import { MONTH_KEYS, WORLD_YEAR_KEY } from "$lib/calendar";
  import type { EditQueue } from "$lib/edits.svelte";

  let {
    queue,
    months,
    worldYear,
    startDate,
    endDate,
    onrange,
    onback,
  }: {
    queue: EditQueue;
    /** The 12 resolved month names (host folds base loc + pending), prefill. */
    months: string[];
    /** The raw `WORLD_YEAR` template (folded), or null when the mod defines none. */
    worldYear: string | null;
    /** Effective playable bounds (from get_defines_dates, host-optimistic). */
    startDate: string;
    endDate: string;
    /** Host callback to optimistically reflect a START/END edit. */
    onrange?: (which: "START_DATE" | "END_DATE", value: string) => void;
    /** Return to the bookmark list. */
    onback?: () => void;
  } = $props();

  function commitMonth(i: number, value: string) {
    const key = MONTH_KEYS[i];
    const v = value.trim();
    if (v === months[i]) return;
    queue.push({
      label: `Rename month "${MONTH_KEYS[i]}"`,
      edits: [{ kind: "locOverride", key, value: v }],
      coalesceKey: `month:${key}`,
    });
  }

  function commitEra(value: string) {
    const v = value.trim();
    if (v === (worldYear ?? "")) return;
    queue.push({
      label: "Edit year label (WORLD_YEAR)",
      edits: [{ kind: "locOverride", key: WORLD_YEAR_KEY, value: v }],
      coalesceKey: `loc:${WORLD_YEAR_KEY}`,
    });
  }

  function commitRange(which: "START_DATE" | "END_DATE", value: string) {
    const cur = which === "START_DATE" ? startDate : endDate;
    if (value === cur) return;
    queue.push({
      label: `Set ${which === "START_DATE" ? "start" : "end"} of playable range`,
      edits: [{ kind: "setDefine", key: which, value }],
      coalesceKey: `define:${which}`,
    });
    onrange?.(which, value);
  }
</script>

<div class="cal-editor">
  <div class="cal-head">
    <button class="back" title="Back to start dates" onclick={() => onback?.()}>←</button>
    <span class="cal-title">Edit calendar</span>
  </div>

  <div class="cal-body">
    <section class="cal-sec">
      <div class="sec-label">Month names</div>
      <div class="months">
        {#each MONTH_KEYS as key, i (key)}
          <label class="month-field">
            <span class="mn-key">{key}</span>
            <input
              class="mn-input"
              value={months[i] ?? ""}
              placeholder={key}
              onchange={(e) => commitMonth(i, (e.currentTarget as HTMLInputElement).value)}
            />
          </label>
        {/each}
      </div>
    </section>

    <section class="cal-sec">
      <div class="sec-label">Year label</div>
      <p class="hint">
        The era/year template. Use <code>$YEAR$</code> where the year goes
        (e.g. <code>The world $YEAR$ AUC</code>). Shown after the day and month
        everywhere the toolkit renders a date.
      </p>
      <input
        class="era-input"
        value={worldYear ?? ""}
        placeholder="The world $YEAR$ AD"
        onchange={(e) => commitEra((e.currentTarget as HTMLInputElement).value)}
      />
    </section>

    <section class="cal-sec">
      <div class="sec-label">Playable range</div>
      <p class="hint">
        The bounds a start date must fall within to be playable
        (<code>NGame.START_DATE</code> / <code>END_DATE</code>).
      </p>
      <div class="range-row">
        <span class="rr-label">Start</span>
        <DatePicker value={startDate} onchange={(v) => commitRange("START_DATE", v)} />
      </div>
      <div class="range-row">
        <span class="rr-label">End</span>
        <DatePicker value={endDate} onchange={(v) => commitRange("END_DATE", v)} />
      </div>
    </section>
  </div>
</div>

<style>
  .cal-editor {
    display: flex;
    flex-direction: column;
    max-height: 24rem;
  }

  .cal-head {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.4rem 0.5rem;
    border-bottom: 1px solid #2b323d;
  }

  .back {
    border: 1px solid #2b323d;
    background: #2b323d;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.85rem;
    line-height: 1;
    padding: 0.15rem 0.45rem;
    cursor: pointer;
  }

  .back:hover {
    background: #4a6da7;
    color: #fff;
  }

  .cal-title {
    font-size: 0.72rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: #9ca3af;
  }

  .cal-body {
    overflow-y: auto;
    padding: 0.25rem 0.6rem 0.6rem;
  }

  .cal-sec {
    padding: 0.5rem 0 0.35rem;
    border-bottom: 1px solid #2b323d;
  }

  .cal-sec:last-child {
    border-bottom: none;
  }

  .sec-label {
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: #9ca3af;
    margin-bottom: 0.35rem;
  }

  .hint {
    margin: 0 0 0.4rem;
    font-size: 0.72rem;
    color: #8a919c;
    line-height: 1.3;
  }

  .hint code {
    background: #21262e;
    color: #cfd4db;
    padding: 0 0.2rem;
  }

  .months {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.3rem 0.5rem;
  }

  .month-field {
    display: flex;
    flex-direction: column;
    gap: 0.1rem;
  }

  .mn-key {
    font-size: 0.62rem;
    text-transform: uppercase;
    letter-spacing: 0.03em;
    color: #7f8792;
  }

  .mn-input,
  .era-input {
    background: #21262e;
    border: 1px solid #2b323d;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.8rem;
    padding: 0.2rem 0.35rem;
    outline: none;
    width: 100%;
    box-sizing: border-box;
  }

  .mn-input:focus,
  .era-input:focus {
    border-color: #4a6da7;
  }

  .range-row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    margin-bottom: 0.35rem;
  }

  .rr-label {
    font-size: 0.7rem;
    text-transform: uppercase;
    color: #8a919c;
    width: 2.5rem;
  }
</style>
