// Sprint 30.1 — pure helpers for the Edits panel (View ▸ Edits).
//
// This module is deliberately runtime-free (no Svelte runes, no Tauri): it turns
// the EditQueue's composites into what the panel renders and decides. It is the
// unit-tested core of 30.1 — the independence predicate that gates the "revert
// this edit alone" action, the file-set extraction it rests on, the best-effort
// jump mapping, and the human-readable TypedEdit summaries.
//
// Only *type* imports from the rune module (erased at build time, per
// vitest.config.js) so this stays importable from plain-Node vitest.

import type { Composite, TypedEdit } from "$lib/edits.svelte";

// --- File-set extraction -------------------------------------------------
//
// Two composites "touch the same file" if their edits could interact when the
// queue is flattened and saved. Every TypedEdit maps to one or more *file keys*.
// Edits with no real file (defines, localisation) map to a synthetic key so all
// edits sharing one writer (the defines writer, the loc writer) collide; a
// ruler rename keys on its tag (per-country history). Any future/unknown kind
// falls back to a single shared "<unknown>" key — the conservative choice, so
// two unmapped edits are always treated as dependent.

/** Synthetic file keys for edits that don't carry a real game-file path. */
const DEFINES_KEY = "<defines>";
const LOC_KEY = "<localisation>";

/** Every file key a single typed edit touches. */
export function editFiles(e: TypedEdit): string[] {
  switch (e.kind) {
    case "setScalar":
    case "setBlock":
    case "removeStatement":
    case "insertStatement":
    case "insertDatedBlock":
    case "addId":
    case "removeId":
    case "appendText":
    case "createFile":
    case "csvRewrite":
    case "deleteFile":
    case "binaryAsset":
      return [e.file];
    case "listMove":
      return e.fromFile === e.toFile ? [e.fromFile] : [e.fromFile, e.toFile];
    case "setDefine":
      return [DEFINES_KEY];
    case "locOverride":
    case "locRemove":
      return [LOC_KEY];
    case "renameRuler":
      return [`<ruler:${e.tag}>`];
    default:
      return ["<unknown>"];
  }
}

/** The union of file keys every edit in the composite touches. */
export function compositeFiles(c: Composite): Set<string> {
  const out = new Set<string>();
  for (const e of c.edits) for (const f of editFiles(e)) out.add(f);
  return out;
}

// --- Independence predicate ----------------------------------------------

/**
 * Can the composite at `index` be reverted on its own, leaving the rest of the
 * queue valid? Yes iff NO LATER composite touches any file it touches
 * (conservative: earlier same-file composites are fine — they apply before it,
 * so dropping this one just removes its last-wins contribution to that file).
 *
 * Pure over the composite array — this is the exact predicate the panel disables
 * the "revert this edit alone" button on, and the unit-tested contract of 30.6.
 */
export function isIndependentlyRevertible(
  composites: readonly Composite[],
  index: number,
): boolean {
  if (index < 0 || index >= composites.length) return false;
  const target = compositeFiles(composites[index]);
  for (let j = index + 1; j < composites.length; j++) {
    for (const f of compositeFiles(composites[j])) {
      if (target.has(f)) return false;
    }
  }
  return true;
}

// --- Jump mapping --------------------------------------------------------
//
// Best-effort "jump to what this edit affects". History files resolve to a
// concrete entity (province / country); static map + common files resolve to a
// map mode. Undecidable edits (defines, localisation) yield null (no jump).

export type EditJump =
  | { kind: "province"; id: number }
  | { kind: "country"; tag: string }
  | { kind: "mode"; mode: string };

/** Map-mode a static game file belongs to, or null if none is a good target. */
function fileToMode(file: string): string | null {
  const f = file.replace(/\\/g, "/").toLowerCase();
  if (f.startsWith("map/area.txt")) return "areas";
  if (f.startsWith("map/region.txt") || f.startsWith("map/superregion.txt")) return "regions";
  if (f.startsWith("map/climate.txt")) return "climate";
  if (f.startsWith("map/terrain.txt")) return "simple_terrain";
  if (f.startsWith("map/adjacencies.csv")) return "provinces";
  if (f.startsWith("common/tradenodes/")) return "trade_nodes";
  if (f.startsWith("common/colonial_regions/")) return "colonial_regions";
  if (f.startsWith("common/trade_companies/")) return "trade_companies";
  return null;
}

/** Leading province id of a `history/provinces/<id> - Name.txt` path, else null. */
function provinceIdFromFile(file: string): number | null {
  const base = file.replace(/\\/g, "/").split("/").pop() ?? "";
  const m = base.match(/^(\d+)/);
  return m ? Number(m[1]) : null;
}

/** Country tag of a `history/countries/<TAG> - Name.txt` path, else null. The
 *  vanilla convention is `TAG - Name.txt`; take the token before the dash. */
function tagFromCountryFile(file: string): string | null {
  const base = file.replace(/\\/g, "/").split("/").pop() ?? "";
  const m = base.match(/^([A-Za-z0-9]{2,3})\s*-\s*/);
  return m ? m[1].toUpperCase() : null;
}

/**
 * Best-effort jump for a composite: the first of its edits that resolves to a
 * target wins. `renameRuler` → the country; `history/provinces` → the province;
 * `history/countries` → the country; a static map/common file → its map mode.
 * Returns null when nothing resolves (defines / localisation / unknown).
 */
export function compositeJump(c: Composite): EditJump | null {
  for (const e of c.edits) {
    if (e.kind === "renameRuler") return { kind: "country", tag: e.tag };
    const files = editFiles(e);
    for (const file of files) {
      const norm = file.replace(/\\/g, "/");
      if (norm.startsWith("history/provinces/")) {
        const id = provinceIdFromFile(file);
        if (id != null) return { kind: "province", id };
      }
      if (norm.startsWith("history/countries/")) {
        const tag = tagFromCountryFile(file);
        if (tag) return { kind: "country", tag };
      }
      const mode = fileToMode(file);
      if (mode) return { kind: "mode", mode };
    }
  }
  return null;
}

// --- Human-readable TypedEdit summary ------------------------------------

/** A path array rendered as `a ▸ b ▸ c` (or `(root)` when empty). */
function fmtPath(path: string[]): string {
  return path.length ? path.join(" ▸ ") : "(root)";
}

function truncate(s: string, n = 60): string {
  const one = s.replace(/\s+/g, " ").trim();
  return one.length > n ? one.slice(0, n - 1) + "…" : one;
}

/** One typed edit rendered as a `{ file, detail }` row for the expanded view. */
export function summarizeEdit(e: TypedEdit): { file: string; detail: string } {
  switch (e.kind) {
    case "setScalar":
      return { file: e.file, detail: `set ${fmtPath(e.path)} = ${truncate(e.value)}` };
    case "setBlock":
      return { file: e.file, detail: `set block ${fmtPath(e.path)} = { … }` };
    case "removeStatement":
      return {
        file: e.file,
        detail: `remove ${e.key}${e.value != null ? ` = ${e.value}` : ""} from ${fmtPath(e.blockPath)}`,
      };
    case "insertStatement":
      return { file: e.file, detail: `insert into ${fmtPath(e.blockPath)}: ${truncate(e.statement)}` };
    case "insertDatedBlock":
      return { file: e.file, detail: `insert ${e.date} block: ${truncate(e.statement)}` };
    case "setDefine":
      return {
        file: "common/defines.lua",
        detail: `set ${e.namespace ?? "NGame"}.${e.key} = ${truncate(e.value)}`,
      };
    case "addId":
      return { file: e.file, detail: `add ${e.id} to ${fmtPath(e.listPath)}` };
    case "removeId":
      return { file: e.file, detail: `remove ${e.id} from ${fmtPath(e.listPath)}` };
    case "listMove":
      return {
        file: e.fromFile === e.toFile ? e.fromFile : `${e.fromFile} → ${e.toFile}`,
        detail: `move ${e.id}: ${fmtPath(e.fromPath)} → ${fmtPath(e.toPath)}`,
      };
    case "appendText":
      return { file: e.file, detail: `append text (${e.text.length} chars)` };
    case "createFile":
      return { file: e.file, detail: "create file" };
    case "csvRewrite":
      return { file: e.file, detail: `rewrite CSV (${e.rows.length} rows)` };
    case "deleteFile":
      return { file: e.file, detail: "delete file" };
    case "locOverride":
      return { file: "localisation", detail: `set loc ${e.key} = ${truncate(e.value)}` };
    case "locRemove":
      return { file: "localisation", detail: `remove loc ${e.key}` };
    case "binaryAsset":
      return { file: e.file, detail: `write binary asset (${e.bytes.length} bytes)` };
    case "renameRuler":
      return { file: "history/countries", detail: `rename ${e.tag} starting ruler → ${e.name}` };
    default:
      return { file: "<unknown>", detail: JSON.stringify(e) };
  }
}
