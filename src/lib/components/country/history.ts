// S3.2 — country history timeline helpers (pure). The backend surfaces each
// dated `Y.M.D = { ... }` block of a country history file as a `DatedBlock`
// (date, post_start, occurrence_index, entries[{key,value,is_block}]). These
// helpers classify those entries into the typed editors the panel renders
// (rulers reuse the existing CharacterCore; advisors get typed rows) and build
// the byte-surgical edits for adding/editing them — everything else falls
// through to the generic Timeline component.

import type { TypedEdit } from "$lib/edits.svelte";
import type { LeaderInfo, Personality } from "./types";
import { parseStatements, datedBlockSegment } from "$lib/editAtDate";

/** One key/value row of a dated block, mirroring the backend `RawStatement`. */
export interface RawEntry {
  key: string;
  value: string;
  is_block: boolean;
}

/** One dated block, mirroring the backend `DatedBlock`. */
export interface CountryDatedBlock {
  date: string;
  post_start: boolean;
  occurrence_index: number;
  entries: RawEntry[];
}

/** Holder roles that reuse the ruler editor fields (CharacterCore). */
export const HOLDER_KEYS = ["monarch", "queen", "heir"] as const;
export type HolderKey = (typeof HOLDER_KEYS)[number];

/** The `add_*_personality` effect key that pairs with a holder role. */
export function personalityEffectFor(holder: HolderKey): string {
  return holder === "monarch"
    ? "add_ruler_personality"
    : holder === "queen"
      ? "add_queen_personality"
      : "add_heir_personality";
}

/** Strip a leading `{` and trailing `}` from a reconstructed block value. */
export function innerOf(text: string): string {
  const open = text.indexOf("{");
  const close = text.lastIndexOf("}");
  return open >= 0 && close > open ? text.slice(open + 1, close) : text;
}

/** Strip surrounding double quotes from a scalar value. */
export function unquote(v: string): string {
  return v.length >= 2 && v.startsWith('"') && v.endsWith('"') ? v.slice(1, -1) : v;
}

/** Parse the flat `key = value` scalars of a reconstructed block body into a map
 *  (last wins). Nested `{ … }` values are skipped — callers that need them (the
 *  leader block) pull the raw value separately. */
export function scalarMap(blockText: string): Map<string, string> {
  const out = new Map<string, string>();
  for (const { key, value } of parseStatements(innerOf(blockText))) {
    if (!value.startsWith("{")) out.set(key, value);
  }
  return out;
}

function num(m: Map<string, string>, k: string): number | null {
  const v = m.get(k);
  if (v == null) return null;
  const n = parseInt(v, 10);
  return Number.isFinite(n) ? n : null;
}

/** Extract the `leader = { … }` sub-block's stats from a holder block body. */
function parseLeader(blockText: string): LeaderInfo | null {
  const inner = innerOf(blockText);
  const m = /leader\s*=\s*\{([^{}]*)\}/.exec(inner);
  if (!m) return null;
  const lm = new Map<string, string>();
  for (const { key, value } of parseStatements(m[1])) lm.set(key, value);
  const n = (k: string) => {
    const v = lm.get(k);
    const p = v != null ? parseInt(v, 10) : NaN;
    return Number.isFinite(p) ? p : null;
  };
  return { fire: n("fire"), shock: n("shock"), manuever: n("manuever"), siege: n("siege") };
}

/**
 * The CharCommon a reused CharacterCore needs for one holder block. `date` is
 * the occurrence-qualified segment (`Y.M.D#occ`) so CharacterCore's reads and
 * writes address the right dated block byte-safely. `personalities` are the
 * matching `add_*_personality` effects that are SIBLINGS of the holder in the
 * same dated block (they live beside, not inside, the holder block).
 */
export function holderChar(block: CountryDatedBlock, holder: HolderKey, entryValue: string) {
  const m = scalarMap(entryValue);
  const seg = datedBlockSegment(block.date, block.occurrence_index);
  const effect = personalityEffectFor(holder);
  const personalities: Personality[] = block.entries
    .filter((e) => e.key === effect && !e.is_block)
    .map((e) => ({ key: e.value, date: seg }));
  return {
    date: seg,
    name: m.has("name") ? unquote(m.get("name")!) : null,
    dynasty: m.has("dynasty") ? unquote(m.get("dynasty")!) : null,
    adm: num(m, "adm"),
    dip: num(m, "dip"),
    mil: num(m, "mil"),
    birth_date: m.get("birth_date") ?? null,
    female: m.get("female") === "yes",
    culture: m.get("culture") ?? null,
    religion: m.get("religion") ?? null,
    personalities,
    leader: parseLeader(entryValue),
  };
}

/** A typed view of one advisor sub-block for the advisor editor rows. */
export interface AdvisorView {
  /** The dated block the advisor lives in. */
  date: string;
  occurrenceIndex: number;
  name: string | null;
  type: string | null;
  skill: number | null;
  advisorDate: string | null;
  deathDate: string | null;
  location: string | null;
  /** Which scalar keys exist on disk (drives set-vs-insert for edits). */
  present: Set<string>;
}

/** Parse an `advisor = { … }` entry into a typed view. */
export function advisorView(block: CountryDatedBlock, entryValue: string): AdvisorView {
  const m = scalarMap(entryValue);
  return {
    date: block.date,
    occurrenceIndex: block.occurrence_index,
    name: m.has("name") ? unquote(m.get("name")!) : null,
    type: m.get("type") ?? null,
    skill: num(m, "skill"),
    advisorDate: m.get("date") ?? null,
    deathDate: m.get("death_date") ?? null,
    location: m.get("location") ?? null,
    present: new Set(m.keys()),
  };
}

/** Fields for a new historical advisor. */
export interface NewAdvisor {
  name: string;
  type: string;
  skill: number;
  date: string;
  deathDate?: string;
  location?: string;
}

/** The `advisor = { … }` block body text for a new advisor (Latin-1 safe). */
export function advisorBody(a: NewAdvisor): string {
  const parts = [`name = "${a.name}"`, `type = ${a.type}`, `skill = ${a.skill}`, `date = ${a.date}`];
  if (a.deathDate) parts.push(`death_date = ${a.deathDate}`);
  if (a.location) parts.push(`location = ${a.location}`);
  return `advisor = { ${parts.join(" ")} }`;
}

/**
 * Edits that place an `advisor = { … }` block into the dated block for `date`:
 * merge into the last existing block for that date, else insert a fresh
 * date-ordered `Y.M.D = { advisor = { … } }` block (an advisor is inherently
 * dated, so it always lands in a dated block regardless of the panel's start).
 */
export function addAdvisorEdits(
  file: string,
  date: string,
  blocks: CountryDatedBlock[],
  body: string,
): TypedEdit[] {
  const matches = blocks.filter((b) => b.date === date);
  if (matches.length > 0) {
    const occ = matches.reduce((mx, b) => Math.max(mx, b.occurrence_index), 0);
    return [{ kind: "insertStatement", file, blockPath: [datedBlockSegment(date, occ)], statement: body }];
  }
  return [{ kind: "insertDatedBlock", file, date, statement: `${date} = { ${body} }` }];
}
