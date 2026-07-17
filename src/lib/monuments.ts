// Sprint 23.1 — Great projects (monuments) wire types (mirror great_projects.rs).
//
// Province-anchored: a project's `start = <province id>` decides which province
// panel hosts it. These are STATIC common files (no date threading).

export const GP_PROJECT_FILE = "common/great_projects/zz_eutoolkit_great_projects.txt";

export interface ModRow {
  key: string;
  value: string;
}

export interface Scalar {
  key: string;
  /** Path relative to the entry block, e.g. ["build_cost"], ["time","months"],
   *  ["tier_1","cost_to_upgrade","factor"]. */
  path: string[];
  kind: string; // num | int | bool | str | enum
  present: boolean;
  value: string;
  options: string[];
}

export interface ModifierBlockRef {
  name: string;
  path: string[];
  present: boolean;
  flat: boolean;
  rows: ModRow[];
}

export interface ScriptBlockRef {
  name: string;
  path: string[];
  registry: string; // triggers | effects
  present: boolean;
}

export interface Tier {
  index: number;
  present: boolean;
  scalars: Scalar[];
  modifierBlocks: ModifierBlockRef[];
  scriptBlocks: ScriptBlockRef[];
  rawExtra: string[];
}

export interface GreatProject {
  key: string;
  file: string;
  origin: string;
  start: number;
  projectType: string;
  nameKey: string;
  nameLoc: string | null;
  descKey: string;
  descLoc: string | null;
  scalars: Scalar[];
  scriptBlocks: ScriptBlockRef[];
  tiers: Tier[];
  sprite: string | null;
  rawExtra: string[];
  raw: string;
}

export interface ProvinceMonuments {
  monuments: GreatProject[];
  projectFile: string;
}

export interface MonumentBrief {
  key: string;
  name: string;
  start: number;
  projectType: string;
  sprite: string | null;
}

export interface LocEntry {
  key: string;
  value: string;
}

export interface GreatProjectScaffold {
  key: string;
  file: string;
  text: string;
  gfxFile: string;
  gfxText: string;
  sourceSprite: string | null;
  locEntries: LocEntry[];
}

const KEY_RE = /^[a-z][a-z0-9_]*$/;
export function isValidKey(key: string): boolean {
  return KEY_RE.test(key);
}
export function slugify(name: string): string {
  const s = name
    .toLowerCase()
    .normalize("NFKD")
    .replace(/[^a-z0-9]+/g, "_")
    .replace(/^_+|_+$/g, "");
  return s || "key";
}

/** Serializes typed modifier rows to a block body (`k = v k = v …`). */
export function modBlockValue(rows: ModRow[]): string {
  return rows.map((r) => `${r.key} = ${r.value}`).join(" ");
}
