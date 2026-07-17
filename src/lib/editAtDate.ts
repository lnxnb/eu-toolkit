// Sprint 12.3 — edit-at-date decision logic (pure).
//
// The toolkit views/edits a mod at a *selected date*. Editing history content at
// the selected date must land in the right place in the province/country file:
//
//   DECISION TABLE (editAtDate)
//   ---------------------------------------------------------------------------
//   selectedDate ≤ startDate      → write TOP-LEVEL keys (the base/start state),
//   (or selectedDate == null)       exactly as the pre-12.3 writers did. We just
//                                   return the caller's `startEdits` untouched.
//
//   selectedDate  > startDate  AND  → MERGE: the file already has a `Y.M.D = {…}`
//   a block for that exact date       block for the selected date. Emit one
//   exists in the file                `InsertStatement` per statement targeting
//                                     that block. When several blocks share the
//                                     date, the LAST occurrence is the merge
//                                     target (deterministic) — addressed as
//                                     `["Y.M.D"]` for occurrence 0, else
//                                     `["Y.M.D#<occ>"]`.
//
//   selectedDate  > startDate  AND  → INSERT: no block for that date yet. Emit a
//   no block for that date            single `InsertDatedBlock` carrying all the
//                                     statements as one `Y.M.D = { … }` block,
//                                     which mod_writer places in date order.
//   ---------------------------------------------------------------------------
//
// `startDate` is the mod's effective start (default bookmark, else earliest, else
// 1444.11.11) — the "earliest defined date" the spec calls the base state. This
// module is pure (no Svelte, no IPC) so the table reads like a unit test by
// inspection; `provinceEditMutations`/`parseStatements` are the read-back side
// used by the pending-edit folds.

import { compareDates } from "./calendar";
import type { TypedEdit } from "./edits.svelte";

/** A file's existing dated block, reduced to what the merge decision needs. */
export interface DatedBlockRef {
  date: string;
  occurrenceIndex: number;
}

export interface EditAtDateInput {
  /** Game-relative history file the edit targets. */
  file: string;
  /** The selected view/edit date ("Y.M.D"), or null when unresolved (= start). */
  selectedDate: string | null;
  /** The mod's effective start date ("Y.M.D"). */
  startDate: string;
  /** The file's existing dated blocks (for the merge-vs-insert decision). */
  datedBlocks: DatedBlockRef[];
  /** The edits the caller would emit at the start date (top-level shape). */
  startEdits: TypedEdit[];
  /** Flat `key = value` statements to place in the dated block at a later date. */
  statements: string[];
}

/** True when `selectedDate` is strictly after `startDate` (i.e. a dated write). */
export function isLaterThanStart(
  selectedDate: string | null,
  startDate: string,
): boolean {
  return selectedDate != null && compareDates(selectedDate, startDate) > 0;
}

/**
 * The block-path segment addressing the given occurrence of a dated block:
 * `"Y.M.D"` for the first (occurrence 0), else `"Y.M.D#<occ>"` — matching the
 * mod_writer occurrence-suffix convention.
 */
export function datedBlockSegment(date: string, occurrenceIndex: number): string {
  return occurrenceIndex === 0 ? date : `${date}#${occurrenceIndex}`;
}

/**
 * Resolve the correct TypedEdit(s) for writing `statements` at the selected date.
 * See the decision table at the top of this file.
 */
export function editAtDate(input: EditAtDateInput): TypedEdit[] {
  const { file, selectedDate, startDate, datedBlocks, startEdits, statements } = input;

  // Start (or earlier / unresolved): keep the pre-12.3 top-level behavior.
  if (!isLaterThanStart(selectedDate, startDate)) return startEdits;
  const date = selectedDate as string;

  // Nothing to write at a later date is a no-op (guards empty statement lists).
  if (statements.length === 0) return [];

  // MERGE into an existing exact-date block (last occurrence wins).
  const matches = datedBlocks.filter((b) => b.date === date);
  if (matches.length > 0) {
    const occ = matches.reduce((m, b) => Math.max(m, b.occurrenceIndex), 0);
    const seg = datedBlockSegment(date, occ);
    return statements.map((statement) => ({
      kind: "insertStatement" as const,
      file,
      blockPath: [seg],
      statement,
    }));
  }

  // INSERT a fresh, date-ordered `Y.M.D = { … }` block.
  return [
    {
      kind: "insertDatedBlock" as const,
      file,
      date,
      statement: `${date} = { ${statements.join(" ")} }`,
    },
  ];
}

// --- Read-back side: normalize a TypedEdit into its field mutations ----------
//
// The pending-edit folds (map repaint, effOf/mutEff, religion/culture/goods/dev
// projections) must understand history edits whether they were written top-level
// (start date) or into a dated block (later date). These helpers turn any
// history TypedEdit into the flat `key/value/remove` tuples the folds apply, so
// one code path handles both shapes.

/** One folded field mutation extracted from an edit. */
export interface FieldMut {
  key: string;
  value: string;
  /** True for a value-removing edit (`removeStatement`). */
  remove: boolean;
}

// Matches top-level `key = value` pairs inside a block body. Values are a quoted
// string, a `{ … }` sub-block, or a bare token. Good enough for the modeled
// scalar keys the folds care about (they ignore nested-block values anyway).
const STATEMENT_RE = /([A-Za-z_][A-Za-z0-9_.]*)\s*=\s*("[^"]*"|\{[^{}]*\}|[^\s{}]+)/g;

/** Extracts `key = value` pairs from a statement or block-body string. */
export function parseStatements(text: string): { key: string; value: string }[] {
  const out: { key: string; value: string }[] = [];
  STATEMENT_RE.lastIndex = 0;
  let m: RegExpExecArray | null;
  while ((m = STATEMENT_RE.exec(text)) !== null) out.push({ key: m[1], value: m[2] });
  return out;
}

/**
 * Normalize a history-file TypedEdit into the field mutations it performs,
 * regardless of whether it targets the top level or a dated block. Returns the
 * target file plus the mutations, or null for edit kinds that don't mutate
 * fields (createFile, listMove, locOverride, …).
 */
export function provinceEditMutations(
  e: TypedEdit,
): { file: string; muts: FieldMut[] } | null {
  switch (e.kind) {
    case "setScalar":
      // path is [key] top-level or [date, key] / [date#n, key] in a dated block.
      return {
        file: e.file,
        muts: [{ key: e.path[e.path.length - 1], value: e.value, remove: false }],
      };
    case "insertStatement": {
      const t = e.statement.trim();
      // A whole `Y.M.D = { … }` block written at the top level (a Timeline
      // add-entry) is NOT folded through this path — only `insertDatedBlock`
      // (which always rides in a date-tagged composite that the folds gate) and
      // flat `key = value` statements fold. This keeps a future-dated timeline
      // block from leaking into the current view's overlay.
      if (/^\d+\.\d+\.\d+\s*=\s*\{/.test(t)) return { file: e.file, muts: [] };
      return {
        file: e.file,
        muts: parseStatements(t).map((s) => ({ ...s, remove: false })),
      };
    }
    case "removeStatement":
      return { file: e.file, muts: [{ key: e.key, value: e.value ?? "", remove: true }] };
    case "insertDatedBlock": {
      // Statement is `Y.M.D = { k1 = v1 k2 = v2 … }` — fold the inner block body.
      const open = e.statement.indexOf("{");
      const close = e.statement.lastIndexOf("}");
      const inner = open >= 0 && close > open ? e.statement.slice(open + 1, close) : "";
      return {
        file: e.file,
        muts: parseStatements(inner).map((s) => ({ ...s, remove: false })),
      };
    }
    default:
      return null;
  }
}
