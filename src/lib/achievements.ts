// Achievements editor (View ▸ Achievements…).
//
// Mirrors the `get_achievements` wire payload (achievements.rs) and folds the
// typed edit queue over the base so create/delete appear live in the list
// (undo/redo revert them). Per-field edits (id/localization scalars, trigger
// trees, loc name/desc) are read at render time through the queue's pending
// helpers / `parse_script_block_with_edits`, so they aren't folded here.
//
// Reality check the UI carries too: editing this file changes the IN-GAME
// achievements window only — a mod can never grant Steam achievements (the
// Steam award is keyed to vanilla's compiled id mapping).

import type { TypedEdit } from "$lib/edits.svelte";

export const ACHIEVEMENTS_FILE = "common/achievements.txt";

/** Trigger blocks in canonical display order (all trigger-shaped). */
export const TRIGGER_BLOCKS = ["possible", "happened", "visible", "provinces_to_highlight"];

// ── Wire types (mirror achievements.rs) ──────────────────────────────────────

export interface ScriptBlockRef {
  name: string;
  present: boolean;
}
export interface Achievement {
  key: string;
  file: string;
  origin: string; // "base" | "mod"
  id: number | null;
  localization: string | null;
  name: string;
  nameKey: string;
  nameLoc: string | null;
  descKey: string;
  descLoc: string | null;
  scriptBlocks: ScriptBlockRef[];
  hasIcon: boolean;
  rawExtra: string[];
  raw: string;
}
export interface AchievementsData {
  achievements: Achievement[];
  file: string;
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

function keyOf(text: string): string | null {
  return /^\s*([A-Za-z0-9_]+)\s*=/.exec(text)?.[1] ?? null;
}

/** Builds a minimal Achievement from a freshly-scaffolded block body. */
export function parseScaffoldAchievement(text: string): Achievement | null {
  const key = keyOf(text);
  if (!key) return null;
  const scalar = (name: string): string | null => {
    const m = new RegExp(`\\b${name}\\s*=\\s*([^\\s{}]+)`).exec(text);
    return m ? m[1] : null;
  };
  const present = (n: string) => new RegExp(`\\b${n}\\s*=\\s*\\{`).test(text);
  const idRaw = scalar("id");
  const localization = scalar("localization");
  const stem = localization ?? key;
  return {
    key,
    file: ACHIEVEMENTS_FILE,
    origin: "mod",
    id: idRaw != null && Number.isFinite(Number(idRaw)) ? Number(idRaw) : null,
    localization,
    name: key,
    nameKey: `${stem}_NAME`,
    nameLoc: null,
    descKey: `${stem}_DESC`,
    descLoc: null,
    scriptBlocks: TRIGGER_BLOCKS.map((n) => ({ name: n, present: present(n) })),
    hasIcon: false,
    rawExtra: [],
    raw: text,
  };
}

// ── Effective data (base + PENDING create/delete) ─────────────────────────────

/** Folds the typed edit queue over `base`, applying create + delete. */
export function foldAchievements(base: AchievementsData, edits: TypedEdit[]): AchievementsData {
  const achievements = base.achievements.slice();
  for (const e of edits) {
    if ((e.kind === "appendText" || e.kind === "createFile") && e.file === ACHIEVEMENTS_FILE) {
      const obj = parseScaffoldAchievement(e.text);
      if (obj && !achievements.some((o) => o.key === obj.key)) achievements.push(obj);
    } else if (e.kind === "removeStatement" && e.blockPath.length === 0 && e.file === ACHIEVEMENTS_FILE) {
      const i = achievements.findIndex((o) => o.key === e.key);
      if (i >= 0) achievements.splice(i, 1);
    }
  }
  return { ...base, achievements };
}

/** All achievement keys (for uniqueness checks). */
export function allKeys(data: AchievementsData): Set<string> {
  return new Set(data.achievements.map((a) => a.key));
}
