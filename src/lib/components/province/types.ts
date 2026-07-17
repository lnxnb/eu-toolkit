// Types + effective-state derivation for the province panel (Sprint 2.2/2.3).
// Mirrors the backend `province_details::ProvinceDetails` payload and the
// `get_geo_options` command. The re-derive helpers fold the timeline's dated
// blocks over the 1444 top level exactly as the game (and the Rust snapshot)
// do, so pre-start timeline edits update the shown effective state live.

/** A raw identity key + its localized display name (backend `KeyName`). */
export interface KeyName {
  key: string;
  name: string;
}

/** One reconstructed statement (backend `RawStatement`). */
export interface RawStatement {
  key: string;
  value: string;
  is_block: boolean;
}

/** The modeled province state at a point in time (backend `ProvinceStateSnapshot`). */
export interface ProvinceSnapshot {
  owner: string | null;
  controller: string | null;
  cores: string[];
  claims: string[];
  culture: string | null;
  religion: string | null;
  trade_goods: string | null;
  latent_trade_goods: string | null;
  base_tax: number | null;
  base_production: number | null;
  base_manpower: number | null;
  capital: string | null;
  is_city: boolean | null;
  hre: boolean | null;
  seat_in_parliament: boolean | null;
  discovered_by: string[];
  buildings: string[];
  native_size: number | null;
  native_ferocity: number | null;
  native_hostileness: number | null;
  center_of_trade: number | null;
  extra_cost: number | null;
  tribal_owner: string | null;
  reformation_center: string | null;
  local_autonomy: number | null;
  unrest: number | null;
  trade_company: string | null;
}

/** One dated block (backend `DatedBlock`). */
export interface DatedBlock {
  date: string;
  post_start: boolean;
  occurrence_index: number;
  entries: RawStatement[];
}

/** Joined geography (backend `Geography`). */
export interface Geography {
  area: KeyName | null;
  region: KeyName | null;
  superregion: KeyName | null;
  trade_node: KeyName | null;
  climate: KeyName | null;
  winter: KeyName | null;
  impassable: boolean;
  monsoon: KeyName | null;
  continent: KeyName | null;
  terrain_override: KeyName | null;
  water: boolean;
}

/** Full province-details payload (backend `ProvinceDetails`). */
export interface ProvinceDetails {
  id: number;
  file: string;
  exists: boolean;
  localized_name: string;
  definition_name: string;
  owner: string | null;
  top_level: ProvinceSnapshot;
  effective_1444: ProvinceSnapshot;
  raw_remainder: RawStatement[];
  dated_blocks: DatedBlock[];
  geography: Geography;
}

/** One geography membership target (backend `GeoOption`). */
export interface GeoOption {
  key: string;
  name: string;
  file: string;
  list_path: string[];
}

/** All geography options grouped by slot (backend `GeoOptions`). */
export interface GeoOptions {
  areas: GeoOption[];
  continents: GeoOption[];
  trade_nodes: GeoOption[];
  terrains: GeoOption[];
  climate_zones: GeoOption[];
  winters: GeoOption[];
  impassable_file: string;
  /** map/continent.txt — target for the create-continent flow (S3.1). */
  continent_file: string;
}

/** Game start date; a dated block with date ≤ START is applied to effective 1444. */
const START = 1444 * 10000 + 11 * 100 + 11;

function dateNum(s: string): number {
  const [y, m, d] = s.split(".");
  return (parseInt(y, 10) || 0) * 10000 + (parseInt(m, 10) || 0) * 100 + (parseInt(d, 10) || 0);
}

function num(v: string): number | null {
  const n = parseFloat(v);
  return Number.isFinite(n) ? n : null;
}
function int(v: string): number | null {
  const n = parseInt(v, 10);
  return Number.isFinite(n) ? n : null;
}

/** Strips `{ coal }` → `coal` (latent trade good block form). */
export function unwrapLatent(v: string): string {
  const m = v.match(/\{?\s*([A-Za-z0-9_]+)\s*\}?/);
  return m ? m[1] : v.trim();
}

/** Applies one `key = value` statement to a snapshot, matching the Rust
 *  `ProvinceStateSnapshot::apply` add/remove/flag semantics. `isBlock` skips
 *  block-valued statements (they don't mutate the modeled snapshot) except the
 *  one modeled block key, `latent_trade_goods`. */
export function applyStatement(s: ProvinceSnapshot, key: string, value: string, isBlock: boolean) {
  if (key === "latent_trade_goods") {
    s.latent_trade_goods = unwrapLatent(value);
    return;
  }
  if (isBlock) return;
  switch (key) {
    case "owner": s.owner = value; break;
    case "controller": s.controller = value; break;
    case "culture": s.culture = value; break;
    case "religion": s.religion = value; break;
    case "trade_goods": s.trade_goods = value; break;
    case "tribal_owner": s.tribal_owner = value; break;
    case "reformation_center": s.reformation_center = value; break;
    case "capital": s.capital = value; break;
    case "add_to_trade_company": s.trade_company = value; break;
    case "base_tax": s.base_tax = num(value); break;
    case "base_production": s.base_production = num(value); break;
    case "base_manpower": s.base_manpower = num(value); break;
    case "native_size": s.native_size = num(value); break;
    case "native_ferocity": s.native_ferocity = num(value); break;
    case "native_hostileness": s.native_hostileness = num(value); break;
    case "local_autonomy":
    case "add_local_autonomy": s.local_autonomy = num(value); break;
    case "unrest": s.unrest = num(value); break;
    case "center_of_trade": s.center_of_trade = int(value); break;
    case "extra_cost": s.extra_cost = int(value); break;
    case "is_city": s.is_city = value === "yes"; break;
    case "hre": s.hre = value === "yes"; break;
    case "seat_in_parliament": s.seat_in_parliament = value === "yes"; break;
    case "add_core": if (!s.cores.includes(value)) s.cores.push(value); break;
    case "remove_core": s.cores = s.cores.filter((c) => c !== value); break;
    case "add_claim": if (!s.claims.includes(value)) s.claims.push(value); break;
    case "remove_claim": s.claims = s.claims.filter((c) => c !== value); break;
    case "discovered_by": if (!s.discovered_by.includes(value)) s.discovered_by.push(value); break;
    default:
      if (value === "yes") { if (!s.buildings.includes(key)) s.buildings.push(key); }
      else if (value === "no") s.buildings = s.buildings.filter((b) => b !== key);
  }
}

function clone(s: ProvinceSnapshot): ProvinceSnapshot {
  return {
    ...s,
    cores: [...s.cores],
    claims: [...s.claims],
    discovered_by: [...s.discovered_by],
    buildings: [...s.buildings],
  };
}

/**
 * Re-derives the effective snapshot from the top level plus every dated block
 * with date ≤ `cutoff` in file order — the game's own rule. `cutoff` defaults to
 * the vanilla start (1444.11.11) for pre-start timeline edits; at a later view
 * date (Sprint 12.3) the panel passes the selected date so a block added at that
 * date updates the shown effective state without a backend round-trip.
 */
export function deriveEffective(
  topLevel: ProvinceSnapshot,
  blocks: DatedBlock[],
  cutoff?: string | null,
): ProvinceSnapshot {
  const cut = cutoff ? dateNum(cutoff) : START;
  const s = clone(topLevel);
  // Apply in FILE ORDER (as the game and the Rust snapshot do), not sorted —
  // `blocks` is kept in file order by the panel (appended entries land last).
  for (const block of blocks) {
    if (dateNum(block.date) > cut) continue;
    for (const e of block.entries) applyStatement(s, e.key, e.value, e.is_block);
  }
  return s;
}
