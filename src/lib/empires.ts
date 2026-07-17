// Sprint 29 — Empires (HRE + Mandate). Wire types mirroring empires.rs plus the
// pending-edit folds so emperor successions / electors / members follow the edit
// queue before save (the same optimistic-fold pattern the rest of the app uses).
//
// Reforms / incidents / decrees are the hidden mechanics families (mechanics.ts /
// MechanicObjectEditor) — the overlay filters imperial_reforms by the `empire`
// scalar (hre vs celestial_empire) and renders `required_reform` as a jump link.

import type { TypedEdit } from "$lib/edits.svelte";
import { compareDates } from "$lib/calendar";
import { parseDate } from "$lib/components/timeline";

// ── Wire types (mirror empires.rs) ───────────────────────────────────────────

export interface EmperorEntry {
  date: string;
  tag: string;
  name: string;
  file: string;
  occurrenceIndex: number;
  postSelected: boolean;
  validTag: boolean;
  isSubject: boolean;
}
export interface EmperorTimeline {
  kind: string; // "hre" | "celestial"
  emperorKey: string; // "emperor" | "celestial_emperor"
  writeFile: string;
  writeFileExists: boolean;
  entries: EmperorEntry[];
  date: string;
  current: string | null;
  currentName: string | null;
}
export interface Elector {
  tag: string;
  name: string;
}
export interface ElectorCandidate {
  tag: string;
  name: string;
  historyFile: string;
}
export interface ElectorsData {
  electors: Elector[];
  candidates: ElectorCandidate[];
}
export interface HreMembers {
  provinceCount: number;
  provinceIds: number[];
  date: string;
}
export interface ReformScaffold {
  key: string;
  file: string;
  text: string;
  locEntries: { key: string; value: string }[];
}

// ── Emperor-timeline edit helpers ────────────────────────────────────────────

/** Add a succession: create the canonical file if it doesn't exist yet, else
 *  insert a date-ordered block. */
export function emperorAddEdits(tl: EmperorTimeline, date: string, tag: string): TypedEdit[] {
  const stmt = `${date} = { ${tl.emperorKey} = ${tag} }`;
  if (!tl.writeFileExists) {
    return [{ kind: "createFile", file: tl.writeFile, text: stmt + "\n" }];
  }
  return [{ kind: "insertDatedBlock", file: tl.writeFile, date, statement: stmt }];
}

/** Change an existing succession's tag in its own dated block (byte-surgical). */
export function emperorEditEdits(tl: EmperorTimeline, e: EmperorEntry, tag: string): TypedEdit[] {
  const seg = e.occurrenceIndex === 0 ? e.date : `${e.date}#${e.occurrenceIndex}`;
  return [{ kind: "setScalar", file: e.file, path: [seg, tl.emperorKey], value: tag, quoted: false }];
}

/** Remove a succession (delete just the emperor line; a shared dated block keeps
 *  its other keys). */
export function emperorRemoveEdits(tl: EmperorTimeline, e: EmperorEntry): TypedEdit[] {
  const seg = e.occurrenceIndex === 0 ? e.date : `${e.date}#${e.occurrenceIndex}`;
  return [{ kind: "removeStatement", file: e.file, blockPath: [seg], key: tl.emperorKey, value: e.tag }];
}

const EMPEROR_STMT_RE = (key: string) =>
  new RegExp(`\\b${key}\\s*=\\s*("?[A-Za-z0-9_-]+"?)`);

/** Fold the pending queue over a loaded emperor timeline so add/edit/remove show
 *  live. Recomputes `postSelected` + `current` against the selected date. */
export function foldEmperorTimeline(
  base: EmperorTimeline,
  edits: TypedEdit[],
  selectedDate: string,
): EmperorTimeline {
  const key = base.emperorKey;
  let entries = base.entries.map((e) => ({ ...e }));
  const stmtRe = EMPEROR_STMT_RE(key);

  const dateOf = (seg: string): [string, number] => {
    const hash = seg.indexOf("#");
    return hash >= 0 ? [seg.slice(0, hash), Number(seg.slice(hash + 1))] : [seg, 0];
  };

  for (const e of edits) {
    if (e.kind === "createFile" && e.file === base.writeFile) {
      // Parse each `Y.M.D = { key = TAG }` block from the created file text.
      const re = /(\d+\.\d+\.\d+)\s*=\s*\{([^}]*)\}/g;
      let m: RegExpExecArray | null;
      while ((m = re.exec(e.text)) !== null) {
        const tagM = stmtRe.exec(m[2]);
        if (tagM) entries.push(mkEntry(m[1], tagM[1].replace(/"/g, ""), e.file));
      }
    } else if (e.kind === "insertDatedBlock" && e.file === base.writeFile) {
      const open = e.statement.indexOf("{");
      const close = e.statement.lastIndexOf("}");
      const inner = open >= 0 ? e.statement.slice(open + 1, close) : "";
      const tagM = stmtRe.exec(inner);
      if (tagM) entries.push(mkEntry(e.date, tagM[1].replace(/"/g, ""), e.file));
    } else if (e.kind === "setScalar" && e.path.length === 2 && e.path[1] === key) {
      const [d, occ] = dateOf(e.path[0]);
      const hit = entries.find((x) => x.file === e.file && x.date === d && x.occurrenceIndex === occ);
      if (hit) hit.tag = e.value;
    } else if (e.kind === "removeStatement" && e.key === key && e.blockPath.length === 1) {
      const [d, occ] = dateOf(e.blockPath[0]);
      entries = entries.filter((x) => !(x.file === e.file && x.date === d && x.occurrenceIndex === occ));
    } else if (e.kind === "insertStatement" && e.file === base.writeFile && e.blockPath.length === 0) {
      // Timeline addEntry recipe (`Y.M.D = { key = TAG }`).
      const m = /(\d+\.\d+\.\d+)\s*=\s*\{([^}]*)\}/.exec(e.statement);
      if (m) {
        const tagM = stmtRe.exec(m[2]);
        if (tagM) entries.push(mkEntry(m[1], tagM[1].replace(/"/g, ""), e.file));
      }
    }
  }

  entries.sort((a, b) => compareDates(a.date, b.date) || a.occurrenceIndex - b.occurrenceIndex);
  let current: string | null = null;
  for (const e of entries) {
    e.postSelected = compareDates(e.date, selectedDate) > 0;
    const isNone = e.tag.replace(/-/g, "").length === 0;
    e.name = isNone ? e.tag : e.name || e.tag;
    if (!e.postSelected) current = isNone ? null : e.tag;
  }
  const cur = entries.find((e) => e.tag === current && !e.postSelected);
  return { ...base, entries, current, currentName: cur?.name ?? current };
}

function mkEntry(date: string, tag: string, file: string): EmperorEntry {
  return {
    date,
    tag,
    name: tag,
    file,
    occurrenceIndex: 0,
    postSelected: false,
    validTag: true,
    isSubject: false,
  };
}

// ── Electors fold ────────────────────────────────────────────────────────────

const TAG_OF_FILE = /history\/countries\/([A-Za-z0-9]{3})[ ._-]/;
function tagOfCountryFile(file: string): string | null {
  const m = TAG_OF_FILE.exec(file);
  return m ? m[1].toUpperCase() : null;
}

/** Fold pending `elector` toggles (country history edits ≤ selected date) over
 *  the backend elector list. Handles the country-panel toggle's shapes:
 *  insertStatement `elector = yes` / removeStatement `elector` (start date) and
 *  dated blocks / setScalar (later dates). */
export function foldElectors(
  base: Elector[],
  edits: TypedEdit[],
  nameOf: (tag: string) => string,
): Elector[] {
  const on = new Map<string, boolean>();
  for (const e of base) on.set(e.tag, true);
  for (const e of edits) {
    const file = "file" in e ? (e.file as string) : "";
    const tag = tagOfCountryFile(file);
    if (!tag) continue;
    if (e.kind === "insertStatement") {
      if (/\belector\s*=\s*yes/.test(e.statement)) on.set(tag, true);
      else if (/\belector\s*=\s*no/.test(e.statement)) on.set(tag, false);
    } else if (e.kind === "insertDatedBlock") {
      if (/\belector\s*=\s*yes/.test(e.statement)) on.set(tag, true);
      else if (/\belector\s*=\s*no/.test(e.statement)) on.set(tag, false);
    } else if (e.kind === "setScalar" && e.path[e.path.length - 1] === "elector") {
      on.set(tag, e.value.trim() === "yes");
    } else if (e.kind === "removeStatement" && e.key === "elector") {
      on.set(tag, false);
    }
  }
  const out: Elector[] = [];
  for (const [tag, isOn] of on) if (isOn) out.push({ tag, name: nameOf(tag) });
  out.sort((a, b) => a.tag.localeCompare(b.tag));
  return out;
}

// ── Members fold ─────────────────────────────────────────────────────────────

const PROV_OF_FILE = /history\/provinces\/(\d+)[ ._-]/;
function provOfFile(file: string): number | null {
  const m = PROV_OF_FILE.exec(file);
  return m ? Number(m[1]) : null;
}

/** Fold pending `hre` province toggles (≤ selected date) over the member id set. */
export function foldMembers(base: HreMembers, edits: TypedEdit[]): HreMembers {
  const on = new Set<number>(base.provinceIds);
  for (const e of edits) {
    const file = "file" in e ? (e.file as string) : "";
    const id = provOfFile(file);
    if (id == null) continue;
    const setYes = () => on.add(id);
    const setNo = () => on.delete(id);
    if (e.kind === "insertStatement" || e.kind === "insertDatedBlock") {
      if (/\bhre\s*=\s*yes/.test(e.statement)) setYes();
      else if (/\bhre\s*=\s*no/.test(e.statement)) setNo();
    } else if (e.kind === "setScalar" && e.path[e.path.length - 1] === "hre") {
      e.value.trim() === "yes" ? setYes() : setNo();
    } else if (e.kind === "removeStatement" && e.key === "hre") {
      setNo();
    }
  }
  const ids = Array.from(on).sort((a, b) => a - b);
  return { ...base, provinceCount: ids.length, provinceIds: ids };
}

/** Visibility gate matching the map overlays: a composite with no date always
 *  applies; a dated composite applies only when its date ≤ the selected date. */
export function visibleAt(selectedDate: string | null) {
  return (compositeDate: string | undefined): boolean =>
    !compositeDate || selectedDate == null || compareDates(compositeDate, selectedDate) <= 0;
}

/** Reforms in file/progression order filtered to one empire key. */
export function reformOrder(objects: { scalars: { key: string; value: string; present: boolean }[] }[], empire: string) {
  return objects.filter((o) => o.scalars.some((s) => s.key === "empire" && s.value === empire));
}

// silence unused import in some build configs
void parseDate;
