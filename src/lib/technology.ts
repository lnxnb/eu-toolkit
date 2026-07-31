// Sprint 22 — Technology & units (View ▸ Technology).
//
// Mirrors the get_technologies / get_units wire payloads (technology.rs) and
// folds the typed edit queue so pending edits (level years/modifiers, group
// steppers, unit pips, created units) show live. Per-field edits are read at
// render time through the queue's pending helpers; created units are folded here.

import type { EditQueue, TypedEdit } from "$lib/edits.svelte";

export const UNITS_DIR = "common/units";
export const TECH_GROUPS_FILE = "common/technology.txt";

// ── Wire types (mirror technology.rs) ────────────────────────────────────────

export interface TechRow {
  key: string;
  value: string;
  kind: string; // "modifier" | "unlock" | "unit"
  label: string;
  /** Set by the fold for a row that exists only as a pending insert. */
  pending?: boolean;
}
export interface TechLevel {
  index: number;
  file: string;
  /** In-game name (`<kind>_tech_cs_<index>_name`); null past vanilla's levels. */
  name: string | null;
  /** Flavor text (`<kind>_tech_cs_<index>_desc`). */
  desc: string | null;
  year: string | null;
  modifiers: TechRow[];
  unlocks: TechRow[];
  units: TechRow[];
  rawExtra: string[];
}
export interface TechTable {
  kind: string; // "adm" | "dip" | "mil"
  label: string;
  file: string;
  monarchPower: string;
  levels: TechLevel[];
}
export interface TechGroup {
  key: string;
  name: string;
  file: string;
  startLevel: string | null;
  startCostModifier: string | null;
  rawExtra: string[];
}
export interface PipStat {
  key: string;
  value: string;
  present: boolean;
}
export interface Unit {
  key: string;
  file: string;
  origin: string; // "base" | "mod"
  name: string;
  category: string;
  unitType: string | null;
  isLand: boolean;
  pips: PipStat[];
  totalPips: number | null;
  arrivesTech: string | null;
  arrivesLevel: number | null;
  rawExtra: string[];
}
export interface TechData {
  tables: TechTable[];
  groups: TechGroup[];
}
export interface LocEntry {
  key: string;
  value: string;
}
export interface UnitScaffold {
  key: string;
  file: string;
  text: string;
  locEntries: LocEntry[];
}

export const LAND_CATEGORIES = ["infantry", "cavalry", "artillery"];
export const SHIP_CATEGORIES = ["galley", "heavy_ship", "light_ship", "transport"];

// ── Key helpers ───────────────────────────────────────────────────────────────

const KEY_RE = /^[a-z][a-z0-9_]*$/;
export function isValidKey(key: string): boolean {
  return KEY_RE.test(key);
}
export function slugify(name: string): string {
  const base = name
    .toLowerCase()
    .normalize("NFKD")
    .replace(/[^a-z0-9]+/g, "_")
    .replace(/^_+|_+$/g, "");
  return base || "unit";
}

// ── Created-unit fold ─────────────────────────────────────────────────────────

/** Parses a scaffolded unit file body into a minimal Unit. */
function parseScaffoldUnit(text: string, file: string, edits: TypedEdit[]): Unit | null {
  const keyM = /([A-Za-z0-9_]+)\.txt$/.exec(file);
  const key = keyM ? keyM[1] : null;
  if (!key) return null;
  const scalar = (name: string): string | null => {
    const m = new RegExp(`\\b${name}\\s*=\\s*("?[^\\s"{}]+"?)`).exec(text);
    return m ? m[1].replace(/"/g, "") : null;
  };
  const category = scalar("type") ?? "";
  const isLand = LAND_CATEGORIES.includes(category);
  const stats = isLand
    ? ["maneuver", "offensive_morale", "defensive_morale", "offensive_fire", "defensive_fire", "offensive_shock", "defensive_shock"]
    : ["hull_size", "base_cannons", "blockade", "sail_speed", "sailors", "trade_power"];
  const pips: PipStat[] = [];
  for (const s of stats) {
    const v = scalar(s);
    if (v != null || isLand) pips.push({ key: s, value: v ?? "0", present: v != null });
  }
  const total = isLand
    ? pips.reduce((a, p) => a + (Number(p.value) || 0), 0)
    : null;
  // Arrival: find the enable registration for this key in the queue.
  let arrivesTech: string | null = null;
  let arrivesLevel: number | null = null;
  for (const e of edits) {
    if (e.kind === "insertStatement" && e.statement.replace(/\s/g, "") === `enable=${key}` && e.blockPath.length === 1) {
      const m = /^technology#(\d+)$/.exec(e.blockPath[0]);
      if (m) {
        arrivesLevel = Number(m[1]);
        arrivesTech = e.file.includes("dip") ? "dip" : e.file.includes("adm") ? "adm" : "mil";
      }
    }
  }
  return {
    key,
    file,
    origin: "mod",
    name: key,
    category,
    unitType: scalar("unit_type"),
    isLand,
    pips,
    totalPips: total,
    arrivesTech,
    arrivesLevel,
    rawExtra: [],
  };
}

/** Folds pending unit creations onto the base unit list. */
export function foldUnits(base: Unit[], edits: TypedEdit[]): Unit[] {
  const units = base.slice();
  for (const e of edits) {
    if (e.kind === "createFile" && e.file.startsWith(UNITS_DIR + "/")) {
      const obj = parseScaffoldUnit(e.text, e.file, edits);
      if (obj && !units.some((u) => u.key === obj.key)) units.push(obj);
    }
  }
  units.sort((a, b) => a.key.localeCompare(b.key));
  return units;
}

/** All existing unit keys (for uniqueness). */
export function allUnitKeys(units: Unit[]): Set<string> {
  return new Set(units.map((u) => u.key));
}

// ── Pending value helpers (queue-aware live values) ───────────────────────────

/** Live value of a level scalar (`year` or a modifier key). */
export function liveLevelScalar(queue: EditQueue, level: TechLevel, key: string, base: string): string {
  queue.version;
  return queue.pendingScalar(level.file, [`technology#${level.index}`, key]) ?? base;
}

// ── Modifier rows: add / edit / remove fold ──────────────────────────────────
//
// A tech level's modifiers are plain `key = <number>` rows inside its
// `technology#<n>` block, so they use the per-row edit vocabulary rather than a
// whole-block rewrite: editing one is a `setScalar`, adding one an
// `insertStatement`, removing one a `removeStatement`. A whole-block `setBlock`
// would drop the level's sibling sub-blocks (`expects_institution`, `effect`),
// which are preserve-unknown.

/** Statement text for a modifier row, matching the file's `key = value` shape. */
export function modifierStatement(key: string, value: string): string {
  return `${key} = ${value.trim() || "0"}`;
}

/** The key a statement assigns, or null when it isn't a simple assignment. */
function statementKey(statement: string): string | null {
  const m = /^\s*([A-Za-z0-9_]+)\s*=/.exec(statement);
  return m ? m[1] : null;
}

/**
 * The level's modifier rows as they would be after the pending edits: values
 * overridden by `setScalar`, rows added by `insertStatement`, rows dropped by
 * `removeStatement`. Pure over a flat edit list so it is directly testable.
 */
export function foldLevelModifiers(
  base: TechRow[],
  file: string,
  index: number,
  edits: TypedEdit[],
): TechRow[] {
  const blockKey = `technology#${index}`;
  const inBlock = (path: string[]) => path.length >= 1 && path[0] === blockKey;
  const rows = base.map((r) => ({ ...r }));

  for (const e of edits) {
    if (e.kind === "setScalar" && e.file === file && inBlock(e.path) && e.path.length === 2) {
      const row = rows.find((r) => r.key === e.path[1]);
      if (row) row.value = e.value;
    } else if (e.kind === "insertStatement" && e.file === file && inBlock(e.blockPath)) {
      const key = statementKey(e.statement);
      // `enable = <unit>` inserts share this block path; only numeric rows are
      // modifiers.
      const value = key ? e.statement.slice(e.statement.indexOf("=") + 1).trim() : "";
      if (key && key !== "enable" && value !== "" && Number.isFinite(Number(value))) {
        const existing = rows.find((r) => r.key === key);
        if (existing) existing.value = value;
        else rows.push({ key, value, kind: "modifier", label: key, pending: true });
      }
    } else if (e.kind === "removeStatement" && e.file === file && inBlock(e.blockPath)) {
      const at = rows.findIndex((r) => r.key === e.key);
      if (at >= 0) rows.splice(at, 1);
    }
  }
  return rows;
}

/** Queue-aware wrapper of {@link foldLevelModifiers}. */
export function liveLevelModifiers(queue: EditQueue, level: TechLevel): TechRow[] {
  queue.version;
  return foldLevelModifiers(level.modifiers, level.file, level.index, queue.serialize());
}

// ── Level title / flavor text ────────────────────────────────────────────────
//
// A tech level has no name in script — the displayed name and flavor text are
// loc keys indexed by LEVEL, so editing them is a plain `locOverride` and the
// technologies file is never touched.

export function levelNameKey(kind: string, index: number): string {
  return `${kind}_tech_cs_${index}_name`;
}
export function levelDescKey(kind: string, index: number): string {
  return `${kind}_tech_cs_${index}_desc`;
}

/** Live loc value: a pending override wins over what the loc files resolved to. */
export function liveLevelLoc(queue: EditQueue, key: string, base: string | null): string {
  queue.version;
  return queue.pendingLocOverride(key) ?? base ?? "";
}

// ── Level deletion (and its index-drift hazard) ──────────────────────────────
//
// Levels are addressed by OCCURRENCE INDEX (`technology#<n>`), so deleting one
// renumbers every later level in the same file. Two consequences, both real:
//
//  1. Any other pending edit in the session that addresses a higher index would
//     land on the wrong block once the delete applies (all of a file's edits
//     compose in queue order on one evolving buffer). We therefore refuse to
//     queue a delete while the file has other pending level edits, and freeze
//     level editing for that file while a delete is pending. Save in between.
//  2. In the GAME, the level's name/flavor loc keys are index-based too, so every
//     later level's displayed name shifts down by one. That is EU4's own model,
//     not something the toolkit can paper over — the UI warns before deleting.

/** Statement path of a level, for occurrence-addressed edits. */
export function levelBlockKey(index: number): string {
  return `technology#${index}`;
}

/** The level index a `removeStatement` targets, or null if it isn't one. */
function deletedLevelIndex(e: TypedEdit, file: string): number | null {
  if (e.kind !== "removeStatement" || e.file !== file) return null;
  if (e.blockPath.length !== 0) return null;
  const m = /^technology#(\d+)$/.exec(e.key);
  return m ? Number(m[1]) : null;
}

/** Level indices with a pending deletion in `file`, in queue order. */
export function pendingLevelDeletes(file: string, edits: TypedEdit[]): number[] {
  const out: number[] = [];
  for (const e of edits) {
    const i = deletedLevelIndex(e, file);
    if (i != null) out.push(i);
  }
  return out;
}

/** True when any pending edit addresses a `technology#<n>` block of `file`. */
export function hasPendingLevelEdits(file: string, edits: TypedEdit[]): boolean {
  const isLevelPath = (p: string[]) => p.length > 0 && /^technology#\d+$/.test(p[0]);
  return edits.some((e) => {
    if (e.kind === "setScalar") return e.file === file && isLevelPath(e.path);
    if (e.kind === "insertStatement" || e.kind === "removeStatement")
      return e.file === file && isLevelPath(e.blockPath);
    if (e.kind === "appendText") return e.file === file;
    return deletedLevelIndex(e, file) != null;
  });
}

/** Drops levels with a pending deletion, so the list matches what will be saved. */
export function foldLevelDeletes(levels: TechLevel[], file: string, edits: TypedEdit[]): TechLevel[] {
  const gone = new Set(pendingLevelDeletes(file, edits));
  if (!gone.size) return levels;
  return levels.filter((l) => !(l.file === file && gone.has(l.index)));
}

/** Live value of a tech-group scalar. */
export function liveGroupScalar(queue: EditQueue, group: TechGroup, key: string, base: string): string {
  queue.version;
  return queue.pendingScalar(group.file, ["groups", group.key, key]) ?? base;
}

/** Live value of a unit stat (file-level scalar). */
export function liveUnitStat(queue: EditQueue, unit: Unit, key: string, base: string): string {
  queue.version;
  return queue.pendingScalar(unit.file, [key]) ?? base;
}
