//! Undiscovered-goods spawn clusters for the Trade Goods map mode.
//!
//! Provinces with `trade_goods = unknown` used to form ONE mode-data group, so
//! clicking any uncolonized province selected the whole uncolonized world. The
//! game decides the actual good on colonization by weighting each good's
//! `chance = { factor … modifier = { factor <trigger> } }` block
//! (common/tradegoods) against the province. This module evaluates those
//! chances statically per province, then groups **adjacent** unknown provinces
//! sharing an identical weight vector into clusters — the new mode-data group
//! granularity, so a click selects one contiguous same-distribution patch and
//! the hover/selection label names the likely goods.
//!
//! ## Static approximation
//! Colonizer-dependent conditions (`FROM`, `colonial_parent`,
//! `has_increased_trade_goods_discovery`, `holy_order`) and `island` are
//! *unknown*; `has_country_flag` is false (flags are false at start, matching
//! trigger_eval); `normal_or_historical_nations` is true and
//! `is_random_new_world` false (a standard, non-RNW campaign). A modifier whose
//! trigger is unknown is skipped — the honest three-valued rule trigger_eval
//! uses. Every skip rule is identical across provinces, so the clustering stays
//! internally consistent even where the absolute percentages are approximate.
//!
//! Modeled conditions (the full vanilla chance vocabulary minus the above):
//! `has_terrain`, `has_climate`, `has_winter`, `area`, `region`, `continent`,
//! `province_id`, `culture_group`, `religion_group`, `native_size`,
//! `development_discounting_tribal`, combined with `AND`/`OR`/`NOT`.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::cache;
use crate::date::{parse_date, Date};
use crate::game_data;
use crate::loc::LocStore;
use crate::map_renderer;
use crate::paradox::{self, Block, Value};
use crate::vfs::Vfs;

/// The cluster assignment for every `trade_goods = unknown` province at a date.
pub struct SpawnClusters {
    /// Per-cluster human summary, e.g. `likely Grain 24%, Fur 18%, Fish 11%`
    /// (loc-resolved good names, top three by weight).
    pub summaries: Vec<String>,
    /// province id -> cluster index into `summaries`.
    pub index: HashMap<u32, usize>,
}

static CLUSTERS: cache::Store<(cache::SessionKey, Date), SpawnClusters> = cache::Store::new();

/// Drops this module's session caches. Called from `cache::invalidate_all`.
pub(crate) fn invalidate_caches() {
    CLUSTERS.clear();
}

/// The memoized cluster assignment for this session at `date`.
pub fn undiscovered_clusters(vfs: &Vfs, loc: &LocStore, date: Date) -> Arc<SpawnClusters> {
    CLUSTERS.get_or_build((cache::session_key(vfs), date), || build(vfs, loc, date))
}

// ---------------------------------------------------------------------------
// Chance model
// ---------------------------------------------------------------------------

/// One good's spawn-chance block: base factor + conditional factor modifiers.
pub(crate) struct GoodChance {
    pub key: String,
    pub base: f64,
    /// (factor, trigger conditions — the modifier block minus `factor`).
    pub modifiers: Vec<(f64, Block)>,
}

/// Parses every good's `chance` block across `common/tradegoods` (first
/// definition of a key wins). Goods without a `chance` block (e.g. `unknown`
/// itself) are excluded — they never spawn.
pub(crate) fn load_chances(vfs: &Vfs) -> Vec<GoodChance> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut out = Vec::new();
    for (name, path) in vfs.list_dir("common/tradegoods") {
        if !name.to_lowercase().ends_with(".txt") {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let root = paradox::parse(&String::from_utf8_lossy(&bytes));
        for (key, block) in root.key_blocks() {
            if !seen.insert(key.to_string()) {
                continue;
            }
            let Some(chance) = block.get_block("chance") else {
                continue;
            };
            out.push(parse_chance(key, chance));
        }
    }
    out
}

fn parse_chance(key: &str, chance: &Block) -> GoodChance {
    let base = chance
        .get_scalar("factor")
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0);
    let mut modifiers = Vec::new();
    for (k, v) in &chance.items {
        let (Some(k), Value::Block(b)) = (k, v) else {
            continue;
        };
        if k != "modifier" {
            continue;
        }
        let factor = b
            .get_scalar("factor")
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(1.0);
        let conditions = Block {
            items: b
                .items
                .iter()
                .filter(|(ck, _)| ck.as_deref() != Some("factor"))
                .cloned()
                .collect(),
        };
        modifiers.push((factor, conditions));
    }
    GoodChance {
        key: key.to_string(),
        base,
        modifiers,
    }
}

// ---------------------------------------------------------------------------
// Province facts + trigger evaluation (three-valued)
// ---------------------------------------------------------------------------

/// The static province-side facts the chance triggers read.
#[derive(Default, Clone)]
pub(crate) struct Facts {
    pub id: u32,
    pub terrain: Option<String>,
    /// arctic / arid / tropical; `None` = temperate (absence in climate.txt).
    pub climate: Option<String>,
    /// mild_winter / normal_winter / severe_winter (climate.txt key names —
    /// exactly the values `has_winter =` compares against).
    pub winter: Option<String>,
    pub area: Option<String>,
    pub region: Option<String>,
    pub continent: Option<String>,
    pub culture_group: Option<String>,
    pub religion_group: Option<String>,
    pub native_size: f64,
    pub development: f64,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum V3 {
    True,
    False,
    Unknown,
}

fn any(vs: impl Iterator<Item = V3>) -> V3 {
    let mut saw_unknown = false;
    for v in vs {
        match v {
            V3::True => return V3::True,
            V3::Unknown => saw_unknown = true,
            V3::False => {}
        }
    }
    if saw_unknown { V3::Unknown } else { V3::False }
}

fn all(vs: impl Iterator<Item = V3>) -> V3 {
    let mut saw_unknown = false;
    for v in vs {
        match v {
            V3::False => return V3::False,
            V3::Unknown => saw_unknown = true,
            V3::True => {}
        }
    }
    if saw_unknown { V3::Unknown } else { V3::True }
}

fn negate(v: V3) -> V3 {
    match v {
        V3::True => V3::False,
        V3::False => V3::True,
        V3::Unknown => V3::Unknown,
    }
}

/// Implicit AND over a block's statements.
fn eval_block(b: &Block, f: &Facts) -> V3 {
    all(b.items.iter().map(|(k, v)| match k {
        Some(k) => eval_cond(k, v, f),
        None => V3::Unknown,
    }))
}

fn eval_cond(key: &str, v: &Value, f: &Facts) -> V3 {
    // Combinators first. EU4 `NOT = { a b }` means "none of" (¬any).
    if let Value::Block(b) = v {
        return match key {
            "AND" => eval_block(b, f),
            "OR" => any(b.items.iter().map(|(k, v)| match k {
                Some(k) => eval_cond(k, v, f),
                None => V3::Unknown,
            })),
            "NOT" => negate(any(b.items.iter().map(|(k, v)| match k {
                Some(k) => eval_cond(k, v, f),
                None => V3::Unknown,
            }))),
            // Colonizer scopes / anything block-shaped we don't model.
            _ => V3::Unknown,
        };
    }
    let Value::Scalar(s) = v else {
        return V3::Unknown;
    };
    let s = s.as_str();
    let eq = |actual: Option<&str>| {
        if actual == Some(s) { V3::True } else { V3::False }
    };
    let num_ge = |actual: f64| match s.parse::<f64>() {
        Ok(n) => {
            if actual >= n { V3::True } else { V3::False }
        }
        Err(_) => V3::Unknown,
    };
    let fixed = |actual: bool| {
        if actual == (s == "yes") { V3::True } else { V3::False }
    };
    match key {
        "has_terrain" => eq(f.terrain.as_deref()),
        "has_climate" => eq(f.climate.as_deref()),
        "has_winter" => eq(f.winter.as_deref()),
        "area" => eq(f.area.as_deref()),
        "region" => eq(f.region.as_deref()),
        "continent" => eq(f.continent.as_deref()),
        "culture_group" => eq(f.culture_group.as_deref()),
        "religion_group" => eq(f.religion_group.as_deref()),
        "province_id" => match s.parse::<u32>() {
            Ok(n) => {
                if f.id == n { V3::True } else { V3::False }
            }
            Err(_) => V3::Unknown,
        },
        "native_size" => num_ge(f.native_size),
        "development_discounting_tribal" => num_ge(f.development),
        // Fixed campaign facts of the static view (see module doc).
        "normal_or_historical_nations" => fixed(true),
        "is_random_new_world" => fixed(false),
        // Country flags are false at start (mirrors trigger_eval).
        "has_country_flag" => fixed(false),
        // island / holy_order / anything colonizer-shaped: honestly unknown.
        _ => V3::Unknown,
    }
}

/// The spawn weight of every good (in `goods` order) for one province: base
/// factor × the factors of every modifier whose trigger is definitely true.
pub(crate) fn eval_weights(goods: &[GoodChance], f: &Facts) -> Vec<f64> {
    goods
        .iter()
        .map(|g| {
            let mut w = g.base;
            for (factor, cond) in &g.modifiers {
                if w == 0.0 {
                    break;
                }
                if eval_block(cond, f) == V3::True {
                    w *= factor;
                }
            }
            w
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Clustering
// ---------------------------------------------------------------------------

/// Connected components among `ids`, where two provinces join when they are
/// map-adjacent AND share a signature. Components are ordered by their lowest
/// province id (stable cluster numbering). Pure — unit-testable.
pub(crate) fn components(
    ids: &[u32],
    signature: &HashMap<u32, u64>,
    adjacency: &HashMap<u32, Vec<u32>>,
) -> Vec<Vec<u32>> {
    let mut sorted: Vec<u32> = ids.to_vec();
    sorted.sort_unstable();
    let id_set: HashSet<u32> = sorted.iter().copied().collect();
    let mut visited: HashSet<u32> = HashSet::new();
    let mut out = Vec::new();
    for &start in &sorted {
        if visited.contains(&start) {
            continue;
        }
        let sig = signature.get(&start);
        let mut comp = Vec::new();
        let mut stack = vec![start];
        visited.insert(start);
        while let Some(id) = stack.pop() {
            comp.push(id);
            if let Some(ns) = adjacency.get(&id) {
                for &n in ns {
                    if !visited.contains(&n)
                        && id_set.contains(&n)
                        && signature.get(&n) == sig
                    {
                        visited.insert(n);
                        stack.push(n);
                    }
                }
            }
        }
        comp.sort_unstable();
        out.push(comp);
    }
    out
}

// ---------------------------------------------------------------------------
// Build
// ---------------------------------------------------------------------------

fn build(vfs: &Vfs, loc: &LocStore, date: Date) -> SpawnClusters {
    let states = game_data::province_history_at(vfs, date);
    let unknown_ids: Vec<u32> = states
        .iter()
        .filter(|(_, st)| st.trade_goods.as_deref() == Some("unknown"))
        .map(|(id, _)| *id)
        .collect();
    if unknown_ids.is_empty() {
        return SpawnClusters {
            summaries: Vec::new(),
            index: HashMap::new(),
        };
    }

    let goods = load_chances(vfs);
    let facts = collect_facts(vfs, loc, date, &unknown_ids, &states);

    // Weight vector per province; signature = hash of the exact f64 bit
    // pattern sequence (identical facts ⇒ identical float ops ⇒ identical
    // bits, so no rounding is needed).
    let mut weights_by_id: HashMap<u32, Vec<f64>> = HashMap::new();
    let mut signature: HashMap<u32, u64> = HashMap::new();
    for &id in &unknown_ids {
        let f = facts.get(&id).cloned().unwrap_or(Facts {
            id,
            ..Facts::default()
        });
        let w = eval_weights(&goods, &f);
        let mut h: u64 = 0xcbf29ce484222325; // FNV-1a over the weight bits
        for x in &w {
            h ^= x.to_bits();
            h = h.wrapping_mul(0x100000001b3);
        }
        signature.insert(id, h);
        weights_by_id.insert(id, w);
    }

    // Adjacency-aware components; if the base map is unreadable, fall back to
    // one cluster per signature (still distribution-granular, just not
    // contiguity-split).
    let comps = match map_renderer::province_adjacency(vfs) {
        Ok(adj) => components(&unknown_ids, &signature, &adj),
        Err(_) => {
            let mut by_sig: HashMap<u64, Vec<u32>> = HashMap::new();
            for &id in &unknown_ids {
                by_sig.entry(signature[&id]).or_default().push(id);
            }
            let mut comps: Vec<Vec<u32>> = by_sig
                .into_values()
                .map(|mut v| {
                    v.sort_unstable();
                    v
                })
                .collect();
            comps.sort_by_key(|c| c[0]);
            comps
        }
    };

    let mut summaries = Vec::with_capacity(comps.len());
    let mut index = HashMap::new();
    for (ci, comp) in comps.iter().enumerate() {
        let rep = comp[0];
        summaries.push(summarize(&goods, weights_by_id.get(&rep), loc));
        for &id in comp {
            index.insert(id, ci);
        }
    }
    SpawnClusters { summaries, index }
}

/// `likely Grain 24%, Fur 18%, Fish 11%` — top three goods by weight. Zero
/// total weight (nothing can spawn under the static view) reads honestly.
fn summarize(goods: &[GoodChance], weights: Option<&Vec<f64>>, loc: &LocStore) -> String {
    let Some(weights) = weights else {
        return "no spawn data".to_string();
    };
    let total: f64 = weights.iter().sum();
    if total <= 0.0 {
        return "no spawn data".to_string();
    }
    let mut ranked: Vec<(usize, f64)> = weights
        .iter()
        .enumerate()
        .filter(|(_, w)| **w > 0.0)
        .map(|(i, w)| (i, *w))
        .collect();
    ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let parts: Vec<String> = ranked
        .iter()
        .take(3)
        .map(|(i, w)| {
            let pct = (w / total * 100.0).round() as u32;
            format!("{} {}%", loc.resolve(&goods[*i].key), pct.max(1))
        })
        .collect();
    format!("likely {}", parts.join(", "))
}

fn collect_facts(
    vfs: &Vfs,
    loc: &LocStore,
    date: Date,
    ids: &[u32],
    states: &HashMap<u32, game_data::ProvinceState>,
) -> HashMap<u32, Facts> {
    let wanted: HashSet<u32> = ids.iter().copied().collect();

    // Terrain (override else terrain.bmp majority). Best-effort: a fixture
    // install without a terrain map just yields no terrain facts.
    let terrain: HashMap<u32, String> = map_renderer::effective_terrain(vfs)
        .map(|eff| {
            let mut m: HashMap<u32, String> = eff
                .auto_by_province
                .iter()
                .map(|(id, t)| (*id, t.clone()))
                .collect();
            for (id, (t, _)) in &eff.by_province {
                m.insert(*id, t.clone());
            }
            m
        })
        .unwrap_or_default();

    // Climate zones + winter bands share map/climate.txt; winter keys are the
    // literal `has_winter` comparison values.
    let mut climate: HashMap<u32, String> = HashMap::new();
    let mut winter: HashMap<u32, String> = HashMap::new();
    if let Ok(bytes) = vfs.read("map/climate.txt") {
        let block = paradox::parse(&String::from_utf8_lossy(&bytes));
        for (key, list) in block.key_blocks() {
            let target = match key {
                "arctic" | "arid" | "tropical" => &mut climate,
                "mild_winter" | "normal_winter" | "severe_winter" => &mut winter,
                _ => continue,
            };
            for id in list.bare_ids() {
                target.insert(id, key.to_string());
            }
        }
    }

    // Area/region joins via the geography network; continents via continent.txt.
    let geo = crate::geography::load_network(vfs, loc);
    let mut area: HashMap<u32, String> = HashMap::new();
    let mut region: HashMap<u32, String> = HashMap::new();
    for a in &geo.areas {
        for &p in &a.provinces {
            area.insert(p, a.key.clone());
            if let Some(r) = &a.region {
                region.insert(p, r.clone());
            }
        }
    }
    let mut continent: HashMap<u32, String> = HashMap::new();
    for (name, ids) in crate::trigger_eval::load_continents(vfs) {
        for id in ids {
            continent.entry(id).or_insert_with(|| name.clone());
        }
    }

    let culture_group: HashMap<String, String> = game_data::culture_list(vfs, loc)
        .into_iter()
        .map(|e| (e.key, e.group))
        .collect();
    let religion_group: HashMap<String, String> = game_data::religion_list(vfs, loc)
        .into_iter()
        .map(|e| (e.key, e.group))
        .collect();

    // native_size is not part of ProvinceState — fold it from the cached ASTs
    // (top level, then dated blocks ≤ date, last write wins).
    let mut native_size: HashMap<u32, f64> = HashMap::new();
    for ast in game_data::province_asts(vfs).iter() {
        if !wanted.contains(&ast.id) {
            continue;
        }
        let Some(block) = &ast.block else { continue };
        let mut ns = block
            .get_scalar("native_size")
            .and_then(|s| s.parse::<f64>().ok());
        for (k, v) in &block.items {
            let (Some(k), Value::Block(b)) = (k, v) else {
                continue;
            };
            let Some(d) = parse_date(k) else { continue };
            if d > date {
                continue;
            }
            if let Some(s) = b.get_scalar("native_size") {
                ns = s.parse::<f64>().ok();
            }
        }
        if let Some(ns) = ns {
            native_size.insert(ast.id, ns);
        }
    }

    let mut out = HashMap::new();
    for &id in ids {
        let st = states.get(&id);
        out.insert(
            id,
            Facts {
                id,
                terrain: terrain.get(&id).cloned(),
                climate: climate.get(&id).cloned(),
                winter: winter.get(&id).cloned(),
                area: area.get(&id).cloned(),
                region: region.get(&id).cloned(),
                continent: continent.get(&id).cloned(),
                culture_group: st
                    .and_then(|s| s.culture.as_ref())
                    .and_then(|c| culture_group.get(c).cloned()),
                religion_group: st
                    .and_then(|s| s.religion.as_ref())
                    .and_then(|r| religion_group.get(r).cloned()),
                native_size: native_size.get(&id).copied().unwrap_or(0.0),
                development: st.and_then(|s| s.development).unwrap_or(0.0) as f64,
            },
        );
    }
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    const INSTALL: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Europa Universalis IV";

    fn chance_fixture() -> Vec<GoodChance> {
        let src = r#"
grain = {
    chance = {
        factor = 20
        modifier = { factor = 0 has_climate = arctic }
        modifier = { factor = 2 region = test_region }
        modifier = { factor = 0.5 NOT = { has_terrain = farmlands } }
        modifier = { factor = 3 FROM = { has_country_flag = x } }
    }
}
fur = {
    chance = {
        factor = 10
        modifier = { factor = 4 OR = { has_climate = arctic has_winter = severe_winter } }
    }
}
"#;
        let root = paradox::parse(src);
        root.key_blocks()
            .into_iter()
            .map(|(k, b)| parse_chance(k, b.get_block("chance").unwrap()))
            .collect()
    }

    #[test]
    fn weights_fold_matching_modifiers_and_skip_unknown() {
        let goods = chance_fixture();
        // Arctic province, not farmlands: grain 20×0 = 0; fur 10×4 = 40.
        let arctic = Facts {
            climate: Some("arctic".into()),
            terrain: Some("tundra".into()),
            ..Facts::default()
        };
        assert_eq!(eval_weights(&goods, &arctic), vec![0.0, 40.0]);
        // Temperate farmlands in test_region: grain 20×2 (region) — the NOT
        // farmlands modifier does NOT apply, and the FROM modifier is unknown
        // (skipped). fur stays base 10.
        let farm = Facts {
            terrain: Some("farmlands".into()),
            region: Some("test_region".into()),
            ..Facts::default()
        };
        assert_eq!(eval_weights(&goods, &farm), vec![40.0, 10.0]);
        // Plain temperate elsewhere: grain 20×0.5 (NOT farmlands true) = 10.
        let plain = Facts {
            terrain: Some("grasslands".into()),
            ..Facts::default()
        };
        assert_eq!(eval_weights(&goods, &plain), vec![10.0, 10.0]);
    }

    #[test]
    fn three_valued_combinators() {
        let f = Facts {
            climate: Some("tropical".into()),
            ..Facts::default()
        };
        let t = |src: &str| eval_block(&paradox::parse(src), &f);
        assert_eq!(t("has_climate = tropical"), V3::True);
        assert_eq!(t("NOT = { has_climate = tropical }"), V3::False);
        // NOT over a list = none-of.
        assert_eq!(t("NOT = { has_climate = arctic has_climate = arid }"), V3::True);
        // Unknown condition propagates through NOT (no false confidence)…
        assert_eq!(t("NOT = { island = yes }"), V3::Unknown);
        // …but a definite true inside OR decides despite an unknown sibling.
        assert_eq!(t("OR = { island = yes has_climate = tropical }"), V3::True);
        // Fixed campaign facts.
        assert_eq!(t("normal_or_historical_nations = yes"), V3::True);
        assert_eq!(t("is_random_new_world = yes"), V3::False);
    }

    #[test]
    fn components_split_by_signature_and_contiguity() {
        // 1—2—3 adjacent chain + isolated 9; 1,2 share a signature, 3 differs,
        // 9 shares with 1,2 but is not connected → three clusters + one.
        let ids = vec![1, 2, 3, 9];
        let mut sig = HashMap::new();
        sig.insert(1u32, 100u64);
        sig.insert(2, 100);
        sig.insert(3, 200);
        sig.insert(9, 100);
        let mut adj: HashMap<u32, Vec<u32>> = HashMap::new();
        adj.insert(1, vec![2]);
        adj.insert(2, vec![1, 3]);
        adj.insert(3, vec![2]);
        let comps = components(&ids, &sig, &adj);
        assert_eq!(comps, vec![vec![1, 2], vec![3], vec![9]]);
    }

    #[test]
    fn vanilla_clusters_are_plural_and_cover_all_unknown() {
        if !Path::new(INSTALL).join("map/provinces.bmp").is_file() {
            return;
        }
        let vfs = Vfs::new(INSTALL, None).unwrap();
        let loc = crate::loc::store(&vfs, INSTALL, None);
        let at = crate::date::DEFAULT_START;
        let clusters = undiscovered_clusters(&vfs, &loc, at);

        let states = game_data::province_history_at(&vfs, at);
        let unknown: Vec<u32> = states
            .iter()
            .filter(|(_, st)| st.trade_goods.as_deref() == Some("unknown"))
            .map(|(id, _)| *id)
            .collect();
        assert!(!unknown.is_empty(), "vanilla 1444 has uncolonized land");
        for id in &unknown {
            assert!(clusters.index.contains_key(id), "province {id} unclustered");
        }
        // The whole point: more than one cluster, and multi-province clusters
        // exist (adjacency actually groups neighbors).
        let n = clusters.summaries.len();
        assert!(n > 1, "expected >1 cluster, got {n}");
        assert!(n < unknown.len(), "every province its own cluster ({n})");
        let distinct: HashSet<&String> = clusters.summaries.iter().collect();
        assert!(distinct.len() > 1, "all clusters share one distribution");
        println!(
            "vanilla: {} unknown provinces -> {} clusters ({} distinct distributions)",
            unknown.len(),
            n,
            distinct.len()
        );
    }
}
