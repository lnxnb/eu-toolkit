// Sprint 19 — Colonial Regions & Trade Companies (framework-free core).
//
// Mirrors the `get_colonial_data` wire payload (colonial.rs), folds the typed
// edit queue over the base so membership steals, create/delete, color, weight,
// native/tax, and naming-rule edits appear live (and undo/redo revert them), and
// builds the province→entry membership index the map recolor + hit-testing read.
//
// Both modes share this module — they differ only in `dir` (which directory the
// entries live in) and whether weight tables / native-tax scalars exist
// (`has_weight_tables`). Province membership lives in a nested `provinces = { }`
// list, so the id-list edits key on `[entryKey, "provinces"]`.

import type { TypedEdit } from "$lib/edits.svelte";

export type Rgb = [number, number, number];

// ── Wire types (mirror colonial.rs; serialize snake_case) ─────────────────────

export interface NamingRule {
  index: number;
  name_key: string;
  name: string;
  has_trigger: boolean;
  raw: string;
}

export interface WeightRow {
  key: string;
  weight: number;
}

export interface ColonialEntry {
  key: string;
  name: string;
  color: Rgb;
  has_color: boolean;
  provinces: number[];
  names: NamingRule[];
  tax_income: number | null;
  native_size: number | null;
  native_ferocity: number | null;
  native_hostileness: number | null;
  trade_goods: WeightRow[];
  culture: WeightRow[];
  religion: WeightRow[];
  raw_extra: string[];
  source_file: string;
}

export interface ColonialData {
  kind: string;
  dir: string;
  project_file: string;
  has_weight_tables: boolean;
  entries: ColonialEntry[];
}

// ── Key helpers ───────────────────────────────────────────────────────────────

/** A safe lowercase snake_case key from a display name. */
export function slugify(name: string): string {
  const s = name
    .toLowerCase()
    .normalize("NFKD")
    .replace(/[^a-z0-9]+/g, "_")
    .replace(/^_+|_+$/g, "");
  return s || "unnamed";
}

export function uniqueKey(base: string, exists: (k: string) => boolean): string {
  if (!exists(base)) return base;
  for (let i = 2; ; i++) {
    const k = `${base}_${i}`;
    if (!exists(k)) return k;
  }
}

/** The loc key a new naming rule / entry name uses (UPPER_SNAKE + suffix). */
export function nameLocKey(entryKey: string, suffix = "Name"): string {
  return `${entryKey.toUpperCase()}_${suffix}`;
}

/** FNV-ish hash → rgb, matching map_renderer::hash_color closely enough for a
 *  placeholder swatch on a just-created (not-yet-on-disk) entity. */
export function hashColor(key: string): Rgb {
  let h = 2166136261 >>> 0;
  for (let i = 0; i < key.length; i++) {
    h ^= key.charCodeAt(i);
    h = Math.imul(h, 16777619) >>> 0;
  }
  return [80 + (h & 0x7f), 80 + ((h >> 8) & 0x7f), 80 + ((h >> 16) & 0x7f)];
}

// ── Statement parse helpers ───────────────────────────────────────────────────

function statementKey(s: string): string {
  const eq = s.indexOf("=");
  return eq < 0 ? "" : s.slice(0, eq).trim();
}
function statementValue(s: string): string {
  const eq = s.indexOf("=");
  return eq < 0 ? "" : s.slice(eq + 1).trim();
}
/** 0-based occurrence in a `key#n` path segment (bare key ⇒ 0). */
export function occurrenceOf(seg: string): number {
  const h = seg.indexOf("#");
  if (h < 0) return 0;
  const n = parseInt(seg.slice(h + 1), 10);
  return Number.isFinite(n) ? n : 0;
}
function parseColor(v: string): Rgb | null {
  const nums = v.trim().replace(/[{}]/g, " ").trim().split(/\s+/).map((t) => parseInt(t, 10));
  if (nums.length >= 3 && nums.every((n) => Number.isFinite(n))) {
    return [nums[0], nums[1], nums[2]] as Rgb;
  }
  return null;
}
/** Extracts the `name = "KEY"` loc key from a raw `names { … }` body/statement. */
function nameKeyOf(raw: string): string {
  const m = /name\s*=\s*"?([A-Za-z0-9_.]+)"?/.exec(raw);
  return m ? m[1] : "";
}

// ── Scaffold parsing (fold a newly-created entry into the effective data) ──────

/** Parses a scaffold block (`key = { color … provinces { ids } names {…} }`). */
export function parseScaffold(text: string, hasWeightTables: boolean): ColonialEntry | null {
  const key = /^\s*([A-Za-z0-9_]+)\s*=/.exec(text)?.[1];
  if (!key) return null;
  const colorM = /color\s*=\s*\{([^}]*)\}/.exec(text);
  const provM = /provinces\s*=\s*\{([^}]*)\}/.exec(text);
  const provinces = provM
    ? provM[1].trim().split(/\s+/).map((t) => parseInt(t, 10)).filter((n) => Number.isFinite(n))
    : [];
  const names: NamingRule[] = [];
  // Each `names = { … }` (scaffolds carry exactly one simple `name = "KEY"`).
  const re = /names\s*=\s*\{([\s\S]*?)\}/g;
  let m: RegExpExecArray | null;
  let idx = 0;
  while ((m = re.exec(text)) !== null) {
    const nk = nameKeyOf(m[1]);
    names.push({ index: idx, name_key: nk, name: nk, has_trigger: /trigger\s*=/.test(m[1]), raw: `names = {${m[1]}}` });
    idx++;
  }
  const color = colorM ? (parseColor(colorM[1]) ?? hashColor(key)) : hashColor(key);
  return {
    key,
    name: names[0]?.name || key,
    color,
    has_color: colorM != null,
    provinces,
    names,
    tax_income: hasWeightTables ? 0 : null,
    native_size: hasWeightTables ? 0 : null,
    native_ferocity: hasWeightTables ? 0 : null,
    native_hostileness: hasWeightTables ? 0 : null,
    trade_goods: [],
    culture: [],
    religion: [],
    raw_extra: [],
    source_file: "",
  };
}

// ── Effective data (base + PENDING) ───────────────────────────────────────────

function cloneEntry(e: ColonialEntry): ColonialEntry {
  return {
    ...e,
    color: [...e.color] as Rgb,
    provinces: e.provinces.slice(),
    names: e.names.map((n) => ({ ...n })),
    trade_goods: e.trade_goods.map((w) => ({ ...w })),
    culture: e.culture.map((w) => ({ ...w })),
    religion: e.religion.map((w) => ({ ...w })),
    raw_extra: e.raw_extra.slice(),
  };
}

const WEIGHT_TABLES = new Set(["trade_goods", "culture", "religion"]);

/** Folds the typed edit queue over `base`, returning the effective data. */
export function foldColonial(base: ColonialData, edits: TypedEdit[]): ColonialData {
  const entries = base.entries.map(cloneEntry);
  const byKey = new Map(entries.map((e) => [e.key, e]));
  const inDir = (file: string) => file.startsWith(base.dir + "/") || file === base.project_file;

  const setScalarField = (e: ColonialEntry, field: string, v: number | null) => {
    switch (field) {
      case "tax_income": e.tax_income = v; break;
      case "native_size": e.native_size = v; break;
      case "native_ferocity": e.native_ferocity = v; break;
      case "native_hostileness": e.native_hostileness = v; break;
    }
  };
  const table = (e: ColonialEntry, name: string): WeightRow[] =>
    name === "trade_goods" ? e.trade_goods : name === "culture" ? e.culture : e.religion;

  for (const e of edits) {
    switch (e.kind) {
      case "appendText":
      case "createFile": {
        if (!inDir(e.file)) break;
        const ent = parseScaffold(e.text, base.has_weight_tables);
        if (ent && !byKey.has(ent.key)) {
          ent.source_file = e.file;
          entries.push(ent);
          byKey.set(ent.key, ent);
        }
        break;
      }
      case "removeStatement": {
        if (!inDir(e.file)) break;
        // Whole-entry delete (top-level block).
        if (e.blockPath.length === 0 && byKey.has(e.key)) {
          const i = entries.findIndex((x) => x.key === e.key);
          if (i >= 0) entries.splice(i, 1);
          byKey.delete(e.key);
          break;
        }
        if (e.blockPath.length === 1) {
          const ent = byKey.get(e.blockPath[0]);
          if (!ent) break;
          // Remove naming rule (names#n).
          if (e.key === "names" || e.key.startsWith("names#")) {
            const idx = occurrenceOf(e.key);
            if (idx >= 0 && idx < ent.names.length) {
              ent.names.splice(idx, 1);
              ent.names.forEach((n, i) => (n.index = i));
            }
          } else {
            setScalarField(ent, e.key, null);
          }
        } else if (e.blockPath.length === 2 && WEIGHT_TABLES.has(e.blockPath[1])) {
          const ent = byKey.get(e.blockPath[0]);
          if (ent) {
            const t = table(ent, e.blockPath[1]);
            const i = t.findIndex((w) => w.key === e.key);
            if (i >= 0) t.splice(i, 1);
          }
        } else if (
          e.blockPath.length === 2 &&
          (e.blockPath[1] === "names" || e.blockPath[1].startsWith("names#")) &&
          e.key === "trigger"
        ) {
          // Remove a naming rule's condition → it becomes an unconditional rule.
          const ent = byKey.get(e.blockPath[0]);
          const idx = occurrenceOf(e.blockPath[1]);
          if (ent && idx >= 0 && idx < ent.names.length) ent.names[idx].has_trigger = false;
        }
        break;
      }
      case "insertStatement": {
        if (!inDir(e.file)) break;
        if (e.blockPath.length === 1) {
          const ent = byKey.get(e.blockPath[0]);
          if (!ent) break;
          const k = statementKey(e.statement);
          if (k === "names") {
            const nk = nameKeyOf(e.statement);
            ent.names.push({
              index: ent.names.length,
              name_key: nk,
              name: nk,
              has_trigger: /trigger\s*=/.test(e.statement),
              raw: e.statement,
            });
          } else {
            const num = parseFloat(statementValue(e.statement));
            if (Number.isFinite(num)) setScalarField(ent, k, num);
          }
        } else if (e.blockPath.length === 2 && WEIGHT_TABLES.has(e.blockPath[1])) {
          const ent = byKey.get(e.blockPath[0]);
          if (ent) {
            const t = table(ent, e.blockPath[1]);
            const k = statementKey(e.statement);
            const w = parseFloat(statementValue(e.statement));
            if (k && Number.isFinite(w) && !t.some((r) => r.key === k)) t.push({ key: k, weight: w });
          }
        } else if (
          e.blockPath.length === 2 &&
          (e.blockPath[1] === "names" || e.blockPath[1].startsWith("names#")) &&
          statementKey(e.statement) === "trigger"
        ) {
          // Add a condition to an unconditional naming rule.
          const ent = byKey.get(e.blockPath[0]);
          const idx = occurrenceOf(e.blockPath[1]);
          if (ent && idx >= 0 && idx < ent.names.length) ent.names[idx].has_trigger = true;
        }
        break;
      }
      case "setScalar": {
        if (!inDir(e.file)) break;
        if (e.path.length === 2) {
          const ent = byKey.get(e.path[0]);
          const num = parseFloat(e.value);
          if (ent && Number.isFinite(num)) setScalarField(ent, e.path[1], num);
        } else if (e.path.length === 3 && WEIGHT_TABLES.has(e.path[1])) {
          const ent = byKey.get(e.path[0]);
          const w = parseFloat(e.value);
          if (ent && Number.isFinite(w)) {
            const t = table(ent, e.path[1]);
            const row = t.find((r) => r.key === e.path[2]);
            if (row) row.weight = w;
          }
        }
        break;
      }
      case "setBlock": {
        if (!inDir(e.file) || e.path.length !== 2) break;
        const ent = byKey.get(e.path[0]);
        if (!ent) break;
        if (e.path[1] === "color") {
          const c = parseColor(e.value);
          if (c) {
            ent.color = c;
            ent.has_color = true;
          }
        } else if (e.path[1] === "names" || e.path[1].startsWith("names#")) {
          // Reorder swap: a rule's body was replaced with another's.
          const idx = occurrenceOf(e.path[1]);
          if (idx >= 0 && idx < ent.names.length) {
            const nk = nameKeyOf(e.value);
            ent.names[idx] = {
              index: idx,
              name_key: nk,
              name: nk,
              has_trigger: /trigger\s*=/.test(e.value),
              raw: `names = { ${e.value} }`,
            };
          }
        }
        break;
      }
      case "addId": {
        if (inDir(e.file) && e.listPath.length === 2 && e.listPath[1] === "provinces") {
          const ent = byKey.get(e.listPath[0]);
          const n = parseInt(e.id, 10);
          if (ent && Number.isFinite(n) && !ent.provinces.includes(n)) ent.provinces.push(n);
        }
        break;
      }
      case "removeId": {
        if (inDir(e.file) && e.listPath.length === 2 && e.listPath[1] === "provinces") {
          const ent = byKey.get(e.listPath[0]);
          const n = parseInt(e.id, 10);
          if (ent) {
            const i = ent.provinces.indexOf(n);
            if (i >= 0) ent.provinces.splice(i, 1);
          }
        }
        break;
      }
      case "listMove": {
        const n = parseInt(e.id, 10);
        if (inDir(e.fromFile) && e.fromPath.length === 2 && e.fromPath[1] === "provinces") {
          const ent = byKey.get(e.fromPath[0]);
          if (ent) {
            const i = ent.provinces.indexOf(n);
            if (i >= 0) ent.provinces.splice(i, 1);
          }
        }
        if (inDir(e.toFile) && e.toPath.length === 2 && e.toPath[1] === "provinces") {
          const ent = byKey.get(e.toPath[0]);
          if (ent && Number.isFinite(n) && !ent.provinces.includes(n)) ent.provinces.push(n);
        }
        break;
      }
    }
  }

  return { ...base, entries };
}

// ── Membership index (province id → entry key) ────────────────────────────────

export function membershipIndex(data: ColonialData): Map<number, string> {
  const m = new Map<number, string>();
  for (const e of data.entries) for (const id of e.provinces) if (!m.has(id)) m.set(id, e.key);
  return m;
}
