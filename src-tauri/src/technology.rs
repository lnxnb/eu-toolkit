//! Sprint 22 — Technology & units (View ▸ Technology).
//!
//! Three subsystems share this module, all keyed off `common/technologies/*.txt`
//! and `common/units/*.txt`:
//!
//! * **Tech tables** — `common/technologies/{adm,dip,mil}.txt`. Each file is a
//!   `monarch_power = ADM|DIP|MIL` header, an optional `ahead_of_time = { … }`
//!   block, then a run of repeated `technology = { … }` blocks — one per tech
//!   LEVEL, in file order (level index = block position, 0-based). A level block
//!   carries `year = N`, an optional `expects_institution = { … }` block, and a
//!   mix of direct scalars:
//!     - `enable = <unit>`         → a unit unlock (repeated; land in mil.txt,
//!                                    ships in dip.txt; adm.txt has none)
//!     - `<key> = yes` / `= no`    → a building / mechanic / government unlock
//!     - `<key> = <number>`        → a modifier gained that level (e.g.
//!                                    `production_efficiency = 0.02`) or a numeric
//!                                    mechanic (`combat_width = 5`)
//!   Repeated `technology` blocks are addressed by occurrence (`technology#<n>`),
//!   exactly like duplicate dated blocks — so a year/modifier edit is a plain
//!   `SetScalar` on `["technology#<level>", key]`, and a new level is an `Append`
//!   of `technology = { year = … }` (the file's own comment: new tech goes AFTER
//!   the last technology). Anything unmodeled inside a level (blocks like
//!   `expects_institution`) round-trips untouched and shows read-only.
//!
//! * **Tech groups** — `common/technology.txt` `groups = { western = { start_level
//!   start_cost_modifier … } … }`. The registry already reads these for pickers;
//!   here they get a typed table editor (the two numeric columns, byte-surgical
//!   `SetScalar` on `["groups", group, key]`).
//!
//! * **Units** — `common/units/<key>.txt`, one unit per file. Land units
//!   (`type = infantry|cavalry|artillery`) carry `unit_type = <graphical group>`
//!   and the seven pips (`maneuver`, `{offensive,defensive}_{morale,fire,shock}`);
//!   ships (`type = galley|heavy_ship|light_ship|transport`) carry `hull_size`,
//!   `base_cannons`, `blockade`, `sail_speed`, `sailors`, sometimes `trade_power`.
//!   A unit "arrives" at the tech level whose block lists `enable = <key>` — the
//!   cross-ref is built from the tech tables. (There is no per-unit `cost` key in
//!   EU4 — unit cost is global/tech-driven — so "edit cost" reduces to editing
//!   pips/ship stats.)

use std::collections::HashMap;

use crate::loc::{self, LocStore};
use crate::paradox::{self, Block, Value};
use crate::validation::{JumpTarget, Severity, ValidationIssue};
use crate::vfs::Vfs;

pub const TECH_DIR: &str = "common/technologies";
pub const TECH_GROUPS_FILE: &str = "common/technology.txt";
pub const UNITS_DIR: &str = "common/units";
pub const UNITS_PROJECT_DIR: &str = "common/units";

/// The seven land-unit pips, in canonical display order.
pub const LAND_PIPS: &[&str] = &[
    "maneuver",
    "offensive_morale",
    "defensive_morale",
    "offensive_fire",
    "defensive_fire",
    "offensive_shock",
    "defensive_shock",
];
/// The editable numeric ship stats, in canonical display order.
pub const SHIP_STATS: &[&str] = &[
    "hull_size",
    "base_cannons",
    "blockade",
    "sail_speed",
    "sailors",
    "trade_power",
];
const LAND_TYPES: &[&str] = &["infantry", "cavalry", "artillery"];

/// Pip-budget tolerance (Sprint 22 validation): a land unit is flagged only when
/// its total pips exceed the strongest OTHER same-level, same-category unit by
/// MORE than this. Derived from vanilla — the largest legitimate peer gap in
/// vanilla is exactly 6 (a level-9 elite infantry at 13 pips vs the next at 7),
/// so a tolerance of 6 keeps vanilla clean while still catching a mod unit that
/// is wildly ahead of its era.
const PIP_BUDGET_TOLERANCE: i64 = 6;

// ---------------------------------------------------------------------------
// Payload types (serialize camelCase; mirrored by src/lib/technology.ts).
// ---------------------------------------------------------------------------

/// One `key = value` row inside a tech level, classified for display/editing.
#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TechRow {
    pub key: String,
    pub value: String,
    /// `modifier` (numeric, editable) | `unlock` (`= yes`, building/mechanic) |
    /// `unit` (`enable = <unit>`).
    pub kind: String,
    /// Localized display label (loc-resolved for unlock/unit; the key for a
    /// modifier — modifier keys have no plain loc).
    pub label: String,
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TechLevel {
    /// 0-based level index within its monarch-power file.
    pub index: usize,
    /// The file this level lives in (for byte-surgical edits).
    pub file: String,
    /// In-game display name, from `<kind>_tech_cs_<index>_name`. `None` when the
    /// level has no loc entry (a mod-appended level past vanilla's 0–32).
    pub name: Option<String>,
    /// Flavor text, from `<kind>_tech_cs_<index>_desc`.
    pub desc: Option<String>,
    pub year: Option<String>,
    /// Numeric modifiers gained this level (editable).
    pub modifiers: Vec<TechRow>,
    /// Building / mechanic / government unlocks (`= yes`).
    pub unlocks: Vec<TechRow>,
    /// Unit unlocks (`enable = <unit>`).
    pub units: Vec<TechRow>,
    /// Unmodeled direct keys/blocks (e.g. `expects_institution`), preserved
    /// untouched and shown read-only.
    pub raw_extra: Vec<String>,
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TechTable {
    /// `adm` | `dip` | `mil`.
    pub kind: String,
    /// Localized power name (else the raw `ADM`/`DIP`/`MIL`).
    pub label: String,
    pub file: String,
    pub monarch_power: String,
    pub levels: Vec<TechLevel>,
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TechGroup {
    pub key: String,
    pub name: String,
    pub file: String,
    pub start_level: Option<String>,
    pub start_cost_modifier: Option<String>,
    pub raw_extra: Vec<String>,
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PipStat {
    pub key: String,
    pub value: String,
    pub present: bool,
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Unit {
    pub key: String,
    pub file: String,
    pub origin: String,
    pub name: String,
    /// `type` value: infantry / cavalry / artillery / galley / heavy_ship /
    /// light_ship / transport.
    pub category: String,
    /// Graphical group (`unit_type`); land units only.
    pub unit_type: Option<String>,
    pub is_land: bool,
    /// The seven pips (land) or the ship stats (ships), in canonical order.
    pub pips: Vec<PipStat>,
    /// Sum of the seven land pips; `None` for ships.
    pub total_pips: Option<i64>,
    /// The monarch power (`mil`/`dip`) whose tech level enables this unit.
    pub arrives_tech: Option<String>,
    /// 0-based tech level that enables this unit (via `enable = <key>`).
    pub arrives_level: Option<usize>,
    /// Unmodeled direct keys (e.g. `sprite_level`), preserved + read-only.
    pub raw_extra: Vec<String>,
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TechData {
    pub tables: Vec<TechTable>,
    pub groups: Vec<TechGroup>,
}

// ---------------------------------------------------------------------------
// Loading helpers.
// ---------------------------------------------------------------------------

fn origin_of(vfs: &Vfs, path: &std::path::Path) -> &'static str {
    if vfs.mod_dir().is_some_and(|m| path.starts_with(m)) {
        "mod"
    } else {
        "base"
    }
}

fn is_numeric(s: &str) -> bool {
    let t = s.trim();
    !t.is_empty() && t.parse::<f64>().is_ok()
}

fn power_kind(power: &str) -> &'static str {
    match power.trim().to_ascii_uppercase().as_str() {
        "ADM" => "adm",
        "DIP" => "dip",
        _ => "mil",
    }
}

/// A tech level's display name / flavor loc keys. Verified format across all six
/// (name, desc) × (adm, dip, mil) sets in `localisation/technology_l_english.yml`,
/// levels 0–32: `adm_tech_cs_5_name`, `mil_tech_cs_0_desc`, … The level index is
/// the ONLY axis — names do not vary by tech group. A level block itself carries
/// no name in script (the `#Land Rights` comments are not data, and vanilla's
/// comment sometimes disagrees with the loc string).
pub fn level_name_key(kind: &str, index: usize) -> String {
    format!("{kind}_tech_cs_{index}_name")
}

pub fn level_desc_key(kind: &str, index: usize) -> String {
    format!("{kind}_tech_cs_{index}_desc")
}

/// Display label for a modifier key. Most core modifiers are hardcoded in the
/// executable and have no loc entry at all; the newer ones use
/// `MODIFIER_<UPPERCASE_KEY>`. Try that, then the bare key, then prettify.
fn modifier_label(key: &str, loc: &LocStore) -> String {
    loc.get(&format!("MODIFIER_{}", key.to_ascii_uppercase()))
        .or_else(|| loc.get(key))
        .map(str::to_string)
        .unwrap_or_else(|| loc::prettify(key))
}

/// Parses one tech level block into typed rows.
fn parse_level(kind: &str, index: usize, file: &str, b: &Block, loc: &LocStore) -> TechLevel {
    let mut year = None;
    let mut modifiers = Vec::new();
    let mut unlocks = Vec::new();
    let mut units = Vec::new();
    let mut raw_extra = Vec::new();
    let mut seen_extra = std::collections::HashSet::new();

    for (k, v) in &b.items {
        let Some(k) = k.as_deref() else { continue };
        match v {
            Value::Scalar(s) => {
                let val = s.trim().to_string();
                if k == "year" {
                    year = Some(val);
                } else if k == "enable" {
                    let label = loc.get(&val).map(str::to_string).unwrap_or_else(|| loc::prettify(&val));
                    units.push(TechRow { key: k.to_string(), value: val, kind: "unit".into(), label });
                } else if val == "yes" || val == "no" {
                    let label = loc.get(k).map(str::to_string).unwrap_or_else(|| loc::prettify(k));
                    unlocks.push(TechRow { key: k.to_string(), value: val, kind: "unlock".into(), label });
                } else if is_numeric(&val) {
                    let label = modifier_label(k, loc);
                    modifiers.push(TechRow { key: k.to_string(), value: val, kind: "modifier".into(), label });
                } else {
                    // Non-numeric, non-bool scalar (rare): show as an unlock-style row.
                    let label = loc.get(k).map(str::to_string).unwrap_or_else(|| loc::prettify(k));
                    unlocks.push(TechRow { key: k.to_string(), value: val, kind: "unlock".into(), label });
                }
            }
            Value::Block(_) => {
                if seen_extra.insert(k.to_string()) {
                    raw_extra.push(k.to_string());
                }
            }
        }
    }

    TechLevel {
        index,
        file: file.to_string(),
        name: loc.get(&level_name_key(kind, index)).map(str::to_string),
        desc: loc.get(&level_desc_key(kind, index)).map(str::to_string),
        year,
        modifiers,
        unlocks,
        units,
        raw_extra,
    }
}

/// Loads the tech tables (adm/dip/mil), grouping the technologies directory by
/// monarch power. Levels within a power are numbered 0-based in file order.
pub fn load_tables(vfs: &Vfs, loc: &LocStore) -> Vec<TechTable> {
    // power_kind -> (monarch_power raw, file, levels)
    let mut by_power: HashMap<&'static str, TechTable> = HashMap::new();
    let order = ["adm", "dip", "mil"];

    // Deterministic file order.
    let mut files: Vec<(String, std::path::PathBuf)> = vfs
        .list_dir(TECH_DIR)
        .into_iter()
        .filter(|(n, _)| n.to_lowercase().ends_with(".txt"))
        .collect();
    files.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));

    for (name, path) in files {
        let Ok(bytes) = std::fs::read(&path) else { continue };
        let block = paradox::parse(&String::from_utf8_lossy(&bytes));
        let power = block.get_scalar("monarch_power").unwrap_or("MIL").to_string();
        let kind = power_kind(&power);
        let rel = format!("{TECH_DIR}/{name}");
        let table = by_power.entry(kind).or_insert_with(|| TechTable {
            kind: kind.to_string(),
            label: loc.get(&power).map(str::to_string).unwrap_or_else(|| power.clone()),
            file: rel.clone(),
            monarch_power: power.clone(),
            levels: Vec::new(),
        });
        // Levels are appended in file order; a mod file with the same power is
        // rare but concatenates after (EU4 keeps all tech in the one file). The
        // occurrence index is the block's position among `technology` blocks in
        // THIS file — exactly what a `technology#<n>` edit path resolves to.
        let mut occ = 0usize;
        for (key, lb) in block.key_blocks() {
            if key != "technology" {
                continue;
            }
            table.levels.push(parse_level(kind, occ, &rel, lb, loc));
            occ += 1;
        }
    }

    order
        .iter()
        .filter_map(|k| by_power.remove(*k))
        .collect()
}

/// unit key -> (power kind "mil"/"dip", level index) from every `enable = <unit>`
/// across the tech tables.
pub fn unit_arrival(tables: &[TechTable]) -> HashMap<String, (String, usize)> {
    let mut out = HashMap::new();
    for t in tables {
        for lvl in &t.levels {
            for u in &lvl.units {
                out.entry(u.value.clone())
                    .or_insert_with(|| (t.kind.clone(), lvl.index));
            }
        }
    }
    out
}

/// Loads the tech groups from `common/technology.txt`.
pub fn load_groups(vfs: &Vfs, loc: &LocStore) -> Vec<TechGroup> {
    let Ok(bytes) = vfs.read(TECH_GROUPS_FILE) else {
        return Vec::new();
    };
    let block = paradox::parse(&String::from_utf8_lossy(&bytes));
    let Some(groups) = block.get_block("groups") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (key, b) in groups.key_blocks() {
        let mut raw_extra = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for (k, _) in &b.items {
            if let Some(k) = k.as_deref() {
                if k != "start_level" && k != "start_cost_modifier" && seen.insert(k.to_string()) {
                    raw_extra.push(k.to_string());
                }
            }
        }
        out.push(TechGroup {
            key: key.to_string(),
            name: loc.get(key).map(str::to_string).unwrap_or_else(|| loc::prettify(key)),
            file: TECH_GROUPS_FILE.to_string(),
            start_level: b.get_scalar("start_level").map(|s| s.trim().to_string()),
            start_cost_modifier: b.get_scalar("start_cost_modifier").map(|s| s.trim().to_string()),
            raw_extra,
        });
    }
    out
}

/// Parses one unit file body into a `Unit`, cross-referencing its arrival level.
fn parse_unit(
    key: &str,
    block: &Block,
    loc: &LocStore,
    file: &str,
    origin: &str,
    arrival: &HashMap<String, (String, usize)>,
) -> Unit {
    let category = block.get_scalar("type").unwrap_or("").trim().to_string();
    let is_land = LAND_TYPES.contains(&category.as_str());
    let unit_type = block.get_scalar("unit_type").map(|s| s.trim().to_string());

    let stat_keys: &[&str] = if is_land { LAND_PIPS } else { SHIP_STATS };
    let mut pips = Vec::new();
    for &sk in stat_keys {
        match block.get_scalar(sk) {
            Some(v) => pips.push(PipStat { key: sk.to_string(), value: v.trim().to_string(), present: true }),
            None => {
                if is_land {
                    // Land units always carry all seven pips; surface absent as 0.
                    pips.push(PipStat { key: sk.to_string(), value: "0".into(), present: false });
                }
            }
        }
    }

    let total_pips = if is_land {
        Some(
            LAND_PIPS
                .iter()
                .filter_map(|k| block.get_scalar(k))
                .filter_map(|v| v.trim().parse::<i64>().ok())
                .sum(),
        )
    } else {
        None
    };

    let mut modeled: std::collections::HashSet<&str> = std::collections::HashSet::new();
    modeled.insert("type");
    modeled.insert("unit_type");
    for &k in stat_keys {
        modeled.insert(k);
    }
    let mut raw_extra = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (k, _) in &block.items {
        if let Some(k) = k.as_deref() {
            if !modeled.contains(k) && seen.insert(k.to_string()) {
                raw_extra.push(k.to_string());
            }
        }
    }

    let (arrives_tech, arrives_level) = match arrival.get(key) {
        Some((t, l)) => (Some(t.clone()), Some(*l)),
        None => (None, None),
    };

    Unit {
        key: key.to_string(),
        file: file.to_string(),
        origin: origin.to_string(),
        name: loc.get(key).map(str::to_string).unwrap_or_else(|| loc::prettify(key)),
        category,
        unit_type,
        is_land,
        pips,
        total_pips,
        arrives_tech,
        arrives_level,
        raw_extra,
    }
}

/// Loads every unit, cross-referencing arrival levels from `tables`.
pub fn load_units(vfs: &Vfs, loc: &LocStore, tables: &[TechTable]) -> Vec<Unit> {
    let arrival = unit_arrival(tables);
    let mut units = Vec::new();
    for (name, path) in vfs.list_dir(UNITS_DIR) {
        if !name.to_lowercase().ends_with(".txt") {
            continue;
        }
        let key = name.strip_suffix(".txt").or_else(|| name.strip_suffix(".TXT")).unwrap_or(&name);
        let Ok(bytes) = std::fs::read(&path) else { continue };
        let origin = origin_of(vfs, &path);
        let block = paradox::parse(&String::from_utf8_lossy(&bytes));
        let rel = format!("{UNITS_DIR}/{name}");
        units.push(parse_unit(key, &block, loc, &rel, origin, &arrival));
    }
    units.sort_by(|a, b| a.key.cmp(&b.key));
    units
}

// ---------------------------------------------------------------------------
// Unit scaffold (create).
// ---------------------------------------------------------------------------

#[derive(serde::Serialize, Clone, Debug)]
pub struct LocEntry {
    pub key: String,
    pub value: String,
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct UnitScaffold {
    pub key: String,
    pub file: String,
    pub text: String,
    pub loc_entries: Vec<LocEntry>,
}

/// A minimal, game-valid unit file for `key` of `category`, plus a loc entry.
/// The frontend pairs this with an `enable = <key>` registration into the chosen
/// tech level (one composite) so the unit is reachable in-game with zero fixes.
pub fn scaffold_unit(key: &str, category: &str, unit_type: &str) -> UnitScaffold {
    let cat = category.trim();
    let is_land = LAND_TYPES.contains(&cat);
    let pretty = loc::prettify(key);
    let text = if is_land {
        let ut = if unit_type.trim().is_empty() { "western" } else { unit_type.trim() };
        format!(
            "# {pretty}\n\n\
type = {cat}\n\
unit_type = {ut}\n\n\
maneuver = 0\n\
offensive_morale = 1\n\
defensive_morale = 1\n\
offensive_fire = 0\n\
defensive_fire = 0\n\
offensive_shock = 0\n\
defensive_shock = 0\n"
        )
    } else {
        let cat = if cat.is_empty() { "heavy_ship" } else { cat };
        format!(
            "# {pretty}\n\n\
type = {cat}\n\n\
hull_size = 20\n\
base_cannons = 20\n\
blockade = 5\n\
sail_speed = 5\n\
sailors = 100\n\
sprite_level = 1\n"
        )
    };
    UnitScaffold {
        key: key.to_string(),
        file: format!("{UNITS_PROJECT_DIR}/{key}.txt"),
        text,
        loc_entries: vec![LocEntry { key: key.to_string(), value: pretty }],
    }
}

// ---------------------------------------------------------------------------
// Validation (Sprint 22): pip-budget outliers + units with no tech level.
// ---------------------------------------------------------------------------

/// Validation domain `units`:
///   * a unit referenced by no tech level (`enable = …` never lists it) → WARN
///   * a land unit whose total pips exceed the strongest OTHER same-level,
///     same-category unit by more than [`PIP_BUDGET_TOLERANCE`] → WARN (an
///     outlier vs its era; ships have no comparable pip budget and are skipped).
pub fn validate_units(vfs: &Vfs, loc: &LocStore) -> Vec<ValidationIssue> {
    let tables = load_tables(vfs, loc);
    let units = load_units(vfs, loc, &tables);
    let mut issues = Vec::new();

    // 1. Units enabled by no tech level.
    for u in &units {
        if u.arrives_level.is_none() {
            issues.push(ValidationIssue::new(
                Severity::Warning,
                format!(
                    "Unit \"{}\" ({}) is not enabled by any tech level",
                    u.name, u.key
                ),
                Some(JumpTarget::File(u.file.clone())),
            ));
        }
    }

    // 2. Pip-budget outliers among land units, bucketed by (level, category).
    let mut buckets: HashMap<(usize, String), Vec<(&Unit, i64)>> = HashMap::new();
    for u in &units {
        if !u.is_land {
            continue;
        }
        let (Some(level), Some(total)) = (u.arrives_level, u.total_pips) else {
            continue;
        };
        buckets.entry((level, u.category.clone())).or_default().push((u, total));
    }
    let mut flagged: Vec<(&Unit, i64, i64)> = Vec::new();
    for ((_level, _cat), members) in &buckets {
        if members.len() < 2 {
            continue; // no peers to judge against
        }
        for (u, total) in members {
            // Strongest OTHER unit in the same bucket.
            let peer_max = members
                .iter()
                .filter(|(o, _)| o.key != u.key)
                .map(|(_, t)| *t)
                .max()
                .unwrap_or(*total);
            if *total > peer_max + PIP_BUDGET_TOLERANCE {
                flagged.push((u, *total, peer_max));
            }
        }
    }
    flagged.sort_by(|a, b| a.0.key.cmp(&b.0.key));
    for (u, total, peer_max) in flagged {
        issues.push(ValidationIssue::new(
            Severity::Warning,
            format!(
                "Unit \"{}\" has {} total pips — far above the same-era {} ceiling of {} (tolerance {})",
                u.name, total, u.category, peer_max, PIP_BUDGET_TOLERANCE
            ),
            Some(JumpTarget::File(u.file.clone())),
        ));
    }

    issues
}

// ---------------------------------------------------------------------------
// Commands.
// ---------------------------------------------------------------------------

#[tauri::command(async)]
pub fn get_technologies(install_path: String, mod_path: Option<String>) -> Result<TechData, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    let loc = loc::store(&vfs, &install_path, mod_path.as_deref());
    let tables = load_tables(&vfs, &loc);
    let groups = load_groups(&vfs, &loc);
    Ok(TechData { tables, groups })
}

#[tauri::command(async)]
pub fn get_units(install_path: String, mod_path: Option<String>) -> Result<Vec<Unit>, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    let loc = loc::store(&vfs, &install_path, mod_path.as_deref());
    let tables = load_tables(&vfs, &loc);
    Ok(load_units(&vfs, &loc, &tables))
}

#[tauri::command(async)]
pub fn scaffold_unit_file(key: String, category: String, unit_type: String) -> Result<UnitScaffold, String> {
    Ok(scaffold_unit(&key, &category, &unit_type))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mod_writer::{apply, Edit};
    use std::path::Path;

    const INSTALL: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Europa Universalis IV";
    const ANBENNAR: &str = r"C:\Users\User\projects\anbennar-eu4-dev";

    fn install_present() -> bool {
        Path::new(INSTALL).join("map/provinces.bmp").is_file()
    }

    fn synthetic(name: &str, files: &[(&str, &str)]) -> (std::path::PathBuf, Vfs) {
        let root = std::env::temp_dir().join(format!("eu_toolkit_technology_test_{name}"));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(root.join("map")).unwrap();
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

    const MIL_SRC: &str = "monarch_power = MIL\n\
ahead_of_time = {\n\tmonthly_russian_modernization = 0.05\n}\n\
technology = {\n\t# Tech 0\n\tyear = 1350\n\tsprite_level = 1\n\tinfantry_fire = 0.25\n\tland_morale = 2.0\n}\n\
technology = {\n\t# Tech 1\n\tyear = 1390\n\texpects_institution = {\n\t\tfeudalism = 0.25\n\t}\n\tfort_15th = yes\n\tenable = western_medieval_infantry\n\tenable = western_medieval_knights\n}\n\
technology = {\n\t# Tech 2\n\tyear = 1420\n\tcombat_width = 5\n\tenable = elite_infantry\n}\n";

    const GROUPS_SRC: &str = "groups = {\n\
\twestern = {\n\t\tstart_level = 3\n\t\tstart_cost_modifier = 0\n\t\tnation_designer_cost = { value = 75 }\n\t}\n\
\teastern = {\n\t\tstart_level = 3\n\t\tstart_cost_modifier = 0.20\n\t}\n}\n";

    fn tech_fixture(name: &str) -> (std::path::PathBuf, Vfs) {
        synthetic(
            name,
            &[
                ("common/technologies/mil.txt", MIL_SRC),
                ("common/technology.txt", GROUPS_SRC),
                (
                    "common/units/western_medieval_infantry.txt",
                    "type = infantry\nunit_type = western\nmaneuver = 1\noffensive_morale = 1\ndefensive_morale = 1\noffensive_fire = 0\ndefensive_fire = 0\noffensive_shock = 0\ndefensive_shock = 0\n",
                ),
                (
                    "common/units/western_medieval_knights.txt",
                    "type = cavalry\nunit_type = western\nmaneuver = 2\noffensive_morale = 1\ndefensive_morale = 1\noffensive_fire = 0\ndefensive_fire = 0\noffensive_shock = 1\ndefensive_shock = 0\n",
                ),
                (
                    "common/units/elite_infantry.txt",
                    "type = infantry\nunit_type = western\nmaneuver = 3\noffensive_morale = 3\ndefensive_morale = 3\noffensive_fire = 3\ndefensive_fire = 3\noffensive_shock = 3\ndefensive_shock = 3\n",
                ),
                (
                    "common/units/lonely_infantry.txt",
                    "type = infantry\nunit_type = western\nmaneuver = 1\noffensive_morale = 1\ndefensive_morale = 1\noffensive_fire = 0\ndefensive_fire = 0\noffensive_shock = 0\ndefensive_shock = 0\n",
                ),
            ],
        )
    }

    #[test]
    fn parses_tech_levels_rows_and_units() {
        let (_root, vfs) = tech_fixture("parse");
        let loc = LocStore::from_pairs(&[("MIL", "Military"), ("western_medieval_infantry", "Men-at-Arms")]);
        let tables = load_tables(&vfs, &loc);
        assert_eq!(tables.len(), 1);
        let mil = &tables[0];
        assert_eq!(mil.kind, "mil");
        assert_eq!(mil.label, "Military");
        assert_eq!(mil.levels.len(), 3);
        // Level 0: year + modifiers, no units.
        let l0 = &mil.levels[0];
        assert_eq!(l0.index, 0);
        assert_eq!(l0.year.as_deref(), Some("1350"));
        assert!(l0.modifiers.iter().any(|m| m.key == "infantry_fire" && m.value == "0.25"));
        assert!(l0.modifiers.iter().any(|m| m.key == "sprite_level"));
        assert!(l0.units.is_empty());
        // Level 1: an unlock (fort_15th = yes) + two unit enables + preserved block.
        let l1 = &mil.levels[1];
        assert!(l1.unlocks.iter().any(|u| u.key == "fort_15th" && u.value == "yes"));
        assert_eq!(l1.units.len(), 2);
        let mai = l1.units.iter().find(|u| u.value == "western_medieval_infantry").unwrap();
        assert_eq!(mai.label, "Men-at-Arms");
        assert!(l1.raw_extra.contains(&"expects_institution".to_string()));
    }

    #[test]
    fn tech_group_parse_and_edit_round_trip() {
        let (_root, vfs) = tech_fixture("groups");
        let loc = LocStore::from_pairs(&[("western", "Western")]);
        let groups = load_groups(&vfs, &loc);
        assert_eq!(groups.len(), 2);
        let w = groups.iter().find(|g| g.key == "western").unwrap();
        assert_eq!(w.name, "Western");
        assert_eq!(w.start_level.as_deref(), Some("3"));
        assert_eq!(w.start_cost_modifier.as_deref(), Some("0"));
        assert!(w.raw_extra.contains(&"nation_designer_cost".to_string()));
        // Edit start_level byte-surgically; other group + raw block preserved.
        let out = apply(
            GROUPS_SRC.as_bytes(),
            &Edit::SetScalar {
                path: vec!["groups".into(), "western".into(), "start_level".into()],
                value: "5".into(),
                quoted: false,
            },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("start_level = 5"));
        assert!(text.contains("nation_designer_cost = { value = 75 }"));
        assert!(text.contains("start_cost_modifier = 0.20"));
    }

    #[test]
    fn level_year_and_modifier_edit_round_trip() {
        // Year of level 1 + a modifier of level 0, both via occurrence path.
        let out = apply(
            MIL_SRC.as_bytes(),
            &Edit::SetScalar {
                path: vec!["technology#1".into(), "year".into()],
                value: "1395".into(),
                quoted: false,
            },
        )
        .unwrap();
        let out = apply(
            &out,
            &Edit::SetScalar {
                path: vec!["technology#0".into(), "infantry_fire".into()],
                value: "0.5".into(),
                quoted: false,
            },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("year = 1395"));
        assert!(text.contains("infantry_fire = 0.5"));
        // Level 0's year and level 1's enables untouched.
        assert!(text.contains("year = 1350"));
        assert!(text.contains("enable = western_medieval_infantry"));
    }

    /// Adding and removing a modifier is a per-statement splice: the level's
    /// preserve-unknown sub-blocks and its sibling levels survive untouched, and
    /// add-then-edit composes (the later SetScalar finds the inserted key).
    #[test]
    fn modifier_add_remove_round_trip() {
        let added = apply(
            MIL_SRC.as_bytes(),
            &Edit::InsertStatement {
                block_path: vec!["technology#1".into()],
                statement: "discipline = 0.05".into(),
            },
        )
        .unwrap();
        // A later value edit lands on the freshly inserted key.
        let edited = apply(
            &added,
            &Edit::SetScalar {
                path: vec!["technology#1".into(), "discipline".into()],
                value: "0.25".into(),
                quoted: false,
            },
        )
        .unwrap();
        let text = String::from_utf8(edited.clone()).unwrap();
        assert!(text.contains("discipline = 0.25"));
        // Level 1's unmodeled sub-block and its enables are intact.
        assert!(text.contains("expects_institution = {"));
        assert!(text.contains("feudalism = 0.25"));
        assert!(text.contains("enable = western_medieval_knights"));
        // Sibling levels untouched.
        assert!(text.contains("infantry_fire = 0.25") && text.contains("combat_width = 5"));

        // Removing it restores the original bytes exactly.
        let removed = apply(
            &added,
            &Edit::RemoveStatement {
                block_path: vec!["technology#1".into()],
                key: "discipline".into(),
                value: None,
            },
        )
        .unwrap();
        assert_eq!(String::from_utf8(removed).unwrap(), MIL_SRC);

        // Removing a disk modifier takes only that row.
        let gone = apply(
            MIL_SRC.as_bytes(),
            &Edit::RemoveStatement {
                block_path: vec!["technology#0".into()],
                key: "land_morale".into(),
                value: None,
            },
        )
        .unwrap();
        let text = String::from_utf8(gone).unwrap();
        assert!(!text.contains("land_morale"));
        assert!(text.contains("infantry_fire = 0.25"));
        assert!(text.contains("# Tech 0"));
    }

    /// Deleting a level is an occurrence-addressed root RemoveStatement. The
    /// levels around it survive byte-for-byte — and the ones after it RENUMBER,
    /// which is why the UI freezes other level edits until the delete is saved.
    #[test]
    fn delete_level_round_trip_and_renumbers() {
        let out = apply(
            MIL_SRC.as_bytes(),
            &Edit::RemoveStatement {
                block_path: vec![],
                key: "technology#1".into(),
                value: None,
            },
        )
        .unwrap();
        let text = String::from_utf8(out.clone()).unwrap();
        // The middle level and everything in it is gone.
        assert!(!text.contains("# Tech 1"));
        assert!(!text.contains("enable = western_medieval_infantry"));
        assert!(!text.contains("expects_institution"));
        // Its neighbours and the file header are untouched.
        assert!(text.contains("monarch_power = MIL"));
        assert!(text.contains("ahead_of_time = {"));
        assert!(text.contains("# Tech 0") && text.contains("land_morale = 2.0"));
        assert!(text.contains("# Tech 2") && text.contains("enable = elite_infantry"));

        // Two levels remain, and the former level 2 is now addressed as #1.
        let loc = LocStore::from_pairs(&[]);
        let b = paradox::parse(&text);
        let levels: Vec<_> = b
            .key_blocks()
            .filter(|(k, _)| *k == "technology")
            .enumerate()
            .map(|(i, (_, lb))| parse_level("mil", i, "f", lb, &loc))
            .collect();
        assert_eq!(levels.len(), 2);
        assert_eq!(levels[1].year.as_deref(), Some("1420"));
        let renamed = apply(
            &out,
            &Edit::SetScalar {
                path: vec!["technology#1".into(), "year".into()],
                value: "1425".into(),
                quoted: false,
            },
        )
        .unwrap();
        let text = String::from_utf8(renamed).unwrap();
        assert!(text.contains("year = 1425") && text.contains("enable = elite_infantry"));
    }

    #[test]
    fn append_new_level_round_trip() {
        let out = apply(
            MIL_SRC.as_bytes(),
            &Edit::Append {
                text: "technology = {\n\tyear = 1450\n\tland_morale = 0.5\n}".into(),
            },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        // The new level lands after the last technology block and parses back as
        // a 4th level.
        let block = paradox::parse(&text);
        let n = block.key_blocks().filter(|(k, _)| *k == "technology").count();
        assert_eq!(n, 4);
        assert!(text.contains("year = 1450"));
        // Original last level still present.
        assert!(text.contains("combat_width = 5"));
    }

    #[test]
    fn unit_pip_edit_round_trip() {
        let src = "type = infantry\nunit_type = western\nmaneuver = 1\noffensive_morale = 1\ndefensive_morale = 1\noffensive_fire = 0\ndefensive_fire = 0\noffensive_shock = 0\ndefensive_shock = 0\n";
        let out = apply(
            src.as_bytes(),
            &Edit::SetScalar {
                path: vec!["offensive_shock".into()],
                value: "2".into(),
                quoted: false,
            },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("offensive_shock = 2"));
        assert!(text.contains("type = infantry"));
        assert!(text.contains("unit_type = western"));
    }

    #[test]
    fn units_cross_reference_arrival_level() {
        let (_root, vfs) = tech_fixture("xref");
        let loc = LocStore::from_pairs(&[]);
        let tables = load_tables(&vfs, &loc);
        let units = load_units(&vfs, &loc, &tables);
        let mai = units.iter().find(|u| u.key == "western_medieval_infantry").unwrap();
        assert_eq!(mai.category, "infantry");
        assert!(mai.is_land);
        assert_eq!(mai.unit_type.as_deref(), Some("western"));
        assert_eq!(mai.arrives_tech.as_deref(), Some("mil"));
        assert_eq!(mai.arrives_level, Some(1));
        assert_eq!(mai.total_pips, Some(3)); // maneuver 1 + off_morale 1 + def_morale 1
        // lonely_infantry is enabled by no tech level → no arrival.
        let lonely = units.iter().find(|u| u.key == "lonely_infantry").unwrap();
        assert_eq!(lonely.arrives_level, None);
    }

    #[test]
    fn create_unit_composite_lands_both_edits() {
        // The create-unit composite: scaffold the unit file + register an
        // `enable` into the chosen tech level. Both must land and parse back.
        let scaffold = scaffold_unit("my_musketeers", "infantry", "western");
        assert_eq!(scaffold.file, "common/units/my_musketeers.txt");
        // Unit file parses with type + pips.
        let ub = paradox::parse(&scaffold.text);
        assert_eq!(ub.get_scalar("type"), Some("infantry"));
        assert_eq!(ub.get_scalar("unit_type"), Some("western"));
        assert!(ub.get_scalar("offensive_morale").is_some());
        assert_eq!(scaffold.loc_entries[0].key, "my_musketeers");
        // Registration into tech level 2 (occurrence path).
        let registered = apply(
            MIL_SRC.as_bytes(),
            &Edit::InsertStatement {
                block_path: vec!["technology#2".into()],
                statement: "enable = my_musketeers".into(),
            },
        )
        .unwrap();
        let text = String::from_utf8(registered).unwrap();
        // The enable is inside level 2 (alongside combat_width), not level 1.
        let block = paradox::parse(&text);
        let techs: Vec<_> = block.key_blocks().filter(|(k, _)| *k == "technology").map(|(_, b)| b).collect();
        assert_eq!(techs.len(), 3);
        let l2_enables: Vec<&str> = techs[2]
            .items
            .iter()
            .filter_map(|(k, v)| match (k.as_deref(), v) {
                (Some("enable"), Value::Scalar(s)) => Some(s.as_str()),
                _ => None,
            })
            .collect();
        assert!(l2_enables.contains(&"my_musketeers"));
    }

    #[test]
    fn validation_flags_orphan_and_pip_outlier() {
        // elite_infantry (21 pips) shares level 2 with... nothing else land-infantry,
        // so add a peer to make the bucket judge it. Rework: level 2 has
        // elite_infantry only → skipped. lonely_infantry has no level → orphan WARN.
        let (_root, vfs) = tech_fixture("validate_neg");
        let loc = LocStore::from_pairs(&[]);
        let issues = validate_units(&vfs, &loc);
        // Orphan: lonely_infantry.
        assert!(
            issues.iter().any(|i| i.message.contains("lonely_infantry") && i.message.contains("not enabled")),
            "expected orphan warning: {:?}",
            issues.iter().map(|i| &i.message).collect::<Vec<_>>()
        );
        // No pip outlier yet (elite_infantry is alone at level 2).
        assert!(!issues.iter().any(|i| i.message.contains("total pips")));
    }

    #[test]
    fn validation_pip_outlier_positive() {
        // Two infantry at the same level: a normal one (3 pips) and a wildly
        // overpowered one (21 pips) → the overpowered one is flagged.
        let (_root, vfs) = synthetic(
            "validate_pos",
            &[
                (
                    "common/technologies/mil.txt",
                    "monarch_power = MIL\ntechnology = {\n\tyear = 1350\n\tenable = normal_inf\n\tenable = op_inf\n}\n",
                ),
                (
                    "common/units/normal_inf.txt",
                    "type = infantry\nunit_type = western\nmaneuver = 1\noffensive_morale = 1\ndefensive_morale = 1\noffensive_fire = 0\ndefensive_fire = 0\noffensive_shock = 0\ndefensive_shock = 0\n",
                ),
                (
                    "common/units/op_inf.txt",
                    "type = infantry\nunit_type = western\nmaneuver = 3\noffensive_morale = 3\ndefensive_morale = 3\noffensive_fire = 3\ndefensive_fire = 3\noffensive_shock = 3\ndefensive_shock = 3\n",
                ),
            ],
        );
        let loc = LocStore::from_pairs(&[]);
        let issues = validate_units(&vfs, &loc);
        let outlier = issues.iter().find(|i| i.message.contains("total pips"));
        assert!(outlier.is_some(), "expected pip outlier: {:?}", issues.iter().map(|i| &i.message).collect::<Vec<_>>());
        assert!(outlier.unwrap().message.contains("op_inf") || outlier.unwrap().message.contains("Op Inf"));
        assert_eq!(outlier.unwrap().severity, Severity::Warning);
    }

    // --- Real install -------------------------------------------------------

    #[test]
    fn vanilla_tech_and_units_full_parse() {
        if !install_present() {
            return;
        }
        let vfs = Vfs::new(INSTALL, None).unwrap();
        let loc = loc::build(&vfs);
        let tables = load_tables(&vfs, &loc);
        // All three power tables present, each with many levels.
        let kinds: Vec<&str> = tables.iter().map(|t| t.kind.as_str()).collect();
        assert!(kinds.contains(&"adm") && kinds.contains(&"dip") && kinds.contains(&"mil"), "kinds: {kinds:?}");
        for t in &tables {
            assert!(t.levels.len() > 20, "{} has {} levels", t.kind, t.levels.len());
            // Level 0 has a year.
            assert!(t.levels[0].year.is_some());
        }
        let mil = tables.iter().find(|t| t.kind == "mil").unwrap();
        // The temple building unlock is an ADM level.
        let adm = tables.iter().find(|t| t.kind == "adm").unwrap();
        assert!(adm.levels.iter().any(|l| l.unlocks.iter().any(|u| u.key == "temple")));
        // mil enables western_medieval_infantry somewhere.
        assert!(mil.levels.iter().any(|l| l.units.iter().any(|u| u.value == "western_medieval_infantry")));

        let groups = load_groups(&vfs, &loc);
        assert!(groups.iter().any(|g| g.key == "western" && g.start_level.as_deref() == Some("3")));

        let units = load_units(&vfs, &loc, &tables);
        assert!(units.len() > 300, "units: {}", units.len());
        let wmi = units.iter().find(|u| u.key == "western_medieval_infantry").unwrap();
        assert_eq!(wmi.category, "infantry");
        assert!(wmi.arrives_level.is_some());
        assert_eq!(wmi.arrives_tech.as_deref(), Some("mil"));
        // A ship arrives via dip.
        let carrack = units.iter().find(|u| u.key == "early_carrack").unwrap();
        assert!(!carrack.is_land);
        assert_eq!(carrack.arrives_tech.as_deref(), Some("dip"));
        assert!(carrack.pips.iter().any(|p| p.key == "hull_size"));
    }

    /// Every vanilla level resolves a real name + flavor text, and the strings
    /// come from loc (not the script comments, which sometimes disagree).
    #[test]
    fn vanilla_levels_resolve_name_and_desc() {
        if !install_present() {
            return;
        }
        let vfs = Vfs::new(INSTALL, None).unwrap();
        let loc = loc::build(&vfs);
        let tables = load_tables(&vfs, &loc);
        for t in &tables {
            // Vanilla ships loc for levels 0..=32; every shipped level is covered.
            for lvl in t.levels.iter().take(33) {
                assert!(
                    lvl.name.as_deref().is_some_and(|n| !n.is_empty()),
                    "{} tech {} has no name",
                    t.kind,
                    lvl.index
                );
                assert!(
                    lvl.desc.as_deref().is_some_and(|d| !d.is_empty()),
                    "{} tech {} has no desc",
                    t.kind,
                    lvl.index
                );
            }
        }
        let adm = tables.iter().find(|t| t.kind == "adm").unwrap();
        assert_eq!(adm.levels[0].name.as_deref(), Some("Tribal Government"));
        // The script comment on this block reads "#Land Rights"; loc is authoritative.
        assert_eq!(adm.levels[5].name.as_deref(), Some("National Ideas"));
        let mil = tables.iter().find(|t| t.kind == "mil").unwrap();
        assert_eq!(mil.levels[0].name.as_deref(), Some("Pre-Medieval Military"));
        assert!(mil.levels[0].desc.as_deref().unwrap().contains("citizen soldier"));
    }

    /// The modifier keys vanilla tech actually grants all resolve to an icon file,
    /// and a bogus key fails cleanly rather than escaping the icon directory.
    #[test]
    fn vanilla_tech_modifier_icons_resolve() {
        if !install_present() {
            return;
        }
        let vfs = Vfs::new(INSTALL, None).unwrap();
        let loc = loc::build(&vfs);
        let tables = load_tables(&vfs, &loc);
        let mut checked = 0;
        for t in &tables {
            for lvl in &t.levels {
                for m in &lvl.modifiers {
                    // Not every modifier key has art; assert on the ones that do
                    // decode, and that decoding never panics.
                    if let Ok(png) = crate::icons::modifier_icon(&vfs, None, &m.key) {
                        assert!(png.starts_with(b"\x89PNG"), "{} is not a PNG", m.key);
                        checked += 1;
                    }
                }
            }
        }
        assert!(checked > 0, "no tech modifier resolved an icon");
        assert!(crate::icons::modifier_icon_rel("../../eu4.exe").is_none());
        assert!(crate::icons::modifier_icon_rel("land_morale").is_some());
        assert!(crate::icons::modifier_icon(&vfs, None, "not_a_real_modifier_xyz").is_err());
    }

    #[test]
    fn vanilla_validation_pip_budget_clean() {
        if !install_present() {
            return;
        }
        let vfs = Vfs::new(INSTALL, None).unwrap();
        let loc = loc::build(&vfs);
        let issues = validate_units(&vfs, &loc);
        let pip_warns = issues.iter().filter(|i| i.message.contains("total pips")).count();
        let orphans = issues.iter().filter(|i| i.message.contains("not enabled")).count();
        println!("[technology:validation] {} pip-budget outliers, {} orphan units", pip_warns, orphans);
        for i in &issues {
            println!("    {:?}: {}", i.severity, i.message);
        }
        // Vanilla is pip-budget clean at tolerance 6.
        assert_eq!(pip_warns, 0, "vanilla should have no pip-budget outliers");
        // Vanilla has exactly one orphan (zulu_chest_and_horns, enabled nowhere).
        assert!(orphans <= 2, "unexpected orphan count: {orphans}");
    }

    #[test]
    fn anbennar_tech_and_units_smoke() {
        if !install_present() || !Path::new(ANBENNAR).is_dir() {
            return;
        }
        let vfs = Vfs::new(INSTALL, Some(ANBENNAR)).unwrap();
        let loc = loc::build(&vfs);
        let tables = load_tables(&vfs, &loc);
        assert!(!tables.is_empty());
        let units = load_units(&vfs, &loc, &tables);
        assert!(!units.is_empty());
        // A mod (or base) unit round-trips a pip edit.
        if let Some(u) = units.iter().find(|u| u.is_land && u.pips.iter().any(|p| p.present)) {
            let bytes = vfs.read(&u.file).unwrap();
            let out = apply(
                &bytes,
                &Edit::SetScalar {
                    path: vec!["offensive_morale".into()],
                    value: "2".into(),
                    quoted: false,
                },
            );
            assert!(out.is_ok(), "pip edit should apply for {}", u.key);
        }
        // Validation runs without panicking.
        let issues = validate_units(&vfs, &loc);
        println!("[technology:anbennar] {} units, {} validation issues", units.len(), issues.len());
    }
}
