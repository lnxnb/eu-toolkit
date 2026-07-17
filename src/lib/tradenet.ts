// Sprint 8 — trade-node network geometry, transforms, and hit-testing.
//
// Framework-free core for the Trade Nodes map mode (spec 8.1–8.5). Holds the
// wire types of `get_trade_network`, the top-left⇄file y-flip, curve sampling
// (Catmull-Rom through a route's control points), and screen-space hit-testing
// for node markers, route arrows, and draggable control-point handles. Keeping
// the math here makes it unit-testable without a DOM (see the scratch harness)
// and lets `TradeNetworkOverlay.svelte` stay a thin renderer.
//
// ── Coordinate model ─────────────────────────────────────────────────────────
// Control points arrive from the backend already converted to TOP-LEFT origin
// map pixels (`control`); the untouched FILE-space (bottom-left) values ride
// alongside as `control_file`. The map→screen projection matches MapView/overlay:
//   screenCssX = mapX · scale + offsetX   (see overlay.ts `project`)
// The route editor drags handles in top-left space and converts back to file
// space with `topToFileY` before writing (`y_file = map_height - y_top`).

import type { Viewport, Point } from "$lib/overlay";
import { project } from "$lib/overlay";
import type { TypedEdit } from "$lib/edits.svelte";

// ── Wire types (mirror tradenodes.rs `TradeNetwork`; serialize snake_case) ────

export type Xy = [number, number];

export interface Outgoing {
  /** nth outgoing block in the source node (0-based) → path seg `outgoing#<index>`. */
  index: number;
  target: string;
  path: number[];
  /** Control points in TOP-LEFT origin map pixels (for canvas rendering). */
  control: Xy[];
  /** Same points in FILE (bottom-left) space, for edit round-trips. */
  control_file: Xy[];
}

export interface Incoming {
  from: string;
  outgoing_index: number;
}

export interface TradeNode {
  key: string;
  name: string;
  color: [number, number, number] | null;
  location: number | null;
  inland: boolean;
  end: boolean;
  members: number[];
  source_file: string;
  outgoing: Outgoing[];
  incoming: Incoming[];
  raw_extra: string[];
}

export interface TradeNetwork {
  map_width: number;
  map_height: number;
  nodes: TradeNode[];
}

/** First-draft geometry for a new route (`derive_route_geometry`). */
export interface DerivedRoute {
  control: Xy[];
  control_file: Xy[];
  path: number[];
}

/** A route identified by its owning node key + occurrence index. */
export interface RouteRef {
  from: string;
  index: number;
}

// ── Transforms (top-left ⇄ file, bottom-left origin) ──────────────────────────

/** file-space y → top-left y (and vice-versa; the map is a vertical flip). */
export function topToFileY(yTop: number, mapH: number): number {
  return mapH - yTop;
}
export function fileToTopY(yFile: number, mapH: number): number {
  return mapH - yFile;
}

/** Convert a top-left control point to file-space (for scaffold/SetBlock). */
export function controlToFile(pt: Xy, mapH: number): Xy {
  return [pt[0], topToFileY(pt[1], mapH)];
}

/** Flatten top-left control points to the `"x y x y …"` file-space string that
 *  the backend `control` SetBlock / scaffold expects (6-dp, matching vanilla). */
export function controlToFileString(control: Xy[], mapH: number): string {
  return control
    .map((p) => {
      const [x, y] = controlToFile(p, mapH);
      return `${x.toFixed(6)} ${y.toFixed(6)}`;
    })
    .join(" ");
}

// ── Curve sampling (Catmull-Rom through the control points) ───────────────────

/**
 * Samples a smooth curve through `points` (map space). With ≤2 points it is the
 * straight segment; with more it is a centripetal-ish Catmull-Rom spline with
 * `seg` samples per span. Endpoints are duplicated so the curve passes through
 * the first and last control point (the node markers).
 */
export function samplePolyline(points: Xy[], seg = 16): Xy[] {
  if (points.length <= 2) return points.slice();
  const out: Xy[] = [];
  const n = points.length;
  for (let i = 0; i < n - 1; i++) {
    const p0 = points[i === 0 ? 0 : i - 1];
    const p1 = points[i];
    const p2 = points[i + 1];
    const p3 = points[i + 2 < n ? i + 2 : n - 1];
    for (let s = 0; s < seg; s++) {
      const t = s / seg;
      out.push(catmull(p0, p1, p2, p3, t));
    }
  }
  out.push(points[n - 1]);
  return out;
}

function catmull(p0: Xy, p1: Xy, p2: Xy, p3: Xy, t: number): Xy {
  const t2 = t * t;
  const t3 = t2 * t;
  const f = (a: number, b: number, c: number, d: number) =>
    0.5 *
    (2 * b + (-a + c) * t + (2 * a - 5 * b + 4 * c - d) * t2 + (-a + 3 * b - 3 * c + d) * t3);
  return [f(p0[0], p1[0], p2[0], p3[0]), f(p0[1], p1[1], p2[1], p3[1])];
}

// ── World wrap (antimeridian) ─────────────────────────────────────────────────
//
// The map wraps horizontally. Vanilla tradenodes.txt encodes a wrapping route
// (e.g. Nippon → California across the Pacific) as control points whose x jumps
// by nearly the map width between consecutive points — the wrap is implied, not
// stored. Sampling those points literally draws a line the long way across the
// whole map. The fix: "unwrap" the sequence into a continuous coordinate space
// (shift each point by ±mapW to stay near its predecessor), run the spline
// there, then cut the sampled curve back into on-map pieces at the edges.

/**
 * Unwraps a control-point sequence: each point after the first is shifted by
 * the multiple of `mapW` that puts it nearest its predecessor, so an implied
 * antimeridian crossing becomes a continuous line (x may leave [0, mapW)).
 */
export function unwrapControl(points: Xy[], mapW: number): Xy[] {
  if (points.length === 0 || mapW <= 0) return points.slice();
  const out: Xy[] = [[points[0][0], points[0][1]]];
  for (let i = 1; i < points.length; i++) {
    const px = out[i - 1][0];
    const x = points[i][0] + Math.round((px - points[i][0]) / mapW) * mapW;
    out.push([x, points[i][1]]);
  }
  return out;
}

/**
 * Cuts an unwrapped polyline into pieces that all lie within [0, mapW): each
 * boundary crossing ends the current piece exactly at one map edge and starts
 * the next at the opposite edge (interpolated y), so wrapped routes visually
 * exit one side and re-enter the other.
 */
export function wrapCurvePieces(samples: Xy[], mapW: number): Xy[][] {
  const cell = (x: number) => Math.floor(x / mapW);
  const pieces: Xy[][] = [];
  let cur: Xy[] = [];
  for (let i = 0; i < samples.length; i++) {
    const [x, y] = samples[i];
    if (i > 0) {
      const [px, py] = samples[i - 1];
      const c0 = cell(px);
      const c1 = cell(x);
      if (c1 !== c0) {
        const dir = c1 > c0 ? 1 : -1;
        for (let c = c0; c !== c1; c += dir) {
          const bx = (dir > 0 ? c + 1 : c) * mapW;
          const t = (bx - px) / (x - px);
          const by = py + (y - py) * t;
          cur.push([dir > 0 ? mapW : 0, by]);
          if (cur.length > 1) pieces.push(cur);
          cur = [[dir > 0 ? 0 : mapW, by]];
        }
      }
    }
    cur.push([x - cell(x) * mapW, y]);
  }
  if (cur.length > 1 || pieces.length === 0) pieces.push(cur);
  return pieces;
}

/**
 * Wrap-aware route curve: unwraps the control points, samples the spline in
 * continuous space, and returns the on-map polyline pieces (one piece when the
 * route doesn't cross the antimeridian). Rendering and hit-testing both use
 * this so they stay geometrically identical.
 */
export function sampleRouteCurve(control: Xy[], mapW: number, seg = 16): Xy[][] {
  return wrapCurvePieces(samplePolyline(unwrapControl(control, mapW), seg), mapW);
}

// ── Node marker placement ─────────────────────────────────────────────────────

/**
 * The marker anchor for a node in map (top-left) space: the centroid of its
 * `location` province, or — if that centroid is unknown — the first control
 * point of its first outgoing route. Returns null when neither is available.
 */
export function markerPoint(
  node: TradeNode,
  centroids: Map<number, Point>,
): Point | null {
  if (node.location != null) {
    const c = centroids.get(node.location);
    if (c) return c;
  }
  const first = node.outgoing[0]?.control[0];
  if (first) return { x: first[0], y: first[1] };
  return null;
}

// ── Hit-testing (screen space) ────────────────────────────────────────────────

function dist2(ax: number, ay: number, bx: number, by: number): number {
  const dx = ax - bx;
  const dy = ay - by;
  return dx * dx + dy * dy;
}

/** Squared distance from point p to segment ab, all in screen CSS px. */
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
  if (len2 === 0) return dist2(px, py, ax, ay);
  let t = ((px - ax) * dx + (py - ay) * dy) / len2;
  t = Math.max(0, Math.min(1, t));
  return dist2(px, py, ax + t * dx, ay + t * dy);
}

/** Node key whose marker is under the screen point (within `radiusPx`), or null.
 *  Nearest marker wins when several overlap. */
export function markerAt(
  network: TradeNetwork,
  centroids: Map<number, Point>,
  sx: number,
  sy: number,
  view: Viewport,
  radiusPx: number,
): string | null {
  let best: string | null = null;
  let bestD = radiusPx * radiusPx;
  for (const node of network.nodes) {
    const m = markerPoint(node, centroids);
    if (!m) continue;
    const s = project(m, view);
    const d = dist2(sx, sy, s.x, s.y);
    if (d <= bestD) {
      bestD = d;
      best = node.key;
    }
  }
  return best;
}

/** The route (from-node + index) whose curve passes nearest the screen point,
 *  within `tolPx`, or null. Curves are sampled from their control points. */
export function routeAt(
  network: TradeNetwork,
  sx: number,
  sy: number,
  view: Viewport,
  tolPx: number,
): RouteRef | null {
  let best: RouteRef | null = null;
  let bestD = tolPx * tolPx;
  for (const node of network.nodes) {
    for (const route of node.outgoing) {
      if (route.control.length < 2) continue;
      for (const curve of sampleRouteCurve(route.control, network.map_width, 12)) {
        if (curve.length < 2) continue;
        let prev = project({ x: curve[0][0], y: curve[0][1] }, view);
        for (let i = 1; i < curve.length; i++) {
          const cur = project({ x: curve[i][0], y: curve[i][1] }, view);
          const d = segDist2(sx, sy, prev.x, prev.y, cur.x, cur.y);
          if (d <= bestD) {
            bestD = d;
            best = { from: node.key, index: route.index };
          }
          prev = cur;
        }
      }
    }
  }
  return best;
}

/** Index of the control-point handle under the screen point (within `halfPx`),
 *  or -1. `control` is top-left map space. Nearest handle wins. */
export function handleAt(
  control: Xy[],
  sx: number,
  sy: number,
  view: Viewport,
  halfPx: number,
): number {
  let best = -1;
  let bestD = halfPx * halfPx;
  for (let i = 0; i < control.length; i++) {
    const s = project({ x: control[i][0], y: control[i][1] }, view);
    const d = dist2(sx, sy, s.x, s.y);
    if (d <= bestD) {
      bestD = d;
      best = i;
    }
  }
  return best;
}

/**
 * The insertion index for a new handle when the user clicks the curve at map
 * point `pt`: the control-point span (i → i+1) whose segment lies nearest the
 * click, so the new handle drops between them. Wrap-aware: a span crossing the
 * antimeridian is measured as its on-map pieces, not the long way across.
 * Returns an index in [1, len].
 */
export function insertIndexAt(control: Xy[], pt: Xy, view: Viewport, mapW: number): number {
  if (control.length < 2) return control.length;
  const un = unwrapControl(control, mapW);
  const s = project({ x: pt[0], y: pt[1] }, view);
  let best = 1;
  let bestD = Infinity;
  for (let i = 0; i < un.length - 1; i++) {
    for (const piece of wrapCurvePieces([un[i], un[i + 1]], mapW)) {
      if (piece.length < 2) continue;
      for (let j = 0; j < piece.length - 1; j++) {
        const a = project({ x: piece[j][0], y: piece[j][1] }, view);
        const b = project({ x: piece[j + 1][0], y: piece[j + 1][1] }, view);
        const d = segDist2(s.x, s.y, a.x, a.y, b.x, b.y);
        if (d < bestD) {
          bestD = d;
          best = i + 1;
        }
      }
    }
  }
  return best;
}

/** Screen-space point of a top-left map point (thin re-export for the overlay). */
export function toScreen(pt: Xy, view: Viewport): Point {
  return project({ x: pt[0], y: pt[1] }, view);
}

// ── Effective network (base + PENDING) ────────────────────────────────────────
//
// The overlay and node panel read a network folded from the base payload plus
// the typed edit queue, so route reshapes, membership steals, node create/delete
// and toggles all appear live and undo/redo revert them. Only the edit shapes
// this frontend generates are folded (see tradenodes.rs recipes). Structural
// edits are applied in queue order; `outgoing#<index>` addresses the nth route
// in the working list at that point, exactly as the byte-surgical writer does.

function parseTriple(s: string): [number, number, number] | null {
  const p = s.trim().split(/\s+/).map(Number);
  return p.length >= 3 && p.every((n) => Number.isFinite(n)) ? [p[0], p[1], p[2]] : null;
}
function parseFloats(s: string): Xy[] {
  const f = s.trim().split(/\s+/).map(Number).filter((n) => Number.isFinite(n));
  const out: Xy[] = [];
  for (let i = 0; i + 1 < f.length; i += 2) out.push([f[i], f[i + 1]]);
  return out;
}
function parseInts(s: string): number[] {
  return s.trim().split(/\s+/).map((t) => parseInt(t, 10)).filter((n) => Number.isFinite(n));
}
/** Occurrence index from an `outgoing#<n>` path segment, or null. */
function outgoingIndex(seg: string): number | null {
  const m = /^outgoing#(\d+)$/.exec(seg);
  return m ? parseInt(m[1], 10) : null;
}
function firstBraces(s: string): string {
  const a = s.indexOf("{");
  const b = s.lastIndexOf("}");
  return a >= 0 && b > a ? s.slice(a + 1, b) : "";
}

/** Parses a `scaffold_trade_node` block into a TradeNode, or null. */
export function parseNodeScaffold(text: string): TradeNode | null {
  const key = /^\s*([A-Za-z0-9_]+)\s*=/.exec(text)?.[1];
  if (!key) return null;
  const loc = /location\s*=\s*(\d+)/.exec(text);
  const col = /color\s*=\s*\{\s*(\d+)\s+(\d+)\s+(\d+)\s*\}/.exec(text);
  const memBlock = /members\s*=\s*\{([\s\S]*?)\}/.exec(text);
  const members = memBlock ? parseInts(memBlock[1]) : [];
  return {
    key,
    name: key,
    color: col ? [+col[1], +col[2], +col[3]] : null,
    location: loc ? parseInt(loc[1], 10) : null,
    inland: /(^|\s)inland\s*=\s*yes/.test(text),
    end: /(^|\s)end\s*=\s*yes/.test(text),
    members,
    source_file: "",
    outgoing: [],
    incoming: [],
    raw_extra: [],
  };
}

/** Parses a `scaffold_trade_route` `outgoing={…}` statement, or null. */
export function parseRouteScaffold(text: string, mapH: number, index: number): Outgoing | null {
  const name = /name\s*=\s*"([^"]*)"/.exec(text);
  if (!name) return null;
  const pathBlock = /path\s*=\s*\{([\s\S]*?)\}/.exec(text);
  const ctrlBlock = /control\s*=\s*\{([\s\S]*?)\}/.exec(text);
  const controlFile = ctrlBlock ? parseFloats(ctrlBlock[1]) : [];
  return {
    index,
    target: name[1],
    path: pathBlock ? parseInts(pathBlock[1]) : [],
    control_file: controlFile,
    control: controlFile.map(([x, y]) => [x, fileToTopY(y, mapH)] as Xy),
  };
}

function cloneNode(n: TradeNode): TradeNode {
  return {
    ...n,
    color: n.color ? ([...n.color] as [number, number, number]) : null,
    members: n.members.slice(),
    outgoing: n.outgoing.map((o) => ({
      ...o,
      path: o.path.slice(),
      control: o.control.map((p) => [...p] as Xy),
      control_file: o.control_file.map((p) => [...p] as Xy),
    })),
    incoming: [],
    raw_extra: n.raw_extra.slice(),
  };
}

/** Folds the typed edit queue over `base`, returning the effective network. */
export function foldNetwork(base: TradeNetwork, edits: TypedEdit[]): TradeNetwork {
  const mapH = base.map_height;
  const nodes: TradeNode[] = base.nodes.map(cloneNode);
  const byKey = new Map<string, TradeNode>();
  for (const n of nodes) byKey.set(n.key, n);

  const applyControl = (n: TradeNode, idx: number, ctrlFile: Xy[]) => {
    const o = n.outgoing[idx];
    if (!o) return;
    o.control_file = ctrlFile;
    o.control = ctrlFile.map(([x, y]) => [x, fileToTopY(y, mapH)] as Xy);
  };

  for (const e of edits) {
    switch (e.kind) {
      case "appendText":
      case "createFile": {
        const created = parseNodeScaffold(e.text);
        if (created && !byKey.has(created.key)) {
          created.source_file = e.file;
          nodes.push(created);
          byKey.set(created.key, created);
        }
        break;
      }
      case "setScalar": {
        if (e.path.length === 2 && e.path[1] === "location") {
          const n = byKey.get(e.path[0]);
          if (n) n.location = parseInt(e.value, 10) || null;
        } else if (e.path.length === 3 && e.path[2] === "name") {
          const n = byKey.get(e.path[0]);
          const idx = outgoingIndex(e.path[1]);
          if (n && idx != null && n.outgoing[idx]) n.outgoing[idx].target = e.value.replace(/"/g, "");
        }
        break;
      }
      case "setBlock": {
        const n = byKey.get(e.path[0]);
        if (!n) break;
        if (e.path.length === 2 && e.path[1] === "color") {
          n.color = parseTriple(e.value);
        } else if (e.path.length === 3) {
          const idx = outgoingIndex(e.path[1]);
          if (idx == null) break;
          if (e.path[2] === "control") applyControl(n, idx, parseFloats(e.value));
          else if (e.path[2] === "path" && n.outgoing[idx]) n.outgoing[idx].path = parseInts(e.value);
        }
        break;
      }
      case "insertStatement": {
        if (e.blockPath.length !== 1) break;
        const n = byKey.get(e.blockPath[0]);
        if (!n) break;
        const st = e.statement.trim();
        if (/^inland\s*=\s*yes/.test(st)) n.inland = true;
        else if (/^end\s*=\s*yes/.test(st)) n.end = true;
        else if (/^color\s*=/.test(st)) {
          const t = parseTriple(firstBraces(st));
          if (t) n.color = t;
        } else if (/^outgoing\s*=/.test(st)) {
          const o = parseRouteScaffold(st, mapH, n.outgoing.length);
          if (o) n.outgoing.push(o);
        }
        break;
      }
      case "removeStatement": {
        if (e.blockPath.length === 0 && byKey.has(e.key)) {
          const i = nodes.findIndex((x) => x.key === e.key);
          if (i >= 0) nodes.splice(i, 1);
          byKey.delete(e.key);
        } else if (e.blockPath.length === 1) {
          const n = byKey.get(e.blockPath[0]);
          if (!n) break;
          if (e.key === "inland") n.inland = false;
          else if (e.key === "end") n.end = false;
          else if (e.key.startsWith("outgoing#")) {
            const idx = outgoingIndex(e.key);
            if (idx != null && idx < n.outgoing.length) n.outgoing.splice(idx, 1);
          }
        }
        break;
      }
      case "addId": {
        if (e.listPath.length === 2 && e.listPath[1] === "members") {
          const n = byKey.get(e.listPath[0]);
          const id = parseInt(e.id, 10);
          if (n && !n.members.includes(id)) n.members.push(id);
        }
        break;
      }
      case "removeId": {
        if (e.listPath.length === 2 && e.listPath[1] === "members") {
          const n = byKey.get(e.listPath[0]);
          const id = parseInt(e.id, 10);
          if (n) {
            const i = n.members.indexOf(id);
            if (i >= 0) n.members.splice(i, 1);
          }
        }
        break;
      }
      case "listMove": {
        if (e.fromPath[1] === "members" && e.toPath[1] === "members") {
          const id = parseInt(e.id, 10);
          const from = byKey.get(e.fromPath[0]);
          const to = byKey.get(e.toPath[0]);
          if (from) {
            const i = from.members.indexOf(id);
            if (i >= 0) from.members.splice(i, 1);
          }
          if (to && !to.members.includes(id)) to.members.push(id);
        }
        break;
      }
    }
  }

  // Renumber occurrence indices to the current array positions and rebuild the
  // incoming reverse index (both may have shifted under splices/appends).
  const has = (k: string) => byKey.has(k);
  for (const n of nodes) {
    n.outgoing.forEach((o, i) => (o.index = i));
    n.incoming = [];
  }
  for (const n of nodes) {
    n.outgoing.forEach((o, i) => {
      const t = byKey.get(o.target);
      if (t && has(o.target)) t.incoming.push({ from: n.key, outgoing_index: i });
    });
  }
  return { map_width: base.map_width, map_height: base.map_height, nodes };
}

/** province id → node key, for map recolor / hit-testing over the effective net. */
export function membershipIndex(network: TradeNetwork): Map<number, string> {
  const m = new Map<number, string>();
  for (const n of network.nodes) for (const id of n.members) m.set(id, n.key);
  return m;
}
