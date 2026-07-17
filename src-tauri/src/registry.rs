//! Phase 0.8 — generic registry loaders over `common/` (and `map/`) locations.
//!
//! A *registry* is "parse a game location into keyed entries + localized names".
//! Rather than ten bespoke parsers, there is one generic [`load_registry`] driven
//! by a per-registry [`RegistryConfig`] (see [`REGISTRIES`]). Every entry carries
//! its key, its localized display name, the file it came from, and the raw parsed
//! block (order- and duplicate-preserving) so later editors can honor the
//! preserve-unknown rule.
//!
//! Merge semantics mirror the game: files load in one alphabetical sweep with mod
//! files shadowing same-named base files (that is exactly what [`Vfs::list_dir`]
//! yields). Within that sweep, a repeated entry key updates the earlier entry's
//! content in place (last definition wins, first-seen position kept) — matching
//! EU4's forward-declaration + override pattern (e.g. `common/subject_types`).
//!
//! Idea/province modifier *keys* are intentionally NOT a file registry — no single
//! game file enumerates them. Instead [`known_modifiers`] exposes a curated static
//! list of the common country/province modifier keys with their value kind, so the
//! typed modifier editor can offer typed inputs while still accepting unknown keys
//! as free text.

use std::collections::HashMap;

use crate::loc::{self, LocStore};
use crate::paradox::{self, Block, Value};
use crate::vfs::Vfs;

// --- Public data types ---------------------------------------------------

/// A serializable, order- and duplicate-preserving mirror of a parsed block.
/// Faithful enough to round-trip bare lists, repeated keys, and nesting for the
/// read-only "advanced/raw" panels that preserve unmodeled content.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(untagged)]
pub enum RawValue {
    Scalar(String),
    Block(Vec<RawItem>),
}

/// One `(optional key, value)` pair from a parsed block. A `None` key is a bare
/// list element (e.g. the ids in `terrain_override = { 1 2 3 }`).
#[derive(Debug, Clone, serde::Serialize)]
pub struct RawItem {
    pub key: Option<String>,
    pub value: RawValue,
}

/// One registry entry: a keyed game object with a localized name.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RegistryEntry {
    /// The script key (e.g. `monarchy`, `temple`, `western`, `westerngfx`).
    pub key: String,
    /// Localized display name (loc key resolved, else prettified `key`).
    pub name: String,
    /// Game-relative file the entry was (last) defined in.
    pub source_file: String,
    /// The parsed block, preserved for later preserve-unknown display. Bare-list
    /// entries (graphical cultures) carry an empty block.
    pub raw: RawValue,
}

// --- Config --------------------------------------------------------------

/// Where a registry's data lives.
enum Source {
    /// A directory of `.txt` files, merged across mod/base (e.g.
    /// `common/governments`). Every `.txt` contributes.
    Folder(&'static str),
    /// A single file resolved through the Vfs (e.g. `common/technology.txt`).
    File(&'static str),
}

/// How entries are shaped inside the source.
enum Shape {
    /// Each `key = { ... }` at the top level is an entry.
    TopLevelBlocks,
    /// Entries are the `key = { ... }` blocks nested under one named parent block
    /// (e.g. `groups = { western = {...} }`, `categories = { grasslands = {...} }`).
    NestedUnder(&'static str),
    /// Bare identifiers, one token each (e.g. `graphicalculturetype.txt`). No block.
    BareList,
}

/// How to turn an entry key into its localisation key.
enum LocPattern {
    /// Loc key == entry key (reforms, tech groups, buildings, personalities,
    /// event/province modifiers, terrain categories, graphical cultures).
    AsIs,
    /// Uppercased entry key (governments: `monarchy` -> `MONARCHY`).
    Upper,
    /// Entry key with a fixed suffix (subject types: `vassal` -> `vassal_title`).
    Suffix(&'static str),
}

impl LocPattern {
    fn apply(&self, key: &str) -> String {
        match self {
            LocPattern::AsIs => key.to_string(),
            LocPattern::Upper => key.to_uppercase(),
            LocPattern::Suffix(s) => format!("{key}{s}"),
        }
    }
}

struct RegistryConfig {
    /// The name callers pass to [`load_registry`] / `get_registry`.
    name: &'static str,
    source: Source,
    shape: Shape,
    loc: LocPattern,
    /// Top-level keys that appear as blocks but are not real entries
    /// (e.g. governments' `pre_dharma_mapping` legacy mapping table).
    exclude: &'static [&'static str],
}

/// The full registry set. Every config was verified against the real game files.
static REGISTRIES: &[RegistryConfig] = &[
    RegistryConfig {
        name: "governments",
        source: Source::Folder("common/governments"),
        shape: Shape::TopLevelBlocks,
        loc: LocPattern::Upper, // MONARCHY -> "Monarchy"
        exclude: &["pre_dharma_mapping"],
    },
    RegistryConfig {
        name: "government_reforms",
        source: Source::Folder("common/government_reforms"),
        shape: Shape::TopLevelBlocks,
        loc: LocPattern::AsIs, // feudalism_reform -> "Feudal Nobility"
        exclude: &[],
    },
    RegistryConfig {
        name: "technology_groups",
        source: Source::File("common/technology.txt"),
        shape: Shape::NestedUnder("groups"),
        loc: LocPattern::AsIs, // western -> "Western"
        exclude: &[],
    },
    RegistryConfig {
        name: "graphical_cultures",
        source: Source::File("common/graphicalculturetype.txt"),
        shape: Shape::BareList,
        loc: LocPattern::AsIs, // westerngfx -> "Western"
        exclude: &[],
    },
    RegistryConfig {
        name: "ruler_personalities",
        source: Source::Folder("common/ruler_personalities"),
        shape: Shape::TopLevelBlocks,
        loc: LocPattern::AsIs, // just_personality -> "Just"
        exclude: &[],
    },
    // S3.2 — advisor types (`common/advisortypes/*.txt`). Each `key = { … }` is a
    // type; its loc key is the type key as-is (`philosopher` -> "Philosopher").
    // Feeds the country history timeline's typed advisor rows (type picker).
    RegistryConfig {
        name: "advisor_types",
        source: Source::Folder("common/advisortypes"),
        shape: Shape::TopLevelBlocks,
        loc: LocPattern::AsIs, // philosopher -> "Philosopher"
        exclude: &[],
    },
    RegistryConfig {
        name: "buildings",
        source: Source::Folder("common/buildings"),
        shape: Shape::TopLevelBlocks,
        loc: LocPattern::AsIs, // temple -> "Temple"
        exclude: &[],
    },
    // Sprint 27 — trade goods. Feeds the buildings editor's manufactory picker.
    RegistryConfig {
        name: "trade_goods",
        source: Source::Folder("common/tradegoods"),
        shape: Shape::TopLevelBlocks,
        loc: LocPattern::AsIs, // grain -> "Grain"
        exclude: &[],
    },
    RegistryConfig {
        name: "event_modifiers",
        source: Source::Folder("common/event_modifiers"),
        shape: Shape::TopLevelBlocks,
        loc: LocPattern::AsIs, // the_proper_old_ways -> "The Proper Old Ways"
        exclude: &[],
    },
    RegistryConfig {
        name: "province_triggered_modifiers",
        source: Source::Folder("common/province_triggered_modifiers"),
        shape: Shape::TopLevelBlocks,
        loc: LocPattern::AsIs, // religious_center -> "Religious Center"
        exclude: &[],
    },
    RegistryConfig {
        name: "subject_types",
        source: Source::Folder("common/subject_types"),
        shape: Shape::TopLevelBlocks,
        loc: LocPattern::Suffix("_title"), // vassal -> vassal_title -> "Vassal"
        // `default` is a template merged into every type; `dummy` is a scripted
        // example. Neither is a selectable subject type.
        exclude: &["default", "dummy"],
    },
    RegistryConfig {
        name: "terrain_categories",
        source: Source::File("map/terrain.txt"),
        shape: Shape::NestedUnder("categories"),
        loc: LocPattern::AsIs, // grasslands -> "Grasslands"
        exclude: &[],
    },
    // Sprint 13.2 — casus belli types. Each block's `war_goal = <wargoal key>`
    // links to a `wargoal_types` entry (surfaced via `raw` for the war editor).
    RegistryConfig {
        name: "cb_types",
        source: Source::Folder("common/cb_types"),
        shape: Shape::TopLevelBlocks,
        loc: LocPattern::AsIs, // cb_conquest -> "Conquest"
        exclude: &[],
    },
    // Sprint 13.2 — war goal types. Each block's base engine `type` (take_*/
    // defend_*/superiority/…) is in `raw`; the frontend derives the target kind
    // (province for take_*/take_region, tag for defend_*, none otherwise).
    RegistryConfig {
        name: "wargoal_types",
        source: Source::Folder("common/wargoal_types"),
        shape: Shape::TopLevelBlocks,
        loc: LocPattern::AsIs, // take_claim -> "Conquest of <target>" style loc
        exclude: &[],
    },
    // Sprint 27 W2 — scripted peace treaties. Feeds the war-goal editor's
    // `required_treaty_to_take_provinces` picker.
    RegistryConfig {
        name: "peace_treaties",
        source: Source::Folder("common/peace_treaties"),
        shape: Shape::TopLevelBlocks,
        loc: LocPattern::AsIs, // po_establish_eyalet -> prettified fallback
        exclude: &[],
    },
    // Sprint 21 — rebel factions. Feeds the province panel's revolt `type` picker.
    // Loc name is `<key>_title` (nationalist_rebels -> "Separatists").
    RegistryConfig {
        name: "rebel_types",
        source: Source::Folder("common/rebel_types"),
        shape: Shape::TopLevelBlocks,
        loc: LocPattern::Suffix("_title"),
        exclude: &[],
    },
];

// --- Loading -------------------------------------------------------------

fn parse_bytes(bytes: &[u8]) -> Block {
    // Game script is Windows-1252; only ASCII tokens matter.
    paradox::parse(&String::from_utf8_lossy(bytes))
}

/// Serializable mirror of a parsed block (preserves order and duplicates).
fn to_raw(block: &Block) -> RawValue {
    RawValue::Block(
        block
            .items
            .iter()
            .map(|(k, v)| RawItem {
                key: k.clone(),
                value: match v {
                    Value::Scalar(s) => RawValue::Scalar(s.clone()),
                    Value::Block(b) => to_raw(b),
                },
            })
            .collect(),
    )
}

/// Accumulates entries preserving first-seen order while letting a repeated key
/// update the existing entry's content in place (EU4 override semantics).
#[derive(Default)]
struct EntryAccum {
    order: Vec<String>,
    index: HashMap<String, usize>,
    raw: Vec<RawValue>,
    source: Vec<String>,
}

impl EntryAccum {
    fn push(&mut self, key: &str, raw: RawValue, source_file: &str) {
        match self.index.get(key) {
            Some(&i) => {
                self.raw[i] = raw;
                self.source[i] = source_file.to_string();
            }
            None => {
                self.index.insert(key.to_string(), self.order.len());
                self.order.push(key.to_string());
                self.raw.push(raw);
                self.source.push(source_file.to_string());
            }
        }
    }

    fn finish(self, config: &RegistryConfig, loc: &LocStore) -> Vec<RegistryEntry> {
        let mut out = Vec::with_capacity(self.order.len());
        for (i, key) in self.order.into_iter().enumerate() {
            let loc_key = config.loc.apply(&key);
            // Prefer the templated loc key; else prettify the *base* key (not the
            // templated one, so `vassal_title` never leaks as "Vassal Title").
            let name = loc
                .get(&loc_key)
                .map(str::to_string)
                .unwrap_or_else(|| loc::prettify(&key));
            out.push(RegistryEntry {
                key,
                name,
                source_file: self.source[i].clone(),
                raw: self.raw[i].clone(),
            });
        }
        out
    }
}

/// Iterates the `(entry_key, block)` pairs a config exposes from one parsed file,
/// applying the shape and exclusions, into `acc`.
fn collect_from_block(config: &RegistryConfig, block: &Block, source_file: &str, acc: &mut EntryAccum) {
    match &config.shape {
        Shape::TopLevelBlocks => {
            for (key, inner) in block.key_blocks() {
                if config.exclude.contains(&key) {
                    continue;
                }
                acc.push(key, to_raw(inner), source_file);
            }
        }
        Shape::NestedUnder(parent) => {
            let Some(parent_block) = block.get_block(parent) else {
                return;
            };
            for (key, inner) in parent_block.key_blocks() {
                if config.exclude.contains(&key) {
                    continue;
                }
                acc.push(key, to_raw(inner), source_file);
            }
        }
        Shape::BareList => {
            for id in block.bare_scalars() {
                if config.exclude.contains(&id) {
                    continue;
                }
                // Bare entries carry an empty block.
                acc.push(id, RawValue::Block(Vec::new()), source_file);
            }
        }
    }
}

/// Loads a registry by name, resolving localized names via `loc`.
pub fn load_registry(vfs: &Vfs, loc: &LocStore, name: &str) -> Result<Vec<RegistryEntry>, String> {
    let config = REGISTRIES
        .iter()
        .find(|c| c.name == name)
        .ok_or_else(|| format!("Unknown registry: {name}"))?;

    let mut acc = EntryAccum::default();
    match &config.source {
        Source::Folder(dir) => {
            for (file_name, path) in vfs.list_dir(dir) {
                if !file_name.to_lowercase().ends_with(".txt") {
                    continue;
                }
                let Ok(bytes) = std::fs::read(&path) else {
                    continue;
                };
                let block = parse_bytes(&bytes);
                let rel = format!("{dir}/{file_name}");
                collect_from_block(config, &block, &rel, &mut acc);
            }
        }
        Source::File(rel) => {
            let bytes = vfs.read(rel)?;
            let block = parse_bytes(&bytes);
            collect_from_block(config, &block, rel, &mut acc);
        }
    }
    Ok(acc.finish(config, loc))
}

/// Tauri command: load a registry as JSON entries.
#[tauri::command(async)]
pub fn get_registry(
    name: String,
    install_path: String,
    mod_path: Option<String>,
) -> Result<Vec<RegistryEntry>, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    let loc = loc::store(&vfs, &install_path, mod_path.as_deref());
    load_registry(&vfs, &loc, &name)
}

// --- Known modifiers (curated static list) -------------------------------

/// How a modifier's value is entered/displayed. `Percent` values are stored as
/// fractions in the files (0.05 == +5%); `Flat` are absolute numbers;
/// `Boolean` are `yes`/`no`.
#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ModifierKind {
    Percent,
    Flat,
    Boolean,
}

/// One known modifier key and its value kind. Keys not in this list are still
/// editable downstream as free text.
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct KnownModifier {
    pub key: &'static str,
    pub kind: ModifierKind,
}

use ModifierKind::{Boolean, Flat, Percent};

/// The ~90 most common EU4 country/province modifier keys with their value kind.
/// Sourced from the well-known EU4 modifier set. Not exhaustive by design — the
/// typed editor uses these for typed inputs and falls back to free text for the
/// long tail (mod-added and niche keys).
static KNOWN_MODIFIERS: &[KnownModifier] = &[
    // Military — land
    KnownModifier { key: "discipline", kind: Percent },
    KnownModifier { key: "land_morale", kind: Percent },
    KnownModifier { key: "infantry_power", kind: Percent },
    KnownModifier { key: "cavalry_power", kind: Percent },
    KnownModifier { key: "artillery_power", kind: Percent },
    KnownModifier { key: "fire_damage", kind: Percent },
    KnownModifier { key: "shock_damage", kind: Percent },
    KnownModifier { key: "fire_damage_received", kind: Percent },
    KnownModifier { key: "shock_damage_received", kind: Percent },
    KnownModifier { key: "cav_to_inf_ratio", kind: Percent },
    KnownModifier { key: "cavalry_flanking", kind: Percent },
    KnownModifier { key: "backrow_artillery_damage", kind: Percent },
    KnownModifier { key: "infantry_cost", kind: Percent },
    KnownModifier { key: "cavalry_cost", kind: Percent },
    KnownModifier { key: "artillery_cost", kind: Percent },
    KnownModifier { key: "land_maintenance_modifier", kind: Percent },
    KnownModifier { key: "reinforce_speed", kind: Percent },
    KnownModifier { key: "recover_army_morale_speed", kind: Percent },
    KnownModifier { key: "land_forcelimit_modifier", kind: Percent },
    KnownModifier { key: "global_manpower_modifier", kind: Percent },
    KnownModifier { key: "manpower_recovery_speed", kind: Percent },
    KnownModifier { key: "mercenary_cost", kind: Percent },
    KnownModifier { key: "merc_maintenance_modifier", kind: Percent },
    KnownModifier { key: "global_regiment_cost", kind: Percent },
    KnownModifier { key: "movement_speed", kind: Percent },
    KnownModifier { key: "siege_ability", kind: Percent },
    KnownModifier { key: "defensiveness", kind: Percent },
    KnownModifier { key: "garrison_size", kind: Percent },
    KnownModifier { key: "land_attrition", kind: Percent },
    KnownModifier { key: "hostile_attrition", kind: Flat },
    KnownModifier { key: "army_tradition", kind: Flat },
    KnownModifier { key: "army_tradition_decay", kind: Percent },
    KnownModifier { key: "leader_land_fire", kind: Flat },
    KnownModifier { key: "leader_land_shock", kind: Flat },
    KnownModifier { key: "leader_land_manuever", kind: Flat },
    KnownModifier { key: "leader_siege", kind: Flat },
    KnownModifier { key: "general_cost", kind: Percent },
    KnownModifier { key: "free_leader_pool", kind: Flat },
    KnownModifier { key: "land_forcelimit", kind: Flat },
    // Military — naval
    KnownModifier { key: "naval_morale", kind: Percent },
    KnownModifier { key: "galley_power", kind: Percent },
    KnownModifier { key: "heavy_ship_power", kind: Percent },
    KnownModifier { key: "light_ship_power", kind: Percent },
    KnownModifier { key: "ship_durability", kind: Percent },
    KnownModifier { key: "naval_maintenance_modifier", kind: Percent },
    KnownModifier { key: "naval_forcelimit_modifier", kind: Percent },
    KnownModifier { key: "global_ship_cost", kind: Percent },
    KnownModifier { key: "global_ship_repair", kind: Percent },
    KnownModifier { key: "global_sailors_modifier", kind: Percent },
    KnownModifier { key: "sailors_recovery_speed", kind: Percent },
    KnownModifier { key: "blockade_efficiency", kind: Percent },
    KnownModifier { key: "naval_attrition", kind: Percent },
    KnownModifier { key: "navy_tradition", kind: Flat },
    KnownModifier { key: "navy_tradition_decay", kind: Percent },
    KnownModifier { key: "leader_naval_fire", kind: Flat },
    KnownModifier { key: "leader_naval_shock", kind: Flat },
    KnownModifier { key: "leader_naval_manuever", kind: Flat },
    KnownModifier { key: "sea_repair", kind: Boolean },
    // Economy
    KnownModifier { key: "global_tax_modifier", kind: Percent },
    KnownModifier { key: "production_efficiency", kind: Percent },
    KnownModifier { key: "trade_efficiency", kind: Percent },
    KnownModifier { key: "global_trade_goods_size_modifier", kind: Percent },
    KnownModifier { key: "global_trade_power", kind: Percent },
    KnownModifier { key: "global_own_trade_power", kind: Percent },
    KnownModifier { key: "global_foreign_trade_power", kind: Percent },
    KnownModifier { key: "provincial_trade_power_modifier", kind: Percent },
    KnownModifier { key: "caravan_power", kind: Percent },
    KnownModifier { key: "trade_range_modifier", kind: Percent },
    KnownModifier { key: "merchants", kind: Flat },
    KnownModifier { key: "placed_merchant_power", kind: Flat },
    KnownModifier { key: "interest", kind: Flat },
    KnownModifier { key: "inflation_reduction", kind: Flat },
    KnownModifier { key: "development_cost", kind: Percent },
    KnownModifier { key: "build_cost", kind: Percent },
    KnownModifier { key: "fort_maintenance_modifier", kind: Percent },
    KnownModifier { key: "available_province_loot", kind: Percent },
    KnownModifier { key: "global_prosperity_growth", kind: Percent },
    // Government / administration
    KnownModifier { key: "global_unrest", kind: Flat },
    KnownModifier { key: "global_autonomy", kind: Percent },
    KnownModifier { key: "max_absolutism", kind: Flat },
    KnownModifier { key: "yearly_absolutism", kind: Flat },
    KnownModifier { key: "stability_cost_modifier", kind: Percent },
    KnownModifier { key: "war_exhaustion", kind: Flat },
    KnownModifier { key: "war_exhaustion_cost", kind: Percent },
    KnownModifier { key: "prestige", kind: Flat },
    KnownModifier { key: "prestige_decay", kind: Percent },
    KnownModifier { key: "legitimacy", kind: Flat },
    KnownModifier { key: "republican_tradition", kind: Flat },
    KnownModifier { key: "devotion", kind: Flat },
    KnownModifier { key: "horde_unity", kind: Flat },
    KnownModifier { key: "meritocracy", kind: Flat },
    KnownModifier { key: "yearly_corruption", kind: Flat },
    KnownModifier { key: "advisor_cost", kind: Percent },
    KnownModifier { key: "advisor_pool", kind: Flat },
    KnownModifier { key: "possible_policy", kind: Flat },
    KnownModifier { key: "free_policy", kind: Flat },
    // Technology / ideas
    KnownModifier { key: "technology_cost", kind: Percent },
    KnownModifier { key: "idea_cost", kind: Percent },
    KnownModifier { key: "adm_tech_cost_modifier", kind: Percent },
    KnownModifier { key: "dip_tech_cost_modifier", kind: Percent },
    KnownModifier { key: "mil_tech_cost_modifier", kind: Percent },
    KnownModifier { key: "global_institution_spread", kind: Percent },
    KnownModifier { key: "embracement_cost", kind: Percent },
    KnownModifier { key: "innovativeness_gain", kind: Percent },
    // Diplomacy
    KnownModifier { key: "diplomatic_reputation", kind: Flat },
    KnownModifier { key: "diplomatic_upkeep", kind: Flat },
    KnownModifier { key: "improve_relation_modifier", kind: Percent },
    KnownModifier { key: "ae_impact", kind: Percent },
    KnownModifier { key: "province_warscore_cost", kind: Percent },
    KnownModifier { key: "vassal_income", kind: Percent },
    KnownModifier { key: "liberty_desire", kind: Flat },
    KnownModifier { key: "reduced_liberty_desire", kind: Flat },
    KnownModifier { key: "spy_offence", kind: Percent },
    KnownModifier { key: "global_spy_defence", kind: Percent },
    KnownModifier { key: "rebel_support_efficiency", kind: Percent },
    // Religion / culture
    KnownModifier { key: "tolerance_own", kind: Flat },
    KnownModifier { key: "tolerance_heathen", kind: Flat },
    KnownModifier { key: "tolerance_heretic", kind: Flat },
    KnownModifier { key: "num_accepted_cultures", kind: Flat },
    KnownModifier { key: "global_missionary_strength", kind: Percent },
    KnownModifier { key: "missionaries", kind: Flat },
    KnownModifier { key: "religious_unity", kind: Percent },
    KnownModifier { key: "papal_influence", kind: Flat },
    KnownModifier { key: "monthly_fervor_increase", kind: Flat },
    KnownModifier { key: "church_power_modifier", kind: Percent },
    // Expansion / colonisation
    KnownModifier { key: "colonists", kind: Flat },
    KnownModifier { key: "global_colonial_growth", kind: Flat },
    KnownModifier { key: "global_settler_increase", kind: Flat },
    KnownModifier { key: "range", kind: Percent },
    KnownModifier { key: "fort_level", kind: Flat },
    // Booleans (capability toggles)
    KnownModifier { key: "may_recruit_female_generals", kind: Boolean },
    KnownModifier { key: "auto_explore_adjacent_to_colony", kind: Boolean },
    KnownModifier { key: "may_explore", kind: Boolean },
    KnownModifier { key: "cb_on_overseas", kind: Boolean },
    KnownModifier { key: "cb_on_primitives", kind: Boolean },
    KnownModifier { key: "cb_on_religious_enemies", kind: Boolean },
    KnownModifier { key: "may_perform_slave_raid", kind: Boolean },
    KnownModifier { key: "reduced_stab_impacts", kind: Boolean },
    KnownModifier { key: "idea_claim_colonies", kind: Boolean },
    KnownModifier { key: "extra_manpower_at_religious_war", kind: Boolean },
    KnownModifier { key: "may_establish_frontier", kind: Boolean },
];

/// The curated known-modifier list (key + value kind). Unknown keys still work
/// as free text in the editor.
pub fn known_modifiers() -> &'static [KnownModifier] {
    KNOWN_MODIFIERS
}

/// Tauri command: serve the known-modifier list to the frontend.
#[tauri::command(async)]
pub fn get_known_modifiers() -> Vec<KnownModifier> {
    KNOWN_MODIFIERS.to_vec()
}

// --- Tests ---------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    const INSTALL: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Europa Universalis IV";
    const ANBENNAR: &str = r"C:\Users\User\projects\anbennar-eu4-dev";

    fn install_present() -> bool {
        Path::new(INSTALL).join("map/provinces.bmp").is_file()
    }

    // Each test gets its own root: tests run in parallel and a shared dir would
    // race cleanup vs setup.
    fn setup(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("eu_toolkit_registry_test_{name}"));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    fn find<'a>(entries: &'a [RegistryEntry], key: &str) -> Option<&'a RegistryEntry> {
        entries.iter().find(|e| e.key == key)
    }

    // --- Synthetic fixture tests (per entry_shape) ---

    #[test]
    fn top_level_blocks_multi_file_override_by_key() {
        let root = setup("toplevel_override");
        let base = root.join("base");
        let modd = root.join("mymod");
        std::fs::create_dir_all(base.join("common/governments")).unwrap();
        std::fs::create_dir_all(modd.join("common/governments")).unwrap();
        // Base file defines monarchy + republic.
        std::fs::write(
            base.join("common/governments/00_governments.txt"),
            "# comment\nmonarchy = { reform_levels = { a = 1 } }\nrepublic = { x = yes }\n",
        )
        .unwrap();
        // Mod adds a later-collating file that overrides monarchy and adds theocracy.
        std::fs::write(
            modd.join("common/governments/01_extra.txt"),
            "monarchy = { overridden = yes }\ntheocracy = { y = 2 }\n",
        )
        .unwrap();
        // A .mod descriptor so it's a valid project.
        std::fs::write(modd.join("descriptor.mod"), "name=\"m\"\n").unwrap();

        let vfs = Vfs::new(base.to_str().unwrap(), Some(modd.to_str().unwrap())).unwrap();
        let loc = LocStore::from_pairs(&[("MONARCHY", "Monarchy")]);
        let entries = load_registry(&vfs, &loc, "governments").unwrap();

        // union of both files, in first-seen order (monarchy, republic, theocracy)
        let keys: Vec<&str> = entries.iter().map(|e| e.key.as_str()).collect();
        assert_eq!(keys, vec!["monarchy", "republic", "theocracy"]);
        // monarchy content overridden by the later file, source updated
        let m = find(&entries, "monarchy").unwrap();
        assert_eq!(m.name, "Monarchy"); // Upper loc pattern
        assert_eq!(m.source_file, "common/governments/01_extra.txt");
        if let RawValue::Block(items) = &m.raw {
            assert_eq!(items.len(), 1);
            assert_eq!(items[0].key.as_deref(), Some("overridden"));
        } else {
            panic!("expected block");
        }
        // republic keeps prettified fallback name (no loc entry)
        assert_eq!(find(&entries, "republic").unwrap().name, "Republic");
    }

    #[test]
    fn same_named_mod_file_shadows_base() {
        let root = setup("shadow");
        let base = root.join("base");
        let modd = root.join("mymod");
        std::fs::create_dir_all(base.join("common/buildings")).unwrap();
        std::fs::create_dir_all(modd.join("common/buildings")).unwrap();
        std::fs::write(
            base.join("common/buildings/00_buildings.txt"),
            "temple = { cost = 100 }\nbarracks = { cost = 50 }\n",
        )
        .unwrap();
        // Mod's same-named file shadows the base file entirely.
        std::fs::write(
            modd.join("common/buildings/00_buildings.txt"),
            "temple = { cost = 999 }\n",
        )
        .unwrap();
        std::fs::write(modd.join("descriptor.mod"), "name=\"m\"\n").unwrap();

        let vfs = Vfs::new(base.to_str().unwrap(), Some(modd.to_str().unwrap())).unwrap();
        let loc = LocStore::from_pairs(&[]);
        let entries = load_registry(&vfs, &loc, "buildings").unwrap();
        // barracks is gone (base file fully shadowed), temple has mod content
        let keys: Vec<&str> = entries.iter().map(|e| e.key.as_str()).collect();
        assert_eq!(keys, vec!["temple"]);
        if let RawValue::Block(items) = &find(&entries, "temple").unwrap().raw {
            assert_eq!(items[0].key.as_deref(), Some("cost"));
            assert!(matches!(&items[0].value, RawValue::Scalar(s) if s == "999"));
        } else {
            panic!("expected block");
        }
    }

    #[test]
    fn nested_under_parent() {
        let root = setup("nested");
        let base = root.join("base");
        std::fs::create_dir_all(base.join("common")).unwrap();
        std::fs::write(
            base.join("common/technology.txt"),
            "# header\ntables = { junk = 1 }\ngroups = {\n western = { start_level = 3 }\n eastern = { start_level = 3 }\n}\n",
        )
        .unwrap();
        let vfs = Vfs::new(base.to_str().unwrap(), None).unwrap();
        let loc = LocStore::from_pairs(&[("western", "Western"), ("eastern", "Eastern")]);
        let entries = load_registry(&vfs, &loc, "technology_groups").unwrap();
        let keys: Vec<&str> = entries.iter().map(|e| e.key.as_str()).collect();
        assert_eq!(keys, vec!["western", "eastern"]);
        assert_eq!(find(&entries, "western").unwrap().name, "Western");
    }

    #[test]
    fn bare_list_windows_1252_and_comments() {
        let root = setup("barelist");
        let base = root.join("base");
        std::fs::create_dir_all(base.join("common")).unwrap();
        // Include a comment and a Windows-1252 high byte in a comment to prove
        // byte reads + from_utf8_lossy tolerate it.
        let mut bytes: Vec<u8> = b"# graphical culture list\nwesterngfx\neasterngfx # trailing\n# ".to_vec();
        bytes.push(0xE9); // 'e-acute' in Windows-1252, inside a comment line
        bytes.extend_from_slice(b" comment\nmuslimgfx\n");
        std::fs::write(base.join("common/graphicalculturetype.txt"), &bytes).unwrap();
        let vfs = Vfs::new(base.to_str().unwrap(), None).unwrap();
        let loc = LocStore::from_pairs(&[("westerngfx", "Western")]);
        let entries = load_registry(&vfs, &loc, "graphical_cultures").unwrap();
        let keys: Vec<&str> = entries.iter().map(|e| e.key.as_str()).collect();
        assert_eq!(keys, vec!["westerngfx", "easterngfx", "muslimgfx"]);
        assert_eq!(find(&entries, "westerngfx").unwrap().name, "Western");
        // bare entries carry an empty block
        assert!(matches!(&find(&entries, "muslimgfx").unwrap().raw, RawValue::Block(v) if v.is_empty()));
    }

    #[test]
    fn subject_type_forward_decl_then_full_def() {
        // Mirrors the real file: an empty forward declaration, then the full
        // definition later — the full one must win (last-parsed) and keep its
        // first-seen slot, with the `_title` loc pattern.
        let root = setup("subject_fwd");
        let base = root.join("base");
        std::fs::create_dir_all(base.join("common/subject_types")).unwrap();
        std::fs::write(
            base.join("common/subject_types/00_subject_types.txt"),
            "vassal = {}\nmarch = {}\ndefault = { template = yes }\nvassal = { copy_from = default count = 1 }\n",
        )
        .unwrap();
        let vfs = Vfs::new(base.to_str().unwrap(), None).unwrap();
        let loc = LocStore::from_pairs(&[("vassal_title", "Vassal"), ("march_title", "March")]);
        let entries = load_registry(&vfs, &loc, "subject_types").unwrap();
        // `default` excluded; order vassal, march
        let keys: Vec<&str> = entries.iter().map(|e| e.key.as_str()).collect();
        assert_eq!(keys, vec!["vassal", "march"]);
        assert_eq!(find(&entries, "vassal").unwrap().name, "Vassal");
        // full definition won
        if let RawValue::Block(items) = &find(&entries, "vassal").unwrap().raw {
            assert!(items.iter().any(|i| i.key.as_deref() == Some("copy_from")));
        }
    }

    #[test]
    fn governments_excludes_pre_dharma_mapping() {
        let root = setup("gov_exclude");
        let base = root.join("base");
        std::fs::create_dir_all(base.join("common/governments")).unwrap();
        std::fs::write(
            base.join("common/governments/00_governments.txt"),
            "monarchy = { a = 1 }\npre_dharma_mapping = { despotic_monarchy = { government = monarchy } }\n",
        )
        .unwrap();
        let vfs = Vfs::new(base.to_str().unwrap(), None).unwrap();
        let loc = LocStore::from_pairs(&[]);
        let entries = load_registry(&vfs, &loc, "governments").unwrap();
        let keys: Vec<&str> = entries.iter().map(|e| e.key.as_str()).collect();
        assert_eq!(keys, vec!["monarchy"]);
    }

    #[test]
    fn unknown_registry_errors() {
        let root = setup("unknown");
        let base = root.join("base");
        std::fs::create_dir_all(base.join("common")).unwrap();
        std::fs::write(base.join("common/technology.txt"), "groups = {}\n").unwrap();
        let vfs = Vfs::new(base.to_str().unwrap(), None).unwrap();
        let loc = LocStore::from_pairs(&[]);
        assert!(load_registry(&vfs, &loc, "no_such_registry").is_err());
    }

    // --- Known modifiers ---

    #[test]
    fn known_modifiers_curated_and_typed() {
        let mods = known_modifiers();
        assert!(mods.len() >= 80, "expected >=80 known modifiers, got {}", mods.len());
        // No duplicate keys.
        let mut keys: Vec<&str> = mods.iter().map(|m| m.key).collect();
        keys.sort_unstable();
        let before = keys.len();
        keys.dedup();
        assert_eq!(before, keys.len(), "duplicate modifier keys present");
        // Spot-check kinds.
        let by = |k: &str| mods.iter().find(|m| m.key == k).map(|m| m.kind);
        assert!(matches!(by("discipline"), Some(ModifierKind::Percent)));
        assert!(matches!(by("tolerance_own"), Some(ModifierKind::Flat)));
        assert!(matches!(by("may_recruit_female_generals"), Some(ModifierKind::Boolean)));
        // All three kinds represented.
        assert!(mods.iter().any(|m| matches!(m.kind, ModifierKind::Percent)));
        assert!(mods.iter().any(|m| matches!(m.kind, ModifierKind::Flat)));
        assert!(mods.iter().any(|m| matches!(m.kind, ModifierKind::Boolean)));
    }

    // --- Real-install tests (no-op silently if absent) ---

    fn real_entries(name: &str) -> Option<Vec<RegistryEntry>> {
        if !install_present() {
            return None;
        }
        let vfs = Vfs::new(INSTALL, None).unwrap();
        let loc = loc::build(&vfs);
        Some(load_registry(&vfs, &loc, name).unwrap())
    }

    #[test]
    fn real_governments() {
        let Some(e) = real_entries("governments") else { return };
        assert!(!e.is_empty());
        let m = find(&e, "monarchy").expect("monarchy government exists");
        assert_eq!(m.name, "Monarchy");
        assert!(find(&e, "republic").is_some());
        assert!(find(&e, "theocracy").is_some());
        assert!(find(&e, "pre_dharma_mapping").is_none(), "mapping table excluded");
    }

    #[test]
    fn real_government_reforms() {
        let Some(e) = real_entries("government_reforms") else { return };
        assert!(!e.is_empty());
        // feudalism_reform localizes to "Feudal Nobility".
        let r = find(&e, "feudalism_reform").expect("feudalism_reform exists");
        assert_eq!(r.name, "Feudal Nobility");
        assert!(find(&e, "ottoman_government").is_some());
    }

    #[test]
    fn real_technology_groups() {
        let Some(e) = real_entries("technology_groups") else { return };
        assert!(!e.is_empty());
        assert_eq!(find(&e, "western").expect("western group").name, "Western");
        assert_eq!(find(&e, "eastern").expect("eastern group").name, "Eastern");
    }

    #[test]
    fn real_graphical_cultures() {
        let Some(e) = real_entries("graphical_cultures") else { return };
        assert!(!e.is_empty());
        assert_eq!(find(&e, "westerngfx").expect("westerngfx").name, "Western");
    }

    #[test]
    fn real_ruler_personalities() {
        let Some(e) = real_entries("ruler_personalities") else { return };
        assert!(!e.is_empty());
        // ambitious_personality does NOT exist in vanilla; just_personality does.
        assert_eq!(find(&e, "just_personality").expect("just_personality").name, "Just");
    }

    #[test]
    fn real_advisor_types() {
        let Some(e) = real_entries("advisor_types") else { return };
        assert!(!e.is_empty());
        // philosopher is an ADM advisor; its loc name is "Philosopher".
        assert_eq!(find(&e, "philosopher").expect("philosopher advisor").name, "Philosopher");
        assert!(find(&e, "statesman").is_some());
        assert!(find(&e, "naval_reformer").is_some());
    }

    #[test]
    fn real_buildings() {
        let Some(e) = real_entries("buildings") else { return };
        assert!(!e.is_empty());
        assert_eq!(find(&e, "temple").expect("temple building").name, "Temple");
        assert!(find(&e, "marketplace").is_some());
    }

    #[test]
    fn real_trade_goods() {
        let Some(e) = real_entries("trade_goods") else { return };
        assert!(!e.is_empty());
        assert_eq!(find(&e, "grain").expect("grain trade good").name, "Grain");
        assert!(find(&e, "wine").is_some());
    }

    #[test]
    fn real_event_modifiers() {
        let Some(e) = real_entries("event_modifiers") else { return };
        assert!(!e.is_empty());
        assert!(find(&e, "the_proper_old_ways").is_some());
    }

    #[test]
    fn real_province_triggered_modifiers() {
        let Some(e) = real_entries("province_triggered_modifiers") else { return };
        assert!(!e.is_empty());
        let r = find(&e, "religious_center").expect("religious_center ptm");
        assert_eq!(r.name, "Religious Center");
    }

    #[test]
    fn real_subject_types() {
        let Some(e) = real_entries("subject_types") else { return };
        assert!(!e.is_empty());
        assert_eq!(find(&e, "vassal").expect("vassal subject type").name, "Vassal");
        assert!(find(&e, "march").is_some());
        assert!(find(&e, "default").is_none(), "template excluded");
    }

    #[test]
    fn real_terrain_categories() {
        let Some(e) = real_entries("terrain_categories") else { return };
        assert!(!e.is_empty());
        assert_eq!(find(&e, "grasslands").expect("grasslands terrain").name, "Grasslands");
        assert!(find(&e, "mountain").is_some());
    }

    #[test]
    fn real_cb_types_carry_war_goal_ref() {
        let Some(e) = real_entries("cb_types") else { return };
        assert!(e.len() > 50, "expected many CB types, got {}", e.len());
        // cb_conquest is a stock CB; its block names its war goal type.
        let cbc = find(&e, "cb_conquest").expect("cb_conquest exists");
        let war_goal = match &cbc.raw {
            RawValue::Block(items) => items
                .iter()
                .find(|i| i.key.as_deref() == Some("war_goal"))
                .and_then(|i| match &i.value {
                    RawValue::Scalar(s) => Some(s.as_str()),
                    _ => None,
                }),
            _ => None,
        };
        assert!(war_goal.is_some(), "cb_conquest should reference a war_goal");
    }

    #[test]
    fn real_wargoal_types_carry_engine_type() {
        let Some(e) = real_entries("wargoal_types") else { return };
        assert!(e.len() > 50, "expected many war goal types, got {}", e.len());
        // Every stock wargoal has a base engine `type`; the frontend derives the
        // target kind from it. take_claim -> take_province engine type (province).
        let engine_type = |key: &str| -> Option<String> {
            let ent = find(&e, key)?;
            match &ent.raw {
                RawValue::Block(items) => items
                    .iter()
                    .find(|i| i.key.as_deref() == Some("type"))
                    .and_then(|i| match &i.value {
                        RawValue::Scalar(s) => Some(s.clone()),
                        _ => None,
                    }),
                _ => None,
            }
        };
        assert_eq!(engine_type("take_claim").as_deref(), Some("take_province"));
        // superiority_crusade (Crusade of Varna's goal) is a superiority goal.
        assert_eq!(engine_type("superiority_crusade").as_deref(), Some("superiority"));
    }

    #[test]
    fn anbennar_custom_cb_and_wargoal() {
        if !install_present() || !Path::new(ANBENNAR).is_dir() {
            return;
        }
        let vfs = Vfs::new(INSTALL, Some(ANBENNAR)).unwrap();
        let loc = loc::build(&vfs);
        // Anbennar replace_paths neither folder wholesale, but adds custom
        // entries additively; the monster-invasion CB/wargoal must appear.
        let cbs = load_registry(&vfs, &loc, "cb_types").unwrap();
        let wgs = load_registry(&vfs, &loc, "wargoal_types").unwrap();
        assert!(find(&cbs, "cb_monster_vs_civ").is_some(), "Anbennar cb_monster_vs_civ");
        assert!(find(&wgs, "superiority_monster").is_some(), "Anbennar superiority_monster");
    }

    // --- Anbennar smoke test (no-op if absent) ---

    #[test]
    fn anbennar_custom_graphical_culture() {
        if !install_present() || !Path::new(ANBENNAR).is_dir() {
            return;
        }
        let vfs = Vfs::new(INSTALL, Some(ANBENNAR)).unwrap();
        let loc = loc::build(&vfs);
        let e = load_registry(&vfs, &loc, "graphical_cultures").unwrap();
        // Anbennar replaces graphicalculturetype.txt with custom entries.
        assert!(find(&e, "elvengfx").is_some(), "Anbennar's elvengfx should appear");
        assert!(find(&e, "orcgreengfx").is_some());
    }
}
