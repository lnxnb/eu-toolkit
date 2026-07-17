//! Sprint 2.4 — full province history parse (top-level + dated blocks) + joined
//! geography, in one JSON payload for the province panel and the reusable
//! history timeline (2.3).
//!
//! Everything reads through the [`Vfs`] so a mod project overlays the base
//! install. This module is intentionally self-contained: it reimplements the
//! small id→file and id-list parsing it needs (mirroring `game_data`) rather
//! than calling into `game_data`, so it stays decoupled from concurrent
//! refactors there. Nothing here writes files — it is a pure read/derive layer.
//!
//! ## Payload shape ([`ProvinceDetails`])
//! * `top_level` — the 1444 state exactly as written at the file's top level
//!   (modeled known keys), plus `buildings` (all `key = yes` scalars that aren't
//!   a known non-building boolean — the panel cross-references common/buildings
//!   to split true buildings from other flags) and `raw_remainder` (every
//!   unmodeled top-level statement, reconstructed as text for read-only
//!   display — preserve-unknown rule).
//! * `effective_1444` — `top_level` re-derived by applying every **pre-start**
//!   dated block (date ≤ 1444.11.11) in file order, exactly as the game does.
//!   A pre-start `religion = protestant` block is reflected here even though the
//!   top level still says `catholic`; the map render and panel show *this*.
//! * `dated_blocks` — every `<date> = { ... }` block in file order, each with
//!   its `entries` (raw key/value rows, values are the raw scalar or the block
//!   reconstructed as text), a `post_start` flag (date > 1444.11.11), and an
//!   `occurrence_index` (the nth block, 0-based, sharing this exact date — see
//!   edit-addressing below).
//! * `geography` — area/region/superregion, trade node, climate zone + winter
//!   severity + impassable + monsoon, continent, terrain override, all with
//!   localized names; plus water flag.
//!
//! ## Edit addressing for dated blocks (recommendation for Sprint 2 wiring)
//! `mod_writer` addresses a dated block by its date key (e.g. `["1453.5.29"]`)
//! and resolves the **first** match. Duplicate dates are legal in these files
//! (Constantinople has none, but many provinces do), so first-match addressing
//! is ambiguous for the 2nd+ block of a shared date. `occurrence_index` is
//! surfaced so the host can act safely:
//!
//! * **Add a dated entry** (`addEntry`): emit a single
//!   `InsertStatement { block_path: [], statement: "<date> = { ... }" }`. Always
//!   safe — it appends; the game applies by file order, so an appended block is
//!   the latest edit for that date, which is the intended semantics. (Display
//!   sorts by date; the file stays append-only.)
//! * **Edit / delete an entry in a block whose date is UNIQUE in the file**
//!   (`occurrence_index == 0` and no sibling shares the date): address by the
//!   date path directly — `SetScalar { path: [date, key] }` or
//!   `RemoveStatement { block_path: [date], key }` (optionally with a `value`
//!   filter to disambiguate duplicate keys *inside* the block, e.g. two
//!   `add_core`s). Safe today.
//! * **Edit / delete inside a DUPLICATE-date block** (`occurrence_index > 0`, or
//!   any sibling shares the date): **not** safely addressable with first-match
//!   today. Two paths forward, in order of preference:
//!     1. *Recommended future fix:* extend `mod_writer` path addressing with an
//!        optional occurrence index on a path segment (e.g. a segment form like
//!        `"1453.5.29#1"`, or an `occurrence: usize` field on the edit) so the
//!        nth same-date block is addressable. `occurrence_index` in this payload
//!        is exactly the value that extension consumes. (This module does not
//!        implement it — `mod_writer` is owned elsewhere this wave.)
//!     2. *Interim, no writer change:* rewrite the whole same-date run —
//!        `RemoveStatement` the date (removes the first), repeat per occurrence,
//!        then re-`InsertStatement` the desired blocks. Coarser undo, but
//!        byte-safe. The host should gate fine-grained edits behind a
//!        "duplicate date" gate keyed on `occurrence_index`/sibling count.

use std::collections::HashMap;

use crate::date::{self, Date};
#[cfg(test)]
use crate::date::DEFAULT_START;
use crate::loc::LocStore;
use crate::paradox::{self, Block, Value};
use crate::vfs::Vfs;

// --- Parsing helpers (mirrors game_data; kept private for decoupling) ------

fn parse_bytes(bytes: &[u8]) -> Block {
    paradox::parse(&String::from_utf8_lossy(bytes))
}

fn parse_rel(vfs: &Vfs, rel: &str) -> Option<Block> {
    vfs.read(rel).ok().map(|b| parse_bytes(&b))
}

use date::parse_date;

// --- Serializable payload ---------------------------------------------------

/// A raw (identity) key + its localized display name.
#[derive(Debug, Clone, serde::Serialize)]
pub struct KeyName {
    pub key: String,
    pub name: String,
}

impl KeyName {
    fn new(key: impl Into<String>, loc: &LocStore) -> Self {
        let key = key.into();
        let name = loc.resolve(&key);
        KeyName { key, name }
    }
}

/// One reconstructed statement for read-only display: a key, its value (raw
/// scalar, or the block reconstructed as inline text), and whether it was a
/// `{ ... }` block. Bare list elements (rare at these levels) get an empty key.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RawStatement {
    pub key: String,
    pub value: String,
    pub is_block: bool,
}

/// The modeled province state at a point in time (1444 as-written, or the
/// re-derived effective 1444). All fields optional/empty when absent from the
/// file — provinces legitimately lack religion/culture/trade_goods etc.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct ProvinceStateSnapshot {
    pub owner: Option<String>,
    pub controller: Option<String>,
    pub cores: Vec<String>,
    pub claims: Vec<String>,
    pub culture: Option<String>,
    pub religion: Option<String>,
    pub trade_goods: Option<String>,
    pub latent_trade_goods: Option<String>,
    pub base_tax: Option<f64>,
    pub base_production: Option<f64>,
    pub base_manpower: Option<f64>,
    /// The `capital = "..."` city-name string (quotes stripped by the parser).
    pub capital: Option<String>,
    pub is_city: Option<bool>,
    pub hre: Option<bool>,
    pub seat_in_parliament: Option<bool>,
    pub discovered_by: Vec<String>,
    /// All `key = yes` scalars except the known non-building booleans
    /// (`is_city`/`hre`/`seat_in_parliament`). The panel splits true buildings
    /// (common/buildings) from other flags; both are preserved here.
    pub buildings: Vec<String>,
    pub native_size: Option<f64>,
    pub native_ferocity: Option<f64>,
    pub native_hostileness: Option<f64>,
    pub center_of_trade: Option<i64>,
    pub extra_cost: Option<i64>,
    pub tribal_owner: Option<String>,
    pub reformation_center: Option<String>,
    /// `add_local_autonomy = N` — a start effect (verified: the top-level key
    /// vanilla uses; there is no `local_autonomy` history key).
    pub local_autonomy: Option<f64>,
    /// `unrest = N` — verified present in province histories (Constantinople's
    /// dated blocks); also valid at the top level.
    pub unrest: Option<f64>,
    /// `add_to_trade_company = TAG` — the start assignment that puts the province
    /// into that country's trade company (verified: a real province-history key,
    /// distinct from the regional grouping in common/trade_companies).
    pub trade_company: Option<String>,
}

impl ProvinceStateSnapshot {
    /// Applies one `key = value` statement to the snapshot, with add/remove list
    /// semantics for cores/claims and `= yes/no` semantics for buildings/flags.
    /// Block values (modifiers etc.) don't change the modeled snapshot. Shared
    /// by top-level parsing and dated-block re-derivation, so effective state
    /// follows exactly the same rules the game applies in file order.
    fn apply(&mut self, key: &str, value: &Value) {
        // `latent_trade_goods` is the one modeled key written as a block in
        // vanilla (`latent_trade_goods = { coal }`); accept both forms.
        if key == "latent_trade_goods" {
            self.latent_trade_goods = match value {
                Value::Scalar(s) => Some(s.clone()),
                Value::Block(b) => b.bare_scalars().next().map(|s| s.to_string()),
            };
            return;
        }
        let scalar = match value {
            Value::Scalar(s) => s.as_str(),
            // Blocks (add_permanent_province_modifier = { ... }) are not modeled
            // scalars; they never mutate the snapshot.
            Value::Block(_) => return,
        };
        let as_f64 = || scalar.parse::<f64>().ok();
        let as_i64 = || scalar.parse::<f64>().ok().map(|f| f as i64);
        let as_bool = || match scalar {
            "yes" => Some(true),
            "no" => Some(false),
            _ => None,
        };
        let s = || Some(scalar.to_string());
        let push_unique = |list: &mut Vec<String>| {
            if !list.iter().any(|x| x == scalar) {
                list.push(scalar.to_string());
            }
        };
        match key {
            "owner" => self.owner = s(),
            "controller" => self.controller = s(),
            "culture" => self.culture = s(),
            "religion" => self.religion = s(),
            "trade_goods" => self.trade_goods = s(),
            "latent_trade_goods" => self.latent_trade_goods = s(),
            "tribal_owner" => self.tribal_owner = s(),
            "reformation_center" => self.reformation_center = s(),
            "capital" => self.capital = s(),
            "base_tax" => self.base_tax = as_f64(),
            "base_production" => self.base_production = as_f64(),
            "base_manpower" => self.base_manpower = as_f64(),
            "native_size" => self.native_size = as_f64(),
            "native_ferocity" => self.native_ferocity = as_f64(),
            "native_hostileness" => self.native_hostileness = as_f64(),
            "center_of_trade" => self.center_of_trade = as_i64(),
            "extra_cost" => self.extra_cost = as_i64(),
            "add_local_autonomy" => self.local_autonomy = as_f64(),
            "unrest" => self.unrest = as_f64(),
            "add_to_trade_company" => self.trade_company = s(),
            "is_city" => self.is_city = as_bool(),
            "hre" => self.hre = as_bool(),
            "seat_in_parliament" => self.seat_in_parliament = as_bool(),
            "add_core" => push_unique(&mut self.cores),
            "remove_core" => self.cores.retain(|c| c != scalar),
            "add_claim" => push_unique(&mut self.claims),
            "remove_claim" => self.claims.retain(|c| c != scalar),
            "discovered_by" => push_unique(&mut self.discovered_by),
            _ => match as_bool() {
                // Building/flag: the KEY is the identity (fort_15th = yes).
                Some(true) => {
                    if !self.buildings.iter().any(|b| b == key) {
                        self.buildings.push(key.to_string());
                    }
                }
                Some(false) => self.buildings.retain(|b| b != key),
                None => {} // unmodeled scalar: captured in raw_remainder instead
            },
        }
    }
}

/// True if a top-level `key = value` statement is consumed by the modeled
/// snapshot (so it should NOT also appear in `raw_remainder`).
fn is_modeled_top_level(key: &str, value: &Value) -> bool {
    const MODELED: &[&str] = &[
        "owner",
        "controller",
        "culture",
        "religion",
        "trade_goods",
        "latent_trade_goods",
        "tribal_owner",
        "reformation_center",
        "capital",
        "base_tax",
        "base_production",
        "base_manpower",
        "native_size",
        "native_ferocity",
        "native_hostileness",
        "center_of_trade",
        "extra_cost",
        "add_local_autonomy",
        "unrest",
        "add_to_trade_company",
        "is_city",
        "hre",
        "seat_in_parliament",
        "add_core",
        "remove_core",
        "add_claim",
        "remove_claim",
        "discovered_by",
    ];
    if MODELED.contains(&key) {
        return true;
    }
    // `key = yes/no` scalars are modeled as buildings/flags.
    matches!(value, Value::Scalar(s) if s == "yes" || s == "no")
}

/// Every top-level `Y.M.D = { ... }` block of `block`, in file order, each with
/// its entries, a `post_start` flag (block date > `at`), and a per-date
/// `occurrence_index` (0-based among blocks sharing the exact date, in file
/// order — load-bearing for occurrence-qualified edit addressing).
///
/// Shared by the province timeline (Sprint 2.3/2.4) and the country history
/// timeline (S3.2): a country history file has the same dated-block shape, so
/// the same reconstruction + occurrence indexing serves both.
pub fn dated_blocks_of(block: &Block, at: Date) -> Vec<DatedBlock> {
    let mut dated_blocks = Vec::new();
    let mut date_counts: HashMap<String, usize> = HashMap::new();
    for (key, value) in &block.items {
        let (Some(k), Value::Block(b)) = (key, value) else {
            continue;
        };
        let Some(block_date) = parse_date(k) else { continue };
        let occurrence_index = {
            let c = date_counts.entry(k.clone()).or_insert(0);
            let i = *c;
            *c += 1;
            i
        };
        let entries = b.items.iter().map(|(ek, ev)| raw_statement(ek, ev)).collect();
        dated_blocks.push(DatedBlock {
            date: k.clone(),
            post_start: block_date > at,
            occurrence_index,
            entries,
        });
    }
    dated_blocks
}

/// One dated history block.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DatedBlock {
    /// The date key exactly as written, e.g. `1453.5.29`.
    pub date: String,
    /// date > 1444.11.11 — doesn't affect the 1444 map render.
    pub post_start: bool,
    /// 0-based index among blocks sharing this exact date, in file order.
    /// Needed to address the right block given first-match edit paths.
    pub occurrence_index: usize,
    /// The block's statements, in file order (duplicates preserved).
    pub entries: Vec<RawStatement>,
}

/// Joined geography for a province (all from map/ files, none from history).
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct Geography {
    pub area: Option<KeyName>,
    pub region: Option<KeyName>,
    pub superregion: Option<KeyName>,
    pub trade_node: Option<KeyName>,
    /// Climate zone (tropical/arid/arctic); absent = temperate.
    pub climate: Option<KeyName>,
    /// Winter severity (mild/normal/severe_winter); absent = no winter.
    pub winter: Option<KeyName>,
    pub impassable: bool,
    pub monsoon: Option<KeyName>,
    pub continent: Option<KeyName>,
    /// Terrain type whose `terrain_override` list contains this province, if any
    /// (else the province takes its terrain from terrain.bmp — not resolved here).
    pub terrain_override: Option<KeyName>,
    /// Sea or lake (from default.map).
    pub water: bool,
}

/// The full province-details payload (Sprint 2.4).
#[derive(Debug, Clone, serde::Serialize)]
pub struct ProvinceDetails {
    pub id: u32,
    /// Game-relative history file path (existing file, else a synthesized
    /// `history/provinces/<id> - <name>.txt` so a first edit has a target).
    pub file: String,
    /// Whether a history file actually exists (vs. a synthesized path).
    pub exists: bool,
    /// PROV<id> loc, falling back to definition.csv's name, then "Province <id>".
    pub localized_name: String,
    /// definition.csv name column (identity-ish; may be empty).
    pub definition_name: String,
    /// Owner tag from the effective 1444 state, convenience for the header.
    pub owner: Option<String>,
    /// 1444 state exactly as written at the top level.
    pub top_level: ProvinceStateSnapshot,
    /// Top level re-derived through pre-start dated blocks (what the game shows).
    pub effective_1444: ProvinceStateSnapshot,
    /// Unmodeled top-level statements, reconstructed for read-only display.
    pub raw_remainder: Vec<RawStatement>,
    /// Every dated block in file order.
    pub dated_blocks: Vec<DatedBlock>,
    pub geography: Geography,
}

// --- Value reconstruction (for read-only display of unmodeled content) -----

fn value_to_text(v: &Value) -> String {
    match v {
        Value::Scalar(s) => s.clone(),
        Value::Block(b) => block_to_text(b),
    }
}

/// Reconstructs a block as inline `{ a = b c = { ... } bare }` text. Faithful to
/// content (not original whitespace/comments) — only for read-only display.
fn block_to_text(b: &Block) -> String {
    let mut parts = Vec::new();
    for (k, v) in &b.items {
        match (k, v) {
            (Some(k), v) => parts.push(format!("{} = {}", k, value_to_text(v))),
            (None, v) => parts.push(value_to_text(v)),
        }
    }
    if parts.is_empty() {
        "{ }".to_string()
    } else {
        format!("{{ {} }}", parts.join(" "))
    }
}

fn raw_statement(key: &Option<String>, value: &Value) -> RawStatement {
    RawStatement {
        key: key.clone().unwrap_or_default(),
        value: value_to_text(value),
        is_block: matches!(value, Value::Block(_)),
    }
}

// --- id → history file resolution (mirrors game_data::province_history) -----

/// definition.csv name column for each province id (may be empty strings).
fn province_names(vfs: &Vfs) -> HashMap<u32, String> {
    let mut names = HashMap::new();
    let Ok(bytes) = vfs.read("map/definition.csv") else {
        return names;
    };
    let text = String::from_utf8_lossy(&bytes);
    for line in text.lines() {
        let mut parts = line.split(';');
        let Some(Ok(id)) = parts.next().map(|s| s.trim().parse::<u32>()) else {
            continue;
        };
        if let Some(name) = parts.nth(3) {
            names.insert(id, name.trim().to_string());
        }
    }
    names
}

/// Resolves a province id to its history file: (game-relative path, bytes,
/// exists). When no file exists, synthesizes a sensibly named path so a first
/// edit still writes somewhere reasonable.
fn resolve_history_file(vfs: &Vfs, id: u32, names: &HashMap<u32, String>) -> (String, Vec<u8>, bool) {
    for (name, path) in vfs.list_dir("history/provinces") {
        if !name.to_lowercase().ends_with(".txt") {
            continue;
        }
        let digits: String = name.chars().take_while(|c| c.is_ascii_digit()).collect();
        if digits.parse::<u32>() == Ok(id) {
            if let Ok(bytes) = std::fs::read(&path) {
                return (format!("history/provinces/{name}"), bytes, true);
            }
        }
    }
    // No file: synthesize a name from definition.csv.
    let nm = names
        .get(&id)
        .filter(|n| !n.is_empty())
        .cloned()
        .unwrap_or_else(|| format!("Province {id}"));
    (format!("history/provinces/{id} - {nm}.txt"), Vec::new(), false)
}

// --- Geography joins (all parsed locally, per preserve-decoupling) ----------

/// Inverts a `key = { ids... }` file (climate/continent lists): id -> every key
/// whose list contains it (a province can be in multiple, e.g. arid + severe).
fn invert_id_lists(block: &Block) -> HashMap<u32, Vec<String>> {
    let mut out: HashMap<u32, Vec<String>> = HashMap::new();
    for (key, list) in block.key_blocks() {
        for id in list.bare_ids() {
            out.entry(id).or_default().push(key.to_string());
        }
    }
    out
}

/// area name containing `id`, from map/area.txt (bare_ids skips the color block).
fn area_for(vfs: &Vfs, id: u32) -> Option<String> {
    let block = parse_rel(vfs, "map/area.txt")?;
    for (name, b) in block.key_blocks() {
        if b.bare_ids().contains(&id) {
            return Some(name.to_string());
        }
    }
    None
}

/// region name containing `area`, from map/region.txt (`areas = { ... }`).
fn region_for_area(vfs: &Vfs, area: &str) -> Option<String> {
    let block = parse_rel(vfs, "map/region.txt")?;
    for (name, b) in block.key_blocks() {
        if let Some(areas) = b.get_block("areas") {
            if areas.bare_scalars().any(|a| a == area) {
                return Some(name.to_string());
            }
        }
    }
    None
}

/// superregion name containing `region`, from map/superregion.txt (bare list).
fn superregion_for_region(vfs: &Vfs, region: &str) -> Option<String> {
    let block = parse_rel(vfs, "map/superregion.txt")?;
    for (name, b) in block.key_blocks() {
        if b.bare_scalars().any(|r| r == region) {
            return Some(name.to_string());
        }
    }
    None
}

/// trade node whose `members` list contains `id` (corridor seas belong to none).
fn trade_node_for(vfs: &Vfs, id: u32) -> Option<String> {
    let block = parse_rel(vfs, "common/tradenodes/00_tradenodes.txt")
        .or_else(|| merged_tradenodes(vfs))?;
    for (name, b) in block.key_blocks() {
        if let Some(members) = b.get_block("members") {
            if members.bare_ids().contains(&id) {
                return Some(name.to_string());
            }
        }
    }
    None
}

/// Merges every common/tradenodes file (mods split the node list across files).
fn merged_tradenodes(vfs: &Vfs) -> Option<Block> {
    let mut merged = Block::default();
    for (name, path) in vfs.list_dir("common/tradenodes") {
        if !name.to_lowercase().ends_with(".txt") {
            continue;
        }
        if let Ok(bytes) = std::fs::read(&path) {
            merged.items.extend(parse_bytes(&bytes).items);
        }
    }
    (!merged.items.is_empty()).then_some(merged)
}

/// terrain type whose `terrain_override` list contains `id`
/// (map/terrain.txt → `categories = { <type> = { terrain_override = { ... } } }`).
fn terrain_override_for(vfs: &Vfs, id: u32) -> Option<String> {
    let block = parse_rel(vfs, "map/terrain.txt")?;
    let categories = block.get_block("categories")?;
    for (ty, b) in categories.key_blocks() {
        if let Some(list) = b.get_block("terrain_override") {
            if list.bare_ids().contains(&id) {
                return Some(ty.to_string());
            }
        }
    }
    None
}

const CLIMATE_ZONES: &[&str] = &["tropical", "arid", "arctic"];
const WINTERS: &[&str] = &["mild_winter", "normal_winter", "severe_winter"];
const MONSOONS: &[&str] = &["mild_monsoon", "normal_monsoon", "severe_monsoon"];

/// water ids (sea + lakes) from map/default.map.
fn is_water(vfs: &Vfs, id: u32) -> bool {
    let Some(block) = parse_rel(vfs, "map/default.map") else {
        return false;
    };
    ["sea_starts", "lakes"].iter().any(|k| {
        block
            .get_block(k)
            .is_some_and(|l| l.bare_ids().contains(&id))
    })
}

fn geography(vfs: &Vfs, loc: &LocStore, id: u32) -> Geography {
    let mut geo = Geography::default();

    if let Some(area) = area_for(vfs, id) {
        if let Some(region) = region_for_area(vfs, &area) {
            if let Some(sr) = superregion_for_region(vfs, &region) {
                geo.superregion = Some(KeyName::new(sr, loc));
            }
            geo.region = Some(KeyName::new(region, loc));
        }
        geo.area = Some(KeyName::new(area, loc));
    }

    geo.trade_node = trade_node_for(vfs, id).map(|n| KeyName::new(n, loc));

    // Climate/winter/impassable/monsoon all live in one inverted map.
    if let Some(climate) = parse_rel(vfs, "map/climate.txt") {
        let by_id = invert_id_lists(&climate);
        if let Some(keys) = by_id.get(&id) {
            geo.climate = keys
                .iter()
                .find(|k| CLIMATE_ZONES.contains(&k.as_str()))
                .map(|k| KeyName::new(k.clone(), loc));
            geo.winter = keys
                .iter()
                .find(|k| WINTERS.contains(&k.as_str()))
                .map(|k| KeyName::new(k.clone(), loc));
            geo.monsoon = keys
                .iter()
                .find(|k| MONSOONS.contains(&k.as_str()))
                .map(|k| KeyName::new(k.clone(), loc));
            geo.impassable = keys.iter().any(|k| k == "impassable");
        }
    }

    if let Some(continent) = parse_rel(vfs, "map/continent.txt") {
        let by_id = invert_id_lists(&continent);
        geo.continent = by_id
            .get(&id)
            .and_then(|ks| ks.first())
            .map(|k| KeyName::new(k.clone(), loc));
    }

    geo.terrain_override = terrain_override_for(vfs, id).map(|t| KeyName::new(t, loc));
    geo.water = is_water(vfs, id);
    geo
}

// --- Top-level entry point --------------------------------------------------

/// Builds the full [`ProvinceDetails`] payload for one province at the effective
/// start date (pre-Sprint-12 signature; used by tests and callers that don't
/// view-at-date). Delegates at 1444.11.11.
#[cfg(test)]
pub fn province_details(vfs: &Vfs, loc: &LocStore, id: u32) -> Result<ProvinceDetails, String> {
    province_details_at(vfs, loc, id, DEFAULT_START)
}

/// Builds the full [`ProvinceDetails`] payload for one province as of `date`.
/// The `effective_1444` snapshot folds every dated block ≤ `date`; a block's
/// `post_start` flag means its date is strictly after `date` (Sprint 12.2).
pub fn province_details_at(
    vfs: &Vfs,
    loc: &LocStore,
    id: u32,
    date: Date,
) -> Result<ProvinceDetails, String> {
    let names = province_names(vfs);
    let (file, bytes, exists) = resolve_history_file(vfs, id, &names);
    let block = parse_bytes(&bytes);

    // Top-level snapshot + raw remainder, in file order.
    let mut top_level = ProvinceStateSnapshot::default();
    let mut raw_remainder = Vec::new();
    for (key, value) in &block.items {
        match key {
            Some(k) => {
                if is_modeled_top_level(k, value) {
                    top_level.apply(k, value);
                } else {
                    raw_remainder.push(raw_statement(key, value));
                }
            }
            None => raw_remainder.push(raw_statement(key, value)),
        }
    }

    // Dated blocks in file order, with per-date occurrence indices.
    let dated_blocks = dated_blocks_of(&block, date);

    // Effective state: top level, then every dated block ≤ the selected date in
    // file order (field kept named `effective_1444` for wire compatibility).
    let mut effective_1444 = top_level.clone();
    for (key, value) in &block.items {
        let (Some(k), Value::Block(b)) = (key, value) else {
            continue;
        };
        let Some(block_date) = parse_date(k) else { continue };
        if block_date > date {
            continue;
        }
        for (ek, ev) in &b.items {
            if let Some(ek) = ek {
                effective_1444.apply(ek, ev);
            }
        }
    }

    let definition_name = names.get(&id).cloned().unwrap_or_default();
    let localized_name = loc.get(&format!("PROV{id}")).map(str::to_string).unwrap_or_else(|| {
        if definition_name.is_empty() {
            format!("Province {id}")
        } else {
            definition_name.clone()
        }
    });

    Ok(ProvinceDetails {
        id,
        file,
        exists,
        localized_name,
        definition_name,
        owner: effective_1444.owner.clone(),
        geography: geography(vfs, loc, id),
        top_level,
        effective_1444,
        raw_remainder,
        dated_blocks,
    })
}

/// Tauri command: full province-details payload (Sprint 2.4). One JSON blob per
/// the module docs. Registered by the orchestrator in `lib.rs`.
#[tauri::command(async)]
pub fn get_province_details(
    install_path: String,
    mod_path: Option<String>,
    id: u32,
    date: Option<String>,
) -> Result<ProvinceDetails, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    let loc = crate::loc::store(&vfs, &install_path, mod_path.as_deref());
    let at = crate::bookmarks::resolve_date(&vfs, date.as_deref())?;
    province_details_at(&vfs, &loc, id, at)
}

// --- Geography edit options (Sprint 2.2 Geography section) ------------------
//
// The province panel's Geography section moves the province between membership
// lists in the shared map/ files. Each option carries the target `file` and the
// `list_path` the id-list edit (AddId/RemoveId/ListMove) addresses, so the
// frontend can emit a byte-surgical splice without re-deriving paths.

/// One selectable membership target (an area, continent, trade node, terrain
/// type, climate zone, or winter severity).
#[derive(Debug, Clone, serde::Serialize)]
pub struct GeoOption {
    pub key: String,
    pub name: String,
    /// Game-relative file the id-list lives in.
    pub file: String,
    /// Path to the id-list inside that file (for AddId/RemoveId/ListMove).
    pub list_path: Vec<String>,
}

/// All geography membership targets, grouped by slot. Climate zone and winter
/// severity are two independent slots (a province can be arid AND severe_winter);
/// "impassable" is a single toggle list carried by `impassable_file`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct GeoOptions {
    pub areas: Vec<GeoOption>,
    pub continents: Vec<GeoOption>,
    pub trade_nodes: Vec<GeoOption>,
    pub terrains: Vec<GeoOption>,
    pub climate_zones: Vec<GeoOption>,
    pub winters: Vec<GeoOption>,
    /// map/climate.txt — the file the `impassable` toggle list lives in.
    pub impassable_file: String,
    /// map/continent.txt — the file continent membership lives in (S3.1). Carried
    /// explicitly so the create-continent flow knows the target even when the
    /// project has no continents yet (a blank-world total conversion).
    pub continent_file: String,
}

/// Top-level `key = { ... }` block names of a map file, in file order.
fn top_level_block_keys(vfs: &Vfs, rel: &str) -> Vec<String> {
    let Some(block) = parse_rel(vfs, rel) else {
        return Vec::new();
    };
    block.key_blocks().map(|(k, _)| k.to_string()).collect()
}

/// Builds the [`GeoOptions`] payload. Areas/continents/terrains/climate come
/// from single canonical map files; trade nodes may be split across files, so
/// each node option records the specific file it lives in (steal semantics need
/// the exact source file for a cross-node move).
pub fn geo_options(vfs: &Vfs, loc: &LocStore) -> GeoOptions {
    let mk = |keys: Vec<String>, file: &str, suffix: Option<&str>| -> Vec<GeoOption> {
        keys.into_iter()
            .map(|k| {
                let mut list_path = vec![k.clone()];
                if let Some(s) = suffix {
                    list_path.push(s.to_string());
                }
                GeoOption {
                    name: loc.resolve(&k),
                    key: k,
                    file: file.to_string(),
                    list_path,
                }
            })
            .collect()
    };

    let areas = mk(top_level_block_keys(vfs, "map/area.txt"), "map/area.txt", None);
    // `island_check_provinces` is an engine helper block, not a continent, so it
    // must never appear as a selectable continent (S3.1).
    let continents = mk(
        top_level_block_keys(vfs, crate::geography::CONTINENT_FILE)
            .into_iter()
            .filter(|k| k != "island_check_provinces")
            .collect(),
        crate::geography::CONTINENT_FILE,
        None,
    );

    // Terrain types: categories.<type>.terrain_override list per type.
    let terrains = parse_rel(vfs, "map/terrain.txt")
        .and_then(|b| b.get_block("categories").map(|c| {
            c.key_blocks()
                .map(|(k, _)| GeoOption {
                    name: loc.resolve(k),
                    key: k.to_string(),
                    file: "map/terrain.txt".to_string(),
                    list_path: vec![
                        "categories".to_string(),
                        k.to_string(),
                        "terrain_override".to_string(),
                    ],
                })
                .collect::<Vec<_>>()
        }))
        .unwrap_or_default();

    // Trade nodes: scan every common/tradenodes file, record each node's own file.
    let mut trade_nodes = Vec::new();
    for (name, path) in vfs.list_dir("common/tradenodes") {
        if !name.to_lowercase().ends_with(".txt") {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else { continue };
        let rel = format!("common/tradenodes/{name}");
        for (k, _) in parse_bytes(&bytes).key_blocks() {
            trade_nodes.push(GeoOption {
                name: loc.resolve(k),
                key: k.to_string(),
                file: rel.clone(),
                list_path: vec![k.to_string(), "members".to_string()],
            });
        }
    }

    let climate_zones = mk(
        CLIMATE_ZONES.iter().map(|s| s.to_string()).collect(),
        "map/climate.txt",
        None,
    );
    let winters = mk(
        WINTERS.iter().map(|s| s.to_string()).collect(),
        "map/climate.txt",
        None,
    );

    GeoOptions {
        areas,
        continents,
        trade_nodes,
        terrains,
        climate_zones,
        winters,
        impassable_file: "map/climate.txt".to_string(),
        continent_file: crate::geography::CONTINENT_FILE.to_string(),
    }
}

/// Tauri command: geography membership options for the province panel's
/// Geography section. **NEW command — needs registration in `lib.rs`.**
#[tauri::command(async)]
pub fn get_geo_options(
    install_path: String,
    mod_path: Option<String>,
) -> Result<GeoOptions, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    let loc = crate::loc::store(&vfs, &install_path, mod_path.as_deref());
    Ok(geo_options(&vfs, &loc))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    const INSTALL: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Europa Universalis IV";
    const ANBENNAR: &str = r"C:\Users\User\projects\anbennar-eu4-dev";

    fn real_install() -> Option<Vfs> {
        Path::new(INSTALL)
            .join("map")
            .join("provinces.bmp")
            .is_file()
            .then(|| Vfs::new(INSTALL, None).unwrap())
    }

    /// A minimal synthetic install with one province history file `content`.
    /// One dir per test (parallel tests must not share a temp dir).
    fn synthetic(name: &str, id: u32, prov_name: &str, content: &str) -> (PathBuf, Vfs) {
        let root = std::env::temp_dir().join(format!("eu_toolkit_provdet_test_{name}"));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("map")).unwrap();
        std::fs::create_dir_all(root.join("history/provinces")).unwrap();
        std::fs::write(root.join("map/provinces.bmp"), b"x").unwrap();
        std::fs::write(
            root.join("map/definition.csv"),
            format!("province;red;green;blue;name;x\n{id};10;20;30;{prov_name};x\n"),
        )
        .unwrap();
        std::fs::write(
            root.join(format!("history/provinces/{id} - {prov_name}.txt")),
            content,
        )
        .unwrap();
        let vfs = Vfs::new(root.to_str().unwrap(), None).unwrap();
        (root, vfs)
    }

    fn loc_empty() -> LocStore {
        LocStore::from_pairs(&[])
    }

    #[test]
    fn top_level_parse_with_remainder_and_buildings() {
        let content = "owner = BYZ\ncontroller = BYZ\nculture = greek\nreligion = orthodox\n\
             capital = \"Constantinople\"\ntrade_goods = glass\nhre = no\n\
             base_tax = 8\nbase_production = 8\nbase_manpower = 4\nis_city = yes\n\
             fort_15th = yes\nadd_core = BYZ\nadd_claim = TUR\n\
             discovered_by = western\ndiscovered_by = ottoman\nextra_cost = 26\ncenter_of_trade = 2\n\
             add_permanent_province_modifier = {\n\tname = bosphorous_sound_toll\n\tduration = -1\n}\n\
             add_province_triggered_modifier = BYZ_galata\n";
        let (_root, vfs) = synthetic("toplevel", 151, "Constantinople", content);
        let d = province_details(&vfs, &loc_empty(), 151).unwrap();

        assert!(d.exists);
        assert_eq!(d.top_level.owner.as_deref(), Some("BYZ"));
        assert_eq!(d.top_level.controller.as_deref(), Some("BYZ"));
        assert_eq!(d.top_level.culture.as_deref(), Some("greek"));
        assert_eq!(d.top_level.religion.as_deref(), Some("orthodox"));
        assert_eq!(d.top_level.capital.as_deref(), Some("Constantinople"));
        assert_eq!(d.top_level.trade_goods.as_deref(), Some("glass"));
        assert_eq!(d.top_level.hre, Some(false));
        assert_eq!(d.top_level.is_city, Some(true));
        assert_eq!(d.top_level.base_tax, Some(8.0));
        assert_eq!(d.top_level.base_production, Some(8.0));
        assert_eq!(d.top_level.base_manpower, Some(4.0));
        assert_eq!(d.top_level.center_of_trade, Some(2));
        assert_eq!(d.top_level.extra_cost, Some(26));
        assert_eq!(d.top_level.cores, vec!["BYZ".to_string()]);
        assert_eq!(d.top_level.claims, vec!["TUR".to_string()]);
        assert_eq!(
            d.top_level.discovered_by,
            vec!["western".to_string(), "ottoman".to_string()]
        );
        // fort_15th is a `= yes` flag → captured under buildings, not remainder.
        assert_eq!(d.top_level.buildings, vec!["fort_15th".to_string()]);
        // is_city/hre are known booleans, NOT buildings.
        assert!(!d.top_level.buildings.iter().any(|b| b == "is_city"));

        // Unmodeled statements preserved for read-only display.
        let rem_keys: Vec<&str> = d.raw_remainder.iter().map(|r| r.key.as_str()).collect();
        assert!(rem_keys.contains(&"add_permanent_province_modifier"));
        assert!(rem_keys.contains(&"add_province_triggered_modifier"));
        let modifier = d
            .raw_remainder
            .iter()
            .find(|r| r.key == "add_permanent_province_modifier")
            .unwrap();
        assert!(modifier.is_block);
        assert!(modifier.value.contains("bosphorous_sound_toll"));
        // No dated blocks in this fixture.
        assert!(d.dated_blocks.is_empty());
    }

    #[test]
    fn models_latent_block_autonomy_and_trade_company() {
        let content = "owner = ENG\nlatent_trade_goods = { coal }\n\
             add_local_autonomy = 50\nunrest = 3\nadd_to_trade_company = ENG\n";
        let (_root, vfs) = synthetic("latent", 42, "Test", content);
        let d = province_details(&vfs, &loc_empty(), 42).unwrap();
        assert_eq!(d.top_level.latent_trade_goods.as_deref(), Some("coal"));
        assert_eq!(d.top_level.local_autonomy, Some(50.0));
        assert_eq!(d.top_level.unrest, Some(3.0));
        assert_eq!(d.top_level.trade_company.as_deref(), Some("ENG"));
        // All modeled → none leak into raw_remainder.
        let rem: Vec<&str> = d.raw_remainder.iter().map(|r| r.key.as_str()).collect();
        assert!(!rem.contains(&"latent_trade_goods"));
        assert!(!rem.contains(&"add_local_autonomy"));
        assert!(!rem.contains(&"add_to_trade_company"));
    }

    #[test]
    fn real_geo_options_populated() {
        let Some(vfs) = real_install() else { return };
        let loc = crate::loc::build(&vfs);
        let g = geo_options(&vfs, &loc);
        assert!(g.areas.iter().any(|a| a.key == "thrace_area"));
        assert!(g.continents.iter().any(|c| c.key == "europe"));
        // The engine helper block is filtered out of the continent options (S3.1).
        assert!(!g.continents.iter().any(|c| c.key == "island_check_provinces"));
        assert_eq!(g.continent_file, "map/continent.txt");
        assert!(g.trade_nodes.len() > 20, "trade nodes: {}", g.trade_nodes.len());
        assert!(g.terrains.iter().any(|t| t.key == "farmlands"));
        assert_eq!(g.climate_zones.len(), 3);
        // Trade-node options carry the exact file + members path.
        let node = &g.trade_nodes[0];
        assert!(node.file.starts_with("common/tradenodes/"));
        assert_eq!(node.list_path.last().map(String::as_str), Some("members"));
    }

    #[test]
    fn dated_blocks_in_order_with_occurrence_index() {
        // Two blocks share 1500.1.1; a third distinct date follows.
        let content = "owner = FRA\n\
             1500.1.1 = { unrest = 5 }\n\
             1500.1.1 = { unrest = 6 }\n\
             1600.1.1 = { base_tax = 10 }\n";
        let (_root, vfs) = synthetic("dated_order", 5, "Test", content);
        let d = province_details(&vfs, &loc_empty(), 5).unwrap();

        assert_eq!(d.dated_blocks.len(), 3);
        assert_eq!(d.dated_blocks[0].date, "1500.1.1");
        assert_eq!(d.dated_blocks[0].occurrence_index, 0);
        assert_eq!(d.dated_blocks[1].date, "1500.1.1");
        assert_eq!(d.dated_blocks[1].occurrence_index, 1);
        assert_eq!(d.dated_blocks[2].date, "1600.1.1");
        assert_eq!(d.dated_blocks[2].occurrence_index, 0);
        // All post-start (after 1444.11.11).
        assert!(d.dated_blocks.iter().all(|b| b.post_start));
        // Entry rows captured.
        assert_eq!(d.dated_blocks[0].entries[0].key, "unrest");
        assert_eq!(d.dated_blocks[0].entries[0].value, "5");
    }

    #[test]
    fn post_start_flag_boundary() {
        let content = "owner = FRA\n\
             1444.11.11 = { religion = catholic }\n\
             1444.11.12 = { religion = orthodox }\n";
        let (_root, vfs) = synthetic("boundary", 6, "Test", content);
        let d = province_details(&vfs, &loc_empty(), 6).unwrap();
        // On the start date = pre-start (applied); one day later = post-start.
        assert!(!d.dated_blocks[0].post_start);
        assert!(d.dated_blocks[1].post_start);
    }

    #[test]
    fn effective_1444_reflects_pre_start_override() {
        // Top level catholic; a pre-start block flips it to protestant, and a
        // post-start block to reformed (must NOT count).
        let content = "owner = FRA\nreligion = catholic\nculture = norman\n\
             1440.1.1 = { religion = protestant add_core = ENG }\n\
             1500.1.1 = { religion = reformed }\n";
        let (_root, vfs) = synthetic("effective", 7, "Test", content);
        let d = province_details(&vfs, &loc_empty(), 7).unwrap();

        // Top level stays as written.
        assert_eq!(d.top_level.religion.as_deref(), Some("catholic"));
        assert!(d.top_level.cores.is_empty());
        // Effective applies only the pre-start override.
        assert_eq!(d.effective_1444.religion.as_deref(), Some("protestant"));
        assert_eq!(d.effective_1444.cores, vec!["ENG".to_string()]);
        // Culture untouched by any dated block.
        assert_eq!(d.effective_1444.culture.as_deref(), Some("norman"));
        // owner convenience mirrors effective.
        assert_eq!(d.owner.as_deref(), Some("FRA"));
    }

    #[test]
    fn effective_remove_core_via_pre_start() {
        let content = "owner = FRA\nadd_core = FRA\nadd_core = ENG\n\
             1440.1.1 = { remove_core = ENG }\n";
        let (_root, vfs) = synthetic("remove_core", 8, "Test", content);
        let d = province_details(&vfs, &loc_empty(), 8).unwrap();
        assert_eq!(d.top_level.cores, vec!["FRA".to_string(), "ENG".to_string()]);
        assert_eq!(d.effective_1444.cores, vec!["FRA".to_string()]);
    }

    #[test]
    fn effective_and_post_start_track_selected_date() {
        // owner FRA top-level; a 1450 block changes it to ENG.
        let content = "owner = FRA\nreligion = catholic\n1450.1.1 = { owner = ENG religion = reformed }\n";
        let (_root, vfs) = synthetic("view_at_date", 9, "Test", content);

        // At the start date the 1450 block is post-start; effective owner is FRA.
        let d0 = province_details_at(&vfs, &loc_empty(), 9, DEFAULT_START).unwrap();
        assert_eq!(d0.effective_1444.owner.as_deref(), Some("FRA"));
        assert_eq!(d0.effective_1444.religion.as_deref(), Some("catholic"));
        assert!(d0.dated_blocks[0].post_start);

        // Viewing at 1453 folds the block in and it is no longer post-start.
        let d1 = province_details_at(&vfs, &loc_empty(), 9, (1453, 1, 1)).unwrap();
        assert_eq!(d1.effective_1444.owner.as_deref(), Some("ENG"));
        assert_eq!(d1.effective_1444.religion.as_deref(), Some("reformed"));
        assert!(!d1.dated_blocks[0].post_start);
        // Top level is always the as-written base state, regardless of date.
        assert_eq!(d1.top_level.owner.as_deref(), Some("FRA"));
    }

    #[test]
    fn missing_history_file_synthesizes_path() {
        let (_root, vfs) = synthetic("missing", 1, "Uppland", "owner = SWE\n");
        // Ask for a province with no history file (id 999 not on disk).
        let d = province_details(&vfs, &loc_empty(), 999).unwrap();
        assert!(!d.exists);
        assert!(d.file.starts_with("history/provinces/999"));
        assert!(d.top_level.owner.is_none());
    }

    #[test]
    fn real_constantinople_details() {
        let Some(vfs) = real_install() else { return };
        let loc = crate::loc::build(&vfs);
        let d = province_details(&vfs, &loc, 151).unwrap();

        // Owner/religion/culture at 1444.
        assert_eq!(d.top_level.owner.as_deref(), Some("BYZ"));
        assert_eq!(d.top_level.religion.as_deref(), Some("orthodox"));
        assert_eq!(d.top_level.culture.as_deref(), Some("greek"));
        assert_eq!(d.top_level.capital.as_deref(), Some("Constantinople"));
        assert_eq!(d.top_level.trade_goods.as_deref(), Some("glass"));
        assert!(d.top_level.discovered_by.iter().any(|x| x == "ottoman"));
        // Localized name resolves via PROV151.
        assert_eq!(d.localized_name, "Constantinople");
        assert_eq!(d.exists, true);

        // The Ottoman conquest (1453.5.29) is POST-start — VERIFIED against the
        // real file: Constantinople has NO pre-start dated blocks, so effective
        // owner is still BYZ (the game's 1444 truth).
        let conquest = d
            .dated_blocks
            .iter()
            .find(|b| b.date == "1453.5.29")
            .expect("1453.5.29 conquest block present");
        assert!(conquest.post_start);
        assert!(d.dated_blocks.iter().all(|b| b.post_start));
        assert_eq!(d.effective_1444.owner.as_deref(), Some("BYZ"));
        assert_eq!(d.effective_1444.religion.as_deref(), Some("orthodox"));

        // Geography joins: Thrace area → Balkan region.
        assert_eq!(d.geography.area.as_ref().map(|k| k.key.as_str()), Some("thrace_area"));
        assert_eq!(
            d.geography.region.as_ref().map(|k| k.key.as_str()),
            Some("balkan_region")
        );
        assert!(d.geography.superregion.is_some());
        assert!(!d.geography.water);
        // Constantinople is a real trade node location.
        assert!(d.geography.trade_node.is_some());
        assert!(d.geography.continent.is_some());
        assert_eq!(
            d.geography.continent.as_ref().map(|k| k.key.as_str()),
            Some("europe")
        );
    }

    #[test]
    fn real_uppland_details() {
        let Some(vfs) = real_install() else { return };
        let loc = crate::loc::build(&vfs);
        let d = province_details(&vfs, &loc, 1).unwrap();
        assert_eq!(d.top_level.owner.as_deref(), Some("SWE"));
        assert_eq!(d.localized_name, "Stockholm"); // PROV1 loc is Stockholm
        assert!(!d.geography.water);
    }

    #[test]
    fn anbennar_smoke() {
        if !Path::new(INSTALL).join("map/provinces.bmp").is_file() || !Path::new(ANBENNAR).is_dir() {
            return;
        }
        let vfs = Vfs::new(INSTALL, Some(ANBENNAR)).unwrap();
        let loc = crate::loc::build(&vfs);
        // Province 1 exists in Anbennar's map too; just prove the payload builds
        // and joins geography without panicking on a total conversion.
        let d = province_details(&vfs, &loc, 1).unwrap();
        assert!(d.file.starts_with("history/provinces/"));
        // Anbennar has its own area tree; area should still resolve for prov 1.
        assert!(d.geography.area.is_some() || !d.exists);
    }
}
