// Dynamic province names (Sprint 24): shared types + pure edit-builders for the
// common/province_names/<culture|culture_group|TAG>.txt files. Entries are
// literal Windows-1252 strings (NOT loc keys): `<id> = "Name"` or the capital
// pair variant `<id> = { "Name" "Capital" }`. The backend reads them
// byte-faithfully; the writer re-encodes new names as Latin-1.

import type { TypedEdit } from "$lib/edits.svelte";

export interface ProvinceNameEntry {
  id: number;
  name: string;
  /** Present only for the `{ "Name" "Capital" }` pair variant. */
  capital: string | null;
}

export interface ProvinceNamesFile {
  key: string;
  source_file: string;
  exists: boolean;
  entries: ProvinceNameEntry[];
}

export interface ProvinceNameAssignment {
  key: string;
  label: string;
  /** "culture" | "group" | "tag". */
  kind: string;
  name: string;
  capital: string | null;
  source_file: string;
}

/** Inner tokens of a pair value: `"Name" "Capital"`. */
export function pairValue(name: string, capital: string): string {
  return `"${name}" "${capital}"`;
}

/** A full `<id> = ...` statement (single or pair variant), authored at column 0. */
export function entryStatement(e: ProvinceNameEntry): string {
  return e.capital != null && e.capital !== ""
    ? `${e.id} = { ${pairValue(e.name, e.capital)} }`
    : `${e.id} = "${e.name}"`;
}

/** Whole-file text for a brand-new file (regime A / CreateFile), in id order. */
export function buildFileText(entries: ProvinceNameEntry[]): string {
  const sorted = [...entries].sort((a, b) => a.id - b.id);
  return sorted.map(entryStatement).join("\n") + (sorted.length ? "\n" : "");
}

/** Normalizes a raw capital field to null (absent) or a trimmed string. */
export function normCapital(capital: string | null | undefined): string | null {
  const c = (capital ?? "").trim();
  return c.length ? c : null;
}

/** Surgical edit(s) for editing an existing (on-buffer) entry. `hadCapital` is
 *  the entry's CURRENT on-buffer shape. Same-shape edits are idempotent
 *  (SetScalar / SetBlock); a shape change is a Remove + Insert. */
export function editEntryEdits(
  file: string,
  e: ProvinceNameEntry,
  hadCapital: boolean,
): TypedEdit[] {
  const hasCapital = e.capital != null && e.capital !== "";
  if (hasCapital === hadCapital) {
    return hasCapital
      ? [{ kind: "setBlock", file, path: [String(e.id)], value: pairValue(e.name, e.capital!) }]
      : [{ kind: "setScalar", file, path: [String(e.id)], value: e.name, quoted: true }];
  }
  return [
    { kind: "removeStatement", file, blockPath: [], key: String(e.id) },
    { kind: "insertStatement", file, blockPath: [], statement: entryStatement(e) },
  ];
}
