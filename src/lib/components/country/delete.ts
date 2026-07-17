// Country-deletion client model (Sprint S2.1). Mirrors the backend
// `country_delete::CountryBlastRadius` payload and provides the pure helpers the
// CountryPanel's delete flow needs:
//   * the blast-radius wire types (camelCase from serde),
//   * the predicate that finds a pending-created country's create composite in
//     the edit queue (so an unsaved country is deleted by dropping that
//     composite rather than through the backend deletion command).
//
// DOM-free and IPC-free so it reads like a spec by inspection.

import type { Composite, TypedEdit } from "$lib/edits.svelte";

/** The project-owned tag-registration file a country create scaffolds into. */
export const COUNTRY_TAG_FILE = "common/country_tags/zz_eutoolkit_countries.txt";

/** One diplomacy relation the deletion will remove (deleted tag's viewpoint). */
export interface BlastRelation {
  relationType: string;
  subjectType: string | null;
  /** `overlord` (subjects orphan), `subject` (overlord loses one), or the type. */
  role: string;
  /** The other country — the jump-link target. */
  partner: string | null;
  active: boolean;
}

/** One war the deleted tag participates in (jump-link warning row). */
export interface BlastWar {
  file: string;
  name: string | null;
  active: boolean;
  side: string | null;
  /** A belligerent on the opposite side — the jump-link target. */
  enemy: string | null;
}

/** The blast radius of deleting a country, mirroring the backend struct. */
export interface CountryBlastRadius {
  tag: string;
  isToolkitCreated: boolean;
  tagFile: string | null;
  ownedProvinces: number[];
  relations: BlastRelation[];
  activeWars: BlastWar[];
  historicalWars: BlastWar[];
  coreReferences: number[];
  tribalOwnerReferences: number[];
  countryFile: string | null;
  historyFile: string | null;
  flagFile: string;
  toolkitFiles: string[];
}

/**
 * True when `composite` is the create composite for `tag` — it appends the tag's
 * registration line to the toolkit tag file. Dropping this composite from the
 * queue deletes a pending-created (unsaved) country.
 */
export function isCreateCompositeFor(composite: Composite, tag: string): boolean {
  return composite.edits.some((e) => isTagRegistration(e, tag));
}

function isTagRegistration(e: TypedEdit, tag: string): boolean {
  if (e.kind !== "appendText" || e.file !== COUNTRY_TAG_FILE) return false;
  return new RegExp(`^\\s*${tag}\\s*=`).test(e.text);
}
