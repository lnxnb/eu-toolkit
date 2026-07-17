//! S3.3 — trade-node mode province overlay data: per-province center-of-trade
//! tier + trade-relevant permanent modifiers, folded at the selected date.
//!
//! The game's trade mapmode decorates provinces with their trade state; this
//! derives the two pieces our overlay draws:
//! * **Center of trade** — the `center_of_trade = 1/2/3` history scalar (already
//!   modeled in [`crate::province_details`]), plus whether the province is
//!   **coastal** (via [`crate::map_renderer::coastal_land_ids`]) so the frontend
//!   picks the coastal vs inland tier icon the game's art distinguishes.
//! * **Trade-relevant permanent modifiers** — `add_permanent_province_modifier`
//!   entries whose modifier definition (the `event_modifiers` registry) contains a
//!   trade-power / trade-value family key (`trade_power`, `province_trade_power_*`,
//!   `trade_value`, `trade_value_modifier`, …). `river_estuary`/estuary modifiers
//!   are the canonical case (`province_trade_power_value = 10`).
//!
//! Date-folding mirrors the rest of the app: top-level history keys apply, then
//! every dated block with `date ≤ selected_date` in file order (later blocks are
//! ignored), so the overlay agrees with the map render + province panel at any
//! date. Only *decorated* provinces (a CoT tier or ≥1 trade modifier) are
//! returned, keeping the payload small.

use std::collections::{HashMap, HashSet};

use crate::date::{parse_date, Date};
#[cfg(test)]
use crate::date::DEFAULT_START;
use crate::loc::{self, LocStore};
use crate::map_renderer;
use crate::paradox::{Block, Value};
use crate::registry::{self, RawItem, RawValue};
use crate::vfs::Vfs;

/// A trade-relevant permanent modifier on a province, for the hover tooltip.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TradeModifierRef {
    /// The modifier's script key (e.g. `seine_estuary_modifier`).
    pub key: String,
    /// Localized display name (loc-resolved, else prettified key).
    pub name: String,
}

/// One decorated province's trade detail (only provinces with a CoT tier and/or
/// at least one trade modifier are emitted).
#[derive(Debug, Clone, serde::Serialize)]
pub struct ProvinceTradeDetail {
    pub id: u32,
    /// `center_of_trade` tier (1/2/3) as of the date, if any.
    pub cot: Option<i64>,
    /// Whether the province touches water (coastal) — selects the icon variant.
    pub coastal: bool,
    /// Trade-relevant permanent modifiers, in first-seen order.
    pub modifiers: Vec<TradeModifierRef>,
}

/// True if a modifier definition block carries a trade-power / trade-value family
/// key. Matches any key containing `trade_power` or `trade_value` (covers the
/// spec's `trade_power`, `province_trade_power_modifier`/`_value`, `trade_value`,
/// `trade_value_modifier`, and siblings). `trade_goods_size` is deliberately
/// excluded — it is a production modifier, not a trade-power/value key.
fn block_has_trade_key(raw: &RawValue) -> bool {
    let RawValue::Block(items) = raw else {
        return false;
    };
    items.iter().any(|RawItem { key, .. }| {
        key.as_deref().is_some_and(|k| {
            let k = k.to_ascii_lowercase();
            k.contains("trade_power") || k.contains("trade_value")
        })
    })
}

/// Modifier key → localized name for every `event_modifiers` entry whose
/// definition contains a trade-power/value key (the overlay's modifier filter).
pub fn trade_modifier_names(vfs: &Vfs, loc: &LocStore) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let Ok(entries) = registry::load_registry(vfs, loc, "event_modifiers") else {
        return out;
    };
    for e in entries {
        if block_has_trade_key(&e.raw) {
            out.insert(e.key, e.name);
        }
    }
    out
}

/// Extracts the `name = X` of an `add_permanent_province_modifier`/
/// `add_province_modifier` block (or a `remove_province_modifier`).
fn modifier_name(block: &Block) -> Option<String> {
    block.get_scalar("name").map(|s| s.to_string())
}

/// Accumulator threaded through top-level + dated history statements in file
/// order so add/remove and last-wins scalar semantics match the game.
#[derive(Default)]
struct Accum {
    cot: Option<i64>,
    /// Permanent-modifier names in first-seen order (a set guards duplicates).
    mods: Vec<String>,
    seen: HashSet<String>,
}

impl Accum {
    fn apply(&mut self, key: &str, value: &Value) {
        match (key, value) {
            ("center_of_trade", Value::Scalar(s)) => {
                if let Ok(n) = s.parse::<f64>() {
                    self.cot = Some(n as i64);
                }
            }
            ("add_permanent_province_modifier", Value::Block(b))
            | ("add_province_modifier", Value::Block(b)) => {
                if let Some(name) = modifier_name(b) {
                    if self.seen.insert(name.clone()) {
                        self.mods.push(name);
                    }
                }
            }
            ("remove_province_modifier", Value::Block(b)) => {
                if let Some(name) = modifier_name(b) {
                    self.mods.retain(|m| m != &name);
                    self.seen.remove(&name);
                }
            }
            _ => {}
        }
    }
}

/// Per-province trade detail as of `date` (S3.3). Folds top-level history keys
/// plus dated blocks ≤ `date`; keeps only trade-relevant modifiers.
pub fn trade_details_at(
    vfs: &Vfs,
    loc: &LocStore,
    date: Date,
) -> Result<Vec<ProvinceTradeDetail>, String> {
    let trade_mods = trade_modifier_names(vfs, loc);
    let coastal = map_renderer::coastal_land_ids(vfs)?;
    // Sea provinces are never decorated (spec S3.3): enforce it directly rather
    // than relying on CoT/estuary keys never appearing on water histories.
    let water = crate::game_data::water_ids(vfs);

    let mut out = Vec::new();
    for ast in crate::game_data::province_asts(vfs).iter() {
        let id = ast.id;
        if water.contains(&id) {
            continue;
        }
        let Some(block) = &ast.block else {
            continue;
        };

        let mut acc = Accum::default();
        // Top-level statements first (skip dated blocks).
        for (k, v) in &block.items {
            if let Some(k) = k {
                if parse_date(k).is_none() {
                    acc.apply(k, v);
                }
            }
        }
        // Then dated blocks with date ≤ selected date, in file order.
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

        // Keep only trade-relevant modifiers, mapped to display names.
        let modifiers: Vec<TradeModifierRef> = acc
            .mods
            .iter()
            .filter_map(|m| {
                trade_mods.get(m).map(|name| TradeModifierRef {
                    key: m.clone(),
                    name: name.clone(),
                })
            })
            .collect();

        // Water provinces were already skipped above; drop any land province with
        // neither a valid CoT tier nor a trade modifier (nothing to draw).
        let cot = acc.cot.filter(|&t| (1..=3).contains(&t));
        if cot.is_none() && modifiers.is_empty() {
            continue;
        }
        out.push(ProvinceTradeDetail {
            id,
            cot,
            coastal: coastal.contains(&id),
            modifiers,
        });
    }
    out.sort_by_key(|p| p.id);
    Ok(out)
}

/// Pre-Sprint-12 wrapper: trade details at the default start date. Tests only.
#[cfg(test)]
pub fn trade_details(vfs: &Vfs, loc: &LocStore) -> Result<Vec<ProvinceTradeDetail>, String> {
    trade_details_at(vfs, loc, DEFAULT_START)
}

/// Tauri command: the trade-node overlay data as of `date` (S3.3). Only decorated
/// provinces are returned.
#[tauri::command(async)]
pub fn get_trade_details(
    install_path: String,
    mod_path: Option<String>,
    date: Option<String>,
) -> Result<Vec<ProvinceTradeDetail>, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    let loc = loc::store(&vfs, &install_path, mod_path.as_deref());
    let at = crate::bookmarks::resolve_date(&vfs, date.as_deref())?;
    trade_details_at(&vfs, &loc, at)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    const INSTALL: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Europa Universalis IV";
    const ANBENNAR: &str = r"C:\Users\User\projects\anbennar-eu4-dev";

    fn real() -> Option<(Vfs, std::sync::Arc<LocStore>)> {
        if !Path::new(INSTALL).join("map").join("provinces.bmp").is_file() {
            return None;
        }
        let vfs = Vfs::new(INSTALL, None).unwrap();
        let loc = loc::store(&vfs, INSTALL, None);
        Some((vfs, loc))
    }

    #[test]
    fn trade_key_filter_picks_estuary_excludes_goods_size() {
        // province_trade_power_value → trade-relevant; trade_goods_size → not.
        let estuary = RawValue::Block(vec![RawItem {
            key: Some("province_trade_power_value".into()),
            value: RawValue::Scalar("10".into()),
        }]);
        let manufactory = RawValue::Block(vec![RawItem {
            key: Some("trade_goods_size".into()),
            value: RawValue::Scalar("0.5".into()),
        }]);
        assert!(block_has_trade_key(&estuary));
        assert!(!block_has_trade_key(&manufactory));
    }

    #[test]
    fn real_cot_tiers_and_estuary_filter() {
        let Some((vfs, loc)) = real() else { return };
        let details = trade_details(&vfs, &loc).unwrap();
        let by_id: HashMap<u32, &ProvinceTradeDetail> =
            details.iter().map(|d| (d.id, d)).collect();

        // 101 Genoa: CoT 3, coastal, no trade modifier.
        let genoa = by_id.get(&101).expect("Genoa is decorated (CoT 3)");
        assert_eq!(genoa.cot, Some(3));
        assert!(genoa.coastal, "Genoa is a coastal port");
        assert!(genoa.modifiers.is_empty());

        // 236 London: CoT 2 + thames_estuary_modifier (province_trade_power_value)
        // — the two-glyph case (icon + badge), coastal.
        let london = by_id.get(&236).expect("London is decorated");
        assert_eq!(london.cot, Some(2));
        assert!(london.coastal);
        assert!(
            london.modifiers.iter().any(|m| m.key == "thames_estuary_modifier"),
            "London carries the Thames estuary trade modifier"
        );

        // 167 Caux: no CoT, seine_estuary_modifier only (badge-only province).
        let caux = by_id.get(&167).expect("Caux is decorated (estuary)");
        assert_eq!(caux.cot, None);
        assert!(caux.modifiers.iter().any(|m| m.key == "seine_estuary_modifier"));

        // 183 Ile-de-France (Paris): CoT 2, inland; its birthplace_of_manufactories
        // modifier is trade_goods_size → excluded, so no badge.
        let paris = by_id.get(&183).expect("Paris is decorated (CoT 2)");
        assert_eq!(paris.cot, Some(2));
        assert!(!paris.coastal, "Ile-de-France is inland");
        assert!(paris.modifiers.is_empty(), "trade_goods_size is filtered out");
    }

    #[test]
    fn modifier_names_are_localized_and_trade_only() {
        let Some((vfs, loc)) = real() else { return };
        let names = trade_modifier_names(&vfs, &loc);
        // Estuary modifiers are in; a pure non-trade one is out.
        assert!(names.contains_key("seine_estuary_modifier"));
        assert!(names.contains_key("river_estuary_modifier"));
        assert!(!names.contains_key("birthplace_of_manufactories"));
        // Names resolve to something non-empty.
        assert!(names.values().all(|n| !n.is_empty()));
    }

    #[test]
    fn anbennar_smoke() {
        if !Path::new(INSTALL).join("map/provinces.bmp").is_file() || !Path::new(ANBENNAR).is_dir() {
            return;
        }
        let vfs = Vfs::new(INSTALL, Some(ANBENNAR)).unwrap();
        let loc = loc::store(&vfs, INSTALL, Some(ANBENNAR));
        // Anbennar has its own map + custom trade modifiers; the derivation must
        // pass them through the filter without panicking.
        let details = trade_details(&vfs, &loc).unwrap();
        // A total conversion still has centers of trade somewhere.
        assert!(details.iter().any(|d| d.cot.is_some()));
    }
}
