// Sprint 12.3 — edit-at-date decision logic (pure).
//
// The toolkit views/edits a mod at a *selected date*. Editing history content at
// the selected date must land in the right place in the province/country file:
//
//   DECISION TABLE (editAtDate)
//   ---------------------------------------------------------------------------
//   selectedDate == null            → write TOP-LEVEL keys (the base state). The
//                                     date is unresolved; there is nothing else
//                                     we could sensibly address.
//
//   the top level is AUTHORITATIVE  → write TOP-LEVEL keys, exactly as the
//   for the written keys at         pre-12.3 writers did. We return the caller's
//   selectedDate                    `startEdits` untouched. See below.
//
//   otherwise  AND a block for      → MERGE: the file already has a `Y.M.D = {…}`
//   that exact date exists            block for the selected date. Emit one
//                                     `InsertStatement` per statement targeting
//                                     that block. When several blocks share the
//                                     date, the LAST occurrence is the merge
//                                     target (deterministic) — addressed as
//                                     `["Y.M.D"]` for occurrence 0, else
//                                     `["Y.M.D#<occ>"]`.
//
//   otherwise  AND no block for     → INSERT: Emit a single `InsertDatedBlock`
//   that date                         carrying all the statements as one
//                                     `Y.M.D = { … }` block, which mod_writer
//                                     places in date order.
//   ---------------------------------------------------------------------------
//
// WHEN IS THE TOP LEVEL AUTHORITATIVE?
//
// Two history models exist in the wild, and the toolkit must serve both:
//
//   Vanilla model:  the top level IS the start state. Dated blocks are all
//                   post-start events. Writing at 1444.11.11 → top level.
//
//   Timeline model: (Extended Timeline, Imperium Universalis and derivatives)
//                   the top level is the state at the file's BASELINE EPOCH
//                   (year 2 for ET), and dated blocks replay history forward.
//                   The start state is the fold of the top level plus every
//                   dated block ≤ the start date. Writing the top level at
//                   1302.9.1 is silently overridden by every intervening block.
//
// So "≤ startDate → top level" is wrong: it assumes the vanilla model. The
// correct, model-agnostic test is per FILE and per KEY — the top level is
// authoritative for a key only when no dated block ≤ selectedDate assigns it.
// `shadowedKeysFrom` computes that key set from the file's blocks; when any
// written key is shadowed the whole write moves into a dated block at the
// selected date, so owner/controller/cores stay together in one block rather
// than splitting across the baseline and the timeline.
//
// A write at a date LATER than the start always uses a dated block (Sprint 12.3
// behaviour) whether or not anything is shadowed — that is the point of it.
//
// `startDate` is the mod's effective start (default bookmark, else earliest, else
// 1444.11.11). This module is pure (no Svelte, no IPC) so the table reads like a
// unit test by inspection; `provinceEditMutations`/`parseStatements` are the
// read-back side used by the pending-edit folds.

import { compareDates } from "./calendar";
import type { TypedEdit } from "./edits.svelte";

/** A file's existing dated block, reduced to what the merge decision needs. */
export interface DatedBlockRef {
  date: string;
  occurrenceIndex: number;
  /**
   * The statement keys this block assigns. Optional — only callers that can
   * supply it get the shadow decision (see `shadowedKeysFrom`); callers without
   * block bodies pass `shadowedKeys` to `editAtDate` directly instead.
   */
  keys?: string[];
}

/**
 * Keys whose top-level form is CUMULATIVE rather than an assignment: a later
 * dated `add_core = X` does not override a top-level `add_core = Y`, they stack.
 * Such a key is shadowed only by its INVERSE appearing in a dated block (a
 * `remove_core` can undo a baseline core). `null` = no inverse exists, so the
 * key can never be shadowed and a top-level write always survives.
 *
 * Every other key is treated as a plain assignment: a dated block carrying the
 * same key shadows the top level.
 */
const CUMULATIVE_INVERSE: Record<string, string | null> = {
  add_core: "remove_core",
  remove_core: "add_core",
  add_claim: "remove_claim",
  remove_claim: "add_claim",
  add_permanent_province_modifier: "remove_province_modifier",
  remove_province_modifier: "add_permanent_province_modifier",
  discovered_by: null,
  // Country-history cumulative keys (the country panel's writers).
  add_accepted_culture: "remove_accepted_culture",
  remove_accepted_culture: "add_accepted_culture",
  set_estate_privilege: "remove_estate_privilege",
  remove_estate_privilege: "set_estate_privilege",
  add_government_reform: null,
  historical_rival: null,
  historical_friend: null,
};

/** The leading `key` of a `key = value` statement, or null when unparseable. */
function statementKey(statement: string): string | null {
  const m = /^\s*([A-Za-z_][A-Za-z0-9_.]*)\s*=/.exec(statement);
  return m ? m[1] : null;
}

/**
 * The keys assigned by dated blocks at or before `date` — i.e. the keys for
 * which the file's top level is NOT authoritative at that date. Callers that
 * hold the file's blocks (the province panel) build the shadow set with this;
 * bulk callers (map paint) get an equivalent set from the backend.
 */
export function shadowedKeysFrom(
  blocks: DatedBlockRef[],
  date: string | null,
): Set<string> {
  const out = new Set<string>();
  if (date == null) return out;
  for (const b of blocks) {
    if (compareDates(b.date, date) > 0) continue;
    for (const k of b.keys ?? []) out.add(k);
  }
  return out;
}

/**
 * True when at least one of `statements` writes a key the top level no longer
 * controls at the selected date, so the whole write must go into a dated block.
 */
export function isShadowed(statements: string[], shadowedKeys: Set<string>): boolean {
  if (shadowedKeys.size === 0) return false;
  return statements.some((s) => {
    const key = statementKey(s);
    if (key == null) return false;
    if (key in CUMULATIVE_INVERSE) {
      const inverse = CUMULATIVE_INVERSE[key];
      return inverse != null && shadowedKeys.has(inverse);
    }
    return shadowedKeys.has(key);
  });
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
  /**
   * Keys already assigned by dated blocks at or before `selectedDate`. When any
   * written key is shadowed the top level is not the state at that date, so the
   * write becomes a dated block even at/below the start date (timeline mods).
   * Omit for callers that cannot know (they keep the vanilla-model behaviour);
   * supply via `shadowedKeysFrom(datedBlocks, selectedDate)` when block bodies
   * are available.
   */
  shadowedKeys?: Set<string>;
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

  // No resolved date: nothing to address but the base state.
  if (selectedDate == null) return startEdits;
  const date = selectedDate;

  // At or below the start, the top level is authoritative ONLY while no dated
  // block ≤ the selected date already assigns one of the written keys. That is
  // always true in vanilla (dated blocks are post-start) and false for a
  // timeline mod whose baseline epoch predates its start date.
  const shadowed = isShadowed(statements, input.shadowedKeys ?? new Set());
  if (!isLaterThanStart(date, startDate) && !shadowed) return startEdits;

  // Nothing to write into a dated block is a no-op (guards empty statement lists).
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
