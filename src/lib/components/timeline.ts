// Data + intent contract for the reusable history Timeline component (Sprint 2.3).
//
// The Timeline is a *dumb* view: it renders dated blocks under a "1444 state"
// anchor and, on every edit gesture, emits a typed INTENT through its `onchange`
// prop. It never touches the pending-edit queue itself — the HOST maps each
// intent to one or more `TypedEdit`s (see edits.svelte.ts) using the addressing
// recipe below. This keeps the component shared between the province panel now
// and country history (rulers/heirs over time) later (SPRINT.md 2.3 final bullet).

import type { TypedEdit } from "$lib/edits.svelte";

/** One key/value row inside a dated block. */
export interface TimelineEntry {
  key: string;
  /** Raw scalar, or a reconstructed block-as-text (then `isBlock` is true). */
  value: string;
  /** True when the value was a `{ ... }` block (edited value is read-only). */
  isBlock?: boolean;
}

/** One dated block, mirroring the backend `DatedBlock`. */
export interface TimelineBlock {
  /** Date key exactly as written, e.g. "1453.5.29". */
  date: string;
  /** date > 1444.11.11 — badged "post-start", doesn't affect the 1444 render. */
  postStart: boolean;
  /** 0-based index among blocks sharing this exact date (file order). Load-
   *  bearing for edit addressing when dates duplicate — see the host recipe. */
  occurrenceIndex: number;
  entries: TimelineEntry[];
}

/**
 * A typed edit intent emitted by the Timeline. The host translates it to
 * `TypedEdit`s. All addressing the host needs is carried on the intent:
 * `date` + `occurrenceIndex` pick the block, `entryIndex` + `key`/`value` pick
 * the row (value disambiguates duplicate keys inside one block).
 *
 * Host mapping recipe (matches province_details.rs module docs):
 * - `addEntry`   → `insertStatement` at top level: `"<date> = { <key> = <value> }"`.
 *                  Append-only; display re-sorts by date. Always byte-safe.
 * - `editValue`  → on a UNIQUE-date block: `setScalar { path: [date, key] }`.
 *                  (occurrenceIndex 0 and no sibling shares the date.)
 * - `editEntry`  → remove the old row + insert the new one inside the block, or
 *                  setScalar when only the value changed under a unique date.
 * - `deleteEntry`→ `removeStatement { blockPath: [date], key, value }` (value
 *                  filter disambiguates duplicate keys within the block).
 * For a DUPLICATE-date block (occurrenceIndex > 0, or a sibling shares the date),
 * fine-grained addressing is not byte-safe with today's first-match writer; the
 * host should gate those edits (see module docs — occurrence-indexed paths are
 * the planned writer extension).
 */
export type TimelineIntent =
  | { kind: "addEntry"; date: string; key: string; value: string }
  | {
      kind: "editValue";
      date: string;
      occurrenceIndex: number;
      entryIndex: number;
      key: string;
      oldValue: string;
      value: string;
    }
  | {
      kind: "editEntry";
      date: string;
      occurrenceIndex: number;
      entryIndex: number;
      oldKey: string;
      oldValue: string;
      key: string;
      value: string;
    }
  | {
      kind: "deleteEntry";
      date: string;
      occurrenceIndex: number;
      entryIndex: number;
      key: string;
      value: string;
    };

/**
 * Maps a Timeline intent to TypedEdit(s), using occurrence-qualified addressing
 * (`<date>#<occurrenceIndex>`) so a duplicate-date block is byte-safe (the
 * mod_writer occurrence extension, Sprint 2.3). `#0` is exactly first-match, so
 * unique-date blocks work through the same uniform path. Shared by the province
 * panel and the country history timeline (S3.2) — both address dated blocks in
 * their own history file identically, so the host recipe lives with the Timeline
 * contract rather than in one panel's helpers.
 */
export function intentToEdits(file: string, intent: TimelineIntent): TypedEdit[] {
  switch (intent.kind) {
    case "addEntry":
      return [
        {
          kind: "insertStatement",
          file,
          blockPath: [],
          statement: `${intent.date} = { ${intent.key} = ${intent.value} }`,
        },
      ];
    case "editValue": {
      const block = `${intent.date}#${intent.occurrenceIndex}`;
      return [{ kind: "setScalar", file, path: [block, intent.key], value: intent.value, quoted: false }];
    }
    case "editEntry": {
      const block = `${intent.date}#${intent.occurrenceIndex}`;
      return [
        { kind: "removeStatement", file, blockPath: [block], key: intent.oldKey, value: intent.oldValue },
        { kind: "insertStatement", file, blockPath: [block], statement: `${intent.key} = ${intent.value}` },
      ];
    }
    case "deleteEntry": {
      const block = `${intent.date}#${intent.occurrenceIndex}`;
      return [{ kind: "removeStatement", file, blockPath: [block], key: intent.key, value: intent.value }];
    }
  }
}

/** Parse "Y.M.D" into a comparable tuple; missing parts default to 1. */
export function parseDate(s: string): [number, number, number] {
  const p = s.split(".");
  const n = (i: number) => {
    const v = parseInt(p[i], 10);
    return Number.isFinite(v) ? v : 1;
  };
  return [n(0), n(1), n(2)];
}

/** Sort key for chronological display (blocks with equal dates keep file order
 *  via occurrenceIndex). */
export function compareBlocks(a: TimelineBlock, b: TimelineBlock): number {
  const [ay, am, ad] = parseDate(a.date);
  const [by, bm, bd] = parseDate(b.date);
  return ay - by || am - bm || ad - bd || a.occurrenceIndex - b.occurrenceIndex;
}
