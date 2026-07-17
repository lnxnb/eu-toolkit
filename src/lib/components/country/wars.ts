// Wars-tab client model (Sprint 13.2/13.3). Mirrors the backend `wars::War`
// payload (see src-tauri/src/wars.rs) and provides the pure logic the Diplomacy
// tab's Wars section, the War panel, and MapView's occupation brushes share:
//   * active-at-date participant folding (mirrors `Participant::active_at`),
//   * enemy / belligerent tag-set derivation for the Occupy / Restore brushes,
//   * war-goal target-kind derivation (from the wargoal engine `type`),
//   * the byte-surgical TypedEdit builders (rename, war goal, participants,
//     new-war scaffold, delete).
//
// All of it is DOM-free and IPC-free so it reads like a spec by inspection.

import type { TypedEdit } from "$lib/edits.svelte";
import { compareDates } from "$lib/calendar";

/** An unmodeled `key = value` carried through untouched (preserve-unknown). */
export interface RawKv {
  key: string;
  value: string;
}

/** A war's `war_goal` block: modeled fields + preserved extras. */
export interface WarGoal {
  goal_type: string | null;
  casus_belli: string | null;
  province: number | null;
  tag: string | null;
  raw_extra: RawKv[];
}

/** One folded war participant. */
export interface Participant {
  tag: string;
  /** `attacker` | `defender`. */
  side: string;
  join_date: string | null;
  leave_date: string | null;
}

/** One war (one history/wars file), mirroring `wars::War`. */
export interface War {
  file: string;
  name: string | null;
  war_goal: WarGoal | null;
  participants: Participant[];
  battle_count: number;
  /** Active at the fetched date: both sides have an active participant. */
  active_at_date: boolean;
}

export const ATTACKER = "attacker";
export const DEFENDER = "defender";

/** The project-owned prefix a toolkit-scaffolded war file carries. */
export const TOOLKIT_WAR_PREFIX = "history/wars/zz_eutoolkit_";

// --- Active-at-date semantics (mirror the backend, Sprint 13.5) -----------

/**
 * A participant is active at `at` iff it joined ≤ at and has NOT left by then —
 * leaving *on* `at` counts as already gone (`leave > at`). Matches
 * `wars::Participant::active_at` exactly (Crusade of Varna's 1444.11.11 rems).
 */
export function participantActiveAt(p: Participant, at: string): boolean {
  const joined = p.join_date != null && compareDates(p.join_date, at) <= 0;
  const notLeft = p.leave_date == null || compareDates(p.leave_date, at) > 0;
  return joined && notLeft;
}

/** The side `tag` is on in `war` via its first ACTIVE participant, or null. */
export function activeSideOf(war: War, tag: string, at: string): string | null {
  const p = war.participants.find((p) => p.tag === tag && participantActiveAt(p, at));
  return p ? p.side : null;
}

/** The side `tag` is on via its first participant (active or not), or null. */
export function sideOf(war: War, tag: string): string | null {
  return war.participants.find((p) => p.tag === tag)?.side ?? null;
}

/** True when `tag` is an active belligerent in ≥1 war active at `at`. */
export function hasActiveWar(wars: War[], tag: string | null, at: string): boolean {
  if (!tag) return false;
  return wars.some((w) => w.active_at_date && activeSideOf(w, tag, at) !== null);
}

/**
 * Active enemy tags of `tag` across its active wars at `at` — the active
 * participants on the OPPOSITE side. This is the set whose owned land the Occupy
 * brush may paint `controller = tag` onto.
 */
export function enemyTags(wars: War[], tag: string | null, at: string): Set<string> {
  const out = new Set<string>();
  if (!tag) return out;
  for (const w of wars) {
    if (!w.active_at_date) continue;
    const side = activeSideOf(w, tag, at);
    if (!side) continue;
    const other = side === ATTACKER ? DEFENDER : ATTACKER;
    for (const p of w.participants) {
      if (p.side === other && participantActiveAt(p, at)) out.add(p.tag);
    }
  }
  return out;
}

/**
 * All active belligerent tags (BOTH sides) across `tag`'s active wars at `at` —
 * the set whose owned land the Restore-control brush may reset (either side of
 * the selected country's wars).
 */
export function belligerentTags(wars: War[], tag: string | null, at: string): Set<string> {
  const out = new Set<string>();
  if (!tag) return out;
  for (const w of wars) {
    if (!w.active_at_date) continue;
    if (activeSideOf(w, tag, at) === null) continue;
    for (const p of w.participants) {
      if (participantActiveAt(p, at)) out.add(p.tag);
    }
  }
  return out;
}

// --- War goal target-kind (from the wargoal engine `type`) ----------------

export type TargetKind = "province" | "tag" | "none";

/**
 * The target kind a wargoal engine `type` implies (documented in registry.rs):
 * `take_*`/`take_region` target a province, `defend_*` target a tag, and
 * `superiority`/`naval_superiority`/`blockade_ports` (and anything else) carry
 * no target.
 */
export function targetKindOf(engineType: string | null | undefined): TargetKind {
  if (!engineType) return "none";
  if (engineType.startsWith("take")) return "province";
  if (engineType.startsWith("defend")) return "tag";
  return "none";
}

/**
 * A registry entry's `raw` block field value (registry.rs serializes RawValue
 * untagged: a scalar is a string, a block is an array of {key, value}).
 */
type RawValue = string | { key: string | null; value: RawValue }[];
export interface WargoalRegistryEntry {
  key: string;
  name: string;
  source_file: string;
  raw?: RawValue;
}

/** The engine `type` scalar inside a wargoal_types entry's raw block, if any. */
export function engineTypeOf(entry: WargoalRegistryEntry): string | null {
  const raw = entry.raw;
  if (!Array.isArray(raw)) return null;
  const t = raw.find((i) => i.key === "type");
  return t && typeof t.value === "string" ? t.value : null;
}

// --- Edit builders (byte-surgical; see wars.rs module docs) ----------------

/** Rename the war — the literal quoted string in the file (SetScalar quoted). */
export function renameWarEdit(war: War, name: string): TypedEdit {
  return { kind: "setScalar", file: war.file, path: ["name"], value: name, quoted: true };
}

/**
 * Set a scalar inside the `war_goal` block (`type` / `casus_belli`): SetScalar
 * when the key is already present, else an InsertStatement into the block.
 */
export function warGoalScalarEdit(
  war: War,
  key: "type" | "casus_belli",
  value: string,
  present: boolean,
): TypedEdit {
  return present
    ? { kind: "setScalar", file: war.file, path: ["war_goal", key], value, quoted: false }
    : { kind: "insertStatement", file: war.file, blockPath: ["war_goal"], statement: `${key} = ${value}` };
}

/** Set the war-goal target (`province` / `tag`) inside the war_goal block. */
export function warGoalTargetEdit(
  war: War,
  key: "province" | "tag",
  value: string,
  present: boolean,
): TypedEdit {
  return present
    ? { kind: "setScalar", file: war.file, path: ["war_goal", key], value, quoted: false }
    : { kind: "insertStatement", file: war.file, blockPath: ["war_goal"], statement: `${key} = ${value}` };
}

/** Remove a stale war-goal target key (when the goal type's target kind flips). */
export function warGoalRemoveTargetEdit(war: War, key: "province" | "tag"): TypedEdit {
  return { kind: "removeStatement", file: war.file, blockPath: ["war_goal"], key, value: null };
}

/** The `add_*`/`rem_*` key for a side + join/leave. */
function partKey(side: string, joining: boolean): string {
  const verb = joining ? "add" : "rem";
  return `${verb}_${side === ATTACKER ? "attacker" : "defender"}`;
}

/** True when `date` already hosts a dated block in the war (some join/leave). */
function dateHasBlock(war: War, date: string): boolean {
  return war.participants.some((p) => p.join_date === date || p.leave_date === date);
}

/**
 * Add `tag` to `side` at its join `date`: merge an `add_*` statement into the
 * existing dated block when the war already has content at that date (vanilla
 * writes one block per date → occurrence 0), else insert a fresh `date = { … }`
 * block placed in date order.
 */
export function addParticipantEdit(war: War, side: string, tag: string, date: string): TypedEdit {
  const stmt = `${partKey(side, true)} = ${tag}`;
  return dateHasBlock(war, date)
    ? { kind: "insertStatement", file: war.file, blockPath: [date], statement: stmt }
    : { kind: "insertDatedBlock", file: war.file, date, statement: `${date} = { ${stmt} }` };
}

/**
 * Mark `tag` leaving `side` at `date` (`rem_*` into the leave-date block; merge
 * or insert as for adds). Used by "remove participant" (set a leave date).
 */
export function leaveParticipantEdit(war: War, side: string, tag: string, date: string): TypedEdit {
  const stmt = `${partKey(side, false)} = ${tag}`;
  return dateHasBlock(war, date)
    ? { kind: "insertStatement", file: war.file, blockPath: [date], statement: stmt }
    : { kind: "insertDatedBlock", file: war.file, date, statement: `${date} = { ${stmt} }` };
}

/**
 * Remove an `add_*`/`rem_*` statement for `tag` from the dated block at
 * `fromDate` (occurrence 0). Used when changing a participant's join/leave date.
 */
export function removePartStatementEdit(
  war: War,
  side: string,
  tag: string,
  joining: boolean,
  fromDate: string,
): TypedEdit {
  return {
    kind: "removeStatement",
    file: war.file,
    blockPath: [fromDate],
    key: partKey(side, joining),
    value: tag,
  };
}

// --- New war scaffold / delete --------------------------------------------

/** Slug for a toolkit-created war file (`zz_eutoolkit_<slug>.txt`). */
export function warSlug(name: string, fallback = "new_war"): string {
  return (
    name
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "_")
      .replace(/^_+|_+$/g, "") || fallback
  );
}

export interface NewWarSpec {
  name: string;
  attacker: string;
  defender: string;
  startDate: string;
  goalType: string;
  casusBelli: string;
  targetKind: TargetKind;
  targetProvince?: number | null;
  targetTag?: string | null;
}

/** The scaffold text for a brand-new war file (matches wars.rs' expected shape). */
export function newWarText(spec: NewWarSpec): string {
  const goalLines = [`\ttype = ${spec.goalType}`, `\tcasus_belli = ${spec.casusBelli}`];
  if (spec.targetKind === "province" && spec.targetProvince != null) {
    goalLines.push(`\tprovince = ${spec.targetProvince}`);
  } else if (spec.targetKind === "tag" && spec.targetTag) {
    goalLines.push(`\ttag = ${spec.targetTag}`);
  }
  return (
    `name = "${spec.name}"\n` +
    `war_goal = {\n${goalLines.join("\n")}\n}\n` +
    `${spec.startDate} = {\n\tadd_attacker = ${spec.attacker}\n\tadd_defender = ${spec.defender}\n}\n`
  );
}

/** The project-relative file path a new toolkit war scaffolds into. */
export function newWarFile(name: string): string {
  return `${TOOLKIT_WAR_PREFIX}${warSlug(name)}.txt`;
}

/** CreateFile edit that scaffolds a new war in the project's history/wars. */
export function newWarEdit(spec: NewWarSpec): TypedEdit {
  return { kind: "createFile", file: newWarFile(spec.name), text: newWarText(spec) };
}

/** True when the war resolves to a toolkit-created (project-only) file. */
export function isToolkitWar(war: War): boolean {
  return war.file.startsWith(TOOLKIT_WAR_PREFIX);
}

/** DeleteFile edit removing a toolkit-created war file outright. */
export function deleteWarEdit(war: War): TypedEdit {
  return { kind: "deleteFile", file: war.file };
}

// --- Pending projection (backend payload folded with the edit queue) -------
//
// get_wars reads from disk (no unsaved edits), so — like diplomacy's
// projectRelations — the displayed wars fold the pending queue: a scaffolded war
// (createFile) appears, a deleted/shadowed war disappears, and rename / war-goal
// / participant edits show before save. The war-file mini-parser handles the
// toolkit's own scaffold shape (no nested battle blocks); backend wars are
// folded statement-by-statement.

/** Extract `key = value` participant statements from a dated-block body. */
function foldBlockBody(participants: Participant[], date: string, body: string): void {
  for (const m of body.matchAll(/(add|rem)_(attacker|defender)\s*=\s*([A-Za-z0-9_]+)/g)) {
    const joining = m[1] === "add";
    const side = m[2] === "attacker" ? ATTACKER : DEFENDER;
    const tag = m[3];
    let p = participants.find((p) => p.tag === tag && p.side === side);
    if (!p) {
      p = { tag, side, join_date: null, leave_date: null };
      participants.push(p);
    }
    if (joining) {
      if (p.join_date == null) p.join_date = date;
    } else {
      p.leave_date = date;
    }
  }
}

/** Parse a toolkit war-file text (scaffold shape) into a War, or null. */
export function parseWarText(file: string, text: string, at: string): War | null {
  const nameM = text.match(/name\s*=\s*"([^"]*)"/);
  const name = nameM ? nameM[1] : null;
  let war_goal: WarGoal | null = null;
  const goalM = text.match(/war_goal\s*=\s*\{([^}]*)\}/);
  if (goalM) {
    const body = goalM[1];
    const g = (k: string) => body.match(new RegExp(`${k}\\s*=\\s*([A-Za-z0-9_]+)`))?.[1] ?? null;
    const provStr = g("province");
    war_goal = {
      goal_type: g("type"),
      casus_belli: g("casus_belli"),
      province: provStr ? parseInt(provStr, 10) : null,
      tag: g("tag"),
      raw_extra: [],
    };
  }
  const participants: Participant[] = [];
  for (const m of text.matchAll(/(\d+\.\d+\.\d+)\s*=\s*\{([\s\S]*?)\}/g)) {
    foldBlockBody(participants, m[1], m[2]);
  }
  if (name == null && participants.length === 0) return null; // empty/comment shadow
  return { file, name, war_goal, participants, battle_count: 0, active_at_date: computeActive(participants, at) };
}

/** Both-sides-have-an-active-participant rule (mirrors `war_active_at`). */
export function computeActive(participants: Participant[], at: string): boolean {
  const sideActive = (side: string) =>
    participants.some((p) => p.side === side && participantActiveAt(p, at));
  return sideActive(ATTACKER) && sideActive(DEFENDER);
}

/** A file the queue removes from the war list (DeleteFile or a shadow scaffold). */
function pendingRemovedWarFiles(edits: TypedEdit[], at: string): Set<string> {
  const removed = new Set<string>();
  for (const e of edits) {
    if (e.kind === "deleteFile" && e.file.startsWith("history/wars/")) removed.add(e.file);
    if (e.kind === "createFile" && e.file.startsWith("history/wars/")) {
      // A comment-only / nameless-participantless scaffold is a delete-shadow.
      if (parseWarText(e.file, e.text, at) === null) removed.add(e.file);
    }
  }
  return removed;
}

/** Wars scaffolded (createFile) by the queue — new toolkit wars. */
function pendingNewWars(edits: TypedEdit[], at: string): War[] {
  const out: War[] = [];
  for (const e of edits) {
    if (e.kind !== "createFile" || !e.file.startsWith("history/wars/")) continue;
    const w = parseWarText(e.file, e.text, at);
    if (w) out.push(w);
  }
  return out;
}

/** Fold the queue's edits to a single backend war (rename/goal/participants). */
function foldWar(war: War, edits: TypedEdit[], at: string): War {
  const participants = war.participants.map((p) => ({ ...p }));
  let name = war.name;
  const goal: WarGoal = war.war_goal
    ? { ...war.war_goal, raw_extra: war.war_goal.raw_extra }
    : { goal_type: null, casus_belli: null, province: null, tag: null, raw_extra: [] };
  let touchedGoal = war.war_goal != null;

  const setGoal = (k: string, v: string | null) => {
    touchedGoal = true;
    if (k === "type") goal.goal_type = v;
    else if (k === "casus_belli") goal.casus_belli = v;
    else if (k === "province") goal.province = v ? parseInt(v, 10) : null;
    else if (k === "tag") goal.tag = v;
  };

  for (const e of edits) {
    if (!("file" in e) || e.file !== war.file) continue;
    if (e.kind === "setScalar") {
      if (e.path.length === 1 && e.path[0] === "name") name = e.value;
      else if (e.path.length === 2 && e.path[0] === "war_goal") setGoal(e.path[1], e.value);
    } else if (e.kind === "insertStatement") {
      if (e.blockPath.length === 1 && e.blockPath[0] === "war_goal") {
        const m = e.statement.match(/([A-Za-z_]+)\s*=\s*([A-Za-z0-9_]+)/);
        if (m) setGoal(m[1], m[2]);
      } else if (e.blockPath.length === 1 && /^\d+\.\d+\.\d+/.test(e.blockPath[0])) {
        foldBlockBody(participants, e.blockPath[0], e.statement);
      }
    } else if (e.kind === "insertDatedBlock") {
      const m = e.statement.match(/^(\d+\.\d+\.\d+)\s*=\s*\{([\s\S]*)\}$/);
      if (m) foldBlockBody(participants, m[1], m[2]);
    } else if (e.kind === "removeStatement") {
      if (e.blockPath.length === 1 && e.blockPath[0] === "war_goal") setGoal(e.key, null);
      else if (e.blockPath.length === 1 && /^\d+\.\d+\.\d+/.test(e.blockPath[0])) {
        // Undo a participant statement at a date (join/leave date change).
        const side = e.key.endsWith("attacker") ? ATTACKER : DEFENDER;
        const joining = e.key.startsWith("add");
        const p = participants.find((p) => p.tag === e.value && p.side === side);
        if (p) {
          if (joining && p.join_date === e.blockPath[0]) p.join_date = null;
          else if (!joining && p.leave_date === e.blockPath[0]) p.leave_date = null;
        }
      }
    }
  }
  return {
    ...war,
    name,
    war_goal: touchedGoal ? goal : war.war_goal,
    participants,
    active_at_date: computeActive(participants, at),
  };
}

/**
 * Displayed wars for `tag`: the backend payload with pending renames / war-goal
 * edits / participant adds folded in, deleted/shadowed wars removed, and queued
 * new-war scaffolds involving `tag` appended. Derived purely from (backend +
 * queue) at date `at`. `tag` filters new scaffolds to those the country is in.
 */
export function projectWars(tag: string, backend: War[], edits: TypedEdit[], at: string): War[] {
  const removed = pendingRemovedWarFiles(edits, at);
  const folded = backend.filter((w) => !removed.has(w.file)).map((w) => foldWar(w, edits, at));
  const created = pendingNewWars(edits, at).filter(
    (w) => !removed.has(w.file) && w.participants.some((p) => p.tag === tag),
  );
  // A scaffold whose file already appears in backend (base-war shadow that also
  // adds content) shouldn't double up; new toolkit wars never collide with base.
  const seen = new Set(folded.map((w) => w.file));
  return [...folded, ...created.filter((w) => !seen.has(w.file))];
}

/**
 * Base-war deletion: EU4 has no "hide this war" lever, so the only mechanism is
 * a project file of the same name shadowing the base file. A comment-only shadow
 * parses to "no war" on the toolkit side (see `wars::all_wars_at`), so the war
 * vanishes from the tool. Whether the *game* tolerates an empty history/wars
 * file is unverified — the War panel surfaces this caveat before committing.
 */
export function shadowDeleteBaseWarEdit(war: War): TypedEdit {
  return {
    kind: "createFile",
    file: war.file,
    text: "# War removed by EU Toolkit (empty shadow hides the base file).\n",
  };
}
