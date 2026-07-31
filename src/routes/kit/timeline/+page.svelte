<!--
  /kit/timeline — DEV BENCH for the Sprint 2.3 Timeline component and the Sprint
  2.4 get_province_details backend payload. NOT shipped UI.

  Two panes:
  (a) Synthetic timeline exercising every intent (add / editValue / editEntry /
      deleteEntry). Intents are logged as JSON so you can see the exact host
      payload; a local reducer also mutates the sample blocks so the view reacts.
  (b) Live province lookup by id (try/catch — falls back gracefully if the
      orchestrator hasn't registered get_province_details yet). Shows the full
      payload: effective-1444 vs top-level diff, the raw-remainder section, and
      the dated timeline driven by the real data.
-->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import Timeline from "$lib/components/Timeline.svelte";
  import type {
    TimelineBlock,
    TimelineIntent,
    TimelineEntry,
  } from "$lib/components/timeline";

  const DEFAULT_INSTALL =
    "C:\\Program Files (x86)\\Steam\\steamapps\\common\\Europa Universalis IV";

  // --- (a) Synthetic bench ----------------------------------------------------
  // A pre-start override (1440 protestant) + duplicate dates (two 1500.1.1) +
  // post-start blocks + a block-valued entry to prove read-only value handling.
  let sampleBlocks = $state<TimelineBlock[]>([
    {
      date: "1440.1.1",
      postStart: false,
      occurrenceIndex: 0,
      entries: [
        { key: "religion", value: "protestant" },
        { key: "add_core", value: "ENG" },
      ],
    },
    {
      date: "1500.1.1",
      postStart: true,
      occurrenceIndex: 0,
      entries: [{ key: "unrest", value: "5" }],
    },
    {
      date: "1500.1.1",
      postStart: true,
      occurrenceIndex: 1,
      entries: [{ key: "unrest", value: "6" }],
    },
    {
      date: "1453.5.29",
      postStart: true,
      occurrenceIndex: 0,
      entries: [
        { key: "owner", value: "TUR" },
        { key: "controller", value: "TUR" },
        { key: "monarch", value: "{ name = Mehmed adm = 6 }", isBlock: true },
      ],
    },
  ]);

  let intentLog = $state<string[]>([]);

  // Local reducer so the synthetic view reacts to intents (a host would instead
  // push TypedEdits; here we mutate the sample directly for demonstration).
  function applyLocally(intent: TimelineIntent) {
    if (intent.kind === "addEntry") {
      const existing = sampleBlocks.filter((b) => b.date === intent.date);
      sampleBlocks = [
        ...sampleBlocks,
        {
          date: intent.date,
          postStart:
            intent.date.localeCompare("1444.11.11", undefined, { numeric: true }) > 0,
          occurrenceIndex: existing.length,
          entries: [{ key: intent.key, value: intent.value }],
        },
      ];
      return;
    }
    const b = sampleBlocks.find(
      (x) => x.date === intent.date && x.occurrenceIndex === intent.occurrenceIndex,
    );
    if (!b) return;
    if (intent.kind === "deleteEntry") {
      b.entries = b.entries.filter((_, i) => i !== intent.entryIndex);
      // Drop now-empty blocks.
      sampleBlocks = sampleBlocks.filter((x) => x.entries.length > 0);
    } else if (intent.kind === "editValue") {
      const e = b.entries[intent.entryIndex];
      if (e) e.value = intent.value;
    } else if (intent.kind === "editEntry") {
      const e = b.entries[intent.entryIndex];
      if (e) {
        e.key = intent.key;
        e.value = intent.value;
      }
    }
    sampleBlocks = [...sampleBlocks];
  }

  function onSampleChange(intent: TimelineIntent) {
    intentLog = [JSON.stringify(intent), ...intentLog].slice(0, 12);
    applyLocally(intent);
  }

  // --- (b) Live province lookup ----------------------------------------------
  interface KeyName {
    key: string;
    name: string;
  }
  interface Snapshot {
    owner: string | null;
    controller: string | null;
    cores: string[];
    claims: string[];
    culture: string | null;
    religion: string | null;
    trade_goods: string | null;
    latent_trade_goods: string | null;
    base_tax: number | null;
    base_production: number | null;
    base_manpower: number | null;
    capital: string | null;
    is_city: boolean | null;
    hre: boolean | null;
    seat_in_parliament: boolean | null;
    discovered_by: string[];
    buildings: string[];
    center_of_trade: number | null;
    extra_cost: number | null;
    tribal_owner: string | null;
    reformation_center: string | null;
  }
  interface RawStatement {
    key: string;
    value: string;
    is_block: boolean;
  }
  interface DatedBlockBE {
    date: string;
    post_start: boolean;
    occurrence_index: number;
    entries: RawStatement[];
  }
  interface Geography {
    area: KeyName | null;
    region: KeyName | null;
    superregion: KeyName | null;
    trade_node: KeyName | null;
    climate: KeyName | null;
    winter: KeyName | null;
    impassable: boolean;
    monsoon: KeyName | null;
    continent: KeyName | null;
    terrain_override: KeyName | null;
    water: boolean;
  }
  interface ProvinceDetails {
    id: number;
    file: string;
    exists: boolean;
    localized_name: string;
    definition_name: string;
    owner: string | null;
    top_level: Snapshot;
    effective_1444: Snapshot;
    raw_remainder: RawStatement[];
    dated_blocks: DatedBlockBE[];
    geography: Geography;
  }

  let provId = $state(151);
  let details = $state<ProvinceDetails | null>(null);
  let liveStatus = $state("");
  let liveLog = $state<string[]>([]);
  let loading = $state(false);

  async function loadProvince() {
    loading = true;
    liveStatus = "";
    liveLog = [];
    try {
      const d = await invoke<ProvinceDetails>("get_province_details", {
        installPath: DEFAULT_INSTALL,
        modPath: null,
        id: provId,
      });
      details = d;
      liveStatus = `Loaded province ${d.id} — ${d.localized_name} (${d.file})`;
    } catch (e) {
      const msg = String(e);
      details = null;
      if (/not found|not registered|unknown command|allowlist/i.test(msg)) {
        liveStatus =
          "get_province_details isn't registered yet — synthetic pane still works. (The orchestrator wires it into lib.rs later.)";
      } else {
        liveStatus = `Could not load: ${msg}`;
      }
    } finally {
      loading = false;
    }
  }

  // Real dated_blocks → Timeline blocks.
  let liveTimelineBlocks = $derived<TimelineBlock[]>(
    (details?.dated_blocks ?? []).map((b) => ({
      date: b.date,
      postStart: b.post_start,
      occurrenceIndex: b.occurrence_index,
      entries: b.entries.map(
        (e): TimelineEntry => ({ key: e.key, value: e.value, isBlock: e.is_block }),
      ),
    })),
  );

  function onLiveChange(intent: TimelineIntent) {
    liveLog = [JSON.stringify(intent), ...liveLog].slice(0, 12);
  }

  // top_level vs effective_1444 diff — which modeled fields the pre-start dated
  // blocks changed. Read-only display of the "re-derive" truth.
  const DIFF_FIELDS: (keyof Snapshot)[] = [
    "owner",
    "controller",
    "culture",
    "religion",
    "trade_goods",
    "base_tax",
    "base_production",
    "base_manpower",
  ];
  let effectiveDiff = $derived.by(() => {
    if (!details) return [];
    const out: { field: string; top: string; eff: string }[] = [];
    for (const f of DIFF_FIELDS) {
      const t = JSON.stringify(details.top_level[f] ?? null);
      const e = JSON.stringify(details.effective_1444[f] ?? null);
      if (t !== e) out.push({ field: f as string, top: t, eff: e });
    }
    // cores/claims list diffs.
    const listDiff = (f: "cores" | "claims") => {
      const t = JSON.stringify(details!.top_level[f]);
      const e = JSON.stringify(details!.effective_1444[f]);
      if (t !== e) out.push({ field: f, top: t, eff: e });
    };
    listDiff("cores");
    listDiff("claims");
    return out;
  });

  function snapshotSummary(s: Snapshot): string {
    const parts: string[] = [];
    if (s.owner) parts.push(`owner=${s.owner}`);
    if (s.controller && s.controller !== s.owner) parts.push(`ctrl=${s.controller}`);
    if (s.religion) parts.push(`rel=${s.religion}`);
    if (s.culture) parts.push(`cul=${s.culture}`);
    if (s.trade_goods) parts.push(`goods=${s.trade_goods}`);
    const dev = [s.base_tax, s.base_production, s.base_manpower]
      .map((v) => v ?? 0)
      .join("/");
    parts.push(`dev=${dev}`);
    if (s.cores.length) parts.push(`cores=[${s.cores.join(",")}]`);
    return parts.join("  ");
  }

  function geoRow(kn: KeyName | null): string {
    return kn ? `${kn.name} (${kn.key})` : "—";
  }
</script>

<div class="bench">
  <header class="bench-head">
    <h1>Timeline Bench</h1>
    <span class="note">
      Sprint 2.3 Timeline + 2.4 get_province_details — dev-only, not shipped UI.
    </span>
    <a class="back" href="/kit">← /kit</a>
  </header>

  <div class="cols">
    <!-- (a) Synthetic -->
    <section class="card">
      <h2>Synthetic timeline — exercises every intent</h2>
      <p class="hint">
        Hover a row for ✎/🗑. Edit key+value → <code>editEntry</code>; edit value only →
        <code>editValue</code>. "+ Add dated entry" → <code>addEntry</code> (auto-sorts by
        date). Two 1500.1.1 blocks show duplicate-date badges + occurrenceIndex.
      </p>
      <Timeline blocks={sampleBlocks} startDate="1444.11.11" onchange={onSampleChange}>
        {#snippet anchor()}
          <span class="anchor-note">Effective 1444 state would render here.</span>
        {/snippet}
      </Timeline>

      <h3>Intent log (newest first)</h3>
      <pre class="log">{intentLog.length ? intentLog.join("\n") : "— no intents yet —"}</pre>
    </section>

    <!-- (b) Live -->
    <section class="card">
      <h2>Live province lookup — get_province_details</h2>
      <div class="controls">
        <label>
          Province id
          <input type="number" min="1" bind:value={provId} />
        </label>
        <button onclick={loadProvince} disabled={loading}>
          {loading ? "Loading…" : "Load"}
        </button>
        <span class="quick">try 1 (Uppland), 151 (Constantinople)</span>
      </div>
      {#if liveStatus}<p class="status">{liveStatus}</p>{/if}

      {#if details}
        <div class="detail">
          <div class="detail-head">
            <strong>{details.localized_name}</strong>
            <span class="dim">#{details.id}</span>
            {#if !details.exists}<span class="badge warn">no file</span>{/if}
          </div>

          <h3>top_level vs effective_1444</h3>
          <p class="mono">top: {snapshotSummary(details.top_level)}</p>
          <p class="mono">eff: {snapshotSummary(details.effective_1444)}</p>
          {#if effectiveDiff.length}
            <table class="diff">
              <thead><tr><th>field</th><th>top_level</th><th>effective_1444</th></tr></thead>
              <tbody>
                {#each effectiveDiff as d}
                  <tr><td>{d.field}</td><td>{d.top}</td><td class="changed">{d.eff}</td></tr>
                {/each}
              </tbody>
            </table>
          {:else}
            <p class="hint">No pre-start dated blocks changed the 1444 state (top = effective).</p>
          {/if}

          <h3>Geography</h3>
          <dl class="geo">
            <dt>Area</dt><dd>{geoRow(details.geography.area)}</dd>
            <dt>Region</dt><dd>{geoRow(details.geography.region)}</dd>
            <dt>Superregion</dt><dd>{geoRow(details.geography.superregion)}</dd>
            <dt>Trade node</dt><dd>{geoRow(details.geography.trade_node)}</dd>
            <dt>Continent</dt><dd>{geoRow(details.geography.continent)}</dd>
            <dt>Climate</dt><dd>{geoRow(details.geography.climate)}</dd>
            <dt>Winter</dt><dd>{geoRow(details.geography.winter)}</dd>
            <dt>Monsoon</dt><dd>{geoRow(details.geography.monsoon)}</dd>
            <dt>Terrain override</dt><dd>{geoRow(details.geography.terrain_override)}</dd>
            <dt>Flags</dt>
            <dd>
              {details.geography.water ? "water " : ""}
              {details.geography.impassable ? "impassable" : ""}
              {!details.geography.water && !details.geography.impassable ? "land" : ""}
            </dd>
          </dl>

          <h3>Buildings / discovered_by</h3>
          <p class="mono">buildings: {details.top_level.buildings.join(", ") || "—"}</p>
          <p class="mono">discovered_by: {details.top_level.discovered_by.join(", ") || "—"}</p>

          <h3>raw_remainder (preserve-unknown, read-only)</h3>
          {#if details.raw_remainder.length}
            <pre class="log">{details.raw_remainder
                .map((r) => `${r.key}${r.is_block ? " = " : " = "}${r.value}`)
                .join("\n")}</pre>
          {:else}
            <p class="hint">— none —</p>
          {/if}

          <h3>Dated timeline (real data)</h3>
          <Timeline blocks={liveTimelineBlocks} startDate="1444.11.11" onchange={onLiveChange}>
            {#snippet anchor()}
              <span class="anchor-note mono">{snapshotSummary(details!.effective_1444)}</span>
            {/snippet}
          </Timeline>

          <h3>Live intent log</h3>
          <pre class="log">{liveLog.length ? liveLog.join("\n") : "— no intents yet —"}</pre>
        </div>
      {/if}
    </section>
  </div>
</div>

<style>
  .bench {
    min-height: 100vh;
    background: var(--bg-0);
    color: var(--text-1);
    font-family: Inter, system-ui, sans-serif;
    padding: 1rem 1rem 4rem;
  }

  .bench-head {
    display: flex;
    align-items: baseline;
    gap: 1rem;
    margin-bottom: 1rem;
  }

  h1 {
    margin: 0;
    font-size: 1.3rem;
  }

  .note {
    color: var(--text-2);
    font-size: 0.85rem;
  }

  .back {
    margin-left: auto;
    color: var(--accent-text);
    font-size: 0.85rem;
  }

  .cols {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
    gap: 0.75rem;
    align-items: start;
  }

  @media (max-width: 60rem) {
    .cols {
      grid-template-columns: 1fr;
    }
  }

  .card {
    background: var(--bg-2);
    border: 1px solid var(--border);
    padding: 0.75rem;
  }

  h2 {
    margin: 0 0 0.6rem;
    font-size: 0.85rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-2);
  }

  h3 {
    margin: 0.9rem 0 0.35rem;
    font-size: 0.75rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-3);
  }

  .hint,
  .status,
  .quick {
    font-size: 0.78rem;
    color: var(--text-2);
  }

  .status {
    margin: 0.4rem 0;
  }

  code {
    color: var(--text-1);
    background: var(--bg-1);
    padding: 0 0.25rem;
  }

  .log {
    margin: 0.2rem 0 0;
    padding: 0.4rem 0.5rem;
    background: var(--bg-1);
    border: 1px solid var(--border);
    font-size: 0.74rem;
    color: var(--text-2);
    white-space: pre-wrap;
    word-break: break-word;
    max-height: 16rem;
    overflow: auto;
  }

  .controls {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    flex-wrap: wrap;
  }

  .controls label {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    font-size: 0.8rem;
    color: var(--text-2);
  }

  .controls input {
    width: 6rem;
    background: var(--bg-1);
    border: 1px solid var(--border);
    color: var(--text-1);
    font-family: inherit;
    padding: 0.2rem 0.35rem;
  }

  .controls button {
    border: 1px solid var(--border);
    background: var(--bg-3);
    color: var(--text-1);
    font-family: inherit;
    font-size: 0.82rem;
    padding: 0.25rem 0.7rem;
    cursor: pointer;
  }

  .controls button:hover {
    background: var(--accent);
    color: var(--text-inverse);
  }

  .detail-head {
    display: flex;
    align-items: baseline;
    gap: 0.5rem;
    margin-top: 0.5rem;
  }

  .dim {
    color: var(--text-2);
    font-size: 0.8rem;
  }

  .mono {
    margin: 0.2rem 0;
    font-family: ui-monospace, "Cascadia Code", monospace;
    font-size: 0.75rem;
    color: var(--text-2);
  }

  .anchor-note {
    font-size: 0.75rem;
    color: var(--text-2);
  }

  .diff {
    border-collapse: collapse;
    font-size: 0.75rem;
    width: 100%;
  }

  .diff th,
  .diff td {
    border: 1px solid var(--border);
    padding: 0.15rem 0.4rem;
    text-align: left;
  }

  .diff th {
    background: var(--bg-3);
    color: var(--text-2);
    font-weight: 600;
  }

  .diff .changed {
    color: var(--ok);
  }

  .geo {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 0.1rem 0.6rem;
    margin: 0;
    font-size: 0.78rem;
  }

  .geo dt {
    color: var(--text-2);
  }

  .geo dd {
    margin: 0;
    color: var(--text-1);
  }

  .badge {
    font-size: 0.66rem;
    padding: 0.1rem 0.35rem;
    color: var(--text-inverse);
  }

  .badge.warn {
    background: var(--warn);
  }
</style>
