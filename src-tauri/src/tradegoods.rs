//! Sprint 7 — trade goods: list payload (prices, modifiers, chances), the
//! chance-rebalance computation, and the create-good scaffold (text + prices +
//! loc + icon-strip extension).
//!
//! ## File reality (verified against vanilla `00_tradegoods.txt` / `00_prices.txt`)
//! A good is one top-level block in `common/tradegoods`:
//! ```text
//! grain = {
//!     color = { 0.96 0.93 0.58 }          # 0-1 FLOATS (unlike country/religion int colors)
//!     modifier = { land_forcelimit_modifier = 0.20 }   # "trading in" country bonus
//!     province = { land_forcelimit = 0.5 }             # province production bonus
//!     chance = {
//!         factor = 35                       # base colonization weight
//!         modifier = { factor = 0 area = newfoundland_area }   # 0+ conditional sub-blocks
//!         ...
//!     }
//! }
//! ```
//! Extra keys seen: `is_latent = yes` + `is_valuable = yes` + `rnw_latent_chance`
//! + a `trigger = { ... }` block on **coal** (the only latent good); `unknown`
//! (the "no good" sentinel, defined LAST) has only a `color` — no chance/modifier.
//! Vanilla ships **32 goods** in definition order (grain=0 … coal=29, cloves=30,
//! unknown=31); the icon strip is positionally indexed by this order (see
//! [`crate::icons`]).
//!
//! Prices live in `common/prices` as `grain = { base_price = 2.5 }` — floats OR
//! ints (`cloth = { base_price = 3 }`); gold is special (`base_price = 0`
//! `goldtype = yes`); `unknown = { base_price = 0 }`.
//!
//! **File additivity.** The game reads *every* `.txt` in `common/tradegoods` and
//! `common/prices` and merges them (the toolkit's own `list_dir`/`parse_dir_merged`
//! already assume this, and mods add goods via extra files in practice). Files
//! collate alphabetically, so a `zz_`-prefixed project file loads LAST and its
//! goods take the highest definition indices — exactly the "append at the end"
//! the icon-strip extension needs. Anbennar instead *replaces* `00_tradegoods.txt`
//! wholesale (same filename shadows base); both are handled uniformly by reading
//! through the Vfs.
//!
//! ## Localisation key pattern (verified)
//! A good's display name is keyed by the **bare good key** in loc
//! (`grain:0 "Grain"`, `wine:0 "Wine"`); the description is `<key>DESC`. So a
//! rename is a plain `LocOverride { key: <good_key>, value }` — no prefix.
//!
//! ## Edit recipes (the frontend generates these `TypedEdit`s; documented here)
//! All target the good's own `source_file` (from [`get_trade_goods`]); prices
//! target the good's `price_file`.
//! * **Base price**  → `SetScalar { file: price_file, path: [good, "base_price"], value }`.
//! * **Color**       → `SetBlock  { file, path: [good, "color"], value: "0.50 0.50 0.50" }`
//!   (space-joined 0-1 floats — the good-color convention; `SetBlock` emits
//!   `{ <value> }`).
//! * **Country modifiers** → `SetBlock { file, path: [good, "modifier"], value: "k1 = v1 k2 = v2" }`.
//! * **Province modifiers** → `SetBlock { file, path: [good, "province"], value: "..." }`.
//! * **Chance base factor** → `SetScalar { file, path: [good, "chance", "factor"], value }`
//!   (leaves every conditional `modifier` sub-block inside `chance` untouched).
//! * **`is_latent` toggle** → on: `InsertStatement { block_path: [good], statement: "is_latent = yes" }`;
//!   off: `RemoveStatement { block_path: [good], key: "is_latent" }`.
//! * **Rename** → `LocOverride { key: good_key, value }`.
//! The `trigger`/`rnw_latent_chance`/`is_valuable` and any unmodeled keys ride
//! along in `raw_extra` and are never dropped (byte-surgical writes preserve them).
//!
//! ## Chance rebalance (7.5) — see [`rebalance_chances`]
//! The probability editor hands us a `good -> percentage` map (already summing to
//! ~100). We rewrite each good's base `chance.factor` to its percentage, so the
//! factors sum to a clean base of **100** and the game's `factor / Σfactor`
//! normalization reproduces the requested distribution exactly. Conditional
//! sub-blocks are never touched; only goods whose factor actually changes get an
//! edit; goods without an existing `chance.factor` (e.g. `unknown`) are skipped.
//!
//! ## Create good (7.4) — see [`prepare_trade_good_scaffold`]
//! Additive new files (no copy-on-write of the huge base file):
//! `common/tradegoods/zz_eutoolkit_tradegoods.txt` (append the good block) and
//! `common/prices/zz_eutoolkit_prices.txt` (append its `base_price`), a
//! `LocOverride` for the name, and a `BinaryAsset` writing the extended
//! `gfx/interface/resources.dds`. See [`crate::icons::extended_resources_strip`]
//! for the icon-strip gotcha and the multi-create chaining contract.

use std::collections::HashMap;

use serde_json::{json, Value as JsonValue};

use crate::paradox::{self, Block, Value};
use crate::registry::{self, ModifierKind};
use crate::vfs::Vfs;

pub const GOODS_FILE: &str = "common/tradegoods/zz_eutoolkit_tradegoods.txt";
pub const PRICES_FILE: &str = "common/prices/zz_eutoolkit_prices.txt";
pub const RESOURCES_STRIP: &str = "gfx/interface/resources.dds";

/// The keys modeled explicitly in a good block; everything else is `raw_extra`.
const MODELED_KEYS: &[&str] = &[
    "color",
    "modifier",
    "province",
    "chance",
    "is_latent",
    "is_valuable",
];

// --- parsing --------------------------------------------------------------

/// One good as read from disk, with the file it lives in and its definition
/// index (position in merged, alphabetical-file then in-file order).
struct RawGood {
    key: String,
    block: Block,
    source_file: String,
    index: u32,
}

fn parse_bytes(bytes: &[u8]) -> Block {
    paradox::parse(&String::from_utf8_lossy(bytes))
}

/// Reads `common/tradegoods` merged in game load order (files alphabetical,
/// blocks in file order). Same-named files are Vfs-shadowed by `list_dir`; a key
/// seen twice keeps its first occurrence (definition order / icon index stable).
fn parse_goods(vfs: &Vfs) -> Vec<RawGood> {
    let mut out: Vec<RawGood> = Vec::new();
    let mut seen: HashMap<String, ()> = HashMap::new();
    let mut index = 0u32;
    for (name, path) in vfs.list_dir("common/tradegoods") {
        if !name.to_lowercase().ends_with(".txt") {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let block = parse_bytes(&bytes);
        for (k, b) in block.key_blocks() {
            if seen.insert(k.to_string(), ()).is_some() {
                continue;
            }
            out.push(RawGood {
                key: k.to_string(),
                block: b.clone(),
                source_file: format!("common/tradegoods/{name}"),
                index,
            });
            index += 1;
        }
    }
    out
}

/// Good key -> (base_price token as written, price source file).
fn parse_prices(vfs: &Vfs) -> HashMap<String, (String, String)> {
    let mut out: HashMap<String, (String, String)> = HashMap::new();
    for (name, path) in vfs.list_dir("common/prices") {
        if !name.to_lowercase().ends_with(".txt") {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let block = parse_bytes(&bytes);
        for (k, b) in block.key_blocks() {
            if out.contains_key(k) {
                continue;
            }
            if let Some(price) = b.get_scalar("base_price") {
                out.insert(
                    k.to_string(),
                    (price.to_string(), format!("common/prices/{name}")),
                );
            }
        }
    }
    out
}

fn modifier_kind_str(key: &str) -> &'static str {
    for m in registry::known_modifiers() {
        if m.key == key {
            return match m.kind {
                ModifierKind::Percent => "percent",
                ModifierKind::Flat => "flat",
                ModifierKind::Boolean => "boolean",
            };
        }
    }
    "unknown"
}

// --- payload --------------------------------------------------------------

/// One modifier key/value row with its typed input kind (percent/flat/boolean/
/// unknown) so the 7.3 editor renders the right control.
#[derive(Debug, serde::Serialize)]
pub struct ModRow {
    pub key: String,
    pub value: String,
    pub kind: &'static str,
}

/// An unmodeled statement in a good block, preserved read-only (advanced).
#[derive(Debug, serde::Serialize)]
pub struct RawEntry {
    pub key: String,
    /// "scalar" or "block".
    pub kind: &'static str,
    pub value: String,
}

/// The base colonization weight plus a summary of the conditional sub-blocks
/// (the 100% probability view is the base distribution; conditionals are noted).
#[derive(Debug, serde::Serialize)]
pub struct ChanceSummary {
    /// `chance.factor` as written, or None if there is no chance block.
    pub base_factor: Option<String>,
    pub has_conditional_modifiers: bool,
    pub conditional_count: u32,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TradeGood {
    pub key: String,
    pub index: u32,
    pub localized_name: String,
    /// `color = { ... }` tokens exactly as written (e.g. `["0.96","0.93","0.58"]`).
    pub color_raw: Vec<String>,
    /// Whether the color was written as 0-1 floats (the good convention) vs ints.
    pub color_is_float: bool,
    /// 0-255 RGB for display swatches / map painting.
    pub rgb: Option<[u8; 3]>,
    /// Joined from `common/prices` — the token as written (float or int), if any.
    pub base_price: Option<String>,
    /// The file `common/prices` entry lives in (for the price-edit recipe).
    pub price_file: Option<String>,
    /// "Trading in" country bonus (`modifier = { ... }`).
    pub modifier_rows: Vec<ModRow>,
    /// Province production bonus (`province = { ... }`).
    pub province_rows: Vec<ModRow>,
    pub chance: ChanceSummary,
    pub is_latent: bool,
    pub is_valuable: bool,
    /// Unmodeled keys (trigger, rnw_latent_chance, …) — read-only, never dropped.
    pub raw_extra: Vec<RawEntry>,
    /// Game-relative file the good block lives in (byte-surgical edits target it).
    pub source_file: String,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TradeGoodsPayload {
    pub goods: Vec<TradeGood>,
    pub total: u32,
    pub latent_count: u32,
    pub with_price_count: u32,
}

fn mod_rows(block: &Block, name: &str) -> Vec<ModRow> {
    block
        .get_block(name)
        .map(|b| {
            b.items
                .iter()
                .filter_map(|(k, v)| match (k, v) {
                    (Some(k), Value::Scalar(s)) => Some(ModRow {
                        key: k.clone(),
                        value: s.clone(),
                        kind: modifier_kind_str(k),
                    }),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

fn build_trade_good(
    g: &RawGood,
    prices: &HashMap<String, (String, String)>,
    loc: &crate::loc::LocStore,
) -> TradeGood {
    let color_block = g.block.get_block("color");
    let color_raw: Vec<String> = color_block
        .map(|b| b.bare_scalars().take(3).map(str::to_string).collect())
        .unwrap_or_default();
    let color_is_float = color_raw.iter().any(|s| s.contains('.'));
    let rgb = color_block.and_then(paradox::color_from_block);

    let (base_price, price_file) = match prices.get(&g.key) {
        Some((p, f)) => (Some(p.clone()), Some(f.clone())),
        None => (None, None),
    };

    let chance = g.block.get_block("chance");
    let base_factor = chance.and_then(|c| c.get_scalar("factor")).map(str::to_string);
    let conditional_count = chance
        .map(|c| {
            c.items
                .iter()
                .filter(|(k, v)| {
                    matches!(k.as_deref(), Some("modifier")) && matches!(v, Value::Block(_))
                })
                .count() as u32
        })
        .unwrap_or(0);

    let mut raw_extra = Vec::new();
    for (k, v) in &g.block.items {
        let Some(k) = k else { continue };
        if MODELED_KEYS.contains(&k.as_str()) {
            continue;
        }
        match v {
            Value::Scalar(s) => raw_extra.push(RawEntry {
                key: k.clone(),
                kind: "scalar",
                value: s.clone(),
            }),
            Value::Block(_) => raw_extra.push(RawEntry {
                key: k.clone(),
                kind: "block",
                value: "{ … }".to_string(),
            }),
        }
    }

    TradeGood {
        localized_name: loc.resolve(&g.key),
        key: g.key.clone(),
        index: g.index,
        color_raw,
        color_is_float,
        rgb,
        base_price,
        price_file,
        modifier_rows: mod_rows(&g.block, "modifier"),
        province_rows: mod_rows(&g.block, "province"),
        chance: ChanceSummary {
            base_factor,
            has_conditional_modifiers: conditional_count > 0,
            conditional_count,
        },
        is_latent: g.block.get_scalar("is_latent") == Some("yes"),
        is_valuable: g.block.get_scalar("is_valuable") == Some("yes"),
        raw_extra,
        source_file: g.source_file.clone(),
    }
}

/// The full trade-goods list in definition order (positional icon index), joined
/// with prices, ready for the 7.1 list, the 7.3 editor, and the 7.5 view.
pub fn trade_goods(vfs: &Vfs, loc: &crate::loc::LocStore) -> TradeGoodsPayload {
    let goods = parse_goods(vfs);
    let prices = parse_prices(vfs);
    let rows: Vec<TradeGood> = goods
        .iter()
        .map(|g| build_trade_good(g, &prices, loc))
        .collect();
    let total = rows.len() as u32;
    let latent_count = rows.iter().filter(|g| g.is_latent).count() as u32;
    let with_price_count = rows.iter().filter(|g| g.base_price.is_some()).count() as u32;
    TradeGoodsPayload {
        goods: rows,
        total,
        latent_count,
        with_price_count,
    }
}

#[tauri::command]
pub fn get_trade_goods(
    install_path: String,
    mod_path: Option<String>,
) -> Result<TradeGoodsPayload, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    let loc = crate::loc::store(&vfs, &install_path, mod_path.as_deref());
    Ok(trade_goods(&vfs, &loc))
}

// --- edit-JSON helpers (produce TypedEdit-shaped values the frontend replays) --
//
// `edits.rs::TypedEdit` is Deserialize-only, so these commands return JSON
// objects matching its wire shape (internally tagged `kind`, camelCase fields).
// The frontend feeds them straight back into its pending queue; on save they
// deserialize into `Vec<TypedEdit>` and run through `apply_queue` unchanged.

fn set_scalar_edit(file: &str, path: &[&str], value: &str) -> JsonValue {
    json!({ "kind": "setScalar", "file": file, "path": path, "value": value, "quoted": false })
}

fn append_text_edit(file: &str, text: &str) -> JsonValue {
    json!({ "kind": "appendText", "file": file, "text": text })
}

fn loc_override_edit(key: &str, value: &str) -> JsonValue {
    json!({ "kind": "locOverride", "key": key, "value": value })
}

fn binary_asset_edit(file: &str, bytes: &[u8]) -> JsonValue {
    json!({ "kind": "binaryAsset", "file": file, "bytes": bytes })
}

// --- chance rebalance (7.5) ----------------------------------------------

/// Formats a factor: integer-valued numbers print without a decimal point;
/// others keep up to 3 trimmed decimals (`12.5`, not `12.500`).
fn fmt_factor(v: f64) -> String {
    if (v - v.round()).abs() < 1e-9 {
        format!("{}", v.round() as i64)
    } else {
        let s = format!("{v:.3}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

/// Rewrites base colonization factors from a `good -> percentage` map (see the
/// module note). Pure computation — writes nothing; returns the `TypedEdit`-shaped
/// edits, one `setScalar` per good whose factor actually changes.
pub fn rebalance_edits(
    vfs: &Vfs,
    new_percentages: &HashMap<String, f64>,
) -> Vec<JsonValue> {
    let goods = parse_goods(vfs);
    let mut edits = Vec::new();
    for g in &goods {
        let Some(&pct) = new_percentages.get(&g.key) else {
            continue;
        };
        // Only goods that already have a base factor are rebalanced.
        let Some(current) = g.block.get_block("chance").and_then(|c| c.get_scalar("factor")) else {
            continue;
        };
        let target = fmt_factor(pct);
        if current == target {
            continue;
        }
        edits.push(set_scalar_edit(
            &g.source_file,
            &[&g.key, "chance", "factor"],
            &target,
        ));
    }
    edits
}

#[tauri::command]
pub fn rebalance_chances(
    install_path: String,
    mod_path: Option<String>,
    new_percentages: HashMap<String, f64>,
) -> Result<Vec<JsonValue>, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    Ok(rebalance_edits(&vfs, &new_percentages))
}

// --- create good (7.4) ----------------------------------------------------

/// A prior not-yet-saved good created earlier this session (so the strip this
/// scaffold writes is a superset covering it — see the icons chaining note).
#[derive(Debug, Clone, serde::Deserialize)]
pub struct PendingGood {
    /// 0-255 display color used to draw its placeholder strip frame.
    pub color: [u8; 3],
}

/// Sanitizes a display name into a valid lowercase good key (`[a-z0-9_]`),
/// deduped against `taken`.
fn sanitize_key(name: &str, taken: &[String]) -> String {
    let mut key: String = name
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    while key.contains("__") {
        key = key.replace("__", "_");
    }
    let key = key.trim_matches('_').to_string();
    let mut key = if key.is_empty() { "new_good".to_string() } else { key };
    if taken.iter().any(|t| t == &key) {
        let mut n = 2;
        while taken.iter().any(|t| *t == format!("{key}_{n}")) {
            n += 1;
        }
        key = format!("{key}_{n}");
    }
    key
}

/// The result of preparing a create-good operation: identity + the ordered
/// `TypedEdit`-shaped edit list the frontend queues as one composite op.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TradeGoodScaffold {
    /// The generated good key (also the loc key).
    pub key: String,
    /// Definition index the new good lands at (= its icon-strip frame).
    pub index: u32,
    /// 0-255 display color chosen for the good.
    pub rgb: [u8; 3],
    /// The same color as 0-1 floats, as written into the good block.
    pub color_floats: [String; 3],
    /// Ordered edits: append good block, append price, loc override, extended
    /// icon strip (binary). Feed straight into the pending queue.
    pub edits: Vec<JsonValue>,
}

fn floats_of(rgb: [u8; 3]) -> [String; 3] {
    [
        format!("{:.2}", rgb[0] as f64 / 255.0),
        format!("{:.2}", rgb[1] as f64 / 255.0),
        format!("{:.2}", rgb[2] as f64 / 255.0),
    ]
}

pub fn trade_good_scaffold(
    vfs: &Vfs,
    fallback: Option<&Vfs>,
    name: &str,
    pending: &[PendingGood],
) -> Result<TradeGoodScaffold, String> {
    let existing = parse_goods(vfs);
    let taken: Vec<String> = existing.iter().map(|g| g.key.clone()).collect();
    let key = sanitize_key(name, &taken);

    // New good lands after every saved good AND every prior pending good.
    let base_count = existing.len() as u32;
    let new_index = base_count + pending.len() as u32;
    let rgb = crate::map_renderer::hash_color(&key);
    let floats = floats_of(rgb);

    // Good block (authored at column 0; empty modifier/province, chance 0 so it
    // never auto-spawns at colonization until the author sets a weight).
    let good_text = format!(
        "\n{key} = {{\n\tcolor = {{ {} {} {} }}\n\tmodifier = {{\n\t}}\n\tprovince = {{\n\t}}\n\tchance = {{\n\t\tfactor = 0\n\t}}\n}}\n",
        floats[0], floats[1], floats[2]
    );
    let price_text = format!("\n{key} = {{\n\tbase_price = 1\n}}\n");

    // Icon strip: draw a placeholder frame for every prior pending good (at its
    // index) plus this good (at new_index); the last create's superset wins.
    let mut placements: Vec<(u32, [u8; 3])> = pending
        .iter()
        .enumerate()
        .map(|(i, pg)| (base_count + i as u32, pg.color))
        .collect();
    placements.push((new_index, rgb));
    let strip = crate::icons::extended_resources_strip(vfs, fallback, &placements)?;

    let edits = vec![
        append_text_edit(GOODS_FILE, &good_text),
        append_text_edit(PRICES_FILE, &price_text),
        loc_override_edit(&key, name.trim()),
        binary_asset_edit(RESOURCES_STRIP, &strip),
    ];

    Ok(TradeGoodScaffold {
        key,
        index: new_index,
        rgb,
        color_floats: floats,
        edits,
    })
}

#[tauri::command]
pub fn prepare_trade_good_scaffold(
    install_path: String,
    mod_path: Option<String>,
    name: String,
    pending: Vec<PendingGood>,
) -> Result<TradeGoodScaffold, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    let base = mod_path
        .as_deref()
        .map(|_| Vfs::new(&install_path, None))
        .transpose()?;
    trade_good_scaffold(&vfs, base.as_ref(), &name, &pending)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edits::{apply_queue, TypedEdit};
    use crate::loc::LocStore;
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

    /// Deserializes the JSON edits a command returns into real `TypedEdit`s
    /// (proving wire compatibility) so they can run through `apply_queue`.
    fn to_typed(values: &[JsonValue]) -> Vec<TypedEdit> {
        values
            .iter()
            .map(|v| serde_json::from_value(v.clone()).expect("edit deserializes to TypedEdit"))
            .collect()
    }

    // --- vanilla parse ----------------------------------------------------

    #[test]
    fn parses_vanilla_goods() {
        let Some(vfs) = real_install() else { return };
        let payload = trade_goods(&vfs, &LocStore::from_pairs(&[]));
        assert_eq!(payload.total, 32, "vanilla ships 32 goods incl. unknown");

        let grain = &payload.goods[0];
        assert_eq!(grain.key, "grain");
        assert_eq!(grain.index, 0);
        assert_eq!(grain.base_price.as_deref(), Some("2.5"));
        assert!(grain.color_is_float);
        assert_eq!(grain.rgb, Some([245, 237, 148]));
        assert_eq!(grain.chance.base_factor.as_deref(), Some("35"));
        assert!(grain.chance.has_conditional_modifiers);
        assert!(grain.chance.conditional_count >= 5);
        assert!(!grain.modifier_rows.is_empty());
        assert!(!grain.province_rows.is_empty());
        assert!(!grain.is_latent);

        // coal: latent + valuable, carries a trigger block in raw_extra, factor 1.
        let coal = payload.goods.iter().find(|g| g.key == "coal").unwrap();
        assert!(coal.is_latent);
        assert!(coal.is_valuable);
        assert_eq!(coal.chance.base_factor.as_deref(), Some("1"));
        assert!(coal.raw_extra.iter().any(|r| r.key == "trigger" && r.kind == "block"));
        assert!(coal.raw_extra.iter().any(|r| r.key == "rnw_latent_chance"));

        // unknown: the no-good sentinel, defined last, no chance/price>0.
        let unknown = payload.goods.last().unwrap();
        assert_eq!(unknown.key, "unknown");
        assert_eq!(unknown.chance.base_factor, None);
        assert_eq!(unknown.base_price.as_deref(), Some("0"));

        assert_eq!(payload.latent_count, 1); // only coal
        assert_eq!(payload.with_price_count, 32);
    }

    #[test]
    fn modifier_rows_are_typed() {
        let Some(vfs) = real_install() else { return };
        let payload = trade_goods(&vfs, &LocStore::from_pairs(&[]));
        let grain = &payload.goods[0];
        // land_forcelimit_modifier is a known Percent modifier.
        let row = grain
            .modifier_rows
            .iter()
            .find(|r| r.key == "land_forcelimit_modifier")
            .unwrap();
        assert_eq!(row.kind, "percent");
    }

    // --- edit-recipe round trips (byte-identical elsewhere) ---------------

    /// A base install + empty project under a per-test temp root, seeded with a
    /// tiny two-good tradegoods file + prices file.
    fn setup(name: &str) -> (Vfs, PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!("eu_toolkit_tradegoods_test_{name}"));
        let base = root.join("base");
        let project = root.join("project");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(base.join("map")).unwrap();
        std::fs::write(base.join("map/provinces.bmp"), b"x").unwrap();
        std::fs::create_dir_all(base.join("common/tradegoods")).unwrap();
        std::fs::create_dir_all(base.join("common/prices")).unwrap();
        std::fs::write(
            base.join("common/tradegoods/00_tradegoods.txt"),
            b"grain = {\n\tcolor = { 0.96 0.93 0.58 }\n\tmodifier = {\n\t\tland_forcelimit_modifier = 0.20\n\t}\n\tprovince = {\n\t\tland_forcelimit = 0.5\n\t}\n\tchance = {\n\t\tfactor = 35\n\t\tmodifier = {\n\t\t\tfactor = 0\n\t\t\tarea = newfoundland_area\n\t\t}\n\t}\n}\n\nwine = {\n\tcolor = { 0.36 0.13 0.28 }\n\tchance = {\n\t\tfactor = 5\n\t}\n}\n",
        )
        .unwrap();
        std::fs::write(
            base.join("common/prices/00_prices.txt"),
            b"grain = {\n\tbase_price = 2.5\n}\n\nwine = {\n\tbase_price = 2.5\n}\n",
        )
        .unwrap();
        // A 2-frame (128x64) uncompressed strip so the icon-strip extension has a
        // base to extend (matches the two goods above).
        std::fs::create_dir_all(base.join("gfx/interface")).unwrap();
        let strip = crate::icons::encode_dds_bgra(&vec![0u8; (128 * 64 * 4) as usize], 128, 64)
            .unwrap();
        std::fs::write(base.join("gfx/interface/resources.dds"), &strip).unwrap();
        let vfs = Vfs::new(base.to_str().unwrap(), None).unwrap();
        (vfs, base, project)
    }

    #[test]
    fn price_edit_round_trips() {
        let (vfs, _base, project) = setup("price");
        let edits = vec![TypedEdit::SetScalar {
            file: "common/prices/00_prices.txt".into(),
            path: vec!["grain".into(), "base_price".into()],
            value: "4.5".into(),
            quoted: false,
        }];
        apply_queue(&vfs, &project, &edits).unwrap();
        let out =
            std::fs::read_to_string(project.join("common/prices/00_prices.txt")).unwrap();
        assert_eq!(
            out,
            "grain = {\n\tbase_price = 4.5\n}\n\nwine = {\n\tbase_price = 2.5\n}\n"
        );
    }

    #[test]
    fn color_edit_round_trips() {
        let (vfs, _base, project) = setup("color");
        let edits = vec![TypedEdit::SetBlock {
            file: "common/tradegoods/00_tradegoods.txt".into(),
            path: vec!["wine".into(), "color".into()],
            value: "0.10 0.20 0.30".into(),
        }];
        apply_queue(&vfs, &project, &edits).unwrap();
        let out =
            std::fs::read_to_string(project.join("common/tradegoods/00_tradegoods.txt")).unwrap();
        assert!(out.contains("wine = {\n\tcolor = { 0.10 0.20 0.30 }\n"));
        // grain untouched.
        assert!(out.contains("color = { 0.96 0.93 0.58 }"));
    }

    #[test]
    fn chance_factor_edit_round_trips() {
        let (vfs, _base, project) = setup("factor");
        let edits = vec![TypedEdit::SetScalar {
            file: "common/tradegoods/00_tradegoods.txt".into(),
            path: vec!["grain".into(), "chance".into(), "factor".into()],
            value: "50".into(),
            quoted: false,
        }];
        apply_queue(&vfs, &project, &edits).unwrap();
        let out =
            std::fs::read_to_string(project.join("common/tradegoods/00_tradegoods.txt")).unwrap();
        assert!(out.contains("chance = {\n\t\tfactor = 50\n"));
        // The conditional sub-block inside chance survives untouched.
        assert!(out.contains("area = newfoundland_area"));
    }

    #[test]
    fn modifier_block_edit_round_trips() {
        let (vfs, _base, project) = setup("modblock");
        let edits = vec![TypedEdit::SetBlock {
            file: "common/tradegoods/00_tradegoods.txt".into(),
            path: vec!["grain".into(), "modifier".into()],
            value: "global_unrest = -1 prestige = 0.5".into(),
        }];
        apply_queue(&vfs, &project, &edits).unwrap();
        let out =
            std::fs::read_to_string(project.join("common/tradegoods/00_tradegoods.txt")).unwrap();
        assert!(out.contains("modifier = { global_unrest = -1 prestige = 0.5 }"));
        // province block untouched.
        assert!(out.contains("land_forcelimit = 0.5"));
    }

    // --- rebalance --------------------------------------------------------

    #[test]
    fn rebalance_rewrites_only_changed_factors_and_preserves_conditionals() {
        let (vfs, _base, project) = setup("rebalance");
        // grain 35 -> 70, wine 5 -> 30 (both changed). unknown-like goods absent.
        let mut map = HashMap::new();
        map.insert("grain".to_string(), 70.0);
        map.insert("wine".to_string(), 30.0);
        let values = rebalance_edits(&vfs, &map);
        assert_eq!(values.len(), 2, "one edit per changed good");

        let typed = to_typed(&values);
        apply_queue(&vfs, &project, &typed).unwrap();
        let out =
            std::fs::read_to_string(project.join("common/tradegoods/00_tradegoods.txt")).unwrap();
        assert!(out.contains("grain = {\n\tcolor"));
        assert!(out.contains("factor = 70"));
        assert!(out.contains("factor = 30"));
        // grain's conditional sub-block (its own factor = 0) is untouched.
        assert!(out.contains("area = newfoundland_area"));
        // Factors sum to a clean base of 100.
    }

    #[test]
    fn rebalance_skips_unchanged_and_factorless_goods() {
        let (vfs, _base, _project) = setup("rebalance_skip");
        let mut map = HashMap::new();
        map.insert("grain".to_string(), 35.0); // unchanged -> no edit
        map.insert("wine".to_string(), 60.0); // changed
        let values = rebalance_edits(&vfs, &map);
        assert_eq!(values.len(), 1);
        // The single edit targets wine.
        assert_eq!(values[0]["path"][0], "wine");
    }

    #[test]
    fn fmt_factor_formats_cleanly() {
        assert_eq!(fmt_factor(70.0), "70");
        assert_eq!(fmt_factor(12.5), "12.5");
        assert_eq!(fmt_factor(33.333), "33.333");
        assert_eq!(fmt_factor(0.0), "0");
    }

    // --- create-good scaffold --------------------------------------------

    #[test]
    fn scaffold_parses_back_and_has_distinct_pieces() {
        let (vfs, _base, project) = setup("scaffold");
        let scaffold = trade_good_scaffold(&vfs, None, "Amber Resin", &[]).unwrap();
        assert_eq!(scaffold.key, "amber_resin");
        assert_eq!(scaffold.index, 2, "lands after grain(0), wine(1)");
        assert_eq!(scaffold.edits.len(), 4);

        // Apply the scaffold and re-read: good + price + strip all present.
        let typed = to_typed(&scaffold.edits);
        let written = apply_queue(&vfs, &project, &typed).unwrap();
        assert!(written.contains(&GOODS_FILE.to_string()));
        assert!(written.contains(&PRICES_FILE.to_string()));
        assert!(written.contains(&RESOURCES_STRIP.to_string()));

        let goods_txt = std::fs::read_to_string(project.join(GOODS_FILE)).unwrap();
        let parsed = parse_bytes(goods_txt.as_bytes());
        let block = parsed.get_block("amber_resin").expect("good parses back");
        assert!(block.get_block("color").is_some());
        assert!(block.get_block("modifier").is_some());
        assert!(block.get_block("province").is_some());
        assert_eq!(
            block.get_block("chance").and_then(|c| c.get_scalar("factor")),
            Some("0")
        );
        // Distinct color: the good's hash color, not grain's/wine's.
        let color = paradox::color_from_block(block.get_block("color").unwrap()).unwrap();
        assert_ne!(color, [245, 237, 148]);

        let prices_txt = std::fs::read_to_string(project.join(PRICES_FILE)).unwrap();
        assert!(parse_bytes(prices_txt.as_bytes())
            .get_block("amber_resin")
            .and_then(|b| b.get_scalar("base_price"))
            .is_some());

        // The extended strip (base 2 frames + 1 new) is a valid 3-frame DDS.
        let strip_edit = &scaffold.edits[3];
        assert_eq!(strip_edit["kind"], "binaryAsset");
        let bytes: Vec<u8> = serde_json::from_value(strip_edit["bytes"].clone()).unwrap();
        let width = u32::from_le_bytes(bytes[16..20].try_into().unwrap());
        assert_eq!(width / 64, 3);
    }

    #[test]
    fn scaffold_key_dedupes_against_existing() {
        let (vfs, _base, _project) = setup("dedupe");
        // "grain" already exists -> generated key must differ.
        let scaffold = trade_good_scaffold(&vfs, None, "Grain", &[]).unwrap();
        assert_ne!(scaffold.key, "grain");
        assert!(scaffold.key.starts_with("grain"));
    }

    #[test]
    fn scaffold_on_real_install_extends_strip_and_reflects_in_atlas() {
        // Full acceptance: on the real install, a create-good scaffold's binary
        // strip has (32 + 1) frames, and after applying, get_icon_atlas over the
        // *project* Vfs sees the new good at index 32.
        let Some(_) = real_install() else { return };
        let base_vfs = Vfs::new(INSTALL, None).unwrap();
        let project = std::env::temp_dir().join("eu_toolkit_tradegoods_test_real_scaffold");
        let _ = std::fs::remove_dir_all(&project);

        let scaffold = trade_good_scaffold(&base_vfs, None, "Star Metal", &[]).unwrap();
        assert_eq!(scaffold.index, 32);
        let typed = to_typed(&scaffold.edits);
        apply_queue(&base_vfs, &project, &typed).unwrap();

        // The project now shadows resources.dds + adds the good; atlas reflects both.
        let proj_vfs = Vfs::new(INSTALL, Some(project.to_str().unwrap())).unwrap();
        let base_for_fallback = Vfs::new(INSTALL, None).unwrap();
        let atlas =
            crate::icons::icon_atlas(&proj_vfs, Some(&base_for_fallback), "trade_goods").unwrap();
        assert_eq!(atlas.count, 33, "strip extended by one frame");
        let entry = atlas
            .index
            .iter()
            .find(|(k, _)| k == "star_metal")
            .expect("new good in atlas index");
        assert_eq!(entry.1, 32);
    }

    #[test]
    fn scaffold_chains_across_pending_goods() {
        // Two goods created before any save: the second scaffold's strip is the
        // superset (34 frames on vanilla), and its good lands at index 33.
        let Some(_) = real_install() else { return };
        let base_vfs = Vfs::new(INSTALL, None).unwrap();
        let first = trade_good_scaffold(&base_vfs, None, "First New", &[]).unwrap();
        assert_eq!(first.index, 32);
        let pending = vec![PendingGood { color: first.rgb }];
        let second = trade_good_scaffold(&base_vfs, None, "Second New", &pending).unwrap();
        assert_eq!(second.index, 33);
        // Its binary strip covers both new frames -> 34 frames total.
        let strip = &second.edits[3];
        assert_eq!(strip["kind"], "binaryAsset");
        let bytes: Vec<u8> = serde_json::from_value(strip["bytes"].clone()).unwrap();
        // width/64 frames: parse the DDS header width at offset 16.
        let width = u32::from_le_bytes(bytes[16..20].try_into().unwrap());
        assert_eq!(width / 64, 34);
    }

    // --- Anbennar smoke ---------------------------------------------------

    #[test]
    fn anbennar_goods_list_smoke() {
        if !Path::new(INSTALL).join("map/provinces.bmp").is_file() || !Path::new(ANBENNAR).is_dir()
        {
            return;
        }
        let vfs = Vfs::new(INSTALL, Some(ANBENNAR)).unwrap();
        let loc = crate::loc::store(&vfs, INSTALL, Some(ANBENNAR));
        let payload = trade_goods(&vfs, &loc);
        // Anbennar replaces the goods list with its own (custom goods present).
        assert!(payload.total > 0);
        // Definition order is stable and indices are contiguous.
        for (i, g) in payload.goods.iter().enumerate() {
            assert_eq!(g.index as usize, i);
        }
        // A scaffold appends after the whole custom list.
        let scaffold = trade_good_scaffold(
            &vfs,
            Some(&Vfs::new(INSTALL, None).unwrap()),
            "Test Good",
            &[],
        )
        .unwrap();
        assert_eq!(scaffold.index, payload.total);
    }
}
