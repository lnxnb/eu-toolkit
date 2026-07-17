// Sprint 19.3 — Government names (dynamic country names & ruler titles).
//
// Mirrors the `get_government_names` wire payload (government_names.rs) and folds
// the typed edit queue over the base so create / delete / reorder (SetBlock body
// swap) / trigger-toggle edits appear live (and undo/redo revert them). Rank×role
// CELL edits are LocOverrides on the cells' loc keys — read via
// `queue.pendingLocOverride(loc_key)` at render, so they aren't folded here.

import type { TypedEdit } from "$lib/edits.svelte";

export const GOV_NAMES_DIR = "common/government_names";
export const GOV_NAMES_PROJECT_FILE =
  "common/government_names/zz_eutoolkit_government_names.txt";

/** The modeled rank×role table columns, in order (`rank` = country name). */
export const ROLES = ["rank", "ruler_male", "ruler_female", "consort_male", "consort_female"] as const;
export type Role = (typeof ROLES)[number];

// ── Wire types (mirror government_names.rs; GovNameCell/Scheme = snake_case) ───

export interface GovNameCell {
  role: string;
  rank: number;
  loc_key: string;
  resolved: string;
}

export interface GovNameScheme {
  key: string;
  file: string;
  origin: string; // "base" | "mod"
  has_trigger: boolean;
  cells: GovNameCell[];
  raw_extra: string[];
  raw: string;
}

export interface GovernmentNamesData {
  dir: string;
  project_file: string;
  schemes: GovNameScheme[];
}

export interface GovNameScaffold {
  key: string;
  file: string;
  text: string;
  cells: GovNameCell[];
}

/** Preview payload (camelCase — matches serde rename on GovNamePreview). */
export interface GovNamePreview {
  tag: string;
  rank: number;
  matchedKey: string | null;
  matchedFile: string | null;
  countryName: string | null;
  rulerName: string | null;
  approximate: boolean;
  skipped: string[];
}

// ── Key helpers ───────────────────────────────────────────────────────────────

const KEY_RE = /^[a-z][a-z0-9_]*$/;
export function isValidSchemeKey(key: string): boolean {
  return KEY_RE.test(key);
}

export function slugify(name: string): string {
  const s = name
    .toLowerCase()
    .normalize("NFKD")
    .replace(/[^a-z0-9]+/g, "_")
    .replace(/^_+|_+$/g, "");
  return s || "scheme";
}

/** Finds a cell for (role, rank), or undefined. */
export function cellOf(scheme: GovNameScheme, role: string, rank: number): GovNameCell | undefined {
  return scheme.cells.find((c) => c.role === role && c.rank === rank);
}

// ── Scheme-body parsing (scaffold text / reorder-swapped body) ────────────────

/** Extracts the role→rank→loc_key cells + trigger presence from a block body
 *  (with or without the `key = { … }` wrapper). Role blocks are flat
 *  `<rank> = LOC` lists (no nested braces), so a non-greedy `{…}` match suffices. */
export function parseSchemeBody(body: string): { cells: GovNameCell[]; hasTrigger: boolean } {
  const cells: GovNameCell[] = [];
  for (const role of ROLES) {
    const m = new RegExp(`\\b${role}\\s*=\\s*\\{([^{}]*)\\}`).exec(body);
    if (!m) continue;
    const inner = m[1];
    const rankRe = /(\d+)\s*=\s*"?([A-Za-z0-9_.]+)"?/g;
    let rm: RegExpExecArray | null;
    while ((rm = rankRe.exec(inner)) !== null) {
      const rank = parseInt(rm[1], 10);
      if (rank >= 1 && rank <= 3) {
        cells.push({ role, rank, loc_key: rm[2], resolved: rm[2] });
      }
    }
  }
  return { cells, hasTrigger: /\btrigger\s*=/.test(body) };
}

function schemeKeyOf(text: string): string | null {
  return /^\s*([A-Za-z0-9_]+)\s*=/.exec(text)?.[1] ?? null;
}

function parseScaffoldScheme(text: string, file: string): GovNameScheme | null {
  const key = schemeKeyOf(text);
  if (!key) return null;
  const { cells, hasTrigger } = parseSchemeBody(text);
  return { key, file, origin: "mod", has_trigger: hasTrigger, cells, raw_extra: [], raw: text };
}

// ── Statement key helper ──────────────────────────────────────────────────────

function statementKey(s: string): string {
  const eq = s.indexOf("=");
  return eq < 0 ? "" : s.slice(0, eq).trim();
}

// ── Effective data (base + PENDING) ───────────────────────────────────────────

function cloneScheme(s: GovNameScheme): GovNameScheme {
  return { ...s, cells: s.cells.map((c) => ({ ...c })), raw_extra: s.raw_extra.slice() };
}

const inGovDir = (file: string) =>
  file.startsWith(GOV_NAMES_DIR + "/") || file === GOV_NAMES_PROJECT_FILE;

/** Folds the typed edit queue over `base`, returning the effective schemes. */
export function foldGovernmentNames(base: GovernmentNamesData, edits: TypedEdit[]): GovernmentNamesData {
  const schemes = base.schemes.map(cloneScheme);
  const byKey = new Map(schemes.map((s) => [s.key, s]));

  for (const e of edits) {
    switch (e.kind) {
      case "appendText":
      case "createFile": {
        if (!inGovDir(e.file)) break;
        const s = parseScaffoldScheme(e.text, e.file);
        if (s && !byKey.has(s.key)) {
          schemes.push(s);
          byKey.set(s.key, s);
        }
        break;
      }
      case "removeStatement": {
        if (!inGovDir(e.file)) break;
        // Whole-scheme delete (top-level block).
        if (e.blockPath.length === 0 && byKey.has(e.key)) {
          const i = schemes.findIndex((x) => x.key === e.key);
          if (i >= 0) schemes.splice(i, 1);
          byKey.delete(e.key);
          break;
        }
        // Remove a scheme's trigger → unconditional (always matches).
        if (e.blockPath.length === 1 && e.key === "trigger") {
          const s = byKey.get(e.blockPath[0]);
          if (s) s.has_trigger = false;
        }
        break;
      }
      case "insertStatement": {
        if (!inGovDir(e.file)) break;
        if (e.blockPath.length === 1 && statementKey(e.statement) === "trigger") {
          const s = byKey.get(e.blockPath[0]);
          if (s) s.has_trigger = true;
        }
        break;
      }
      case "setBlock": {
        // Reorder body swap: a scheme's body was replaced with another's.
        if (!inGovDir(e.file) || e.path.length !== 1) break;
        const s = byKey.get(occSchemeKey(e.path[0]));
        if (!s) break;
        const { cells, hasTrigger } = parseSchemeBody(e.value);
        s.cells = cells;
        s.has_trigger = hasTrigger;
        s.raw = `${s.key} = { ${e.value} }`;
        break;
      }
    }
  }

  return { ...base, schemes };
}

// A top-level scheme path segment carries no occurrence in practice (unique
// keys), but strip a `#n` defensively.
function occSchemeKey(seg: string): string {
  const h = seg.indexOf("#");
  return h < 0 ? seg : seg.slice(0, h);
}
