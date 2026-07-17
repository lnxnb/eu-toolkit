// Phase 0.7 — per-province icon/label/stat overlay: pure geometry + centroid math.
//
// This module holds the framework-free core of the overlay layer (spec 7.6):
// province-centroid computation from the per-pixel province-id buffer, the
// map→screen projection, and the zoom-threshold/size/culling math. The Svelte
// component `IconOverlay.svelte` is a thin renderer over these functions; keeping
// the math here makes it unit-testable without a DOM (see the scratch harness in
// the module's tests) and reusable by future overlays (religion icons, dev stat
// boxes, CoT badges — all ride the same layer).
//
// ── Coordinate model (mirrors MapView) ───────────────────────────────────────
// The map bitmap is `mapW × mapH` *map pixels*. MapView draws it with
// `ctx.setTransform(scale·dpr, 0, 0, scale·dpr, offsetX·dpr, offsetY·dpr)`, i.e.
//   screenCssX = mapX · scale + offsetX
//   screenCssY = mapY · scale + offsetY
// `scale` is map-pixels-per-CSS-pixel; `offsetX/Y` are in CSS pixels. The overlay
// canvas shares the container's CSS size and applies `dpr` itself. `Viewport`
// below carries exactly those four numbers, so the overlay stays pixel-aligned
// with the map with no extra glue.

/** The map→screen transform, identical to MapView's `scale`/`offsetX`/`offsetY`. */
export interface Viewport {
  /** Map-pixels → CSS-pixels factor (MapView's `scale`). */
  scale: number;
  /** Pan in CSS pixels (MapView's `offsetX`). */
  offsetX: number;
  /** Pan in CSS pixels (MapView's `offsetY`). */
  offsetY: number;
}

/** A point in map-pixel space. */
export interface Point {
  x: number;
  y: number;
}

/** Tuning for the fade/size behaviour across the zoom threshold. */
export interface OverlayConfig {
  /** Below this scale icons are fully hidden. */
  fadeStart: number;
  /** At/above this scale icons are fully opaque; linear fade in between. */
  fadeEnd: number;
  /** Nominal icon footprint in *map pixels*; on-screen size = this · scale. */
  baseMapSize: number;
  /** Clamp of the on-screen icon size, in CSS pixels. */
  minPx: number;
  maxPx: number;
}

export const DEFAULT_CONFIG: OverlayConfig = {
  fadeStart: 2,
  fadeEnd: 3,
  baseMapSize: 12,
  minPx: 14,
  maxPx: 44,
};

/**
 * Per-province centroid cache. Keyed to a specific province-id buffer instance;
 * recompute only when the buffer changes (it's stable for a session).
 */
export interface CentroidCache {
  /** province id → centroid in map-pixel space (interior-snapped). */
  centroids: Map<number, Point>;
  /** The buffer these centroids were computed from (identity check). */
  source: Uint16Array;
  mapW: number;
  mapH: number;
}

/** The NONE sentinel used by the province-id buffer (skip these pixels). */
const NONE = 0xffff;

/**
 * Computes the interior-snapped centroid of every province in `ids`
 * (`mapW × mapH`, row-major). One pass accumulates the mean pixel and bounding
 * box per id; a second pass snaps any centroid that lands outside its province
 * (crescent/annular shapes) to the nearest interior pixel via a bounded spiral.
 *
 * Cost is O(pixels) plus, only for the handful of provinces whose mean is
 * exterior, a small local search — cheap at map resolution and done once per
 * session (cache the result).
 */
export function computeCentroids(
  ids: Uint16Array,
  mapW: number,
  mapH: number,
): Map<number, Point> {
  interface Acc {
    sx: number;
    sy: number;
    n: number;
    minX: number;
    minY: number;
    maxX: number;
    maxY: number;
  }
  const acc = new Map<number, Acc>();

  for (let y = 0; y < mapH; y++) {
    const row = y * mapW;
    for (let x = 0; x < mapW; x++) {
      const id = ids[row + x];
      if (id === NONE) continue;
      let a = acc.get(id);
      if (a === undefined) {
        a = { sx: 0, sy: 0, n: 0, minX: x, minY: y, maxX: x, maxY: y };
        acc.set(id, a);
      }
      a.sx += x;
      a.sy += y;
      a.n++;
      if (x < a.minX) a.minX = x;
      if (x > a.maxX) a.maxX = x;
      if (y < a.minY) a.minY = y;
      if (y > a.maxY) a.maxY = y;
    }
  }

  const out = new Map<number, Point>();
  for (const [id, a] of acc) {
    const cx = a.sx / a.n;
    const cy = a.sy / a.n;
    const px = Math.round(cx);
    const py = Math.round(cy);
    if (isInterior(ids, mapW, mapH, px, py, id)) {
      out.set(id, { x: cx, y: cy });
    } else {
      const snapped = snapToInterior(ids, mapW, mapH, px, py, id, a);
      out.set(id, snapped ?? { x: cx, y: cy });
    }
  }
  return out;
}

function isInterior(
  ids: Uint16Array,
  mapW: number,
  mapH: number,
  x: number,
  y: number,
  id: number,
): boolean {
  if (x < 0 || y < 0 || x >= mapW || y >= mapH) return false;
  return ids[y * mapW + x] === id;
}

/**
 * Nearest pixel belonging to `id`, searched in growing square rings around
 * `(x, y)`. Bounded by the province's own bounding box, so a stray id can never
 * scan the whole map. Returns the map-pixel center, or null if nothing is found
 * within the box (degenerate input).
 */
function snapToInterior(
  ids: Uint16Array,
  mapW: number,
  mapH: number,
  x: number,
  y: number,
  id: number,
  box: { minX: number; minY: number; maxX: number; maxY: number },
): Point | null {
  const maxR =
    Math.max(box.maxX - box.minX, box.maxY - box.minY, 1) + 1;
  for (let r = 1; r <= maxR; r++) {
    // Walk the perimeter of the r-ring only.
    for (let dx = -r; dx <= r; dx++) {
      for (const dy of dx === -r || dx === r ? range(-r, r) : [-r, r]) {
        const nx = x + dx;
        const ny = y + dy;
        if (isInterior(ids, mapW, mapH, nx, ny, id)) {
          return { x: nx, y: ny };
        }
      }
    }
  }
  return null;
}

function range(lo: number, hi: number): number[] {
  const out: number[] = [];
  for (let v = lo; v <= hi; v++) out.push(v);
  return out;
}

/** Map-pixel point → screen CSS-pixel point under `view`. */
export function project(p: Point, view: Viewport): Point {
  return {
    x: p.x * view.scale + view.offsetX,
    y: p.y * view.scale + view.offsetY,
  };
}

/**
 * Icon opacity across the zoom threshold: 0 below `fadeStart`, 1 at/above
 * `fadeEnd`, linear in between. Lets the layer fade in as provinces grow large
 * enough for a legible icon.
 */
export function iconOpacity(scale: number, cfg: OverlayConfig): number {
  if (scale <= cfg.fadeStart) return 0;
  if (scale >= cfg.fadeEnd) return 1;
  return (scale - cfg.fadeStart) / (cfg.fadeEnd - cfg.fadeStart);
}

/** On-screen icon size (CSS px): `baseMapSize · scale`, clamped to [min, max]. */
export function iconSize(scale: number, cfg: OverlayConfig): number {
  return Math.min(cfg.maxPx, Math.max(cfg.minPx, cfg.baseMapSize * scale));
}

/**
 * Viewport culling: is a mark of half-extent `half` (CSS px) centered at screen
 * `(sx, sy)` at all visible within a `viewW × viewH` viewport? A small margin
 * keeps marks that straddle the edge.
 */
export function isVisible(
  sx: number,
  sy: number,
  half: number,
  viewW: number,
  viewH: number,
): boolean {
  return (
    sx + half >= 0 && sx - half <= viewW && sy + half >= 0 && sy - half <= viewH
  );
}

/** What to draw on a province: any combination of icon frame, label, stat box. */
export interface OverlayItem {
  /** Frame index into the atlas strip (see icons.rs `get_icon_atlas`). */
  iconIndex?: number;
  /**
   * Optional secondary glyph drawn offset from the primary `iconIndex` (top-right,
   * smaller). Used by the S3.3 trade-details overlay to stack a modifier badge
   * beside a center-of-trade icon (max two glyphs per province). Ignored when
   * there is no primary icon.
   */
  badgeIndex?: number;
  /** A short centered text label (e.g. trade-node name). */
  label?: string;
  /** Small multi-line stat box (e.g. dev tax/production/manpower). */
  statBox?: string[];
  /**
   * Optional per-`statBox`-line atlas frame index, drawn as a small icon to the
   * left of each line (dev stat boxes, 9.3: tax/production/manpower icons).
   * Same length as `statBox`; when absent (or no atlas) the box is text-only, so
   * existing consumers are unaffected.
   */
  statIcons?: number[];
}

/** The sprite strip an overlay draws icon frames from. */
export interface Atlas {
  image: CanvasImageSource;
  frameW: number;
  frameH: number;
  count: number;
}

/**
 * Decodes the `get_icon_atlas` wire buffer:
 * `[u32 headerLen][header JSON][PNG bytes]`, little-endian. Returns the parsed
 * header plus the raw PNG bytes (caller turns them into an ImageBitmap).
 */
export function parseAtlasWire(buf: ArrayBuffer): {
  kind: string;
  frameW: number;
  frameH: number;
  count: number;
  index: Record<string, number>;
  png: Uint8Array;
} {
  const headerLen = new Uint32Array(buf.slice(0, 4))[0];
  const headerJson = new TextDecoder().decode(new Uint8Array(buf, 4, headerLen));
  const header = JSON.parse(headerJson) as {
    kind: string;
    frameW: number;
    frameH: number;
    count: number;
    index: Record<string, number>;
  };
  const png = new Uint8Array(buf, 4 + headerLen);
  return { ...header, png };
}
