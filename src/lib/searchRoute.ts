// Path → editor routing for project-wide search (Sprint 30.3).
//
// A search hit is a (file, line) pair. Clicking it should land the user in the
// editor that owns that file — the same jump-targets the validation strips use,
// generalized to arbitrary game-relative paths. Files with no dedicated editor
// fall back to a read-only preview.
//
// ── Routing table ────────────────────────────────────────────────────────────
//   history/provinces/<id> - Name.txt        → province panel (select <id>)
//   history/countries/<TAG> - Name.txt       → country panel (political select)
//   history/diplomacy|wars|advisors/**        → preview (no per-file editor)
//   common/religions/**                       → Religion mode
//   common/cultures/**                        → Culture mode
//   common/tradenodes/**                      → Trade Nodes mode
//   common/tradegoods/**                      → Trade Goods mode
//   common/colonial_regions/**                → Colonial Regions mode
//   common/trade_companies/**                 → Trade Companies mode
//   common/estates/** · estate_privileges/**  → Estates overlay
//     · estate_agendas/** · estate_crown_land → Estates overlay
//   common/rebel_types/**                     → Rebels overlay
//   common/achievements.txt                   → Achievements overlay
//   common/technologies/** · common/units/**  → Technology overlay
//   common/government_names/**                → Government names overlay
//   common/scripted_triggers|effects/**       → Scripted overlay
//   common/on_actions/**                      → On Actions overlay
//   common/imperial_reforms|incidents/**      → Empires overlay
//     · common/decrees/**
//   common/defines*  ·  defines.lua           → Defines overlay
//   common/<mechanics-family-dir>/**          → Mechanics overlay (that family)
//   decisions/**                              → Decisions overlay
//   events/**                                 → Events overlay
//   missions/**                               → Missions overlay
//   localisation/**  (*.yml)                  → Localisation overlay
//   map/area.txt                              → Areas mode
//   map/region.txt · map/superregion.txt      → Regions mode
//   map/climate.txt                           → Climate mode
//   map/terrain.txt                           → Simple Terrain mode
//   map/adjacencies.csv · definition.csv      → Provinces mode
//     · default.map · continent.txt
//   (anything else)                           → read-only preview
//
// The mechanics-family directories are data-driven: the caller passes a
// dir→familyId map built from `get_mechanic_families` so mod-added families and
// the full Sprint 27 sweep route without hardcoding 50 directory names here.

export type OverlayId =
  | "decisions"
  | "events"
  | "missions"
  | "govnames"
  | "estates"
  | "rebels"
  | "achievements"
  | "technology"
  | "mechanics"
  | "empires"
  | "scripted"
  | "onactions"
  | "localisation"
  | "defines";

export type SearchRoute =
  | { kind: "province"; id: number }
  | { kind: "country"; tag: string }
  | { kind: "mode"; mode: string }
  | { kind: "overlay"; overlay: OverlayId; family?: string }
  | { kind: "preview" };

/** Static single-directory → map-mode routes (exact `common/<dir>/` prefixes). */
const COMMON_MODE: Record<string, string> = {
  religions: "religion",
  cultures: "culture",
  tradenodes: "trade_nodes",
  tradegoods: "trade_goods",
  colonial_regions: "colonial_regions",
  trade_companies: "trade_companies",
};

/** Static `common/<dir>/` prefixes → overlay (no per-entity focus). */
const COMMON_OVERLAY: Record<string, OverlayId> = {
  estates: "estates",
  estate_privileges: "estates",
  estate_agendas: "estates",
  estate_crown_land: "estates",
  estates_preload: "estates",
  rebel_types: "rebels",
  technologies: "technology",
  units: "technology",
  government_names: "govnames",
  scripted_triggers: "scripted",
  scripted_effects: "scripted",
  on_actions: "onactions",
  imperial_reforms: "empires",
  imperial_incidents: "empires",
  decrees: "empires",
};

/** `map/<file>` → mode. */
const MAP_MODE: Record<string, string> = {
  "area.txt": "areas",
  "region.txt": "regions",
  "superregion.txt": "regions",
  "climate.txt": "climate",
  "terrain.txt": "simple_terrain",
  "adjacencies.csv": "provinces",
  "definition.csv": "provinces",
  "default.map": "provinces",
  "continent.txt": "provinces",
};

/** Leading province id of a `history/provinces/<id> - Name.txt` path. */
export function provinceIdOf(file: string): number | null {
  const m = /(?:^|\/)history\/provinces\/(\d+)\s*[-—]/.exec(file);
  return m ? Number(m[1]) : null;
}

/** Leading TAG of a `history/countries/<TAG> - Name.txt` path. */
export function countryTagOf(file: string): string | null {
  const m = /(?:^|\/)history\/countries\/([A-Za-z0-9_]{2,3})\s*[-—]/.exec(file);
  return m ? m[1].toUpperCase() : null;
}

/**
 * Routes a game-relative file path to the editor that owns it.
 * `mechanicsDirs` maps a `common/<dir>` → mechanics family id.
 */
export function routeForFile(
  file: string,
  mechanicsDirs: Map<string, string> = new Map(),
): SearchRoute {
  const f = file.replace(/\\/g, "/");

  // history/*
  const pid = provinceIdOf(f);
  if (pid != null) return { kind: "province", id: pid };
  const tag = countryTagOf(f);
  if (tag) return { kind: "country", tag };

  // common/*
  const cm = /(?:^|\/)common\/([^/]+)\//.exec(f);
  if (cm) {
    const dir = cm[1];
    if (COMMON_MODE[dir]) return { kind: "mode", mode: COMMON_MODE[dir] };
    if (COMMON_OVERLAY[dir]) return { kind: "overlay", overlay: COMMON_OVERLAY[dir] };
    const fam = mechanicsDirs.get(`common/${dir}`);
    if (fam) return { kind: "overlay", overlay: "mechanics", family: fam };
    if (dir === "defines") return { kind: "overlay", overlay: "defines" };
  }
  if (/(?:^|\/)common\/defines\.lua$/.test(f)) return { kind: "overlay", overlay: "defines" };
  if (/(?:^|\/)common\/achievements\.txt$/.test(f))
    return { kind: "overlay", overlay: "achievements" };

  // top-level content directories
  if (/(?:^|\/)decisions\//.test(f)) return { kind: "overlay", overlay: "decisions" };
  if (/(?:^|\/)events\//.test(f)) return { kind: "overlay", overlay: "events" };
  if (/(?:^|\/)missions\//.test(f)) return { kind: "overlay", overlay: "missions" };
  if (/(?:^|\/)localisation\//.test(f) && f.toLowerCase().endsWith(".yml"))
    return { kind: "overlay", overlay: "localisation" };

  // map/*
  const mm = /(?:^|\/)map\/([^/]+)$/.exec(f);
  if (mm && MAP_MODE[mm[1]]) return { kind: "mode", mode: MAP_MODE[mm[1]] };

  return { kind: "preview" };
}
