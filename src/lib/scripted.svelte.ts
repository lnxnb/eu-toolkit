// Sprint 28 — the shared scripted-trigger / scripted-effect registry.
//
// Every 14.2 condition/effect tree in the app renders an unmodeled key as a raw
// row. Many of those "unknown" keys are actually calls to a scripted trigger or
// scripted effect (`has_mil_advisor = yes`, `add_loot_from_province_effect = yes`).
// This module is the session-wide registry `ScriptNode` consults so it can render
// such a name as a jump-LINK to its definition instead. Mod-defined names resolve
// too (the backend scans through the Vfs).
//
// A module-level singleton (rather than a threaded prop) so ANY tree — decisions,
// events, missions, mechanics, on_actions, scripted bodies themselves — resolves
// links without every host having to pass the map down.

import { invoke } from "@tauri-apps/api/core";

export interface ScriptedDef {
  name: string;
  kind: "trigger" | "effect";
  file: string;
  origin: "base" | "mod";
  path: string[];
  params: string[];
  lineCount: number;
}

let defs = $state<ScriptedDef[]>([]);
let byName = $state<Map<string, ScriptedDef>>(new Map());

/** All scripted definitions loaded this session (browser list). */
export function scriptedDefs(): ScriptedDef[] {
  return defs;
}

/** The definition a call-site name resolves to, or undefined. */
export function resolveScripted(name: string | null | undefined): ScriptedDef | undefined {
  if (!name) return undefined;
  return byName.get(name);
}

/** Loads (or reloads) the registry for the current session. Safe to call on
 *  session change and after a save (a freshly-scaffolded scripted name appears). */
export async function loadScriptedDefs(
  installPath: string,
  modPath: string | null,
): Promise<void> {
  try {
    const list = await invoke<ScriptedDef[]>("get_scripted_definitions", {
      installPath,
      modPath,
    });
    defs = list;
    byName = new Map(list.map((d) => [d.name, d]));
  } catch {
    defs = [];
    byName = new Map();
  }
}

// --- Jump handler (set by MapView; called by ScriptNode links) -------------

let jumpHandler: ((def: ScriptedDef) => void) | null = null;

/** Registers the handler that opens the scripted browser focused on a def. */
export function setScriptedJump(fn: ((def: ScriptedDef) => void) | null): void {
  jumpHandler = fn;
}

/** Jump to a scripted definition (opens the browser focused on it). */
export function jumpToScripted(def: ScriptedDef): void {
  jumpHandler?.(def);
}
