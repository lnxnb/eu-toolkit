// Country-panel starting-privilege fold (Sprint 20).
//
// The backend `get_country_estates` returns disk state folded to the selected
// date; this layers the session's pending (unsaved) grants/revocations on top so
// the section reflects edits live. Adds come in as `insertStatement`
// (`set_estate_privilege = X`) or `insertDatedBlock` (a `Y.M.D = { … }` body);
// revocations as `removeStatement` with a value filter.

import type { TypedEdit } from "$lib/edits.svelte";
import type { CountryEstates, StartingPrivilege } from "$lib/estates";

export const SET_PRIVILEGE = "set_estate_privilege";

const GRANT_RE = new RegExp(`\\b${SET_PRIVILEGE}\\s*=\\s*([A-Za-z0-9_]+)`, "g");

function grantsIn(text: string): string[] {
  const out: string[] = [];
  let m: RegExpExecArray | null;
  GRANT_RE.lastIndex = 0;
  while ((m = GRANT_RE.exec(text)) !== null) out.push(m[1]);
  return out;
}

/** Extract the dated block's date from an `insertDatedBlock` statement body. */
function dateOf(statement: string): string | null {
  return /^\s*(\d+\.\d+\.\d+)\s*=/.exec(statement)?.[1] ?? null;
}

/**
 * Fold `edits` (already gated to the selected date by the caller) over the
 * backend starting list, returning the effective grants. `estate`/`name` are
 * resolved from the backend catalog where possible.
 */
export function foldStartingPrivileges(
  base: CountryEstates,
  edits: TypedEdit[],
): StartingPrivilege[] {
  const out: StartingPrivilege[] = base.starting.map((s) => ({ ...s }));

  const estateOf = (priv: string): string | null => {
    for (const e of base.estates) if (e.privileges.some((p) => p.key === priv)) return e.key;
    return null;
  };
  const nameOf = (priv: string): string => {
    for (const e of base.estates) {
      const hit = e.privileges.find((p) => p.key === priv);
      if (hit) return hit.name;
    }
    return priv;
  };
  const add = (priv: string, date: string | null) => {
    if (out.some((s) => s.privilege === priv)) return;
    out.push({ privilege: priv, name: nameOf(priv), estate: estateOf(priv), date });
  };
  const remove = (priv: string) => {
    const i = out.findIndex((s) => s.privilege === priv);
    if (i >= 0) out.splice(i, 1);
  };

  for (const e of edits) {
    if (e.kind === "insertStatement" && e.file === base.file && e.blockPath.length === 0) {
      for (const g of grantsIn(e.statement)) add(g, null);
    } else if (e.kind === "insertStatement" && e.file === base.file && e.blockPath.length === 1) {
      // Merged into an existing dated block ("Y.M.D" segment).
      const d = /^(\d+\.\d+\.\d+)/.exec(e.blockPath[0])?.[1] ?? null;
      for (const g of grantsIn(e.statement)) add(g, d);
    } else if (e.kind === "insertDatedBlock" && e.file === base.file) {
      const d = dateOf(e.statement);
      for (const g of grantsIn(e.statement)) add(g, d);
    } else if (
      e.kind === "removeStatement" &&
      e.file === base.file &&
      e.key === SET_PRIVILEGE &&
      e.value != null
    ) {
      remove(e.value);
    }
  }
  return out;
}
