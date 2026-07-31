<!--
  TradeNodePanel — Sprint 8.2 node panel + 8.4 route editor + 8.5 delete flow.

  Reads the *effective* node (base + pending) passed by MapView, so every field
  reflects queued edits and undo/redo. All structural changes are pushed to the
  shared EditQueue with the tradenodes.rs recipes; MapView folds the queue back
  into the effective network (repainting the map + overlay). The color/location/
  toggle edits need no callback — the fold propagates them. Only selection and
  map-interactive tools (Set Location, Add Route, route handle dragging) are
  delegated to MapView via the action callbacks.
-->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { SidePanel, ColorPicker } from "$lib/components/ui";
  import FieldRow from "../country/FieldRow.svelte";
  import ValidationStrip, { type ValidationIssue, type JumpTarget } from "../ValidationStrip.svelte";
  import DeleteNodeConfirm from "./DeleteNodeConfirm.svelte";
  import type { EditQueue, TypedEdit } from "$lib/edits.svelte";
  import {
    controlToFileString,
    type TradeNetwork,
    type TradeNode,
    type RouteRef,
  } from "$lib/tradenet";

  let {
    installPath,
    modPath,
    queue,
    network,
    node,
    colorPresent,
    mapH,
    issues,
    selectedRoute,
    onclose,
    onselectnode,
    onselectroute,
    onsetlocation,
    onaddroute,
    ondeleted,
    onjump,
    onopenmechanics,
  }: {
    installPath: string;
    modPath: string | null;
    queue: EditQueue;
    network: TradeNetwork;
    node: TradeNode;
    /** True when the node has a `color` on disk (or in its create scaffold). */
    colorPresent: boolean;
    mapH: number;
    issues: ValidationIssue[];
    selectedRoute: RouteRef | null;
    onclose: () => void;
    onselectnode: (key: string) => void;
    onselectroute: (ref: RouteRef | null) => void;
    onsetlocation: () => void;
    onaddroute: () => void;
    ondeleted: () => void;
    onjump: (j: JumpTarget) => void;
    /** Open the Mechanics editor (Sprint 27 W4) at the trading_policies family
     *  — trading policies are a node-scoped mechanic, so the node panel links
     *  into their definition editor. */
    onopenmechanics?: (family: string, key?: string) => void;
  } = $props();

  const file = $derived(node.source_file || "common/tradenodes/00_tradenodes.txt");
  const key = $derived(node.key);

  // --- Name (loc override; loc key IS the node key) ---
  const pendingName = $derived(queue.pendingLocOverride(key));
  const titleName = $derived(pendingName ?? node.name ?? key);
  function commitName(v: string) {
    queue.push({
      label: `Rename ${key}`,
      edits: [{ kind: "locOverride", key, value: v }],
      coalesceKey: `tnname:${key}`,
    });
  }

  // --- Color ---
  const colorRGB = $derived({
    r: node.color?.[0] ?? 128,
    g: node.color?.[1] ?? 128,
    b: node.color?.[2] ?? 128,
  });
  const colorEdited = $derived(
    queue.findLast(
      (e) =>
        (e.kind === "setBlock" && e.file === file && e.path.length === 2 && e.path[0] === key && e.path[1] === "color") ||
        (e.kind === "insertStatement" && e.file === file && e.blockPath.length === 1 && e.blockPath[0] === key && /^color\s*=/.test(e.statement)),
    ) !== undefined,
  );
  function commitColor(c: { r: number; g: number; b: number }) {
    const edit: TypedEdit = colorPresent
      ? { kind: "setBlock", file, path: [key, "color"], value: `${c.r} ${c.g} ${c.b}` }
      : { kind: "insertStatement", file, blockPath: [key], statement: `color={ ${c.r} ${c.g} ${c.b} }` };
    queue.push({ label: `Set color of ${key}`, edits: [edit], coalesceKey: `tncolor:${key}` });
  }
  function css(c: [number, number, number] | null): string {
    return c ? `rgb(${c[0]}, ${c[1]}, ${c[2]})` : "transparent";
  }

  // --- inland / end toggles ---
  function toggleFlag(flag: "inland" | "end", on: boolean) {
    queue.push({
      label: `${on ? "Set" : "Clear"} ${flag} on ${key}`,
      edits: [
        on
          ? { kind: "insertStatement", file, blockPath: [key], statement: `${flag}=yes` }
          : { kind: "removeStatement", file, blockPath: [key], key: flag },
      ],
    });
  }

  // --- location ---
  const locEdited = $derived(queue.pendingScalar(file, [key, "location"]) !== undefined);

  // --- outgoing route ops ---
  function editRoute(index: number) {
    onselectroute({ from: key, index });
  }
  function deleteRoute(index: number, target: string) {
    if (!confirm(`Delete the route ${key} → ${target}?`)) return;
    queue.push({
      label: `Delete route ${key} → ${target}`,
      edits: [{ kind: "removeStatement", file, blockPath: [key], key: `outgoing#${index}` }],
    });
    if (selectedRoute && selectedRoute.index === index) onselectroute(null);
  }

  // --- route editor (selected route belongs to this node) ---
  const editing = $derived(
    selectedRoute && selectedRoute.from === key ? node.outgoing[selectedRoute.index] ?? null : null,
  );
  let pathText = $state("");
  // Reseed the manual path field whenever the edited route changes.
  $effect(() => {
    pathText = editing ? editing.path.join(" ") : "";
  });

  function commitControl(controlTopLeft: [number, number][], idx: number) {
    queue.push({
      label: `Reshape route ${key} → ${editing?.target ?? ""}`,
      edits: [
        { kind: "setBlock", file, path: [key, `outgoing#${idx}`, "control"], value: controlToFileString(controlTopLeft, mapH) },
      ],
      coalesceKey: `tnctrl:${key}:${idx}`,
    });
  }
  function removeHandle(handleIndex: number) {
    if (!editing || editing.control.length <= 2) return;
    const next = editing.control.filter((_, i) => i !== handleIndex);
    commitControl(next, editing.index);
  }
  function commitPathValue(ids: number[]) {
    if (!editing) return;
    queue.push({
      label: `Set path of ${key} → ${editing.target}`,
      edits: [{ kind: "setBlock", file, path: [key, `outgoing#${editing.index}`, "path"], value: ids.join(" ") }],
      coalesceKey: `tnpath:${key}:${editing.index}`,
    });
  }
  function commitPathText() {
    const ids = pathText.split(/[\s,]+/).map((t) => parseInt(t, 10)).filter((n) => Number.isFinite(n));
    commitPathValue(ids);
  }
  async function rederivePath() {
    if (!editing) return;
    try {
      // Re-derive from the route's CURRENT (edited) control curve, not a
      // straight node-to-node line: send the live control points (file-space,
      // already reflecting any reshape folded into the effective network) and
      // let the backend sample the wrap-aware spline under them.
      const path = await invoke<number[]>("derive_route_path", {
        installPath,
        modPath,
        controlFile: editing.control_file,
        fromNode: key,
        toNode: editing.target,
      });
      commitPathValue(path);
      pathText = path.join(" ");
    } catch (e) {
      alert(`Re-derive failed: ${e}`);
    }
  }
  async function reverseRoute() {
    if (!editing) return;
    const target = editing.target;
    const targetNode = network.nodes.find((n) => n.key === target);
    if (!targetNode) {
      alert(`Target node ${target} not found.`);
      return;
    }
    const revPath = editing.path.slice().reverse();
    const revControlFile = editing.control_file.slice().reverse();
    try {
      const stmt = await invoke<string>("scaffold_trade_route", {
        target: key,
        path: revPath,
        control: revControlFile,
      });
      queue.push({
        label: `Reverse route ${key} → ${target}`,
        edits: [
          { kind: "removeStatement", file, blockPath: [key], key: `outgoing#${editing.index}` },
          {
            kind: "insertStatement",
            file: targetNode.source_file || "common/tradenodes/00_tradenodes.txt",
            blockPath: [target],
            statement: stmt,
          },
        ],
      });
      onselectroute(null);
    } catch (e) {
      alert(`Reverse failed: ${e}`);
    }
  }

  // --- delete node ---
  let deleteOpen = $state(false);

  function jumpTo(j: JumpTarget) {
    if (j.kind === "node") onselectnode(j.id);
    else onjump(j);
  }
</script>

<SidePanel title={titleName} {onclose}>
  {#snippet header()}
    <div class="head">
      <span class="swatch" style="background: {css(node.color)}"></span>
      <span class="key-chip">{key}</span>
      {#if node.end}<span class="badge end">end</span>{/if}
      {#if node.inland}<span class="badge inland">inland</span>{/if}
    </div>
  {/snippet}

  <div class="strip-wrap">
    <ValidationStrip {issues} onjump={jumpTo} title="Trade graph" />
  </div>

  <section>
    <h3>Identity</h3>
    <FieldRow label="Name" edited={pendingName !== undefined}>
      <input class="text" value={titleName} oninput={(e) => commitName((e.target as HTMLInputElement).value)} />
    </FieldRow>
    <FieldRow label="Key"><span class="mono">{key}</span></FieldRow>
    <FieldRow label="Color" edited={colorEdited}>
      <ColorPicker value={colorRGB} onchange={commitColor} />
      <span class="mono">rgb({colorRGB.r}, {colorRGB.g}, {colorRGB.b})</span>
    </FieldRow>
    <FieldRow label="Location" edited={locEdited}>
      <span class="mono">{node.location ?? "—"}</span>
      <button class="btn small" onclick={onsetlocation}>Set Location…</button>
    </FieldRow>
    <FieldRow label="Type">
      <label class="chk"><input type="checkbox" checked={node.inland} onchange={(e) => toggleFlag("inland", (e.target as HTMLInputElement).checked)} /> inland</label>
      <label class="chk"><input type="checkbox" checked={node.end} onchange={(e) => toggleFlag("end", (e.target as HTMLInputElement).checked)} /> end</label>
    </FieldRow>
    <FieldRow label="Members"><span>{node.members.length}</span></FieldRow>
    {#if onopenmechanics}
      <FieldRow label="Trading policies">
        <button class="link" title="Edit trading-policy definitions" onclick={() => onopenmechanics?.("trading_policies")}>policies…</button>
      </FieldRow>
    {/if}
  </section>

  <section>
    <div class="sec-head">
      <h3>Outgoing routes ({node.outgoing.length})</h3>
      <button class="btn small" onclick={onaddroute}>+ Add route…</button>
    </div>
    {#if node.outgoing.length === 0}
      <p class="dim small">{node.end ? "End node — no outgoing routes (expected)." : "No outgoing routes."}</p>
    {:else}
      <ul class="routes">
        {#each node.outgoing as o (o.index)}
          <li class="route" class:active={selectedRoute?.from === key && selectedRoute?.index === o.index}>
            <button class="link grow" onclick={() => onselectnode(o.target)} title="Jump to {o.target}">→ {o.target}</button>
            <button class="btn tiny" onclick={() => editRoute(o.index)}>Edit</button>
            <button class="btn tiny danger" onclick={() => deleteRoute(o.index, o.target)}>✕</button>
          </li>
        {/each}
      </ul>
    {/if}
  </section>

  {#if editing}
    <section class="editor">
      <h3>Route editor — {key} → {editing.target}</h3>
      <p class="dim small">Drag the square handles on the map. Click the curve to add one; press Delete to remove the last, or ✕ below.</p>
      <div class="handles">
        {#each editing.control as pt, i (i)}
          <span class="handle">
            #{i} <span class="mono">{Math.round(pt[0])},{Math.round(pt[1])}</span>
            <button class="btn tiny danger" disabled={editing.control.length <= 2} onclick={() => removeHandle(i)}>✕</button>
          </span>
        {/each}
      </div>
      <FieldRow label="Path (province ids)">
        <input class="text" bind:value={pathText} onchange={commitPathText} placeholder="e.g. 1273 1202" />
      </FieldRow>
      <div class="editor-actions">
        <button class="btn small" onclick={rederivePath}>Re-derive path</button>
        <button class="btn small" onclick={reverseRoute}>Reverse direction</button>
        <button class="btn small danger" onclick={() => deleteRoute(editing!.index, editing!.target)}>Delete route</button>
        <button class="btn small" onclick={() => onselectroute(null)}>Done</button>
      </div>
    </section>
  {/if}

  <section>
    <h3>Incoming routes ({node.incoming.length})</h3>
    {#if node.incoming.length === 0}
      <p class="dim small">No incoming routes.</p>
    {:else}
      <ul class="routes">
        {#each node.incoming as inc (inc.from + "#" + inc.outgoing_index)}
          <li class="route">
            <button class="link grow" onclick={() => onselectnode(inc.from)} title="Jump to {inc.from}">{inc.from} →</button>
            <span class="tag">read-only</span>
          </li>
        {/each}
      </ul>
    {/if}
  </section>

  {#if node.raw_extra.length > 0}
    <section>
      <h3>Advanced (read-only)</h3>
      <p class="dim small">Unmodeled keys — preserved untouched on save.</p>
      <ul class="raw">
        {#each node.raw_extra as r (r)}<li><span class="mono">{r}</span></li>{/each}
      </ul>
    </section>
  {/if}

  <section>
    <button class="btn danger wide" onclick={() => (deleteOpen = true)}>Delete node…</button>
  </section>
</SidePanel>

{#if deleteOpen}
  <DeleteNodeConfirm
    {node}
    {network}
    {queue}
    onclose={() => (deleteOpen = false)}
    ondeleted={() => {
      deleteOpen = false;
      ondeleted();
    }}
  />
{/if}

<style>
  .head {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }
  .swatch {
    width: 1rem;
    height: 1rem;
    display: inline-block;
    border: 1px solid var(--border);
  }
  .key-chip {
    font-size: 0.8rem;
    color: var(--text-2);
  }
  .badge {
    font-size: 0.68rem;
    padding: 0.05rem 0.35rem;
    border: 1px solid var(--border);
    color: var(--text-inverse);
  }
  .badge.end {
    background: var(--accent);
  }
  .badge.inland {
    background: var(--warn);
  }
  .strip-wrap {
    margin: -0.2rem 0 0.4rem;
  }
  section {
    padding: 0.4rem 0 0.6rem;
    border-bottom: 1px solid var(--bg-1);
  }
  section.editor {
    background: rgba(74, 109, 167, 0.08);
    border-left: 2px solid var(--accent);
    padding-left: 0.5rem;
  }
  h3 {
    margin: 0 0 0.4rem;
    font-size: 0.8rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-2);
  }
  .sec-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
  }
  .text {
    width: 100%;
    background: var(--bg-0);
    border: 1px solid var(--border-strong);
    color: var(--text-1);
    font-family: inherit;
    font-size: 0.85rem;
    padding: 0.2rem 0.4rem;
  }
  .mono {
    font-family: ui-monospace, monospace;
    color: var(--text-2);
    font-size: 0.82rem;
  }
  .chk {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    font-size: 0.82rem;
    color: var(--text-1);
    margin-right: 0.7rem;
  }
  .routes {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .route {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    padding: 0.12rem 0;
  }
  .route.active {
    background: rgba(74, 109, 167, 0.18);
  }
  .grow {
    flex: 1;
    text-align: left;
  }
  .handles {
    display: flex;
    flex-wrap: wrap;
    gap: 0.3rem;
    margin-bottom: 0.5rem;
  }
  .handle {
    display: inline-flex;
    align-items: center;
    gap: 0.25rem;
    font-size: 0.72rem;
    color: var(--text-1);
    background: var(--bg-2);
    border: 1px solid var(--border);
    padding: 0.1rem 0.3rem;
  }
  .editor-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.35rem;
    margin-top: 0.4rem;
  }
  .btn {
    border: 1px solid var(--border-strong);
    background: transparent;
    color: var(--text-1);
    font-family: inherit;
    font-size: 0.8rem;
    padding: 0.2rem 0.6rem;
    cursor: pointer;
  }
  .btn:hover {
    border-color: var(--text-2);
  }
  .btn.small {
    font-size: 0.76rem;
    padding: 0.15rem 0.45rem;
  }
  .btn.tiny {
    font-size: 0.72rem;
    padding: 0.05rem 0.35rem;
  }
  .btn.wide {
    width: 100%;
  }
  .btn.danger {
    color: var(--err);
    border-color: var(--danger-bg);
  }
  .btn.danger:hover {
    background: var(--danger-bg);
    border-color: var(--danger-bg);
    color: var(--text-inverse);
  }
  .btn:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .link {
    border: 1px solid var(--border-strong);
    background: var(--bg-2);
    color: var(--accent-text);
    font-family: inherit;
    font-size: 0.78rem;
    padding: 0.1rem 0.4rem;
    cursor: pointer;
  }
  .link:hover {
    border-color: var(--accent);
    color: var(--text-inverse);
  }
  .tag {
    font-size: 0.7rem;
    color: var(--text-2);
  }
  .raw {
    list-style: none;
    margin: 0;
    padding: 0;
    font-size: 0.8rem;
    color: var(--text-1);
  }
  .dim {
    color: var(--text-2);
  }
  .small {
    font-size: 0.76rem;
  }
</style>
