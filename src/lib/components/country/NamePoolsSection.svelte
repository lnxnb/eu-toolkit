<!--
  NamePoolsSection — bulk name-pool editing (Sprint 1.2). Five textareas
  (monarch_names as `"Name #N" = weight` lines, plus leader/ship/army/fleet names
  one token per line). Each commit diffs the edited text against the on-disk pool
  and queues the minimal insert/remove statements into the common/countries file,
  coalesced per pool so repeated edits stay one undo unit. Raw tokens keep their
  quotes so byte-surgical add/remove matches the file exactly; monarch weights may
  be negative (female names).
-->
<script lang="ts">
  import type { EditQueue, TypedEdit } from "$lib/edits.svelte";
  import type { CountryDetails } from "./types";

  let {
    tag,
    queue,
    details,
  }: {
    tag: string;
    queue: EditQueue;
    details: CountryDetails;
  } = $props();

  const file = $derived(details.country_file ?? `common/countries/${details.name}.txt`);
  const pools = $derived(details.name_pools);

  // Base text per pool (what's on disk), for the diff + the initial textarea value.
  const monarchBase = $derived(pools.monarch_names.map((m) => `${m.name} = ${m.weight}`));
  const leaderBase = $derived(pools.leader_names);
  const shipBase = $derived(pools.ship_names);
  const armyBase = $derived(pools.army_names);
  const fleetBase = $derived(pools.fleet_names);

  // Local editable text (initialized from base; resets when the country changes).
  let monarchText = $state("");
  let leaderText = $state("");
  let shipText = $state("");
  let armyText = $state("");
  let fleetText = $state("");
  $effect(() => {
    // Re-init whenever the loaded pools change (country switch / reload).
    monarchText = monarchBase.join("\n");
    leaderText = leaderBase.join("\n");
    shipText = shipBase.join("\n");
    armyText = armyBase.join("\n");
    fleetText = fleetBase.join("\n");
  });

  function lines(text: string): string[] {
    return text.split("\n").map((l) => l.trim()).filter((l) => l.length > 0);
  }
  function multisetDiff(base: string[], desired: string[]): { added: string[]; removed: string[] } {
    const b = [...base];
    const added: string[] = [];
    for (const d of desired) {
      const i = b.indexOf(d);
      if (i >= 0) b.splice(i, 1);
      else added.push(d);
    }
    return { added, removed: b };
  }
  function quoteIfNeeded(tok: string): string {
    return /\s/.test(tok) && !(tok.startsWith('"') && tok.endsWith('"')) ? `"${tok}"` : tok;
  }

  // --- Bare-token pools (leader/ship/army/fleet) ---
  function commitBare(pool: string, base: string[], text: string) {
    const desired = lines(text);
    const { added, removed } = multisetDiff(base, desired);
    if (!added.length && !removed.length) return;
    const edits: TypedEdit[] = [
      ...removed.map((id): TypedEdit => ({ kind: "removeId", file, listPath: [pool], id })),
      ...added.map((id): TypedEdit => ({ kind: "addId", file, listPath: [pool], id: quoteIfNeeded(id) })),
    ];
    queue.push({ label: `Edit ${pool} of ${tag}`, edits, coalesceKey: `namepool:${tag}:${pool}` });
  }

  // --- monarch_names (`Name = weight`) ---
  function parseMonarch(line: string): { name: string; weight: string } | null {
    const eq = line.lastIndexOf("=");
    if (eq < 0) return null;
    const name = line.slice(0, eq).trim();
    const weight = line.slice(eq + 1).trim();
    return name ? { name, weight } : null;
  }
  function commitMonarch() {
    const desired = lines(monarchText);
    const { added, removed } = multisetDiff(monarchBase, desired);
    if (!added.length && !removed.length) return;
    const edits: TypedEdit[] = [];
    for (const line of removed) {
      const p = parseMonarch(line);
      if (p) edits.push({ kind: "removeStatement", file, blockPath: ["monarch_names"], key: p.name, value: null });
    }
    for (const line of added) {
      const p = parseMonarch(line);
      if (p) edits.push({ kind: "insertStatement", file, blockPath: ["monarch_names"], statement: `${p.name} = ${p.weight}` });
    }
    if (edits.length) queue.push({ label: `Edit monarch names of ${tag}`, edits, coalesceKey: `namepool:${tag}:monarch_names` });
  }
</script>

<section>
  <h3>Name Pools</h3>

  <div class="pool">
    <div class="lbl">Monarch names <span class="count">({lines(monarchText).length})</span></div>
    <p class="hint">One per line: <code>"Name #0" = weight</code> (negative weight = female).</p>
    <textarea class="area" bind:value={monarchText} onchange={commitMonarch} spellcheck="false"></textarea>
  </div>

  <div class="pool">
    <div class="lbl">Leader names <span class="count">({lines(leaderText).length})</span></div>
    <textarea class="area" bind:value={leaderText} onchange={() => commitBare("leader_names", leaderBase, leaderText)} spellcheck="false"></textarea>
  </div>

  <div class="pool">
    <div class="lbl">Ship names <span class="count">({lines(shipText).length})</span></div>
    <textarea class="area" bind:value={shipText} onchange={() => commitBare("ship_names", shipBase, shipText)} spellcheck="false"></textarea>
  </div>

  <div class="pool">
    <div class="lbl">Army names <span class="count">({lines(armyText).length})</span></div>
    <textarea class="area" bind:value={armyText} onchange={() => commitBare("army_names", armyBase, armyText)} spellcheck="false"></textarea>
  </div>

  <div class="pool">
    <div class="lbl">Fleet names <span class="count">({lines(fleetText).length})</span></div>
    <textarea class="area" bind:value={fleetText} onchange={() => commitBare("fleet_names", fleetBase, fleetText)} spellcheck="false"></textarea>
  </div>
</section>

<style>
  section {
    margin-bottom: 1rem;
  }

  h3 {
    margin: 0 0 0.5rem;
    font-size: 0.8rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-2);
  }

  .pool {
    margin-bottom: 0.6rem;
  }

  .lbl {
    font-size: 0.72rem;
    text-transform: uppercase;
    letter-spacing: 0.03em;
    color: var(--text-2);
    margin-bottom: 0.15rem;
  }

  .count {
    color: var(--text-3);
  }

  .hint {
    margin: 0 0 0.2rem;
    font-size: 0.7rem;
    color: var(--text-3);
  }

  .hint code {
    background: var(--bg-1);
    padding: 0 0.2rem;
  }

  .area {
    width: 100%;
    box-sizing: border-box;
    min-height: 4rem;
    resize: vertical;
    background: var(--bg-1);
    border: 1px solid var(--border);
    color: var(--text-1);
    font-family: ui-monospace, monospace;
    font-size: 0.75rem;
    padding: 0.3rem 0.4rem;
    outline: none;
  }
</style>
