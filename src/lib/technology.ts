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
}
export interface TechLevel {
  index: number;
  file: string;
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
