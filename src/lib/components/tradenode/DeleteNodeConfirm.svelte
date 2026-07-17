<!--
  DeleteNodeConfirm — Sprint 8.5 delete-node confirmation.

  Lists every route touching the node. Outgoing routes live inside the node block
  and vanish with it (informational). Incoming routes (from other nodes) would
  dangle, so each gets a per-route choice: retarget to another node, or delete.
  Confirm builds ONE composite: the node removal plus, for incoming routes, a
  `SetScalar name` (retarget, quoted) or `RemoveStatement outgoing#i` (delete).
  Deletes within a source node are ordered high-index-first so occurrence indices
  don't shift under earlier removals.
-->
<script lang="ts">
  import type { EditQueue, TypedEdit } from "$lib/edits.svelte";
  import type { TradeNetwork, TradeNode, Incoming } from "$lib/tradenet";

  let {
    node,
    network,
    queue,
    onclose,
    ondeleted,
  }: {
    node: TradeNode;
    network: TradeNetwork;
    queue: EditQueue;
    onclose: () => void;
    ondeleted: () => void;
  } = $props();

  // Per incoming route: "delete" or a target node key to retarget to.
  let choices = $state<Record<string, string>>({});
  const incoming = $derived(node.incoming);

  const nodeByKey = $derived(new Map(network.nodes.map((n) => [n.key, n])));
  function fileOf(key: string): string {
    return nodeByKey.get(key)?.source_file ?? "common/tradenodes/00_tradenodes.txt";
  }
  const routeKey = (i: Incoming) => `${i.from}#${i.outgoing_index}`;

  // Retarget candidates: any node except the one being deleted.
  const candidates = $derived(network.nodes.filter((n) => n.key !== node.key));

  function confirm() {
    const edits: TypedEdit[] = [];
    // Retargets first (they don't shift indices).
    for (const inc of incoming) {
      const c = choices[routeKey(inc)] ?? "delete";
      if (c !== "delete") {
        edits.push({
          kind: "setScalar",
          file: fileOf(inc.from),
          path: [inc.from, `outgoing#${inc.outgoing_index}`, "name"],
          value: c,
          quoted: true,
        });
      }
    }
    // Deletes: group by source node, remove high index first.
    const dels = incoming
      .filter((inc) => (choices[routeKey(inc)] ?? "delete") === "delete")
      .sort((a, b) => b.outgoing_index - a.outgoing_index);
    for (const inc of dels) {
      edits.push({
        kind: "removeStatement",
        file: fileOf(inc.from),
        blockPath: [inc.from],
        key: `outgoing#${inc.outgoing_index}`,
      });
    }
    // The node itself (top-level key removal); its outgoing routes go with it.
    edits.push({
      kind: "removeStatement",
      file: node.source_file || "common/tradenodes/00_tradenodes.txt",
      blockPath: [],
      key: node.key,
    });
    queue.push({ label: `Delete trade node ${node.name}`, edits });
    ondeleted();
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onclose()} />
<div class="scrim" role="presentation" onclick={onclose}>
  <div
    class="dialog"
    role="dialog"
    aria-modal="true"
    aria-label="Delete trade node"
    tabindex="-1"
    onclick={(e) => e.stopPropagation()}
    onkeydown={(e) => e.stopPropagation()}
  >
    <h2>Delete trade node “{node.name}”?</h2>
    <p class="dim">
      Its {node.members.length} member province{node.members.length === 1 ? "" : "s"} become
      node-less (legal). Choose what happens to each route.
    </p>

    <section>
      <h3>Outgoing ({node.outgoing.length})</h3>
      {#if node.outgoing.length === 0}
        <p class="dim small">None.</p>
      {:else}
        <ul class="routes">
          {#each node.outgoing as o (o.index)}
            <li><span class="mono">→ {o.target}</span> <span class="tag">removed with node</span></li>
          {/each}
        </ul>
      {/if}
    </section>

    <section>
      <h3>Incoming ({incoming.length})</h3>
      {#if incoming.length === 0}
        <p class="dim small">None.</p>
      {:else}
        <ul class="routes">
          {#each incoming as inc (routeKey(inc))}
            <li class="inc">
              <span class="mono">{inc.from} →</span>
              <select bind:value={choices[routeKey(inc)]}>
                <option value="delete">Delete route</option>
                {#each candidates as c (c.key)}
                  <option value={c.key}>Retarget → {c.name}</option>
                {/each}
              </select>
            </li>
          {/each}
        </ul>
      {/if}
    </section>

    <div class="actions">
      <button class="btn" onclick={onclose}>Cancel</button>
      <button class="btn danger" onclick={confirm}>Delete node</button>
    </div>
  </div>
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 40;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(10, 12, 15, 0.55);
  }
  .dialog {
    width: 30rem;
    max-width: 92vw;
    max-height: 84vh;
    overflow-y: auto;
    background: #2b323d;
    border: 1px solid #1f242c;
    color: #cfd4db;
    padding: 1rem 1.1rem;
    box-shadow: 3px 4px 14px rgba(0, 0, 0, 0.45);
  }
  h2 {
    margin: 0 0 0.5rem;
    font-size: 1rem;
  }
  h3 {
    margin: 0 0 0.3rem;
    font-size: 0.75rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: #9ca3af;
  }
  section {
    padding: 0.5rem 0;
    border-top: 1px solid #232a33;
  }
  .routes {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .routes li {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.15rem 0;
    font-size: 0.82rem;
  }
  .inc select {
    background: #14181d;
    border: 1px solid #4b5563;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.8rem;
    padding: 0.15rem 0.3rem;
  }
  .mono {
    font-family: ui-monospace, monospace;
    color: #cfd4db;
  }
  .tag {
    font-size: 0.72rem;
    color: #9ca3af;
  }
  .dim {
    color: #9ca3af;
  }
  .small {
    font-size: 0.78rem;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
    margin-top: 0.8rem;
  }
  .btn {
    border: 1px solid #4b5563;
    background: transparent;
    color: #cfd4db;
    font-family: inherit;
    font-size: 0.85rem;
    padding: 0.3rem 0.8rem;
    cursor: pointer;
  }
  .btn:hover {
    border-color: #9ca3af;
  }
  .btn.danger {
    background: #7a2820;
    border-color: #7a2820;
    color: #fff;
  }
  .btn.danger:hover {
    background: #9a3226;
    border-color: #9a3226;
  }
</style>
