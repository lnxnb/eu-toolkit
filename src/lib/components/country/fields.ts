// Edit builders shared by the country panel sections (Sprint 1.2). Each returns
// TypedEdit(s) for the pending-edit queue; presence decides set-vs-insert.
//
// Country-history writes route through `pushAtDate` (mirroring the province
// panel's fields.ts): the top level is written only when it is authoritative for
// the written keys at the selected date — on a timeline mod whose country file
// carries pre-start dated blocks, or at a date past the start, the write lands
// in a dated block instead (see editAtDate.ts). The `pendingHist*` readers are
// the matching display folds: they see the write whichever shape it took.

import type { EditQueue, TypedEdit } from "$lib/edits.svelte";
import {
  editAtDate,
  isShadowed,
  shadowedKeysFrom,
  parseStatements,
  type DatedBlockRef,
} from "$lib/editAtDate";
import { compareDates } from "$lib/calendar";
import type { CountryDatedBlock } from "./history";

/** Sprint 12.3 date context for a country history file's writers. */
export interface CountryDateCtx {
  file: string;
  selectedDate: string | null;
  startDate: string;
  /** The file's dated blocks (`details.dated_blocks`). */
  blocks: CountryDatedBlock[];
}

export function blockRefs(blocks: CountryDatedBlock[]): DatedBlockRef[] {
  return blocks.map((b) => ({
    date: b.date,
    occurrenceIndex: b.occurrence_index,
    keys: b.entries.map((e) => e.key),
  }));
}

/**
 * True when writing `statements` at the selected date must land in a dated
 * block rather than the top level — the date is past the start, or the file's
 * own pre-start history already overrides those keys (timeline mods).
 */
export function writesDatedBlock(
  ctx: CountryDateCtx | null | undefined,
  statements: string[],
): boolean {
  if (!ctx || ctx.selectedDate == null) return false;
  if (compareDates(ctx.selectedDate, ctx.startDate) > 0) return true;
  return isShadowed(statements, shadowedKeysFrom(blockRefs(ctx.blocks), ctx.selectedDate));
}

/**
 * Route a country-history write through the date rule (Sprint 12.3), exactly
 * like the province panel's `pushAtDate`: `editAtDate` decides top-level vs
 * dated block; a dated write tags the composite with the date so the map folds
 * gate it correctly.
 */
export function pushAtDate(
  queue: EditQueue,
  ctx: CountryDateCtx | null | undefined,
  label: string,
  startEdits: TypedEdit[],
  statements: string[],
  coalesceKey?: string,
): void {
  if (!ctx || ctx.selectedDate == null) {
    queue.push({ label, edits: startEdits, ...(coalesceKey ? { coalesceKey } : {}) });
    return;
  }
  const refs = blockRefs(ctx.blocks);
  const edits = editAtDate({
    file: ctx.file,
    selectedDate: ctx.selectedDate,
    startDate: ctx.startDate,
    datedBlocks: refs,
    startEdits,
    statements,
    shadowedKeys: shadowedKeysFrom(refs, ctx.selectedDate),
  });
  // Identity return = the top level is the right target; that write edits the
  // baseline and is date-agnostic, so it is not date-tagged.
  if (edits === startEdits) {
    queue.push({ label, edits, ...(coalesceKey ? { coalesceKey } : {}) });
    return;
  }
  queue.push({ label, edits, date: ctx.selectedDate, ...(coalesceKey ? { coalesceKey } : {}) });
}

// --- Display folds (top-level AND dated pending writes) ----------------------

const DATE_SEG_RE = /^(\d+\.\d+\.\d+)(#\d+)?$/;

/** The date a dated-block path segment addresses, or null for non-date paths. */
function segDate(seg: string): string | null {
  return DATE_SEG_RE.exec(seg)?.[1] ?? null;
}

/** True when a dated write at `d` is visible at the selected date. */
function visible(d: string | null, selectedDate: string | null): boolean {
  return d != null && selectedDate != null && compareDates(d, selectedDate) <= 0;
}

/**
 * Replace nested `{ … }` blocks in a dated-block body with a placeholder so
 * `parseStatements` only sees the body's own depth-0 scalars (a holder create's
 * `monarch = { religion = x }` must not read as a country-religion write).
 */
function flattenBody(body: string): string {
  let out = "";
  let depth = 0;
  for (const ch of body) {
    if (ch === "{") {
      if (depth === 0) out += " __block__ ";
      depth++;
      continue;
    }
    if (ch === "}") {
      depth = Math.max(0, depth - 1);
      continue;
    }
    if (depth === 0) out += ch;
  }
  return out;
}

/** Depth-0 `key = value` scalars of an `insertDatedBlock` statement's body. */
function datedBodyScalars(statement: string): { key: string; value: string }[] {
  const open = statement.indexOf("{");
  const close = statement.lastIndexOf("}");
  if (open < 0 || close <= open) return [];
  return parseStatements(flattenBody(statement.slice(open + 1, close))).filter(
    (s) => s.value !== "__block__",
  );
}

function stmtKeyOf(s: string): string {
  const eq = s.indexOf("=");
  return eq < 0 ? "" : s.slice(0, eq).trim();
}
function stmtValOf(s: string): string {
  const eq = s.indexOf("=");
  return eq < 0 ? "" : s.slice(eq + 1).trim();
}

/**
 * Pending value of a history scalar, seeing both top-level writes (the
 * `pendingField` shapes) and dated-block writes at or before the selected date
 * (setScalar at `[Y.M.D, key]`, insertStatement into `[Y.M.D]`, or a fresh
 * `insertDatedBlock`). Later edits win. `undefined` = the queue doesn't touch
 * the key; `{ value: null }` = a pending removal.
 */
export function pendingHistField(
  queue: EditQueue,
  file: string,
  key: string,
  selectedDate: string | null,
): { value: string | null } | undefined {
  let hit: { value: string | null } | undefined;
  for (const e of queue.serialize()) {
    if (e.kind === "setScalar" && e.file === file) {
      if (e.path.length === 1 && e.path[0] === key) hit = { value: e.value };
      else if (
        e.path.length === 2 &&
        e.path[1] === key &&
        visible(segDate(e.path[0]), selectedDate)
      )
        hit = { value: e.value };
    } else if (e.kind === "insertStatement" && e.file === file) {
      const inTarget =
        e.blockPath.length === 0 ||
        (e.blockPath.length === 1 && visible(segDate(e.blockPath[0]), selectedDate));
      if (inTarget && stmtKeyOf(e.statement) === key) hit = { value: stmtValOf(e.statement) };
    } else if (e.kind === "removeStatement" && e.file === file) {
      const inTarget =
        e.blockPath.length === 0 ||
        (e.blockPath.length === 1 && visible(segDate(e.blockPath[0]), selectedDate));
      if (inTarget && e.key === key && e.value == null) hit = { value: null };
    } else if (e.kind === "insertDatedBlock" && e.file === file) {
      if (!visible(e.date, selectedDate)) continue;
      for (const s of datedBodyScalars(e.statement)) if (s.key === key) hit = { value: s.value };
    }
  }
  return hit;
}

/**
 * Effective membership of a repeated history key (`add_accepted_culture`,
 * `add_government_reform`, …): the base list plus pending adds minus pending
 * removals, whether each edit is top-level or dated ≤ the selected date. When
 * the key has a dated inverse (`remove_accepted_culture`), pending inverse
 * statements subtract too.
 */
export function pendingHistList(
  queue: EditQueue,
  file: string,
  key: string,
  base: string[],
  selectedDate: string | null,
  inverseKey?: string,
): string[] {
  const out = base.slice();
  const push = (v: string) => {
    if (v && !out.includes(v)) out.push(v);
  };
  const drop = (v: string) => {
    const i = out.indexOf(v);
    if (i >= 0) out.splice(i, 1);
  };
  const applyPair = (k: string, v: string) => {
    if (k === key) push(v);
    else if (inverseKey && k === inverseKey) drop(v);
  };
  for (const e of queue.serialize()) {
    if (e.kind === "insertStatement" && e.file === file) {
      const inTarget =
        e.blockPath.length === 0 ||
        (e.blockPath.length === 1 && visible(segDate(e.blockPath[0]), selectedDate));
      if (inTarget) applyPair(stmtKeyOf(e.statement), stmtValOf(e.statement));
    } else if (e.kind === "removeStatement" && e.file === file && e.value != null) {
      const inTarget =
        e.blockPath.length === 0 ||
        (e.blockPath.length === 1 && visible(segDate(e.blockPath[0]), selectedDate));
      if (inTarget && e.key === key) drop(e.value);
    } else if (e.kind === "insertDatedBlock" && e.file === file) {
      if (!visible(e.date, selectedDate)) continue;
      for (const s of datedBodyScalars(e.statement)) applyPair(s.key, s.value);
    }
  }
  return out;
}

/**
 * Set a single top-level scalar field to `value`. Replaces the key in place when
 * `present`, else inserts a new statement — the byte-surgical split the writer
 * needs (setScalar requires the key to exist).
 */
export function scalarEdit(
  file: string,
  key: string,
  value: string,
  present: boolean,
  quoted = false,
): TypedEdit {
  return present
    ? { kind: "setScalar", file, path: [key], value, quoted }
    : { kind: "insertStatement", file, blockPath: [], statement: `${key} = ${value}` };
}

/** Remove a top-level key (no value filter) — e.g. national_focus = none. */
export function removeEdit(file: string, key: string): TypedEdit {
  return { kind: "removeStatement", file, blockPath: [], key };
}

/** Insert a `key = value` statement (list add: reforms, cultures, rivals). */
export function listAdd(file: string, key: string, value: string): TypedEdit {
  return { kind: "insertStatement", file, blockPath: [], statement: `${key} = ${value}` };
}

/** Remove the `key = value` statement matching `value` (list remove). */
export function listRemove(file: string, key: string, value: string): TypedEdit {
  return { kind: "removeStatement", file, blockPath: [], key, value };
}

/** Replace a `{ r g b }` block value (map color, revolutionary colors). */
export function blockEdit(file: string, key: string, value: string): TypedEdit {
  return { kind: "setBlock", file, path: [key], value };
}
