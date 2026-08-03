//! Phase 0.9 — validation framework: per-domain reports of
//! `{severity, message, jump}`. Each domain is a read-only check over the Vfs
//! that reuses the game_data/paradox public APIs (or does a minimal local parse
//! where a shape isn't exposed). Sprints add domains by appending to
//! [`DOMAIN_REGISTRY`]; the `validate` command dispatches on the domain string.
//!
//! Consumers: the persistent non-blocking strip (frontend `ValidationStrip`)
//! renders the issues and jumps to a problem via the typed [`JumpTarget`].
//! Deepest consumers per SPRINT.md: trade-node graph (8.6), area/region orphans
//! (10.1/10.2), climate impassable (11.1); diplomacy (3.4) reuses the same shape.

use std::collections::{HashMap, HashSet};

use crate::date::Date;
use crate::loc::LocStore;
use crate::paradox::{self, Block};
use crate::vfs::Vfs;

/// Report severity. Serializes lowercase (`"error"`, `"warning"`, `"info"`).
#[derive(serde::Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

/// A typed, serializable pointer to the thing an issue is about, so the
/// frontend can act on it (select a province, open a country, …) rather than
/// parse the message. Adjacently tagged: `{ "kind": "province", "id": 123 }`,
/// `{ "kind": "country", "id": "FRA" }`. Extensible — add variants as sprints
/// need them (Region, TradeGood, …); the frontend switches on `kind`.
// Node/File aren't constructed yet — reserved for the trade-node (8.6) and
// file-level domains; part of the frontend-facing API surface.
#[allow(dead_code)]
#[derive(serde::Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum JumpTarget {
    Province(u32),
    Country(String),
    Area(String),
    Node(String),
    File(String),
    ColonialRegion(String),
    TradeCompany(String),
}

/// One entry in a domain's validation report.
#[derive(serde::Serialize, Clone, Debug)]
pub struct ValidationIssue {
    pub severity: Severity,
    pub message: String,
    /// Where to go to fix it; `None` for issues with no single location.
    pub jump: Option<JumpTarget>,
}

impl ValidationIssue {
    pub fn new(severity: Severity, message: String, jump: Option<JumpTarget>) -> Self {
        Self {
            severity,
            message,
            jump,
        }
    }
}

/// A domain check: read-only over the Vfs + localisation, at a selected date,
/// returns its report. Date-independent domains ignore the date argument.
type DomainCheck = fn(&Vfs, &LocStore, Date) -> Vec<ValidationIssue>;

// Date-aware registry adapters. Only `climate` (impassable-owner) and
// `diplomacy` (active relations) are date-sensitive; the rest ignore it.
fn dom_areas(v: &Vfs, l: &LocStore, _d: Date) -> Vec<ValidationIssue> {
    check_areas(v, l)
}
fn dom_continents(v: &Vfs, l: &LocStore, _d: Date) -> Vec<ValidationIssue> {
    check_continents(v, l)
}
fn dom_climate(v: &Vfs, l: &LocStore, d: Date) -> Vec<ValidationIssue> {
    check_climate_at(v, l, d)
}
fn dom_diplomacy(v: &Vfs, l: &LocStore, d: Date) -> Vec<ValidationIssue> {
    crate::diplomacy::check_diplomacy_at(v, l, d)
}
fn dom_trade_nodes(v: &Vfs, l: &LocStore, _d: Date) -> Vec<ValidationIssue> {
    check_trade_nodes(v, l)
}
fn dom_wars(v: &Vfs, l: &LocStore, d: Date) -> Vec<ValidationIssue> {
    crate::wars::check_wars(v, l, d)
}
fn dom_colonial_regions(v: &Vfs, l: &LocStore, _d: Date) -> Vec<ValidationIssue> {
    check_colonial(v, l, "colonial_regions")
}
fn dom_trade_companies(v: &Vfs, l: &LocStore, _d: Date) -> Vec<ValidationIssue> {
    check_colonial(v, l, "trade_companies")
}
fn dom_units(v: &Vfs, l: &LocStore, _d: Date) -> Vec<ValidationIssue> {
    crate::technology::validate_units(v, l)
}

/// The domain registry. Sprints add cheaply by appending a `(name, fn)` row.
const DOMAIN_REGISTRY: &[(&str, DomainCheck)] = &[
    ("areas", dom_areas),
    ("continents", dom_continents),
    ("climate", dom_climate),
    ("diplomacy", dom_diplomacy),
    ("trade_nodes", dom_trade_nodes),
    ("wars", dom_wars),
    ("colonial_regions", dom_colonial_regions),
    ("trade_companies", dom_trade_companies),
    ("units", dom_units),
];

/// Runs one validation domain and returns its report.
// Registered by the orchestrator in lib.rs; unused until then.
#[allow(dead_code)]
#[tauri::command(async)]
pub fn validate(
    domain: String,
    install_path: String,
    mod_path: Option<String>,
    date: Option<String>,
) -> Result<Vec<ValidationIssue>, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    let loc = crate::loc::store(&vfs, &install_path, mod_path.as_deref());
    let at = crate::bookmarks::resolve_date(&vfs, date.as_deref())?;
    let check = DOMAIN_REGISTRY
        .iter()
        .find(|(name, _)| *name == domain)
        .map(|(_, f)| *f)
        .ok_or_else(|| format!("Unknown validation domain: {domain}"))?;
    Ok(check(&vfs, &loc, at))
}

/// The names of every registered domain (handy for a "run all" UI later).
#[allow(dead_code)]
pub fn domains() -> Vec<&'static str> {
    DOMAIN_REGISTRY.iter().map(|(name, _)| *name).collect()
}

/// One domain's slice of an aggregate ([`validate_all`]) run: the domain name
/// tag plus its full report. Serializes as `{ "domain": "areas", "issues": [ … ] }`.
#[derive(serde::Serialize, Clone, Debug)]
pub struct DomainReport {
    pub domain: String,
    pub issues: Vec<ValidationIssue>,
}

/// Sprint 30.2 — the Problems dashboard aggregate. Runs EVERY registered domain
/// in one pass (one Vfs + LocStore + resolved date, shared across domains) and
/// returns their domain-tagged reports in registry order. This does NOT
/// re-implement any check: it dispatches the exact same [`DomainCheck`] fns the
/// single-domain [`validate`] command uses, so `validate_all`'s per-domain issue
/// list is byte-identical to calling `validate(domain, …)` for that domain.
/// Date-aware domains (climate, diplomacy, wars) resolve the date the same way.
#[allow(dead_code)]
#[tauri::command(async)]
pub fn validate_all(
    install_path: String,
    mod_path: Option<String>,
    date: Option<String>,
) -> Result<Vec<DomainReport>, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    let loc = crate::loc::store(&vfs, &install_path, mod_path.as_deref());
    let at = crate::bookmarks::resolve_date(&vfs, date.as_deref())?;
    Ok(DOMAIN_REGISTRY
        .iter()
        .map(|(name, check)| DomainReport {
            domain: name.to_string(),
            issues: check(&vfs, &loc, at),
        })
        .collect())
}

// --- shared local parse helpers -----------------------------------------

fn parse_block(vfs: &Vfs, rel: &str) -> Option<Block> {
    vfs.read(rel)
        .ok()
        .map(|b| paradox::parse(&String::from_utf8_lossy(&b)))
}

/// Every province id in the universe, from definition.csv (the header row's
/// `province` token simply fails to parse and is skipped).
fn province_ids(vfs: &Vfs) -> HashSet<u32> {
    let mut out = HashSet::new();
    if let Ok(bytes) = vfs.read("map/definition.csv") {
        for line in String::from_utf8_lossy(&bytes).lines() {
            if let Some(Ok(id)) = line.split(';').next().map(|s| s.trim().parse::<u32>()) {
                out.insert(id);
            }
        }
    }
    out
}

/// Bare ids of a named `key = { … }` list inside `block`.
fn id_set(block: Option<&Block>, key: &str) -> HashSet<u32> {
    block
        .and_then(|b| b.get_block(key))
        .map(|b| b.bare_ids().into_iter().collect())
        .unwrap_or_default()
}

/// Owner as of `at` (top level, then dated `owner` changes with a date ≤ `at`,
/// in file order) for each of the given province ids. Reads only the requested
/// history files.
fn top_level_owners(vfs: &Vfs, ids: &HashSet<u32>, at: Date) -> HashMap<u32, String> {
    let mut out = HashMap::new();
    if ids.is_empty() {
        return out;
    }
    for ast in crate::game_data::province_asts(vfs).iter() {
        let id = ast.id;
        if !ids.contains(&id) {
            continue;
        }
        if let Some(block) = &ast.block {
            let mut owner = block.get_scalar("owner").map(str::to_string);
            for (k, v) in &block.items {
                let (Some(k), crate::paradox::Value::Block(b)) = (k, v) else {
                    continue;
                };
                match crate::date::parse_date(k) {
                    Some(d) if d <= at => {
                        if let Some(o) = b.get_scalar("owner") {
                            owner = Some(o.to_string());
                        }
                    }
                    _ => {}
                }
            }
            if let Some(owner) = owner {
                out.insert(id, owner);
            }
        }
    }
    out
}

// --- domain: areas -------------------------------------------------------
//
// SPRINT 10.1/10.2 consumer. Checks the area→region→superregion hierarchy:
//   * land provinces assigned to no area (game needs areas for states)
//   * areas with zero provinces
//   * (non-empty) areas belonging to no region
//   * regions belonging to no superregion
//
// Land = definition.csv minus water (default.map sea_starts + lakes), minus
// RNW placeholders (default.map only_used_for_random), minus impassable
// wastelands (climate.txt) — those legitimately have no area. Vanilla comes
// out with only its genuinely-empty area stubs (netherlands_area = { } and a
// handful of others) and random_new_world_region (no superregion), all warnings.
fn check_areas(vfs: &Vfs, loc: &LocStore) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();

    let defs = province_ids(vfs);
    let default_map = parse_block(vfs, "map/default.map");
    let climate = parse_block(vfs, "map/climate.txt");

    let mut excluded = id_set(default_map.as_ref(), "sea_starts");
    excluded.extend(id_set(default_map.as_ref(), "lakes"));
    excluded.extend(id_set(default_map.as_ref(), "only_used_for_random"));
    excluded.extend(id_set(climate.as_ref(), "impassable"));

    // Area membership (all areas, including empty stubs, in file order).
    let area_block = parse_block(vfs, "map/area.txt").unwrap_or_default();
    let mut assigned: HashSet<u32> = HashSet::new();
    let mut areas: Vec<(String, Vec<u32>)> = Vec::new();
    for (name, b) in area_block.key_blocks() {
        let ids = b.bare_ids();
        assigned.extend(&ids);
        areas.push((name.to_string(), ids));
    }

    // 1. Land provinces with no area.
    let mut orphan_land: Vec<u32> = defs
        .iter()
        .filter(|id| !excluded.contains(id) && !assigned.contains(id))
        .copied()
        .collect();
    orphan_land.sort_unstable();
    for id in orphan_land {
        let pname = loc.get(&format!("PROV{id}")).map(str::to_string);
        let label = pname
            .map(|n| format!("{id} ({n})"))
            .unwrap_or_else(|| id.to_string());
        issues.push(ValidationIssue::new(
            Severity::Warning,
            format!("Land province {label} is not assigned to any area"),
            Some(JumpTarget::Province(id)),
        ));
    }

    // 2. Areas with zero provinces.
    for (name, ids) in &areas {
        if ids.is_empty() {
            issues.push(ValidationIssue::new(
                Severity::Warning,
                format!("Area \"{}\" has no provinces", loc.resolve(name)),
                Some(JumpTarget::Area(name.clone())),
            ));
        }
    }

    // Region → area membership.
    let region_block = parse_block(vfs, "map/region.txt").unwrap_or_default();
    let mut region_areas: HashSet<String> = HashSet::new();
    let mut regions: Vec<String> = Vec::new();
    for (name, b) in region_block.key_blocks() {
        if let Some(area_list) = b.get_block("areas") {
            for a in area_list.bare_scalars() {
                region_areas.insert(a.to_string());
            }
        }
        regions.push(name.to_string());
    }

    // 3. Non-empty areas in no region (empty ones are already flagged above).
    for (name, ids) in &areas {
        if !ids.is_empty() && !region_areas.contains(name) {
            issues.push(ValidationIssue::new(
                Severity::Warning,
                format!("Area \"{}\" belongs to no region", loc.resolve(name)),
                Some(JumpTarget::Area(name.clone())),
            ));
        }
    }

    // Superregion → region membership.
    let super_block = parse_block(vfs, "map/superregion.txt").unwrap_or_default();
    let mut super_regions: HashSet<String> = HashSet::new();
    for (_name, b) in super_block.key_blocks() {
        for r in b.bare_scalars() {
            super_regions.insert(r.to_string());
        }
    }

    // 4. Regions in no superregion.
    for name in &regions {
        if !super_regions.contains(name) {
            issues.push(ValidationIssue::new(
                Severity::Warning,
                format!("Region \"{}\" belongs to no superregion", loc.resolve(name)),
                None,
            ));
        }
    }

    issues
}

// --- domain: continents --------------------------------------------------
//
// S3.1 consumer. `map/continent.txt` is the same bare-id list as area.txt. The
// game errors on:
//   * a land province on no continent            → Error
//   * a continent with zero provinces            → Error
// Land = definition.csv minus water (default.map sea_starts + lakes) minus RNW
// placeholders (only_used_for_random). Unlike areas, impassable wastelands DO
// carry a continent in vanilla, so they are NOT excluded here. Two blocks in
// continent.txt are not continents/are legitimately empty and are exempt:
//   * `island_check_provinces` — an engine helper list, never a continent
//   * `new_world` — the RNW continent, ships empty in vanilla
// Verified against the real install: vanilla produces zero continent errors.

/// Blocks in continent.txt that are not real continents (never flagged, never
/// counted toward a province's continent membership).
const CONTINENT_NON_CONTINENT_BLOCKS: &[&str] = &["island_check_provinces"];
/// Real continents that may legitimately hold zero provinces.
const CONTINENT_ALLOW_EMPTY: &[&str] = &["new_world"];

fn check_continents(vfs: &Vfs, loc: &LocStore) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();

    let defs = province_ids(vfs);
    let default_map = parse_block(vfs, "map/default.map");

    // Water + RNW placeholders are excluded; wastelands keep their continent.
    let mut excluded = id_set(default_map.as_ref(), "sea_starts");
    excluded.extend(id_set(default_map.as_ref(), "lakes"));
    excluded.extend(id_set(default_map.as_ref(), "only_used_for_random"));

    let continent_block = parse_block(vfs, "map/continent.txt").unwrap_or_default();
    let mut assigned: HashSet<u32> = HashSet::new();
    let mut continents: Vec<(String, Vec<u32>)> = Vec::new();
    for (name, b) in continent_block.key_blocks() {
        if CONTINENT_NON_CONTINENT_BLOCKS.contains(&name) {
            continue;
        }
        let ids = b.bare_ids();
        assigned.extend(&ids);
        continents.push((name.to_string(), ids));
    }

    // 1. Land provinces on no continent (error).
    let mut orphan_land: Vec<u32> = defs
        .iter()
        .filter(|id| !excluded.contains(id) && !assigned.contains(id))
        .copied()
        .collect();
    orphan_land.sort_unstable();
    for id in orphan_land {
        let pname = loc.get(&format!("PROV{id}")).map(str::to_string);
        let label = pname
            .map(|n| format!("{id} ({n})"))
            .unwrap_or_else(|| id.to_string());
        issues.push(ValidationIssue::new(
            Severity::Error,
            format!("Land province {label} is not on any continent"),
            Some(JumpTarget::Province(id)),
        ));
    }

    // 2. Continents with zero provinces (error; new_world is legitimately empty).
    for (name, ids) in &continents {
        if ids.is_empty() && !CONTINENT_ALLOW_EMPTY.contains(&name.as_str()) {
            issues.push(ValidationIssue::new(
                Severity::Error,
                format!("Continent \"{}\" has no provinces", loc.resolve(name)),
                None,
            ));
        }
    }

    issues
}

// --- domain: colonial regions / trade companies (Sprint 19) --------------
//
// Shared check over both membership modes (kind selects the directory):
//   * a province assigned to two+ entries → Warning (overlap; the game keeps
//     only the first-loaded, silently dropping the province from the rest)
//   * an entry with zero provinces        → Warning (empty region/company)
// Both are warnings — most of the map is legitimately in neither, and an empty
// entry loads fine, it just does nothing.
fn check_colonial(vfs: &Vfs, loc: &LocStore, kind: &str) -> Vec<ValidationIssue> {
    let (label, jump): (&str, fn(String) -> JumpTarget) = match kind {
        "colonial_regions" => ("Colonial region", JumpTarget::ColonialRegion),
        _ => ("Trade company", JumpTarget::TradeCompany),
    };
    let entries = crate::colonial::membership(vfs, kind);
    let mut issues = Vec::new();

    // Province → the entries that claim it (in file order).
    let mut claims: HashMap<u32, Vec<String>> = HashMap::new();
    for (key, _color, ids) in &entries {
        for id in ids {
            claims.entry(*id).or_default().push(key.clone());
        }
    }
    let mut overlaps: Vec<(u32, Vec<String>)> = claims
        .into_iter()
        .filter(|(_, ks)| ks.len() > 1)
        .collect();
    overlaps.sort_by_key(|(id, _)| *id);
    for (id, keys) in overlaps {
        let names: Vec<String> = keys.iter().map(|k| loc.resolve(k)).collect();
        issues.push(ValidationIssue::new(
            Severity::Warning,
            format!(
                "Province {id} is in {} {}: {}",
                keys.len(),
                if kind == "colonial_regions" { "colonial regions" } else { "trade companies" },
                names.join(", ")
            ),
            Some(jump(keys[0].clone())),
        ));
    }

    // Entries with zero provinces.
    for (key, _color, ids) in &entries {
        if ids.is_empty() {
            issues.push(ValidationIssue::new(
                Severity::Warning,
                format!("{label} \"{}\" has no provinces", loc.resolve(key)),
                Some(jump(key.clone())),
            ));
        }
    }

    issues
}

// --- domain: climate -----------------------------------------------------
//
// SPRINT 11.1 consumer. climate.txt has independent slots; a province may sit
// in one entry per slot. Two entries of the same slot is file corruption the
// game tolerates unpredictably:
//   * climate-zone slot (tropical/arid/arctic)   → Error (vanilla-clean)
//   * winter-severity slot (mild/normal/severe)  → Error (vanilla-clean)
//   * monsoon slot (mild/normal/severe_monsoon)  → Warning — vanilla ships 3
//     overlaps (2888/2904/2905 in mild+normal_monsoon), so the game clearly
//     tolerates it; surfaced but not an error.
// Plus: impassable provinces that have an owner — legal but odd → Info.
#[cfg(test)]
fn check_climate(vfs: &Vfs, loc: &LocStore) -> Vec<ValidationIssue> {
    check_climate_at(vfs, loc, crate::date::DEFAULT_START)
}

fn check_climate_at(vfs: &Vfs, _loc: &LocStore, at: Date) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();
    let climate = parse_block(vfs, "map/climate.txt").unwrap_or_default();

    // Every zone/slot list, keyed by name.
    let mut zone_ids: HashMap<String, Vec<u32>> = HashMap::new();
    for (name, b) in climate.key_blocks() {
        zone_ids.entry(name.to_string()).or_default().extend(b.bare_ids());
    }

    let slots: [(&[&str], Severity); 3] = [
        (&["tropical", "arid", "arctic"], Severity::Error),
        (
            &["mild_winter", "normal_winter", "severe_winter"],
            Severity::Error,
        ),
        (
            &["mild_monsoon", "normal_monsoon", "severe_monsoon"],
            Severity::Warning,
        ),
    ];

    for (zones, severity) in slots {
        // province id -> first zone it was seen in, within this slot.
        let mut seen: HashMap<u32, String> = HashMap::new();
        let mut dups: Vec<(u32, String, String)> = Vec::new();
        let mut reported: HashSet<u32> = HashSet::new();
        for z in zones {
            let Some(ids) = zone_ids.get(*z) else {
                continue;
            };
            for &id in ids {
                match seen.get(&id) {
                    Some(prev) if prev != z && reported.insert(id) => {
                        dups.push((id, prev.clone(), z.to_string()));
                    }
                    None => {
                        seen.insert(id, z.to_string());
                    }
                    _ => {}
                }
            }
        }
        dups.sort_unstable_by_key(|(id, _, _)| *id);
        for (id, a, b) in dups {
            issues.push(ValidationIssue::new(
                severity,
                format!("Province {id} appears in two climate zones of the same slot ({a} and {b})"),
                Some(JumpTarget::Province(id)),
            ));
        }
    }

    // Impassable provinces that have an owner (legal but weird).
    let impassable: HashSet<u32> = zone_ids
        .get("impassable")
        .map(|v| v.iter().copied().collect())
        .unwrap_or_default();
    let owners = top_level_owners(vfs, &impassable, at);
    let mut owned: Vec<(u32, String)> = owners.into_iter().collect();
    owned.sort_unstable_by_key(|(id, _)| *id);
    for (id, owner) in owned {
        issues.push(ValidationIssue::new(
            Severity::Info,
            format!("Impassable province {id} has an owner ({owner})"),
            Some(JumpTarget::Province(id)),
        ));
    }

    issues
}

// --- domain: trade_nodes -------------------------------------------------
//
// SPRINT 8.6 consumer — the deepest graph check. Reads the full node/route
// graph via `tradenodes::node_graph` and reports:
//   ERRORS (vanilla-clean):
//     * a route whose target node doesn't exist
//     * a node whose `location` is not one of its own members
//     * a non-`end` node with zero outgoing routes (a dead-end sink)
//     * a steering cycle (DFS back-edge) — breaks the game's economy
//     * a node that cannot reach any `end` node by following routes
//   WARNINGS (advisory; vanilla ships some):
//     * a node with zero members
//     * an `end` node that nonetheless has outgoing routes
//     * a route whose `path` is empty, or whose path endpoints touch neither
//       the source's members (first id) nor the target's (last id)
//
// Grounding: vanilla's 3 end nodes (genua/venice/english_channel) carry no
// outgoing, every non-end node steers toward an end, no cycles exist, and each
// node's location sits in its own members — so the ERROR checks are clean.
fn check_trade_nodes(vfs: &Vfs, loc: &LocStore) -> Vec<ValidationIssue> {
    let graph = crate::tradenodes::node_graph(vfs);
    let mut issues = Vec::new();

    let keys: HashSet<&str> = graph.iter().map(|n| n.key.as_str()).collect();
    let index: HashMap<&str, usize> = graph
        .iter()
        .enumerate()
        .map(|(i, n)| (n.key.as_str(), i))
        .collect();
    let name = |n: &crate::tradenodes::GraphNode| loc.resolve(&n.key);

    for n in &graph {
        // Zero members (warning).
        if n.members.is_empty() {
            issues.push(ValidationIssue::new(
                Severity::Warning,
                format!("Trade node \"{}\" has no members", name(n)),
                Some(JumpTarget::Node(n.key.clone())),
            ));
        }
        // location must be a member of its own node (error).
        if let Some(locp) = n.location {
            if !n.members.contains(&locp) {
                issues.push(ValidationIssue::new(
                    Severity::Error,
                    format!(
                        "Trade node \"{}\" collection province {locp} is not one of its members",
                        name(n)
                    ),
                    Some(JumpTarget::Node(n.key.clone())),
                ));
            }
        }
        // Non-end node needs at least one outgoing route (error).
        if !n.end && n.routes.is_empty() {
            issues.push(ValidationIssue::new(
                Severity::Error,
                format!(
                    "Trade node \"{}\" is not an end node but has no outgoing routes",
                    name(n)
                ),
                Some(JumpTarget::Node(n.key.clone())),
            ));
        }
        // End node with outgoing routes (warning).
        if n.end && !n.routes.is_empty() {
            issues.push(ValidationIssue::new(
                Severity::Warning,
                format!(
                    "End trade node \"{}\" has {} outgoing route(s)",
                    name(n),
                    n.routes.len()
                ),
                Some(JumpTarget::Node(n.key.clone())),
            ));
        }
        for r in &n.routes {
            // Route endpoint must exist (error).
            if !keys.contains(r.target.as_str()) {
                issues.push(ValidationIssue::new(
                    Severity::Error,
                    format!(
                        "Trade node \"{}\" has a route to unknown node \"{}\"",
                        name(n),
                        r.target
                    ),
                    Some(JumpTarget::Node(n.key.clone())),
                ));
                continue;
            }
            // Path connectivity (warning). Route-corridor seas legitimately
            // belong to NO node (known gotcha), so path provinces are
            // normally not members of either endpoint — the only reliable
            // "disconnected" signal is an empty path. Vanilla is clean.
            if r.path.is_empty() {
                issues.push(ValidationIssue::new(
                    Severity::Warning,
                    format!(
                        "Route \"{}\" → \"{}\" has an empty path",
                        name(n),
                        loc.resolve(&r.target)
                    ),
                    Some(JumpTarget::Node(n.key.clone())),
                ));
            }
        }
    }

    // Cycle detection (DFS colors) over steering edges (error, reported once).
    #[derive(Clone, Copy, PartialEq)]
    enum Color {
        White,
        Gray,
        Black,
    }
    let n = graph.len();
    let mut color = vec![Color::White; n];
    let mut cycle_reported = false;
    // Iterative DFS to avoid stack overflow on deep chains.
    for start in 0..n {
        if color[start] != Color::White {
            continue;
        }
        let mut stack: Vec<(usize, usize)> = vec![(start, 0)];
        color[start] = Color::Gray;
        while let Some(&mut (node, ref mut edge)) = stack.last_mut() {
            if *edge < graph[node].routes.len() {
                let target = &graph[node].routes[*edge].target;
                *edge += 1;
                if let Some(&t) = index.get(target.as_str()) {
                    match color[t] {
                        Color::White => {
                            color[t] = Color::Gray;
                            stack.push((t, 0));
                        }
                        Color::Gray if !cycle_reported => {
                            cycle_reported = true;
                            issues.push(ValidationIssue::new(
                                Severity::Error,
                                format!(
                                    "Trade route cycle detected involving \"{}\" → \"{}\"",
                                    name(&graph[node]),
                                    loc.resolve(target)
                                ),
                                Some(JumpTarget::Node(graph[node].key.clone())),
                            ));
                        }
                        _ => {}
                    }
                }
            } else {
                color[node] = Color::Black;
                stack.pop();
            }
        }
    }

    // Reachability: every node must reach an `end` node (error). Memoized.
    // 0 = unknown, 1 = reaches end, 2 = does not.
    let mut reach = vec![0u8; n];
    fn reaches_end(
        i: usize,
        graph: &[crate::tradenodes::GraphNode],
        index: &HashMap<&str, usize>,
        reach: &mut [u8],
        visiting: &mut HashSet<usize>,
    ) -> bool {
        if graph[i].end {
            return true;
        }
        if reach[i] != 0 {
            return reach[i] == 1;
        }
        if !visiting.insert(i) {
            return false; // on the current DFS path (cycle) — no end this way
        }
        let mut ok = false;
        for r in &graph[i].routes {
            if let Some(&t) = index.get(r.target.as_str()) {
                if reaches_end(t, graph, index, reach, visiting) {
                    ok = true;
                    break;
                }
            }
        }
        visiting.remove(&i);
        reach[i] = if ok { 1 } else { 2 };
        ok
    }
    for i in 0..n {
        let mut visiting = HashSet::new();
        if !graph[i].end && !reaches_end(i, &graph, &index, &mut reach, &mut visiting) {
            issues.push(ValidationIssue::new(
                Severity::Error,
                format!(
                    "Trade node \"{}\" cannot reach any end node",
                    name(&graph[i])
                ),
                Some(JumpTarget::Node(graph[i].key.clone())),
            ));
        }
    }

    issues
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    const INSTALL: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Europa Universalis IV";
    const ANBENNAR: &str = r"C:\Users\User\projects\anbennar-eu4-dev";

    /// Writes a synthetic install with the given map/history files and returns
    /// (root, Vfs). One dir per test — parallel tests must not share a temp dir.
    fn synthetic(name: &str, files: &[(&str, &str)]) -> (PathBuf, Vfs) {
        let root = std::env::temp_dir().join(format!("eu_toolkit_validation_test_{name}"));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("map")).unwrap();
        std::fs::create_dir_all(root.join("history/provinces")).unwrap();
        std::fs::write(root.join("map/provinces.bmp"), b"x").unwrap();
        for (rel, contents) in files {
            let path = root.join(rel);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(path, contents).unwrap();
        }
        let vfs = Vfs::new(root.to_str().unwrap(), None).unwrap();
        (root, vfs)
    }

    fn loc_for(vfs: &Vfs) -> crate::loc::LocStore {
        crate::loc::build(vfs)
    }

    fn has_jump(issues: &[ValidationIssue], jump: &JumpTarget) -> bool {
        issues.iter().any(|i| i.jump.as_ref() == Some(jump))
    }

    #[test]
    fn unassigned_land_reported_with_province_jump() {
        // Province 1 = land (no area), 2 = sea. Province 1 must be flagged.
        let (_root, vfs) = synthetic(
            "unassigned_land",
            &[
                (
                    "map/definition.csv",
                    "province;red;green;blue;name;x\n1;1;1;1;Aa;x\n2;2;2;2;Sea;x\n",
                ),
                ("map/default.map", "sea_starts = { 2 }\nlakes = { }\n"),
                ("map/climate.txt", "impassable = { }\n"),
                ("map/area.txt", ""),
                ("map/region.txt", ""),
                ("map/superregion.txt", ""),
            ],
        );
        let loc = loc_for(&vfs);
        let issues = check_areas(&vfs, &loc);
        assert!(
            has_jump(&issues, &JumpTarget::Province(1)),
            "expected province 1 flagged as unassigned land: {issues:?}"
        );
        // The sea tile (2) must NOT be flagged.
        assert!(!has_jump(&issues, &JumpTarget::Province(2)));
    }

    #[test]
    fn area_missing_from_region_reported() {
        // orphan_area holds province 1 but no region references it.
        let (_root, vfs) = synthetic(
            "orphan_area",
            &[
                (
                    "map/definition.csv",
                    "province;red;green;blue;name;x\n1;1;1;1;Aa;x\n",
                ),
                ("map/default.map", "sea_starts = { }\n"),
                ("map/climate.txt", "impassable = { }\n"),
                ("map/area.txt", "orphan_area = { 1 }\n"),
                ("map/region.txt", ""),
                ("map/superregion.txt", ""),
            ],
        );
        let loc = loc_for(&vfs);
        let issues = check_areas(&vfs, &loc);
        assert!(
            has_jump(&issues, &JumpTarget::Area("orphan_area".into())),
            "expected orphan_area flagged as region-less: {issues:?}"
        );
        // Province 1 IS assigned, so no unassigned-land issue for it.
        assert!(!has_jump(&issues, &JumpTarget::Province(1)));
    }

    #[test]
    fn empty_area_reported() {
        let (_root, vfs) = synthetic(
            "empty_area",
            &[
                (
                    "map/definition.csv",
                    "province;red;green;blue;name;x\n1;1;1;1;Aa;x\n",
                ),
                ("map/default.map", "sea_starts = { }\n"),
                ("map/climate.txt", "impassable = { }\n"),
                ("map/area.txt", "a_area = { 1 }\nempty_area = { }\n"),
                ("map/region.txt", "a_region = { areas = { a_area empty_area } }\n"),
                ("map/superregion.txt", "a_super = { a_region }\n"),
            ],
        );
        let loc = loc_for(&vfs);
        let issues = check_areas(&vfs, &loc);
        let empty = issues.iter().find(|i| {
            i.jump == Some(JumpTarget::Area("empty_area".into()))
                && i.message.contains("no provinces")
        });
        assert!(empty.is_some(), "expected empty_area flagged: {issues:?}");
        assert_eq!(empty.unwrap().severity, Severity::Warning);
    }

    #[test]
    fn duplicate_zone_climate_reported() {
        // Province 1 is in both tropical and arctic (same slot) → Error.
        let (_root, vfs) = synthetic(
            "dup_climate",
            &[
                (
                    "map/definition.csv",
                    "province;red;green;blue;name;x\n1;1;1;1;Aa;x\n",
                ),
                (
                    "map/climate.txt",
                    "tropical = { 1 }\narctic = { 1 }\nimpassable = { }\n",
                ),
            ],
        );
        let loc = loc_for(&vfs);
        let issues = check_climate(&vfs, &loc);
        let dup = issues
            .iter()
            .find(|i| i.jump == Some(JumpTarget::Province(1)));
        assert!(dup.is_some(), "expected province 1 dup-zone flagged: {issues:?}");
        assert_eq!(dup.unwrap().severity, Severity::Error);
    }

    #[test]
    fn impassable_with_owner_reported_as_info() {
        let (_root, vfs) = synthetic(
            "impassable_owner",
            &[
                (
                    "map/definition.csv",
                    "province;red;green;blue;name;x\n1;1;1;1;Aa;x\n",
                ),
                ("map/climate.txt", "impassable = { 1 }\n"),
                (
                    "history/provinces/1 - Aa.txt",
                    "owner = SWE\ncontroller = SWE\n",
                ),
            ],
        );
        let loc = loc_for(&vfs);
        let issues = check_climate(&vfs, &loc);
        let info = issues
            .iter()
            .find(|i| i.jump == Some(JumpTarget::Province(1)));
        assert!(info.is_some(), "expected impassable-with-owner info: {issues:?}");
        assert_eq!(info.unwrap().severity, Severity::Info);
    }

    #[test]
    fn clean_fixture_empty_report() {
        // Fully consistent: all land assigned, areas in regions, regions in a
        // superregion, no climate dups, no owned impassable.
        let (_root, vfs) = synthetic(
            "clean",
            &[
                (
                    "map/definition.csv",
                    "province;red;green;blue;name;x\n1;1;1;1;Aa;x\n2;2;2;2;Bb;x\n3;3;3;3;Sea;x\n",
                ),
                ("map/default.map", "sea_starts = { 3 }\nlakes = { }\n"),
                (
                    "map/climate.txt",
                    "tropical = { 1 }\nmild_winter = { 2 }\nimpassable = { }\n",
                ),
                ("map/area.txt", "a_area = { 1 2 }\n"),
                ("map/region.txt", "a_region = { areas = { a_area } }\n"),
                ("map/superregion.txt", "a_super = { a_region }\n"),
            ],
        );
        let loc = loc_for(&vfs);
        assert!(
            check_areas(&vfs, &loc).is_empty(),
            "areas: {:?}",
            check_areas(&vfs, &loc)
        );
        assert!(
            check_climate(&vfs, &loc).is_empty(),
            "climate: {:?}",
            check_climate(&vfs, &loc)
        );
    }

    // --- continents domain (S3.1) --------------------------------------

    #[test]
    fn land_without_continent_reported_as_error() {
        // Province 1 = land on no continent; 2 = land on europe; 3 = sea. Only 1
        // is flagged, as an error with a province jump.
        let (_root, vfs) = synthetic(
            "land_no_continent",
            &[
                (
                    "map/definition.csv",
                    "province;red;green;blue;name;x\n1;1;1;1;Aa;x\n2;2;2;2;Bb;x\n3;3;3;3;Sea;x\n",
                ),
                ("map/default.map", "sea_starts = { 3 }\nlakes = { }\n"),
                ("map/continent.txt", "europe = { 2 }\n"),
            ],
        );
        let loc = loc_for(&vfs);
        let issues = check_continents(&vfs, &loc);
        let one = issues
            .iter()
            .find(|i| i.jump == Some(JumpTarget::Province(1)));
        assert!(one.is_some(), "expected province 1 flagged: {issues:?}");
        assert_eq!(one.unwrap().severity, Severity::Error);
        assert!(!has_jump(&issues, &JumpTarget::Province(2)), "2 is on a continent");
        assert!(!has_jump(&issues, &JumpTarget::Province(3)), "3 is sea");
    }

    #[test]
    fn empty_continent_reported_as_error_but_new_world_exempt() {
        // asia has no provinces → error. new_world is empty but exempt.
        // island_check_provinces is a helper block, never treated as a continent.
        let (_root, vfs) = synthetic(
            "empty_continent",
            &[
                (
                    "map/definition.csv",
                    "province;red;green;blue;name;x\n1;1;1;1;Aa;x\n",
                ),
                ("map/default.map", "sea_starts = { }\nlakes = { }\n"),
                (
                    "map/continent.txt",
                    "europe = { 1 }\nasia = { }\nnew_world = { }\nisland_check_provinces = { 1 }\n",
                ),
            ],
        );
        let loc = loc_for(&vfs);
        let issues = check_continents(&vfs, &loc);
        let asia = issues
            .iter()
            .find(|i| i.severity == Severity::Error && i.message.to_lowercase().contains("asia"));
        assert!(asia.is_some(), "expected asia empty-continent error: {issues:?}");
        assert!(
            !issues.iter().any(|i| i.message.to_lowercase().contains("new_world")),
            "new_world must be exempt: {issues:?}"
        );
        assert!(
            !issues.iter().any(|i| i.message.to_lowercase().contains("island_check")),
            "island_check_provinces is not a continent: {issues:?}"
        );
    }

    #[test]
    fn clean_continents_empty_report() {
        // All land on continents, no empty continent (bar new_world). Wasteland /
        // impassable provinces still need a continent — 4 is impassable AND on a
        // continent, so it is fine.
        let (_root, vfs) = synthetic(
            "clean_continents",
            &[
                (
                    "map/definition.csv",
                    "province;red;green;blue;name;x\n1;1;1;1;Aa;x\n2;2;2;2;Bb;x\n3;3;3;3;Sea;x\n4;4;4;4;Waste;x\n",
                ),
                ("map/default.map", "sea_starts = { 3 }\nlakes = { }\n"),
                ("map/climate.txt", "impassable = { 4 }\n"),
                ("map/continent.txt", "europe = { 1 2 4 }\nnew_world = { }\n"),
            ],
        );
        let loc = loc_for(&vfs);
        assert!(
            check_continents(&vfs, &loc).is_empty(),
            "continents: {:?}",
            check_continents(&vfs, &loc)
        );
    }

    #[test]
    fn jump_target_serializes_adjacently_tagged() {
        let j = serde_json::to_value(JumpTarget::Province(42)).unwrap();
        assert_eq!(j["kind"], "province");
        assert_eq!(j["id"], 42);
        let c = serde_json::to_value(JumpTarget::Country("FRA".into())).unwrap();
        assert_eq!(c["kind"], "country");
        assert_eq!(c["id"], "FRA");
        let s = serde_json::to_value(Severity::Warning).unwrap();
        assert_eq!(s, "warning");
    }

    fn real_install() -> Option<Vfs> {
        Path::new(INSTALL)
            .join("map")
            .join("provinces.bmp")
            .is_file()
            .then(|| Vfs::new(INSTALL, None).unwrap())
    }

    #[test]
    fn real_install_domains_no_errors_and_fast() {
        let Some(vfs) = real_install() else { return };
        let loc = crate::loc::store(&vfs, INSTALL, None);

        let start = std::time::Instant::now();
        let areas = check_areas(&vfs, &loc);
        let continents = check_continents(&vfs, &loc);
        let climate = check_climate(&vfs, &loc);
        let elapsed = start.elapsed();

        // Print the report so the orchestrator can eyeball vanilla's warnings.
        let summary = |name: &str, issues: &[ValidationIssue]| {
            let errs = issues.iter().filter(|i| i.severity == Severity::Error).count();
            let warns = issues.iter().filter(|i| i.severity == Severity::Warning).count();
            let infos = issues.iter().filter(|i| i.severity == Severity::Info).count();
            println!(
                "[validation:{name}] {} issues ({errs} error, {warns} warning, {infos} info)",
                issues.len()
            );
            for i in issues {
                println!("    {:?}: {}", i.severity, i.message);
            }
        };
        summary("areas", &areas);
        summary("continents", &continents);
        summary("climate", &climate);
        println!("[validation] domains took {elapsed:?}");

        // No Errors in vanilla (warnings/info are expected & allowed).
        assert_eq!(
            areas.iter().filter(|i| i.severity == Severity::Error).count(),
            0,
            "vanilla areas produced errors"
        );
        assert_eq!(
            continents
                .iter()
                .filter(|i| i.severity == Severity::Error)
                .count(),
            0,
            "vanilla continents produced errors"
        );
        assert_eq!(
            climate
                .iter()
                .filter(|i| i.severity == Severity::Error)
                .count(),
            0,
            "vanilla climate produced errors"
        );
        assert!(elapsed.as_secs_f32() < 2.0, "domains too slow: {elapsed:?}");
    }

    // --- colonial regions / trade companies domain (Sprint 19) ----------

    #[test]
    fn colonial_overlap_and_empty_warned() {
        // colonial_a claims 1 2 3 (shares 3 with colonial_b); colonial_empty has
        // no provinces. Expect: province 3 overlap warning + empty-region warning.
        let (_root, vfs) = synthetic(
            "colonial_warn",
            &[(
                "common/colonial_regions/00.txt",
                "colonial_a = { color = { 1 2 3 } provinces = { 1 2 3 } names = { name = \"A\" } }\n\
                 colonial_b = { color = { 4 5 6 } provinces = { 3 4 } names = { name = \"B\" } }\n\
                 colonial_empty = { color = { 7 8 9 } provinces = { } names = { name = \"E\" } }\n",
            )],
        );
        let loc = loc_for(&vfs);
        let issues = check_colonial(&vfs, &loc, "colonial_regions");
        // Overlap on province 3 (message mentions the province + both regions).
        assert!(
            issues.iter().any(|i| i.message.contains("Province 3")
                && i.severity == Severity::Warning),
            "expected province-3 overlap warning: {issues:?}"
        );
        // Empty region flagged with a jump to it.
        assert!(
            has_jump(&issues, &JumpTarget::ColonialRegion("colonial_empty".into())),
            "expected empty-region warning with jump: {issues:?}"
        );
        // No false positives: province 1 (single claim) is not flagged.
        assert!(!issues.iter().any(|i| i.message.contains("Province 1")));
    }

    #[test]
    fn colonial_clean_synthetic_no_warnings() {
        let (_root, vfs) = synthetic(
            "colonial_clean",
            &[(
                "common/colonial_regions/00.txt",
                "colonial_a = { color = { 1 2 3 } provinces = { 1 2 } names = { name = \"A\" } }\n\
                 colonial_b = { color = { 4 5 6 } provinces = { 3 4 } names = { name = \"B\" } }\n",
            )],
        );
        let loc = loc_for(&vfs);
        let issues = check_colonial(&vfs, &loc, "colonial_regions");
        assert!(issues.is_empty(), "expected no warnings, got: {issues:?}");
    }

    // --- trade_nodes domain --------------------------------------------

    #[test]
    fn trade_nodes_clean_synthetic_no_errors() {
        let (_root, vfs) = synthetic(
            "tn_clean",
            &[(
                "common/tradenodes/00.txt",
                "a={ location=1 members={ 1 } outgoing={ name=\"e\" path={ 1 } } }\n\
                 e={ location=2 members={ 2 } end=yes }\n",
            )],
        );
        let loc = loc_for(&vfs);
        let issues = check_trade_nodes(&vfs, &loc);
        let errors: Vec<_> = issues
            .iter()
            .filter(|i| i.severity == Severity::Error)
            .collect();
        assert!(errors.is_empty(), "expected clean graph, got: {errors:?}");
    }

    #[test]
    fn trade_nodes_cycle_detected() {
        let (_root, vfs) = synthetic(
            "tn_cycle",
            &[(
                "common/tradenodes/00.txt",
                "a={ location=1 members={ 1 } outgoing={ name=\"b\" path={ 1 } } }\n\
                 b={ location=2 members={ 2 } outgoing={ name=\"a\" path={ 2 } } }\n",
            )],
        );
        let loc = loc_for(&vfs);
        let issues = check_trade_nodes(&vfs, &loc);
        assert!(
            issues
                .iter()
                .any(|i| i.severity == Severity::Error && i.message.contains("cycle")),
            "expected cycle error: {issues:?}"
        );
    }

    #[test]
    fn trade_nodes_unreachable_end_detected() {
        // a<->b loop with no end node: neither reaches an end.
        let (_root, vfs) = synthetic(
            "tn_unreachable",
            &[(
                "common/tradenodes/00.txt",
                "a={ location=1 members={ 1 } outgoing={ name=\"b\" path={ 1 } } }\n\
                 b={ location=2 members={ 2 } outgoing={ name=\"a\" path={ 2 } } }\n",
            )],
        );
        let loc = loc_for(&vfs);
        let issues = check_trade_nodes(&vfs, &loc);
        assert!(
            issues
                .iter()
                .any(|i| i.severity == Severity::Error && i.message.contains("cannot reach")),
            "expected unreachable-end error: {issues:?}"
        );
    }

    #[test]
    fn trade_nodes_orphan_location_detected() {
        let (_root, vfs) = synthetic(
            "tn_orphan",
            &[(
                "common/tradenodes/00.txt",
                "a={ location=99 members={ 1 2 } outgoing={ name=\"e\" path={ 2 } } }\n\
                 e={ location=3 members={ 3 } end=yes }\n",
            )],
        );
        let loc = loc_for(&vfs);
        let issues = check_trade_nodes(&vfs, &loc);
        let orphan = issues.iter().find(|i| {
            i.severity == Severity::Error
                && i.jump == Some(JumpTarget::Node("a".into()))
                && i.message.contains("collection province 99")
        });
        assert!(orphan.is_some(), "expected orphan-location error: {issues:?}");
    }

    #[test]
    fn trade_nodes_unknown_target_detected() {
        let (_root, vfs) = synthetic(
            "tn_unknown",
            &[(
                "common/tradenodes/00.txt",
                "a={ location=1 members={ 1 } outgoing={ name=\"ghost\" path={ 1 } } }\n\
                 e={ location=2 members={ 2 } end=yes }\n",
            )],
        );
        let loc = loc_for(&vfs);
        let issues = check_trade_nodes(&vfs, &loc);
        assert!(
            issues
                .iter()
                .any(|i| i.severity == Severity::Error && i.message.contains("unknown node")),
            "expected unknown-target error: {issues:?}"
        );
    }

    #[test]
    fn vanilla_trade_nodes_no_errors() {
        let Some(vfs) = real_install() else { return };
        let loc = crate::loc::store(&vfs, INSTALL, None);
        let start = std::time::Instant::now();
        let issues = check_trade_nodes(&vfs, &loc);
        let elapsed = start.elapsed();
        let errs = issues.iter().filter(|i| i.severity == Severity::Error).count();
        let warns = issues.iter().filter(|i| i.severity == Severity::Warning).count();
        println!("[validation:trade_nodes] {} issues ({errs} error, {warns} warning) in {elapsed:?}", issues.len());
        for i in &issues {
            println!("    {:?}: {}", i.severity, i.message);
        }
        assert_eq!(errs, 0, "vanilla trade_nodes produced errors");
    }

    #[test]
    fn anbennar_trade_nodes_report() {
        if !Path::new(INSTALL).join("map/provinces.bmp").is_file() || !Path::new(ANBENNAR).is_dir()
        {
            return;
        }
        let vfs = Vfs::new(INSTALL, Some(ANBENNAR)).unwrap();
        let loc = crate::loc::store(&vfs, INSTALL, Some(ANBENNAR));
        let issues = check_trade_nodes(&vfs, &loc);
        let errs = issues.iter().filter(|i| i.severity == Severity::Error).count();
        let warns = issues.iter().filter(|i| i.severity == Severity::Warning).count();
        println!("[validation:trade_nodes:anbennar] {} issues ({errs} error, {warns} warning)", issues.len());
        for i in &issues {
            println!("    {:?}: {}", i.severity, i.message);
        }
    }

    // --- validate_all aggregate (Sprint 30.2) --------------------------

    #[test]
    fn validate_all_tags_every_domain_and_matches_single_runs() {
        // A synthetic install exercising a few domains: an orphan area province,
        // a duplicate climate zone, and a colonial overlap. validate_all must
        // return one report per registered domain, in registry order, and each
        // report's issues must equal the single-domain `validate` result.
        let (root, _vfs) = synthetic(
            "validate_all_agg",
            &[
                (
                    "map/definition.csv",
                    "province;red;green;blue;name;x\n1;1;1;1;Aa;x\n2;2;2;2;Sea;x\n",
                ),
                ("map/default.map", "sea_starts = { 2 }\nlakes = { }\n"),
                (
                    "map/climate.txt",
                    "tropical = { 1 }\narctic = { 1 }\nimpassable = { }\n",
                ),
                ("map/area.txt", ""),
                ("map/region.txt", ""),
                ("map/superregion.txt", ""),
                ("map/continent.txt", "europe = { 1 }\n"),
                (
                    "common/colonial_regions/00.txt",
                    "colonial_a = { color = { 1 2 3 } provinces = { 1 } names = { name = \"A\" } }\n\
                     colonial_b = { color = { 4 5 6 } provinces = { 1 } names = { name = \"B\" } }\n",
                ),
            ],
        );
        let path = root.to_str().unwrap().to_string();

        let reports = validate_all(path.clone(), None, None).unwrap();

        // One report per registered domain, in registry order.
        let names: Vec<&str> = reports.iter().map(|r| r.domain.as_str()).collect();
        assert_eq!(names, domains(), "domain tags must match the registry order");

        // Each aggregated domain's issues equal a standalone `validate` call.
        for r in &reports {
            let individual = validate(r.domain.clone(), path.clone(), None, None).unwrap();
            assert_eq!(
                r.issues.len(),
                individual.len(),
                "domain {} count mismatch (aggregate {} vs single {})",
                r.domain,
                r.issues.len(),
                individual.len()
            );
            // Same messages too (not just counts), proving no re-implementation.
            let agg_msgs: Vec<&str> = r.issues.iter().map(|i| i.message.as_str()).collect();
            let one_msgs: Vec<&str> = individual.iter().map(|i| i.message.as_str()).collect();
            assert_eq!(agg_msgs, one_msgs, "domain {} messages differ", r.domain);
        }

        // The exercised domains actually produced issues (fixture is meaningful).
        let climate = reports.iter().find(|r| r.domain == "climate").unwrap();
        assert!(
            climate.issues.iter().any(|i| i.severity == Severity::Error),
            "expected a climate dup-zone error: {:?}",
            climate.issues
        );
        let colonial = reports
            .iter()
            .find(|r| r.domain == "colonial_regions")
            .unwrap();
        assert!(!colonial.issues.is_empty(), "expected colonial overlap");
    }

    #[test]
    fn validate_all_real_install_matches_sum_of_domains() {
        if real_install().is_none() {
            return;
        }
        let install = INSTALL.to_string();
        let start = std::time::Instant::now();
        let reports = validate_all(install.clone(), None, None).unwrap();
        let elapsed = start.elapsed();

        // Every domain present exactly once.
        assert_eq!(reports.len(), domains().len());
        let total: usize = reports.iter().map(|r| r.issues.len()).sum();

        // Sum of independent single-domain runs must equal the aggregate.
        let mut sum = 0usize;
        for d in domains() {
            let one = validate(d.to_string(), install.clone(), None, None).unwrap();
            let agg = reports.iter().find(|r| r.domain == d).unwrap();
            assert_eq!(one.len(), agg.issues.len(), "domain {d} count mismatch");
            sum += one.len();
        }
        assert_eq!(total, sum, "aggregate total must equal sum of domain runs");
        println!(
            "[validate_all] vanilla: {} issues across {} domains in {elapsed:?}",
            total,
            reports.len()
        );
    }

    #[test]
    fn anbennar_domains_run_without_panic() {
        if !Path::new(INSTALL).join("map/provinces.bmp").is_file() || !Path::new(ANBENNAR).is_dir()
        {
            return;
        }
        let vfs = Vfs::new(INSTALL, Some(ANBENNAR)).unwrap();
        let loc = crate::loc::store(&vfs, INSTALL, Some(ANBENNAR));
        let areas = check_areas(&vfs, &loc);
        let continents = check_continents(&vfs, &loc);
        let climate = check_climate(&vfs, &loc);
        println!(
            "[validation:anbennar] areas={} continents={} climate={}",
            areas.len(),
            continents.len(),
            climate.len()
        );
    }
}
