// Shared pending-edit helpers for the Ruler / Queen / Heir sections (Sprint 1.2).
//
// Character fields live at a nested block path in the history file, e.g. the
// monarch's adm is [<date>, "monarch", "adm"]. The EditQueue's built-in
// projections only fold TOP-level fields, so these helpers re-fold the queue for
// an arbitrary block path (setScalar / insertStatement / removeStatement, later
// wins) exactly the way the backend applies it, and build the matching edits
// (present ⇒ setScalar, absent ⇒ insert; toggles/overrides via insert/remove).

import type { EditQueue, TypedEdit } from "$lib/edits.svelte";

export function samePath(a: string[], b: string[]): boolean {
  return a.length === b.length && a.every((v, i) => v === b[i]);
}

function stmtKey(s: string): string {
  const eq = s.indexOf("=");
  return eq < 0 ? "" : s.slice(0, eq).trim();
}
function stmtVal(s: string): string {
  const eq = s.indexOf("=");
  return eq < 0 ? "" : s.slice(eq + 1).trim();
}

/**
 * Effective pending value of a scalar `key` inside `blockPath` of `file`,
 * folding setScalar / insertStatement / removeStatement. Returns `undefined`
 * when the queue doesn't touch it (caller falls back to base); `null` = a
 * pending removal.
 */
export function charField(
  queue: EditQueue,
  file: string,
  blockPath: string[],
  key: string,
): { value: string | null } | undefined {
  const path = [...blockPath, key];
  const hit = queue.findLast((e) => {
    if (e.kind === "setScalar") return e.file === file && samePath(e.path, path);
    if (e.kind === "insertStatement")
      return e.file === file && samePath(e.blockPath, blockPath) && stmtKey(e.statement) === key;
    if (e.kind === "removeStatement")
      return (
        e.file === file &&
        samePath(e.blockPath, blockPath) &&
        e.key === key &&
        e.value == null
      );
    return false;
  });
  if (!hit) return undefined;
  if (hit.kind === "removeStatement") return { value: null };
  if (hit.kind === "setScalar") return { value: hit.value };
  if (hit.kind === "insertStatement") return { value: stmtVal(hit.statement) };
  return undefined;
}

/** Effective value (pending or base) of a character scalar field. */
export function charValue(
  queue: EditQueue,
  file: string,
  blockPath: string[],
  key: string,
  base: string | null,
): string | null {
  const p = charField(queue, file, blockPath, key);
  return p !== undefined ? p.value : base;
}

/** True when a pending edit changes `key` away from its on-disk value. */
export function charEdited(
  queue: EditQueue,
  file: string,
  blockPath: string[],
  key: string,
  base: string | null,
): boolean {
  const p = charField(queue, file, blockPath, key);
  return p !== undefined && p.value !== base;
}

/**
 * Build the edit that sets `key` inside `blockPath` to `value`: replace in place
 * when the key is present on disk, else insert a new statement into the block.
 */
export function setCharEdit(
  file: string,
  blockPath: string[],
  key: string,
  value: string,
  present: boolean,
  quoted = false,
): TypedEdit {
  return present
    ? { kind: "setScalar", file, path: [...blockPath, key], value, quoted }
    : {
        kind: "insertStatement",
        file,
        blockPath,
        statement: `${key} = ${quoted ? `"${value}"` : value}`,
      };
}

/** Remove key `key` from `blockPath` (e.g. clearing a female/regent flag). */
export function removeCharEdit(file: string, blockPath: string[], key: string): TypedEdit {
  return { kind: "removeStatement", file, blockPath, key, value: null };
}

/**
 * Effective personality keys for a character: base personalities, minus pending
 * removals (removeStatement at their dated block), plus pending inserts.
 */
export function effectivePersonalities(
  queue: EditQueue,
  file: string,
  effectKey: string,
  base: { key: string; date: string }[],
): string[] {
  const out = base.map((p) => p.key);
  for (const e of queue.serialize()) {
    if (
      e.kind === "insertStatement" &&
      e.file === file &&
      stmtKey(e.statement) === effectKey
    ) {
      const v = stmtVal(e.statement);
      if (v && !out.includes(v)) out.push(v);
    } else if (
      e.kind === "removeStatement" &&
      e.file === file &&
      e.key === effectKey &&
      e.value != null
    ) {
      const i = out.indexOf(e.value);
      if (i >= 0) out.splice(i, 1);
    }
  }
  return out;
}

/** Insert an `add_*_personality = key` into the character's dated block. */
export function addPersonalityEdit(
  file: string,
  date: string,
  effectKey: string,
  key: string,
): TypedEdit {
  return { kind: "insertStatement", file, blockPath: [date], statement: `${effectKey} = ${key}` };
}

/** Remove an `add_*_personality = key` from the dated block it lives in. */
export function removePersonalityEdit(
  file: string,
  date: string,
  effectKey: string,
  key: string,
): TypedEdit {
  return { kind: "removeStatement", file, blockPath: [date], key: effectKey, value: key };
}

/**
 * Whether a whole holder block (`monarch`/`queen`/`heir`) exists after pending
 * edits: base presence, flipped by the last pending insert/remove of that key.
 * A holder created at a later date arrives as an `insertDatedBlock` whose body
 * is `<date> = { <holder> = { … } }` (Sprint 12.3), so we detect that too.
 */
export function holderExists(
  queue: EditQueue,
  file: string,
  holderKey: string,
  base: boolean,
): boolean {
  const holderRe = new RegExp(`(^|[\\s{])${holderKey}\\s*=`);
  const hit = queue.findLast((e) => {
    if (e.kind === "insertStatement" && e.file === file)
      return stmtKey(e.statement) === holderKey;
    if (e.kind === "insertDatedBlock" && e.file === file) {
      const open = e.statement.indexOf("{");
      return open >= 0 && holderRe.test(e.statement.slice(open + 1));
    }
    if (e.kind === "removeStatement" && e.file === file)
      return e.key === holderKey && e.value == null;
    return false;
  });
  if (!hit) return base;
  return hit.kind !== "removeStatement";
}

/**
 * Build the edit that creates a whole holder block (`monarch`/`queen`/`heir`) as
 * a date-ordered dated block, generalizing the old hardwired `1444.11.11` create
 * (Sprint 12.3): `<date> = { <holder> = { <fields> } }`. `date` is the selected
 * view date, or the effective start when at/ before it.
 */
export function createHolderEdit(
  file: string,
  date: string,
  holder: string,
  fields: string,
): TypedEdit {
  return {
    kind: "insertDatedBlock",
    file,
    date,
    statement: `${date} = { ${holder} = { ${fields} } }`,
  };
}

/** The date a "create at this date" holder/history write lands on: the selected
 *  view date, else the effective start. */
export function createAtDate(selectedDate: string | null, startDate: string): string {
  return selectedDate ?? startDate;
}
