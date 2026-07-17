use std::collections::{HashMap, HashSet};
use std::io::Cursor;
use std::sync::Arc;

use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::{ExtendedColorType, ImageEncoder};

use crate::cache;
use crate::date::Date;
#[cfg(test)]
use crate::date::DEFAULT_START;
use crate::game_data;
use crate::vfs::Vfs;

const LAND: [u8; 3] = [200, 200, 196];
const WATER: [u8; 3] = [164, 186, 201];
const BORDER: [u8; 3] = [96, 96, 94];

/// Fixed stripe color for rebel-held provinces (`controller = REB`), which have
/// no country color (Sprint 13.3). Shared with `game_data`'s stripe payload so
/// the backend render and client compositor agree.
pub const REBEL_GRAY: [u8; 3] = [80, 80, 80];

/// Diagonal-stripe band width for occupation rendering: `((x + y) / STRIPE_BAND)
/// % 2` picks owner vs controller. Mirrored client-side (Sprint 13.3).
const STRIPE_BAND: usize = 8;

/// (id, label) for every supported map mode, in display order.
pub const MAP_MODES: &[(&str, &str)] = &[
    ("provinces", "Provinces"),
    ("political", "Political"),
    ("religion", "Religion"),
    ("culture", "Culture"),
    ("trade_goods", "Trade Goods"),
    ("trade_nodes", "Trade Nodes"),
    ("development", "Development"),
    ("areas", "Areas"),
    ("regions", "Regions"),
    ("colonial_regions", "Colonial Regions"),
    ("trade_companies", "Trade Companies"),
    ("climate", "Climate"),
    ("winter", "Winter"),
    ("simple_terrain", "Simple Terrain"),
    ("terrain", "Terrain"),
    ("heightmap", "Heightmap"),
    ("province_colors", "Province Colors"),
];

/// Modes that are just images of a shipped bitmap: no editing tools, no side
/// panel (Sprint 11.3 badges them "View Only"). Exposed to the frontend via
/// `list_map_modes` so the mode list can render the badge.
pub const VIEW_ONLY_MODES: &[&str] = &["terrain", "heightmap", "province_colors"];

struct BaseMap {
    width: u32,
    height: u32,
    /// Province id per pixel (u32::MAX for colors missing from definition.csv).
    ids: Vec<u32>,
    water: HashSet<u32>,
    /// Raw provinces.bmp RGB, kept for the Province Colors mode.
    raw: Vec<u8>,
}

// --- Session caches (see cache.rs) ----------------------------------------
//
// Decoding the 34MB provinces.bmp + building the 11.5M-entry id buffer per
// render was a lag root cause; the base map never changes within a session.
// Rendered mode PNGs are also memoized (mode switching back to a visited mode
// is then instant); the store self-clears past a small cap so date-scrubbing
// can't grow it unboundedly.

static BASE_MAPS: cache::Store<cache::SessionKey, BaseMap> = cache::Store::new();
static RENDERED: cache::Store<(cache::SessionKey, String, Date), Vec<u8>> = cache::Store::new();
static COASTAL: cache::Store<cache::SessionKey, HashSet<u32>> = cache::Store::new();
static ADJACENCY: cache::Store<cache::SessionKey, HashMap<u32, Vec<u32>>> = cache::Store::new();

/// Rendered-PNG entries kept before the cache clears itself (17 modes; a date
/// change re-keys, so a scrubbing session would otherwise accumulate).
const RENDER_CACHE_CAP: usize = 24;

/// Drops this module's session caches. Called from `cache::invalidate_all`.
pub(crate) fn invalidate_caches() {
    BASE_MAPS.clear();
    RENDERED.clear();
    COASTAL.clear();
    ADJACENCY.clear();
}

/// province id -> map-adjacent province ids (4-neighborhood over the id
/// buffer, horizontal wrap honored — same neighbor rule as `coastal_land_ids`
/// and the border painter). Water and land both included; unknown-color pixels
/// (u32::MAX) excluded. Memoized per session.
pub(crate) fn province_adjacency(vfs: &Vfs) -> Result<Arc<HashMap<u32, Vec<u32>>>, String> {
    ADJACENCY.get_or_try_build(cache::session_key(vfs), || {
        let base = load_base(vfs)?;
        let w = base.width as usize;
        let h = base.height as usize;
        let ids = &base.ids;
        let mut pairs: HashSet<(u32, u32)> = HashSet::new();
        let mut add = |a: u32, b: u32| {
            if a != b && a != u32::MAX && b != u32::MAX {
                pairs.insert((a.min(b), a.max(b)));
            }
        };
        for y in 0..h {
            let row = y * w;
            for x in 0..w {
                let id = ids[row + x];
                let right = if x + 1 < w { ids[row + x + 1] } else { ids[row] };
                add(id, right);
                if y + 1 < h {
                    add(id, ids[row + w + x]);
                }
            }
        }
        let mut out: HashMap<u32, Vec<u32>> = HashMap::new();
        for (a, b) in pairs {
            out.entry(a).or_default().push(b);
            out.entry(b).or_default().push(a);
        }
        Ok(out)
    })
}

/// Renders a map mode at the effective start date (pre-Sprint-12 signature; used
/// by tests). Delegates at 1444.11.11.
#[cfg(test)]
pub fn render_map_mode(vfs: &Vfs, mode: &str) -> Result<Vec<u8>, String> {
    render_map_mode_at(vfs, mode, DEFAULT_START)
}

/// Renders a map mode as of `date`: the province-derived modes (political,
/// religion, culture, trade goods, development) fold dated history blocks ≤ date,
/// so the PNG matches the date-aware `mode_data` groups and province panel.
pub fn render_map_mode_at(vfs: &Vfs, mode: &str, date: Date) -> Result<Vec<u8>, String> {
    if RENDERED.len() >= RENDER_CACHE_CAP {
        RENDERED.clear();
    }
    let key = (cache::session_key(vfs), mode.to_string(), date);
    let png = RENDERED.get_or_try_build(key, || render_map_mode_uncached(vfs, mode, date))?;
    Ok(png.as_ref().clone())
}

fn render_map_mode_uncached(vfs: &Vfs, mode: &str, date: Date) -> Result<Vec<u8>, String> {
    match mode {
        "terrain" => encode_bmp_file(vfs, "map/terrain.bmp"),
        "heightmap" => encode_bmp_file(vfs, "map/heightmap.bmp"),
        _ => {
            let base = load_base(vfs)?;
            if mode == "province_colors" {
                return encode_png(&base.raw, base.width, base.height);
            }
            let (colors, default_land) = mode_colors(vfs, mode, &base, date)?;
            // Occupation stripes: political mode paints any province whose
            // effective controller differs from its owner in diagonal bands of
            // owner-color / controller-color. Empty for every other mode.
            let stripes = if mode == "political" {
                political_controller_stripes(vfs, date)
            } else {
                HashMap::new()
            };
            paint(&base, &colors, default_land, &stripes)
        }
    }
}

/// province id -> controller stripe color for every occupied province as of
/// `date` (effective controller differs from effective owner). Rebel-held
/// provinces (`controller = REB`) get [`REBEL_GRAY`]. Skips ownerless provinces
/// (no base color to stripe against).
fn political_controller_stripes(vfs: &Vfs, date: Date) -> HashMap<u32, [u8; 3]> {
    let country = game_data::country_colors(vfs);
    let mut out = HashMap::new();
    for (id, state) in game_data::province_history_at(vfs, date) {
        let (Some(owner), Some(controller)) = (state.owner, state.controller) else {
            continue;
        };
        if owner == controller {
            continue;
        }
        let color = if controller == "REB" {
            REBEL_GRAY
        } else {
            country
                .get(&controller)
                .copied()
                .unwrap_or_else(|| hash_color(&controller))
        };
        out.insert(id, color);
    }
    out
}

/// Builds the province id -> color table for a data-driven mode as of `date`.
/// Returns the table plus the fallback color for land provinces without an entry.
fn mode_colors(
    vfs: &Vfs,
    mode: &str,
    base: &BaseMap,
    date: Date,
) -> Result<(HashMap<u32, [u8; 3]>, [u8; 3]), String> {
    let mut colors = HashMap::new();
    match mode {
        "provinces" => {}
        "political" => {
            let country = game_data::country_colors(vfs);
            for (id, state) in game_data::province_history_at(vfs, date) {
                if let Some(c) = state.owner.as_deref().and_then(|t| country.get(t)) {
                    colors.insert(id, *c);
                }
            }
        }
        "religion" => {
            let religion = game_data::religion_colors(vfs);
            for (id, state) in game_data::province_history_at(vfs, date) {
                if let Some(c) = state.religion.as_deref().and_then(|r| religion.get(r)) {
                    colors.insert(id, *c);
                }
            }
        }
        "culture" => {
            for (id, state) in game_data::province_history_at(vfs, date) {
                if let Some(culture) = &state.culture {
                    colors.insert(id, hash_color(culture));
                }
            }
        }
        "trade_goods" => {
            let goods = game_data::trade_good_colors(vfs);
            for (id, state) in game_data::province_history_at(vfs, date) {
                if let Some(c) = state.trade_goods.as_deref().and_then(|g| goods.get(g)) {
                    colors.insert(id, *c);
                }
            }
        }
        "trade_nodes" => {
            for (name, (color, ids)) in game_data::trade_nodes(vfs) {
                let c = color.unwrap_or_else(|| hash_color(&name));
                for id in ids {
                    colors.insert(id, c);
                }
            }
        }
        "development" => {
            for (id, state) in game_data::province_history_at(vfs, date) {
                if let Some(dev) = state.development {
                    colors.insert(id, dev_color(dev));
                }
            }
        }
        "areas" => {
            for (name, ids) in game_data::areas(vfs) {
                let c = hash_color(&name);
                for id in ids {
                    colors.insert(id, c);
                }
            }
        }
        "regions" => {
            for (name, ids) in game_data::regions(vfs) {
                let c = hash_color(&name);
                for id in ids {
                    colors.insert(id, c);
                }
            }
        }
        // Colonial regions / trade companies (Sprint 19): membership-colored by
        // each entry's explicit `color` (hash fallback). Unassigned land stays
        // neutral (most of the world is in neither — legal).
        "colonial_regions" | "trade_companies" => {
            for (_name, color, ids) in crate::colonial::membership(vfs, mode) {
                for id in ids {
                    colors.insert(id, color);
                }
            }
        }
        // climate.txt shares one file across independent slots (zone, winter,
        // monsoon, impassable). Filter to the zone slot so a province that is
        // also in a winter list still renders its zone (and impassable stays
        // visible, per Sprint 11.2).
        "climate" => {
            for (id, zone) in game_data::climate_slot(vfs, game_data::CLIMATE_ZONE_KEYS) {
                let c = match zone.as_str() {
                    "tropical" => [64, 142, 63],
                    "arid" => [216, 196, 120],
                    "arctic" => [235, 235, 238],
                    "impassable" => [80, 80, 80],
                    _ => continue,
                };
                colors.insert(id, c);
            }
            // Unlisted land is temperate.
            return Ok((colors, [126, 171, 97]));
        }
        // Winter-severity visualization (Sprint 11.1): the winter slot of
        // climate.txt, distinct blues by severity. Non-winter land stays default.
        "winter" => {
            for (id, key) in game_data::climate_slot(vfs, game_data::WINTER_KEYS) {
                let c = match key.as_str() {
                    "mild_winter" => [176, 206, 224],
                    "normal_winter" => [116, 158, 204],
                    "severe_winter" => [72, 92, 148],
                    _ => continue,
                };
                colors.insert(id, c);
            }
            return Ok((colors, LAND));
        }
        // Effective gameplay terrain per province (Sprint 11.2): terrain_override
        // if present, else the dominant terrain.bmp palette class. Sea renders as
        // sea. Reuses the already-loaded base for the pixel pass.
        "simple_terrain" => {
            let eff = effective_terrain_with_base(vfs, base)?;
            for (id, (cat, _)) in &eff.by_province {
                if eff.water.contains(id) {
                    continue;
                }
                colors.insert(*id, terrain_color(cat));
            }
            return Ok((colors, LAND));
        }
        other => return Err(format!("Unknown map mode: {other}")),
    }
    let _ = base;
    Ok((colors, LAND))
}

/// The decoded base map for this session, built once and memoized.
fn load_base(vfs: &Vfs) -> Result<Arc<BaseMap>, String> {
    BASE_MAPS.get_or_try_build(cache::session_key(vfs), || load_base_uncached(vfs))
}

fn load_base_uncached(vfs: &Vfs) -> Result<BaseMap, String> {
    let bmp = vfs.read("map/provinces.bmp")?;
    let img = image::load_from_memory(&bmp)
        .map_err(|e| format!("Failed to load map/provinces.bmp: {e}"))?
        .to_rgb8();
    let (width, height) = img.dimensions();

    let color_to_id = load_definitions(vfs)?;
    let water = load_water_ids(vfs)?;

    let raw = img.into_raw();
    let mut ids = vec![0u32; (width as usize) * (height as usize)];
    for (i, px) in raw.chunks_exact(3).enumerate() {
        ids[i] = *color_to_id
            .get(&pack(px[0], px[1], px[2]))
            .unwrap_or(&u32::MAX);
    }

    Ok(BaseMap {
        width,
        height,
        ids,
        water,
        raw,
    })
}

/// Fills provinces from the color table (water defaults to WATER, land to
/// `default_land`) and draws borders where provinces meet. Boundaries
/// involving land get a dark border; water-water boundaries get a thin light
/// one (the fill color slightly darkened) so sea tiles stay distinguishable
/// without visual noise.
fn paint(
    base: &BaseMap,
    colors: &HashMap<u32, [u8; 3]>,
    default_land: [u8; 3],
    stripes: &HashMap<u32, [u8; 3]>,
) -> Result<Vec<u8>, String> {
    const NO_BORDER: u8 = 0;
    const WATER_BORDER: u8 = 1;
    const LAND_BORDER: u8 = 2;

    let width = base.width as usize;
    let ids = &base.ids;
    let mut out = vec![0u8; ids.len() * 3];

    for (i, &id) in ids.iter().enumerate() {
        let is_water = base.water.contains(&id);
        let x = i % width;

        let classify = |neighbor: u32| {
            if neighbor == id {
                NO_BORDER
            } else if is_water && base.water.contains(&neighbor) {
                WATER_BORDER
            } else {
                LAND_BORDER
            }
        };
        let mut border = NO_BORDER;
        if x > 0 {
            border = border.max(classify(ids[i - 1]));
        }
        if i >= width {
            border = border.max(classify(ids[i - width]));
        }

        let base_fill = if let Some(&c) = colors.get(&id) {
            c
        } else if is_water {
            WATER
        } else {
            default_land
        };
        // Occupation stripes: alternate owner (base_fill) / controller color in
        // diagonal bands. Border pixels still take the border treatment below.
        let fill = match stripes.get(&id) {
            Some(&controller_color) => {
                let y = i / width;
                if ((x + y) / STRIPE_BAND) % 2 == 0 {
                    base_fill
                } else {
                    controller_color
                }
            }
            None => base_fill,
        };
        let color = match border {
            LAND_BORDER => BORDER,
            WATER_BORDER => fill.map(|v| (v as f32 * 0.86) as u8),
            _ => fill,
        };
        out[i * 3..i * 3 + 3].copy_from_slice(&color);
    }

    encode_png(&out, base.width, base.height)
}

fn encode_bmp_file(vfs: &Vfs, rel: &str) -> Result<Vec<u8>, String> {
    let bytes = vfs.read(rel)?;
    let img = image::load_from_memory(&bytes)
        .map_err(|e| format!("Failed to load {rel}: {e}"))?
        .to_rgb8();
    let (w, h) = img.dimensions();
    encode_png(&img.into_raw(), w, h)
}

pub(crate) fn encode_png(rgb: &[u8], width: u32, height: u32) -> Result<Vec<u8>, String> {
    let mut png = Vec::new();
    PngEncoder::new_with_quality(
        Cursor::new(&mut png),
        CompressionType::Fast,
        FilterType::Adaptive,
    )
    .write_image(rgb, width, height, ExtendedColorType::Rgb8)
    .map_err(|e| format!("Failed to encode map image: {e}"))?;
    Ok(png)
}

/// Province id per pixel for frontend hit-testing, as a little-endian binary
/// buffer: [u32 width][u32 height][u16 id per pixel]. Ids that don't fit u16
/// (unknown colors mapped to u32::MAX) become 0.
pub fn province_id_buffer(vfs: &Vfs) -> Result<Vec<u8>, String> {
    let base = load_base(vfs)?;
    let mut out = Vec::with_capacity(8 + base.ids.len() * 2);
    out.extend_from_slice(&base.width.to_le_bytes());
    out.extend_from_slice(&base.height.to_le_bytes());
    for &id in &base.ids {
        let id16 = if id > u16::MAX as u32 { 0u16 } else { id as u16 };
        out.extend_from_slice(&id16.to_le_bytes());
    }
    Ok(out)
}

/// Land province ids that touch water, i.e. **coastal** provinces (S3.3). A land
/// province is coastal when any of its pixels is 4-neighbor-adjacent to a pixel of
/// a *water* province. Derived from the province-id buffer + default.map water set
/// (the same signal the game uses to distinguish coastal vs inland trade centers),
/// since the codebase has no prior coastal helper. The horizontal map wrap
/// (antimeridian) is honored: column 0 and the last column are neighbors. One
/// O(pixels) pass over the base map; callers cache the result.
pub(crate) fn coastal_land_ids(vfs: &Vfs) -> Result<Arc<HashSet<u32>>, String> {
    COASTAL.get_or_try_build(cache::session_key(vfs), || coastal_land_ids_uncached(vfs))
}

fn coastal_land_ids_uncached(vfs: &Vfs) -> Result<HashSet<u32>, String> {
    let base = load_base(vfs)?;
    let w = base.width as usize;
    let h = base.height as usize;
    let ids = &base.ids;
    let water = &base.water;
    let mut coastal: HashSet<u32> = HashSet::new();
    for y in 0..h {
        let row = y * w;
        for x in 0..w {
            let id = ids[row + x];
            // Only land provinces can be coastal; skip water and unknown ids.
            if water.contains(&id) || id == u32::MAX {
                continue;
            }
            if coastal.contains(&id) {
                continue;
            }
            // 4-neighborhood with horizontal wrap; vertical edges have no wrap.
            let left = ids[row + if x == 0 { w - 1 } else { x - 1 }];
            let right = ids[row + if x + 1 == w { 0 } else { x + 1 }];
            let up = if y > 0 { Some(ids[row - w + x]) } else { None };
            let down = if y + 1 < h { Some(ids[row + w + x]) } else { None };
            let touches_water = water.contains(&left)
                || water.contains(&right)
                || up.map(|n| water.contains(&n)).unwrap_or(false)
                || down.map(|n| water.contains(&n)).unwrap_or(false);
            if touches_water {
                coastal.insert(id);
            }
        }
    }
    Ok(coastal)
}

fn pack(r: u8, g: u8, b: u8) -> u32 {
    ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
}

/// Deterministic distinct-ish color from a name (for cultures/areas/regions,
/// which have no colors in the game files).
pub(crate) fn hash_color(name: &str) -> [u8; 3] {
    let mut h: u32 = 2166136261;
    for b in name.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(16777619);
    }
    let hue = (h % 360) as f32;
    let sat = 0.45 + ((h >> 9) % 30) as f32 / 100.0;
    let light = 0.55 + ((h >> 17) % 15) as f32 / 100.0;
    hsl_to_rgb(hue, sat, light)
}

/// Pale to dark green over the typical 3-30 development range.
fn dev_color(dev: f32) -> [u8; 3] {
    let t = ((dev - 3.0) / 27.0).clamp(0.0, 1.0);
    let lerp = |a: f32, b: f32| (a + (b - a) * t).round() as u8;
    [lerp(216.0, 20.0), lerp(232.0, 105.0), lerp(200.0, 20.0)]
}

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> [u8; 3] {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = h / 60.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r, g, b) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    [
        ((r + m) * 255.0).round() as u8,
        ((g + m) * 255.0).round() as u8,
        ((b + m) * 255.0).round() as u8,
    ]
}

/// definition.csv: `province;red;green;blue;name;x` per line, `;`-separated.
/// Names can contain non-UTF8 (Windows-1252) bytes, so decode lossily and
/// skip anything that doesn't parse (header line, comments, RNW stubs).
fn load_definitions(vfs: &Vfs) -> Result<HashMap<u32, u32>, String> {
    let bytes = vfs.read("map/definition.csv")?;
    let text = String::from_utf8_lossy(&bytes);
    let mut map = HashMap::new();
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
        map.insert(pack(r, g, b), id);
    }
    if map.is_empty() {
        return Err("map/definition.csv contained no province definitions".to_string());
    }
    Ok(map)
}

/// Sea + lake province ids (from `map/default.map` `sea_starts`/`lakes`). Public
/// wrapper for callers that need the water set (adjacency validation / strait
/// type derivation, Sprint 25). Cheap: reads only default.map.
pub(crate) fn water_ids(vfs: &Vfs) -> Result<HashSet<u32>, String> {
    load_water_ids(vfs)
}

fn load_water_ids(vfs: &Vfs) -> Result<HashSet<u32>, String> {
    let bytes = vfs.read("map/default.map")?;
    let block = crate::paradox::parse(&String::from_utf8_lossy(&bytes));
    let mut water: HashSet<u32> = HashSet::new();
    for key in ["sea_starts", "lakes"] {
        if let Some(list) = block.get_block(key) {
            water.extend(list.bare_ids());
        }
    }
    Ok(water)
}

// --- Simple Terrain mode (Sprint 11.2) -----------------------------------

/// Effective gameplay terrain of every province: `terrain_override` from
/// map/terrain.txt where present, else the dominant terrain.bmp palette class
/// mapped to a terrain category. `water` are sea/lake ids (rendered as sea).
pub struct EffectiveTerrain {
    /// province id -> (terrain category key, is_override).
    pub by_province: HashMap<u32, (String, bool)>,
    /// province id -> dominant terrain.bmp category (the "auto" class), computed
    /// for EVERY province regardless of an override (Sprint 11.2's "Auto (from
    /// terrain.bmp)" eraser needs to know what a province reverts to). Absent for
    /// provinces with no map pixels (RNW stubs).
    pub auto_by_province: HashMap<u32, String>,
    /// Sea + lake province ids (default.map).
    pub water: HashSet<u32>,
}

/// Curated fixed colors for the vanilla terrain categories (Sprint 11.2).
/// Mod-added categories (or the rare snow special case, which maps to the
/// `mountain` category in gameplay) fall back to a stable hash color.
pub fn terrain_color(category: &str) -> [u8; 3] {
    match category {
        "grasslands" => [124, 186, 78],       // green
        "farmlands" => [96, 158, 46],         // rich green
        "hills" => [196, 170, 122],           // tan
        "highlands" => [178, 132, 78],        // orange-tan uplands
        "mountain" => [132, 116, 108],        // gray-brown
        "impassable_mountains" => [92, 86, 82],
        "forest" => [42, 96, 48],             // dark green
        "woods" => [74, 128, 66],             // medium-dark green
        "jungle" => [28, 82, 32],             // deep green
        "marsh" => [72, 154, 142],            // teal
        "steppe" => [182, 192, 122],          // pale olive
        "desert" => [226, 210, 130],          // sand yellow
        "coastal_desert" => [236, 202, 152],  // pale orange
        "drylands" => [206, 162, 102],        // orange-tan
        "savannah" => [202, 200, 92],         // yellow-green
        "glacier" => [206, 226, 236],         // ice blue
        "snow" => [236, 238, 240],            // white (defensive; not a category)
        "coastline" => [118, 176, 152],       // greenish coast
        "pti" => [122, 122, 122],             // wasteland gray
        "ocean" | "inland_ocean" => WATER,
        _ => hash_color(category),
    }
}

/// Classifies every province by effective terrain (loads its own base map).
pub fn effective_terrain(vfs: &Vfs) -> Result<EffectiveTerrain, String> {
    let base = load_base(vfs)?;
    effective_terrain_with_base(vfs, &base)
}

/// As [`effective_terrain`], reusing an already-loaded base map (one pixel pass
/// over terrain.bmp, building a per-province palette histogram).
fn effective_terrain_with_base(vfs: &Vfs, base: &BaseMap) -> Result<EffectiveTerrain, String> {
    let palette_types = game_data::terrain_palette_types(vfs);
    let overrides = game_data::terrain_override_map(vfs);
    let indices = load_terrain_indices(vfs, base.width, base.height)?;

    let max_id = base
        .ids
        .iter()
        .copied()
        .filter(|&x| x != u32::MAX)
        .max()
        .unwrap_or(0) as usize;
    // Per-province palette-index histogram. [u32; 256] per province is cheap at
    // these counts and much faster than a nested map over ~11.5M pixels.
    let mut hist: Vec<[u32; 256]> = vec![[0u32; 256]; max_id + 1];
    for (i, &id) in base.ids.iter().enumerate() {
        if id == u32::MAX {
            continue;
        }
        hist[id as usize][indices[i] as usize] += 1;
    }

    let mut by_province: HashMap<u32, (String, bool)> = HashMap::new();
    let mut auto_by_province: HashMap<u32, String> = HashMap::new();
    for (id, counts) in hist.iter().enumerate() {
        let id = id as u32;
        let total: u32 = counts.iter().sum();
        // Auto (terrain.bmp) class is computed for EVERY province with pixels,
        // even those carrying an override — the eraser reverts to it.
        if total > 0 {
            let best_idx = counts
                .iter()
                .enumerate()
                .max_by_key(|(_, c)| **c)
                .map(|(i, _)| i as u8)
                .unwrap_or(0);
            if let Some(cat) = palette_types.get(&best_idx) {
                auto_by_province.insert(id, cat.clone());
            }
        }
        if let Some(cat) = overrides.get(&id) {
            by_province.insert(id, (cat.clone(), true));
            continue;
        }
        if let Some(cat) = auto_by_province.get(&id) {
            by_province.insert(id, (cat.clone(), false));
        }
    }
    // Overrides for provinces whose ids fall outside the map histogram range.
    for (id, cat) in &overrides {
        by_province.entry(*id).or_insert_with(|| (cat.clone(), true));
    }

    Ok(EffectiveTerrain {
        by_province,
        auto_by_province,
        water: base.water.clone(),
    })
}

/// Reads map/terrain.bmp as raw 8-bit palette indices in top-down row order
/// (matching the province-id buffer orientation). terrain.bmp is a paletted BMP,
/// so `image`'s RGB decode would lose the indices the `terrain` block keys on.
fn load_terrain_indices(vfs: &Vfs, exp_w: u32, exp_h: u32) -> Result<Vec<u8>, String> {
    let bytes = vfs.read("map/terrain.bmp")?;
    if bytes.len() < 54 || &bytes[0..2] != b"BM" {
        return Err("map/terrain.bmp is not a BMP".to_string());
    }
    let data_off = u32::from_le_bytes(bytes[10..14].try_into().unwrap()) as usize;
    let width = i32::from_le_bytes(bytes[18..22].try_into().unwrap());
    let height_raw = i32::from_le_bytes(bytes[22..26].try_into().unwrap());
    let bpp = u16::from_le_bytes(bytes[28..30].try_into().unwrap());
    if bpp != 8 {
        return Err(format!("map/terrain.bmp is {bpp}bpp, expected 8bpp indexed"));
    }
    let width = width as usize;
    let top_down = height_raw < 0;
    let height = height_raw.unsigned_abs() as usize;
    if width as u32 != exp_w || height as u32 != exp_h {
        return Err(format!(
            "map/terrain.bmp {width}x{height} does not match provinces.bmp {exp_w}x{exp_h}"
        ));
    }
    let row_size = width.div_ceil(4) * 4; // 8bpp: 1 byte/pixel, rows padded to 4
    let mut out = vec![0u8; width * height];
    for y in 0..height {
        let src_row = if top_down { y } else { height - 1 - y };
        let start = data_off + src_row * row_size;
        if start + width > bytes.len() {
            return Err("map/terrain.bmp pixel data is truncated".to_string());
        }
        out[y * width..y * width + width].copy_from_slice(&bytes[start..start + width]);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    const INSTALL: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Europa Universalis IV";
    const ANBENNAR: &str = r"C:\Users\User\projects\anbennar-eu4-dev";

    fn real_install() -> Option<Vfs> {
        Path::new(INSTALL)
            .join("map")
            .join("provinces.bmp")
            .is_file()
            .then(|| Vfs::new(INSTALL, None).unwrap())
    }

    fn anbennar() -> Option<Vfs> {
        (Path::new(INSTALL).is_dir() && Path::new(ANBENNAR).is_dir())
            .then(|| Vfs::new(INSTALL, Some(ANBENNAR)).unwrap())
    }

    #[test]
    fn water_ids_from_default_map() {
        let text = r#"
width = 5632
# sea_starts = { 999 }
open_sea_starts = { 7 8 }
sea_starts = {
	1 2 3
	4
}
lakes = { 10 11 }
"#;
        let block = crate::paradox::parse(text);
        assert_eq!(block.get_block("sea_starts").unwrap().bare_ids(), vec![1, 2, 3, 4]);
        assert_eq!(block.get_block("lakes").unwrap().bare_ids(), vec![10, 11]);
    }

    /// Perf evidence for the session caches (run with `-- --ignored --nocapture`):
    /// cold = the full pre-cache cost (bmp decode + all history parses + paint +
    /// encode); warm same-mode = the render-PNG memo; warm other-mode = paint +
    /// encode over the cached base map/ASTs.
    #[test]
    #[ignore]
    fn render_timing_cold_vs_warm() {
        let Some(install) = real_install() else { return };
        crate::cache::invalidate_all();
        let t = std::time::Instant::now();
        render_map_mode(&install, "political").unwrap();
        let cold = t.elapsed();
        let t = std::time::Instant::now();
        render_map_mode(&install, "political").unwrap();
        let warm_same = t.elapsed();
        let t = std::time::Instant::now();
        render_map_mode(&install, "religion").unwrap();
        let warm_other = t.elapsed();
        println!(
            "political cold: {cold:?} | political warm: {warm_same:?} | religion after: {warm_other:?}"
        );
        assert!(warm_same < cold);
    }

    #[test]
    fn renders_real_map_when_installed() {
        let Some(install) = real_install() else { return };
        let png = render_map_mode(&install,"provinces").expect("render should succeed");
        assert!(png.len() > 100_000, "png suspiciously small: {}", png.len());
        assert_eq!(&png[..4], &[0x89, b'P', b'N', b'G']);
    }

    #[test]
    fn renders_all_modes_when_installed() {
        let Some(install) = real_install() else { return };
        for (mode, _) in MAP_MODES {
            let png = render_map_mode(&install,mode)
                .unwrap_or_else(|e| panic!("mode {mode} failed: {e}"));
            assert!(png.len() > 10_000, "mode {mode} png too small: {}", png.len());
        }
    }

    #[test]
    fn real_country_details_load() {
        let Some(install) = real_install() else { return };
        let loc = crate::loc::build(&install);
        let details = crate::game_data::country_details(&install, &loc, "SWE").unwrap();
        assert_eq!(details.name, "Sweden");
        assert_eq!(details.localized_name, "Sweden");
        assert_eq!(details.adjective.as_deref(), Some("Swedish"));
        assert!(details.color.is_some());
        // Sweden starts in a PU under Denmark: no own monarch until 1448.
        assert!(details.ruler.is_none());
        let ideas = details.ideas.expect("Sweden has national ideas");
        assert_eq!(ideas.ideas.len(), 7, "expected 7 national ideas");
        assert!(!ideas.traditions.is_empty());
        assert!(details.capital_name.is_some());

        // France has a monarch defined before 1444.
        let france = crate::game_data::country_details(&install, &loc, "FRA").unwrap();
        let ruler = france.ruler.expect("France has a 1444 ruler");
        assert!(ruler.name.is_some());
        assert!(ruler.adm.is_some());

        let flag = crate::game_data::country_flag_png(&install,"SWE").unwrap();
        assert_eq!(&flag[..4], &[0x89, b'P', b'N', b'G']);

        // Tag validation blocks path traversal.
        assert!(crate::game_data::country_flag_png(&install,"../x").is_err());

        let political = crate::game_data::mode_data(&install, &loc, "political").unwrap();
        assert!(political.groups.len() > 300);
        // Localized name flows through the mode-data groups; Stockholm(1) is SWE.
        let g = political.values[1];
        assert_ne!(g, crate::game_data::NONE_GROUP);
        assert_eq!(political.groups[g as usize].key, "SWE");
        assert_eq!(political.groups[g as usize].label, "Sweden");
    }

    #[test]
    fn province_id_buffer_layout() {
        let Some(install) = real_install() else { return };
        let buf = province_id_buffer(&install).unwrap();
        let w = u32::from_le_bytes(buf[0..4].try_into().unwrap());
        let h = u32::from_le_bytes(buf[4..8].try_into().unwrap());
        assert_eq!(buf.len(), 8 + (w as usize) * (h as usize) * 2);
        assert!(w >= 5632 && h >= 2048, "unexpected dimensions {w}x{h}");
    }

    /// End-to-end total conversion check: Anbennar overlays the base game and
    /// renders with its own map, countries, and history.
    #[test]
    fn renders_anbennar_when_present() {
        let Some(vfs) = anbennar() else { return };
        let png = render_map_mode(&vfs, "political").expect("anbennar political render");
        assert!(png.len() > 100_000);

        let loc = crate::loc::build(&vfs);
        let political = crate::game_data::mode_data(&vfs, &loc, "political").unwrap();
        assert!(political.groups.len() > 100, "only {} groups", political.groups.len());
        // Anbennar replaces vanilla countries; vanilla-only tags like SWE
        // shouldn't own provinces there.
        assert!(!political.groups.iter().any(|g| g.key == "SWE"));
    }

    /// Not a check — dumps renders for manual inspection.
    /// Run with: cargo test dump_renders -- --ignored
    #[test]
    #[ignore]
    fn dump_renders() {
        let Some(install) = real_install() else { return };
        let dir = std::env::temp_dir().join("eu_toolkit_renders");
        std::fs::create_dir_all(&dir).unwrap();
        for mode in ["provinces", "political", "trade_nodes", "simple_terrain", "winter", "climate", "colonial_regions", "trade_companies"] {
            let png = render_map_mode(&install,mode).unwrap();
            std::fs::write(dir.join(format!("{mode}.png")), png).unwrap();
        }
    }

    // --- Simple Terrain (Sprint 11.2) ---

    /// Writes an 8bpp indexed BMP (bottom-up, 256-entry dummy palette) from
    /// top-down palette indices — the shape terrain.bmp uses.
    fn write_indexed_bmp(path: &Path, width: usize, height: usize, top_down: &[u8]) {
        assert_eq!(top_down.len(), width * height);
        let row_size = width.div_ceil(4) * 4;
        let data_off = 54 + 1024;
        let img_size = row_size * height;
        let mut b = Vec::new();
        b.extend_from_slice(b"BM");
        b.extend_from_slice(&((data_off + img_size) as u32).to_le_bytes());
        b.extend_from_slice(&0u32.to_le_bytes()); // reserved
        b.extend_from_slice(&(data_off as u32).to_le_bytes());
        b.extend_from_slice(&40u32.to_le_bytes()); // DIB header size
        b.extend_from_slice(&(width as i32).to_le_bytes());
        b.extend_from_slice(&(height as i32).to_le_bytes()); // positive = bottom-up
        b.extend_from_slice(&1u16.to_le_bytes()); // planes
        b.extend_from_slice(&8u16.to_le_bytes()); // bpp
        b.extend_from_slice(&0u32.to_le_bytes()); // compression
        b.extend_from_slice(&(img_size as u32).to_le_bytes());
        b.extend_from_slice(&0i32.to_le_bytes()); // xppm
        b.extend_from_slice(&0i32.to_le_bytes()); // yppm
        b.extend_from_slice(&256u32.to_le_bytes()); // colors used
        b.extend_from_slice(&0u32.to_le_bytes()); // colors important
        b.extend_from_slice(&[0u8; 1024]); // dummy palette (indices are what matter)
        for y in 0..height {
            let src_row = height - 1 - y; // bottom-up: first stored row is bottom
            for x in 0..width {
                b.push(top_down[src_row * width + x]);
            }
            for _ in width..row_size {
                b.push(0);
            }
        }
        std::fs::write(path, &b).unwrap();
    }

    /// Builds a 6x2 fixture install: 4 provinces. id1 majority grasslands,
    /// id2 majority desert, id3 auto-grasslands but terrain_override=marsh,
    /// id4 sea. Returns its Vfs.
    fn terrain_fixture(name: &str) -> Vfs {
        let root = std::env::temp_dir().join(format!("eu_toolkit_terrain_fixture_{name}"));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("map")).unwrap();

        // provinces.bmp (24bpp via the image crate so load_base can read it).
        let colors = [[10u8, 0, 0], [20, 0, 0], [30, 0, 0], [0, 0, 40]];
        let ids_grid = [1usize, 1, 1, 2, 2, 2, 3, 3, 3, 4, 4, 4]; // 6x2 top-down
        let mut img = image::RgbImage::new(6, 2);
        for (i, px) in img.pixels_mut().enumerate() {
            *px = image::Rgb(colors[ids_grid[i] - 1]);
        }
        img.save(root.join("map/provinces.bmp")).unwrap();

        // terrain.bmp indices (top-down): id1=[0,0,1] id2=[3,3,6] id3=[0,0,0] id4=[15,15,15]
        let terrain_idx = [0u8, 0, 1, 3, 3, 6, 0, 0, 0, 15, 15, 15];
        write_indexed_bmp(&root.join("map/terrain.bmp"), 6, 2, &terrain_idx);

        std::fs::write(
            root.join("map/definition.csv"),
            "province;red;green;blue;name;x\n1;10;0;0;A;x\n2;20;0;0;B;x\n3;30;0;0;C;x\n4;0;0;40;D;x\n",
        )
        .unwrap();
        std::fs::write(root.join("map/default.map"), "sea_starts = { 4 }\nlakes = { }\n").unwrap();
        std::fs::write(
            root.join("map/terrain.txt"),
            "categories = {\n\
               grasslands = { color = { 90 235 27 } type = plains movement_cost = 1.0 }\n\
               hills = { color = { 1 1 1 } type = hills defence = 1 }\n\
               desert = { color = { 1 1 1 } type = desert }\n\
               mountain = { color = { 1 1 1 } type = mountains defence = 2 movement_cost = 1.5 }\n\
               marsh = { color = { 1 1 1 } type = marsh terrain_override = { 3 } }\n\
               ocean = { color = { 1 1 1 } is_water = yes type = ocean movement_cost = 1.0 }\n\
             }\n\
             terrain = {\n\
               grasslands = { type = grasslands color = { 0 } }\n\
               hills = { type = hills color = { 1 } }\n\
               desert = { type = desert color = { 3 } }\n\
               mountain = { type = mountain color = { 6 } }\n\
               ocean = { type = ocean color = { 15 } }\n\
             }\n",
        )
        .unwrap();
        Vfs::new(root.to_str().unwrap(), None).unwrap()
    }

    #[test]
    fn effective_terrain_override_and_majority() {
        let vfs = terrain_fixture("classify");
        let eff = effective_terrain(&vfs).unwrap();
        // Majority classification from terrain.bmp.
        assert_eq!(eff.by_province.get(&1), Some(&("grasslands".to_string(), false)));
        assert_eq!(eff.by_province.get(&2), Some(&("desert".to_string(), false)));
        // terrain_override wins over the (grasslands) raster class.
        assert_eq!(eff.by_province.get(&3), Some(&("marsh".to_string(), true)));
        // Sea province: classified ocean but flagged water.
        assert!(eff.water.contains(&4));
    }

    #[test]
    fn simple_terrain_mode_data_excludes_water() {
        let vfs = terrain_fixture("modedata");
        let loc = crate::loc::LocStore::from_pairs(&[]);
        let data = game_data::mode_data(&vfs, &loc, "simple_terrain").unwrap();
        assert_eq!(data.kind, "categorical");
        let group = |id: u32| {
            let g = data.values[id as usize];
            (g != game_data::NONE_GROUP).then(|| data.groups[g as usize].key.clone())
        };
        assert_eq!(group(1).as_deref(), Some("grasslands"));
        assert_eq!(group(2).as_deref(), Some("desert"));
        assert_eq!(group(3).as_deref(), Some("marsh"));
        assert_eq!(group(4), None, "sea is groupless (renders as sea)");
        // Curated color flows into the group swatch.
        let g1 = data.values[1] as usize;
        assert_eq!(data.groups[g1].color, terrain_color("grasslands"));
    }

    #[test]
    fn simple_terrain_renders() {
        let vfs = terrain_fixture("render");
        let png = render_map_mode(&vfs, "simple_terrain").unwrap();
        assert_eq!(&png[..4], &[0x89, b'P', b'N', b'G']);
    }

    // --- Sprint 13.3 occupation stripes ---

    /// Builds a fixture install with one land province (`1`) owned by FRA and
    /// controlled by ENG (an occupation), plus their country colors. `size` is
    /// the square map edge (large enough to span several diagonal stripe bands).
    fn occupation_fixture(name: &str, size: u32) -> Vfs {
        let root = std::env::temp_dir().join(format!("eu_toolkit_occupation_{name}"));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("map")).unwrap();
        std::fs::create_dir_all(root.join("history/provinces")).unwrap();
        std::fs::create_dir_all(root.join("common/country_tags")).unwrap();
        std::fs::create_dir_all(root.join("common/countries")).unwrap();

        // A single-province map so the whole image is province 1 (no internal
        // borders); every pixel is a clean stripe band.
        let mut img = image::RgbImage::new(size, size);
        for px in img.pixels_mut() {
            *px = image::Rgb([10, 0, 0]);
        }
        img.save(root.join("map/provinces.bmp")).unwrap();
        std::fs::write(
            root.join("map/definition.csv"),
            "province;red;green;blue;name;x\n1;10;0;0;A;x\n",
        )
        .unwrap();
        std::fs::write(root.join("map/default.map"), "sea_starts = { }\nlakes = { }\n").unwrap();
        std::fs::write(
            root.join("history/provinces/1 - A.txt"),
            "owner = FRA\ncontroller = ENG\n",
        )
        .unwrap();
        std::fs::write(
            root.join("common/country_tags/00.txt"),
            "FRA = \"countries/France.txt\"\nENG = \"countries/England.txt\"\n",
        )
        .unwrap();
        std::fs::write(root.join("common/countries/France.txt"), "color = { 20 40 200 }\n").unwrap();
        std::fs::write(root.join("common/countries/England.txt"), "color = { 200 40 20 }\n").unwrap();
        Vfs::new(root.to_str().unwrap(), None).unwrap()
    }

    #[test]
    fn political_occupation_renders_both_stripe_colors() {
        let vfs = occupation_fixture("stripes", 32);
        let png = render_map_mode(&vfs, "political").unwrap();
        let img = image::load_from_memory(&png).unwrap().to_rgb8();
        let owner = image::Rgb([20u8, 40, 200]); // FRA (blue) band
        let controller = image::Rgb([200u8, 40, 20]); // ENG (red) band
        let mut saw_owner = false;
        let mut saw_controller = false;
        for p in img.pixels() {
            if *p == owner {
                saw_owner = true;
            }
            if *p == controller {
                saw_controller = true;
            }
        }
        assert!(saw_owner, "occupied province must show the owner stripe color");
        assert!(
            saw_controller,
            "occupied province must show the controller stripe color"
        );
        // Bands alternate every STRIPE_BAND px along (x+y): pixel (0,0) is owner
        // (band 0); (STRIPE_BAND,0) is controller (band 1).
        let at = |x: u32, y: u32| *img.get_pixel(x, y);
        assert_eq!(at(0, 0), owner, "band 0 = owner");
        assert_eq!(at(STRIPE_BAND as u32, 0), controller, "band 1 = controller");
    }

    #[test]
    fn political_no_occupation_has_no_stripes() {
        // Owner == controller → no stripes; the whole province is the owner color.
        let vfs = occupation_fixture("nostripe", 16);
        // Rewrite the province so controller matches owner.
        let mod_dir = std::env::temp_dir().join("eu_toolkit_occupation_nostripe");
        std::fs::write(
            mod_dir.join("history/provinces/1 - A.txt"),
            "owner = FRA\ncontroller = FRA\n",
        )
        .unwrap();
        let png = render_map_mode(&vfs, "political").unwrap();
        let img = image::load_from_memory(&png).unwrap().to_rgb8();
        let controller = image::Rgb([200u8, 40, 20]);
        assert!(
            !img.pixels().any(|p| *p == controller),
            "no occupation → the controller color must not appear"
        );
    }

    /// Writes a larger occupied render for eyeballing the diagonal stripes.
    /// Run with: cargo test dump_occupation_render -- --ignored
    #[test]
    #[ignore]
    fn dump_occupation_render() {
        let vfs = occupation_fixture("dump", 128);
        let png = render_map_mode(&vfs, "political").unwrap();
        let dir = std::env::temp_dir().join("eu_toolkit_renders");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("occupation_stripes.png"), png).unwrap();
    }

    #[test]
    fn map_modes_include_new_and_view_only() {
        let ids: Vec<&str> = MAP_MODES.iter().map(|(id, _)| *id).collect();
        assert!(ids.contains(&"simple_terrain"));
        assert!(ids.contains(&"winter"));
        // View-only flags: exactly the three raster modes.
        assert!(VIEW_ONLY_MODES.contains(&"terrain"));
        assert!(VIEW_ONLY_MODES.contains(&"heightmap"));
        assert!(VIEW_ONLY_MODES.contains(&"province_colors"));
        assert!(!VIEW_ONLY_MODES.contains(&"simple_terrain"));
        assert!(!VIEW_ONLY_MODES.contains(&"political"));
    }

    /// Real-install spot checks: known terrain_override provinces + timing.
    #[test]
    fn real_effective_terrain_spot_checks() {
        let Some(install) = real_install() else { return };
        let start = std::time::Instant::now();
        let eff = effective_terrain(&install).unwrap();
        let elapsed = start.elapsed();
        assert!(
            elapsed.as_secs_f32() < 2.0,
            "effective_terrain took {elapsed:?} (>2s)"
        );
        // 4175 is in the `mountain` terrain_override list (Balkans/Alps).
        assert_eq!(
            eff.by_province.get(&4175).map(|(c, _)| c.as_str()),
            Some("mountain"),
            "province 4175 should be an overridden mountain"
        );
        assert_eq!(eff.by_province.get(&4175).map(|(_, o)| *o), Some(true));
        // Stockholm (1) is Swedish land — classified, not water.
        assert!(eff.by_province.contains_key(&1));
        assert!(!eff.water.contains(&1));
    }

    #[test]
    fn real_winter_mode_groups() {
        let Some(install) = real_install() else { return };
        let loc = crate::loc::build(&install);
        let data = game_data::mode_data(&install, &loc, "winter").unwrap();
        assert_eq!(data.kind, "categorical");
        let keys: HashSet<&str> = data.groups.iter().map(|g| g.key.as_str()).collect();
        for w in ["mild_winter", "normal_winter", "severe_winter"] {
            assert!(keys.contains(w), "winter mode missing {w}");
        }
        // Winter mode must not leak zone/monsoon lists into its groups.
        assert!(!keys.contains("arctic"));
        assert!(!keys.contains("severe_monsoon"));
    }

    /// Anbennar smoke: its custom map + terrain classifies without panic; any
    /// custom terrain types get hash colors (never crash on an unknown key).
    #[test]
    fn anbennar_effective_terrain_smoke() {
        let Some(vfs) = anbennar() else { return };
        let eff = effective_terrain(&vfs).unwrap();
        assert!(!eff.by_province.is_empty());
        let png = render_map_mode(&vfs, "simple_terrain").expect("anbennar simple_terrain render");
        assert!(png.len() > 100_000);
    }

    #[test]
    fn real_game_data_loads() {
        let Some(install) = real_install() else { return };
        let countries = crate::game_data::country_colors(&install);
        assert!(countries.len() > 500, "only {} country colors", countries.len());
        assert!(countries.contains_key("SWE"));
        let religions = crate::game_data::religion_colors(&install);
        assert!(religions.contains_key("catholic"));
        let history = crate::game_data::province_history(&install);
        assert!(history.len() > 3000, "only {} province histories", history.len());
        // Stockholm (province 1) is Swedish in 1444.
        assert_eq!(history.get(&1).and_then(|s| s.owner.as_deref()), Some("SWE"));
        let nodes = crate::game_data::trade_nodes(&install);
        assert!(nodes.len() > 50, "only {} trade nodes", nodes.len());
        assert!(nodes.values().any(|(_, ids)| !ids.is_empty()));
    }
}
