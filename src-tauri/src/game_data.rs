//! Loaders for the static game data the map modes are built from. Everything
//! reads through the Vfs so mod projects overlay the base installation.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

use crate::cache;
use crate::date::{self, Date};
#[cfg(test)]
use crate::date::DEFAULT_START;
use crate::loc::LocStore;
use crate::paradox::{self, Block, Value};
use crate::vfs::Vfs;

fn parse_bytes(bytes: &[u8]) -> Block {
    // Game files are Windows-1252; we only need the ASCII tokens.
    paradox::parse(&String::from_utf8_lossy(bytes))
}

fn parse_path(path: &Path) -> Option<Block> {
    std::fs::read(path).ok().map(|b| parse_bytes(&b))
}

fn parse_rel(vfs: &Vfs, rel: &str) -> Option<Block> {
    vfs.read(rel).ok().map(|b| parse_bytes(&b))
}

/// Parses every .txt in a game directory (merged across mod/base layers) and
/// concatenates the results — the game splits things like country tags
/// across multiple files.
fn parse_dir_merged(vfs: &Vfs, rel_dir: &str) -> Block {
    let mut merged = Block::default();
    for (name, path) in vfs.list_dir(rel_dir) {
        if !name.to_lowercase().ends_with(".txt") {
            continue;
        }
        if let Some(block) = parse_path(&path) {
            merged.items.extend(block.items);
        }
    }
    merged
}

// --- Session caches (see cache.rs) ----------------------------------------
//
// Parsing all ~4k history/provinces files (and the ~1k country files behind
// country_colors) per command call was the app-wide lag root cause: every map
// render, mode-data payload, and trigger snapshot re-did it from scratch.
// The parsed ASTs are memoized per session; the date-folding stays per-call
// (it's a cheap in-memory pass over the cached blocks).

/// One `history/provinces` file, parsed once per session.
pub(crate) struct ProvinceAst {
    pub id: u32,
    /// File name including extension (`1 - Uppland.txt`); the political payload
    /// synthesizes history paths from it.
    pub file_name: String,
    /// `None` when the file failed to read (the name is still recorded,
    /// matching the uncached behavior).
    pub block: Option<Block>,
}

static PROVINCE_ASTS: cache::Store<cache::SessionKey, Vec<ProvinceAst>> = cache::Store::new();
static COUNTRY_COLORS: cache::Store<cache::SessionKey, HashMap<String, [u8; 3]>> =
    cache::Store::new();

/// Every parsed province history file for this session, in `list_dir` order
/// (sorted names — the same order the uncached loops iterated).
pub(crate) fn province_asts(vfs: &Vfs) -> Arc<Vec<ProvinceAst>> {
    PROVINCE_ASTS.get_or_build(cache::session_key(vfs), || {
        let mut out = Vec::new();
        for (name, path) in vfs.list_dir("history/provinces") {
            if !name.to_lowercase().ends_with(".txt") {
                continue;
            }
            let digits: String = name.chars().take_while(|c| c.is_ascii_digit()).collect();
            let Ok(id) = digits.parse::<u32>() else {
                continue;
            };
            out.push(ProvinceAst {
                id,
                file_name: name,
                block: parse_path(&path),
            });
        }
        out
    })
}

/// Drops this module's session caches. Called from `cache::invalidate_all`.
pub(crate) fn invalidate_caches() {
    PROVINCE_ASTS.clear();
    COUNTRY_COLORS.clear();
    COUNTRY_HISTORY_INDEX.clear();
}

/// Country tag -> map color, resolved via common/country_tags -> common/countries.
/// Memoized per session (resolving it walks every country file); the clone out
/// of the cache is ~1k small entries and keeps the by-value signature callers
/// consume with `into_keys`/`into_values`.
pub fn country_colors(vfs: &Vfs) -> HashMap<String, [u8; 3]> {
    COUNTRY_COLORS
        .get_or_build(cache::session_key(vfs), || country_colors_uncached(vfs))
        .as_ref()
        .clone()
}

fn country_colors_uncached(vfs: &Vfs) -> HashMap<String, [u8; 3]> {
    let tags = parse_dir_merged(vfs, "common/country_tags");
    let mut colors = HashMap::new();
    for (key, value) in &tags.items {
        let (Some(tag), Value::Scalar(rel_path)) = (key, value) else {
            continue;
        };
        let Some(country) = parse_rel(vfs, &format!("common/{rel_path}")) else {
            continue;
        };
        if let Some(color) = country.get_block("color").and_then(paradox::color_from_block) {
            colors.insert(tag.clone(), color);
        }
    }
    colors
}

/// Religion name -> color. Religions are nested one level inside religion groups.
pub fn religion_colors(vfs: &Vfs) -> HashMap<String, [u8; 3]> {
    let root = parse_dir_merged(vfs, "common/religions");
    let mut colors = HashMap::new();
    for (_group, group_block) in root.key_blocks() {
        for (name, block) in group_block.key_blocks() {
            if let Some(color) = block.get_block("color").and_then(paradox::color_from_block) {
                colors.insert(name.to_string(), color);
            }
        }
    }
    colors
}

/// Trade good name -> color (defined as 0-1 floats in the game files).
pub fn trade_good_colors(vfs: &Vfs) -> HashMap<String, [u8; 3]> {
    let root = parse_dir_merged(vfs, "common/tradegoods");
    let mut colors = HashMap::new();
    for (name, block) in root.key_blocks() {
        if let Some(color) = block.get_block("color").and_then(paradox::color_from_block) {
            colors.insert(name.to_string(), color);
        }
    }
    colors
}

/// Trade node name -> (color if defined, member province ids).
pub fn trade_nodes(vfs: &Vfs) -> HashMap<String, (Option<[u8; 3]>, Vec<u32>)> {
    let root = parse_dir_merged(vfs, "common/tradenodes");
    let mut nodes = HashMap::new();
    for (name, block) in root.key_blocks() {
        let Some(members) = block.get_block("members") else {
            continue;
        };
        let color = block.get_block("color").and_then(paradox::color_from_block);
        nodes.insert(name.to_string(), (color, members.bare_ids()));
    }
    nodes
}

// --- Unified map-mode data (Phase 0.4) -----------------------------------
//
// One shape drives selection/hover/highlight for every mode. A *categorical*
// mode (political, religion, culture, trade goods/nodes, areas, regions,
// climate, provinces) yields a `groups` list plus a province-id -> group-index
// map; a *gradient* mode (development) yields a per-province numeric value; a
// *raster* mode (terrain/heightmap/province colors) has no group model at all.
//
// The frontend hit-tests a pixel to a province id (via the separate
// `get_province_ids` pixel buffer) and then indexes `values[id]` to find the
// group/value. `0xffff` means "no group / no value" (unowned land, ocean in
// religion mode, temperate land in climate mode) — the same NONE sentinel the
// pixel buffer uses, so a NONE hover never matches NONE pixels.

/// Sentinel written into `values` for a province with no group / no value.
pub const NONE_GROUP: u16 = 0xffff;

/// One selectable group in a categorical mode (a country, religion, area, …).
#[derive(serde::Serialize, Clone, Debug)]
pub struct ModeGroup {
    /// Raw identity key (tag / religion key / area name / province id).
    pub key: String,
    /// Localized display label (falls back to prettified key).
    pub label: String,
    /// Map/swatch color; matches the rendered mode where a color exists,
    /// else a stable hash color.
    pub color: [u8; 3],
}

/// A province whose effective controller differs from its owner (Sprint 13.3
/// occupation): the owner's color comes from the province's `values` group; this
/// carries the controller's stripe color so the client compositor can render the
/// diagonal owner/controller stripes without a backend re-render. Only the
/// political mode populates these.
#[derive(serde::Serialize, Clone, Debug)]
pub struct StripeEntry {
    pub id: u32,
    /// Controller's map color (rebel gray for `controller = REB`).
    pub color: [u8; 3],
}

/// Payload for `get_mode_data`: everything the frontend needs to hover,
/// select, highlight, and (for gradients) read per-province values.
pub struct ModeData {
    /// "categorical" | "gradient" | "raster".
    pub kind: &'static str,
    /// Categorical groups, in stable first-seen order. Empty otherwise.
    pub groups: Vec<ModeGroup>,
    /// Highest province id covered (`values.len() == max_id + 1`). 0 for raster.
    pub max_id: u32,
    /// Province id -> group index (categorical) or scaled value (gradient);
    /// `NONE_GROUP` for provinces outside the mode. Empty for raster.
    pub values: Vec<u16>,
    /// Gradient decode factor: real value = `values[id] / value_scale`
    /// (development stores base_tax+production+manpower total x10). None else.
    pub value_scale: Option<f32>,
    /// Occupation stripes (political mode only; empty otherwise). Additive to the
    /// wire header — older parsers ignore the extra field.
    pub stripes: Vec<StripeEntry>,
}

impl ModeData {
    /// Wire encoding: `[u32 header_len][header JSON][u16 value per province id]`,
    /// all little-endian. The JSON header carries `kind`, `groups`, `maxId`,
    /// and `valueScale`; the trailing u16 array is `values` (omitted for
    /// raster modes, where it is empty).
    pub fn to_wire(&self) -> Vec<u8> {
        #[derive(serde::Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Header<'a> {
            kind: &'a str,
            groups: &'a [ModeGroup],
            max_id: u32,
            value_scale: Option<f32>,
            stripes: &'a [StripeEntry],
        }
        let header = Header {
            kind: self.kind,
            groups: &self.groups,
            max_id: self.max_id,
            value_scale: self.value_scale,
            stripes: &self.stripes,
        };
        // Serializing this small header never fails.
        let json = serde_json::to_vec(&header).unwrap_or_default();
        let mut out = Vec::with_capacity(4 + json.len() + self.values.len() * 2);
        out.extend_from_slice(&(json.len() as u32).to_le_bytes());
        out.extend_from_slice(&json);
        for &v in &self.values {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out
    }
}

/// Interns group keys to stable indices while collecting their metadata.
struct GroupInterner {
    groups: Vec<ModeGroup>,
    index: HashMap<String, u16>,
}

impl GroupInterner {
    fn new() -> Self {
        Self {
            groups: Vec::new(),
            index: HashMap::new(),
        }
    }

    /// Returns the group index for `key`, creating it (with `label`/`color`,
    /// evaluated only on first sight) if new.
    fn intern(
        &mut self,
        key: &str,
        label: impl FnOnce() -> String,
        color: impl FnOnce() -> [u8; 3],
    ) -> u16 {
        if let Some(&i) = self.index.get(key) {
            return i;
        }
        let i = self.groups.len() as u16;
        self.groups.push(ModeGroup {
            key: key.to_string(),
            label: label(),
            color: color(),
        });
        self.index.insert(key.to_string(), i);
        i
    }
}

/// definition.csv rows as `(id, rgb, name)` — the province universe and its
/// raw map colors. Reused for the province-count bound and Provinces mode.
fn province_definitions(vfs: &Vfs) -> Vec<(u32, [u8; 3], String)> {
    let mut out = Vec::new();
    let Ok(bytes) = vfs.read("map/definition.csv") else {
        return out;
    };
    let text = String::from_utf8_lossy(&bytes);
    for line in text.lines() {
        let mut parts = line.split(';');
        let (Some(id), Some(r), Some(g), Some(b)) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        let (Ok(id), Ok(r), Ok(g), Ok(b)) = (
            id.trim().parse::<u32>(),
            r.trim().parse::<u8>(),
            g.trim().parse::<u8>(),
            b.trim().parse::<u8>(),
        ) else {
            continue;
        };
        let name = parts.next().unwrap_or("").trim().to_string();
        out.push((id, [r, g, b], name));
    }
    out
}

/// Builds the unified [`ModeData`] for a map mode. Colors mirror the renderer
/// (`map_renderer::mode_colors`) so highlights line up with what's on screen.
#[cfg(test)]
pub fn mode_data(vfs: &Vfs, loc: &LocStore, mode: &str) -> Result<ModeData, String> {
    mode_data_with_overrides(vfs, loc, mode, &HashMap::new())
}

/// As [`mode_data`], but with per-culture display-color overrides applied in the
/// culture map mode. Cultures carry no color in the game files (Sprint 6.1) — the
/// toolkit hashes one, and users may pin an override stored in the toolkit DB
/// (never in the mod). `culture_overrides` maps culture key -> rgb; absent keys
/// fall back to the hash color. Derives at the effective start date.
#[cfg(test)]
pub fn mode_data_with_overrides(
    vfs: &Vfs,
    loc: &LocStore,
    mode: &str,
    culture_overrides: &HashMap<String, [u8; 3]>,
) -> Result<ModeData, String> {
    mode_data_with_overrides_at(vfs, loc, mode, culture_overrides, DEFAULT_START)
}

/// As [`mode_data_with_overrides`], but the province-derived modes (political,
/// religion, culture, trade goods, development) reflect the state **at `date`**:
/// dated history blocks with a date ≤ `date` are folded in (Sprint 12.2 view-at-
/// date). Geography/climate/area/region modes are date-independent.
pub fn mode_data_with_overrides_at(
    vfs: &Vfs,
    loc: &LocStore,
    mode: &str,
    culture_overrides: &HashMap<String, [u8; 3]>,
    date: Date,
) -> Result<ModeData, String> {
    use crate::map_renderer::hash_color;

    // Raster modes are just images — no group model.
    if matches!(mode, "terrain" | "heightmap" | "province_colors") {
        return Ok(ModeData {
            kind: "raster",
            groups: Vec::new(),
            max_id: 0,
            values: Vec::new(),
            value_scale: None,
            stripes: Vec::new(),
        });
    }

    let defs = province_definitions(vfs);
    let max_id = defs
        .iter()
        .map(|(id, _, _)| *id)
        .max()
        .ok_or("map/definition.csv contained no province definitions")?;
    let mut values = vec![NONE_GROUP; max_id as usize + 1];

    // Development is a gradient: store the dev total x10 so one decimal of
    // precision survives the u16 (real dev = value / 10).
    if mode == "development" {
        for (id, state) in province_history_at(vfs, date) {
            if id > max_id {
                continue;
            }
            if let Some(dev) = state.development {
                values[id as usize] = (dev * 10.0).round().clamp(0.0, 65534.0) as u16;
            }
        }
        return Ok(ModeData {
            kind: "gradient",
            groups: Vec::new(),
            max_id,
            values,
            value_scale: Some(10.0),
            stripes: Vec::new(),
        });
    }

    let mut it = GroupInterner::new();
    // Occupation stripes (populated by the political arm only).
    let mut stripes: Vec<StripeEntry> = Vec::new();
    match mode {
        // Each province is its own group (localized name, raw map color) so
        // Provinces mode gets per-province hover/select for free.
        "provinces" => {
            for (id, color, name) in &defs {
                if *id > max_id {
                    continue;
                }
                let (id, color, name) = (*id, *color, name.clone());
                let idx = it.intern(
                    &id.to_string(),
                    || {
                        loc.get(&format!("PROV{id}"))
                            .map(str::to_string)
                            .unwrap_or_else(|| {
                                if name.is_empty() {
                                    format!("Province {id}")
                                } else {
                                    name
                                }
                            })
                    },
                    || color,
                );
                values[id as usize] = idx;
            }
        }
        "political" => {
            let country = country_colors(vfs);
            for (id, state) in province_history_at(vfs, date) {
                if id > max_id {
                    continue;
                }
                let Some(owner) = state.owner else { continue };
                let idx = it.intern(
                    &owner,
                    || loc.resolve(&owner),
                    || {
                        country
                            .get(&owner)
                            .copied()
                            .unwrap_or_else(|| hash_color(&owner))
                    },
                );
                values[id as usize] = idx;
                // Occupation: controller differs from owner. Carry the
                // controller's color so the client stripes owner/controller.
                if let Some(controller) = state.controller {
                    if controller != owner {
                        let color = if controller == "REB" {
                            crate::map_renderer::REBEL_GRAY
                        } else {
                            country
                                .get(&controller)
                                .copied()
                                .unwrap_or_else(|| hash_color(&controller))
                        };
                        stripes.push(StripeEntry { id, color });
                    }
                }
            }
        }
        "religion" => {
            let religion = religion_colors(vfs);
            for (id, state) in province_history_at(vfs, date) {
                let (Some(r), true) = (state.religion, id <= max_id) else {
                    continue;
                };
                let idx = it.intern(
                    &r,
                    || loc.resolve(&r),
                    || religion.get(&r).copied().unwrap_or_else(|| hash_color(&r)),
                );
                values[id as usize] = idx;
            }
        }
        "culture" => {
            for (id, state) in province_history_at(vfs, date) {
                let (Some(c), true) = (state.culture, id <= max_id) else {
                    continue;
                };
                let idx = it.intern(
                    &c,
                    || loc.resolve(&c),
                    || {
                        culture_overrides
                            .get(&c)
                            .copied()
                            .unwrap_or_else(|| hash_color(&c))
                    },
                );
                values[id as usize] = idx;
            }
        }
        "trade_goods" => {
            let goods = trade_good_colors(vfs);
            // Undiscovered provinces (`trade_goods = unknown`) are split into
            // spawn-distribution clusters (goods_spawn) instead of one global
            // group, so hover/click selects a contiguous same-distribution
            // patch. Every cluster keeps the unknown good's color — the render
            // (map_renderer colors by good) is unchanged; only the selection
            // granularity and label differ. Group keys are `unknown#<n>`; the
            // frontend maps them back to the base good key ("unknown") for
            // list/paint purposes (goodKeyOfGroup).
            let clusters = crate::goods_spawn::undiscovered_clusters(vfs, loc, date);
            let unknown_color = goods
                .get("unknown")
                .copied()
                .unwrap_or_else(|| hash_color("unknown"));
            for (id, state) in province_history_at(vfs, date) {
                let (Some(g), true) = (state.trade_goods, id <= max_id) else {
                    continue;
                };
                let idx = if g == "unknown" {
                    if let Some(&ci) = clusters.index.get(&id) {
                        it.intern(
                            &format!("unknown#{ci}"),
                            || format!("{} — {}", loc.resolve("unknown"), clusters.summaries[ci]),
                            || unknown_color,
                        )
                    } else {
                        it.intern(&g, || loc.resolve(&g), || unknown_color)
                    }
                } else {
                    it.intern(
                        &g,
                        || loc.resolve(&g),
                        || goods.get(&g).copied().unwrap_or_else(|| hash_color(&g)),
                    )
                };
                values[id as usize] = idx;
            }
        }
        "trade_nodes" => {
            for (name, (color, ids)) in trade_nodes(vfs) {
                let idx = it.intern(
                    &name,
                    || loc.resolve(&name),
                    || color.unwrap_or_else(|| hash_color(&name)),
                );
                for id in ids {
                    if id <= max_id {
                        values[id as usize] = idx;
                    }
                }
            }
        }
        "areas" => {
            for (name, ids) in areas(vfs) {
                let idx = it.intern(&name, || loc.resolve(&name), || hash_color(&name));
                for id in ids {
                    if id <= max_id {
                        values[id as usize] = idx;
                    }
                }
            }
        }
        "regions" => {
            for (name, ids) in regions(vfs) {
                let idx = it.intern(&name, || loc.resolve(&name), || hash_color(&name));
                for id in ids {
                    if id <= max_id {
                        values[id as usize] = idx;
                    }
                }
            }
        }
        // Colonial regions / trade companies (Sprint 19): each entry is a group
        // colored by its explicit `color` (matching the renderer). Unassigned
        // provinces stay groupless (neutral land — legal for most of the map).
        "colonial_regions" | "trade_companies" => {
            for (name, color, ids) in crate::colonial::membership(vfs, mode) {
                let idx = it.intern(&name, || loc.resolve(&name), || color);
                for id in ids {
                    if id <= max_id {
                        values[id as usize] = idx;
                    }
                }
            }
        }
        // The zone slot only (tropical/arid/arctic + impassable); winter lists
        // and unlisted (temperate) land are groupless, exactly as they render.
        "climate" => {
            for (id, zone) in climate_slot(vfs, CLIMATE_ZONE_KEYS) {
                if id > max_id {
                    continue;
                }
                let color = match zone.as_str() {
                    "tropical" => [64, 142, 63],
                    "arid" => [216, 196, 120],
                    "arctic" => [235, 235, 238],
                    "impassable" => [80, 80, 80],
                    _ => continue,
                };
                let idx = it.intern(&zone, || loc.resolve(&zone), || color);
                values[id as usize] = idx;
            }
        }
        // Winter-severity slot (Sprint 11.1): mild/normal/severe groups; land
        // with no winter stays groupless (the frontend's "none" eraser).
        "winter" => {
            for (id, key) in climate_slot(vfs, WINTER_KEYS) {
                if id > max_id {
                    continue;
                }
                let color = match key.as_str() {
                    "mild_winter" => [176, 206, 224],
                    "normal_winter" => [116, 158, 204],
                    "severe_winter" => [72, 92, 148],
                    _ => continue,
                };
                let idx = it.intern(&key, || loc.resolve(&key), || color);
                values[id as usize] = idx;
            }
        }
        // Effective gameplay terrain (Sprint 11.2): terrain_override else the
        // dominant terrain.bmp palette class. Sea is groupless (renders as sea).
        "simple_terrain" => {
            use crate::map_renderer::terrain_color;
            let eff = crate::map_renderer::effective_terrain(vfs)?;
            for (id, (cat, _is_override)) in &eff.by_province {
                if *id > max_id || eff.water.contains(id) {
                    continue;
                }
                let idx = it.intern(cat, || loc.resolve(cat), || terrain_color(cat));
                values[*id as usize] = idx;
            }
        }
        other => return Err(format!("Unknown map mode: {other}")),
    }

    Ok(ModeData {
        kind: "categorical",
        groups: it.groups,
        max_id,
        values,
        value_scale: None,
        stripes,
    })
}

/// A leader sub-block (`leader = { ... }`) inside a monarch/heir block. Note the
/// game's own misspelling "manuever" — it is the real file key.
#[derive(Debug, serde::Serialize)]
pub struct LeaderInfo {
    pub fire: Option<i32>,
    pub shock: Option<i32>,
    pub manuever: Option<i32>,
    pub siege: Option<i32>,
}

/// One `add_ruler_personality` / `add_heir_personality` / `add_queen_personality`
/// effect. `date` is the dated block it lives in, so removal can address it.
#[derive(Debug, serde::Serialize)]
pub struct Personality {
    pub key: String,
    pub date: String,
}

/// The monarch at game start. `date` is the dated block key the `monarch` block
/// was found under — the frontend addresses ruler edits at `[date, "monarch", …]`.
#[derive(Debug, serde::Serialize)]
pub struct RulerInfo {
    pub date: String,
    pub name: Option<String>,
    pub dynasty: Option<String>,
    pub adm: Option<i32>,
    pub dip: Option<i32>,
    pub mil: Option<i32>,
    pub birth_date: Option<String>,
    pub female: bool,
    pub regent: bool,
    pub culture: Option<String>,
    pub religion: Option<String>,
    pub personalities: Vec<Personality>,
    pub leader: Option<LeaderInfo>,
}

/// The queen/consort at game start (may not exist). Same fields as the monarch
/// plus `country_of_origin` and `death_date`.
#[derive(Debug, serde::Serialize)]
pub struct QueenInfo {
    pub date: String,
    pub name: Option<String>,
    pub dynasty: Option<String>,
    pub adm: Option<i32>,
    pub dip: Option<i32>,
    pub mil: Option<i32>,
    pub birth_date: Option<String>,
    pub death_date: Option<String>,
    pub female: bool,
    pub regent: bool,
    pub country_of_origin: Option<String>,
    pub culture: Option<String>,
    pub religion: Option<String>,
    pub personalities: Vec<Personality>,
    pub leader: Option<LeaderInfo>,
}

/// The heir at game start (may not exist).
#[derive(Debug, serde::Serialize)]
pub struct HeirInfo {
    pub date: String,
    pub name: Option<String>,
    pub monarch_name: Option<String>,
    pub dynasty: Option<String>,
    pub adm: Option<i32>,
    pub dip: Option<i32>,
    pub mil: Option<i32>,
    pub birth_date: Option<String>,
    pub death_date: Option<String>,
    pub claim: Option<i32>,
    pub female: bool,
    pub culture: Option<String>,
    pub religion: Option<String>,
    pub personalities: Vec<Personality>,
    pub leader: Option<LeaderInfo>,
}

/// One monarch-name pool entry: `"Name #N" = weight`. `name` keeps its raw quoted
/// token form (Latin-1 decoded) so byte-surgical removal matches the file token.
#[derive(Debug, serde::Serialize)]
pub struct MonarchName {
    pub name: String,
    pub weight: String,
}

/// The country's name pools (from its `common/countries/<file>`). Each list holds
/// raw element tokens (Latin-1 decoded, quotes preserved) so edits round-trip.
#[derive(Debug, serde::Serialize)]
pub struct NamePools {
    pub monarch_names: Vec<MonarchName>,
    pub leader_names: Vec<String>,
    pub ship_names: Vec<String>,
    pub army_names: Vec<String>,
    pub fleet_names: Vec<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct IdeaEffect {
    pub key: String,
    pub value: String,
}

#[derive(Debug, serde::Serialize)]
pub struct NationalIdea {
    /// Raw idea key (identity).
    pub name: String,
    /// Localized idea name (`<key>:` loc), falling back to prettified key.
    pub localized_name: String,
    /// Localized idea description (`<key>_desc:` loc), empty if none.
    pub localized_desc: String,
    pub effects: Vec<IdeaEffect>,
}

#[derive(Debug, serde::Serialize)]
pub struct NationalIdeas {
    /// Raw idea-set key, e.g. `SWE_ideas` (identity).
    pub name: String,
    /// Localized idea-set name (`<key>:` loc), falling back to prettified key.
    pub localized_name: String,
    /// Localized traditions name (`<set>_start:` loc).
    pub traditions_name: String,
    /// Localized ambition name (`<set>_bonus:` loc).
    pub ambition_name: String,
    /// Game-relative file the idea-set block lives in (for byte-surgical edits).
    pub source_file: String,
    pub traditions: Vec<IdeaEffect>,
    pub ideas: Vec<NationalIdea>,
    pub ambition: Vec<IdeaEffect>,
}

#[derive(Debug, serde::Serialize)]
pub struct CountryDetails {
    pub tag: String,
    /// Country file stem (identity-ish; kept for reference).
    pub name: String,
    /// Localized country name (`TAG:` loc; falls back to the file stem).
    pub localized_name: String,
    /// Localized country adjective (`TAG_ADJ:` loc), if defined.
    pub adjective: Option<String>,
    pub color: Option<[u8; 3]>,
    /// Game-relative path of the country's `common/countries/<file>` (where
    /// color / revolutionary_colors / graphical_culture live). None if the tag
    /// has no country file.
    pub country_file: Option<String>,
    /// Game-relative path of the country's `history/countries/<TAG - Name>.txt`
    /// (government, religion, culture, reforms, capital, …). None if absent.
    pub history_file: Option<String>,
    /// Revolutionary colors — three **palette indices** (NOT 0-255 RGB), each
    /// referencing the game's flag color palette. None if the key is absent.
    pub revolutionary_colors: Option<[i64; 3]>,
    /// Graphical culture key (from the country file), if present.
    pub graphical_culture: Option<String>,
    /// Raw government key (identity).
    pub government: Option<String>,
    /// Localized government name.
    pub government_name: Option<String>,
    /// Government rank 1/2/3 (Duchy/Kingdom/Empire), if present.
    pub government_rank: Option<i64>,
    pub religion: Option<String>,
    pub religion_name: Option<String>,
    pub primary_culture: Option<String>,
    pub primary_culture_name: Option<String>,
    pub technology_group: Option<String>,
    pub technology_group_name: Option<String>,
    /// Unit type key (usually mirrors the tech group), if present.
    pub unit_type: Option<String>,
    /// National focus: ADM / DIP / MIL (key absent = none).
    pub national_focus: Option<String>,
    /// Mercantilism value, if present.
    pub mercantilism: Option<f64>,
    /// `elector = yes` present (HRE elector).
    pub elector: bool,
    /// `add_government_reform` values in file order.
    pub government_reforms: Vec<String>,
    /// `add_accepted_culture` values in file order.
    pub accepted_cultures: Vec<String>,
    /// `historical_rival` tags in file order.
    pub historical_rivals: Vec<String>,
    /// `historical_friend` tags in file order.
    pub historical_friends: Vec<String>,
    pub capital: Option<u32>,
    /// Localized capital province name (`PROV<id>:` loc; falls back to
    /// definition.csv's name column).
    pub capital_name: Option<String>,
    pub ruler: Option<RulerInfo>,
    /// When `ruler` is None, a human explanation (PU junior / no monarch defined).
    pub ruler_reason: Option<String>,
    pub queen: Option<QueenInfo>,
    pub heir: Option<HeirInfo>,
    /// Name pools from the `common/countries/<file>` (monarch/leader/ship/army/fleet).
    pub name_pools: NamePools,
    /// `historical_idea_groups` list (ordered) from the country file.
    pub historical_idea_groups: Vec<String>,
    /// `historical_units` list (ordered) from the country file.
    pub historical_units: Vec<String>,
    pub ideas: Option<NationalIdeas>,
    /// Every dated `Y.M.D = { ... }` block in the history file, in file order
    /// (S3.2 country history timeline). `post_start` on each is relative to the
    /// selected date, so blocks strictly later than the view date are badged and
    /// excluded from the effective state above.
    pub dated_blocks: Vec<crate::province_details::DatedBlock>,
}

fn validate_tag(tag: &str) -> Result<(), String> {
    if tag.len() == 3 && tag.chars().all(|c| c.is_ascii_alphanumeric()) {
        Ok(())
    } else {
        Err(format!("Invalid country tag: {tag}"))
    }
}

/// Game-relative path of the country's common/countries file, via country_tags.
fn country_file_rel(vfs: &Vfs, tag: &str) -> Option<String> {
    let tags = parse_dir_merged(vfs, "common/country_tags");
    let rel = tags.get_scalar(tag)?;
    Some(format!("common/{rel}"))
}

/// Tag -> (file name, resolved path) for every `history/countries` file, from
/// ONE directory pass, memoized per session. Enumerating the directory per
/// lookup was the mission-board lag root cause: the trigger snapshot calls
/// [`country_history_file`] for ~1k tags, which was ~1k full enumerations
/// (35s on vanilla). First matching file per tag wins, preserving the old
/// first-in-sorted-order semantics.
static COUNTRY_HISTORY_INDEX: cache::Store<
    cache::SessionKey,
    HashMap<String, (String, std::path::PathBuf)>,
> = cache::Store::new();

fn country_history_index(vfs: &Vfs) -> Arc<HashMap<String, (String, std::path::PathBuf)>> {
    COUNTRY_HISTORY_INDEX.get_or_build(cache::session_key(vfs), || {
        let mut out: HashMap<String, (String, std::path::PathBuf)> = HashMap::new();
        for (name, path) in vfs.list_dir("history/countries") {
            let Some(stem) = name.get(..3) else { continue };
            if name.len() > 3
                && !name.as_bytes()[3].is_ascii_alphanumeric()
                && name.to_lowercase().ends_with(".txt")
            {
                out.entry(stem.to_ascii_uppercase())
                    .or_insert_with(|| (name.clone(), path.clone()));
            }
        }
        out
    })
}

/// The country's history file ("SWE - Sweden.txt"): (file name, bytes). The
/// tag→file mapping is served from the session index; the BYTES are read fresh
/// per call (the edit writer resolves through this, so content must never be
/// stale).
pub fn country_history_file(vfs: &Vfs, tag: &str) -> Option<(String, Vec<u8>)> {
    let idx = country_history_index(vfs);
    let (name, path) = idx.get(&tag.to_ascii_uppercase())?;
    std::fs::read(path).ok().map(|bytes| (name.clone(), bytes))
}

use date::parse_date;

/// Dated blocks on or before `at`, in file order (key string + block).
fn dated_blocks_le(history: &Block, at: Date) -> Vec<(&str, &Block)> {
    history
        .items
        .iter()
        .filter_map(|(key, value)| {
            let (Some(key), Value::Block(block)) = (key, value) else {
                return None;
            };
            let date = parse_date(key)?;
            (date <= at).then_some((key.as_str(), block))
        })
        .collect()
}

/// The latest dated block on or before start containing a `<holder>` sub-block
/// (monarch / heir / queen), taking the last in file order among equal dates.
fn latest_holder<'a>(dated: &[(&'a str, &'a Block)], holder: &str) -> Option<(&'a str, &'a Block)> {
    let mut best: Option<((u32, u32, u32), &str, &Block)> = None;
    for (key, block) in dated {
        if let Some(h) = block.get_block(holder) {
            let date = parse_date(key).unwrap_or((0, 0, 0));
            if best.map_or(true, |(d, _, _)| date >= d) {
                best = Some((date, key, h));
            }
        }
    }
    best.map(|(_, key, block)| (key, block))
}

/// Accumulated `<effect>` values across dated blocks on/before start, honoring
/// `clear_scripted_personalities = yes` resets. Each keeps the date it lives in.
fn collect_personalities(dated: &[(&str, &Block)], effect: &str) -> Vec<Personality> {
    let mut out: Vec<Personality> = Vec::new();
    for (key, block) in dated {
        if block.get_scalar("clear_scripted_personalities") == Some("yes") {
            out.clear();
        }
        for (k, v) in &block.items {
            if let (Some(k), Value::Scalar(s)) = (k, v) {
                if k == effect {
                    out.push(Personality {
                        key: s.clone(),
                        date: key.to_string(),
                    });
                }
            }
        }
    }
    out
}

fn stat_of(block: &Block, key: &str) -> Option<i32> {
    block.get_scalar(key).and_then(|s| s.parse().ok())
}

/// A `leader = { ... }` sub-block of a character block, if present.
fn leader_of(block: &Block) -> Option<LeaderInfo> {
    let l = block.get_block("leader")?;
    Some(LeaderInfo {
        fire: stat_of(l, "fire"),
        shock: stat_of(l, "shock"),
        manuever: stat_of(l, "manuever"),
        siege: stat_of(l, "siege"),
    })
}

/// The ruler (monarch), queen, and heir at the effective start date. Kept for
/// the pre-Sprint-12 callers/tests; delegates to [`characters_at`] at 1444.11.11.
#[cfg(test)]
fn characters_at_start(
    history: &Block,
) -> (Option<RulerInfo>, Option<String>, Option<QueenInfo>, Option<HeirInfo>) {
    characters_at(history, DEFAULT_START)
}

/// The ruler (monarch), queen, and heir at `at`, plus a reason string when no
/// ruler exists. Each holder is the latest one dated on/before `at`.
fn characters_at(
    history: &Block,
    at: Date,
) -> (Option<RulerInfo>, Option<String>, Option<QueenInfo>, Option<HeirInfo>) {
    let dated = dated_blocks_le(history, at);

    let ruler = latest_holder(&dated, "monarch").map(|(date, m)| RulerInfo {
        date: date.to_string(),
        name: m.get_scalar("name").map(str::to_string),
        dynasty: m.get_scalar("dynasty").map(str::to_string),
        adm: stat_of(m, "adm"),
        dip: stat_of(m, "dip"),
        mil: stat_of(m, "mil"),
        birth_date: m.get_scalar("birth_date").map(str::to_string),
        female: m.get_scalar("female") == Some("yes"),
        regent: m.get_scalar("regent") == Some("yes"),
        culture: m.get_scalar("culture").map(str::to_string),
        religion: m.get_scalar("religion").map(str::to_string),
        personalities: collect_personalities(&dated, "add_ruler_personality"),
        leader: leader_of(m),
    });

    let queen = latest_holder(&dated, "queen").map(|(date, q)| QueenInfo {
        date: date.to_string(),
        name: q.get_scalar("name").map(str::to_string),
        dynasty: q.get_scalar("dynasty").map(str::to_string),
        adm: stat_of(q, "adm"),
        dip: stat_of(q, "dip"),
        mil: stat_of(q, "mil"),
        birth_date: q.get_scalar("birth_date").map(str::to_string),
        death_date: q.get_scalar("death_date").map(str::to_string),
        female: q.get_scalar("female") == Some("yes"),
        regent: q.get_scalar("regent") == Some("yes"),
        country_of_origin: q.get_scalar("country_of_origin").map(str::to_string),
        culture: q.get_scalar("culture").map(str::to_string),
        religion: q.get_scalar("religion").map(str::to_string),
        personalities: collect_personalities(&dated, "add_queen_personality"),
        leader: leader_of(q),
    });

    let heir = latest_holder(&dated, "heir").map(|(date, h)| HeirInfo {
        date: date.to_string(),
        name: h.get_scalar("name").map(str::to_string),
        monarch_name: h.get_scalar("monarch_name").map(str::to_string),
        dynasty: h.get_scalar("dynasty").map(str::to_string),
        adm: stat_of(h, "adm"),
        dip: stat_of(h, "dip"),
        mil: stat_of(h, "mil"),
        birth_date: h.get_scalar("birth_date").map(str::to_string),
        death_date: h.get_scalar("death_date").map(str::to_string),
        claim: stat_of(h, "claim"),
        female: h.get_scalar("female") == Some("yes"),
        culture: h.get_scalar("culture").map(str::to_string),
        religion: h.get_scalar("religion").map(str::to_string),
        personalities: collect_personalities(&dated, "add_heir_personality"),
        leader: leader_of(h),
    });

    // Reason when no starting ruler: distinguish "defined only after start"
    // (PU junior like Sweden) from "no monarch anywhere".
    let reason = if ruler.is_some() {
        None
    } else if history
        .items
        .iter()
        .any(|(k, v)| matches!((k, v), (Some(_), Value::Block(b)) if b.get_block("monarch").is_some()))
    {
        Some(format!(
            "No monarch is defined at {} — the earliest ruler is dated after the selected date \
             (e.g. a junior partner in a personal union). Create a starting ruler to override.",
            date::format_date(at)
        ))
    } else {
        Some("No ruler is defined for this country.".to_string())
    };

    (ruler, reason, queen, heir)
}

/// Raw element tokens of `key = { ... }` in `src`, decoded Latin-1 (Windows-1252)
/// with quotes preserved, so byte-surgical add/remove matches the file tokens.
/// Comments are skipped. Returns None when the block is absent. `=` tokens are
/// kept (monarch_names needs them) — callers filter as needed.
fn raw_block_tokens(src: &[u8], key: &str) -> Option<Vec<String>> {
    // Find the top-level `key` symbol followed by `=` `{`.
    let kb = key.as_bytes();
    let mut i = 0;
    let n = src.len();
    let latin1 = |s: usize, e: usize| src[s..e].iter().map(|&b| b as char).collect::<String>();
    while i < n {
        match src[i] {
            b'#' => {
                while i < n && src[i] != b'\n' {
                    i += 1;
                }
            }
            b'"' => {
                i += 1;
                while i < n && src[i] != b'"' {
                    i += 1;
                }
                i += 1;
            }
            b'{' | b'}' | b'=' => i += 1,
            c if c.is_ascii_whitespace() => i += 1,
            _ => {
                let start = i;
                while i < n
                    && !src[i].is_ascii_whitespace()
                    && !matches!(src[i], b'{' | b'}' | b'=' | b'#' | b'"')
                {
                    i += 1;
                }
                if &src[start..i] == kb {
                    // Expect `=` then `{`.
                    let mut j = i;
                    let skip_ws_comments = |mut p: usize| {
                        while p < n {
                            if src[p] == b'#' {
                                while p < n && src[p] != b'\n' {
                                    p += 1;
                                }
                            } else if src[p].is_ascii_whitespace() {
                                p += 1;
                            } else {
                                break;
                            }
                        }
                        p
                    };
                    j = skip_ws_comments(j);
                    if j < n && src[j] == b'=' {
                        j = skip_ws_comments(j + 1);
                        if j < n && src[j] == b'{' {
                            return Some(collect_tokens_until_close(src, j + 1, latin1));
                        }
                    }
                }
            }
        }
    }
    None
}

/// Collects tokens (quoted keep quotes, plus bare `=`) from `start` until the
/// matching close brace at depth 0. Assumes name-pool blocks (no nested braces).
fn collect_tokens_until_close(
    src: &[u8],
    start: usize,
    latin1: impl Fn(usize, usize) -> String,
) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = start;
    let n = src.len();
    while i < n {
        match src[i] {
            b'}' => break,
            b'#' => {
                while i < n && src[i] != b'\n' {
                    i += 1;
                }
            }
            b'=' => {
                out.push("=".to_string());
                i += 1;
            }
            b'"' => {
                let s = i;
                i += 1;
                while i < n && src[i] != b'"' {
                    i += 1;
                }
                i = (i + 1).min(n);
                out.push(latin1(s, i)); // includes quotes
            }
            c if c.is_ascii_whitespace() => i += 1,
            _ => {
                let s = i;
                while i < n
                    && !src[i].is_ascii_whitespace()
                    && !matches!(src[i], b'{' | b'}' | b'=' | b'#' | b'"')
                {
                    i += 1;
                }
                out.push(latin1(s, i));
            }
        }
    }
    out
}

/// A bare-token name pool (leader/ship/army/fleet names) as raw element strings.
fn raw_name_list(src: &[u8], key: &str) -> Vec<String> {
    raw_block_tokens(src, key)
        .map(|toks| toks.into_iter().filter(|t| t != "=").collect())
        .unwrap_or_default()
}

/// The `monarch_names` pool: `"Name #N" = weight` entries in file order.
fn raw_monarch_names(src: &[u8]) -> Vec<MonarchName> {
    let Some(toks) = raw_block_tokens(src, "monarch_names") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut i = 0;
    while i < toks.len() {
        // Expect: name  "="  weight
        if i + 2 < toks.len() && toks[i + 1] == "=" {
            out.push(MonarchName {
                name: toks[i].clone(),
                weight: toks[i + 2].clone(),
            });
            i += 3;
        } else {
            i += 1;
        }
    }
    out
}

fn name_pools(src: &[u8]) -> NamePools {
    NamePools {
        monarch_names: raw_monarch_names(src),
        leader_names: raw_name_list(src, "leader_names"),
        ship_names: raw_name_list(src, "ship_names"),
        army_names: raw_name_list(src, "army_names"),
        fleet_names: raw_name_list(src, "fleet_names"),
    }
}

/// A pickable idea group (`aristocracy_ideas`, `offensive_ideas`, …) for the
/// historical-idea-groups list: an idea-set block carrying `category = ADM/DIP/MIL`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct IdeaGroupEntry {
    pub key: String,
    pub name: String,
    pub category: String,
}

/// All player-pickable idea groups (the 8-idea groups with a `category`), in file
/// order, deduped by key (mod files may re-list). For the historical setup picker.
pub fn idea_group_list(vfs: &Vfs, loc: &LocStore) -> Vec<IdeaGroupEntry> {
    let root = parse_dir_merged(vfs, "common/ideas");
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for (key, block) in root.key_blocks() {
        let Some(cat) = block.get_scalar("category") else {
            continue;
        };
        if !matches!(cat, "ADM" | "DIP" | "MIL") {
            continue;
        }
        if seen.insert(key.to_string()) {
            out.push(IdeaGroupEntry {
                key: key.to_string(),
                name: loc.resolve(key),
                category: cat.to_string(),
            });
        }
    }
    out
}

/// Unit key list (file stems of `common/units`), sorted + deduped. For the
/// historical-units picker (kept simple: all unit keys).
pub fn unit_list(vfs: &Vfs) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut seen = HashSet::new();
    for (name, _) in vfs.list_dir("common/units") {
        let stem = Path::new(&name)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned());
        if let Some(stem) = stem {
            if name.to_lowercase().ends_with(".txt") && seen.insert(stem.clone()) {
                out.push(stem);
            }
        }
    }
    out.sort();
    out
}

/// True if a trigger block (or anything nested in it, e.g. `OR = { ... }`)
/// contains `tag = <tag>`.
fn trigger_matches_tag(block: &Block, tag: &str) -> bool {
    for (key, value) in &block.items {
        match (key, value) {
            (Some(k), Value::Scalar(s)) if k.eq_ignore_ascii_case("tag") && s == tag => {
                return true;
            }
            (_, Value::Block(inner)) => {
                if trigger_matches_tag(inner, tag) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

pub fn national_ideas(vfs: &Vfs, loc: &LocStore, tag: &str) -> Option<NationalIdeas> {
    const META_KEYS: &[&str] = &["start", "bonus", "trigger", "free", "important", "category"];

    let scalar_effects = |block: &Block| -> Vec<IdeaEffect> {
        block
            .items
            .iter()
            .filter_map(|(k, v)| match (k, v) {
                (Some(k), Value::Scalar(s)) => Some(IdeaEffect {
                    key: k.clone(),
                    value: s.clone(),
                }),
                _ => None,
            })
            .collect()
    };

    let exact_key = format!("{tag}_ideas");
    // Scan each file individually so we can report the file the set lives in
    // (edits are byte-surgical, copy-on-write into that specific file).
    let mut fallback: Option<(String, String, Block)> = None;
    let mut chosen: Option<(String, String, Block)> = None;
    for (fname, path) in vfs.list_dir("common/ideas") {
        if !fname.to_lowercase().ends_with(".txt") {
            continue;
        }
        let Some(root) = parse_path(&path) else { continue };
        let rel = format!("common/ideas/{fname}");
        for (name, block) in root.key_blocks() {
            if name.eq_ignore_ascii_case(&exact_key) {
                chosen = Some((rel.clone(), name.to_string(), block.clone()));
                break;
            }
            if fallback.is_none()
                && block
                    .get_block("trigger")
                    .is_some_and(|t| trigger_matches_tag(t, tag))
            {
                fallback = Some((rel.clone(), name.to_string(), block.clone()));
            }
        }
        if chosen.is_some() {
            break;
        }
    }

    let (source_file, name, block) = chosen.or(fallback)?;
    Some(NationalIdeas {
        localized_name: loc.resolve(&name),
        traditions_name: loc.resolve(&format!("{name}_start")),
        ambition_name: loc.resolve(&format!("{name}_bonus")),
        source_file,
        traditions: block.get_block("start").map(&scalar_effects).unwrap_or_default(),
        ambition: block.get_block("bonus").map(&scalar_effects).unwrap_or_default(),
        ideas: block
            .key_blocks()
            .filter(|(k, _)| !META_KEYS.contains(k))
            .map(|(k, b)| NationalIdea {
                name: k.to_string(),
                localized_name: loc.resolve(k),
                localized_desc: loc.get(&format!("{k}_desc")).unwrap_or("").to_string(),
                effects: scalar_effects(b),
            })
            .collect(),
        name,
    })
}

/// Province id -> display name, from definition.csv's name column.
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
            let name = name.trim();
            if !name.is_empty() {
                names.insert(id, name.to_string());
            }
        }
    }
    names
}

/// Country details at the effective start date (pre-Sprint-12 signature; used by
/// tests and callers that don't view-at-date). Delegates at 1444.11.11.
#[cfg(test)]
pub fn country_details(vfs: &Vfs, loc: &LocStore, tag: &str) -> Result<CountryDetails, String> {
    country_details_at(vfs, loc, tag, DEFAULT_START)
}

/// Country details as of `date`: the ruler/queen/heir are the latest defined on
/// or before `date` (the rest of the payload is the file's base state).
pub fn country_details_at(
    vfs: &Vfs,
    loc: &LocStore,
    tag: &str,
    date: Date,
) -> Result<CountryDetails, String> {
    validate_tag(tag)?;

    let file_rel = country_file_rel(vfs, tag);
    let name = file_rel
        .as_deref()
        .map(Path::new)
        .and_then(Path::file_stem)
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| tag.to_string());
    let country_bytes = file_rel.as_deref().and_then(|rel| vfs.read(rel).ok());
    let country_block = country_bytes.as_deref().map(parse_bytes);
    let color = country_block
        .as_ref()
        .and_then(|b| b.get_block("color").and_then(paradox::color_from_block));
    let revolutionary_colors = country_block
        .as_ref()
        .and_then(|b| b.get_block("revolutionary_colors"))
        .and_then(three_ints);
    let graphical_culture = country_block
        .as_ref()
        .and_then(|b| b.get_scalar("graphical_culture"))
        .map(str::to_string);

    let hist = country_history_file(vfs, tag);
    let history_file = hist.as_ref().map(|(name, _)| format!("history/countries/{name}"));
    let history = hist.map(|(_, bytes)| parse_bytes(&bytes));
    let get = |key: &str| {
        history
            .as_ref()
            .and_then(|h| h.get_scalar(key))
            .map(str::to_string)
    };
    // All top-level scalar values for a repeated key (add_government_reform, …),
    // in file order. Dated blocks are skipped (top-level only = 1444 state).
    let list_values = |key: &str| -> Vec<String> {
        history
            .as_ref()
            .map(|h| {
                h.items
                    .iter()
                    .filter_map(|(k, v)| match (k, v) {
                        (Some(k), Value::Scalar(s)) if k == key => Some(s.clone()),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    // Base (top-level) modeled scalars + list accumulators. These are then folded
    // forward through the history's dated blocks with a date ≤ the selected date
    // (S3.2 effective-at-date), so viewing a country after a `1515.1.1 = {
    // government = monarchy }` block shows that government — the panel's effective
    // state re-derives exactly the way the game applies dated history in order.
    let mut capital = get("capital").and_then(|s| s.parse::<u32>().ok());
    let mut government = get("government");
    let mut religion = get("religion");
    let mut primary_culture = get("primary_culture");
    let mut technology_group = get("technology_group");
    let mut unit_type = get("unit_type");
    let mut national_focus = get("national_focus");
    let mut government_rank = get("government_rank").and_then(|s| s.parse::<i64>().ok());
    let mut mercantilism = get("mercantilism").and_then(|s| s.parse::<f64>().ok());
    let mut elector = get("elector").as_deref() == Some("yes");
    let mut government_reforms = list_values("add_government_reform");
    let mut accepted_cultures = list_values("add_accepted_culture");

    if let Some(h) = history.as_ref() {
        for (_key, blk) in dated_blocks_le(h, date) {
            for (ek, ev) in &blk.items {
                let (Some(ek), Value::Scalar(s)) = (ek, ev) else {
                    continue;
                };
                match ek.as_str() {
                    "capital" => capital = s.parse::<u32>().ok().or(capital),
                    "government" => government = Some(s.clone()),
                    "religion" => religion = Some(s.clone()),
                    "primary_culture" => primary_culture = Some(s.clone()),
                    "technology_group" => technology_group = Some(s.clone()),
                    "unit_type" => unit_type = Some(s.clone()),
                    "national_focus" => national_focus = Some(s.clone()),
                    "government_rank" => government_rank = s.parse::<i64>().ok().or(government_rank),
                    "mercantilism" => mercantilism = s.parse::<f64>().ok().or(mercantilism),
                    "elector" => elector = s == "yes",
                    "add_government_reform" => {
                        if !government_reforms.iter().any(|r| r == s) {
                            government_reforms.push(s.clone());
                        }
                    }
                    "add_accepted_culture" => {
                        if !accepted_cultures.iter().any(|c| c == s) {
                            accepted_cultures.push(s.clone());
                        }
                    }
                    "remove_accepted_culture" => accepted_cultures.retain(|c| c != s),
                    _ => {}
                }
            }
        }
    }

    // Prefer the PROV<id> loc string; fall back to definition.csv's name column.
    let capital_name = capital.and_then(|id| {
        loc.get(&format!("PROV{id}"))
            .map(str::to_string)
            .or_else(|| province_names(vfs).remove(&id))
    });

    let dated_blocks = history
        .as_ref()
        .map(|h| crate::province_details::dated_blocks_of(h, date))
        .unwrap_or_default();

    let (ruler, ruler_reason, queen, heir) = history
        .as_ref()
        .map(|h| characters_at(h, date))
        .unwrap_or((None, None, None, None));

    // Name pools + historical setup come from the common/countries file.
    let name_pools = country_bytes
        .as_deref()
        .map(name_pools)
        .unwrap_or(NamePools {
            monarch_names: Vec::new(),
            leader_names: Vec::new(),
            ship_names: Vec::new(),
            army_names: Vec::new(),
            fleet_names: Vec::new(),
        });
    let list_block = |key: &str| -> Vec<String> {
        country_block
            .as_ref()
            .and_then(|b| b.get_block(key))
            .map(|b| b.bare_scalars().map(str::to_string).collect())
            .unwrap_or_default()
    };
    let historical_idea_groups = list_block("historical_idea_groups");
    let historical_units = list_block("historical_units");

    Ok(CountryDetails {
        tag: tag.to_string(),
        localized_name: loc.resolve_or(tag, &name),
        name,
        adjective: loc.get(&format!("{tag}_ADJ")).map(str::to_string),
        color,
        country_file: file_rel,
        history_file,
        revolutionary_colors,
        graphical_culture,
        government_name: government.as_deref().map(|k| loc.resolve(k)),
        government,
        government_rank,
        religion_name: religion.as_deref().map(|k| loc.resolve(k)),
        religion,
        primary_culture_name: primary_culture.as_deref().map(|k| loc.resolve(k)),
        primary_culture,
        technology_group_name: technology_group.as_deref().map(|k| loc.resolve(k)),
        technology_group,
        unit_type,
        national_focus,
        mercantilism,
        elector,
        government_reforms,
        accepted_cultures,
        historical_rivals: list_values("historical_rival"),
        historical_friends: list_values("historical_friend"),
        capital,
        capital_name,
        ruler,
        ruler_reason,
        queen,
        heir,
        name_pools,
        historical_idea_groups,
        historical_units,
        ideas: national_ideas(vfs, loc, tag),
        dated_blocks,
    })
}

/// First three bare scalar tokens of a block parsed as integers, e.g. the three
/// values of `revolutionary_colors = { 15 0 16 }` (palette indices).
fn three_ints(block: &Block) -> Option<[i64; 3]> {
    let mut it = block.bare_scalars().filter_map(|s| s.parse::<i64>().ok());
    Some([it.next()?, it.next()?, it.next()?])
}

// --- Grouped option lists for the country panel dropdowns (Sprint 1.2) -------

/// One selectable entry that belongs to a group (religion within a religion
/// group, culture within a culture group). `color` is present for religions.
#[derive(Debug, Clone, serde::Serialize)]
pub struct GroupedEntry {
    pub key: String,
    pub name: String,
    pub group: String,
    pub group_name: String,
    pub color: Option<[u8; 3]>,
}

/// All religions grouped by religion group, in file order. A nested block is a
/// religion if it carries a `color` block or an `icon` scalar (skips group-level
/// helper blocks like `religious_schools`, `harmonized_modifier`).
pub fn religion_list(vfs: &Vfs, loc: &LocStore) -> Vec<GroupedEntry> {
    let root = parse_dir_merged(vfs, "common/religions");
    let mut out = Vec::new();
    for (group, group_block) in root.key_blocks() {
        for (key, block) in group_block.key_blocks() {
            let is_religion =
                block.get_block("color").is_some() || block.get_scalar("icon").is_some();
            if !is_religion {
                continue;
            }
            out.push(GroupedEntry {
                key: key.to_string(),
                name: loc.resolve(key),
                group: group.to_string(),
                group_name: loc.resolve(group),
                color: block.get_block("color").and_then(paradox::color_from_block),
            });
        }
    }
    out
}

// --- Religion details (Sprint 5.2) --------------------------------------
//
// Religions live nested one level inside a religion *group* block in
// common/religions. Group-level keys (defender_of_faith, crusade_name,
// can_form_personal_unions, center_of_religion, flag_emblem_*, religious_schools)
// stay with the group and are not part of a religion's own block.

/// Religion-level feature toggles surfaced as checkboxes in the panel. Anything
/// present in the block that is neither here nor otherwise modeled is preserved
/// on disk (byte-surgical) and shown in the read-only advanced section.
pub const RELIGION_FEATURES: &[&str] = &[
    "hre_religion",
    "hre_heretic_religion",
    "uses_church_power",
    "fervor",
    "uses_karma",
    "uses_piety",
    "uses_harmony",
    "uses_isolationism",
    "personal_deity",
    "misguided_heretic",
    "declare_war_in_regency",
    "has_patriarchs",
    "allow_female_defenders_of_the_faith",
    "ancestors",
    "authority",
    "doom",
    "fetishist_cult",
    "religious_reforms",
    "require_reformed_for_institution_development",
    "can_have_secondary_religion",
    "uses_anglican_power",
    "uses_hussite_power",
    "uses_judaism_power",
];

/// Religion-level keys the panel models explicitly (edited through typed fields),
/// so they must not double up in the read-only advanced section.
const RELIGION_MODELED: &[&str] = &["color", "icon", "country", "province", "heretic", "date"];

/// One `key = value` modifier row (a country/province modifier block entry).
#[derive(Debug, serde::Serialize)]
pub struct ModRow {
    pub key: String,
    pub value: String,
}

/// One unmodeled entry of a religion block, kept for the read-only advanced
/// section — never dropped (byte-surgical edits preserve it on disk).
#[derive(Debug, serde::Serialize)]
pub struct RawEntry {
    pub key: String,
    /// "scalar" or "block".
    pub kind: &'static str,
    /// Scalar value, or "{ … }" for nested blocks.
    pub value: String,
}

/// A religion group option (for the create-flow / move-to-group dropdown).
#[derive(Debug, serde::Serialize)]
pub struct ReligionGroupEntry {
    pub key: String,
    pub name: String,
}

/// Full details of a single religion (its own block inside a group).
#[derive(Debug, serde::Serialize)]
pub struct ReligionDetails {
    pub key: String,
    pub group_key: String,
    pub group_name: String,
    pub localized_name: String,
    /// The three raw ints of `color = { r g b }` (vanilla convention is ints).
    pub color: Option<[i64; 3]>,
    /// Religion icon index (1-based, as written; frame in the strip is `icon - 1`).
    pub icon: Option<i64>,
    pub country_modifiers: Vec<ModRow>,
    pub province_modifiers: Vec<ModRow>,
    pub heretics: Vec<String>,
    /// `date = Y.M.D` enable date (Protestant 1517.10.31, …), if present.
    pub enable_date: Option<String>,
    /// Known feature booleans present with value `yes`.
    pub features: Vec<String>,
    /// Unmodeled entries (advanced/raw section; preserved on write).
    pub raw_remainder: Vec<RawEntry>,
    /// Game-relative file the group+religion live in (byte-surgical edits target it).
    pub source_file: String,
    /// Exact original `<key> = { ... }` block text, for a byte-faithful group move.
    pub raw_block_text: String,
    pub country_count: u32,
    pub province_count: u32,
    /// Up to 8 country tags using this religion at 1444 (jump links).
    pub sample_tags: Vec<String>,
    /// Up to 8 province ids of this religion at 1444 (jump links).
    pub sample_provinces: Vec<u32>,
}

/// All religion groups (top-level blocks of common/religions), localized, in file
/// order, deduped by key. For the create flow and the move-to-group dropdown.
pub fn religion_group_list(vfs: &Vfs, loc: &LocStore) -> Vec<ReligionGroupEntry> {
    let root = parse_dir_merged(vfs, "common/religions");
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for (group, _b) in root.key_blocks() {
        if seen.insert(group.to_string()) {
            out.push(ReligionGroupEntry {
                key: group.to_string(),
                name: loc.resolve(group),
            });
        }
    }
    out
}

/// Byte span `[start, end)` of a nested `key = { ... }` following `path` from the
/// top level: `end` is just past the matching close brace. Comments/strings are
/// skipped so braces inside them don't confuse the matcher.
pub(crate) fn find_block_span(src: &[u8], path: &[&str]) -> Option<(usize, usize)> {
    let (mut lo, mut hi) = (0usize, src.len());
    let mut result = None;
    for (i, key) in path.iter().enumerate() {
        let (ks, inner, end) = find_key_block(src, lo, hi, key)?;
        if i == path.len() - 1 {
            result = Some((ks, end));
        }
        // Descend into this block's inner region for the next path element.
        lo = inner;
        hi = end;
    }
    result
}

/// Finds `key = { ... }` within `src[lo..hi)` at the current nesting level (nested
/// blocks are skipped whole). Returns `(key_start, inner_start, block_end)` where
/// `inner_start` is just after `{` and `block_end` is just after the matching `}`.
fn find_key_block(src: &[u8], lo: usize, hi: usize, key: &str) -> Option<(usize, usize, usize)> {
    let kb = key.as_bytes();
    let n = hi.min(src.len());
    let mut i = lo;
    // Skip whitespace/comments/strings, returning the new index.
    let skip = |mut p: usize| {
        while p < n {
            if src[p] == b'#' {
                while p < n && src[p] != b'\n' {
                    p += 1;
                }
            } else if src[p].is_ascii_whitespace() {
                p += 1;
            } else {
                break;
            }
        }
        p
    };
    let match_close = |mut p: usize| {
        // p is just after an opening '{'. Return index just past its match.
        let mut depth = 1;
        while p < n && depth > 0 {
            match src[p] {
                b'#' => {
                    while p < n && src[p] != b'\n' {
                        p += 1;
                    }
                }
                b'"' => {
                    p += 1;
                    while p < n && src[p] != b'"' {
                        p += 1;
                    }
                    p += 1;
                }
                b'{' => {
                    depth += 1;
                    p += 1;
                }
                b'}' => {
                    depth -= 1;
                    p += 1;
                }
                _ => p += 1,
            }
        }
        p
    };
    while i < n {
        match src[i] {
            b'#' => {
                while i < n && src[i] != b'\n' {
                    i += 1;
                }
            }
            b'"' => {
                i += 1;
                while i < n && src[i] != b'"' {
                    i += 1;
                }
                i += 1;
            }
            b'{' => {
                // A bare block at this level — skip it whole.
                i = match_close(i + 1);
            }
            b'}' | b'=' => i += 1,
            c if c.is_ascii_whitespace() => i += 1,
            _ => {
                let s = i;
                while i < n
                    && !src[i].is_ascii_whitespace()
                    && !matches!(src[i], b'{' | b'}' | b'=' | b'#' | b'"')
                {
                    i += 1;
                }
                // Look ahead for `= {`.
                let mut j = skip(i);
                if j < n && src[j] == b'=' {
                    j = skip(j + 1);
                    if j < n && src[j] == b'{' {
                        let inner = j + 1;
                        let end = match_close(inner);
                        if &src[s..i] == kb {
                            return Some((s, inner, end));
                        }
                        // Not our key — continue scanning past this block.
                        i = end;
                        continue;
                    }
                }
                // Scalar or other token: `i` is already past it.
            }
        }
    }
    None
}

/// The exact original text of a nested `key = { ... }` block (Latin-1 decoded so
/// bytes round-trip when re-inserted as Latin-1 elsewhere).
pub(crate) fn extract_named_block(src: &[u8], path: &[&str]) -> Option<String> {
    let (s, e) = find_block_span(src, path)?;
    Some(src[s..e].iter().map(|&b| b as char).collect())
}

/// Full details of one religion, including its source file and the exact original
/// block text (for a byte-faithful group move that preserves unmodeled content).
pub fn religion_details(vfs: &Vfs, loc: &LocStore, key: &str) -> Result<ReligionDetails, String> {
    // Scan each common/religions file so we can name the file the block lives in.
    let mut found: Option<(String, String, Vec<u8>, Block)> = None;
    for (fname, path) in vfs.list_dir("common/religions") {
        if !fname.to_lowercase().ends_with(".txt") {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let block = parse_bytes(&bytes);
        for (group, gb) in block.key_blocks() {
            if let Some(rb) = gb.get_block(key) {
                let is_religion =
                    rb.get_block("color").is_some() || rb.get_scalar("icon").is_some();
                if is_religion {
                    found = Some((
                        format!("common/religions/{fname}"),
                        group.to_string(),
                        bytes.clone(),
                        rb.clone(),
                    ));
                    break;
                }
            }
        }
        if found.is_some() {
            break;
        }
    }
    let (source_file, group_key, bytes, rb) =
        found.ok_or_else(|| format!("Religion not found: {key}"))?;

    let color = rb.get_block("color").and_then(|b| {
        three_ints(b).or_else(|| {
            paradox::color_from_block(b).map(|c| [c[0] as i64, c[1] as i64, c[2] as i64])
        })
    });
    let icon = rb.get_scalar("icon").and_then(|s| s.parse::<i64>().ok());

    let mod_rows = |name: &str| -> Vec<ModRow> {
        rb.get_block(name)
            .map(|b| {
                b.items
                    .iter()
                    .filter_map(|(k, v)| match (k, v) {
                        (Some(k), Value::Scalar(s)) => Some(ModRow {
                            key: k.clone(),
                            value: s.clone(),
                        }),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    let country_modifiers = mod_rows("country");
    let province_modifiers = mod_rows("province");
    let heretics = rb
        .get_block("heretic")
        .map(|b| b.bare_scalars().map(str::to_string).collect())
        .unwrap_or_default();
    let enable_date = rb.get_scalar("date").map(str::to_string);
    let features: Vec<String> = RELIGION_FEATURES
        .iter()
        .filter(|f| rb.get_scalar(f) == Some("yes"))
        .map(|f| f.to_string())
        .collect();

    let mut raw_remainder = Vec::new();
    for (k, v) in &rb.items {
        let Some(k) = k else { continue };
        if RELIGION_MODELED.contains(&k.as_str()) || RELIGION_FEATURES.contains(&k.as_str()) {
            continue;
        }
        match v {
            Value::Scalar(s) => raw_remainder.push(RawEntry {
                key: k.clone(),
                kind: "scalar",
                value: s.clone(),
            }),
            Value::Block(_) => raw_remainder.push(RawEntry {
                key: k.clone(),
                kind: "block",
                value: "{ … }".to_string(),
            }),
        }
    }

    let raw_block_text = extract_named_block(&bytes, &[&group_key, key]).unwrap_or_default();

    // Usage at 1444: provinces of this religion, and countries whose top-level
    // religion is this key.
    let mut province_count = 0u32;
    let mut sample_provinces: Vec<u32> = Vec::new();
    for (id, st) in province_history(vfs) {
        if st.religion.as_deref() == Some(key) {
            province_count += 1;
            sample_provinces.push(id);
        }
    }
    sample_provinces.sort_unstable();
    sample_provinces.truncate(8);

    let mut country_count = 0u32;
    let mut sample_tags: Vec<String> = Vec::new();
    for (fname, path) in vfs.list_dir("history/countries") {
        if !fname.to_lowercase().ends_with(".txt") {
            continue;
        }
        let Some(b) = parse_path(&path) else { continue };
        if b.get_scalar("religion") == Some(key) {
            country_count += 1;
            if sample_tags.len() < 8 {
                let tag: String = fname.chars().take(3).collect();
                sample_tags.push(tag.to_uppercase());
            }
        }
    }

    Ok(ReligionDetails {
        localized_name: loc.resolve(key),
        group_name: loc.resolve(&group_key),
        key: key.to_string(),
        group_key,
        color,
        icon,
        country_modifiers,
        province_modifiers,
        heretics,
        enable_date,
        features,
        raw_remainder,
        source_file,
        raw_block_text,
        country_count,
        province_count,
        sample_tags,
        sample_provinces,
    })
}

/// Group-level (non-culture) block/scalar keys inside a culture group. A nested
/// block whose key is one of these is a group helper, not a culture.
const CULTURE_GROUP_KEYS: &[&str] = &[
    "male_names",
    "female_names",
    "dynasty_names",
    "country",
    "province",
    "graphical_culture",
    "second_graphical_culture",
];

/// Culture-level keys the panel models explicitly (edited through typed fields),
/// so they must not double up in the read-only advanced section.
const CULTURE_MODELED: &[&str] = &["primary", "male_names", "female_names", "dynasty_names"];

/// All cultures grouped by culture group, in file order. Group-level helper keys
/// (name pools, modifiers, graphical_culture) are excluded — only real culture blocks.
pub fn culture_list(vfs: &Vfs, loc: &LocStore) -> Vec<GroupedEntry> {
    let root = parse_dir_merged(vfs, "common/cultures");
    let mut out = Vec::new();
    for (group, group_block) in root.key_blocks() {
        for (key, _block) in group_block.key_blocks() {
            if CULTURE_GROUP_KEYS.contains(&key) {
                continue;
            }
            out.push(GroupedEntry {
                key: key.to_string(),
                name: loc.resolve(key),
                group: group.to_string(),
                group_name: loc.resolve(group),
                color: None,
            });
        }
    }
    out
}

/// A culture group option (create flow / move-to-group dropdown).
#[derive(Debug, serde::Serialize)]
pub struct CultureGroupEntry {
    pub key: String,
    pub name: String,
}

/// All culture groups (top-level blocks of common/cultures), localized, in file
/// order, deduped by key. For the create flow and the move-to-group dropdown.
pub fn culture_group_list(vfs: &Vfs, loc: &LocStore) -> Vec<CultureGroupEntry> {
    let root = parse_dir_merged(vfs, "common/cultures");
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for (group, _b) in root.key_blocks() {
        if seen.insert(group.to_string()) {
            out.push(CultureGroupEntry {
                key: group.to_string(),
                name: loc.resolve(group),
            });
        }
    }
    out
}

/// Bare/quoted string tokens inside a name-pool block reached by `path` (e.g.
/// `[group, culture, "male_names"]`). Decoded as Latin-1 so Windows-1252 high
/// bytes (accented names) round-trip exactly — the parser's `from_utf8_lossy`
/// would corrupt them to U+FFFD and they'd fail to re-encode. Comments skipped.
fn pool_names(src: &[u8], path: &[&str]) -> Vec<String> {
    let Some((s, e)) = find_block_span(src, path) else {
        return Vec::new();
    };
    // The block's own `{` is the first brace within its span; `e` is just past
    // the matching `}`, so the inner region is (open, e-1).
    let Some(rel) = src[s..e].iter().position(|&b| b == b'{') else {
        return Vec::new();
    };
    let open = s + rel + 1;
    let close = e.saturating_sub(1).min(src.len());
    if open >= close {
        return Vec::new();
    }
    let inner = &src[open..close];
    let n = inner.len();
    let mut out = Vec::new();
    let mut i = 0;
    while i < n {
        let c = inner[i];
        if c == b'#' {
            while i < n && inner[i] != b'\n' {
                i += 1;
            }
        } else if c == b'"' {
            i += 1;
            let start = i;
            while i < n && inner[i] != b'"' {
                i += 1;
            }
            out.push(inner[start..i].iter().map(|&b| b as char).collect());
            if i < n {
                i += 1; // closing quote
            }
        } else if c.is_ascii_whitespace() || matches!(c, b'{' | b'}' | b'=') {
            i += 1;
        } else {
            let start = i;
            while i < n
                && !inner[i].is_ascii_whitespace()
                && !matches!(inner[i], b'"' | b'{' | b'}' | b'=' | b'#')
            {
                i += 1;
            }
            out.push(inner[start..i].iter().map(|&b| b as char).collect());
        }
    }
    out
}

/// Full details of a single culture (its own block inside a culture group). See
/// [`CultureDetails`]. Cultures carry no color in the files (Sprint 6.1).
#[derive(Debug, serde::Serialize)]
pub struct CultureDetails {
    pub key: String,
    pub group_key: String,
    pub group_name: String,
    pub localized_name: String,
    /// `primary = TAG` (optional): the culture's primary nation.
    pub primary: Option<String>,
    /// Culture-level name pools (empty when the culture falls back to the group).
    pub male_names: Vec<String>,
    pub female_names: Vec<String>,
    pub dynasty_names: Vec<String>,
    /// Whether each culture-level pool block exists on disk (empty vs absent —
    /// drives set-block vs insert-statement when editing).
    pub male_names_present: bool,
    pub female_names_present: bool,
    pub dynasty_names_present: bool,
    /// Group-level fallback pools (used when the culture omits its own).
    pub group_male_names: Vec<String>,
    pub group_female_names: Vec<String>,
    pub group_dynasty_names: Vec<String>,
    pub group_male_names_present: bool,
    pub group_female_names_present: bool,
    pub group_dynasty_names_present: bool,
    /// Group-level graphical culture(s).
    pub group_graphical_culture: Option<String>,
    pub group_second_graphical_culture: Option<String>,
    /// Unmodeled culture-level entries (read-only advanced; preserved on write).
    pub raw_remainder: Vec<RawEntry>,
    /// Game-relative file the group+culture live in (edits target it).
    pub source_file: String,
    /// Exact original `<key> = { ... }` block text, for a byte-faithful group move.
    pub raw_block_text: String,
    /// Countries with this culture as `primary_culture` at 1444.
    pub primary_count: u32,
    pub primary_tags: Vec<String>,
    /// Countries with this culture in `add_accepted_culture` at 1444.
    pub accepted_count: u32,
    pub accepted_tags: Vec<String>,
    /// Provinces of this culture at 1444.
    pub province_count: u32,
    pub sample_provinces: Vec<u32>,
}

/// Full details of one culture, including its source file and the exact original
/// block text (for a byte-faithful group move that preserves unmodeled content).
pub fn culture_details(vfs: &Vfs, loc: &LocStore, key: &str) -> Result<CultureDetails, String> {
    if CULTURE_GROUP_KEYS.contains(&key) {
        return Err(format!("Not a culture: {key}"));
    }
    // Scan each common/cultures file so we can name the file the block lives in
    // and read raw bytes for Latin-1-faithful pool extraction.
    let mut found: Option<(String, String, Vec<u8>, Block, Block)> = None;
    for (fname, path) in vfs.list_dir("common/cultures") {
        if !fname.to_lowercase().ends_with(".txt") {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let block = parse_bytes(&bytes);
        for (group, gb) in block.key_blocks() {
            if let Some(cb) = gb.get_block(key) {
                found = Some((
                    format!("common/cultures/{fname}"),
                    group.to_string(),
                    bytes.clone(),
                    cb.clone(),
                    gb.clone(),
                ));
                break;
            }
        }
        if found.is_some() {
            break;
        }
    }
    let (source_file, group_key, bytes, cb, gb) =
        found.ok_or_else(|| format!("Culture not found: {key}"))?;

    let primary = cb.get_scalar("primary").map(str::to_string);
    let male_names = pool_names(&bytes, &[&group_key, key, "male_names"]);
    let female_names = pool_names(&bytes, &[&group_key, key, "female_names"]);
    let dynasty_names = pool_names(&bytes, &[&group_key, key, "dynasty_names"]);
    let group_male_names = pool_names(&bytes, &[&group_key, "male_names"]);
    let group_female_names = pool_names(&bytes, &[&group_key, "female_names"]);
    let group_dynasty_names = pool_names(&bytes, &[&group_key, "dynasty_names"]);
    let group_graphical_culture = gb.get_scalar("graphical_culture").map(str::to_string);
    let group_second_graphical_culture =
        gb.get_scalar("second_graphical_culture").map(str::to_string);

    let mut raw_remainder = Vec::new();
    for (k, v) in &cb.items {
        let Some(k) = k else { continue };
        if CULTURE_MODELED.contains(&k.as_str()) {
            continue;
        }
        match v {
            Value::Scalar(s) => raw_remainder.push(RawEntry {
                key: k.clone(),
                kind: "scalar",
                value: s.clone(),
            }),
            Value::Block(_) => raw_remainder.push(RawEntry {
                key: k.clone(),
                kind: "block",
                value: "{ … }".to_string(),
            }),
        }
    }

    let raw_block_text = extract_named_block(&bytes, &[&group_key, key]).unwrap_or_default();

    // Usage at 1444: provinces of this culture, and countries whose top-level
    // primary_culture / add_accepted_culture is this key.
    let mut province_count = 0u32;
    let mut sample_provinces: Vec<u32> = Vec::new();
    for (id, st) in province_history(vfs) {
        if st.culture.as_deref() == Some(key) {
            province_count += 1;
            sample_provinces.push(id);
        }
    }
    sample_provinces.sort_unstable();
    sample_provinces.truncate(8);

    let mut primary_count = 0u32;
    let mut primary_tags: Vec<String> = Vec::new();
    let mut accepted_count = 0u32;
    let mut accepted_tags: Vec<String> = Vec::new();
    for (fname, path) in vfs.list_dir("history/countries") {
        if !fname.to_lowercase().ends_with(".txt") {
            continue;
        }
        let Some(b) = parse_path(&path) else { continue };
        let tag: String = fname.chars().take(3).collect::<String>().to_uppercase();
        if b.get_scalar("primary_culture") == Some(key) {
            primary_count += 1;
            if primary_tags.len() < 8 {
                primary_tags.push(tag.clone());
            }
        }
        let accepted = b.items.iter().any(|(k, v)| {
            matches!((k, v), (Some(k), Value::Scalar(s)) if k == "add_accepted_culture" && s == key)
        });
        if accepted {
            accepted_count += 1;
            if accepted_tags.len() < 8 {
                accepted_tags.push(tag);
            }
        }
    }

    let male_names_present = cb.get_block("male_names").is_some();
    let female_names_present = cb.get_block("female_names").is_some();
    let dynasty_names_present = cb.get_block("dynasty_names").is_some();
    let group_male_names_present = gb.get_block("male_names").is_some();
    let group_female_names_present = gb.get_block("female_names").is_some();
    let group_dynasty_names_present = gb.get_block("dynasty_names").is_some();

    Ok(CultureDetails {
        localized_name: loc.resolve(key),
        group_name: loc.resolve(&group_key),
        key: key.to_string(),
        group_key,
        primary,
        male_names,
        female_names,
        dynasty_names,
        male_names_present,
        female_names_present,
        dynasty_names_present,
        group_male_names,
        group_female_names,
        group_dynasty_names,
        group_male_names_present,
        group_female_names_present,
        group_dynasty_names_present,
        group_graphical_culture,
        group_second_graphical_culture,
        raw_remainder,
        source_file,
        raw_block_text,
        primary_count,
        primary_tags,
        accepted_count,
        accepted_tags,
        province_count,
        sample_provinces,
    })
}

/// A country tag with its localized name and map color, for tag pickers
/// (historical rivals/friends). Resolved via country_tags -> countries.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CountryBrief {
    pub tag: String,
    pub name: String,
    pub color: Option<[u8; 3]>,
}

/// Every country tag in base+mod, with localized name + map color.
pub fn country_list(vfs: &Vfs, loc: &LocStore) -> Vec<CountryBrief> {
    let colors = country_colors(vfs);
    let tags = parse_dir_merged(vfs, "common/country_tags");
    let mut out = Vec::new();
    for (key, value) in &tags.items {
        let (Some(tag), Value::Scalar(_)) = (key, value) else {
            continue;
        };
        out.push(CountryBrief {
            tag: tag.clone(),
            name: loc.resolve(tag),
            color: colors.get(tag).copied(),
        });
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

/// Converts an image file (PNG/JPG/BMP/TGA) picked by the user into a 128x128
/// flag: returns `(tga_bytes, png_preview)`. The TGA is what the game wants at
/// `gfx/flags/TAG.tga` (TGA has no magic bytes, so the format is set explicitly
/// on encode); the PNG is for the panel's live preview of the pending flag.
pub fn convert_flag(path: &str) -> Result<(Vec<u8>, Vec<u8>), String> {
    use image::codecs::tga::TgaEncoder;
    use image::{ExtendedColorType, ImageEncoder};

    let img = image::open(path).map_err(|e| format!("Failed to read image {path}: {e}"))?;
    let resized = img
        .resize_exact(128, 128, image::imageops::FilterType::Lanczos3)
        .to_rgb8();
    let raw = resized.into_raw();

    let mut tga = Vec::new();
    // Uncompressed true-color TGA (image type 2), matching vanilla flags — the
    // game's loader is happiest with these; RLE is disabled for that reason.
    TgaEncoder::new(&mut tga)
        .disable_rle()
        .write_image(&raw, 128, 128, ExtendedColorType::Rgb8)
        .map_err(|e| format!("Failed to encode TGA: {e}"))?;
    let png = crate::map_renderer::encode_png(&raw, 128, 128)?;
    Ok((tga, png))
}

/// The country's flag (gfx/flags/TAG.tga), converted to PNG.
pub fn country_flag_png(vfs: &Vfs, tag: &str) -> Result<Vec<u8>, String> {
    validate_tag(tag)?;
    let rel = format!("gfx/flags/{tag}.tga");
    let bytes = vfs.read(&rel)?;
    // TGA has no magic bytes, so the format can't be sniffed from content.
    let img = image::load_from_memory_with_format(&bytes, image::ImageFormat::Tga)
        .map_err(|e| format!("Failed to load flag for {tag}: {e}"))?
        .to_rgb8();
    let (w, h) = img.dimensions();
    crate::map_renderer::encode_png(&img.into_raw(), w, h)
}

#[derive(Debug, Default)]
pub struct ProvinceState {
    pub owner: Option<String>,
    /// Effective controller as of the fold date (Sprint 13.3 occupation). Often
    /// equals `owner`; when it differs the province is occupied (or rebel-held,
    /// `controller = REB`) and the political render stripes owner/controller.
    pub controller: Option<String>,
    pub religion: Option<String>,
    pub culture: Option<String>,
    pub trade_goods: Option<String>,
    pub development: Option<f32>,
    /// Whether the province is in the Holy Roman Empire as of the fold date
    /// (history `hre = yes`, cleared by a later `hre = no`). Used to derive a
    /// country's `is_part_of_hre` from its capital province.
    pub hre: bool,
}

/// Initial (1444) province state from history/provinces. Only top-level keys
/// are read; dated blocks are post-start events and are skipped. Kept for the
/// callers that want the file's as-written base state (map render, usage counts).
pub fn province_history(vfs: &Vfs) -> HashMap<u32, ProvinceState> {
    let mut states = HashMap::new();
    for ast in province_asts(vfs).iter() {
        let id = ast.id;
        let Some(block) = &ast.block else {
            continue;
        };
        let get = |key: &str| block.get_scalar(key).map(str::to_string);
        let dev_part = |key: &str| block.get_scalar(key).and_then(|s| s.parse::<f32>().ok());
        let dev_parts = [
            dev_part("base_tax"),
            dev_part("base_production"),
            dev_part("base_manpower"),
        ];
        let development = dev_parts
            .iter()
            .any(Option::is_some)
            .then(|| dev_parts.iter().flatten().sum());
        states.insert(
            id,
            ProvinceState {
                owner: get("owner"),
                controller: get("controller"),
                religion: get("religion"),
                culture: get("culture"),
                trade_goods: get("trade_goods"),
                development,
                hre: block.get_scalar("hre") == Some("yes"),
            },
        );
    }
    states
}

/// Mutable owner/religion/culture/goods/dev accumulator used to fold a province
/// history file up to a selected date (Sprint 12.2 view-at-date).
#[derive(Default)]
struct DerivedState {
    owner: Option<String>,
    controller: Option<String>,
    religion: Option<String>,
    culture: Option<String>,
    trade_goods: Option<String>,
    base_tax: Option<f32>,
    base_production: Option<f32>,
    base_manpower: Option<f32>,
    hre: bool,
}

impl DerivedState {
    /// Applies one `key = value` statement with the game's file-order semantics:
    /// scalars set; `base_*` set; `add_base_*` increment.
    fn apply(&mut self, key: &str, value: &Value) {
        let Value::Scalar(s) = value else { return };
        let f = || s.parse::<f32>().ok();
        match key {
            "owner" => self.owner = Some(s.clone()),
            "controller" => self.controller = Some(s.clone()),
            "religion" => self.religion = Some(s.clone()),
            "culture" => self.culture = Some(s.clone()),
            "trade_goods" => self.trade_goods = Some(s.clone()),
            "hre" => self.hre = s == "yes",
            "base_tax" => self.base_tax = f(),
            "base_production" => self.base_production = f(),
            "base_manpower" => self.base_manpower = f(),
            "add_base_tax" => self.base_tax = Some(self.base_tax.unwrap_or(0.0) + f().unwrap_or(0.0)),
            "add_base_production" => {
                self.base_production = Some(self.base_production.unwrap_or(0.0) + f().unwrap_or(0.0))
            }
            "add_base_manpower" => {
                self.base_manpower = Some(self.base_manpower.unwrap_or(0.0) + f().unwrap_or(0.0))
            }
            _ => {}
        }
    }

    fn into_state(self) -> ProvinceState {
        let parts = [self.base_tax, self.base_production, self.base_manpower];
        let development = parts
            .iter()
            .any(Option::is_some)
            .then(|| parts.iter().flatten().sum());
        ProvinceState {
            owner: self.owner,
            controller: self.controller,
            religion: self.religion,
            culture: self.culture,
            trade_goods: self.trade_goods,
            development,
            hre: self.hre,
        }
    }
}

/// Province state as of `date`: the top-level state with every dated block whose
/// date ≤ `date` folded in, in file order (exactly as the game applies history).
/// At the effective start this matches `province_details::effective_1444`.
pub fn province_history_at(vfs: &Vfs, date: Date) -> HashMap<u32, ProvinceState> {
    let mut states = HashMap::new();
    for ast in province_asts(vfs).iter() {
        let id = ast.id;
        let Some(block) = &ast.block else {
            continue;
        };
        let mut acc = DerivedState::default();
        // Top level first (beginning-of-time state).
        for (k, v) in &block.items {
            if let Some(k) = k {
                if parse_date(k).is_none() {
                    acc.apply(k, v);
                }
            }
        }
        // Then dated blocks ≤ date, in file order.
        for (k, v) in &block.items {
            let (Some(k), Value::Block(b)) = (k, v) else {
                continue;
            };
            let Some(d) = parse_date(k) else { continue };
            if d > date {
                continue;
            }
            for (ek, ev) in &b.items {
                if let Some(ek) = ek {
                    acc.apply(ek, ev);
                }
            }
        }
        states.insert(id, acc.into_state());
    }
    states
}

/// Water province ids (sea + lakes) from map/default.map.
pub fn water_ids(vfs: &Vfs) -> HashSet<u32> {
    let mut water = HashSet::new();
    if let Some(block) = parse_rel(vfs, "map/default.map") {
        for key in ["sea_starts", "lakes"] {
            if let Some(list) = block.get_block(key) {
                water.extend(list.bare_ids());
            }
        }
    }
    water
}

/// Per-province political + eligibility data for the bottom-toolbar brush tools
/// (Sprint 1.4). One record per definition.csv province so the frontend can map
/// a province id to its history file, know its 1444 owner/controller/cores, and
/// filter water/wasteland — everything the add/remove edit generators need to
/// decide insert-vs-replace and which cores belong to the owner.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ProvincePolitical {
    pub id: u32,
    /// Game-relative history file path. The existing file when one is present,
    /// else a synthesized `history/provinces/<id> - <name>.txt` so a first edit
    /// on a fileless land province still writes to a sensibly named file.
    pub file: String,
    /// 1444 owner tag (top-level `owner`), if any.
    pub owner: Option<String>,
    /// 1444 controller tag (top-level `controller`), if any.
    pub controller: Option<String>,
    /// 1444 cores (top-level `add_core` values), in file order.
    pub cores: Vec<String>,
    /// Sea or lake (from default.map) — ineligible for the add tool.
    pub water: bool,
    /// Impassable wasteland (climate.txt `impassable`) — ineligible for add.
    pub wasteland: bool,
    /// 1444 `base_tax` (top-level scalar), if present. Development painting
    /// (Sprint 9) needs the three components per province in one bulk payload —
    /// `get_province_details` is too slow for airbrush strokes. `None` means the
    /// key is absent (a keyless component; the paint tool creates it at the floor).
    pub base_tax: Option<f64>,
    /// 1444 `base_production` (top-level scalar), if present.
    pub base_production: Option<f64>,
    /// 1444 `base_manpower` (top-level scalar), if present.
    pub base_manpower: Option<f64>,
}

/// Builds the [`ProvincePolitical`] payload for every province at the file's
/// base (top-level) state. Pre-Sprint-12 signature; used by the country-create
/// province picker (date-independent eligibility).
pub fn province_political(vfs: &Vfs) -> Vec<ProvincePolitical> {
    province_political_impl(vfs, None)
}

/// [`province_political`] as of `date`: owner/controller/cores/dev reflect every
/// dated history block with a date ≤ `date` (Sprint 12.2 view-at-date).
pub fn province_political_at(vfs: &Vfs, date: Date) -> Vec<ProvincePolitical> {
    province_political_impl(vfs, Some(date))
}

/// Shared builder. `at = None` reads only top-level keys (identical to the
/// pre-Sprint-12 behavior); `at = Some(date)` folds dated blocks ≤ date.
fn province_political_impl(vfs: &Vfs, at: Option<Date>) -> Vec<ProvincePolitical> {
    let defs = province_definitions(vfs);
    let water = water_ids(vfs);
    let impassable: HashSet<u32> = climate_zones(vfs)
        .into_iter()
        .filter(|(_, z)| z == "impassable")
        .map(|(id, _)| id)
        .collect();

    // id -> existing history file name, and id -> (owner, controller, cores).
    let mut files: HashMap<u32, String> = HashMap::new();
    let mut fields: HashMap<u32, (Option<String>, Option<String>, Vec<String>)> = HashMap::new();
    // id -> [base_tax, base_production, base_manpower] scalars (dev painting).
    let mut devs: HashMap<u32, [Option<f64>; 3]> = HashMap::new();
    for ast in province_asts(vfs).iter() {
        let id = ast.id;
        files.insert(id, ast.file_name.clone());
        if let Some(block) = &ast.block {
            // Statements to apply: top level, plus (when viewing at a date) the
            // dated blocks ≤ date in file order.
            let mut owner = None;
            let mut controller = None;
            let mut cores: Vec<String> = Vec::new();
            let mut dev = [None, None, None];
            let mut apply = |k: &str, v: &Value| {
                let scalar = match v {
                    Value::Scalar(s) => s.as_str(),
                    Value::Block(_) => return,
                };
                match k {
                    "owner" => owner = Some(scalar.to_string()),
                    "controller" => controller = Some(scalar.to_string()),
                    "add_core" => {
                        if !cores.iter().any(|c| c == scalar) {
                            cores.push(scalar.to_string());
                        }
                    }
                    "remove_core" => cores.retain(|c| c != scalar),
                    "base_tax" => dev[0] = scalar.parse::<f64>().ok(),
                    "base_production" => dev[1] = scalar.parse::<f64>().ok(),
                    "base_manpower" => dev[2] = scalar.parse::<f64>().ok(),
                    "add_base_tax" => {
                        dev[0] = Some(dev[0].unwrap_or(0.0) + scalar.parse::<f64>().unwrap_or(0.0))
                    }
                    "add_base_production" => {
                        dev[1] = Some(dev[1].unwrap_or(0.0) + scalar.parse::<f64>().unwrap_or(0.0))
                    }
                    "add_base_manpower" => {
                        dev[2] = Some(dev[2].unwrap_or(0.0) + scalar.parse::<f64>().unwrap_or(0.0))
                    }
                    _ => {}
                }
            };
            for (k, v) in &block.items {
                if let Some(k) = k {
                    if parse_date(k).is_none() {
                        apply(k, v);
                    }
                }
            }
            if let Some(date) = at {
                for (k, v) in &block.items {
                    let (Some(k), Value::Block(b)) = (k, v) else {
                        continue;
                    };
                    let Some(d) = parse_date(k) else { continue };
                    if d > date {
                        continue;
                    }
                    for (ek, ev) in &b.items {
                        if let Some(ek) = ek {
                            apply(ek, ev);
                        }
                    }
                }
            }
            drop(apply); // release the mutable captures before moving them out
            fields.insert(id, (owner, controller, cores));
            devs.insert(id, dev);
        }
    }

    let mut out = Vec::with_capacity(defs.len());
    for (id, _rgb, name) in &defs {
        let id = *id;
        let file = files.get(&id).cloned().unwrap_or_else(|| {
            let nm = if name.is_empty() {
                format!("Province {id}")
            } else {
                name.clone()
            };
            format!("{id} - {nm}.txt")
        });
        let (owner, controller, cores) = fields.get(&id).cloned().unwrap_or((None, None, Vec::new()));
        let [base_tax, base_production, base_manpower] = devs.get(&id).copied().unwrap_or([None; 3]);
        out.push(ProvincePolitical {
            id,
            file: format!("history/provinces/{file}"),
            owner,
            controller,
            cores,
            water: water.contains(&id),
            wasteland: impassable.contains(&id),
            base_tax,
            base_production,
            base_manpower,
        });
    }
    out
}

/// Province id -> climate zone name, from map/climate.txt. NOTE: climate.txt
/// shares one file across independent slots (zone, winter, monsoon, impassable),
/// so this collapses them all — a province in several lists keeps only the last.
/// For slot-correct reads use [`climate_slot`].
pub fn climate_zones(vfs: &Vfs) -> HashMap<u32, String> {
    let mut zones = HashMap::new();
    let Some(block) = parse_rel(vfs, "map/climate.txt") else {
        return zones;
    };
    for (zone, ids) in block.key_blocks() {
        for id in ids.bare_ids() {
            zones.insert(id, zone.to_string());
        }
    }
    zones
}

/// Climate-zone slot keys (map/climate.txt). "temperate" is the absence of all.
pub const CLIMATE_ZONE_KEYS: &[&str] = &["tropical", "arid", "arctic", "impassable"];
/// Winter-severity slot keys (map/climate.txt). Absence = no winter.
pub const WINTER_KEYS: &[&str] = &["mild_winter", "normal_winter", "severe_winter"];

/// Province id -> the matching list key among `wanted`, from map/climate.txt.
/// Because the file's slots (zone / winter / monsoon / impassable) coexist per
/// province, callers pass just their slot's keys to avoid cross-slot collisions.
pub fn climate_slot(vfs: &Vfs, wanted: &[&str]) -> HashMap<u32, String> {
    let mut out = HashMap::new();
    let Some(block) = parse_rel(vfs, "map/climate.txt") else {
        return out;
    };
    for (key, ids) in block.key_blocks() {
        if wanted.contains(&key) {
            for id in ids.bare_ids() {
                out.insert(id, key.to_string());
            }
        }
    }
    out
}

/// The game-relative file every climate slot lives in (single canonical file).
pub const CLIMATE_FILE: &str = "map/climate.txt";

/// One (province id, list key) pairing for the climate selector payload.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ClimateEntry {
    pub id: u32,
    pub key: String,
}

/// Payload for `get_climate` (Sprint 11.1): both independent slots of
/// map/climate.txt — the climate zone (tropical/arid/arctic/impassable, absence
/// = temperate) and the winter severity (mild/normal/severe, absence = none) —
/// plus the set of top-level list blocks that actually exist in the file (so the
/// frontend knows when a paint must first create an empty `key = { }` block).
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClimatePayload {
    pub file: String,
    pub zones: Vec<ClimateEntry>,
    pub winters: Vec<ClimateEntry>,
    /// Top-level list keys present in climate.txt (for create-when-absent).
    pub existing_lists: Vec<String>,
}

/// Builds the [`ClimatePayload`]: the two independent slots + which list blocks
/// exist. Both slots share one file; a province may appear in one list per slot.
pub fn climate_payload(vfs: &Vfs) -> ClimatePayload {
    let mut zones = Vec::new();
    let mut winters = Vec::new();
    let mut existing_lists = Vec::new();
    if let Some(block) = parse_rel(vfs, CLIMATE_FILE) {
        for (key, ids) in block.key_blocks() {
            existing_lists.push(key.to_string());
            if CLIMATE_ZONE_KEYS.contains(&key) {
                for id in ids.bare_ids() {
                    zones.push(ClimateEntry { id, key: key.to_string() });
                }
            } else if WINTER_KEYS.contains(&key) {
                for id in ids.bare_ids() {
                    winters.push(ClimateEntry { id, key: key.to_string() });
                }
            }
        }
    }
    ClimatePayload {
        file: CLIMATE_FILE.to_string(),
        zones,
        winters,
        existing_lists,
    }
}

// --- Simple Terrain data (Sprint 11.2) -----------------------------------
//
// Gameplay terrain lives in map/terrain.txt in two parts:
//   categories = { <cat> = { color=.. type=.. movement_cost=.. defence=..
//                            terrain_override = { <ids> } } }   — the gameplay
//     categories (grasslands, mountain, hills, desert, farmlands, forest, woods,
//     jungle, marsh, steppe, drylands, savannah, highlands, coastal_desert,
//     coastline, glacier, ocean, inland_ocean, pti, impassable_mountains), each
//     with its modifiers and an explicit per-province override list.
//   terrain = { <name> = { type = <cat> color = { <palette_index> } } }  — maps
//     terrain.bmp 8-bit palette indices to a category `type`.
// Effective terrain of a province = its terrain_override category if listed,
// else the category of its dominant terrain.bmp palette index.

/// map/terrain.txt `terrain` block: palette index -> category `type`.
pub fn terrain_palette_types(vfs: &Vfs) -> HashMap<u8, String> {
    let mut out = HashMap::new();
    let Some(block) = parse_rel(vfs, "map/terrain.txt") else {
        return out;
    };
    let Some(terrain) = block.get_block("terrain") else {
        return out;
    };
    for (_name, b) in terrain.key_blocks() {
        let Some(ty) = b.get_scalar("type") else {
            continue;
        };
        if let Some(color) = b.get_block("color") {
            for idx in color.bare_ids() {
                if idx <= 255 {
                    out.insert(idx as u8, ty.to_string());
                }
            }
        }
    }
    out
}

/// map/terrain.txt `categories.<cat>.terrain_override` -> province id -> category.
/// Later category definitions / ids win (last-wins), matching game load order.
pub fn terrain_override_map(vfs: &Vfs) -> HashMap<u32, String> {
    let mut out = HashMap::new();
    let Some(block) = parse_rel(vfs, "map/terrain.txt") else {
        return out;
    };
    let Some(categories) = block.get_block("categories") else {
        return out;
    };
    for (cat, b) in categories.key_blocks() {
        if let Some(list) = b.get_block("terrain_override") {
            for id in list.bare_ids() {
                out.insert(id, cat.to_string());
            }
        }
    }
    out
}

/// One terrain category with its gameplay modifiers (Sprint 11.2 right-side
/// list). `color` mirrors the Simple Terrain renderer. Terrain.txt carries no
/// combat-width key, so that field of the spec has no source and is omitted.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerrainCategory {
    pub key: String,
    pub name: String,
    pub color: [u8; 3],
    pub is_water: bool,
    /// True when the category already has a `terrain_override = { ... }` block
    /// (even if empty) — the frontend must not create a second one when painting.
    pub has_override_block: bool,
    pub movement_cost: Option<f64>,
    pub defence: Option<i64>,
    pub local_defensiveness: Option<f64>,
    pub local_development_cost: Option<f64>,
    pub supply_limit: Option<f64>,
    pub allowed_num_of_buildings: Option<i64>,
    pub nation_designer_cost_multiplier: Option<f64>,
}

/// All terrain categories in map/terrain.txt (definition order), with modifiers
/// and localized names. Mod-added categories included (hash-colored).
pub fn terrain_categories(vfs: &Vfs, loc: &LocStore) -> Vec<TerrainCategory> {
    use crate::map_renderer::terrain_color;
    let mut out = Vec::new();
    let Some(block) = parse_rel(vfs, "map/terrain.txt") else {
        return out;
    };
    let Some(categories) = block.get_block("categories") else {
        return out;
    };
    let f = |b: &Block, k: &str| b.get_scalar(k).and_then(|s| s.parse::<f64>().ok());
    let i = |b: &Block, k: &str| b.get_scalar(k).and_then(|s| s.parse::<i64>().ok());
    for (key, b) in categories.key_blocks() {
        out.push(TerrainCategory {
            name: loc.resolve(key),
            color: terrain_color(key),
            is_water: b.get_scalar("is_water") == Some("yes"),
            has_override_block: b.get_block("terrain_override").is_some(),
            movement_cost: f(b, "movement_cost"),
            defence: i(b, "defence"),
            local_defensiveness: f(b, "local_defensiveness"),
            local_development_cost: f(b, "local_development_cost"),
            supply_limit: f(b, "supply_limit"),
            allowed_num_of_buildings: i(b, "allowed_num_of_buildings"),
            nation_designer_cost_multiplier: f(b, "nation_designer_cost_multiplier"),
            key: key.to_string(),
        });
    }
    out
}

/// Effective terrain of one province (for the frontend's hover/override display).
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvinceTerrain {
    pub id: u32,
    /// Effective terrain category key.
    pub terrain: String,
    /// True when set by a terrain_override list (vs. auto-classified from bmp).
    pub is_override: bool,
    /// The dominant terrain.bmp category — what the province reverts to when the
    /// override is erased (Sprint 11.2 "Auto" eraser). For a non-overridden
    /// province this equals `terrain`; `None` when the province has no map pixels.
    pub auto_terrain: Option<String>,
    /// Sea or lake (default.map) — not paintable land.
    pub is_water: bool,
}

/// Payload for `get_effective_terrain`: per-province effective terrain plus the
/// terrain-category catalog (colors + modifiers for the right-side list).
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EffectiveTerrainPayload {
    pub provinces: Vec<ProvinceTerrain>,
    pub categories: Vec<TerrainCategory>,
}

/// Builds the [`EffectiveTerrainPayload`] (classifies the map + joins category
/// metadata). One terrain.bmp pixel pass; scoped to the command.
pub fn effective_terrain_payload(
    vfs: &Vfs,
    loc: &LocStore,
) -> Result<EffectiveTerrainPayload, String> {
    let eff = crate::map_renderer::effective_terrain(vfs)?;
    let mut provinces: Vec<ProvinceTerrain> = eff
        .by_province
        .iter()
        .map(|(id, (cat, is_override))| ProvinceTerrain {
            id: *id,
            terrain: cat.clone(),
            is_override: *is_override,
            auto_terrain: eff.auto_by_province.get(id).cloned(),
            is_water: eff.water.contains(id),
        })
        .collect();
    provinces.sort_by_key(|p| p.id);
    Ok(EffectiveTerrainPayload {
        provinces,
        categories: terrain_categories(vfs, loc),
    })
}

/// Area name -> province ids, from map/area.txt. Area blocks may start with
/// an optional `color = { ... }`, which bare_ids naturally skips.
pub fn areas(vfs: &Vfs) -> HashMap<String, Vec<u32>> {
    let mut out = HashMap::new();
    let Some(block) = parse_rel(vfs, "map/area.txt") else {
        return out;
    };
    for (name, b) in block.key_blocks() {
        let ids = b.bare_ids();
        if !ids.is_empty() {
            out.insert(name.to_string(), ids);
        }
    }
    out
}

/// Region name -> province ids, resolved through the region's area list.
pub fn regions(vfs: &Vfs) -> HashMap<String, Vec<u32>> {
    let areas = areas(vfs);
    let mut out = HashMap::new();
    let Some(block) = parse_rel(vfs, "map/region.txt") else {
        return out;
    };
    for (name, b) in block.key_blocks() {
        let Some(area_list) = b.get_block("areas") else {
            continue;
        };
        let ids: Vec<u32> = area_list
            .bare_scalars()
            .filter_map(|a| areas.get(a))
            .flatten()
            .copied()
            .collect();
        if !ids.is_empty() {
            out.insert(name.to_string(), ids);
        }
    }
    out
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

    // --- Simple Terrain / Winter parsing (Sprint 11.1/11.2) ---

    #[test]
    fn terrain_palette_and_override_parse() {
        let root = std::env::temp_dir().join("eu_toolkit_terrain_parse_test");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("map")).unwrap();
        std::fs::write(
            root.join("map/terrain.txt"),
            "categories = {\n\
               grasslands = { color = { 90 235 27 } type = plains movement_cost = 1.0 supply_limit = 8 }\n\
               mountain = { color = { 105 24 4 } type = mountains movement_cost = 1.5 defence = 2 local_development_cost = 0.35 }\n\
               marsh = { color = { 13 189 130 } type = marsh terrain_override = { 96 893 } }\n\
               ocean = { is_water = yes type = ocean }\n\
             }\n\
             terrain = {\n\
               grasslands = { type = grasslands color = { 0 } }\n\
               plains     = { type = grasslands color = { 4 } }\n\
               mountain   = { type = mountain color = { 6 } }\n\
               woods      = { type = woods color = { 255 } }\n\
             }\n",
        )
        .unwrap();
        let vfs = Vfs::new(root.to_str().unwrap(), None).unwrap();

        let palette = terrain_palette_types(&vfs);
        assert_eq!(palette.get(&0).map(String::as_str), Some("grasslands"));
        assert_eq!(palette.get(&4).map(String::as_str), Some("grasslands"));
        assert_eq!(palette.get(&6).map(String::as_str), Some("mountain"));
        assert_eq!(palette.get(&255).map(String::as_str), Some("woods"));

        let overrides = terrain_override_map(&vfs);
        assert_eq!(overrides.get(&96).map(String::as_str), Some("marsh"));
        assert_eq!(overrides.get(&893).map(String::as_str), Some("marsh"));

        let loc = crate::loc::LocStore::from_pairs(&[]);
        let cats = terrain_categories(&vfs, &loc);
        let mtn = cats.iter().find(|c| c.key == "mountain").unwrap();
        assert_eq!(mtn.movement_cost, Some(1.5));
        assert_eq!(mtn.defence, Some(2));
        assert_eq!(mtn.local_development_cost, Some(0.35));
        assert!(!mtn.is_water);
        let ocean = cats.iter().find(|c| c.key == "ocean").unwrap();
        assert!(ocean.is_water);
    }

    // S2.7: the byte-surgical edits the terrain property editor emits
    // (setScalar / insertStatement / removeStatement on a category's modeled
    // keys) must round-trip and leave the rest of terrain.txt byte-identical.
    const TERRAIN_CATS_SRC: &str = "categories = {\n\tgrasslands = {\n\t\tcolor = { 90 235 27 }\n\t\ttype = plains\n\t\tsound_type = plains\n\t\tmovement_cost = 1.0\n\t\tsupply_limit = 8\n\t}\n\tmountain = {\n\t\tcolor = { 105 24 4 }\n\t\ttype = mountains\n\t\tmovement_cost = 1.5\n\t\tdefence = 2\n\t\tlocal_development_cost = 0.35\n\t\tterrain_override = { 96 893 }\n\t}\n}\n";

    #[test]
    fn terrain_modifier_change_value_rest_byte_identical() {
        // Change movement_cost 1.5 -> 2.0 on mountain (setScalar).
        let out = crate::mod_writer::apply(
            TERRAIN_CATS_SRC.as_bytes(),
            &crate::mod_writer::Edit::SetScalar {
                path: vec!["categories".into(), "mountain".into(), "movement_cost".into()],
                value: "2.0".into(),
                quoted: false,
            },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("mountain = {\n\t\tcolor = { 105 24 4 }\n\t\ttype = mountains\n\t\tmovement_cost = 2.0\n"));
        // grasslands block + mountain's other keys untouched.
        assert!(text.contains("grasslands = {\n\t\tcolor = { 90 235 27 }\n\t\ttype = plains\n\t\tsound_type = plains\n\t\tmovement_cost = 1.0\n\t\tsupply_limit = 8\n\t}"));
        assert!(text.contains("defence = 2\n\t\tlocal_development_cost = 0.35\n\t\tterrain_override = { 96 893 }"));
    }

    #[test]
    fn terrain_modifier_add_then_remove_is_identity() {
        // Add defence to grasslands (insertStatement), then remove it — identity.
        let added = crate::mod_writer::apply(
            TERRAIN_CATS_SRC.as_bytes(),
            &crate::mod_writer::Edit::InsertStatement {
                block_path: vec!["categories".into(), "grasslands".into()],
                statement: "defence = 1".into(),
            },
        )
        .unwrap();
        let text = String::from_utf8(added.clone()).unwrap();
        assert!(text.contains("defence = 1"), "defence added: {text}");
        // terrain_override on mountain (and everything else) preserved.
        assert!(text.contains("terrain_override = { 96 893 }"));
        let removed = crate::mod_writer::apply(
            &added,
            &crate::mod_writer::Edit::RemoveStatement {
                block_path: vec!["categories".into(), "grasslands".into()],
                key: "defence".into(),
                value: None,
            },
        )
        .unwrap();
        assert_eq!(removed, TERRAIN_CATS_SRC.as_bytes(), "add then remove is identity");
    }

    #[test]
    fn terrain_modifier_remove_percent_key_preserves_block() {
        // Remove local_development_cost from mountain (present -> absent).
        let out = crate::mod_writer::apply(
            TERRAIN_CATS_SRC.as_bytes(),
            &crate::mod_writer::Edit::RemoveStatement {
                block_path: vec!["categories".into(), "mountain".into()],
                key: "local_development_cost".into(),
                value: None,
            },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(!text.contains("local_development_cost"), "removed: {text}");
        assert!(text.contains("defence = 2\n\t\tterrain_override = { 96 893 }"), "neighbors close up: {text}");
        // grasslands entirely untouched.
        assert!(text.contains("grasslands = {\n\t\tcolor = { 90 235 27 }"));
    }

    #[test]
    fn anbennar_terrain_categories_load() {
        if !Path::new(INSTALL).join("map/provinces.bmp").is_file() || !Path::new(ANBENNAR).is_dir() {
            return;
        }
        let vfs = Vfs::new(INSTALL, Some(ANBENNAR)).unwrap();
        let loc = crate::loc::store(&vfs, INSTALL, Some(ANBENNAR));
        let cats = terrain_categories(&vfs, &loc);
        assert!(!cats.is_empty(), "anbennar terrain categories load");
        // Every category resolves is_water and carries a color-derived swatch.
        assert!(cats.iter().any(|c| c.is_water), "some water category");
        assert!(cats.iter().any(|c| c.movement_cost.is_some()), "some category has movement_cost");
    }

    #[test]
    fn real_terrain_palette_and_categories() {
        let Some(install) = real_install() else { return };
        let palette = terrain_palette_types(&install);
        // Vanilla: palette index 0 is grasslands, 15 is ocean.
        assert_eq!(palette.get(&0).map(String::as_str), Some("grasslands"));
        assert_eq!(palette.get(&15).map(String::as_str), Some("ocean"));
        let loc = crate::loc::build(&install);
        let cats = terrain_categories(&install, &loc);
        let g = cats.iter().find(|c| c.key == "grasslands").expect("grasslands category");
        assert_eq!(g.name, "Grasslands");
        assert!(cats.iter().any(|c| c.key == "mountain"));
        assert!(cats.iter().find(|c| c.key == "ocean").unwrap().is_water);
        // Overrides: vanilla assigns 4175 to mountain.
        assert_eq!(terrain_override_map(&install).get(&4175).map(String::as_str), Some("mountain"));
    }

    #[test]
    fn climate_slot_separates_zone_and_winter() {
        let root = std::env::temp_dir().join("eu_toolkit_climate_slot_test");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("map")).unwrap();
        // Province 5 is BOTH arctic and severe_winter — slots must not collide.
        std::fs::write(
            root.join("map/climate.txt"),
            "arctic = { 5 6 }\nsevere_winter = { 5 }\nmild_winter = { 7 }\nimpassable = { 9 }\n",
        )
        .unwrap();
        let vfs = Vfs::new(root.to_str().unwrap(), None).unwrap();
        let zones = climate_slot(&vfs, CLIMATE_ZONE_KEYS);
        assert_eq!(zones.get(&5).map(String::as_str), Some("arctic"));
        assert_eq!(zones.get(&9).map(String::as_str), Some("impassable"));
        assert!(!zones.contains_key(&7)); // winter-only province not a zone
        let winters = climate_slot(&vfs, WINTER_KEYS);
        assert_eq!(winters.get(&5).map(String::as_str), Some("severe_winter"));
        assert_eq!(winters.get(&7).map(String::as_str), Some("mild_winter"));
        assert!(!winters.contains_key(&6)); // zone-only province not a winter
    }

    /// A minimal synthetic install, enough for the group-index assignment logic.
    /// One dir per test (parallel tests must not share a temp dir).
    fn synthetic(name: &str) -> (PathBuf, Vfs) {
        let root = std::env::temp_dir().join(format!("eu_toolkit_mode_data_test_{name}"));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("map")).unwrap();
        std::fs::create_dir_all(root.join("history/provinces")).unwrap();
        std::fs::write(root.join("map/provinces.bmp"), b"x").unwrap();
        std::fs::write(
            root.join("map/definition.csv"),
            "province;red;green;blue;name;x\n\
             1;10;20;30;Aa;x\n\
             2;40;50;60;Bb;x\n\
             3;0;0;0;Sea;x\n",
        )
        .unwrap();
        std::fs::write(
            root.join("history/provinces/1 - Aa.txt"),
            b"owner = SWE\nreligion = catholic\n",
        )
        .unwrap();
        std::fs::write(
            root.join("history/provinces/2 - Bb.txt"),
            b"owner = SWE\nreligion = protestant\n",
        )
        .unwrap();
        // Province 3: a sea tile — no owner, no religion.
        std::fs::write(root.join("history/provinces/3 - Sea.txt"), b"").unwrap();
        let vfs = Vfs::new(root.to_str().unwrap(), None).unwrap();
        (root, vfs)
    }

    /// A minimal install whose one province has a top-level owner and a dated
    /// owner change, for view-at-date derivation.
    fn view_at_date_install(name: &str) -> (PathBuf, Vfs) {
        let root = std::env::temp_dir().join(format!("eu_toolkit_viewdate_test_{name}"));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("map")).unwrap();
        std::fs::create_dir_all(root.join("history/provinces")).unwrap();
        std::fs::write(root.join("map/provinces.bmp"), b"x").unwrap();
        std::fs::write(
            root.join("map/definition.csv"),
            "province;red;green;blue;name;x\n1;10;20;30;Aa;x\n",
        )
        .unwrap();
        std::fs::write(
            root.join("history/provinces/1 - Aa.txt"),
            b"owner = FRA\nreligion = catholic\nbase_tax = 3\n1450.1.1 = { owner = ENG religion = reformed add_base_tax = 2 }\n",
        )
        .unwrap();
        let vfs = Vfs::new(root.to_str().unwrap(), None).unwrap();
        (root, vfs)
    }

    #[test]
    fn province_history_folds_dated_blocks_up_to_date() {
        let (_r, vfs) = view_at_date_install("prov_hist");
        // At the start date, the 1450 block is in the future: base state stands.
        let at_start = province_history_at(&vfs, DEFAULT_START);
        let s = at_start.get(&1).unwrap();
        assert_eq!(s.owner.as_deref(), Some("FRA"));
        assert_eq!(s.religion.as_deref(), Some("catholic"));
        assert_eq!(s.development, Some(3.0));

        // At a later date the dated block applies: new owner/religion + dev bump.
        let later = province_history_at(&vfs, (1453, 1, 1));
        let s = later.get(&1).unwrap();
        assert_eq!(s.owner.as_deref(), Some("ENG"));
        assert_eq!(s.religion.as_deref(), Some("reformed"));
        assert_eq!(s.development, Some(5.0));
    }

    #[test]
    fn political_mode_and_political_payload_are_date_aware() {
        let (_r, vfs) = view_at_date_install("pol_mode");
        let loc = crate::loc::build(&vfs);
        // Political mode: FRA at start, ENG later.
        let start = mode_data_with_overrides_at(&vfs, &loc, "political", &HashMap::new(), DEFAULT_START).unwrap();
        assert_eq!(start.groups[start.values[1] as usize].key, "FRA");
        let later = mode_data_with_overrides_at(&vfs, &loc, "political", &HashMap::new(), (1453, 1, 1)).unwrap();
        assert_eq!(later.groups[later.values[1] as usize].key, "ENG");

        // province_political payload mirrors the same owner-at-date.
        let pp_start = province_political_at(&vfs, DEFAULT_START);
        assert_eq!(pp_start.iter().find(|p| p.id == 1).unwrap().owner.as_deref(), Some("FRA"));
        let pp_later = province_political_at(&vfs, (1453, 1, 1));
        assert_eq!(pp_later.iter().find(|p| p.id == 1).unwrap().owner.as_deref(), Some("ENG"));
    }

    #[test]
    fn ruler_at_date_picks_latest_monarch_le_date() {
        let root = std::env::temp_dir().join("eu_toolkit_viewdate_test_ruler");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("map")).unwrap();
        std::fs::create_dir_all(root.join("history/countries")).unwrap();
        std::fs::create_dir_all(root.join("common/country_tags")).unwrap();
        std::fs::write(root.join("map/provinces.bmp"), b"x").unwrap();
        std::fs::write(root.join("common/country_tags/00.txt"), "FRA = \"countries/France.txt\"\n").unwrap();
        std::fs::create_dir_all(root.join("common/countries")).unwrap();
        std::fs::write(root.join("common/countries/France.txt"), "color = { 1 2 3 }\n").unwrap();
        std::fs::write(
            root.join("history/countries/FRA - France.txt"),
            b"government = monarchy\n1400.1.1 = { monarch = { name = \"Old King\" adm = 1 } }\n1455.1.1 = { monarch = { name = \"New King\" adm = 5 } }\n",
        )
        .unwrap();
        let vfs = Vfs::new(root.to_str().unwrap(), None).unwrap();
        let loc = crate::loc::build(&vfs);

        // At the start date only the 1400 monarch qualifies.
        let d1 = country_details_at(&vfs, &loc, "FRA", DEFAULT_START).unwrap();
        assert_eq!(d1.ruler.as_ref().unwrap().name.as_deref(), Some("Old King"));
        // At a later date the 1455 monarch is the latest ≤ date.
        let d2 = country_details_at(&vfs, &loc, "FRA", (1460, 1, 1)).unwrap();
        assert_eq!(d2.ruler.as_ref().unwrap().name.as_deref(), Some("New King"));
    }

    // --- S3.2: country history timeline + effective-at-date fold -------------

    /// A minimal synthetic install with one country `FRA` whose history file is
    /// `history`. One dir per test (parallel tests must not share a temp dir).
    fn synthetic_country(name: &str, history: &str) -> (PathBuf, Vfs) {
        let root = std::env::temp_dir().join(format!("eu_toolkit_country_hist_test_{name}"));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("map")).unwrap();
        std::fs::create_dir_all(root.join("history/countries")).unwrap();
        std::fs::create_dir_all(root.join("common/country_tags")).unwrap();
        std::fs::create_dir_all(root.join("common/countries")).unwrap();
        std::fs::write(root.join("map/provinces.bmp"), b"x").unwrap();
        std::fs::write(
            root.join("common/country_tags/00.txt"),
            "FRA = \"countries/France.txt\"\n",
        )
        .unwrap();
        std::fs::write(root.join("common/countries/France.txt"), "color = { 1 2 3 }\n").unwrap();
        std::fs::write(root.join("history/countries/FRA - France.txt"), history).unwrap();
        let vfs = Vfs::new(root.to_str().unwrap(), None).unwrap();
        (root, vfs)
    }

    #[test]
    fn country_dated_blocks_surface_in_timeline() {
        // Two blocks share 1500.1.1 (occurrence indexing), one carries an advisor
        // sub-block (reconstructed as an is_block entry), one is post-start.
        let history = "government = monarchy\n\
             1500.1.1 = { government = republic }\n\
             1500.1.1 = { advisor = { name = \"Jean\" type = artist skill = 2 } }\n\
             1600.1.1 = { add_accepted_culture = breton }\n";
        let (_root, vfs) = synthetic_country("timeline_parse", history);
        let loc = crate::loc::build(&vfs);
        let d = country_details_at(&vfs, &loc, "FRA", DEFAULT_START).unwrap();

        assert_eq!(d.dated_blocks.len(), 3);
        assert_eq!(d.dated_blocks[0].date, "1500.1.1");
        assert_eq!(d.dated_blocks[0].occurrence_index, 0);
        assert_eq!(d.dated_blocks[1].date, "1500.1.1");
        assert_eq!(d.dated_blocks[1].occurrence_index, 1);
        // The advisor entry is reconstructed as a block value.
        let advisor = &d.dated_blocks[1].entries[0];
        assert_eq!(advisor.key, "advisor");
        assert!(advisor.is_block);
        assert!(advisor.value.contains("type = artist"));
        // All three blocks are post-start relative to 1444.11.11.
        assert!(d.dated_blocks.iter().all(|b| b.post_start));
    }

    #[test]
    fn country_effective_state_folds_dated_scalars_at_date() {
        let history = "government = monarchy\nreligion = catholic\n\
             add_accepted_culture = gascon\n\
             1510.1.1 = { government = republic religion = protestant add_accepted_culture = breton }\n";
        let (_root, vfs) = synthetic_country("effective_fold", history);
        let loc = crate::loc::build(&vfs);

        // At the start date the 1510 block is post-start → base state only.
        let d0 = country_details_at(&vfs, &loc, "FRA", DEFAULT_START).unwrap();
        assert_eq!(d0.government.as_deref(), Some("monarchy"));
        assert_eq!(d0.religion.as_deref(), Some("catholic"));
        assert_eq!(d0.accepted_cultures, vec!["gascon".to_string()]);
        assert!(d0.dated_blocks[0].post_start);

        // Viewing at 1520 folds the 1510 block: government/religion change and the
        // accepted culture accumulates (S3.2 effective-at-date re-derivation).
        let d1 = country_details_at(&vfs, &loc, "FRA", (1520, 1, 1)).unwrap();
        assert_eq!(d1.government.as_deref(), Some("republic"));
        assert_eq!(d1.religion.as_deref(), Some("protestant"));
        assert_eq!(
            d1.accepted_cultures,
            vec!["gascon".to_string(), "breton".to_string()]
        );
        assert!(!d1.dated_blocks[0].post_start);
    }

    #[test]
    fn advisor_block_edit_round_trips_byte_surgical() {
        use crate::mod_writer::{apply, Edit};
        let src = b"government = monarchy\n\
             1500.1.1 = { advisor = { name = \"Jean\" type = artist skill = 2 date = 1500.1.1 } }\n";
        // Edit the skill inside the dated advisor block (occurrence 0).
        let bumped = apply(
            src,
            &Edit::SetScalar {
                path: vec!["1500.1.1".into(), "advisor".into(), "skill".into()],
                value: "4".into(),
                quoted: false,
            },
        )
        .unwrap();
        let text = String::from_utf8(bumped.clone()).unwrap();
        assert!(text.contains("skill = 4"));
        // Name/type untouched.
        assert!(text.contains("name = \"Jean\""));
        assert!(text.contains("type = artist"));
        // Editing the skill back to 2 restores the file byte-for-byte.
        let back = apply(
            &bumped,
            &Edit::SetScalar {
                path: vec!["1500.1.1".into(), "advisor".into(), "skill".into()],
                value: "2".into(),
                quoted: false,
            },
        )
        .unwrap();
        assert_eq!(back, src);

        // Retyping the advisor also stays surgical.
        let retyped = apply(
            src,
            &Edit::SetScalar {
                path: vec!["1500.1.1".into(), "advisor".into(), "type".into()],
                value: "statesman".into(),
                quoted: false,
            },
        )
        .unwrap();
        assert!(String::from_utf8(retyped).unwrap().contains("type = statesman"));
    }

    #[test]
    fn real_france_timeline_add_advisor_and_delete_round_trip() {
        let Some(vfs) = real_install() else { return };
        let (_name, original) = country_history_file(&vfs, "FRA").expect("FRA history file");
        use crate::mod_writer::{apply, Edit};

        // A date not present in the file (avoids merging into an existing block).
        let date = "1600.6.15";
        let block = format!(
            "{date} = {{ advisor = {{ name = \"Toolkit Test\" type = statesman skill = 3 date = {date} }} }}"
        );
        let added = apply(
            &original,
            &Edit::InsertDatedBlock {
                date: date.into(),
                statement: block,
            },
        )
        .unwrap();
        assert_ne!(added, original, "insert changed the file");

        // The inserted block parses and is addressable as a dated block.
        let parsed = parse_bytes(&added);
        let blocks = crate::province_details::dated_blocks_of(&parsed, DEFAULT_START);
        let inserted = blocks
            .iter()
            .find(|b| b.date == date)
            .expect("inserted dated block present");
        assert_eq!(inserted.entries[0].key, "advisor");
        assert!(inserted.entries[0].value.contains("statesman"));

        // Deleting it restores the original bytes exactly (delete is the inverse
        // of the date-ordered insert — every other byte round-trips).
        let back = apply(
            &added,
            &Edit::RemoveStatement {
                block_path: vec![],
                key: date.into(),
                value: None,
            },
        )
        .unwrap();
        assert_eq!(back, original, "add + delete is byte-identity");
    }

    #[test]
    fn real_france_edit_dated_field_round_trips() {
        let Some(vfs) = real_install() else { return };
        let (_name, original) = country_history_file(&vfs, "FRA").expect("FRA history file");
        use crate::mod_writer::{apply, Edit};

        // France's `1477.1.6 = { add_accepted_culture = burgundian }` is a unique
        // dated block with a single scalar entry — edit it and edit it back.
        let edited = apply(
            &original,
            &Edit::SetScalar {
                path: vec!["1477.1.6".into(), "add_accepted_culture".into()],
                value: "gascon".into(),
                quoted: false,
            },
        )
        .unwrap();
        assert_ne!(edited, original);
        assert!(String::from_utf8_lossy(&edited).contains("add_accepted_culture = gascon"));
        let back = apply(
            &edited,
            &Edit::SetScalar {
                path: vec!["1477.1.6".into(), "add_accepted_culture".into()],
                value: "burgundian".into(),
                quoted: false,
            },
        )
        .unwrap();
        assert_eq!(back, original, "edit + edit-back is byte-identity");
    }

    #[test]
    fn anbennar_country_timeline_smoke() {
        if !Path::new(INSTALL).join("map/provinces.bmp").is_file() || !Path::new(ANBENNAR).is_dir() {
            return;
        }
        let vfs = Vfs::new(INSTALL, Some(ANBENNAR)).unwrap();
        let loc = crate::loc::build(&vfs);
        // Scan Anbennar's country tags; at least one country must build a
        // non-empty history timeline without panicking on a total conversion.
        let countries = country_list(&vfs, &loc);
        let mut any_dated = false;
        let mut checked = 0;
        for c in countries.iter().take(80) {
            if let Ok(d) = country_details_at(&vfs, &loc, &c.tag, DEFAULT_START) {
                checked += 1;
                if !d.dated_blocks.is_empty() {
                    any_dated = true;
                    // Occurrence indices are consistent per date.
                    for b in &d.dated_blocks {
                        assert!(b.date.split('.').count() == 3);
                    }
                }
            }
        }
        assert!(checked > 0, "no Anbennar countries resolved");
        assert!(any_dated, "no Anbennar country had dated history blocks");
    }

    #[test]
    fn group_index_assignment_dedups_and_marks_none() {
        let (_root, vfs) = synthetic("groups");
        let loc = crate::loc::build(&vfs);
        let data = mode_data(&vfs, &loc, "political").unwrap();

        assert_eq!(data.kind, "categorical");
        // Both owned provinces share one SWE group.
        assert_eq!(data.groups.len(), 1);
        assert_eq!(data.groups[0].key, "SWE");
        assert_eq!(data.values[1], 0);
        assert_eq!(data.values[2], 0);
        // The sea tile has no owner -> NONE sentinel.
        assert_eq!(data.values[3], NONE_GROUP);
        // Buffer is province-id-indexed and bounded by the province count.
        assert_eq!(data.max_id, 3);
        assert_eq!(data.values.len(), 4);

        // Two distinct religions -> two groups, index 0 for the first seen.
        let rel = mode_data(&vfs, &loc, "religion").unwrap();
        assert_eq!(rel.groups.len(), 2);
        assert_ne!(rel.values[1], rel.values[2]);
        assert_eq!(rel.values[3], NONE_GROUP);
    }

    #[test]
    fn wire_round_trips_header_and_values() {
        let (_root, vfs) = synthetic("wire");
        let loc = crate::loc::build(&vfs);
        let data = mode_data(&vfs, &loc, "political").unwrap();
        let wire = data.to_wire();

        let header_len = u32::from_le_bytes(wire[..4].try_into().unwrap()) as usize;
        let header: serde_json::Value =
            serde_json::from_slice(&wire[4..4 + header_len]).unwrap();
        assert_eq!(header["kind"], "categorical");
        assert_eq!(header["maxId"], 3);
        assert_eq!(header["groups"][0]["key"], "SWE");

        let values = &wire[4 + header_len..];
        assert_eq!(values.len(), 4 * 2);
        // Province 3 (sea) is the NONE sentinel, little-endian.
        assert_eq!(&values[6..8], &[0xff, 0xff]);
    }

    #[test]
    fn real_mode_data_when_installed() {
        let Some(vfs) = real_install() else { return };
        let loc = crate::loc::build(&vfs);

        // Religion: catholic exists with the game's color; Stockholm(1) is catholic.
        let rel = mode_data(&vfs, &loc, "religion").unwrap();
        let colors = religion_colors(&vfs);
        let cat = rel
            .groups
            .iter()
            .find(|g| g.key == "catholic")
            .expect("catholic religion group");
        assert_eq!(cat.color, colors["catholic"]);
        assert_eq!(cat.label, "Catholic");
        let g1 = rel.values[1];
        assert_ne!(g1, NONE_GROUP);
        assert_eq!(rel.groups[g1 as usize].key, "catholic");
        // Ocean tiles have no religion -> some NONE entries exist.
        assert!(rel.values.iter().any(|&v| v == NONE_GROUP));
        // Buffer length is the province-count bound.
        assert_eq!(rel.values.len(), rel.max_id as usize + 1);

        // Political: Stockholm(1) -> SWE group, label "Sweden".
        let pol = mode_data(&vfs, &loc, "political").unwrap();
        assert!(pol.groups.len() > 300);
        let s = pol.values[1];
        assert_eq!(pol.groups[s as usize].key, "SWE");
        assert_eq!(pol.groups[s as usize].label, "Sweden");

        // Development is a gradient with a decode scale.
        let dev = mode_data(&vfs, &loc, "development").unwrap();
        assert_eq!(dev.kind, "gradient");
        assert_eq!(dev.value_scale, Some(10.0));
        assert!(dev.groups.is_empty());

        // Raster modes carry no group model.
        let ter = mode_data(&vfs, &loc, "terrain").unwrap();
        assert_eq!(ter.kind, "raster");
        assert!(ter.groups.is_empty());
        assert!(ter.values.is_empty());

        // Every categorical/gradient mode builds without error.
        for mode in [
            "provinces",
            "culture",
            "trade_goods",
            "trade_nodes",
            "areas",
            "regions",
            "climate",
        ] {
            let d = mode_data(&vfs, &loc, mode).unwrap();
            assert!(!d.values.is_empty(), "mode {mode} produced no values");
        }
    }

    #[test]
    fn real_province_political_payload() {
        let Some(vfs) = real_install() else { return };
        let payload = province_political(&vfs);
        assert!(payload.len() > 3000, "only {} provinces", payload.len());
        let by_id: HashMap<u32, &ProvincePolitical> =
            payload.iter().map(|p| (p.id, p)).collect();

        // Uppland (1): Swedish land, has a real history file, not water/wasteland.
        let uppland = by_id[&1];
        assert_eq!(uppland.owner.as_deref(), Some("SWE"));
        assert!(uppland.file.starts_with("history/provinces/1"));
        assert!(uppland.file.to_lowercase().contains("uppland"));
        assert!(!uppland.water);
        assert!(!uppland.wasteland);
        // Owned provinces are cored by their owner in 1444.
        assert!(uppland.cores.iter().any(|c| c == "SWE"));

        // At least one sea province is flagged water (e.g. it exists in default.map).
        assert!(payload.iter().any(|p| p.water), "no water provinces flagged");
        // Every record's file lives under history/provinces.
        assert!(payload.iter().all(|p| p.file.starts_with("history/provinces/")));
    }

    #[test]
    fn synthetic_province_political_flags() {
        let root = std::env::temp_dir().join("eu_toolkit_prov_pol_test_synth");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("map")).unwrap();
        std::fs::create_dir_all(root.join("history/provinces")).unwrap();
        std::fs::write(root.join("map/provinces.bmp"), b"x").unwrap();
        std::fs::write(
            root.join("map/definition.csv"),
            "province;red;green;blue;name;x\n1;1;1;1;Land;x\n2;2;2;2;Sea;x\n3;3;3;3;Waste;x\n",
        )
        .unwrap();
        std::fs::write(root.join("map/default.map"), b"sea_starts = { 2 }\nlakes = {}\n").unwrap();
        std::fs::write(root.join("map/climate.txt"), b"impassable = { 3 }\n").unwrap();
        std::fs::write(
            root.join("history/provinces/1 - Land.txt"),
            b"owner = FRA\ncontroller = FRA\nadd_core = FRA\nadd_core = ENG\nreligion = catholic\n\
              base_tax = 3\nbase_production = 2\nbase_manpower = 1\n",
        )
        .unwrap();
        // Province 3 (wasteland) has partial dev keys to exercise the None case.
        std::fs::write(
            root.join("history/provinces/3 - Waste.txt"),
            b"base_tax = 5\n",
        )
        .unwrap();
        let vfs = Vfs::new(root.to_str().unwrap(), None).unwrap();

        let payload = province_political(&vfs);
        let by_id: HashMap<u32, &ProvincePolitical> = payload.iter().map(|p| (p.id, p)).collect();

        let land = by_id[&1];
        assert_eq!(land.owner.as_deref(), Some("FRA"));
        assert_eq!(land.controller.as_deref(), Some("FRA"));
        assert_eq!(land.cores, vec!["FRA".to_string(), "ENG".to_string()]);
        assert!(!land.water && !land.wasteland);
        assert_eq!(land.file, "history/provinces/1 - Land.txt");
        // Dev components are carried in the bulk payload for airbrush painting (9.1).
        assert_eq!(land.base_tax, Some(3.0));
        assert_eq!(land.base_production, Some(2.0));
        assert_eq!(land.base_manpower, Some(1.0));

        // Sea province: flagged water, no history file -> synthesized name.
        let sea = by_id[&2];
        assert!(sea.water);
        assert!(sea.owner.is_none());
        assert_eq!(sea.file, "history/provinces/2 - Sea.txt");
        // No history file -> all dev components absent (None), not zero.
        assert_eq!(sea.base_tax, None);
        assert_eq!(sea.base_production, None);
        assert_eq!(sea.base_manpower, None);

        // Wasteland: flagged impassable; partial dev keys -> present/absent mix.
        let waste = by_id[&3];
        assert!(waste.wasteland);
        assert_eq!(waste.base_tax, Some(5.0));
        assert_eq!(waste.base_production, None);
        assert_eq!(waste.base_manpower, None);
    }

    // --- Development painting: dev edit shapes round-trip byte-surgically (9.4) ---
    // Mirrors the add/remove-province round-trip precedent (edits.rs), asserting the
    // brush's generated setScalar / insertStatement edits touch ONLY the three dev
    // keys and leave every other byte of the province file identical.
    #[test]
    fn dev_paint_edits_round_trip_only_dev_keys() {
        use crate::edits::{apply_queue, TypedEdit};

        let root = std::env::temp_dir().join("eu_toolkit_dev_paint_roundtrip_test");
        let _ = std::fs::remove_dir_all(&root);
        let base = root.join("base");
        let project = root.join("project");
        std::fs::create_dir_all(base.join("history/provinces")).unwrap();
        std::fs::create_dir_all(&project).unwrap();
        // A province with base_tax present (raise → setScalar) and base_production
        // present, but base_manpower ABSENT (raise → insertStatement, created key).
        // A trailing comment + Windows-1252 byte must survive untouched.
        let original =
            b"owner = FRA\nbase_tax = 3\nbase_production = 2\nculture = fran\xe7ais # keep\n".to_vec();
        std::fs::write(base.join("history/provinces/7 - Test.txt"), &original).unwrap();

        let vfs = Vfs::new(base.to_str().unwrap(), None).unwrap();
        let file = "history/provinces/7 - Test.txt".to_string();
        let edits = vec![
            // Raise base_tax 3 -> 5 (present: setScalar).
            TypedEdit::SetScalar {
                file: file.clone(),
                path: vec!["base_tax".into()],
                value: "5".into(),
                quoted: false,
            },
            // Create the missing base_manpower at the floor (absent: insertStatement).
            TypedEdit::InsertStatement {
                file: file.clone(),
                block_path: vec![],
                statement: "base_manpower = 1".into(),
            },
        ];
        let written = apply_queue(&vfs, &project, &edits).unwrap();
        assert!(written.contains(&file));

        let out = std::fs::read(project.join(&file)).unwrap();
        let text = String::from_utf8_lossy(&out);
        // Only the three dev keys changed; everything else is byte-identical.
        assert!(text.contains("base_tax = 5"));
        assert!(text.contains("base_production = 2"));
        assert!(text.contains("base_manpower = 1"));
        assert!(text.contains("owner = FRA"));
        // The comment + non-ASCII byte round-trip unchanged (byte-surgical splice).
        assert!(out.windows(b"# keep\n".len()).any(|w| w == b"# keep\n"));
        assert!(out.contains(&0xe7)); // Windows-1252 'ç' preserved as a raw byte
    }

    #[test]
    fn real_country_details_identity_and_government() {
        let Some(vfs) = real_install() else { return };
        let loc = crate::loc::build(&vfs);
        let d = country_details(&vfs, &loc, "FRA").unwrap();

        // Identity fields parse from the real files.
        assert_eq!(d.government.as_deref(), Some("monarchy"));
        assert_eq!(d.religion.as_deref(), Some("catholic"));
        assert_eq!(d.primary_culture.as_deref(), Some("cosmopolitan_french"));
        assert_eq!(d.technology_group.as_deref(), Some("western"));
        assert_eq!(d.graphical_culture.as_deref(), Some("westerngfx"));
        assert_eq!(d.national_focus.as_deref(), Some("DIP"));
        assert!(d.color.is_some());
        // revolutionary_colors are palette indices (small ints), not 0-255 RGB.
        let revo = d.revolutionary_colors.expect("FRA has revolutionary_colors");
        assert!(revo.iter().all(|&i| (0..=64).contains(&i)), "indices, got {revo:?}");
        // Reforms/accepted cultures/rivals collected from top-level statements.
        assert!(d.government_reforms.iter().any(|r| r == "feudal_france_reform"));
        assert!(d.accepted_cultures.iter().any(|c| c == "gascon"));
        assert!(d.historical_rivals.iter().any(|t| t == "HAB"));
        // File paths point at the real files.
        assert!(d.country_file.as_deref().unwrap().ends_with("France.txt"));
        assert!(d.history_file.as_deref().unwrap().contains("FRA"));
    }

    #[test]
    fn real_grouped_lists_and_countries() {
        let Some(vfs) = real_install() else { return };
        let loc = crate::loc::build(&vfs);

        let religions = religion_list(&vfs, &loc);
        let cat = religions.iter().find(|r| r.key == "catholic").expect("catholic");
        assert_eq!(cat.group, "christian");
        assert!(cat.color.is_some());
        // Group helper blocks are not mistaken for religions.
        assert!(!religions.iter().any(|r| r.key == "religious_schools"));

        let cultures = culture_list(&vfs, &loc);
        assert!(cultures.iter().any(|c| c.key == "cosmopolitan_french"));
        assert!(!cultures.iter().any(|c| c.key == "male_names"));

        let countries = country_list(&vfs, &loc);
        let fra = countries.iter().find(|c| c.tag == "FRA").expect("FRA");
        assert_eq!(fra.name, "France");
        assert!(fra.color.is_some());
    }

    #[test]
    fn convert_flag_produces_128_tga_and_png() {
        // Encode a tiny PNG, convert it, and sanity-check the TGA header.
        let dir = std::env::temp_dir().join("eu_toolkit_flag_conv_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("in.png");
        let raw: Vec<u8> = (0..4 * 4 * 3).map(|i| (i % 256) as u8).collect();
        crate::map_renderer::encode_png(&raw, 4, 4)
            .map(|png| std::fs::write(&src, png).unwrap())
            .unwrap();

        let (tga, png) = convert_flag(src.to_str().unwrap()).unwrap();
        // Uncompressed true-color TGA: image type 2 (byte 2), 128x128, 24bpp.
        assert_eq!(tga[2], 2, "image type should be uncompressed true-color");
        let w = u16::from_le_bytes([tga[12], tga[13]]);
        let h = u16::from_le_bytes([tga[14], tga[15]]);
        assert_eq!((w, h), (128, 128));
        assert_eq!(tga[16], 24, "24 bits per pixel");
        // PNG preview decodes back to 128x128.
        let decoded = image::load_from_memory(&png).unwrap();
        assert_eq!(decoded.width(), 128);
        assert_eq!(decoded.height(), 128);
    }

    #[test]
    fn anbennar_mode_data_smoke() {
        if !Path::new(INSTALL).join("map/provinces.bmp").is_file() || !Path::new(ANBENNAR).is_dir()
        {
            return;
        }
        let vfs = Vfs::new(INSTALL, Some(ANBENNAR)).unwrap();
        let loc = crate::loc::build(&vfs);
        let pol = mode_data(&vfs, &loc, "political").unwrap();
        assert!(pol.groups.len() > 100, "only {} groups", pol.groups.len());
        // Anbennar replaces vanilla countries; SWE shouldn't own land there.
        assert!(!pol.groups.iter().any(|g| g.key == "SWE"));
    }

    // --- Sprint 1.2 (2nd half): ruler / queen / heir / name pools ---------

    /// A FRA-shaped history file: monarch + queen in one dated block, heir in
    /// another, personalities (with a clear) in the heir's block, a leader inside
    /// the monarch block, and a post-start block that must be ignored.
    const HISTORY: &[u8] = br#"government = monarchy
1422.10.22 = {
	monarch = {
		name = "Charles VII"
		dynasty = "de Valois"
		birth_date = 1403.2.22
		adm = 4
		dip = 2
		mil = 4
		leader = { name = "Charles VII" type = general fire = 2 shock = 1 manuever = 3 siege = 0 }
	}
	queen = {
		country_of_origin = PRO
		name = "Marie"
		dynasty = "d'Anjou"
		female = yes
		adm = 4
		dip = 4
		mil = 4
	}
}
1423.7.3 = {
	heir = {
		name = "Louis"
		monarch_name = "Louis XI"
		dynasty = "de Valois"
		birth_date = 1423.7.3
		claim = 95
		adm = 4
		dip = 3
		mil = 2
	}
	clear_scripted_personalities = yes
	add_heir_personality = intricate_web_weaver_personality
	add_ruler_personality = inspiring_leader_personality
	add_ruler_personality = well_advised_personality
	add_queen_personality = zealot_personality
}
1461.7.22 = { monarch = { name = "Too Late" } }
"#;

    #[test]
    fn characters_parse_ruler_queen_heir() {
        let h = parse_bytes(HISTORY);
        let (ruler, reason, queen, heir) = characters_at_start(&h);

        let r = ruler.expect("ruler present");
        assert_eq!(r.date, "1422.10.22");
        assert_eq!(r.name.as_deref(), Some("Charles VII"));
        assert_eq!(r.dynasty.as_deref(), Some("de Valois"));
        assert_eq!((r.adm, r.dip, r.mil), (Some(4), Some(2), Some(4)));
        assert_eq!(r.birth_date.as_deref(), Some("1403.2.22"));
        assert!(reason.is_none());
        // Leader inside the monarch block (note the game's "manuever" spelling).
        let l = r.leader.expect("ruler leader");
        assert_eq!((l.fire, l.shock, l.manuever, l.siege), (Some(2), Some(1), Some(3), Some(0)));
        // Ruler personalities from the dated block (accumulated after the clear).
        let keys: Vec<&str> = r.personalities.iter().map(|p| p.key.as_str()).collect();
        assert_eq!(keys, ["inspiring_leader_personality", "well_advised_personality"]);
        assert_eq!(r.personalities[0].date, "1423.7.3");

        let q = queen.expect("queen present");
        assert_eq!(q.date, "1422.10.22");
        assert_eq!(q.name.as_deref(), Some("Marie"));
        assert_eq!(q.country_of_origin.as_deref(), Some("PRO"));
        assert!(q.female);
        assert_eq!(q.personalities.iter().map(|p| p.key.as_str()).collect::<Vec<_>>(), ["zealot_personality"]);

        let hr = heir.expect("heir present");
        assert_eq!(hr.date, "1423.7.3");
        assert_eq!(hr.name.as_deref(), Some("Louis"));
        assert_eq!(hr.monarch_name.as_deref(), Some("Louis XI"));
        assert_eq!(hr.claim, Some(95));
        assert_eq!(hr.personalities.iter().map(|p| p.key.as_str()).collect::<Vec<_>>(), ["intricate_web_weaver_personality"]);
    }

    #[test]
    fn no_ruler_reason_for_pu_junior() {
        // Only a post-start monarch (Sweden's shape): no ruler, PU-style reason.
        let h = parse_bytes(b"government = monarchy\n1448.6.20 = { monarch = { name = \"Karl VIII\" } }\n");
        let (ruler, reason, _, _) = characters_at_start(&h);
        assert!(ruler.is_none());
        assert!(reason.unwrap().contains("junior partner"));

        // No monarch anywhere.
        let h2 = parse_bytes(b"government = republic\n");
        let (_, reason2, _, _) = characters_at_start(&h2);
        assert!(reason2.unwrap().contains("No ruler"));
    }

    #[test]
    fn name_pools_preserve_raw_tokens_and_latin1() {
        // Windows-1252 é (0xE9) inside a monarch name + a negative female weight;
        // quoted and bare leader tokens.
        let mut src = b"monarch_names = {\n\t\"Louis #10\" = 160\n\t\"Fran".to_vec();
        src.push(0xE9); // é
        src.extend_from_slice(b"ois #0\" = 40\n\t\"Jeanne #0\" = -10\n}\n");
        src.extend_from_slice(b"leader_names = { \"de Broglie\" Achille \"de La Motte\" }\n");
        let pools = name_pools(&src);
        assert_eq!(pools.monarch_names.len(), 3);
        assert_eq!(pools.monarch_names[0].name, "\"Louis #10\"");
        assert_eq!(pools.monarch_names[0].weight, "160");
        assert_eq!(pools.monarch_names[1].name, "\"Fran\u{e9}ois #0\"");
        assert_eq!(pools.monarch_names[2].weight, "-10");
        assert_eq!(pools.leader_names, vec!["\"de Broglie\"", "Achille", "\"de La Motte\""]);
    }

    // --- mod_writer round-trips for the frontend's edit shapes ------------

    /// Applies the frontend's exact edit shapes and asserts they round-trip; the
    /// writer is `crate::mod_writer` (called here only, never modified).
    #[test]
    fn ruler_stat_set_nested_path() {
        use crate::mod_writer::{apply, Edit};
        let out = apply(
            HISTORY,
            &Edit::SetScalar {
                path: vec!["1422.10.22".into(), "monarch".into(), "adm".into()],
                value: "6".into(),
                quoted: false,
            },
        )
        .unwrap();
        let t = String::from_utf8(out).unwrap();
        assert!(t.contains("adm = 6"));
        assert!(t.contains("name = \"Charles VII\""));
        assert!(t.contains("name = \"Too Late\"")); // post-start block untouched
    }

    #[test]
    fn personality_add_and_remove_at_dated_block() {
        use crate::mod_writer::{apply, Edit};
        // Add into the ruler's own dated block.
        let added = apply(
            HISTORY,
            &Edit::InsertStatement {
                block_path: vec!["1422.10.22".into()],
                statement: "add_ruler_personality = tolerant_personality".into(),
            },
        )
        .unwrap();
        assert!(String::from_utf8(added).unwrap().contains("add_ruler_personality = tolerant_personality"));
        // Remove the existing one from the block it lives in (value-filtered).
        let removed = apply(
            HISTORY,
            &Edit::RemoveStatement {
                block_path: vec!["1423.7.3".into()],
                key: "add_ruler_personality".into(),
                value: Some("well_advised_personality".into()),
            },
        )
        .unwrap();
        let t = String::from_utf8(removed).unwrap();
        assert!(!t.contains("well_advised_personality"));
        assert!(t.contains("inspiring_leader_personality")); // sibling kept
    }

    #[test]
    fn heir_block_insert_and_delete_preserve_1252() {
        use crate::mod_writer::{apply, Edit};
        // A file with a Windows-1252 byte that must survive both edits.
        let mut src = b"government = monarchy # caf".to_vec();
        src.push(0xE9);
        src.extend_from_slice(b"\n1440.1.1 = {\n\tmonarch = {\n\t\tname = \"A\"\n\t}\n}\n");
        // Insert a whole heir block into the monarch's dated block.
        let with_heir = apply(
            &src,
            &Edit::InsertStatement {
                block_path: vec!["1440.1.1".into()],
                statement: "heir = {\n\tname = \"B\"\n\tmonarch_name = \"B II\"\n\tclaim = 90\n\tbirth_date = 1440.1.1\n}".into(),
            },
        )
        .unwrap();
        let t = String::from_utf8_lossy(&with_heir);
        assert!(t.contains("heir = {"));
        assert!(t.contains("claim = 90"));
        assert!(with_heir.windows(2).any(|w| w == b"\xE9\n"), "high byte survives insert");
        // Delete it again.
        let back = apply(
            &with_heir,
            &Edit::RemoveStatement {
                block_path: vec!["1440.1.1".into()],
                key: "heir".into(),
                value: None,
            },
        )
        .unwrap();
        assert_eq!(back, src, "delete restores the original bytes exactly");
    }

    #[test]
    fn queen_block_add_and_remove() {
        use crate::mod_writer::{apply, Edit};
        let src = b"1440.1.1 = {\n\tmonarch = {\n\t\tname = \"A\"\n\t}\n}\n";
        let added = apply(
            src,
            &Edit::InsertStatement {
                block_path: vec!["1440.1.1".into()],
                statement: "queen = {\n\tname = \"Q\"\n\tcountry_of_origin = ARA\n\tfemale = yes\n}".into(),
            },
        )
        .unwrap();
        assert!(String::from_utf8(added.clone()).unwrap().contains("country_of_origin = ARA"));
        let removed = apply(
            &added,
            &Edit::RemoveStatement {
                block_path: vec!["1440.1.1".into()],
                key: "queen".into(),
                value: None,
            },
        )
        .unwrap();
        assert_eq!(removed, src);
    }

    #[test]
    fn monarch_names_insert_and_remove() {
        use crate::mod_writer::{apply, Edit};
        let src = b"monarch_names = {\n\t\"Louis #10\" = 160\n\t\"Charles #5\" = 80\n}\n";
        // Add a new weighted entry.
        let added = apply(
            src,
            &Edit::InsertStatement {
                block_path: vec!["monarch_names".into()],
                statement: "\"Henri #4\" = 30".into(),
            },
        )
        .unwrap();
        assert!(String::from_utf8(added).unwrap().contains("\"Henri #4\" = 30"));
        // Remove by its quoted key (matches the file token including quotes).
        let removed = apply(
            src,
            &Edit::RemoveStatement {
                block_path: vec!["monarch_names".into()],
                key: "\"Charles #5\"".into(),
                value: None,
            },
        )
        .unwrap();
        let t = String::from_utf8(removed).unwrap();
        assert!(!t.contains("Charles #5"));
        assert!(t.contains("\"Louis #10\" = 160"));
    }

    #[test]
    fn historical_list_reorder_via_setblock() {
        use crate::mod_writer::{apply, Edit};
        let src = b"historical_idea_groups = {\n\taristocracy_ideas\n\treligious_ideas\n}\n";
        let out = apply(
            src,
            &Edit::SetBlock {
                path: vec!["historical_idea_groups".into()],
                value: "religious_ideas aristocracy_ideas offensive_ideas".into(),
            },
        )
        .unwrap();
        let t = String::from_utf8(out).unwrap();
        assert!(t.contains("{ religious_ideas aristocracy_ideas offensive_ideas }"));
    }

    #[test]
    fn ideas_scaffold_parses_back() {
        // The unique-ideas scaffold the panel appends must parse into a valid set.
        let scaffold = "TST_ideas = {\n\tstart = {\n\t\tland_morale = 0.1\n\t}\n\tbonus = {\n\t\tdiscipline = 0.05\n\t}\n\ttrigger = {\n\t\ttag = TST\n\t}\n\tfree = yes\n\ttst_idea_1 = { infantry_power = 0.1 }\n\ttst_idea_2 = { tax_income = 1 }\n}\n";
        let b = parse_bytes(scaffold.as_bytes());
        let set = b.get_block("TST_ideas").expect("set parses");
        assert!(set.get_block("start").is_some());
        assert!(set.get_block("bonus").is_some());
        assert!(set.get_block("trigger").unwrap().get_scalar("tag") == Some("TST"));
        assert_eq!(set.get_scalar("free"), Some("yes"));
        assert!(set.get_block("tst_idea_1").is_some());
    }

    #[test]
    fn real_ruler_and_ideas_spot_checks() {
        let Some(vfs) = real_install() else { return };
        let loc = crate::loc::build(&vfs);

        // FRA has a full ruler, queen, and heir at start.
        let fra = country_details(&vfs, &loc, "FRA").unwrap();
        let r = fra.ruler.expect("FRA ruler");
        assert_eq!(r.name.as_deref(), Some("Charles VII"));
        assert!(fra.queen.is_some());
        assert!(fra.heir.is_some());
        // FRA ships name pools + historical setup in its country file.
        assert!(!fra.name_pools.monarch_names.is_empty());
        assert!(fra.historical_idea_groups.iter().any(|g| g == "economic_ideas"));
        assert!(!fra.historical_units.is_empty());
        // FRA has a unique idea set with a known source file + localized names.
        let ideas = fra.ideas.expect("FRA ideas");
        assert!(ideas.source_file.starts_with("common/ideas/"));
        assert!(ideas.ideas.len() >= 7);

        // SWE: no 1444 ruler (PU junior), reason present.
        let swe = country_details(&vfs, &loc, "SWE").unwrap();
        assert!(swe.ruler.is_none());
        assert!(swe.ruler_reason.is_some());

        // Idea-group + unit pick lists are non-empty.
        assert!(idea_group_list(&vfs, &loc).iter().any(|g| g.key == "offensive_ideas"));
        assert!(!unit_list(&vfs).is_empty());
    }

    // --- Sprint 5: religion details / editing round-trips -----------------

    /// A two-group religions fixture (mirrors 00_religion.txt's shape): group-level
    /// keys, two religions in one group with unmodeled content, a second group.
    const RELIGIONS: &[u8] = br#"christian = {
	defender_of_faith = yes
	can_form_personal_unions = yes

	catholic = {
		color = { 204 204 0 }
		icon = 1
		country = {
			tolerance_own = 1
		}
		hre_religion = yes
		heretic = { BOGOMILIST WALDENSIAN }
	}
	orthodox = {
		color = { 178 127 0 }
		icon = 4
		misguided_heretic = yes
		orthodox_icons = {
			patriarch_authority = 0.1
		}
		heretic = { OLD_BELIEVER MOLOKAN }
	}
	crusade_name = CRUSADE
}
muslim = {
	sunni = {
		color = { 0 153 0 }
		icon = 5
	}
	crusade_name = JIHAD
}
"#;

    #[test]
    fn extract_named_block_returns_exact_text() {
        let text = extract_named_block(RELIGIONS, &["christian", "orthodox"]).unwrap();
        assert!(text.starts_with("orthodox = {"));
        assert!(text.trim_end().ends_with('}'));
        // Unmodeled nested content is captured verbatim.
        assert!(text.contains("orthodox_icons = {"));
        assert!(text.contains("patriarch_authority = 0.1"));
        assert!(text.contains("OLD_BELIEVER MOLOKAN"));
        // The sibling's content is NOT included.
        assert!(!text.contains("catholic"));
        assert!(!text.contains("crusade_name"));
    }

    #[test]
    fn religion_details_parse_synthetic() {
        // Parse the fixture directly (religion_details needs a Vfs; here we test
        // the field extraction against a hand-parsed block).
        let root = parse_bytes(RELIGIONS);
        let g = root.get_block("christian").unwrap();
        let rb = g.get_block("catholic").unwrap();
        assert_eq!(three_ints(rb.get_block("color").unwrap()), Some([204, 204, 0]));
        assert_eq!(rb.get_scalar("icon"), Some("1"));
        assert!(RELIGION_FEATURES.contains(&"hre_religion"));
        assert_eq!(rb.get_scalar("hre_religion"), Some("yes"));
        let heretics: Vec<&str> = rb.get_block("heretic").unwrap().bare_scalars().collect();
        assert_eq!(heretics, vec!["BOGOMILIST", "WALDENSIAN"]);
    }

    #[test]
    fn religion_recolor_roundtrip_byte_identical() {
        use crate::mod_writer::{apply, Edit};
        let out = apply(
            RELIGIONS,
            &Edit::SetBlock {
                path: vec!["christian".into(), "catholic".into(), "color".into()],
                value: "10 20 30".into(),
            },
        )
        .unwrap();
        let t = String::from_utf8(out).unwrap();
        assert!(t.contains("color = { 10 20 30 }"));
        // Everything else byte-identical: siblings, group keys, second group.
        assert!(t.contains("color = { 178 127 0 }")); // orthodox untouched
        assert!(t.contains("color = { 0 153 0 }")); // sunni untouched
        assert!(t.contains("crusade_name = CRUSADE"));
        assert!(t.contains("tolerance_own = 1"));
        assert!(t.contains("heretic = { BOGOMILIST WALDENSIAN }"));
    }

    #[test]
    fn religion_heretics_add_remove_roundtrip() {
        use crate::mod_writer::{apply, Edit};
        // Add a heretic (whole-block set).
        let added = apply(
            RELIGIONS,
            &Edit::SetBlock {
                path: vec!["christian".into(), "catholic".into(), "heretic".into()],
                value: "BOGOMILIST WALDENSIAN FRATICELLI".into(),
            },
        )
        .unwrap();
        let t = String::from_utf8(added).unwrap();
        assert!(t.contains("heretic = { BOGOMILIST WALDENSIAN FRATICELLI }"));
        // orthodox heretic list untouched.
        assert!(t.contains("heretic = { OLD_BELIEVER MOLOKAN }"));
    }

    #[test]
    fn religion_tolerance_modifier_edit_roundtrip() {
        use crate::mod_writer::{apply, Edit};
        let out = apply(
            RELIGIONS,
            &Edit::SetBlock {
                path: vec!["christian".into(), "catholic".into(), "country".into()],
                value: "tolerance_own = 2 tolerance_heretic = -1".into(),
            },
        )
        .unwrap();
        let t = String::from_utf8(out).unwrap();
        assert!(t.contains("country = { tolerance_own = 2 tolerance_heretic = -1 }"));
        // hre_religion (a sibling key of country) preserved.
        assert!(t.contains("hre_religion = yes"));
    }

    #[test]
    fn religion_group_move_preserves_unmodeled_content() {
        use crate::mod_writer::{apply_all, Edit};
        // Move orthodox from christian into muslim, preserving its unmodeled
        // orthodox_icons block and heretic list byte-for-byte.
        let block_text = extract_named_block(RELIGIONS, &["christian", "orthodox"]).unwrap();
        let out = apply_all(
            RELIGIONS,
            &[
                Edit::RemoveStatement {
                    block_path: vec!["christian".into()],
                    key: "orthodox".into(),
                    value: None,
                },
                Edit::InsertStatement {
                    block_path: vec!["muslim".into()],
                    statement: block_text,
                },
            ],
        )
        .unwrap();
        let t = String::from_utf8(out).unwrap();
        // orthodox now under muslim, with its unmodeled block intact.
        assert!(t.contains("orthodox_icons = {"));
        assert!(t.contains("patriarch_authority = 0.1"));
        assert!(t.contains("OLD_BELIEVER MOLOKAN"));
        // catholic and christian group keys still present.
        assert!(t.contains("catholic = {"));
        assert!(t.contains("crusade_name = CRUSADE"));
        // orthodox appears exactly once (moved, not duplicated).
        assert_eq!(t.matches("orthodox = {").count(), 1);
        // The moved block sits inside muslim (after sunni, before muslim's close).
        let muslim_at = t.find("muslim = {").unwrap();
        assert!(t[muslim_at..].contains("orthodox = {"));
    }

    #[test]
    fn created_religion_scaffold_parses_and_inserts() {
        use crate::mod_writer::{apply, Edit};
        // The exact scaffold the frontend inserts into a group (authored at col 0).
        let scaffold = "zunist = {\n\tcolor = { 128 64 200 }\n\ticon = 5\n\tcountry = {\n\t\ttolerance_own = 2\n\t\ttolerance_heretic = -1\n\t\ttolerance_heathen = -2\n\t}\n\theretic = { }\n}";
        // It parses into a valid religion on its own.
        let parsed = parse_bytes(scaffold.as_bytes());
        let z = parsed.get_block("zunist").expect("zunist parses");
        assert_eq!(three_ints(z.get_block("color").unwrap()), Some([128, 64, 200]));
        assert_eq!(z.get_scalar("icon"), Some("5"));
        assert!(z.get_block("country").unwrap().get_scalar("tolerance_own") == Some("2"));
        assert!(z.get_block("heretic").is_some());
        // Inserting it into a group round-trips and re-parses as a group member.
        let out = apply(
            RELIGIONS,
            &Edit::InsertStatement {
                block_path: vec!["muslim".into()],
                statement: scaffold.into(),
            },
        )
        .unwrap();
        let reparsed = parse_bytes(&out);
        let muslim = reparsed.get_block("muslim").unwrap();
        assert!(muslim.get_block("zunist").is_some(), "religion inside group");
        assert!(muslim.get_block("sunni").is_some(), "sibling preserved");
    }

    #[test]
    fn province_religion_paint_and_remove_roundtrip() {
        use crate::mod_writer::{apply, Edit};
        // Paint onto a province that already has a religion (setScalar in place).
        let src = b"owner = SWE\nreligion = catholic\nculture = swedish\n";
        let painted = apply(
            src,
            &Edit::SetScalar {
                path: vec!["religion".into()],
                value: "protestant".into(),
                quoted: false,
            },
        )
        .unwrap();
        let t = String::from_utf8(painted).unwrap();
        assert!(t.contains("religion = protestant"));
        assert!(t.contains("owner = SWE") && t.contains("culture = swedish"));

        // Remove the religion key -> no-religion (rest byte-identical).
        let removed = apply(
            src,
            &Edit::RemoveStatement {
                block_path: vec![],
                key: "religion".into(),
                value: None,
            },
        )
        .unwrap();
        let t2 = String::from_utf8(removed).unwrap();
        assert!(!t2.contains("religion"));
        assert!(t2.contains("owner = SWE") && t2.contains("culture = swedish"));

        // Paint onto a province with NO religion key (insert).
        let src2 = b"owner = SWE\nculture = swedish\n";
        let inserted = apply(
            src2,
            &Edit::InsertStatement {
                block_path: vec![],
                statement: "religion = animism".into(),
            },
        )
        .unwrap();
        let t3 = String::from_utf8(inserted).unwrap();
        assert!(t3.contains("religion = animism"));
        assert!(t3.contains("owner = SWE"));
    }

    #[test]
    fn real_religion_details_and_groups() {
        let Some(vfs) = real_install() else { return };
        let loc = crate::loc::build(&vfs);

        let cat = religion_details(&vfs, &loc, "catholic").unwrap();
        assert_eq!(cat.group_key, "christian");
        assert_eq!(cat.color, Some([204, 204, 0]));
        assert_eq!(cat.icon, Some(1));
        assert_eq!(cat.localized_name, "Catholic");
        assert!(cat.features.iter().any(|f| f == "hre_religion"));
        assert!(cat.heretics.iter().any(|h| h == "BOGOMILIST"));
        assert!(cat.source_file.starts_with("common/religions/"));
        assert!(cat.raw_block_text.starts_with("catholic = {"));
        assert!(cat
            .country_modifiers
            .iter()
            .any(|m| m.key == "tolerance_own"));
        // Usage at 1444: catholic is widely used.
        assert!(cat.province_count > 100, "only {}", cat.province_count);
        assert!(cat.country_count > 10, "only {}", cat.country_count);
        assert!(!cat.sample_tags.is_empty());
        assert!(!cat.sample_provinces.is_empty());

        // Orthodox lives in the christian group too.
        let ortho = religion_details(&vfs, &loc, "orthodox").unwrap();
        assert_eq!(ortho.group_key, "christian");

        let groups = religion_group_list(&vfs, &loc);
        assert!(groups.iter().any(|g| g.key == "christian"));
        assert!(groups.iter().any(|g| g.key == "muslim"));
        assert!(groups.iter().any(|g| g.key == "pagan"));
    }

    #[test]
    fn anbennar_religion_groups_and_details_smoke() {
        if !Path::new(INSTALL).join("map/provinces.bmp").is_file() || !Path::new(ANBENNAR).is_dir()
        {
            return;
        }
        let vfs = Vfs::new(INSTALL, Some(ANBENNAR)).unwrap();
        let loc = crate::loc::build(&vfs);
        // Anbennar defines custom religion groups; the list must include some that
        // vanilla lacks (or at least a large custom set).
        let groups = religion_group_list(&vfs, &loc);
        assert!(groups.len() >= 5, "only {} groups", groups.len());
        // A custom religion's details load: pick any religion from the list.
        let religions = religion_list(&vfs, &loc);
        let any = religions.first().expect("some religion");
        let d = religion_details(&vfs, &loc, &any.key).unwrap();
        assert_eq!(d.key, any.key);
        assert!(!d.source_file.is_empty());
    }

    // --- Culture (Sprint 6) -------------------------------------------------

    const CULTURES: &[u8] = b"scandia = {\n\tgraphical_culture = westerngfx\n\tsecond_graphical_culture = easterngfx\n\n\tmale_names = { Erik Olof }\n\tdynasty_names = { \"von Test\" Bonde }\n\n\tsveier = {\n\t\tprimary = SWE\n\t\tcountry = { }\n\t\tprovince = { local_has_carolean = yes }\n\t\tmale_names = { Karl Gustav }\n\t\tfemale_names = { Kristina Margareta }\n\t\tdynasty_names = { \"von Klinckow\" Vasa }\n\t}\n\tdaner = {\n\t\tprimary = DAN\n\t\tmale_names = { Frederik Christian }\n\t}\n}\notherworld = {\n\tgraphical_culture = asiangfx\n\telf = {\n\t\tmale_names = { Legolas Elrond }\n\t}\n}\n";

    #[test]
    fn pool_names_decodes_latin1_bare_and_quoted() {
        // 0xF6 is 'ö' in Windows-1252/Latin-1.
        let src = b"g = {\n\tc = {\n\t\tmale_names = { G\xF6ran \"Nils Nilsson\" Erik }\n\t}\n}";
        let names = pool_names(src, &["g", "c", "male_names"]);
        assert_eq!(names, vec!["G\u{F6}ran", "Nils Nilsson", "Erik"]);
    }

    #[test]
    fn pool_names_culture_and_group_levels() {
        assert_eq!(
            pool_names(CULTURES, &["scandia", "sveier", "male_names"]),
            vec!["Karl", "Gustav"]
        );
        assert_eq!(
            pool_names(CULTURES, &["scandia", "sveier", "dynasty_names"]),
            vec!["von Klinckow", "Vasa"]
        );
        // Group-level fallback pools (one tab up).
        assert_eq!(
            pool_names(CULTURES, &["scandia", "male_names"]),
            vec!["Erik", "Olof"]
        );
        assert_eq!(
            pool_names(CULTURES, &["scandia", "dynasty_names"]),
            vec!["von Test", "Bonde"]
        );
        // Absent pool -> empty.
        assert!(pool_names(CULTURES, &["scandia", "daner", "female_names"]).is_empty());
    }

    #[test]
    fn culture_pool_edit_roundtrip_byte_identical() {
        use crate::mod_writer::{apply, Edit};
        // setBlock rewrites just the one pool block; siblings + group pools intact.
        let out = apply(
            CULTURES,
            &Edit::SetBlock {
                path: vec!["scandia".into(), "sveier".into(), "male_names".into()],
                value: "Karl Gustav Nils".into(),
            },
        )
        .unwrap();
        let t = String::from_utf8(out).unwrap();
        assert!(t.contains("male_names = { Karl Gustav Nils }"));
        // daner's pool + group pools + sveier's other pools untouched.
        assert!(t.contains("male_names = { Frederik Christian }"));
        assert!(t.contains("male_names = { Erik Olof }"));
        assert!(t.contains("female_names = { Kristina Margareta }"));
        assert!(t.contains("province = { local_has_carolean = yes }"));
    }

    #[test]
    fn culture_group_move_preserves_unmodeled_content() {
        use crate::mod_writer::{apply_all, Edit};
        let block_text = extract_named_block(CULTURES, &["scandia", "sveier"]).unwrap();
        assert!(block_text.starts_with("sveier = {"));
        let out = apply_all(
            CULTURES,
            &[
                Edit::RemoveStatement {
                    block_path: vec!["scandia".into()],
                    key: "sveier".into(),
                    value: None,
                },
                Edit::InsertStatement {
                    block_path: vec!["otherworld".into()],
                    statement: block_text,
                },
            ],
        )
        .unwrap();
        let t = String::from_utf8(out).unwrap();
        // sveier moved into otherworld, unmodeled country/province blocks intact.
        assert_eq!(t.matches("sveier = {").count(), 1);
        assert!(t.contains("province = { local_has_carolean = yes }"));
        assert!(t.contains("dynasty_names = { \"von Klinckow\" Vasa }"));
        let ow = t.find("otherworld = {").unwrap();
        assert!(t[ow..].contains("sveier = {"));
        // daner + group pools still in scandia.
        assert!(t.contains("daner = {"));
        assert!(t.contains("male_names = { Erik Olof }"));
    }

    #[test]
    fn created_culture_scaffold_parses_and_inserts() {
        use crate::mod_writer::{apply, Edit};
        // The scaffold the frontend inserts: a culture with pools copied from a
        // sibling (the game needs male/dynasty names to generate rulers).
        let scaffold = "gothic = {\n\tmale_names = { Karl Gustav }\n\tfemale_names = { Kristina Margareta }\n\tdynasty_names = { \"von Klinckow\" Vasa }\n}";
        let parsed = parse_bytes(scaffold.as_bytes());
        let g = parsed.get_block("gothic").expect("gothic parses");
        assert!(g.get_block("male_names").is_some());
        assert!(g.get_block("dynasty_names").is_some());
        // Inserting into a group round-trips and re-parses as a group member.
        let out = apply(
            CULTURES,
            &Edit::InsertStatement {
                block_path: vec!["scandia".into()],
                statement: scaffold.into(),
            },
        )
        .unwrap();
        let reparsed = parse_bytes(&out);
        let scandia = reparsed.get_block("scandia").unwrap();
        assert!(scandia.get_block("gothic").is_some(), "culture inside group");
        assert!(scandia.get_block("sveier").is_some(), "sibling preserved");
        assert_eq!(
            pool_names(&out, &["scandia", "gothic", "male_names"]),
            vec!["Karl", "Gustav"]
        );
    }

    #[test]
    fn province_culture_paint_and_remove_roundtrip() {
        use crate::mod_writer::{apply, Edit};
        // Paint onto a province that already has a culture (setScalar in place).
        let src = b"owner = SWE\nculture = swedish\nreligion = catholic\n";
        let painted = apply(
            src,
            &Edit::SetScalar {
                path: vec!["culture".into()],
                value: "norwegian".into(),
                quoted: false,
            },
        )
        .unwrap();
        let t = String::from_utf8(painted).unwrap();
        assert!(t.contains("culture = norwegian"));
        assert!(t.contains("owner = SWE") && t.contains("religion = catholic"));

        // Remove the culture key -> no-culture (rest byte-identical).
        let removed = apply(
            src,
            &Edit::RemoveStatement {
                block_path: vec![],
                key: "culture".into(),
                value: None,
            },
        )
        .unwrap();
        let t2 = String::from_utf8(removed).unwrap();
        assert!(!t2.contains("culture"));
        assert!(t2.contains("owner = SWE") && t2.contains("religion = catholic"));

        // Paint onto a province with NO culture key (insert).
        let src2 = b"owner = SWE\nreligion = catholic\n";
        let inserted = apply(
            src2,
            &Edit::InsertStatement {
                block_path: vec![],
                statement: "culture = swedish".into(),
            },
        )
        .unwrap();
        let t3 = String::from_utf8(inserted).unwrap();
        assert!(t3.contains("culture = swedish"));
        assert!(t3.contains("owner = SWE"));
    }

    #[test]
    fn real_culture_details_and_groups() {
        let Some(vfs) = real_install() else { return };
        let loc = crate::loc::build(&vfs);

        let swe = culture_details(&vfs, &loc, "swedish").unwrap();
        assert_eq!(swe.group_key, "scandinavian");
        assert_eq!(swe.primary.as_deref(), Some("SWE"));
        assert!(!swe.male_names.is_empty(), "swedish male_names non-empty");
        assert!(swe.source_file.starts_with("common/cultures/"));
        assert!(swe.raw_block_text.starts_with("swedish = {"));
        assert_eq!(swe.group_graphical_culture.as_deref(), Some("westerngfx"));
        // Unmodeled country/province blocks preserved read-only.
        assert!(swe.raw_remainder.iter().any(|r| r.key == "province"));
        assert!(swe.province_count > 0, "swedish provinces at 1444");

        let groups = culture_group_list(&vfs, &loc);
        assert!(groups.iter().any(|g| g.key == "scandinavian"));
    }

    #[test]
    fn culture_color_override_reflected_in_mode_data() {
        let Some(vfs) = real_install() else { return };
        let loc = crate::loc::build(&vfs);
        let mut overrides = HashMap::new();
        overrides.insert("swedish".to_string(), [10u8, 20, 30]);
        let data = mode_data_with_overrides(&vfs, &loc, "culture", &overrides).unwrap();
        let swe = data
            .groups
            .iter()
            .find(|g| g.key == "swedish")
            .expect("swedish culture painted at 1444");
        assert_eq!(swe.color, [10, 20, 30]);
        // A culture without an override keeps its hash color (not the override).
        if let Some(other) = data.groups.iter().find(|g| g.key != "swedish") {
            assert_ne!(other.color, [10, 20, 30]);
        }
    }

    #[test]
    fn anbennar_culture_groups_and_details_smoke() {
        if !Path::new(INSTALL).join("map/provinces.bmp").is_file() || !Path::new(ANBENNAR).is_dir()
        {
            return;
        }
        let vfs = Vfs::new(INSTALL, Some(ANBENNAR)).unwrap();
        let loc = crate::loc::build(&vfs);
        // Anbennar ships fully custom culture groups.
        let groups = culture_group_list(&vfs, &loc);
        assert!(groups.len() >= 5, "only {} groups", groups.len());
        let cultures = culture_list(&vfs, &loc);
        let any = cultures.first().expect("some culture");
        let d = culture_details(&vfs, &loc, &any.key).unwrap();
        assert_eq!(d.key, any.key);
        assert!(!d.source_file.is_empty());
    }
}
