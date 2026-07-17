// Sprint 11.1 — climate (framework-free core).
//
// map/climate.txt is a set of province-id lists split across TWO independent
// slots plus a monsoon date block we never touch:
//   • climate zone:   tropical / arid / arctic / impassable   (absence = temperate)
//   • winter severity: mild_winter / normal_winter / severe_winter (absence = none)
// A province may appear in one list per slot; painting one slot must never
// clobber the other. This module holds the `get_climate` wire types, folds the
// typed edit queue over the base payload so paints/steals/creates appear live
// (and undo/redo revert them), and supplies the slot colors the map recolor uses.

import type { TypedEdit } from "$lib/edits.svelte";

export type Rgb = [number, number, number];

export type ClimateSlot = "zone" | "winter";

/** Zone-slot list keys (map/climate.txt). Absence of all = temperate. */
export const ZONE_KEYS = ["tropical", "arid", "arctic", "impassable"] as const;
/** Winter-slot list keys. Absence of all = no winter. */
export const WINTER_KEYS = ["mild_winter", "normal_winter", "severe_winter"] as const;

/** Which slot a list key belongs to, or null if it is neither (e.g. monsoon). */
export function slotOfKey(key: string): ClimateSlot | null {
  if ((ZONE_KEYS as readonly string[]).includes(key)) return "zone";
  if ((WINTER_KEYS as readonly string[]).includes(key)) return "winter";
  return null;
}

// Colors mirror map_renderer.rs (climate/winter render arms) so the client-side
// recolor + winter tint match the backend render exactly.
export const ZONE_COLORS: Record<string, Rgb> = {
  tropical: [64, 142, 63],
  arid: [216, 196, 120],
  arctic: [235, 235, 238],
  impassable: [80, 80, 80],
};
/** The zone render color for a (possibly null = temperate) zone key. */
export const TEMPERATE_COLOR: Rgb = [126, 171, 97];
export function zoneColor(key: string | null): Rgb {
  return key ? (ZONE_COLORS[key] ?? TEMPERATE_COLOR) : TEMPERATE_COLOR;
}

export const WINTER_COLORS: Record<string, Rgb> = {
  mild_winter: [176, 206, 224],
  normal_winter: [116, 158, 204],
  severe_winter: [72, 92, 148],
};
/** The winter render color; non-winter land renders as plain land (LAND). */
export const WINTER_LAND: Rgb = [200, 200, 196];
export function winterColor(key: string | null): Rgb {
  return key ? (WINTER_COLORS[key] ?? WINTER_LAND) : WINTER_LAND;
}

/** Linear blend of two colors, `t` toward `b` (for the winter tint overlay). */
export function blend(a: Rgb, b: Rgb, t: number): Rgb {
  return [
    Math.round(a[0] + (b[0] - a[0]) * t),
    Math.round(a[1] + (b[1] - a[1]) * t),
    Math.round(a[2] + (b[2] - a[2]) * t),
  ];
}

// ── Wire types (mirror game_data.rs ClimatePayload; camelCase) ────────────────

export interface ClimateEntry {
  id: number;
  key: string;
}

export interface ClimatePayload {
  file: string;
  zones: ClimateEntry[];
  winters: ClimateEntry[];
  /** Top-level list keys present in climate.txt (create-when-absent guard). */
  existingLists: string[];
}

// ── Effective model (base + PENDING) ──────────────────────────────────────────

export interface ClimateModel {
  file: string;
  /** province id → zone key (subset of ZONE_KEYS); absent = temperate. */
  zone: Map<number, string>;
  /** province id → winter key; absent = no winter. */
  winter: Map<number, string>;
  /** List blocks that exist (base + queued creations). */
  existingLists: Set<string>;
}

function slotMap(model: ClimateModel, slot: ClimateSlot): Map<number, string> {
  return slot === "zone" ? model.zone : model.winter;
}

/** Folds the typed edit queue over the base payload → the effective model. */
export function foldClimate(base: ClimatePayload, edits: TypedEdit[]): ClimateModel {
  const zone = new Map<number, string>(base.zones.map((e) => [e.id, e.key]));
  const winter = new Map<number, string>(base.winters.map((e) => [e.id, e.key]));
  const existingLists = new Set(base.existingLists);
  const model: ClimateModel = { file: base.file, zone, winter, existingLists };

  const add = (listKey: string, idStr: string) => {
    const slot = slotOfKey(listKey);
    const n = parseInt(idStr, 10);
    if (!slot || !Number.isFinite(n)) return;
    slotMap(model, slot).set(n, listKey);
  };
  const remove = (listKey: string, idStr: string) => {
    const slot = slotOfKey(listKey);
    const n = parseInt(idStr, 10);
    if (!slot || !Number.isFinite(n)) return;
    const m = slotMap(model, slot);
    if (m.get(n) === listKey) m.delete(n);
  };

  for (const e of edits) {
    switch (e.kind) {
      case "addId":
        if (e.file === base.file && e.listPath.length === 1) add(e.listPath[0], e.id);
        break;
      case "removeId":
        if (e.file === base.file && e.listPath.length === 1) remove(e.listPath[0], e.id);
        break;
      case "listMove":
        if (e.fromFile === base.file && e.fromPath.length === 1) remove(e.fromPath[0], e.id);
        if (e.toFile === base.file && e.toPath.length === 1) add(e.toPath[0], e.id);
        break;
      case "insertStatement":
        if (e.file === base.file && e.blockPath.length === 0) {
          const eq = e.statement.indexOf("=");
          if (eq > 0) existingLists.add(e.statement.slice(0, eq).trim());
        }
        break;
    }
  }
  return model;
}

/** Effective list key for a province in one slot (absent = null). */
export function climateKey(model: ClimateModel, slot: ClimateSlot, id: number): string | null {
  return slotMap(model, slot).get(id) ?? null;
}

/** Per-list province counts for the selector (both slots). */
export function climateCounts(model: ClimateModel): Map<string, number> {
  const m = new Map<string, number>();
  for (const k of model.zone.values()) m.set(k, (m.get(k) ?? 0) + 1);
  for (const k of model.winter.values()) m.set(k, (m.get(k) ?? 0) + 1);
  return m;
}
