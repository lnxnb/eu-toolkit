// Sprint 26 — Country-interior mechanics pack (config-driven definition editor).
//
// Mirrors the `get_mechanics` wire payload (mechanics.rs) and folds the typed
// edit queue over the base so create/delete appear live in the list (undo/redo
// revert them). Per-field edits (typed scalars, modifier blocks, self-modifier
// rows, trigger/effect trees, loc name/desc) are read at render time through the
// queue's pending helpers / parse_script_block_with_edits, so they aren't folded
// here. The family config is served by the backend (`get_mechanic_families` /
// each `MechanicsData.meta`) — the frontend renders generically from it.

import type { TypedEdit } from "$lib/edits.svelte";

// ── Wire types (mirror mechanics.rs) ─────────────────────────────────────────

export interface Scalar {
  key: string;
  kind: string; // "num" | "int" | "bool" | "enum" | "str" | "token"
  present: boolean;
  value: string;
  options: string[];
  picker: string; // "" | "building" | "trade_good" | "province"
}
export interface ModRow {
  key: string;
  value: string;
}
// Sprint 27 — a bare-token list field (buildings' manufactory).
export interface ListField {
  name: string;
  present: boolean;
  picker: string; // "trade_good" | …
  tokens: string[];
}
export interface ModifierBlock {
  name: string;
  present: boolean;
  flat: boolean;
  rows: ModRow[];
}
export interface ScriptBlockRef {
  name: string;
  registry: string; // "triggers" | "effects"
  present: boolean;
}
export interface ReformStep {
  key: string;
  flat: boolean;
  rows: ModRow[];
}
export interface EventRef {
  key: string;
  id: string;
}
// Sprint 27 Wave 3 — one child of a sub-group container (age objective/ability).
export interface SubEntry {
  key: string;
  name: string;
  modifierBlocks: ModifierBlock[];
  scriptBlocks: ScriptBlockRef[];
  rawExtra: string[];
  raw: string;
}
export interface SubGroupData {
  container: string;
  label: string;
  childIsTrigger: boolean;
  childModifiers: string[];
  childScripts: ScriptBlockRef[];
  entries: SubEntry[];
  childScaffold: string;
}
export interface MechanicObject {
  family: string;
  key: string;
  /** Occurrence-qualified edit path segment (`key`, or `key#n` for a de-duped
   *  family — subject types forward-declare then define). Sprint 27 W2. */
  editKey: string;
  file: string;
  origin: string; // "base" | "mod"
  name: string;
  nameKey: string;
  descKey: string;
  descLoc: string | null;
  icon: string | null;
  iconKind: string; // "none" | "sprite"
  color: [number, number, number] | null;
  scalars: Scalar[];
  modifierBlocks: ModifierBlock[];
  listFields: ListField[];
  selfModifier: boolean;
  selfRows: ModRow[];
  scriptBlocks: ScriptBlockRef[];
  ordered: boolean;
  orderedChildren: ReformStep[];
  subGroups: SubGroupData[];
  eventRefs: EventRef[];
  group: string | null;
  rawExtra: string[];
  raw: string;
}
export interface FamilyMeta {
  id: string;
  label: string;
  projectFile: string;
  hasColor: boolean;
  iconKind: string;
  selfModifier: boolean;
  ordered: boolean;
  groupNested: boolean;
  availTrigger: string; // "" = none
  scriptBlocks: ScriptBlockRef[];
  scalars: Scalar[];
  modifiers: string[];
  listFields: ListField[];
  iconGfx: boolean;
  descSuffix: boolean;
  /** Whether to offer a "＋ new…" create affordance (false for hardcoded
   *  families like AI personalities). Sprint 27 W5. */
  allowCreate: boolean;
  subGroups: SubGroupData[];
}
export interface MechanicsData {
  meta: FamilyMeta;
  objects: MechanicObject[];
}
export interface LocEntry {
  key: string;
  value: string;
}
export interface Scaffold {
  key: string;
  file: string;
  text: string;
  locEntries: LocEntry[];
  group: string | null;
  groupNested: boolean;
  // Sprint 27 — named-sprite emission (buildings/institutions).
  gfxFile: string | null;
  gfxText: string | null;
}
export interface MechanicEventRef {
  file: string;
  origin: string;
  count: number;
}

// ── Key helpers ───────────────────────────────────────────────────────────────

const KEY_RE = /^[a-z][a-z0-9_]*$/;
export function isValidKey(key: string): boolean {
  return KEY_RE.test(key);
}
export function slugify(name: string): string {
  const base = name
    .toLowerCase()
    .normalize("NFKD")
    .replace(/[^a-z0-9]+/g, "_")
    .replace(/^_+|_+$/g, "");
  return base || "key";
}

// ── Scaffold-text parsing (for the create fold) ───────────────────────────────

function objectKeyOf(text: string): string | null {
  return /^\s*([A-Za-z0-9_]+)\s*=/.exec(text)?.[1] ?? null;
}

/** Builds a minimal MechanicObject from a freshly-scaffolded block body, using
 *  the family meta so the created row renders with the same typed surface. */
export function parseScaffoldObject(
  text: string,
  file: string,
  meta: FamilyMeta,
  group: string | null,
): MechanicObject | null {
  const key = objectKeyOf(text);
  if (!key) return null;
  const scalar = (name: string): string | null => {
    const m = new RegExp(`\\b${name}\\s*=\\s*("?[^\\s"{}]+"?)`).exec(text);
    return m ? m[1].replace(/"/g, "") : null;
  };
  const present = (n: string) => new RegExp(`\\b${n}\\s*=\\s*\\{`).test(text);
  const scalars: Scalar[] = meta.scalars.map((s) => {
    const v = scalar(s.key);
    return { key: s.key, kind: s.kind, present: v != null, value: v ?? "", options: s.options ?? [], picker: s.picker ?? "" };
  });
  const modifierBlocks: ModifierBlock[] = meta.modifiers.map((n) => ({
    name: n,
    present: present(n),
    flat: true,
    rows: [],
  }));
  const listFields: ListField[] = (meta.listFields ?? []).map((l) => ({
    name: l.name,
    present: present(l.name),
    picker: l.picker,
    tokens: [],
  }));
  const scriptBlocks: ScriptBlockRef[] = meta.scriptBlocks.map((b) => ({
    name: b.name,
    registry: b.registry,
    present: present(b.name),
  }));
  const colorM = /\bcolor\s*=\s*\{\s*(\d+)\s+(\d+)\s+(\d+)\s*\}/.exec(text);
  // Sub-groups (ages): re-render the container specs with no parsed entries yet
  // (the freshly-created object gains entries once saved & re-parsed).
  const subGroups: SubGroupData[] = (meta.subGroups ?? []).map((sg) => ({
    ...sg,
    entries: [],
  }));
  return {
    family: meta.id,
    key,
    editKey: key,
    file,
    origin: "mod",
    name: key,
    nameKey: key,
    descKey: meta.descSuffix ? `${key}_desc` : `desc_${key}`,
    descLoc: null,
    icon: scalar("icon"),
    iconKind: meta.iconKind,
    color: colorM ? [Number(colorM[1]), Number(colorM[2]), Number(colorM[3])] : null,
    scalars,
    modifierBlocks,
    listFields,
    selfModifier: meta.selfModifier,
    selfRows: [],
    scriptBlocks,
    ordered: meta.ordered,
    orderedChildren: [],
    subGroups,
    eventRefs: [],
    group,
    rawExtra: [],
    raw: text,
  };
}

// ── Effective data (base + PENDING create/delete) ─────────────────────────────

/** Folds the typed edit queue over `base`, applying create + delete for the
 *  directory families (project-file appends). Group-nested creates (schools)
 *  are insertStatement into an existing block — folded by key too. */
export function foldMechanics(base: MechanicsData, edits: TypedEdit[]): MechanicsData {
  const meta = base.meta;
  const objects = base.objects.slice();
  for (const e of edits) {
    if ((e.kind === "appendText" || e.kind === "createFile") && e.file === meta.projectFile) {
      const obj = parseScaffoldObject(e.text, e.file, meta, null);
      if (obj && !objects.some((o) => o.key === obj.key)) objects.push(obj);
    } else if (
      meta.groupNested &&
      e.kind === "insertStatement" &&
      e.blockPath.length === 2 &&
      e.blockPath[1] === "religious_schools" &&
      e.file === meta.projectFile
    ) {
      const obj = parseScaffoldObject(e.statement, e.file, meta, e.blockPath[0]);
      if (obj && !objects.some((o) => o.key === obj.key)) objects.push(obj);
    } else if (
      e.kind === "removeStatement" &&
      e.file === meta.projectFile &&
      (e.blockPath.length === 0 ||
        (e.blockPath.length === 2 && e.blockPath[1] === "religious_schools"))
    ) {
      const i = objects.findIndex((o) => o.key === e.key);
      if (i >= 0) objects.splice(i, 1);
    }
  }
  return { ...base, objects };
}

/** All object keys (for uniqueness checks). */
export function allKeys(data: MechanicsData): Set<string> {
  return new Set(data.objects.map((o) => o.key));
}

/** Serialize modifier rows to a block body (`k = v k2 = v2`). */
export function modBlockValue(rows: ModRow[]): string {
  return rows.map((r) => `${r.key} = ${r.value}`).join(" ");
}
