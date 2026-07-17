// Sprint 20 — Estates subsystem (View ▸ Estates).
//
// Mirrors the `get_estates` wire payload (estates.rs) and folds the typed edit
// queue over the base so create/delete appear live in the list (undo/redo revert
// them). Per-field edits (scalars, modifier blocks, trigger/effect trees, loc
// name/desc, icon) are read at render time through the queue's pending helpers /
// `parse_script_block_with_edits`, so they aren't folded here.

import type { TypedEdit } from "$lib/edits.svelte";

export const ESTATES_PROJECT_FILE = "common/estates/zz_eutoolkit_estates.txt";
export const PRIVILEGES_PROJECT_FILE =
  "common/estate_privileges/zz_eutoolkit_estate_privileges.txt";
export const AGENDAS_PROJECT_FILE = "common/estate_agendas/zz_eutoolkit_estate_agendas.txt";

// ── Wire types (mirror estates.rs) ───────────────────────────────────────────

export interface ModRow {
  key: string;
  value: string;
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
export interface Scalar {
  key: string;
  kind: string; // "num" | "int" | "bool"
  present: boolean;
  value: string;
}
export interface EstateObject {
  kind: string; // "estate" | "privilege" | "agenda"
  key: string;
  file: string;
  origin: string; // "base" | "mod"
  name: string;
  locKey: string;
  descKey: string;
  descLoc: string | null;
  icon: string | null;
  iconKind: string; // "none" | "index" | "sprite"
  color: [number, number, number] | null;
  scalars: Scalar[];
  modifierBlocks: ModifierBlock[];
  scriptBlocks: ScriptBlockRef[];
  privileges: string[];
  agendas: string[];
  rawExtra: string[];
  raw: string;
}
export interface EstatesData {
  estates: EstateObject[];
  privileges: EstateObject[];
  agendas: EstateObject[];
  estatesProjectFile: string;
  privilegesProjectFile: string;
  agendasProjectFile: string;
}
export interface Scaffold {
  key: string;
  file: string;
  text: string;
  locNameKey: string;
  locDescKey: string;
  locName: string;
}

// Country-panel payloads.
export interface PrivilegeBrief {
  key: string;
  name: string;
  file: string;
}
export interface EstateBrief {
  key: string;
  name: string;
  icon: string | null;
  privileges: PrivilegeBrief[];
}
export interface StartingPrivilege {
  privilege: string;
  name: string;
  estate: string | null;
  date: string | null;
}
export interface CountryEstates {
  tag: string;
  file: string;
  starting: StartingPrivilege[];
  estates: EstateBrief[];
}
export interface PrivilegeHolder {
  tag: string;
  name: string;
  date: string | null;
}

// ── Per-kind schema (mirrors estates.rs; drives created-object rendering) ─────

export type EstateKind = "estate" | "privilege" | "agenda";

export interface KindSchema {
  kind: EstateKind;
  label: string;
  projectFile: string;
  iconKind: "none" | "index" | "sprite";
  hasColor: boolean;
  hasLists: boolean;
  scalars: { key: string; kind: string }[];
  modifiers: string[];
  triggers: string[];
  effects: string[];
}

export const KIND_SCHEMAS: Record<EstateKind, KindSchema> = {
  estate: {
    kind: "estate",
    label: "Estates",
    projectFile: ESTATES_PROJECT_FILE,
    iconKind: "index",
    hasColor: true,
    hasLists: true,
    scalars: [
      { key: "base_influence", kind: "num" },
      { key: "influence_from_dev_modifier", kind: "num" },
      { key: "contributes_to_curia_treasury", kind: "bool" },
    ],
    modifiers: [
      "country_modifier_happy",
      "country_modifier_neutral",
      "country_modifier_angry",
      "land_ownership_modifier",
    ],
    triggers: ["trigger"],
    effects: [],
  },
  privilege: {
    kind: "privilege",
    label: "Privileges",
    projectFile: PRIVILEGES_PROJECT_FILE,
    iconKind: "sprite",
    hasColor: false,
    hasLists: false,
    scalars: [
      { key: "land_share", kind: "num" },
      { key: "max_absolutism", kind: "num" },
      { key: "loyalty", kind: "num" },
      { key: "influence", kind: "num" },
      { key: "cooldown_years", kind: "int" },
    ],
    modifiers: ["benefits", "penalties", "modifier_by_land_ownership"],
    triggers: ["is_valid", "can_select", "can_revoke"],
    effects: ["on_granted", "on_revoked", "on_invalid"],
  },
  agenda: {
    kind: "agenda",
    label: "Agendas",
    projectFile: AGENDAS_PROJECT_FILE,
    iconKind: "none",
    hasColor: false,
    hasLists: false,
    scalars: [{ key: "max_days_active", kind: "int" }],
    modifiers: ["modifier"],
    triggers: [
      "can_select",
      "task_requirements",
      "fail_if",
      "invalid_trigger",
      "provinces_to_highlight",
    ],
    effects: [
      "pre_effect",
      "immediate_effect",
      "task_completed_effect",
      "failing_effect",
      "on_invalid",
    ],
  },
};

// ── Key helpers ───────────────────────────────────────────────────────────────

const KEY_RE = /^[a-z][a-z0-9_]*$/;
export function isValidKey(key: string): boolean {
  return KEY_RE.test(key);
}
export function slugify(name: string): string {
  const s = name
    .toLowerCase()
    .normalize("NFKD")
    .replace(/[^a-z0-9]+/g, "_")
    .replace(/^_+|_+$/g, "");
  return s || "key";
}

/** Serializes typed modifier rows to a block body (`k = v k = v …`). */
export function modBlockValue(rows: ModRow[]): string {
  return rows.map((r) => `${r.key} = ${r.value}`).join(" ");
}

// ── Scaffold-text parsing (for the create fold) ───────────────────────────────

function objectKeyOf(text: string): string | null {
  return /^\s*([A-Za-z0-9_]+)\s*=/.exec(text)?.[1] ?? null;
}

/** Flat `k = scalar` rows inside a `name = { … }` block (no nested braces). */
function parseFlatBlock(body: string, name: string): { present: boolean; rows: ModRow[] } {
  const m = new RegExp(`\\b${name}\\s*=\\s*\\{([^{}]*)\\}`).exec(body);
  if (!m) return { present: false, rows: [] };
  const rows: ModRow[] = [];
  const re = /([A-Za-z_][A-Za-z0-9_]*)\s*=\s*("?[^\s"}]+"?)/g;
  let rm: RegExpExecArray | null;
  while ((rm = re.exec(m[1])) !== null) rows.push({ key: rm[1], value: rm[2].replace(/"/g, "") });
  return { present: true, rows };
}

/** Builds a minimal EstateObject from a freshly-scaffolded block body. */
export function parseScaffoldObject(
  text: string,
  kind: EstateKind,
  file: string,
): EstateObject | null {
  const key = objectKeyOf(text);
  if (!key) return null;
  const schema = KIND_SCHEMAS[kind];
  const scalar = (name: string): string | null => {
    const m = new RegExp(`\\b${name}\\s*=\\s*("?[^\\s"{}]+"?)`).exec(text);
    return m ? m[1].replace(/"/g, "") : null;
  };
  const iconVal = scalar("icon");
  const scalars: Scalar[] = schema.scalars.map((s) => {
    const v = scalar(s.key);
    return { key: s.key, kind: s.kind, present: v != null, value: v ?? "" };
  });
  const modifierBlocks: ModifierBlock[] = schema.modifiers.map((n) => {
    const { present, rows } = parseFlatBlock(text, n);
    return { name: n, present, flat: true, rows };
  });
  const scriptBlocks: ScriptBlockRef[] = [
    ...schema.triggers.map((n) => ({
      name: n,
      registry: "triggers",
      present: new RegExp(`\\b${n}\\s*=\\s*\\{`).test(text),
    })),
    ...schema.effects.map((n) => ({
      name: n,
      registry: "effects",
      present: new RegExp(`\\b${n}\\s*=\\s*\\{`).test(text),
    })),
  ];
  return {
    kind,
    key,
    file,
    origin: "mod",
    name: key,
    locKey: key,
    descKey: `${key}_desc`,
    descLoc: null,
    icon: iconVal,
    iconKind: schema.iconKind,
    color: null,
    scalars,
    modifierBlocks,
    scriptBlocks,
    privileges: [],
    agendas: [],
    rawExtra: [],
    raw: text,
  };
}

// ── Effective data (base + PENDING create/delete) ─────────────────────────────

function listOf(data: EstatesData, kind: EstateKind): EstateObject[] {
  return kind === "estate" ? data.estates : kind === "privilege" ? data.privileges : data.agendas;
}
function kindForFile(file: string): EstateKind | null {
  if (file.startsWith("common/estate_privileges/")) return "privilege";
  if (file.startsWith("common/estate_agendas/")) return "agenda";
  if (file.startsWith("common/estates/")) return "estate";
  return null;
}

/** Folds the typed edit queue over `base`, applying create + delete. */
export function foldEstates(base: EstatesData, edits: TypedEdit[]): EstatesData {
  const estates = base.estates.slice();
  const privileges = base.privileges.slice();
  const agendas = base.agendas.slice();
  const bucket = (k: EstateKind) =>
    k === "estate" ? estates : k === "privilege" ? privileges : agendas;

  for (const e of edits) {
    if (e.kind === "appendText" || e.kind === "createFile") {
      const kind = kindForFile(e.file);
      if (!kind) continue;
      const obj = parseScaffoldObject(e.text, kind, e.file);
      if (obj && !bucket(kind).some((o) => o.key === obj.key)) bucket(kind).push(obj);
    } else if (e.kind === "removeStatement" && e.blockPath.length === 0) {
      const kind = kindForFile(e.file);
      if (!kind) continue;
      const arr = bucket(kind);
      const i = arr.findIndex((o) => o.key === e.key);
      if (i >= 0) arr.splice(i, 1);
    }
  }

  return { ...base, estates, privileges, agendas };
}

/** All object keys across the three kinds (for uniqueness checks). */
export function allKeys(data: EstatesData): Set<string> {
  const s = new Set<string>();
  for (const o of [...data.estates, ...data.privileges, ...data.agendas]) s.add(o.key);
  return s;
}

export { listOf };
