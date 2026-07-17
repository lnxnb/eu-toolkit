// Sprint 25 — adjacencies (straits/canals/lakes/land bridges) geometry & folds.
//
// Framework-free core for the Provinces map mode's adjacency overlay. Holds the
// wire types of `get_adjacencies`, the pending-edit fold (one `csvRewrite`
// carries the whole desired row list, so last-wins gives the effective state),
// wrap-aware line geometry between endpoint province centroids (reusing the
// trade-route antimeridian machinery), screen-space hit-testing, and the two
// "+ Add strait" heuristics (type derivation + through-province suggestion).
//
// Kept DOM-free so it is unit-testable; `AdjacencyOverlay.svelte` stays a thin
// renderer and MapView owns pointer handling / edit dispatch.

import type { Viewport, Point } from "$lib/overlay";
import { project } from "$lib/overlay";
import { unwrapControl, wrapCurvePieces, type Xy } from "$lib/tradenet";
import type { TypedEdit } from "$lib/edits.svelte";

// ── Wire types (mirror adjacencies.rs; serialize camelCase) ───────────────────

export interface AdjRow {
  from: number;
  to: number;
  /** The `Type` column: "sea" | "land" | "canal" | "lake". */
  kind: string;
  through: number;
  startX: number;
  startY: number;
  stopX: number;
  stopY: number;
  comment: string;
}

/** A row plus its base-file origin index (null = newly added). Mirrors the
 *  backend `RowInput` (serde-flattened AdjRow + `origin`). */
export type AdjRowInput = AdjRow & { origin: number | null };

export interface AdjacenciesPayload {
  rows: AdjRow[];
  waterIds: number[];
}

export const ADJ_FILE = "map/adjacencies.csv";
export const ADJ_TYPES = ["sea", "land", "canal", "lake"] as const;

export function cloneRow(r: AdjRowInput): AdjRowInput {
  return { ...r };
}

export function baseToInputs(base: AdjRow[]): AdjRowInput[] {
  return base.map((r, i) => ({ ...r, origin: i }));
}

/**
 * Effective adjacency rows = base rows folded with the pending queue. Every
 * `csvRewrite` on the adjacencies file carries the FULL desired list, so the
 * last one wins (this is exactly how the byte-surgical backend applies them).
 * Static/date-agnostic — callers pass the full `serialize()` payload.
 */
export function foldAdjacencies(base: AdjRow[], edits: TypedEdit[]): AdjRowInput[] {
  let rows = baseToInputs(base);
  for (const e of edits) {
    if (e.kind === "csvRewrite" && e.file === ADJ_FILE) {
      rows = e.rows.map(cloneRow);
    }
  }
  return rows;
}

/** Builds the single `csvRewrite` edit that persists `rows` (the full list). */
export function rewriteEdit(rows: AdjRowInput[]): TypedEdit {
  return { kind: "csvRewrite", file: ADJ_FILE, rows: rows.map(cloneRow) };
}

// ── Line geometry (wrap-aware, between endpoint centroids) ────────────────────

/**
 * On-map polyline pieces for the straight adjacency line between endpoint
 * centroids `a` and `b`. Wrap-aware: a link crossing the antimeridian comes
 * back as two pieces going the SHORT way (exit one edge, re-enter the other),
 * reusing the trade-route unwrap/cut machinery.
 */
export function adjLinePieces(a: Xy, b: Xy, mapW: number): Xy[][] {
  return wrapCurvePieces(unwrapControl([a, b], mapW), mapW).filter((p) => p.length >= 2);
}

/** Endpoint centroid pair for a row, or null when either centroid is unknown. */
export function endpoints(
  row: AdjRow,
  centroids: Map<number, Point>,
): [Xy, Xy] | null {
  const a = centroids.get(row.from);
  const b = centroids.get(row.to);
  if (!a || !b) return null;
  return [
    [a.x, a.y],
    [b.x, b.y],
  ];
}

// ── Per-type style ────────────────────────────────────────────────────────────

/** Canvas dash pattern per type: sea = dashed, canal = solid, land = dotted,
 *  lake = dash-dot (distinct). Unknown types fall back to a medium dash. */
export function dashForType(kind: string): number[] {
  switch (kind) {
    case "sea":
      return [7, 5];
    case "canal":
      return [];
    case "land":
      return [1.5, 4];
    case "lake":
      return [11, 4, 2, 4];
    default:
      return [4, 4];
  }
}

/** Base line color per type (emphasis brightens this at draw time). */
export function colorForType(kind: string): string {
  switch (kind) {
    case "sea":
      return "#4aa3df";
    case "canal":
      return "#e08a3c";
    case "land":
      return "#c2cf55";
    case "lake":
      return "#9b7be0";
    default:
      return "#cfd4db";
  }
}

// ── Hit-testing (screen space) ────────────────────────────────────────────────

function segDist2(
  px: number,
  py: number,
  ax: number,
  ay: number,
  bx: number,
  by: number,
): number {
  const dx = bx - ax;
  const dy = by - ay;
  const len2 = dx * dx + dy * dy;
  if (len2 === 0) {
    const ex = px - ax;
    const ey = py - ay;
    return ex * ex + ey * ey;
  }
  let t = ((px - ax) * dx + (py - ay) * dy) / len2;
  t = Math.max(0, Math.min(1, t));
  const cx = ax + t * dx;
  const cy = ay + t * dy;
  return (px - cx) * (px - cx) + (py - cy) * (py - cy);
}

/**
 * Index (into `rows`) of the adjacency line nearest the screen point `sx,sy`,
 * within `tolPx`, or null. Wrap-aware (measures on-map pieces, not the long way).
 */
export function adjacencyAt(
  rows: AdjRow[],
  centroids: Map<number, Point>,
  sx: number,
  sy: number,
  view: Viewport,
  mapW: number,
  tolPx: number,
): number | null {
  let best: number | null = null;
  let bestD = tolPx * tolPx;
  for (let ri = 0; ri < rows.length; ri++) {
    const ep = endpoints(rows[ri], centroids);
    if (!ep) continue;
    for (const piece of adjLinePieces(ep[0], ep[1], mapW)) {
      let prev = project({ x: piece[0][0], y: piece[0][1] }, view);
      for (let i = 1; i < piece.length; i++) {
        const cur = project({ x: piece[i][0], y: piece[i][1] }, view);
        const d = segDist2(sx, sy, prev.x, prev.y, cur.x, cur.y);
        if (d <= bestD) {
          bestD = d;
          best = ri;
        }
        prev = cur;
      }
    }
  }
  return best;
}

// ── "+ Add strait" heuristics ─────────────────────────────────────────────────

/**
 * Default type for a new adjacency between two endpoints, from their
 * water-ness. A land↔land link across water (a strait) and a link touching a
 * water province are both `sea` — the overwhelmingly common case and the only
 * one derivable without proving province adjacency; land bridges / canals /
 * lakes are rare and set explicitly in the editor. So this returns `"sea"`
 * whenever the endpoints don't form an obvious canal, which the caller can't
 * detect cheaply either → always `"sea"` by default (documented deviation:
 * heuristic is intentionally conservative and user-refinable).
 */
export function deriveType(_fromWater: boolean, _toWater: boolean): string {
  return "sea";
}

/**
 * Suggests the `through` water province a fleet would block for a new strait
 * between `a` and `b` (map/top-left space): sample the province-id buffer at
 * the midpoint, and if that isn't water, ring-search outward for the nearest
 * water province. Returns the province id, or -1 when none is found within
 * `maxRadius` pixels. `waterIds` is the sea/lake set; `idAt(x,y)` reads the
 * province-id buffer (0xffff sentinel for out-of-range / none).
 */
export function suggestThrough(
  a: Xy,
  b: Xy,
  waterIds: Set<number>,
  idAt: (x: number, y: number) => number,
  maxRadius = 60,
): number {
  const mx = Math.round((a[0] + b[0]) / 2);
  const my = Math.round((a[1] + b[1]) / 2);
  const NONE = 0xffff;
  const check = (x: number, y: number): number | null => {
    const id = idAt(x, y);
    return id !== NONE && waterIds.has(id) ? id : null;
  };
  const hit0 = check(mx, my);
  if (hit0 != null) return hit0;
  for (let r = 1; r <= maxRadius; r++) {
    // Walk the ring at radius r (square perimeter is enough for a nearest-ish
    // water tile; exact geodesic distance is unnecessary for a suggestion).
    for (let dx = -r; dx <= r; dx++) {
      const top = check(mx + dx, my - r);
      if (top != null) return top;
      const bot = check(mx + dx, my + r);
      if (bot != null) return bot;
    }
    for (let dy = -r + 1; dy <= r - 1; dy++) {
      const left = check(mx - r, my + dy);
      if (left != null) return left;
      const right = check(mx + r, my + dy);
      if (right != null) return right;
    }
  }
  return -1;
}

/** True when the two directed pairs describe the same undirected adjacency. */
export function samePair(a: AdjRow, b: AdjRow): boolean {
  return (a.from === b.from && a.to === b.to) || (a.from === b.to && a.to === b.from);
}
