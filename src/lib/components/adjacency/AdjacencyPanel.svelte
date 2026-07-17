<!--
  AdjacencyPanel — Sprint 25 thin editor for the selected strait/adjacency.

  Reads the effective (base + pending) row passed by MapView and emits a whole
  updated row on any field change; MapView commits it as one line-surgical
  `csvRewrite` composite (coalesced per field so typing is a single undo unit).
  Endpoints and the `through` province are re-pickable by clicking the map (armed
  tools owned by MapView). Delete asks for confirmation.
-->
<script lang="ts">
  import { SidePanel } from "$lib/components/ui";
  import { ADJ_TYPES, type AdjRow, type AdjRowInput } from "$lib/adjnet";

  interface AdjIssue {
    severity: string;
    message: string;
  }

  let {
    row,
    waterIds,
    issues = [],
    armed,
    onchange,
    onpickendpoint,
    onpickthrough,
    ondelete,
    onclose,
  }: {
    row: AdjRowInput;
    waterIds: Set<number>;
    issues?: AdjIssue[];
    armed: string | null;
    onchange: (row: AdjRow, coalesceKey: string) => void;
    onpickendpoint: (which: "from" | "to") => void;
    onpickthrough: () => void;
    ondelete: () => void;
    onclose: () => void;
  } = $props();

  let confirming = $state(false);
  $effect(() => {
    // Reset the delete confirm when the selection changes.
    void row.from;
    void row.to;
    confirming = false;
  });

  function bare(r: AdjRowInput): AdjRow {
    const { origin: _origin, ...rest } = r;
    return rest;
  }

  function set<K extends keyof AdjRow>(key: K, value: AdjRow[K], coalesce: string) {
    onchange({ ...bare(row), [key]: value }, coalesce);
  }

  function setNum<K extends keyof AdjRow>(key: K, raw: string, coalesce: string) {
    const n = raw.trim() === "" || raw.trim() === "-" ? -1 : parseInt(raw, 10);
    set(key, (Number.isFinite(n) ? n : -1) as AdjRow[K], coalesce);
  }

  const throughIsWater = $derived(row.through >= 0 && waterIds.has(row.through));
  const rowLabel = $derived(`${row.from} ↔ ${row.to}`);
</script>

<SidePanel title="Adjacency" {onclose}>
  <div class="adj-body">
    <div class="hd">
      <span class="pair">{rowLabel}</span>
      <span class="type-badge type-{row.kind}">{row.kind}</span>
    </div>

    {#if issues.length > 0}
      <ul class="issues">
        {#each issues as iss}
          <li class:err={iss.severity === "error"}>{iss.message}</li>
        {/each}
      </ul>
    {/if}

    <section>
      <h4>Endpoints</h4>
      <div class="field">
        <label for="adj-from">From</label>
        <input
          id="adj-from"
          type="number"
          value={row.from}
          onchange={(e) => setNum("from", e.currentTarget.value, "adj-from")}
        />
        <button
          class="pick"
          class:active={armed === "adj_pick_from"}
          onclick={() => onpickendpoint("from")}>Pick</button
        >
      </div>
      <div class="field">
        <label for="adj-to">To</label>
        <input
          id="adj-to"
          type="number"
          value={row.to}
          onchange={(e) => setNum("to", e.currentTarget.value, "adj-to")}
        />
        <button
          class="pick"
          class:active={armed === "adj_pick_to"}
          onclick={() => onpickendpoint("to")}>Pick</button
        >
      </div>
    </section>

    <section>
      <h4>Type</h4>
      <select value={row.kind} onchange={(e) => set("kind", e.currentTarget.value, "adj-kind")}>
        {#each ADJ_TYPES as t}
          <option value={t}>{t}</option>
        {/each}
      </select>
    </section>

    <section>
      <h4>Through <span class="hint">(water tile a fleet can block)</span></h4>
      <div class="field">
        <input
          id="adj-through"
          type="number"
          value={row.through}
          class:bad={row.kind === "sea" && !throughIsWater}
          onchange={(e) => setNum("through", e.currentTarget.value, "adj-through")}
        />
        <button
          class="pick"
          class:active={armed === "adj_pick_through"}
          onclick={onpickthrough}>Pick</button
        >
      </div>
      {#if row.kind === "sea" && !throughIsWater}
        <p class="warn">Through province is not water.</p>
      {/if}
    </section>

    <section>
      <h4>Pixel overrides <span class="hint">(-1 = auto)</span></h4>
      <div class="coords">
        <label>start x<input type="number" value={row.startX} onchange={(e) => setNum("startX", e.currentTarget.value, "adj-sx")} /></label>
        <label>start y<input type="number" value={row.startY} onchange={(e) => setNum("startY", e.currentTarget.value, "adj-sy")} /></label>
        <label>stop x<input type="number" value={row.stopX} onchange={(e) => setNum("stopX", e.currentTarget.value, "adj-tx")} /></label>
        <label>stop y<input type="number" value={row.stopY} onchange={(e) => setNum("stopY", e.currentTarget.value, "adj-ty")} /></label>
      </div>
    </section>

    <section>
      <h4>Comment</h4>
      <input
        type="text"
        value={row.comment}
        placeholder="e.g. Majorca-Minorca"
        onchange={(e) => set("comment", e.currentTarget.value, "adj-comment")}
      />
    </section>

    <section class="danger">
      {#if confirming}
        <span class="confirm-q">Delete this adjacency?</span>
        <button class="del" onclick={() => { confirming = false; ondelete(); }}>Delete</button>
        <button onclick={() => (confirming = false)}>Cancel</button>
      {:else}
        <button class="del" onclick={() => (confirming = true)}>Delete adjacency</button>
      {/if}
    </section>
  </div>
</SidePanel>

<style>
  .adj-body {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
    padding: 0.5rem 0.65rem;
    color: #cfd4db;
    font-size: 0.82rem;
  }
  .hd {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .pair {
    font-weight: 700;
    font-size: 0.95rem;
  }
  .type-badge {
    text-transform: uppercase;
    font-size: 0.65rem;
    letter-spacing: 0.05em;
    padding: 0.1rem 0.4rem;
    border: 1px solid #2b323d;
    background: #3f4855;
  }
  .type-sea { color: #4aa3df; }
  .type-canal { color: #e08a3c; }
  .type-land { color: #c2cf55; }
  .type-lake { color: #9b7be0; }
  h4 {
    margin: 0 0 0.3rem;
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: #97a0ad;
  }
  .hint {
    text-transform: none;
    letter-spacing: 0;
    color: #6f7a88;
    font-size: 0.66rem;
  }
  .field {
    display: flex;
    align-items: center;
    gap: 0.4rem;
  }
  .field label {
    width: 2.6rem;
    color: #97a0ad;
  }
  input,
  select {
    background: #232a33;
    border: 1px solid #2b323d;
    color: #e6e9ee;
    padding: 0.2rem 0.35rem;
    font: inherit;
    flex: 1;
    min-width: 0;
  }
  input.bad,
  input.bad:focus {
    border-color: #c0522f;
  }
  .coords {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.35rem;
  }
  .coords label {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
    color: #97a0ad;
    font-size: 0.66rem;
  }
  button {
    background: #3f4855;
    border: 1px solid #2b323d;
    color: #cfd4db;
    padding: 0.2rem 0.5rem;
    font: inherit;
    cursor: pointer;
  }
  button:hover {
    background: #4a6da7;
    color: #fff;
  }
  .pick.active {
    background: #4a6da7;
    color: #fff;
  }
  .issues {
    list-style: none;
    margin: 0;
    padding: 0.3rem 0.4rem;
    background: #2a2118;
    border: 1px solid #4a3a1c;
    font-size: 0.72rem;
  }
  .issues li {
    color: #d8b878;
  }
  .issues li.err {
    color: #e08a72;
  }
  .warn {
    margin: 0.25rem 0 0;
    color: #e08a72;
    font-size: 0.72rem;
  }
  .danger {
    border-top: 1px solid #2b323d;
    padding-top: 0.6rem;
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }
  .del {
    color: #e08a72;
  }
  .del:hover {
    background: #8a3020;
    color: #fff;
  }
  .confirm-q {
    color: #d8b878;
    font-size: 0.75rem;
  }
</style>
