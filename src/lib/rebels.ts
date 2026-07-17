// Sprint 21 — Rebel factions subsystem (View ▸ Rebels).
//
// Mirrors the `get_rebels` wire payload (rebels.rs) and folds the typed edit
// queue over the base so create/delete appear live in the list (undo/redo revert
// them). Per-field edits (typed scalars, color, trigger/effect/weight trees, loc
// name/title/desc) are read at render time through the queue's pending helpers /
// `parse_script_block_with_edits`, so they aren't folded here.

import type { TypedEdit } from "$lib/edits.svelte";

export const REBELS_PROJECT_FILE = "common/rebel_types/zz_eutoolkit_rebel_types.txt";

// ── Wire types (mirror rebels.rs) ────────────────────────────────────────────

export interface Scalar {
  key: string;
  kind: string; // "num" | "int" | "bool" | "enum" | "str"
  present: boolean;
  value: string;
  options: string[]; // non-empty for "enum"
}
export interface ScriptBlockRef {
  name: string;
  registry: string; // "triggers" | "effects"
  present: boolean;
}
export interface RebelFaction {
  key: string;
  file: string;
  origin: string; // "base" | "mod"
  title: string;
  nameKey: string;
  nameLoc: string | null;
  titleKey: string;
  titleLoc: string | null;
  descKey: string;
  descLoc: string | null;
  color: [number, number, number] | null;
  scalars: Scalar[];
  scriptBlocks: ScriptBlockRef[];
  rawExtra: string[];
  raw: string;
}
export interface RebelsData {
  factions: RebelFaction[];
  projectFile: string;
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
}
export interface RebelProvince {
  id: number;
  name: string;
  date: string | null;
}

// ── Schema (mirrors rebels.rs; drives created-faction rendering) ──────────────

interface ScalarSpec {
  key: string;
  kind: string;
  options?: string[];
}

export const SCALAR_SPECS: ScalarSpec[] = [
  { key: "area", kind: "enum", options: ["nation", "nation_rebel_tag", "nation_religion"] },
  { key: "government", kind: "enum", options: ["any", "monarchy", "republic", "theocracy", "anti"] },
  { key: "defection", kind: "enum", options: ["none", "culture", "culture_group", "religion", "any", "nation_rebel_tag"] },
  { key: "independence", kind: "enum", options: ["none", "culture", "culture_group", "religion", "any", "nation_rebel_tag"] },
  { key: "gfx_type", kind: "enum", options: ["culture_province", "culture_owner"] },
  { key: "defect_delay", kind: "int" },
  { key: "unit_transfer", kind: "bool" },
  { key: "will_relocate", kind: "bool" },
  { key: "resilient", kind: "bool" },
  { key: "reinforcing", kind: "bool" },
  { key: "general", kind: "bool" },
  { key: "smart", kind: "bool" },
  { key: "dynasty", kind: "bool" },
  { key: "disband_on_leader_death", kind: "bool" },
  { key: "revolutionary", kind: "bool" },
  { key: "handle_action_negotiate", kind: "bool" },
  { key: "handle_action_stability", kind: "bool" },
  { key: "handle_action_build_core", kind: "bool" },
  { key: "handle_action_send_missionary", kind: "bool" },
  { key: "handle_action_change_culture", kind: "bool" },
  { key: "artillery", kind: "num" },
  { key: "infantry", kind: "num" },
  { key: "cavalry", kind: "num" },
  { key: "morale", kind: "num" },
  { key: "religion", kind: "str" },
  { key: "demands_description", kind: "str" },
];

const TRIGGER_BLOCKS = ["siege_won_trigger", "can_negotiate_trigger", "can_enforce_trigger"];
const EFFECT_BLOCKS = ["siege_won_effect", "demands_enforced_effect"];
const WEIGHT_BLOCKS = ["spawn_chance", "movement_evaluation"];

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
  const s = base || "key";
  // Rebel type keys conventionally end in `_rebels`.
  return s.endsWith("_rebels") ? s : `${s}_rebels`;
}

// ── Scaffold-text parsing (for the create fold) ───────────────────────────────

function factionKeyOf(text: string): string | null {
  return /^\s*([A-Za-z0-9_]+)\s*=/.exec(text)?.[1] ?? null;
}

/** Builds a minimal RebelFaction from a freshly-scaffolded block body. */
export function parseScaffoldFaction(text: string, file: string): RebelFaction | null {
  const key = factionKeyOf(text);
  if (!key) return null;
  const scalar = (name: string): string | null => {
    const m = new RegExp(`\\b${name}\\s*=\\s*("?[^\\s"{}]+"?)`).exec(text);
    return m ? m[1].replace(/"/g, "") : null;
  };
  const scalars: Scalar[] = SCALAR_SPECS.map((s) => {
    const v = scalar(s.key);
    return { key: s.key, kind: s.kind, present: v != null, value: v ?? "", options: s.options ?? [] };
  });
  const present = (n: string) => new RegExp(`\\b${n}\\s*=\\s*\\{`).test(text);
  const scriptBlocks: ScriptBlockRef[] = [
    ...TRIGGER_BLOCKS.map((n) => ({ name: n, registry: "triggers", present: present(n) })),
    ...EFFECT_BLOCKS.map((n) => ({ name: n, registry: "effects", present: present(n) })),
    ...WEIGHT_BLOCKS.map((n) => ({ name: n, registry: "triggers", present: present(n) })),
  ];
  const colorM = /\bcolor\s*=\s*\{\s*(\d+)\s+(\d+)\s+(\d+)\s*\}/.exec(text);
  return {
    key,
    file,
    origin: "mod",
    title: key,
    nameKey: `${key}_name`,
    nameLoc: null,
    titleKey: `${key}_title`,
    titleLoc: null,
    descKey: `${key}_desc`,
    descLoc: null,
    color: colorM ? [Number(colorM[1]), Number(colorM[2]), Number(colorM[3])] : null,
    scalars,
    scriptBlocks,
    rawExtra: [],
    raw: text,
  };
}

// ── Effective data (base + PENDING create/delete) ─────────────────────────────

/** Folds the typed edit queue over `base`, applying create + delete. */
export function foldRebels(base: RebelsData, edits: TypedEdit[]): RebelsData {
  const factions = base.factions.slice();
  for (const e of edits) {
    if ((e.kind === "appendText" || e.kind === "createFile") && e.file === REBELS_PROJECT_FILE) {
      const obj = parseScaffoldFaction(e.text, e.file);
      if (obj && !factions.some((o) => o.key === obj.key)) factions.push(obj);
    } else if (e.kind === "removeStatement" && e.blockPath.length === 0 && e.file === REBELS_PROJECT_FILE) {
      const i = factions.findIndex((o) => o.key === e.key);
      if (i >= 0) factions.splice(i, 1);
    }
  }
  return { ...base, factions };
}

/** All faction keys (for uniqueness checks). */
export function allKeys(data: RebelsData): Set<string> {
  return new Set(data.factions.map((f) => f.key));
}

// ── Revolt block (province panel) ─────────────────────────────────────────────

export interface Revolt {
  type: string | null;
  size: string | null;
  leader: string | null;
  name: string | null;
}

/** Parses a `revolt = { … }` block body (with or without braces) into fields. */
export function parseRevolt(body: string): Revolt {
  const inner = body.replace(/^\s*\{/, "").replace(/\}\s*$/, "");
  const scalar = (k: string): string | null => {
    const m = new RegExp(`\\b${k}\\s*=\\s*("[^"]*"|[^\\s{}]+)`).exec(inner);
    return m ? m[1].replace(/^"|"$/g, "") : null;
  };
  return { type: scalar("type"), size: scalar("size"), leader: scalar("leader"), name: scalar("name") };
}

/** True when a `revolt` block value is empty (`{}` / `{ }` — revolt cleared). */
export function revoltEmpty(body: string): boolean {
  return /^\s*\{\s*\}\s*$/.test(body);
}

/** Serializes revolt fields to a block body (`{ type = … size = … leader = "…" }`). */
export function revoltBody(r: Revolt): string {
  const parts: string[] = [];
  if (r.type) parts.push(`type = ${r.type}`);
  if (r.size != null && r.size !== "") parts.push(`size = ${r.size}`);
  if (r.leader) parts.push(`leader = "${r.leader}"`);
  if (r.name) parts.push(`name = "${r.name}"`);
  return parts.length ? `{ ${parts.join(" ")} }` : "{ }";
}
