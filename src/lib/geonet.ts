// Sprint 10 — areas & regions geography network (framework-free core).
//
// Holds the wire types of `get_geo_network` (mirrors geography.rs), folds the
// typed edit queue over the base payload so province/area/region membership
// steals, create/delete, and superregion moves appear live and undo/redo revert
// them, and builds the province→area / province→region membership indices the
// map recolor + hit-testing read. Areas contain provinces; regions contain
// areas; superregions contain regions — so the two map modes edit at different
// granularities (see geography.rs edit recipes).

import type { TypedEdit } from "$lib/edits.svelte";

export type Rgb = [number, number, number];

// ── Wire types (mirror geography.rs; serialize snake_case) ────────────────────

export interface GeoArea {
  key: string;
  name: string;
  /** Explicit `color = { r g b }` from area.txt, if any. */
  color: Rgb | null;
  /** Toolkit hash color — what the map render/highlight actually uses. */
  hash_color: Rgb;
  provinces: number[];
  /** Parent region key (rollup), or null. */
  region: string | null;
  source_file: string;
}

/** One `monsoon = { start end }` block — dates are the game's `YY.MM.DD` with a
 *  `00` year (season is year-agnostic). Mirrors geography.rs `MonsoonRange`. */
export interface MonsoonRange {
  start: string;
  end: string;
}

export interface GeoRegion {
  key: string;
  name: string;
  hash_color: Rgb;
  /** Member area keys, in file order. */
  areas: string[];
  superregion: string | null;
  has_monsoon: boolean;
  /** The region's monsoon blocks, in file order (S2.6 editable). */
  monsoon: MonsoonRange[];
  raw_extra: string[];
  source_file: string;
}

export interface GeoSuperregion {
  key: string;
  name: string;
  regions: string[];
  source_file: string;
}

export interface GeoNetwork {
  areas: GeoArea[];
  regions: GeoRegion[];
  superregions: GeoSuperregion[];
  area_file: string;
  region_file: string;
  superregion_file: string;
}

// ── Key helpers ───────────────────────────────────────────────────────────────

/** A safe lowercase snake_case key from a display name (Latin letters/digits). */
export function slugify(name: string): string {
  const s = name
    .toLowerCase()
    .normalize("NFKD")
    .replace(/[^a-z0-9]+/g, "_")
    .replace(/^_+|_+$/g, "");
  return s || "unnamed";
}

/** A key not already used by any area/region, suffixing _2, _3, … if needed. */
export function uniqueKey(base: string, exists: (k: string) => boolean): string {
  if (!exists(base)) return base;
  for (let i = 2; ; i++) {
    const k = `${base}_${i}`;
    if (!exists(k)) return k;
  }
}

// ── Scaffold parsing (fold newly-created areas/regions into the effective net) ─

function parseIds(s: string): number[] {
  return s
    .trim()
    .split(/\s+/)
    .map((t) => parseInt(t, 10))
    .filter((n) => Number.isFinite(n));
}

function parseNames(s: string): string[] {
  return s.trim().split(/\s+/).filter((t) => t.length > 0);
}

/** Parses a `scaffold_area` block (`key = { <ids> }`) into a GeoArea, or null. */
export function parseAreaScaffold(text: string, hash: Rgb): GeoArea | null {
  const key = /^\s*([A-Za-z0-9_]+)\s*=/.exec(text)?.[1];
  if (!key) return null;
  const body = text.slice(text.indexOf("{") + 1, text.lastIndexOf("}"));
  return {
    key,
    name: key,
    color: null,
    hash_color: hash,
    provinces: parseIds(body),
    region: null,
    source_file: "",
  };
}

/** Parses a `scaffold_superregion` block (`key = { <region> }`) into a
 *  GeoSuperregion, or null (S3.1). Bare scalars are region names. */
export function parseSuperregionScaffold(text: string): GeoSuperregion | null {
  const key = /^\s*([A-Za-z0-9_]+)\s*=/.exec(text)?.[1];
  if (!key) return null;
  const body = text.slice(text.indexOf("{") + 1, text.lastIndexOf("}"));
  return {
    key,
    name: key,
    regions: parseNames(body),
    source_file: "",
  };
}

/** Parses a `scaffold_region` block into a GeoRegion, or null. */
export function parseRegionScaffold(text: string, hash: Rgb): GeoRegion | null {
  const key = /^\s*([A-Za-z0-9_]+)\s*=/.exec(text)?.[1];
  if (!key) return null;
  const areasBlock = /areas\s*=\s*\{([\s\S]*?)\}/.exec(text);
  return {
    key,
    name: key,
    hash_color: hash,
    areas: areasBlock ? parseNames(areasBlock[1]) : [],
    superregion: null,
    has_monsoon: false,
    monsoon: [],
    raw_extra: [],
    source_file: "",
  };
}

// ── Monsoon edit parsing (S2.6) ──────────────────────────────────────────────

/** Two whitespace-separated dates → a MonsoonRange, or null. Used for both a
 *  setBlock value ("00.10.01 00.12.15") and an inserted block's inner tokens. */
export function parseMonsoonDates(inner: string): MonsoonRange | null {
  const toks = inner.trim().split(/\s+/).filter(Boolean);
  if (toks.length < 2) return null;
  return { start: toks[0], end: toks[1] };
}

/** Parses an inserted `monsoon = { start end }` statement into a range, or null. */
export function parseMonsoonStatement(statement: string): MonsoonRange | null {
  const b = statement.indexOf("{");
  const e = statement.lastIndexOf("}");
  if (b < 0 || e < 0 || e < b) return null;
  return parseMonsoonDates(statement.slice(b + 1, e));
}

/** Key of a `key = value` statement string, or "" when malformed. */
function statementKeyOf(statement: string): string {
  const eq = statement.indexOf("=");
  return eq < 0 ? "" : statement.slice(0, eq).trim();
}

/** The 0-based occurrence index in a `key#n` path segment (bare key ⇒ 0). */
export function occurrenceOf(seg: string): number {
  const h = seg.indexOf("#");
  if (h < 0) return 0;
  const n = parseInt(seg.slice(h + 1), 10);
  return Number.isFinite(n) ? n : 0;
}

function cloneArea(a: GeoArea): GeoArea {
  return { ...a, color: a.color ? ([...a.color] as Rgb) : null, hash_color: [...a.hash_color] as Rgb, provinces: a.provinces.slice() };
}
function cloneRegion(r: GeoRegion): GeoRegion {
  return {
    ...r,
    hash_color: [...r.hash_color] as Rgb,
    areas: r.areas.slice(),
    monsoon: r.monsoon.map((m) => ({ ...m })),
    raw_extra: r.raw_extra.slice(),
  };
}
function cloneSuper(s: GeoSuperregion): GeoSuperregion {
  return { ...s, regions: s.regions.slice() };
}

// ── Effective network (base + PENDING) ────────────────────────────────────────

/**
 * A cheap FNV-ish hash → an rgb, matching `map_renderer::hash_color` closely
 * enough for a placeholder swatch on a just-created (not-yet-on-disk) entity.
 * The real color is re-fetched from the backend after save.
 */
export function hashColor(key: string): Rgb {
  let h = 2166136261 >>> 0;
  for (let i = 0; i < key.length; i++) {
    h ^= key.charCodeAt(i);
    h = Math.imul(h, 16777619) >>> 0;
  }
  // Spread into a bright-ish rgb; exact match isn't required (backend re-supplies).
  return [80 + (h & 0x7f), 80 + ((h >> 8) & 0x7f), 80 + ((h >> 16) & 0x7f)];
}

/** Folds the typed edit queue over `base`, returning the effective network. */
export function foldGeo(base: GeoNetwork, edits: TypedEdit[]): GeoNetwork {
  const areas = base.areas.map(cloneArea);
  const regions = base.regions.map(cloneRegion);
  const superregions = base.superregions.map(cloneSuper);
  const areaByKey = new Map(areas.map((a) => [a.key, a]));
  const regionByKey = new Map(regions.map((r) => [r.key, r]));
  const superByKey = new Map(superregions.map((s) => [s.key, s]));

  const removeFromList = (arr: (number | string)[], v: number | string) => {
    const i = arr.indexOf(v);
    if (i >= 0) arr.splice(i, 1);
  };

  for (const e of edits) {
    switch (e.kind) {
      case "appendText":
      case "createFile": {
        if (e.file === base.area_file) {
          const a = parseAreaScaffold(e.text, hashColor(/^\s*([A-Za-z0-9_]+)/.exec(e.text)?.[1] ?? ""));
          if (a && !areaByKey.has(a.key)) {
            a.source_file = e.file;
            areas.push(a);
            areaByKey.set(a.key, a);
          }
        } else if (e.file === base.region_file) {
          const r = parseRegionScaffold(e.text, hashColor(/^\s*([A-Za-z0-9_]+)/.exec(e.text)?.[1] ?? ""));
          if (r && !regionByKey.has(r.key)) {
            r.source_file = e.file;
            regions.push(r);
            regionByKey.set(r.key, r);
          }
        } else if (e.file === base.superregion_file) {
          // S3.1: a pending-created superregion (RegionPanel "+ Create Superregion").
          const s = parseSuperregionScaffold(e.text);
          if (s && !superByKey.has(s.key)) {
            s.source_file = e.file;
            superregions.push(s);
            superByKey.set(s.key, s);
          }
        }
        break;
      }
      case "insertStatement": {
        // Add-monsoon-row (S2.6): `monsoon = { start end }` into a region block.
        if (
          e.file === base.region_file &&
          e.blockPath.length === 1 &&
          statementKeyOf(e.statement) === "monsoon"
        ) {
          const r = regionByKey.get(e.blockPath[0]);
          const range = parseMonsoonStatement(e.statement);
          if (r && range) {
            r.monsoon.push(range);
            r.has_monsoon = true;
          }
        }
        break;
      }
      case "setBlock": {
        // Edit-monsoon-row (S2.6): setBlock on `[region, "monsoon#n"]`.
        if (
          e.file === base.region_file &&
          e.path.length === 2 &&
          (e.path[1] === "monsoon" || e.path[1].startsWith("monsoon#"))
        ) {
          const r = regionByKey.get(e.path[0]);
          const range = parseMonsoonDates(e.value);
          const idx = occurrenceOf(e.path[1]);
          if (r && range && idx >= 0 && idx < r.monsoon.length) r.monsoon[idx] = range;
        }
        break;
      }
      case "removeStatement": {
        // Remove-monsoon-row (S2.6): removeStatement key `monsoon#n` in a region.
        if (
          e.file === base.region_file &&
          e.blockPath.length === 1 &&
          (e.key === "monsoon" || e.key.startsWith("monsoon#"))
        ) {
          const r = regionByKey.get(e.blockPath[0]);
          const idx = occurrenceOf(e.key);
          if (r && idx >= 0 && idx < r.monsoon.length) {
            r.monsoon.splice(idx, 1);
            r.has_monsoon = r.monsoon.length > 0;
          }
          break;
        }
        if (e.blockPath.length !== 0) break;
        if (e.file === base.area_file && areaByKey.has(e.key)) {
          const i = areas.findIndex((a) => a.key === e.key);
          if (i >= 0) areas.splice(i, 1);
          areaByKey.delete(e.key);
          // Also strip it from every region's areas list.
          for (const r of regions) removeFromList(r.areas, e.key);
        } else if (e.file === base.region_file && regionByKey.has(e.key)) {
          const i = regions.findIndex((r) => r.key === e.key);
          if (i >= 0) regions.splice(i, 1);
          regionByKey.delete(e.key);
          for (const s of superregions) removeFromList(s.regions, e.key);
        } else if (e.file === base.superregion_file && superByKey.has(e.key)) {
          // S3.1: delete a superregion — its regions become unassigned (the
          // rollup recompute below sets each region.superregion back to null).
          const i = superregions.findIndex((s) => s.key === e.key);
          if (i >= 0) superregions.splice(i, 1);
          superByKey.delete(e.key);
        }
        break;
      }
      case "addId": {
        applyAdd(e.file, e.listPath, e.id, base, areaByKey, regionByKey, superByKey);
        break;
      }
      case "removeId": {
        applyRemove(e.file, e.listPath, e.id, base, areaByKey, regionByKey, superByKey);
        break;
      }
      case "listMove": {
        applyRemove(e.fromFile, e.fromPath, e.id, base, areaByKey, regionByKey, superByKey);
        applyAdd(e.toFile, e.toPath, e.id, base, areaByKey, regionByKey, superByKey);
        break;
      }
    }
  }

  // Recompute rollups: area→region and region→superregion.
  for (const a of areas) a.region = null;
  for (const r of regions) {
    r.superregion = null;
    for (const ak of r.areas) {
      const a = areaByKey.get(ak);
      if (a && a.region == null) a.region = r.key;
    }
  }
  for (const s of superregions) {
    for (const rk of s.regions) {
      const r = regionByKey.get(rk);
      if (r && r.superregion == null) r.superregion = s.key;
    }
  }

  return {
    areas,
    regions,
    superregions,
    area_file: base.area_file,
    region_file: base.region_file,
    superregion_file: base.superregion_file,
  };
}

function applyAdd(
  file: string,
  listPath: string[],
  id: string,
  base: GeoNetwork,
  areaByKey: Map<string, GeoArea>,
  regionByKey: Map<string, GeoRegion>,
  superByKey: Map<string, GeoSuperregion>,
) {
  if (file === base.area_file && listPath.length === 1) {
    const a = areaByKey.get(listPath[0]);
    const n = parseInt(id, 10);
    if (a && Number.isFinite(n) && !a.provinces.includes(n)) a.provinces.push(n);
  } else if (file === base.region_file && listPath.length === 2 && listPath[1] === "areas") {
    const r = regionByKey.get(listPath[0]);
    if (r && !r.areas.includes(id)) r.areas.push(id);
  } else if (file === base.superregion_file && listPath.length === 1) {
    const s = superByKey.get(listPath[0]);
    if (s && !s.regions.includes(id)) s.regions.push(id);
  }
}

function applyRemove(
  file: string,
  listPath: string[],
  id: string,
  base: GeoNetwork,
  areaByKey: Map<string, GeoArea>,
  regionByKey: Map<string, GeoRegion>,
  superByKey: Map<string, GeoSuperregion>,
) {
  if (file === base.area_file && listPath.length === 1) {
    const a = areaByKey.get(listPath[0]);
    const n = parseInt(id, 10);
    if (a) {
      const i = a.provinces.indexOf(n);
      if (i >= 0) a.provinces.splice(i, 1);
    }
  } else if (file === base.region_file && listPath.length === 2 && listPath[1] === "areas") {
    const r = regionByKey.get(listPath[0]);
    if (r) {
      const i = r.areas.indexOf(id);
      if (i >= 0) r.areas.splice(i, 1);
    }
  } else if (file === base.superregion_file && listPath.length === 1) {
    const s = superByKey.get(listPath[0]);
    if (s) {
      const i = s.regions.indexOf(id);
      if (i >= 0) s.regions.splice(i, 1);
    }
  }
}

// ── Membership indices (for map recolor / hit-testing) ────────────────────────

/** province id → area key, over the effective network. */
export function areaMembershipIndex(net: GeoNetwork): Map<number, string> {
  const m = new Map<number, string>();
  for (const a of net.areas) for (const id of a.provinces) m.set(id, a.key);
  return m;
}

/** area key → region key, over the effective network. */
export function areaToRegion(net: GeoNetwork): Map<string, string> {
  const m = new Map<string, string>();
  for (const r of net.regions) for (const ak of r.areas) if (!m.has(ak)) m.set(ak, r.key);
  return m;
}

/** province id → region key, resolved province → area → region. */
export function regionMembershipIndex(net: GeoNetwork): Map<number, string> {
  const a2r = areaToRegion(net);
  const m = new Map<number, string>();
  for (const a of net.areas) {
    const rk = a2r.get(a.key);
    if (rk == null) continue;
    for (const id of a.provinces) m.set(id, rk);
  }
  return m;
}
