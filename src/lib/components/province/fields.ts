// Edit builders for the province panel (Sprint 2.2) + the timeline-intent →
// TypedEdit mapping (Sprint 2.3 host recipe). Province edits target the
// province's own `history/provinces/<id>.txt` file; geography edits splice
// membership lists in shared map/ files.

import type { EditQueue, TypedEdit } from "$lib/edits.svelte";
import type { GeoOption, DatedBlock } from "./types";
import { editAtDate, isShadowed, shadowedKeysFrom, type DatedBlockRef } from "$lib/editAtDate";
import { compareDates } from "$lib/calendar";

/**
 * Sprint 12.3 date context for a province's writers. When the selected date is
 * later than the start, a field write goes into the dated block for that date
 * (merge-or-insert) instead of the top level, and `foldStatements` mirrors the
 * write into the panel's local blocks so the shown effective state follows.
 */
export interface DateCtx {
  file: string;
  selectedDate: string | null;
  startDate: string;
  /** The file's existing dated blocks (for the merge-vs-insert decision). */
  blocks: DatedBlock[];
  /** Optimistically fold `key = value` statements into the local blocks. */
  foldStatements: (statements: string[]) => void;
}

/** True when the selected date is strictly after the mod's effective start. */
export function isLaterDate(ctx: DateCtx | null | undefined): boolean {
  return !!ctx && ctx.selectedDate != null && compareDates(ctx.selectedDate, ctx.startDate) > 0;
}

/**
 * True when writing `statements` at the selected date must land in a dated block
 * rather than at the top level — either because the date is past the start, or
 * because the file's own pre-start history already overrides those keys (see
 * editAtDate.ts). Sections that build differently-shaped top-level vs dated
 * edits (RebelsSection) branch on this rather than on `isLaterDate` alone.
 */
export function writesDatedBlock(
  ctx: DateCtx | null | undefined,
  statements: string[],
): boolean {
  if (!ctx || ctx.selectedDate == null) return false;
  if (isLaterDate(ctx)) return true;
  return isShadowed(statements, shadowedKeysFrom(blockRefs(ctx.blocks), ctx.selectedDate));
}

function blockRefs(blocks: DatedBlock[]): DatedBlockRef[] {
  return blocks.map((b) => ({
    date: b.date,
    occurrenceIndex: b.occurrence_index,
    keys: b.entries.map((e) => e.key),
  }));
}

/**
 * Route a field write through the date rule (Sprint 12.3). The target is decided
 * by `editAtDate`, NOT by the date comparison alone: a write at the start date
 * still lands in a dated block when the file's own history already overrides the
 * written keys before that date (timeline mods — see editAtDate.ts). When the
 * write does become dated we tag the composite with the date and fold the
 * statements into the panel's local blocks so the shown effective state follows.
 */
export function pushAtDate(
  queue: EditQueue,
  ctx: DateCtx | null | undefined,
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
  // `editAtDate` returns `startEdits` by identity when the top level is the
  // right target — that write is date-agnostic (it edits the baseline) and must
  // not be date-tagged or folded into the dated blocks.
  if (edits === startEdits) {
    queue.push({ label, edits, ...(coalesceKey ? { coalesceKey } : {}) });
    return;
  }
  queue.push({ label, edits, date: ctx.selectedDate, ...(coalesceKey ? { coalesceKey } : {}) });
  ctx.foldStatements(statements);
}

/** Set a top-level scalar: replace in place when present, else insert. */
export function scalarEdit(
  file: string,
  key: string,
  value: string,
  present: boolean,
  quoted = false,
): TypedEdit {
  return present
    ? { kind: "setScalar", file, path: [key], value, quoted }
    : { kind: "insertStatement", file, blockPath: [], statement: `${key} = ${quoted ? `"${value}"` : value}` };
}

/** Remove a top-level key (optionally value-filtered for repeated keys). */
export function removeEdit(file: string, key: string, value?: string): TypedEdit {
  return { kind: "removeStatement", file, blockPath: [], key, ...(value != null ? { value } : {}) };
}

/** Insert a repeated `key = value` statement (add_core, add_claim, discovered_by…). */
export function listAdd(file: string, key: string, value: string): TypedEdit {
  return { kind: "insertStatement", file, blockPath: [], statement: `${key} = ${value}` };
}

/** Remove the `key = value` statement matching `value` (list remove). */
export function listRemove(file: string, key: string, value: string): TypedEdit {
  return { kind: "removeStatement", file, blockPath: [], key, value };
}

/** Toggle a boolean flag/building `key = yes` (add when on, remove when off). */
export function toggleFlag(file: string, key: string, on: boolean, present: boolean): TypedEdit {
  return on
    ? (present
        ? { kind: "setScalar", file, path: [key], value: "yes", quoted: false }
        : { kind: "insertStatement", file, blockPath: [], statement: `${key} = yes` })
    : { kind: "removeStatement", file, blockPath: [] as string[], key };
}

/**
 * Move the province id between two membership id-lists in the shared map/ files
 * (area/continent/trade node/climate/winter/terrain). When only one side is
 * given it degrades to a pure add or remove.
 */
export function membershipMove(
  id: number,
  from: GeoOption | null,
  to: GeoOption | null,
): TypedEdit[] {
  const sid = String(id);
  if (from && to) {
    return [
      {
        kind: "listMove",
        fromFile: from.file,
        fromPath: from.list_path,
        toFile: to.file,
        toPath: to.list_path,
        id: sid,
      },
    ];
  }
  if (to) return [{ kind: "addId", file: to.file, listPath: to.list_path, id: sid }];
  if (from) return [{ kind: "removeId", file: from.file, listPath: from.list_path, id: sid }];
  return [];
}

/**
 * Pending-aware single-field operations bound to one province history `file`.
 * `val`/`edited` read the pending value over a display base (the effective-1444
 * value the game shows); `set` writes with an explicit `present` flag (whether
 * the key exists at the on-disk TOP level — that's what set-vs-insert needs,
 * not the effective value which may come from a dated block).
 */
export function fieldOps(queue: EditQueue, file: string, ctx?: DateCtx) {
  return {
    val(key: string, displayBase: string | null): string | null {
      // At a later date the write is a dated-block edit that the top-level
      // projection can't see; `displayBase` is the re-derived effective value
      // (which already folds the local dated block), so fall back to it.
      const p = queue.pendingField(file, key);
      return p !== undefined ? p.value : displayBase;
    },
    edited(key: string, displayBase: string | null): boolean {
      const p = queue.pendingField(file, key);
      return p !== undefined && p.value !== displayBase;
    },
    set(key: string, present: boolean, value: string, label: string, quoted = false) {
      pushAtDate(
        queue,
        ctx,
        label,
        [scalarEdit(file, key, value, present, quoted)],
        [`${key} = ${quoted ? `"${value}"` : value}`],
      );
    },
    clear(key: string, label: string) {
      // A scalar clear has no clean dated-block form (there is no "unset"); keep
      // the top-level removal (edits the base state) at any date.
      queue.push({ label, edits: [removeEdit(file, key)] });
    },
  };
}

// The Timeline intent → TypedEdit recipe now lives with the Timeline contract
// (shared by the province and country history timelines, S3.2). Re-exported here
// so the province panel's existing `import { intentToEdits } from "./fields"`
// keeps working unchanged.
export { intentToEdits } from "$lib/components/timeline";
