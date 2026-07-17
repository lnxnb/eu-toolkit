// Edit builders shared by the country panel sections (Sprint 1.2). Each returns
// TypedEdit(s) for the pending-edit queue; presence decides set-vs-insert.

import type { TypedEdit } from "$lib/edits.svelte";

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
