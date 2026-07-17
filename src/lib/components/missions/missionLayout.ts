// Pure layout-composition core for the combined per-country mission board
// (Sprint 17 rework). No Svelte, no IO — every rule here is unit-tested in
// `missionLayout.test.ts`.
//
// ## How the game composes a country's mission tree (verified against vanilla)
//   * `slot` (1–5) is a SERIES-level property = the tab COLUMN. A country can
//     receive several series in the SAME slot (e.g. Aachen's westfalian_group_1/2/3
//     are all `slot = 1`). Those series STACK VERTICALLY, in received order.
//   * `position` is a GLOBAL ROW coordinate within a slot column — NOT a per-series
//     index. Vanilla authors same-slot series with disjoint, sequential position
//     ranges: westfalian_group_1 fills rows 1–4 (its missions omit `position`, so
//     they fall on running rows), group_2's `wes_hre_peace` has `position = 5`, and
//     group_3's `wes_rheinland_start` has `position = 6` with the rest continuing
//     7, 8. So a positionless mission takes `previous-row-in-this-slot + 1`, which
//     makes stacked series flow one after another automatically while explicit
//     positions still pin a mission to its row (and align it across columns —
//     row 3 in slot 1 draws at the same Y as row 3 in slot 2, exactly as in game).
//   * `required_missions` links resolve by BARE KEY across ALL displayed series, so
//     an arrow can span columns (30 cross-file links exist in vanilla; e.g. the
//     Imperial Austrian tree requires missions from the base Austrian tree). A
//     reference to a mission not present in any displayed series renders as an
//     "external: <key>" stub rather than a broken arrow (4 such refs exist in
//     vanilla itself).
//
// The same routine drives the single-series board (All-series tab) — a list of one.

import type { MissionSeries, MissionEntry } from "./missionsTypes";

// Board geometry (shared with MissionBoard.svelte).
export const NCOLS = 5;
export const COL_W = 196;
export const ROW_H = 120;
export const PAD = 26;
export const NODE_W = 156;
export const NODE_H = 92;
/** Height reserved above a series' first node for its clickable header. */
export const HEADER_H = 22;

export const nodeX = (col: number): number => PAD + col * COL_W;
export const nodeY = (row: number): number => PAD + (row - 1) * ROW_H;
export const nodeCX = (col: number): number => nodeX(col) + NODE_W / 2;
export const boardWidth = (): number => NCOLS * COL_W + PAD;
export const boardHeight = (maxRow: number): number => (maxRow + 1) * ROW_H + PAD;

/** A single mission placed at a concrete (column, row) on the combined board. */
export interface PlacedNode {
  key: string;
  seriesKey: string;
  file: string;
  /** Index into the input `series[]` — the identity used for editing + dedupe. */
  seriesIndex: number;
  col: number; // 0-based column (slot - 1)
  row: number; // 1-based row
  mission: MissionEntry;
  series: MissionSeries;
}

/** One series' band inside its slot column (drives the header + divider). */
export interface SeriesSection {
  seriesIndex: number;
  seriesKey: string;
  series: MissionSeries;
  slot: number; // 1..5
  col: number; // 0..4
  minRow: number;
  maxRow: number;
  /** True for a series shown only because the user expanded the approximate set. */
  approx: boolean;
  /** True for the first (top) series in its slot — no divider drawn above it. */
  first: boolean;
}

/** An empty, clickable cell that scaffolds a new mission into a specific series. */
export interface AddCell {
  seriesIndex: number;
  series: MissionSeries;
  col: number;
  row: number;
}

/** A resolved requirement arrow (prerequisite → dependent), possibly cross-column. */
export interface BoardArrow {
  fromKey: string;
  toKey: string;
  fromCol: number;
  fromRow: number;
  toCol: number;
  toRow: number;
  /** True when the arrow spans two different slot columns. */
  cross: boolean;
}

/** A `required_missions` reference whose target is in no displayed series. */
export interface ExternalRef {
  seriesIndex: number;
  nodeKey: string;
  missingKey: string;
}

export interface BoardLayout {
  nodes: PlacedNode[];
  sections: SeriesSection[];
  addCells: AddCell[];
  arrows: BoardArrow[];
  externals: ExternalRef[];
  maxRow: number;
  /** First placement per bare mission key (arrow-resolution + link targeting). */
  posByKey: Map<string, PlacedNode>;
}

/** Clamps a series `slot` to the 1–5 column range (absent ⇒ slot 1). */
export function clampSlot(slot: number | null | undefined): number {
  if (slot == null || !Number.isFinite(slot)) return 1;
  return Math.min(5, Math.max(1, Math.round(slot)));
}

/**
 * Composes a list of mission series into a single laid-out board. Series are
 * grouped by slot, stacked in the given order within each slot, and each mission
 * gets a global row (explicit `position`, else previous-row-in-slot + 1, with
 * residual collisions bumped down). Requirement arrows resolve by bare key across
 * every series; unresolved targets become external stubs.
 */
export function composeBoard(series: MissionSeries[]): BoardLayout {
  // Group series indices by slot, preserving input (received) order.
  const bySlot = new Map<number, number[]>();
  series.forEach((s, i) => {
    const slot = clampSlot(s.slot);
    const arr = bySlot.get(slot) ?? [];
    arr.push(i);
    bySlot.set(slot, arr);
  });

  const nodes: PlacedNode[] = [];
  const sections: SeriesSection[] = [];
  const addCells: AddCell[] = [];
  let maxRow = 1;

  for (const [slot, indices] of bySlot) {
    const col = slot - 1;
    const used = new Set<number>();
    let runningRow = 0;
    // Each series' first row, in stack order — used to attribute add-cells.
    const bands: { seriesIndex: number; minRow: number }[] = [];

    indices.forEach((si, k) => {
      const s = series[si];
      let sMin = Infinity;
      let sMax = 0;

      for (const m of s.missions) {
        let desired =
          m.position != null && Number.isFinite(m.position)
            ? Math.max(1, Math.round(m.position))
            : runningRow + 1;
        if (desired < 1) desired = 1;
        while (used.has(desired)) desired++;
        used.add(desired);
        runningRow = desired;
        sMin = Math.min(sMin, desired);
        sMax = Math.max(sMax, desired);
        nodes.push({
          key: m.key,
          seriesKey: s.key,
          file: s.file,
          seriesIndex: si,
          col,
          row: desired,
          mission: m,
          series: s,
        });
        maxRow = Math.max(maxRow, desired);
      }

      if (!Number.isFinite(sMin)) {
        // Empty series (e.g. every mission just deleted): reserve one row so its
        // header + an add-cell still render, and later series stack below it.
        let r = runningRow + 1;
        while (used.has(r)) r++;
        sMin = r;
        sMax = r;
        runningRow = r;
      }

      sections.push({
        seriesIndex: si,
        seriesKey: s.key,
        series: s,
        slot,
        col,
        minRow: sMin,
        maxRow: sMax,
        approx: s.approx === true,
        first: k === 0,
      });
      bands.push({ seriesIndex: si, minRow: sMin });
      maxRow = Math.max(maxRow, sMax);
    });

    // Add-cells: every unoccupied row up to one past the slot's last row, owned by
    // the last series whose band starts at or above it (so a "+" lands in the right
    // file/series). One trailing row always offers an append point.
    const slotMax = Math.max(1, ...used, ...bands.map((b) => b.minRow));
    for (let r = 1; r <= slotMax + 1; r++) {
      if (used.has(r)) continue;
      let owner = bands[0];
      for (const b of bands) if (b.minRow <= r) owner = b;
      if (!owner) continue;
      addCells.push({ seriesIndex: owner.seriesIndex, series: series[owner.seriesIndex], col, row: r });
    }
  }

  // Arrow resolution — first placement wins for a repeated bare key.
  const posByKey = new Map<string, PlacedNode>();
  for (const n of nodes) if (!posByKey.has(n.key)) posByKey.set(n.key, n);

  const arrows: BoardArrow[] = [];
  const externals: ExternalRef[] = [];
  for (const n of nodes) {
    for (const req of n.mission.requiredMissions) {
      const src = posByKey.get(req);
      if (src) {
        arrows.push({
          fromKey: req,
          toKey: n.key,
          fromCol: src.col,
          fromRow: src.row,
          toCol: n.col,
          toRow: n.row,
          cross: src.col !== n.col,
        });
      } else {
        externals.push({ seriesIndex: n.seriesIndex, nodeKey: n.key, missingKey: req });
      }
    }
  }

  return { nodes, sections, addCells, arrows, externals, maxRow, posByKey };
}

/**
 * Merges every displayed series' `required_missions` into one edge map
 * (mission key → prerequisite keys), so cycle checks span the combined graph.
 */
export function combinedEdges(series: MissionSeries[]): Map<string, string[]> {
  const edges = new Map<string, string[]>();
  for (const s of series) {
    for (const m of s.missions) {
      const prev = edges.get(m.key);
      edges.set(m.key, prev ? [...prev, ...m.requiredMissions] : [...m.requiredMissions]);
    }
  }
  return edges;
}

/**
 * Whether adding "`dependent` requires `prereq`" would create a cycle in the
 * combined graph `edges`. True when `prereq === dependent` or `prereq` already
 * (transitively) requires `dependent`. Mirrors backend `missions::creates_cycle`,
 * but over the union of all displayed series (cross-series links are allowed).
 */
export function combinedCreatesCycle(
  edges: Map<string, string[]>,
  dependent: string,
  prereq: string,
): boolean {
  if (dependent === prereq) return true;
  const seen = new Set<string>();
  const stack = [...(edges.get(prereq) ?? [])];
  while (stack.length) {
    const c = stack.pop()!;
    if (c === dependent) return true;
    if (seen.has(c)) continue;
    seen.add(c);
    for (const r of edges.get(c) ?? []) stack.push(r);
  }
  return false;
}
