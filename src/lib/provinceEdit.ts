// Province Colors map-mode structural editing — pure client-side helpers.
//
// These operate on the two buffers MapView already holds for the raster mode:
//   - `ids`:  Uint16Array, one province id per pixel (top-down `y*w + x`),
//             `0xffff` = the "no province" sentinel (see the project's
//             hit-testing convention).
//   - `rgba`: the province_colors image data (RGBA, 4 bytes/pixel), same
//             top-down index — used to read a province's actual bitmap color.
//
// The output is `BmpOp`s (mirrored by the backend `province_edit::BmpOp`): the
// frontend ships these semantics, never the 34 MB bitmap. Pixel indices are the
// SAME top-down `y*w + x` the backend paints, so no coordinate translation is
// needed at the IPC boundary.

import type { BmpOp } from "$lib/edits.svelte";

/** The "no province" sentinel in the id buffer. */
export const NO_PROVINCE = 0xffff;

type Ids = Uint16Array;
type Rgb = [number, number, number];

/**
 * Flat pixel indices inside a filled disc of `radius` (in map pixels) centered
 * on `(cx, cy)`. Vertically clamped to the map; horizontally wrapped at the
 * antimeridian so a brush near the map seam paints across it (matching the
 * renderer's wrap convention). A radius of 0 paints the single center pixel.
 */
export function brushDisc(
  cx: number,
  cy: number,
  radius: number,
  w: number,
  h: number,
): number[] {
  const out: number[] = [];
  const r = Math.max(0, Math.floor(radius));
  const r2 = r * r;
  for (let dy = -r; dy <= r; dy++) {
    const y = cy + dy;
    if (y < 0 || y >= h) continue;
    for (let dx = -r; dx <= r; dx++) {
      if (dx * dx + dy * dy > r2) continue;
      const x = (((cx + dx) % w) + w) % w; // horizontal wrap
      out.push(y * w + x);
    }
  }
  return out;
}

/** Every pixel index belonging to province `id`. */
export function provincePixels(ids: Ids, id: number): number[] {
  const out: number[] = [];
  for (let i = 0; i < ids.length; i++) if (ids[i] === id) out.push(i);
  return out;
}

/**
 * Province ids 4-adjacent (horizontal wrap honored, vertical clamped) to any
 * pixel of province `id` — its map neighbours. Excludes `id` itself and the
 * sentinel. This is the candidate set for Dissolve's "divide between the
 * neighbours" target picking.
 */
export function borderingProvinces(ids: Ids, id: number, w: number, h: number): number[] {
  const nb = new Set<number>();
  for (let i = 0; i < ids.length; i++) {
    if (ids[i] !== id) continue;
    const x = i % w;
    const y = (i / w) | 0;
    const cands = [
      x === 0 ? i + w - 1 : i - 1,
      x === w - 1 ? i + 1 - w : i + 1,
      y > 0 ? i - w : -1,
      y < h - 1 ? i + w : -1,
    ];
    for (const j of cands) {
      if (j < 0) continue;
      const o = ids[j];
      if (o !== id && o !== NO_PROVINCE) nb.add(o);
    }
  }
  return [...nb];
}

/**
 * The bitmap RGB of province `id`, read from the province_colors image data at
 * the first pixel that belongs to it. `null` if the province has no pixels
 * (e.g. an RNW stub with a definition but no map presence).
 */
export function provinceColor(ids: Ids, rgba: Uint8ClampedArray, id: number): Rgb | null {
  for (let i = 0; i < ids.length; i++) {
    if (ids[i] === id) return [rgba[i * 4], rgba[i * 4 + 1], rgba[i * 4 + 2]];
  }
  return null;
}

/** RGB at a single pixel index in the province_colors image data. */
export function colorAt(rgba: Uint8ClampedArray, idx: number): Rgb {
  return [rgba[idx * 4], rgba[idx * 4 + 1], rgba[idx * 4 + 2]];
}

/** A Paint op setting the given pixels to `color` (expand / whole-province take / carve). */
export function paintOp(pixels: number[], color: Rgb): BmpOp {
  return { op: "paint", pixels, color };
}

/**
 * A Dissolve op removing province color `from`, dividing its pixels among the
 * `into` target colors (each dissolved pixel to the nearest target). One target
 * is a plain merge; several is the split.
 */
export function dissolveOp(from: Rgb, into: Rgb[]): BmpOp {
  return { op: "dissolve", from, into };
}

// --- Client-side op applier (mirrors backend `province_edit::apply_ops`) ------
//
// Used to keep the live province_colors edit canvas exact: the displayed bitmap
// is always the pristine (saved) image with every PENDING op re-applied, so
// undo/redo reflect immediately without a backend re-render. The dissolve BFS is
// the same multi-source, 4-connected (horizontal-wrap) flood the backend runs.

function rgbEq(rgba: Uint8ClampedArray, idx: number, c: Rgb): boolean {
  const o = idx * 4;
  return rgba[o] === c[0] && rgba[o + 1] === c[1] && rgba[o + 2] === c[2];
}

function setRgb(rgba: Uint8ClampedArray, idx: number, c: Rgb): void {
  const o = idx * 4;
  rgba[o] = c[0];
  rgba[o + 1] = c[1];
  rgba[o + 2] = c[2];
}

function neighborIdx(idx: number, w: number, h: number): number[] {
  const x = idx % w;
  const y = (idx / w) | 0;
  const out = [x === 0 ? idx + w - 1 : idx - 1, x === w - 1 ? idx + 1 - w : idx + 1];
  if (y > 0) out.push(idx - w);
  if (y < h - 1) out.push(idx + w);
  return out;
}

function dissolveRgba(rgba: Uint8ClampedArray, from: Rgb, into: Rgb[], w: number, h: number): void {
  if (into.length === 0) return;
  const n = w * h;
  const fromPixels: number[] = [];
  for (let i = 0; i < n; i++) if (rgbEq(rgba, i, from)) fromPixels.push(i);
  if (fromPixels.length === 0) return;
  const isTarget = (idx: number) => into.some((t) => rgbEq(rgba, idx, t));
  const assigned = new Map<number, Rgb>();
  const q: number[] = [];
  for (const p of fromPixels) {
    for (const nb of neighborIdx(p, w, h)) {
      if (isTarget(nb) && !assigned.has(p)) assigned.set(p, colorAt(rgba, nb));
    }
    if (assigned.has(p)) q.push(p);
  }
  for (let head = 0; head < q.length; head++) {
    const p = q[head];
    const c = assigned.get(p)!;
    for (const nb of neighborIdx(p, w, h)) {
      if (rgbEq(rgba, nb, from) && !assigned.has(nb)) {
        assigned.set(nb, c);
        q.push(nb);
      }
    }
  }
  const fallback = into[0];
  for (const p of fromPixels) setRgb(rgba, p, assigned.get(p) ?? fallback);
}

/** Applies `ops` in order to `rgba` (RGBA image data) in place — the client
 *  mirror of the backend so the preview matches what a save will write. */
export function applyOpsToRgba(rgba: Uint8ClampedArray, ops: BmpOp[], w: number, h: number): void {
  for (const op of ops) {
    if (op.op === "paint") {
      for (const p of op.pixels) setRgb(rgba, p, op.color);
    } else {
      dissolveRgba(rgba, op.from, op.into, w, h);
    }
  }
}
