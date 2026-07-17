// Sprint 23.2 — Mercenary companies wire types (mirror mercenary_companies.rs).
//
// Province-anchored via `home_province = <id>`. STATIC common files (no dates).

export const MERC_PROJECT_FILE = "common/mercenary_companies/zz_eutoolkit_mercenaries.txt";

export interface ModRow {
  key: string;
  value: string;
}

export interface Scalar {
  key: string;
  kind: string; // num | int | bool | str
  present: boolean;
  value: string;
}

export interface ModifierBlockRef {
  name: string;
  present: boolean;
  flat: boolean;
  rows: ModRow[];
}

export interface MercenaryCompany {
  key: string;
  file: string;
  origin: string;
  homeProvince: number;
  nameKey: string;
  nameLoc: string | null;
  scalars: Scalar[];
  sprites: string;
  spritesPresent: boolean;
  triggerPresent: boolean;
  modifier: ModifierBlockRef;
  rawExtra: string[];
  raw: string;
}

export interface ProvinceMercenaries {
  companies: MercenaryCompany[];
  projectFile: string;
}

export interface LocEntry {
  key: string;
  value: string;
}

export interface MercenaryScaffold {
  key: string;
  file: string;
  text: string;
  locEntries: LocEntry[];
}

const KEY_RE = /^[a-z][a-z0-9_]*$/;
export function isValidKey(key: string): boolean {
  return KEY_RE.test(key);
}
export function slugify(name: string): string {
  let s = name
    .toLowerCase()
    .normalize("NFKD")
    .replace(/[^a-z0-9]+/g, "_")
    .replace(/^_+|_+$/g, "");
  if (s && !s.startsWith("merc_")) s = `merc_${s}`;
  return s || "merc_company";
}

/** Serializes typed modifier rows to a block body (`k = v k = v …`). */
export function modBlockValue(rows: ModRow[]): string {
  return rows.map((r) => `${r.key} = ${r.value}`).join(" ");
}
