<!--
  MissionBoard — the graphical, editable mission tree for a LIST of series
  (Sprint 17 rework). One component, two callers:
    • By country — every series the country receives, composed into the in-game
      combined view (all five slot columns; same-slot series stacked with a
      divider + clickable header).
    • All series — a list of one series (the browser still opens a single tree).

  Layout is delegated to the pure, unit-tested `missionLayout.ts`:
    • `slot` (1–5) is a SERIES-level column; same-slot series stack vertically.
    • `position` is a GLOBAL row within a slot; absent ⇒ previous-row-in-slot + 1.
      Rows align across columns, exactly as the game draws the tree.
    • `required_missions` arrows resolve by bare key across ALL displayed series;
      a target in no displayed series renders as an "external: <key>" stub badge.

  Interactions (all reported up with the owning series index; the board owns no
  queue state): click a node → select · drag vertically → onmove · Link mode →
  onlink(dependent, prereq) [host rejects cycles across the combined graph] ·
  click an empty cell → onadd(series, row) · click a series header → onselectseries.
  Horizontal drag (changing a mission's slot) stays out of scope — slot is a
  series property, so a node's column is fixed by its series.
-->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import type { MissionSeries } from "./missionsTypes";
  import {
    composeBoard,
    boardWidth,
    boardHeight,
    nodeX,
    nodeY,
    nodeCX,
    NCOLS,
    ROW_H,
    PAD,
    NODE_W,
    NODE_H,
    HEADER_H,
    type PlacedNode,
  } from "./missionLayout";

  let {
    series,
    installPath,
    modPath = null,
    selectedKey = null,
    selectedSeriesIndex = null,
    onselect,
    onmove,
    onlink,
    onadd,
    onselectseries,
  }: {
    series: MissionSeries[];
    installPath: string;
    modPath?: string | null;
    selectedKey?: string | null;
    selectedSeriesIndex?: number | null;
    onselect: (seriesIndex: number, key: string) => void;
    onmove: (seriesIndex: number, key: string, position: number) => void;
    onlink: (dependentSeriesIndex: number, dependent: string, prereq: string) => void;
    onadd: (seriesIndex: number, position: number) => void;
    onselectseries: (seriesIndex: number) => void;
  } = $props();

  const layout = $derived(composeBoard(series));
  const boardW = boardWidth();
  const boardH = $derived(boardHeight(layout.maxRow));

  // External stubs grouped per node key (for the badge under a node).
  const externalsByNode = $derived.by(() => {
    const m = new Map<string, string[]>();
    for (const e of layout.externals) {
      const arr = m.get(e.nodeKey) ?? [];
      arr.push(e.missingKey);
      m.set(e.nodeKey, arr);
    }
    return m;
  });

  // --- Icon cache (bounded: a country's ~8 series' handful of icons) --------
  let iconUrls = $state<Record<string, string>>({});
  const iconLoading = new Set<string>();
  async function ensureIcon(name: string) {
    if (!name || iconUrls[name] || iconLoading.has(name)) return;
    iconLoading.add(name);
    try {
      const buf = await invoke<ArrayBuffer>("get_sprite", { installPath, modPath, name });
      const url = URL.createObjectURL(new Blob([buf], { type: "image/png" }));
      iconUrls = { ...iconUrls, [name]: url };
    } catch {
      /* leave blank on undecodable icons */
    } finally {
      iconLoading.delete(name);
    }
  }
  $effect(() => {
    for (const n of layout.nodes) if (n.mission.icon) void ensureIcon(n.mission.icon);
  });
  $effect(() => () => {
    for (const u of Object.values(iconUrls)) URL.revokeObjectURL(u);
  });

  // --- Link mode -----------------------------------------------------------
  let linkMode = $state(false);
  let linkFrom = $state<PlacedNode | null>(null);
  function nodeClick(n: PlacedNode) {
    if (dragMoved) return; // a drag, not a click
    if (linkMode) {
      if (!linkFrom) {
        linkFrom = n;
      } else if (linkFrom.key !== n.key) {
        // Second click = dependent; first = prerequisite (may be another series).
        onlink(n.seriesIndex, n.key, linkFrom.key);
        linkFrom = null;
      } else {
        linkFrom = null;
      }
      return;
    }
    onselect(n.seriesIndex, n.key);
  }
  function toggleLink() {
    linkMode = !linkMode;
    linkFrom = null;
  }

  // --- Vertical drag to move (rewrite position) ----------------------------
  let scroller: HTMLDivElement;
  let dragKey = $state<string | null>(null);
  let dragSeriesIndex = 0;
  let dragGhostY = $state(0);
  let dragMoved = false;
  let dragStartY = 0;

  function pointerDown(e: PointerEvent, n: PlacedNode) {
    if (linkMode || e.button !== 0) return;
    dragKey = n.key;
    dragSeriesIndex = n.seriesIndex;
    dragMoved = false;
    const rect = scroller.getBoundingClientRect();
    dragStartY = e.clientY;
    dragGhostY = e.clientY - rect.top + scroller.scrollTop - NODE_H / 2;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }
  function pointerMove(e: PointerEvent) {
    if (!dragKey) return;
    if (Math.abs(e.clientY - dragStartY) > 4) dragMoved = true;
    const rect = scroller.getBoundingClientRect();
    dragGhostY = e.clientY - rect.top + scroller.scrollTop - NODE_H / 2;
  }
  function pointerUp() {
    if (!dragKey) return;
    if (dragMoved) {
      const targetRow = Math.max(1, Math.round((dragGhostY - PAD) / ROW_H) + 1);
      onmove(dragSeriesIndex, dragKey, targetRow);
    }
    dragKey = null;
    // Let the click handler see dragMoved this tick, then reset next frame.
    requestAnimationFrame(() => (dragMoved = false));
  }

  // Bezier arrow path — vertical for same-column, gentle S for cross-column.
  function arrowPath(x1: number, y1: number, x2: number, y2: number): string {
    return `M ${x1} ${y1} C ${x1} ${y1 + 30}, ${x2} ${y2 - 30}, ${x2} ${y2}`;
  }
</script>

<div class="board-wrap">
  <div class="board-toolbar">
    <button class="tool" class:on={linkMode} onclick={toggleLink} title="Draw a requirement arrow: click the prerequisite, then the dependent">
      {linkMode ? "Linking… (click prerequisite, then dependent)" : "🔗 Link requirements"}
    </button>
    {#if linkMode && linkFrom}
      <span class="linkhint">prerequisite: <code>{linkFrom.key}</code> — now click the dependent, in any column (Esc to cancel)</span>
    {/if}
    <span class="spacer"></span>
    <span class="hint">Drag a node to change its row · click an empty cell to add a mission · click a series title to edit its settings</span>
  </div>

  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="scroller"
    bind:this={scroller}
    role="presentation"
    onpointermove={pointerMove}
    onpointerup={pointerUp}
    onpointerleave={pointerUp}
  >
    <div class="board" style:width="{boardW}px" style:height="{boardH}px">
      <!-- Slot column guides -->
      {#each Array(NCOLS) as _, c (c)}
        <div class="slotcol" style:left="{nodeX(c)}px" style:width="{NODE_W}px" style:height="{boardH - PAD}px">
          <span class="slotlabel">slot {c + 1}</span>
        </div>
      {/each}

      <!-- Requirement arrows (resolved across all displayed series) -->
      <svg class="arrows" width={boardW} height={boardH} aria-hidden="true">
        <defs>
          <marker id="mreqhead" markerWidth="8" markerHeight="8" refX="6" refY="4" orient="auto">
            <!-- Authored mission-tree connector artwork; these are diagram data colors. -->
            <path d="M0,0 L8,4 L0,8 Z" fill="#c8a24a" />
          </marker>
        </defs>
        {#each layout.arrows as a, i (i)}
          <path
            d={arrowPath(nodeCX(a.fromCol), nodeY(a.fromRow) + NODE_H, nodeCX(a.toCol), nodeY(a.toRow))}
            fill="none"
            stroke={a.cross ? "#7f9bc7" : "#c8a24a"}
            stroke-width="2"
            stroke-dasharray={a.cross ? "5 4" : undefined}
            marker-end="url(#mreqhead)"
          />
        {/each}
      </svg>

      <!-- Series headers + dividers -->
      {#each layout.sections as sec (sec.seriesKey + ":" + sec.seriesIndex)}
        {#if !sec.first}
          <div class="divider" style:left="{nodeX(sec.col)}px" style:top="{nodeY(sec.minRow) - HEADER_H - 6}px" style:width="{NODE_W}px"></div>
        {/if}
        <button
          class="series-header"
          class:sel={sec.seriesIndex === selectedSeriesIndex}
          class:approx={sec.approx}
          style:left="{nodeX(sec.col)}px"
          style:top="{nodeY(sec.minRow) - HEADER_H}px"
          style:width="{NODE_W}px"
          title="Edit series settings for {sec.seriesKey}"
          onclick={() => onselectseries(sec.seriesIndex)}
        >
          {#if sec.approx}<span class="approx-mark">APPROX</span>{/if}
          <span class="sh-key">{sec.seriesKey}</span>
        </button>
      {/each}

      <!-- Empty add-cells (attributed to their owning series) -->
      {#each layout.addCells as cell, i (cell.col + ":" + cell.row + ":" + i)}
        <button
          class="empty-cell"
          style:left="{nodeX(cell.col)}px"
          style:top="{nodeY(cell.row)}px"
          style:width="{NODE_W}px"
          style:height="{NODE_H}px"
          title="Add a mission here (row {cell.row}) in {cell.series.key}"
          onclick={() => onadd(cell.seriesIndex, cell.row)}
        >＋</button>
      {/each}

      <!-- Mission nodes -->
      {#each layout.nodes as n (n.seriesKey + ":" + n.key)}
        {@const ext = externalsByNode.get(n.key)}
        <div
          class="node"
          class:selected={n.key === selectedKey}
          class:linkfrom={linkFrom?.key === n.key}
          class:dragging={n.key === dragKey}
          style:left="{nodeX(n.col)}px"
          style:top={n.key === dragKey ? `${dragGhostY}px` : `${nodeY(n.row)}px`}
          style:width="{NODE_W}px"
          style:height="{NODE_H}px"
          role="button"
          tabindex="0"
          onpointerdown={(e) => pointerDown(e, n)}
          onclick={() => nodeClick(n)}
          onkeydown={(e) => (e.key === "Enter" || e.key === " ") && (e.preventDefault(), onselect(n.seriesIndex, n.key))}
        >
          <div class="node-icon">
            {#if n.mission.icon && iconUrls[n.mission.icon]}
              <img src={iconUrls[n.mission.icon]} alt={n.mission.icon} />
            {:else}
              <span class="icon-ph">◈</span>
            {/if}
          </div>
          <div class="node-body">
            <span class="node-title">{n.mission.title}</span>
            {#if n.mission.pendingBadge}<span class="mini-badge">unsaved</span>{/if}
            {#if ext}
              {#each ext as x (x)}
                <span class="ext-badge" title="Requires a mission not in any displayed series">external: {x}</span>
              {/each}
            {/if}
          </div>
        </div>
      {/each}
    </div>
  </div>
</div>

<style>
  .board-wrap {
    display: flex;
    flex-direction: column;
    min-height: 0;
    flex: 1;
  }

  .board-toolbar {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    flex: none;
    padding: 0.35rem 0.5rem;
    background: var(--bg-2);
    border-bottom: 1px solid var(--border);
  }

  .tool {
    border: 1px solid var(--border);
    background: var(--bg-3);
    color: var(--text-1);
    font-family: inherit;
    font-size: 0.78rem;
    padding: 0.22rem 0.6rem;
    cursor: pointer;
  }
  .tool:hover { background: var(--accent); color: var(--text-inverse); }
  .tool.on { background: var(--warn); color: var(--text-inverse); }

  .linkhint { font-size: 0.76rem; color: var(--warn); }
  .linkhint code { color: var(--ok); }
  .spacer { flex: 1; }
  .hint { font-size: 0.72rem; color: var(--text-3); }

  .scroller {
    flex: 1;
    min-height: 0;
    overflow: auto;
    background: var(--bg-0);
  }

  .board {
    position: relative;
  }

  .slotcol {
    position: absolute;
    top: 8px;
    border: 1px dashed var(--bg-2);
    background: rgba(255, 255, 255, 0.008);
    pointer-events: none;
  }
  .slotlabel {
    position: absolute;
    top: 2px;
    left: 4px;
    font-size: 0.64rem;
    color: var(--bg-3);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .arrows {
    position: absolute;
    top: 0;
    left: 0;
    pointer-events: none;
  }

  .divider {
    position: absolute;
    border-top: 2px dashed var(--bg-3);
    height: 0;
    pointer-events: none;
  }
  .series-header {
    position: absolute;
    height: 18px;
    display: flex;
    align-items: center;
    gap: 0.3rem;
    border: 1px solid var(--bg-3);
    background: var(--bg-1);
    color: var(--ok);
    font-family: inherit;
    font-size: 0.66rem;
    padding: 0 0.35rem;
    cursor: pointer;
    overflow: hidden;
    white-space: nowrap;
  }
  .series-header:hover { border-color: var(--accent); background: var(--bg-3); }
  .series-header.sel { border-color: var(--accent); background: var(--accent); color: var(--accent-text); }
  .series-header.approx { border-color: var(--warn); color: var(--warn); }
  .sh-key { overflow: hidden; text-overflow: ellipsis; }
  .approx-mark {
    font-size: 0.56rem;
    background: var(--warn);
    color: var(--text-inverse);
    padding: 0 0.2rem;
    letter-spacing: 0.04em;
    flex: none;
  }

  .empty-cell {
    position: absolute;
    border: 1px dashed var(--accent);
    background: rgba(74, 109, 167, 0.05);
    color: var(--accent);
    font-size: 1.4rem;
    cursor: pointer;
    opacity: 0.35;
  }
  .empty-cell:hover { opacity: 1; background: rgba(74, 109, 167, 0.18); }

  .node {
    position: absolute;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.2rem;
    border: 2px solid var(--accent);
    background: var(--bg-1);
    color: var(--text-1);
    padding: 0.3rem;
    cursor: grab;
    user-select: none;
    box-shadow: 0 2px 6px rgba(0, 0, 0, 0.4);
  }
  .node:hover { border-color: var(--accent); }
  .node.selected { border-color: var(--accent); background: var(--accent); }
  .node.linkfrom { border-color: var(--warn); }
  .node.dragging { cursor: grabbing; opacity: 0.85; z-index: 5; }

  .node-icon {
    width: 40px;
    height: 40px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--bg-0);
    overflow: hidden;
    flex: none;
  }
  .node-icon img { max-width: 100%; max-height: 100%; image-rendering: pixelated; }
  .icon-ph { color: var(--bg-3); font-size: 1.4rem; }

  .node-body {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.1rem;
    min-height: 0;
  }
  .node-title {
    font-size: 0.72rem;
    text-align: center;
    line-height: 1.05;
    overflow: hidden;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
  }
  .mini-badge {
    font-size: 0.6rem;
    text-transform: uppercase;
    background: var(--warn);
    color: var(--text-inverse);
    padding: 0 0.25rem;
  }
  .ext-badge {
    font-size: 0.58rem;
    background: var(--warn);
    color: var(--warn);
    border: 1px solid var(--warn);
    padding: 0 0.2rem;
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
