// Sprint 11.2 — Simple Terrain wire types (mirror game_data.rs, camelCase over
// IPC) + the effective-terrain fold shared by the list and the map recolor.

import type { TypedEdit } from "$lib/edits.svelte";
import type { KnownModifier, ModifierRow } from "$lib/components/ui";

export type Rgb = [number, number, number];

/** The list eraser entry: removes a province's terrain_override so it reverts to
 *  the terrain.bmp auto classification. Not a real terrain key. */
export const AUTO_KEY = "__auto__";

/** map/terrain.txt — the single file terrain_override lists live in. */
export const TERRAIN_FILE = "map/terrain.txt";

export interface TerrainCategory {
  key: string;
  name: string;
  color: Rgb;
  isWater: boolean;
  /** The category already has a terrain_override block (don't create a second). */
  hasOverrideBlock: boolean;
  movementCost: number | null;
  defence: number | null;
  localDefensiveness: number | null;
  localDevelopmentCost: number | null;
  supplyLimit: number | null;
  allowedNumOfBuildings: number | null;
  nationDesignerCostMultiplier: number | null;
}

export interface ProvinceTerrain {
  id: number;
  terrain: string;
  isOverride: boolean;
  autoTerrain: string | null;
  isWater: boolean;
}

export interface EffectiveTerrainPayload {
  provinces: ProvinceTerrain[];
  categories: TerrainCategory[];
}

/** A compact one-line summary of a category's gameplay modifiers (combat width
 *  has no per-category source in terrain.txt, so it is omitted — see game_data.rs). */
export function terrainModifierSummary(c: TerrainCategory): string {
  const parts: string[] = [];
  if (c.movementCost != null) parts.push(`Move ×${c.movementCost}`);
  if (c.defence != null && c.defence !== 0) parts.push(`Def ${c.defence > 0 ? "+" : ""}${c.defence}`);
  if (c.localDefensiveness != null && c.localDefensiveness !== 0)
    parts.push(`Fort ${pct(c.localDefensiveness)}`);
  if (c.localDevelopmentCost != null && c.localDevelopmentCost !== 0)
    parts.push(`Dev ${pct(c.localDevelopmentCost)}`);
  if (c.supplyLimit != null && c.supplyLimit !== 0) parts.push(`Supply ${sign(c.supplyLimit)}`);
  if (c.allowedNumOfBuildings != null && c.allowedNumOfBuildings !== 0)
    parts.push(`Bld ${sign(c.allowedNumOfBuildings)}`);
  return parts.join(" · ");
}

function pct(v: number): string {
  const p = Math.round(v * 100);
  return `${p > 0 ? "+" : ""}${p}%`;
}
function sign(v: number): string {
  return `${v > 0 ? "+" : ""}${v}`;
}

// ── Terrain modifier editing (S2.7) ───────────────────────────────────────────
//
// The seven modeled gameplay keys inside a `categories.<cat>` block, typed for
// the shared ModifierEditor (same component as ideas/religion). Kinds match how
// vanilla writes the value and how terrainModifierSummary reads it:
//   • local_defensiveness / local_development_cost are stored as FRACTIONS
//     (0.25, 0.1) shown as percent (percent kind keeps the file fraction and
//     only changes the display — no unit conversion in the file).
//   • everything else is a raw number (movement_cost 1.25, defence 2,
//     supply_limit 5, allowed_num_of_buildings 1, nation_designer_cost ×0.9).
// is_water / type / sound_type / AI keys are NOT modeled → preserve-unknown.
export const TERRAIN_MODIFIERS: KnownModifier[] = [
  { key: "movement_cost", label: "Movement cost", kind: "flat" },
  { key: "defence", label: "Defence", kind: "flat" },
  { key: "local_defensiveness", label: "Local defensiveness", kind: "percent" },
  { key: "local_development_cost", label: "Local development cost", kind: "percent" },
  { key: "supply_limit", label: "Supply limit", kind: "flat" },
  { key: "allowed_num_of_buildings", label: "Allowed buildings", kind: "flat" },
  { key: "nation_designer_cost_multiplier", label: "Nation designer cost ×", kind: "flat" },
];

const MODELED_KEYS = new Set(TERRAIN_MODIFIERS.map((m) => m.key));

/** The base on-disk value of a modeled modifier key on a category, or null. */
function baseValue(cat: TerrainCategory, key: string): number | null {
  switch (key) {
    case "movement_cost": return cat.movementCost;
    case "defence": return cat.defence;
    case "local_defensiveness": return cat.localDefensiveness;
    case "local_development_cost": return cat.localDevelopmentCost;
    case "supply_limit": return cat.supplyLimit;
    case "allowed_num_of_buildings": return cat.allowedNumOfBuildings;
    case "nation_designer_cost_multiplier": return cat.nationDesignerCostMultiplier;
    default: return null;
  }
}

/** Initial ModifierEditor rows for a category — its present modeled fields, in
 *  the canonical key order. Absent (null) fields yield no row. */
export function terrainModifierRows(cat: TerrainCategory): ModifierRow[] {
  const rows: ModifierRow[] = [];
  for (const m of TERRAIN_MODIFIERS) {
    const v = baseValue(cat, m.key);
    if (v != null) rows.push({ key: m.key, value: String(v) });
  }
  return rows;
}

/** Assigns a modeled numeric field (or clears it) on a category copy. */
function setField(cat: TerrainCategory, key: string, val: number | null): void {
  switch (key) {
    case "movement_cost": cat.movementCost = val; break;
    case "defence": cat.defence = val; break;
    case "local_defensiveness": cat.localDefensiveness = val; break;
    case "local_development_cost": cat.localDevelopmentCost = val; break;
    case "supply_limit": cat.supplyLimit = val; break;
    case "allowed_num_of_buildings": cat.allowedNumOfBuildings = val; break;
    case "nation_designer_cost_multiplier": cat.nationDesignerCostMultiplier = val; break;
  }
}

function numOrNull(s: string): number | null {
  const n = Number(s.trim());
  return Number.isFinite(n) ? n : null;
}

/**
 * Folds the queued modeled-modifier edits over a base category so the panel +
 * list show the EFFECTIVE (base + pending) values and a reselect re-seeds the
 * editor with pending intact. Only touches this category's modeled keys; the
 * commit still diffs against BASE so composites stay idempotent.
 */
export function foldTerrainModifiers(cat: TerrainCategory, edits: TypedEdit[]): TerrainCategory {
  const next = { ...cat };
  for (const e of edits) {
    if (
      e.kind === "setScalar" &&
      e.file === TERRAIN_FILE &&
      e.path.length === 3 &&
      e.path[0] === "categories" &&
      e.path[1] === cat.key &&
      MODELED_KEYS.has(e.path[2])
    ) {
      setField(next, e.path[2], numOrNull(e.value));
    } else if (
      e.kind === "insertStatement" &&
      e.file === TERRAIN_FILE &&
      e.blockPath.length === 2 &&
      e.blockPath[0] === "categories" &&
      e.blockPath[1] === cat.key
    ) {
      const eq = e.statement.indexOf("=");
      if (eq > 0) {
        const k = e.statement.slice(0, eq).trim();
        if (MODELED_KEYS.has(k)) setField(next, k, numOrNull(e.statement.slice(eq + 1)));
      }
    } else if (
      e.kind === "removeStatement" &&
      e.file === TERRAIN_FILE &&
      e.blockPath.length === 2 &&
      e.blockPath[0] === "categories" &&
      e.blockPath[1] === cat.key &&
      MODELED_KEYS.has(e.key)
    ) {
      setField(next, e.key, null);
    }
  }
  return next;
}

/**
 * Byte-surgical edits transforming a category's on-disk modeled-modifier state
 * into `rows` (the ModifierEditor's current set). Diffs against BASE each call
 * so the composite is idempotent from disk (safe to coalesce/replace):
 *   • absent → present ⇒ InsertStatement `key = value`
 *   • present → present, value changed (numerically) ⇒ SetScalar
 *   • present → present, unchanged ⇒ nothing (untouched line stays byte-identical,
 *     so "1.0" is never rewritten to "1")
 *   • present → absent ⇒ RemoveStatement
 * Only modeled keys plus any keys the user explicitly added are touched; all
 * other block content (type, sound_type, terrain_override, AI keys) is preserved.
 */
export function terrainModifierEdits(cat: TerrainCategory, rows: ModifierRow[]): TypedEdit[] {
  const blockPath = ["categories", cat.key];
  const desired = new Map(rows.map((r) => [r.key, r.value] as const));
  const keys = new Set<string>([...MODELED_KEYS, ...desired.keys()]);
  const edits: TypedEdit[] = [];
  for (const key of keys) {
    const want = desired.get(key);
    const baseNum = MODELED_KEYS.has(key) ? baseValue(cat, key) : null;
    const basePresent = baseNum != null;
    if (want === undefined) {
      if (basePresent) edits.push({ kind: "removeStatement", file: TERRAIN_FILE, blockPath, key });
      continue;
    }
    if (!basePresent) {
      edits.push({ kind: "insertStatement", file: TERRAIN_FILE, blockPath, statement: `${key} = ${want}` });
    } else {
      const wn = Number(want);
      if (!Number.isFinite(wn) || wn !== baseNum) {
        edits.push({ kind: "setScalar", file: TERRAIN_FILE, path: [...blockPath, key], value: want, quoted: false });
      }
    }
  }
  return edits;
}

// ── Effective override fold (base mode-data + queued terrain_override edits) ───
//
// terrain_override paints are AddId/RemoveId/ListMove on map/terrain.txt with a
// nested list path ["categories", <cat>, "terrain_override"] and the province id
// as the list element. Folding maps province id → the pending override category,
// or AUTO_KEY when a queued RemoveId reverted it to the bmp class.

/** Folds one queued edit into an override overlay (province id → cat | AUTO_KEY). */
export function foldTerrainEditInto(overlay: Map<number, string>, e: TypedEdit): void {
  const catOf = (path: string[]): string | null =>
    path.length === 3 && path[0] === "categories" && path[2] === "terrain_override" ? path[1] : null;
  if (e.kind === "addId" && e.file === TERRAIN_FILE) {
    const cat = catOf(e.listPath);
    const n = parseInt(e.id, 10);
    if (cat && Number.isFinite(n)) overlay.set(n, cat);
  } else if (e.kind === "removeId" && e.file === TERRAIN_FILE) {
    const cat = catOf(e.listPath);
    const n = parseInt(e.id, 10);
    if (cat && Number.isFinite(n)) overlay.set(n, AUTO_KEY);
  } else if (e.kind === "listMove") {
    const n = parseInt(e.id, 10);
    if (!Number.isFinite(n)) return;
    if (e.toFile === TERRAIN_FILE) {
      const to = catOf(e.toPath);
      if (to) overlay.set(n, to);
    } else if (e.fromFile === TERRAIN_FILE) {
      const from = catOf(e.fromPath);
      if (from) overlay.set(n, AUTO_KEY);
    }
  }
}
