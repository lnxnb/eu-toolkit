//! Sprint 14.3 — trigger evaluator ("who can…" lists).
//!
//! Evaluates a typed trigger tree ([`crate::script_tree`]) against the derived
//! world state at the selected date (Sprint 12) for every existing country. The
//! result is three-valued — **yes / no / unknown** — and *unknown propagates
//! honestly*: a trigger that touches an unsupported condition never returns a
//! confident yes/no. Every condition key that couldn't be evaluated is collected
//! so the UI can render the "approximate — N conditions not evaluated" badge.
//!
//! ## Supported subset (spec 14.3)
//! `tag`/`exists`/`religion`/`religion_group`/`culture`(+group)/`primary_culture`/
//! `government`/`technology_group`/`owns`/`owns_core_province`/`capital`/
//! `num_of_cities`/`total_development`/`is_year`/`is_subject`(+`is_subject_of`)/
//! `overlord_of`/`is_at_war`/`war_with`/`dynasty`/`has_*_flag` (false at start),
//! combined with `AND`/`OR`/`NOT`/`NAND`/`NOR`/`hidden_trigger`, country-tag and
//! `ROOT`/`THIS` scope changes, and cheap `any_/all_owned_province` quantifiers
//! (province `religion`/`culture`/`trade_goods` only). Everything else → unknown.
//!
//! ## Snapshot
//! Built once per evaluation from the same date-parameterized loaders the rest of
//! the app uses: [`game_data::province_history_at`] (owner/religion/culture/goods/
//! dev), [`game_data::province_political_at`] (cores), country history files
//! (religion/primary_culture/government/tech_group/capital + latest ruler
//! dynasty ≤ date), culture→group / religion→group indexes, active diplomacy at
//! date (subjects/overlords) and active wars at date (war participants per side).

use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::date::{self, Date};
use crate::game_data;
use crate::paradox::{self, Value};
use crate::script_tree::{self, TreeNode};
use crate::vfs::Vfs;

// ---------------------------------------------------------------------------
// Three-valued logic
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Yes,
    No,
    Unknown,
}

impl Verdict {
    fn negate(self) -> Verdict {
        match self {
            Verdict::Yes => Verdict::No,
            Verdict::No => Verdict::Yes,
            Verdict::Unknown => Verdict::Unknown,
        }
    }
    fn as_str(self) -> &'static str {
        match self {
            Verdict::Yes => "yes",
            Verdict::No => "no",
            Verdict::Unknown => "unknown",
        }
    }
}

fn bv(b: bool) -> Verdict {
    if b {
        Verdict::Yes
    } else {
        Verdict::No
    }
}

/// AND: any No → No; else any Unknown → Unknown; else Yes.
fn all_of(vs: &[Verdict]) -> Verdict {
    let mut unknown = false;
    for &v in vs {
        match v {
            Verdict::No => return Verdict::No,
            Verdict::Unknown => unknown = true,
            Verdict::Yes => {}
        }
    }
    if unknown {
        Verdict::Unknown
    } else {
        Verdict::Yes
    }
}

/// OR: any Yes → Yes; else any Unknown → Unknown; else No.
fn any_of(vs: &[Verdict]) -> Verdict {
    let mut unknown = false;
    for &v in vs {
        match v {
            Verdict::Yes => return Verdict::Yes,
            Verdict::Unknown => unknown = true,
            Verdict::No => {}
        }
    }
    if unknown {
        Verdict::Unknown
    } else {
        Verdict::No
    }
}

// ---------------------------------------------------------------------------
// World snapshot
// ---------------------------------------------------------------------------

/// Derived per-country state at the evaluation date.
#[derive(Debug, Default, Clone)]
pub struct CountryState {
    pub tag: String,
    pub owned: HashSet<u32>,
    pub cores: HashSet<u32>,
    pub religion: Option<String>,
    pub culture: Option<String>,
    pub government: Option<String>,
    pub tech_group: Option<String>,
    pub capital: Option<u32>,
    pub dynasty: Option<String>,
    pub overlord: Option<String>,
    pub num_cities: usize,
    pub total_dev: f32,
    /// Capital province is in the HRE (`hre = yes` on the capital's history at the
    /// evaluation date). Answers the country-scope `is_part_of_hre` trigger.
    pub is_part_of_hre: bool,
    /// This country holds the imperial throne at the evaluation date (latest
    /// `emperor = TAG` ≤ date in `history/diplomacy/hre.txt`). Answers `is_emperor`.
    pub is_emperor: bool,
}

impl CountryState {
    /// A country "exists" iff it owns at least one province at the date.
    pub fn exists(&self) -> bool {
        !self.owned.is_empty()
    }
}

/// The whole derived world at the evaluation date.
#[derive(Debug, Default)]
pub struct WorldSnapshot {
    pub countries: HashMap<String, CountryState>,
    pub culture_group: HashMap<String, String>,
    pub religion_group: HashMap<String, String>,
    pub prov_religion: HashMap<u32, String>,
    pub prov_culture: HashMap<u32, String>,
    pub prov_trade_goods: HashMap<u32, String>,
    /// Owner tag per province at the date (for the province-scope `owned_by`/
    /// `owner = TAG` conditions reached through `capital_scope`).
    pub prov_owner: HashMap<u32, String>,
    /// Continent name → its province ids (`map/continent.txt`, VFS-merged). A
    /// province can appear under several keys (a real continent plus `new_world`/
    /// `island_check_provinces`), so membership is stored as name→set and queried
    /// by continent, not the reverse.
    pub continents: HashMap<String, HashSet<u32>>,
    /// Province id → its area / region / superregion key (derived from the
    /// [`crate::geography`] area→region→superregion network — not re-parsed).
    pub prov_area: HashMap<u32, String>,
    pub prov_region: HashMap<u32, String>,
    pub prov_superregion: HashMap<u32, String>,
    /// subject tag → overlord tag (first active dependency).
    pub subject_overlord: HashMap<String, String>,
    /// (attackers, defenders) per active war.
    pub war_sides: Vec<(HashSet<String>, HashSet<String>)>,
    pub year: u32,
    /// DLC display names present in the BASE install (`dlc/*/**.dlc` descriptors).
    ///
    /// **Honesty note:** this is "as installed, all DLC enabled" — a player can
    /// disable owned DLC in the launcher, which we cannot observe. We treat every
    /// installed DLC as active so `has_dlc` yields a definite verdict rather than
    /// poisoning the whole trigger to Unknown. Base-install only: DLC ownership is
    /// never resolved through mod paths.
    pub installed_dlc: HashSet<String>,
    /// Scripted-trigger name → its child condition nodes (VFS-merged, so a mod's
    /// scripted triggers shadow the base). Lets `is_iroquois = yes`-style calls
    /// inline their body and reach a definite verdict instead of Unknown.
    pub scripted_triggers: HashMap<String, Vec<TreeNode>>,
}

impl WorldSnapshot {
    fn is_at_war(&self, tag: &str) -> bool {
        self.war_sides
            .iter()
            .any(|(a, d)| a.contains(tag) || d.contains(tag))
    }
    fn war_with(&self, tag: &str, other: &str) -> bool {
        self.war_sides.iter().any(|(a, d)| {
            (a.contains(tag) && d.contains(other)) || (d.contains(tag) && a.contains(other))
        })
    }
}

/// The DLC display names installed in `base_dir` (base install only).
///
/// EU4 ships each DLC as `dlc/dlc0XX_<slug>/dlc0XX.dlc`, a Clausewitz descriptor
/// whose `name = "…"` is the string `has_dlc` compares against (e.g.
/// `name = "Conquest of Paradise"`). Some `dlc/` subfolders are empty (the DLC's
/// content isn't downloaded) and carry no `.dlc` file — those are simply absent.
pub fn installed_dlc(base_dir: &Path) -> HashSet<String> {
    let mut set = HashSet::new();
    let Ok(subdirs) = std::fs::read_dir(base_dir.join("dlc")) else {
        return set;
    };
    for sub in subdirs.flatten() {
        let Ok(files) = std::fs::read_dir(sub.path()) else {
            continue;
        };
        for f in files.flatten() {
            let p = f.path();
            if p.extension().and_then(|e| e.to_str()) != Some("dlc") {
                continue;
            }
            let Ok(bytes) = std::fs::read(&p) else {
                continue;
            };
            let block = paradox::parse(&String::from_utf8_lossy(&bytes));
            if let Some(name) = block.get_scalar("name") {
                set.insert(name.to_string());
            }
        }
    }
    set
}

/// Loads every scripted trigger (`common/scripted_triggers/*.txt`, VFS-merged)
/// into `name → child condition nodes`. First definition wins on a name clash.
fn load_scripted_triggers(vfs: &Vfs) -> HashMap<String, Vec<TreeNode>> {
    let mut out: HashMap<String, Vec<TreeNode>> = HashMap::new();
    for (name, path) in vfs.list_dir("common/scripted_triggers") {
        if !name.to_lowercase().ends_with(".txt") {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let block = paradox::parse(&String::from_utf8_lossy(&bytes));
        for (key, _) in block.key_blocks() {
            if out.contains_key(key) {
                continue;
            }
            let nodes = script_tree::build_nodes(&bytes, &[key.to_string()]);
            out.insert(key.to_string(), nodes);
        }
    }
    out
}

/// Continent name → its province ids from `map/continent.txt` (VFS-merged, so a
/// mod's continent file shadows the base). Same `key = { id id … }` shape as
/// `climate.txt`; keys include the six real continents plus `new_world` /
/// `island_check_provinces`, all preserved as written.
fn load_continents(vfs: &Vfs) -> HashMap<String, HashSet<u32>> {
    let mut out: HashMap<String, HashSet<u32>> = HashMap::new();
    let Ok(bytes) = vfs.read("map/continent.txt") else {
        return out;
    };
    let block = paradox::parse(&String::from_utf8_lossy(&bytes));
    for (name, b) in block.key_blocks() {
        out.insert(name.to_string(), b.bare_ids().into_iter().collect());
    }
    out
}

/// The imperial tag at `date`: the latest `emperor = TAG` on or before `date` in
/// `history/diplomacy/hre.txt`. `emperor = ---` (dissolution) yields `None`.
fn current_emperor(vfs: &Vfs, date: Date) -> Option<String> {
    let bytes = vfs.read("history/diplomacy/hre.txt").ok()?;
    let block = paradox::parse(&String::from_utf8_lossy(&bytes));
    let mut best: Option<(Date, String)> = None;
    for (k, v) in &block.items {
        let (Some(k), Value::Block(b)) = (k, v) else {
            continue;
        };
        let Some(d) = date::parse_date(k) else { continue };
        if d > date {
            continue;
        }
        if let Some(tag) = b.get_scalar("emperor") {
            if best.as_ref().map_or(true, |(bd, _)| d >= *bd) {
                best = Some((d, tag.to_string()));
            }
        }
    }
    best.map(|(_, t)| t).filter(|t| t != "---")
}

/// Extracts a country's modeled fields from its history file bytes, with the
/// ruling dynasty taken from the latest monarch defined on or before `date`.
fn extract_country(bytes: &[u8], date: Date) -> CountryState {
    let block = paradox::parse(&String::from_utf8_lossy(bytes));
    let mut best_dynasty: Option<(Date, String)> = None;
    // A top-level `monarch` counts as beginning-of-time.
    if let Some(dyn_) = block
        .get_block("monarch")
        .and_then(|m| m.get_scalar("dynasty"))
    {
        best_dynasty = Some(((0, 0, 0), dyn_.to_string()));
    }
    for (k, v) in &block.items {
        let (Some(k), Value::Block(b)) = (k, v) else {
            continue;
        };
        let Some(d) = date::parse_date(k) else {
            continue;
        };
        if d > date {
            continue;
        }
        if let Some(dyn_) = b.get_block("monarch").and_then(|m| m.get_scalar("dynasty")) {
            if best_dynasty.as_ref().map_or(true, |(bd, _)| d >= *bd) {
                best_dynasty = Some((d, dyn_.to_string()));
            }
        }
    }
    CountryState {
        religion: block.get_scalar("religion").map(str::to_string),
        culture: block.get_scalar("primary_culture").map(str::to_string),
        government: block.get_scalar("government").map(str::to_string),
        tech_group: block.get_scalar("technology_group").map(str::to_string),
        capital: block.get_scalar("capital").and_then(|s| s.parse().ok()),
        dynasty: best_dynasty.map(|(_, s)| s),
        ..Default::default()
    }
}

/// Builds the world snapshot at `date`.
pub fn build_snapshot(vfs: &Vfs, loc: &crate::loc::LocStore, date: Date) -> WorldSnapshot {
    let states = game_data::province_history_at(vfs, date);
    let water = game_data::water_ids(vfs);

    let mut owned: HashMap<String, HashSet<u32>> = HashMap::new();
    let mut dev: HashMap<u32, f32> = HashMap::new();
    let mut prov_religion = HashMap::new();
    let mut prov_culture = HashMap::new();
    let mut prov_trade_goods = HashMap::new();
    let mut prov_owner = HashMap::new();
    for (id, st) in &states {
        if let Some(o) = &st.owner {
            owned.entry(o.clone()).or_default().insert(*id);
            prov_owner.insert(*id, o.clone());
        }
        if let Some(d) = st.development {
            dev.insert(*id, d);
        }
        if let Some(r) = &st.religion {
            prov_religion.insert(*id, r.clone());
        }
        if let Some(c) = &st.culture {
            prov_culture.insert(*id, c.clone());
        }
        if let Some(g) = &st.trade_goods {
            prov_trade_goods.insert(*id, g.clone());
        }
    }

    // Cores per tag (add_core folding at date).
    let mut cores: HashMap<String, HashSet<u32>> = HashMap::new();
    for pp in game_data::province_political_at(vfs, date) {
        for c in &pp.cores {
            cores.entry(c.clone()).or_default().insert(pp.id);
        }
    }

    let culture_group = game_data::culture_list(vfs, loc)
        .into_iter()
        .map(|e| (e.key, e.group))
        .collect();
    let religion_group = game_data::religion_list(vfs, loc)
        .into_iter()
        .map(|e| (e.key, e.group))
        .collect();

    // Active dependencies → subject→overlord.
    let mut subject_overlord = HashMap::new();
    for r in crate::diplomacy::all_relations_at(vfs, date) {
        if r.active_at_start && r.relation_type == "dependency" {
            if let (Some(f), Some(s)) = (r.first, r.second) {
                subject_overlord.entry(s).or_insert(f);
            }
        }
    }

    // Active wars → per-side participant sets (participants active at the date).
    let mut war_sides = Vec::new();
    for w in crate::wars::all_wars_at(vfs, date) {
        if !w.active_at_date {
            continue;
        }
        let (mut att, mut def) = (HashSet::new(), HashSet::new());
        for p in &w.participants {
            let joined = p
                .join_date
                .as_deref()
                .and_then(date::parse_date)
                .map_or(false, |j| j <= date);
            let left = p
                .leave_date
                .as_deref()
                .and_then(date::parse_date)
                .map_or(false, |l| l <= date);
            if joined && !left {
                match p.side.as_str() {
                    "attacker" => {
                        att.insert(p.tag.clone());
                    }
                    "defender" => {
                        def.insert(p.tag.clone());
                    }
                    _ => {}
                }
            }
        }
        war_sides.push((att, def));
    }

    // Province geography (area/region/superregion) from the area→region→
    // superregion network — reused, not re-parsed.
    let geo = crate::geography::load_network(vfs, loc);
    let mut prov_area = HashMap::new();
    let mut prov_region = HashMap::new();
    let mut region_super: HashMap<String, String> = HashMap::new();
    for r in &geo.regions {
        if let Some(s) = &r.superregion {
            region_super.insert(r.key.clone(), s.clone());
        }
    }
    let mut prov_superregion = HashMap::new();
    for a in &geo.areas {
        for &p in &a.provinces {
            prov_area.insert(p, a.key.clone());
            if let Some(r) = &a.region {
                prov_region.insert(p, r.clone());
                if let Some(s) = region_super.get(r) {
                    prov_superregion.insert(p, s.clone());
                }
            }
        }
    }

    let continents = load_continents(vfs);
    let emperor = current_emperor(vfs, date);

    // Per-country states over every tag.
    let mut countries: HashMap<String, CountryState> = HashMap::new();
    for brief in game_data::country_list(vfs, loc) {
        let tag = brief.tag;
        let owned_set = owned.remove(&tag).unwrap_or_default();
        let cores_set = cores.remove(&tag).unwrap_or_default();
        let num_cities = owned_set.iter().filter(|id| !water.contains(id)).count();
        let total_dev: f32 = owned_set.iter().filter_map(|id| dev.get(id)).sum();
        let mut cs = game_data::country_history_file(vfs, &tag)
            .map(|(_, bytes)| extract_country(&bytes, date))
            .unwrap_or_default();
        // A country is in the HRE iff its capital province is (history `hre`).
        cs.is_part_of_hre = cs
            .capital
            .and_then(|c| states.get(&c))
            .map_or(false, |s| s.hre);
        cs.is_emperor = emperor.as_deref() == Some(tag.as_str());
        cs.tag = tag.clone();
        cs.owned = owned_set;
        cs.cores = cores_set;
        cs.num_cities = num_cities;
        cs.total_dev = total_dev;
        cs.overlord = subject_overlord.get(&tag).cloned();
        countries.insert(tag, cs);
    }

    WorldSnapshot {
        countries,
        culture_group,
        religion_group,
        prov_religion,
        prov_culture,
        prov_trade_goods,
        prov_owner,
        continents,
        prov_area,
        prov_region,
        prov_superregion,
        subject_overlord,
        war_sides,
        year: date.0,
        installed_dlc: installed_dlc(vfs.base_dir()),
        scripted_triggers: load_scripted_triggers(vfs),
    }
}

// ---------------------------------------------------------------------------
// Evaluation
// ---------------------------------------------------------------------------

/// Guards scripted-trigger inlining against cyclic/deeply-nested definitions.
const MAX_DEPTH: u32 = 24;

fn eval_nodes(
    nodes: &[TreeNode],
    cs: &CountryState,
    snap: &WorldSnapshot,
    un: &mut HashSet<String>,
    depth: u32,
) -> Verdict {
    let vs: Vec<Verdict> = nodes
        .iter()
        .map(|n| eval_node(n, cs, snap, un, depth))
        .collect();
    all_of(&vs)
}

fn eval_node(
    node: &TreeNode,
    cs: &CountryState,
    snap: &WorldSnapshot,
    un: &mut HashSet<String>,
    depth: u32,
) -> Verdict {
    if node.node_type == "group" {
        eval_group(node, cs, snap, un, depth)
    } else {
        eval_leaf(node, cs, snap, un, depth)
    }
}

fn eval_group(
    node: &TreeNode,
    cs: &CountryState,
    snap: &WorldSnapshot,
    un: &mut HashSet<String>,
    depth: u32,
) -> Verdict {
    match node.group_kind.as_str() {
        "and" | "hidden" | "limit" => eval_nodes(&node.children, cs, snap, un, depth),
        "or" => {
            let vs: Vec<Verdict> = node
                .children
                .iter()
                .map(|n| eval_node(n, cs, snap, un, depth))
                .collect();
            any_of(&vs)
        }
        // NOT / NOR: true iff none of the children are true → !(OR).
        "not" | "nor" => {
            let vs: Vec<Verdict> = node
                .children
                .iter()
                .map(|n| eval_node(n, cs, snap, un, depth))
                .collect();
            any_of(&vs).negate()
        }
        "nand" => eval_nodes(&node.children, cs, snap, un, depth).negate(),
        "tooltip" => {
            // Skip the `tooltip = X` label child; AND the real conditions.
            let vs: Vec<Verdict> = node
                .children
                .iter()
                .filter(|c| c.key.as_deref() != Some("tooltip"))
                .map(|n| eval_node(n, cs, snap, un, depth))
                .collect();
            all_of(&vs)
        }
        "quantifier" => eval_quantifier(node, cs, snap, un),
        "scope" => eval_scope(node, cs, snap, un, depth),
        other => {
            // control (if/else) / calc_true_if / anonymous → honestly unknown.
            un.insert(node.key.clone().unwrap_or_else(|| format!("<{other}>")));
            Verdict::Unknown
        }
    }
}

fn is_tag(key: &str) -> bool {
    key.len() == 3 && key.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
}

fn eval_scope(
    node: &TreeNode,
    cs: &CountryState,
    snap: &WorldSnapshot,
    un: &mut HashSet<String>,
    depth: u32,
) -> Verdict {
    let key = node.key.as_deref().unwrap_or("");
    match key.to_ascii_uppercase().as_str() {
        "ROOT" | "THIS" => eval_nodes(&node.children, cs, snap, un, depth),
        // `capital_scope = { … }` switches to the country's capital PROVINCE and
        // evaluates province-geography conditions there.
        "CAPITAL_SCOPE" => match cs.capital {
            Some(cap) => eval_prov_nodes(&node.children, cap, cs, snap, un, depth),
            None => {
                un.insert("capital_scope".to_string());
                Verdict::Unknown
            }
        },
        _ if is_tag(key) => match snap.countries.get(key) {
            Some(other) => eval_nodes(&node.children, other, snap, un, depth),
            None => {
                un.insert(key.to_string());
                Verdict::Unknown
            }
        },
        _ => {
            // FROM / PREV / OWNER (as a country-scope changer) / province-id scope.
            un.insert(key.to_string());
            Verdict::Unknown
        }
    }
}

// ---------------------------------------------------------------------------
// Province scope (reached via `capital_scope`)
// ---------------------------------------------------------------------------

/// AND of every child, evaluated in province scope against `prov`.
fn eval_prov_nodes(
    nodes: &[TreeNode],
    prov: u32,
    cs: &CountryState,
    snap: &WorldSnapshot,
    un: &mut HashSet<String>,
    depth: u32,
) -> Verdict {
    let vs: Vec<Verdict> = nodes
        .iter()
        .map(|n| eval_prov_node(n, prov, cs, snap, un, depth))
        .collect();
    all_of(&vs)
}

fn eval_prov_node(
    node: &TreeNode,
    prov: u32,
    cs: &CountryState,
    snap: &WorldSnapshot,
    un: &mut HashSet<String>,
    depth: u32,
) -> Verdict {
    if node.node_type == "group" {
        eval_prov_group(node, prov, cs, snap, un, depth)
    } else {
        eval_prov_leaf(node, prov, cs, snap, un)
    }
}

fn eval_prov_group(
    node: &TreeNode,
    prov: u32,
    cs: &CountryState,
    snap: &WorldSnapshot,
    un: &mut HashSet<String>,
    depth: u32,
) -> Verdict {
    match node.group_kind.as_str() {
        "and" | "hidden" | "limit" => eval_prov_nodes(&node.children, prov, cs, snap, un, depth),
        "or" => {
            let vs: Vec<Verdict> = node
                .children
                .iter()
                .map(|n| eval_prov_node(n, prov, cs, snap, un, depth))
                .collect();
            any_of(&vs)
        }
        "not" | "nor" => {
            let vs: Vec<Verdict> = node
                .children
                .iter()
                .map(|n| eval_prov_node(n, prov, cs, snap, un, depth))
                .collect();
            any_of(&vs).negate()
        }
        "nand" => eval_prov_nodes(&node.children, prov, cs, snap, un, depth).negate(),
        "tooltip" => {
            let vs: Vec<Verdict> = node
                .children
                .iter()
                .filter(|c| c.key.as_deref() != Some("tooltip"))
                .map(|n| eval_prov_node(n, prov, cs, snap, un, depth))
                .collect();
            all_of(&vs)
        }
        "scope" => eval_prov_scope(node, prov, cs, snap, un, depth),
        other => {
            un.insert(node.key.clone().unwrap_or_else(|| format!("<{other}>")));
            Verdict::Unknown
        }
    }
}

/// A scope change written inside province scope. `ROOT`/tag jump back to a
/// country; `OWNER`/`CONTROLLER` resolve the province's owning country.
fn eval_prov_scope(
    node: &TreeNode,
    prov: u32,
    cs: &CountryState,
    snap: &WorldSnapshot,
    un: &mut HashSet<String>,
    depth: u32,
) -> Verdict {
    let key = node.key.as_deref().unwrap_or("");
    match key.to_ascii_uppercase().as_str() {
        "ROOT" => eval_nodes(&node.children, cs, snap, un, depth),
        "THIS" => eval_prov_nodes(&node.children, prov, cs, snap, un, depth),
        "OWNER" | "CONTROLLER" => match snap
            .prov_owner
            .get(&prov)
            .and_then(|t| snap.countries.get(t))
        {
            Some(owner) => eval_nodes(&node.children, owner, snap, un, depth),
            None => {
                un.insert(key.to_string());
                Verdict::Unknown
            }
        },
        _ if is_tag(key) => match snap.countries.get(key) {
            Some(other) => eval_nodes(&node.children, other, snap, un, depth),
            None => {
                un.insert(key.to_string());
                Verdict::Unknown
            }
        },
        _ => {
            un.insert(key.to_string());
            Verdict::Unknown
        }
    }
}

/// A province-scope condition leaf. Only what the snapshot answers cheaply is
/// modeled; anything else is honestly Unknown (recorded for the badge).
fn eval_prov_leaf(
    node: &TreeNode,
    prov: u32,
    cs: &CountryState,
    snap: &WorldSnapshot,
    un: &mut HashSet<String>,
) -> Verdict {
    let Some(v) = &node.value else {
        return Verdict::Unknown;
    };
    let key = node.key.as_deref().unwrap_or("");
    let text = v.text.trim();
    let no_sense = text.eq_ignore_ascii_case("no");
    let flip = |verdict: Verdict| if no_sense { verdict.negate() } else { verdict };

    match key.to_ascii_lowercase().as_str() {
        "continent" => bv(snap
            .continents
            .get(text)
            .map_or(false, |set| set.contains(&prov))),
        "region" => bv(snap.prov_region.get(&prov).map(String::as_str) == Some(text)),
        "superregion" => bv(snap.prov_superregion.get(&prov).map(String::as_str) == Some(text)),
        "area" => bv(snap.prov_area.get(&prov).map(String::as_str) == Some(text)),
        "province_id" => match text.parse::<u32>() {
            Ok(n) => bv(prov == n),
            Err(_) => {
                un.insert(key.to_string());
                Verdict::Unknown
            }
        },
        "is_capital" => flip(bv(cs.capital == Some(prov))),
        "owned_by" | "owner" => {
            let want = match text.to_ascii_uppercase().as_str() {
                "ROOT" | "THIS" => cs.tag.as_str(),
                _ => text,
            };
            bv(snap.prov_owner.get(&prov).map(String::as_str) == Some(want))
        }
        _ => {
            un.insert(key.to_string());
            Verdict::Unknown
        }
    }
}

fn cheap_prov_key(key: &str) -> bool {
    matches!(key, "religion" | "culture" | "trade_goods")
}

fn prov_attr<'a>(snap: &'a WorldSnapshot, key: &str, id: u32) -> Option<&'a String> {
    match key {
        "religion" => snap.prov_religion.get(&id),
        "culture" => snap.prov_culture.get(&id),
        "trade_goods" => snap.prov_trade_goods.get(&id),
        _ => None,
    }
}

fn eval_quantifier(
    node: &TreeNode,
    cs: &CountryState,
    snap: &WorldSnapshot,
    un: &mut HashSet<String>,
) -> Verdict {
    let key = node.key.as_deref().unwrap_or("").to_ascii_lowercase();
    if key == "any_owned_province" || key == "all_owned_province" {
        // Only cheap province-attribute leaves are supported inside.
        let cheap = node.children.iter().all(|c| {
            c.node_type == "leaf" && c.key.as_deref().map(cheap_prov_key).unwrap_or(false)
        });
        if !cheap || node.children.is_empty() {
            un.insert(key);
            return Verdict::Unknown;
        }
        let prov_matches = |id: u32| {
            node.children.iter().all(|leaf| {
                let k = leaf.key.as_deref().unwrap_or("");
                let want = leaf.value.as_ref().map(|v| v.text.as_str()).unwrap_or("");
                prov_attr(snap, k, id).map(String::as_str) == Some(want)
            })
        };
        if key == "any_owned_province" {
            bv(cs.owned.iter().any(|&id| prov_matches(id)))
        } else {
            bv(!cs.owned.is_empty() && cs.owned.iter().all(|&id| prov_matches(id)))
        }
    } else {
        un.insert(node.key.clone().unwrap_or_default());
        Verdict::Unknown
    }
}

/// Compares `actual` (a modeled field) to `expected`; a missing field is No.
fn cmp(actual: Option<&String>, expected: &str) -> Verdict {
    match actual {
        Some(a) => bv(a == expected),
        None => Verdict::No,
    }
}

fn eval_leaf(
    node: &TreeNode,
    cs: &CountryState,
    snap: &WorldSnapshot,
    un: &mut HashSet<String>,
    depth: u32,
) -> Verdict {
    let Some(v) = &node.value else {
        return Verdict::Unknown;
    };
    let key = node.key.as_deref().unwrap_or("");
    let text = v.text.trim();
    let no_sense = text.eq_ignore_ascii_case("no");
    let flip = |verdict: Verdict| if no_sense { verdict.negate() } else { verdict };

    let num = |un: &mut HashSet<String>, f: &dyn Fn(u32) -> Verdict| match text.parse::<u32>() {
        Ok(n) => f(n),
        Err(_) => {
            un.insert(key.to_string());
            Verdict::Unknown
        }
    };
    let ge = |un: &mut HashSet<String>, actual: f64| match text.parse::<f64>() {
        Ok(t) => bv(actual >= t),
        Err(_) => {
            un.insert(key.to_string());
            Verdict::Unknown
        }
    };
    let group_cmp = |un: &mut HashSet<String>,
                     map: &HashMap<String, String>,
                     member: Option<&String>|
     -> Verdict {
        match member {
            Some(m) => match map.get(m) {
                Some(g) => bv(g == text),
                None => {
                    un.insert(key.to_string());
                    Verdict::Unknown
                }
            },
            None => Verdict::No,
        }
    };

    let result = match key.to_ascii_lowercase().as_str() {
        "tag" | "was_tag" => bv(cs.tag == text),
        "exists" => {
            if no_sense || text.eq_ignore_ascii_case("yes") {
                bv(cs.exists())
            } else {
                bv(snap.countries.get(text).map_or(false, |c| c.exists()))
            }
        }
        "religion" => cmp(cs.religion.as_ref(), text),
        "religion_group" => group_cmp(un, &snap.religion_group, cs.religion.as_ref()),
        "culture" | "primary_culture" => cmp(cs.culture.as_ref(), text),
        "culture_group" => group_cmp(un, &snap.culture_group, cs.culture.as_ref()),
        "government" => cmp(cs.government.as_ref(), text),
        "technology_group" => cmp(cs.tech_group.as_ref(), text),
        "owns" | "controls" => num(un, &|n| bv(cs.owned.contains(&n))),
        "owns_core_province" => {
            num(un, &|n| bv(cs.owned.contains(&n) && cs.cores.contains(&n)))
        }
        "capital" => num(un, &|n| bv(cs.capital == Some(n))),
        "num_of_cities" => ge(un, cs.num_cities as f64),
        "total_development" => ge(un, cs.total_dev as f64),
        "is_year" => ge(un, snap.year as f64),
        "is_subject" => bv(cs.overlord.is_some()),
        "is_subject_of" => cmp(cs.overlord.as_ref(), text),
        "overlord_of" => bv(snap.subject_overlord.get(text) == Some(&cs.tag)),
        "is_at_war" => bv(snap.is_at_war(&cs.tag)),
        "war_with" => bv(snap.war_with(&cs.tag, text)),
        "dynasty" => cmp(cs.dynasty.as_ref(), text),
        // HRE membership (capital province `hre = yes`) and the imperial throne
        // (latest `emperor = TAG` ≤ date). Both decidable → not badge keys.
        "is_part_of_hre" => bv(cs.is_part_of_hre),
        "is_emperor" => bv(cs.is_emperor),
        // `always = yes|no` is a constant; the ubiquitous generic-series gate.
        "always" => Verdict::Yes,
        // Flags are never set at game start.
        "has_country_flag" | "has_global_flag" | "has_ruler_flag" | "has_province_flag" => {
            Verdict::No
        }
        // DLC gates evaluate against what's installed in the base game (all DLC
        // treated as enabled — see WorldSnapshot::installed_dlc). Definitely
        // decidable, so `has_dlc` is NOT recorded as an unevaluated key.
        "has_dlc" => bv(snap.installed_dlc.contains(text)),
        // We model the standard historical start, never a random map, so
        // `map_setup = map_setup_random` is a decisive No (and its ubiquitous
        // `NOT = { … }` wrapper a decisive Yes).
        "map_setup" => bv(!text.eq_ignore_ascii_case("map_setup_random")),
        // A scripted trigger invoked as `name = yes|no`: inline its body (an
        // implicit AND) so a hard verdict propagates instead of leaking Unknown.
        // Look up by the ORIGINAL-case `key`, not the lowercased match value:
        // scripted triggers are stored (and written in game files) case-sensitively,
        // so mixed-case names like `ARB_hedjaz_najd_bedouin_potential` must match
        // their stored key — a lowercased lookup silently misses and leaks Unknown.
        _ if snap.scripted_triggers.contains_key(key) => {
            if !(no_sense || text.eq_ignore_ascii_case("yes")) || depth >= MAX_DEPTH {
                un.insert(key.to_string());
                Verdict::Unknown
            } else {
                let nodes = &snap.scripted_triggers[key];
                eval_nodes(nodes, cs, snap, un, depth + 1)
            }
        }
        _ => {
            un.insert(key.to_string());
            Verdict::Unknown
        }
    };
    flip(result)
}

// ---------------------------------------------------------------------------
// Command
// ---------------------------------------------------------------------------

/// One country's verdict for the evaluated trigger.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CountryVerdict {
    pub tag: String,
    /// `yes` | `no` | `unknown`.
    pub verdict: String,
}

/// The full evaluation: a verdict per existing country + the unevaluated keys.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TriggerEvaluation {
    pub verdicts: Vec<CountryVerdict>,
    /// Condition keys the evaluator could not decide (for the approximate badge).
    pub unevaluated: Vec<String>,
}

/// Evaluates the trigger `nodes` (an implicit AND) against every existing
/// country in `snap`. Pure — the testable core of [`evaluate_trigger`].
pub fn evaluate_all(nodes: &[TreeNode], snap: &WorldSnapshot) -> TriggerEvaluation {
    let mut un: HashSet<String> = HashSet::new();
    let mut verdicts = Vec::new();
    let mut tags: Vec<&String> = snap.countries.keys().collect();
    tags.sort();
    for tag in tags {
        let cs = &snap.countries[tag];
        if !cs.exists() {
            continue;
        }
        let v = eval_nodes(nodes, cs, snap, &mut un, 0);
        verdicts.push(CountryVerdict {
            tag: tag.clone(),
            verdict: v.as_str().to_string(),
        });
    }
    let mut unevaluated: Vec<String> = un.into_iter().collect();
    unevaluated.sort();
    TriggerEvaluation {
        verdicts,
        unevaluated,
    }
}

/// Evaluates the trigger `nodes` (implicit AND) for a SINGLE country's state,
/// **ignoring the `exists()` gate** — the per-tag mission board asks "the tree
/// this tag would receive if playing it at this date", which must answer for
/// formables (tags that own nothing at the date but have a country definition).
/// Existence-dependent conditions still evaluate honestly: `exists = ARB` is No
/// at 1444, `tag = ARB` is Yes when `cs` is ARB, and modeled country fields
/// (religion/culture/…) come from the tag's history/country file where present.
/// Pure — the testable core of the per-tag path in [`evaluate_series_potential`].
pub fn evaluate_for_state(
    nodes: &[TreeNode],
    cs: &CountryState,
    snap: &WorldSnapshot,
) -> (Verdict, Vec<String>) {
    let mut un: HashSet<String> = HashSet::new();
    let v = eval_nodes(nodes, cs, snap, &mut un, 0);
    let mut unevaluated: Vec<String> = un.into_iter().collect();
    unevaluated.sort();
    (v, unevaluated)
}

/// Evaluates the trigger block at `path` inside `file`, at the selected date,
/// for every existing country.
#[tauri::command]
pub fn evaluate_trigger(
    install_path: String,
    mod_path: Option<String>,
    date: Option<String>,
    file: String,
    path: Vec<String>,
) -> Result<TriggerEvaluation, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    let loc = crate::loc::store(&vfs, &install_path, mod_path.as_deref());
    let at = crate::bookmarks::resolve_date(&vfs, date.as_deref())?;
    let bytes = vfs.read(&file)?;
    let nodes = script_tree::build_nodes(&bytes, &path);
    let snap = build_snapshot(&vfs, &loc, at);
    Ok(evaluate_all(&nodes, &snap))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    const INSTALL: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Europa Universalis IV";

    /// Builds a tiny synthetic world: three countries with distinct
    /// religion/culture and a couple of owned provinces each.
    fn fixture() -> WorldSnapshot {
        let mut countries = HashMap::new();
        let mk = |tag: &str, religion: &str, culture: &str, prov: &[u32]| CountryState {
            tag: tag.into(),
            owned: prov.iter().copied().collect(),
            cores: prov.iter().copied().collect(),
            religion: Some(religion.into()),
            culture: Some(culture.into()),
            government: Some("monarchy".into()),
            tech_group: Some("western".into()),
            capital: prov.first().copied(),
            dynasty: Some("de Valois".into()),
            overlord: None,
            num_cities: prov.len(),
            total_dev: prov.len() as f32 * 5.0,
            is_part_of_hre: false,
            is_emperor: false,
        };
        countries.insert("FRA".into(), mk("FRA", "catholic", "cosmopolitan_french", &[183, 184]));
        countries.insert("ENG".into(), mk("ENG", "catholic", "english", &[236]));
        countries.insert("OTT".into(), mk("OTT", "sunni", "turkish", &[151]));

        let mut culture_group = HashMap::new();
        culture_group.insert("cosmopolitan_french".into(), "french".into());
        culture_group.insert("english".into(), "british".into());
        culture_group.insert("turkish".into(), "turko_semitic".into());
        let mut religion_group = HashMap::new();
        religion_group.insert("catholic".into(), "christian".into());
        religion_group.insert("sunni".into(), "muslim".into());

        WorldSnapshot {
            countries,
            culture_group,
            religion_group,
            year: 1500,
            ..Default::default()
        }
    }

    fn nodes(script: &str) -> Vec<TreeNode> {
        // Wrap in a synthetic block so the spans builder addresses "t".
        let src = format!("t = {{\n{script}\n}}");
        script_tree::build_nodes(src.as_bytes(), &["t".into()])
    }

    fn matched(eval: &TriggerEvaluation) -> Vec<&str> {
        eval.verdicts
            .iter()
            .filter(|c| c.verdict == "yes")
            .map(|c| c.tag.as_str())
            .collect()
    }

    #[test]
    fn tag_gate_matches_exactly_that_tag() {
        let snap = fixture();
        let e = evaluate_all(&nodes("tag = FRA"), &snap);
        assert_eq!(matched(&e), vec!["FRA"]);
        assert!(e.unevaluated.is_empty());
    }

    #[test]
    fn or_of_two_religions_matches_both() {
        let snap = fixture();
        // catholic (FRA, ENG) OR sunni (OTT) → all three.
        let e = evaluate_all(
            &nodes("OR = { religion = catholic\nreligion = sunni }"),
            &snap,
        );
        let mut m = matched(&e);
        m.sort();
        assert_eq!(m, vec!["ENG", "FRA", "OTT"]);
    }

    #[test]
    fn religion_and_culture_group_and_not() {
        let snap = fixture();
        // Christian religion group AND french culture group AND NOT English tag.
        let e = evaluate_all(
            &nodes("religion_group = christian\nculture_group = french\nNOT = { tag = ENG }"),
            &snap,
        );
        assert_eq!(matched(&e), vec!["FRA"]);
    }

    #[test]
    fn unknown_key_yields_unknown_and_badge_lists_it() {
        let snap = fixture();
        let e = evaluate_all(
            &nodes("tag = FRA\nmystery_condition = yes\ncustom_thing = yes"),
            &snap,
        );
        // FRA: tag matches (Yes) but the unsupported children poison it → Unknown.
        let fra = e.verdicts.iter().find(|c| c.tag == "FRA").unwrap();
        assert_eq!(fra.verdict, "unknown");
        // ENG/OTT: tag = FRA is No, so the whole AND is honestly No (not unknown).
        assert_eq!(e.verdicts.iter().find(|c| c.tag == "ENG").unwrap().verdict, "no");
        // The badge lists every unevaluated key (recorded even when short-circuited).
        assert!(e.unevaluated.contains(&"mystery_condition".to_string()));
        assert!(e.unevaluated.contains(&"custom_thing".to_string()));
    }

    #[test]
    fn and_no_beats_unknown_or_yes_beats_unknown() {
        // The three-valued invariants the whole evaluator rests on.
        assert_eq!(all_of(&[Verdict::No, Verdict::Unknown]), Verdict::No);
        assert_eq!(all_of(&[Verdict::Unknown, Verdict::No]), Verdict::No);
        assert_eq!(all_of(&[Verdict::Yes, Verdict::Unknown]), Verdict::Unknown);
        assert_eq!(any_of(&[Verdict::Yes, Verdict::Unknown]), Verdict::Yes);
        assert_eq!(any_of(&[Verdict::Unknown, Verdict::Yes]), Verdict::Yes);
        assert_eq!(any_of(&[Verdict::No, Verdict::Unknown]), Verdict::Unknown);
    }

    #[test]
    fn has_dlc_and_map_setup_are_decisive() {
        let mut snap = fixture();
        snap.installed_dlc.insert("Wealth of Nations".into());
        // Installed → Yes; absent → No; neither is recorded as unevaluated.
        let e = evaluate_all(&nodes("has_dlc = \"Wealth of Nations\""), &snap);
        assert_eq!(matched(&e).len(), 3);
        assert!(e.unevaluated.is_empty(), "has_dlc must be decidable, not a badge");
        let e = evaluate_all(&nodes("has_dlc = \"El Dorado\""), &snap);
        assert!(matched(&e).is_empty());
        // The vanilla `NOT = { map_setup = map_setup_random }` gate → Yes for all.
        let e = evaluate_all(&nodes("NOT = { map_setup = map_setup_random }"), &snap);
        assert_eq!(matched(&e).len(), 3);
        assert!(e.unevaluated.is_empty());
        // A DLC-gated tag series that BYZ-shaped countries lack still hard-Nos when
        // the DLC is present but the tag differs (AND short-circuits on the No).
        let e = evaluate_all(&nodes("tag = OTT\nhas_dlc = \"El Dorado\""), &snap);
        assert!(matched(&e).is_empty());
    }

    #[test]
    fn scripted_trigger_mixed_case_name_inlines() {
        // Regression: a scripted trigger with UPPERCASE letters in its name (e.g.
        // vanilla `ARB_hedjaz_najd_bedouin_potential`) must inline. The match arm
        // keys on the lowercased condition name, but the map is case-sensitive, so
        // a lowercased lookup silently missed → the trigger leaked Unknown. The fix
        // looks it up by the original-case key.
        let mut snap = fixture();
        snap.scripted_triggers.insert(
            "ARB_Is_Frenchish".into(),
            nodes("OR = { tag = FRA }"),
        );
        let e = evaluate_all(&nodes("ARB_Is_Frenchish = yes"), &snap);
        assert_eq!(matched(&e), vec!["FRA"]);
        assert!(
            e.unevaluated.is_empty(),
            "mixed-case scripted trigger must inline to a hard verdict, not leak Unknown"
        );
    }

    #[test]
    fn evaluate_for_state_handles_nonexistent_formable_tag() {
        // Part C: a formable's tag has a country definition but owns nothing at the
        // date. `evaluate_for_state` must not panic and must answer honestly.
        let snap = fixture();
        let cs = CountryState {
            tag: "ARB".into(),
            ..Default::default()
        };
        // `tag = ARB` → Yes even though ARB doesn't exist in the snapshot.
        assert_eq!(evaluate_for_state(&nodes("tag = ARB"), &cs, &snap).0, Verdict::Yes);
        // `exists = yes` (self) → No: owns no provinces.
        assert_eq!(evaluate_for_state(&nodes("exists = yes"), &cs, &snap).0, Verdict::No);
        // `exists = FRA` → Yes (FRA exists in the fixture); `exists = ARB` → No.
        assert_eq!(evaluate_for_state(&nodes("exists = FRA"), &cs, &snap).0, Verdict::Yes);
        assert_eq!(evaluate_for_state(&nodes("exists = ARB"), &cs, &snap).0, Verdict::No);
        // Owned-province quantifier over empty ownership → No, no panic.
        assert_eq!(
            evaluate_for_state(&nodes("any_owned_province = { religion = catholic }"), &cs, &snap).0,
            Verdict::No
        );
    }

    #[test]
    fn scripted_trigger_inlines_to_a_hard_verdict() {
        let mut snap = fixture();
        // `is_frenchish = { OR = { tag = FRA } }` → FRA yes, ENG/OTT a hard No
        // (NOT the Unknown an unmodeled key would leak).
        snap.scripted_triggers.insert(
            "is_frenchish".into(),
            nodes("OR = { tag = FRA }"),
        );
        let e = evaluate_all(&nodes("is_frenchish = yes"), &snap);
        assert_eq!(matched(&e), vec!["FRA"]);
        assert_eq!(e.verdicts.iter().find(|c| c.tag == "ENG").unwrap().verdict, "no");
        assert!(e.unevaluated.is_empty(), "resolved scripted trigger is not a badge");
        // `= no` negates the inlined body.
        let e = evaluate_all(&nodes("is_frenchish = no"), &snap);
        let mut m = matched(&e);
        m.sort();
        assert_eq!(m, vec!["ENG", "OTT"]);
    }

    #[test]
    fn installed_dlc_reads_descriptor_folders() {
        // Synthetic mirror of the real layout: dlc/<slug>/<name>.dlc with a
        // `name = "…"` scalar; empty sub-folders (no .dlc) contribute nothing.
        let root = std::env::temp_dir().join("eu_toolkit_installed_dlc_test");
        let _ = std::fs::remove_dir_all(&root);
        let dlc = root.join("dlc");
        std::fs::create_dir_all(dlc.join("dlc010_conquest_of_paradise")).unwrap();
        std::fs::create_dir_all(dlc.join("dlc099_empty_folder")).unwrap();
        std::fs::write(
            dlc.join("dlc010_conquest_of_paradise/dlc010.dlc"),
            b"name = \"Conquest of Paradise\"\naffects_checksum = no\ncategory = \"expansion\"\n",
        )
        .unwrap();
        let set = installed_dlc(&root);
        assert!(set.contains("Conquest of Paradise"));
        assert_eq!(set.len(), 1);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn numeric_and_flag_and_war_conditions() {
        let mut snap = fixture();
        // OTT at war with FRA.
        let mut att = HashSet::new();
        att.insert("OTT".to_string());
        let mut def = HashSet::new();
        def.insert("FRA".to_string());
        snap.war_sides.push((att, def));

        // num_of_cities >= 2 → only FRA (2 provinces).
        assert_eq!(matched(&evaluate_all(&nodes("num_of_cities = 2"), &snap)), vec!["FRA"]);
        // war_with FRA → OTT.
        assert_eq!(matched(&evaluate_all(&nodes("war_with = FRA"), &snap)), vec!["OTT"]);
        // has_country_flag is false at start → nobody.
        assert!(matched(&evaluate_all(&nodes("has_country_flag = x"), &snap)).is_empty());
        // is_year 1500 (year==1500) → everyone; is_year 1600 → nobody.
        assert_eq!(matched(&evaluate_all(&nodes("is_year = 1500"), &snap)).len(), 3);
        assert!(matched(&evaluate_all(&nodes("is_year = 1600"), &snap)).is_empty());
    }

    #[test]
    fn capital_scope_region_continent_and_geography_are_decisive() {
        let mut snap = fixture();
        // FRA capital 183 in europe + x_region/x_area/x_super; ENG cap 236 in
        // europe only; OTT cap 151 nowhere modeled.
        snap.continents
            .insert("europe".into(), [183, 236].into_iter().collect());
        snap.prov_area.insert(183, "x_area".into());
        snap.prov_region.insert(183, "x_region".into());
        snap.prov_superregion.insert(183, "x_super".into());
        snap.prov_owner.insert(183, "FRA".into());

        // capital_scope { region = x_region } → only FRA.
        let e = evaluate_all(&nodes("capital_scope = { region = x_region }"), &snap);
        assert_eq!(matched(&e), vec!["FRA"]);
        assert!(e.unevaluated.is_empty(), "region inside capital_scope is decidable");
        // continent = europe → FRA + ENG (both capitals in europe set), OTT No.
        let e = evaluate_all(&nodes("capital_scope = { continent = europe }"), &snap);
        let mut m = matched(&e);
        m.sort();
        assert_eq!(m, vec!["ENG", "FRA"]);
        assert_eq!(e.verdicts.iter().find(|c| c.tag == "OTT").unwrap().verdict, "no");
        // OR of two regions, area, superregion, and owned_by ROOT all resolve.
        let e = evaluate_all(
            &nodes("capital_scope = { OR = { region = x_region region = y_region } }"),
            &snap,
        );
        assert_eq!(matched(&e), vec!["FRA"]);
        let e = evaluate_all(&nodes("capital_scope = { area = x_area }"), &snap);
        assert_eq!(matched(&e), vec!["FRA"]);
        let e = evaluate_all(&nodes("capital_scope = { superregion = x_super }"), &snap);
        assert_eq!(matched(&e), vec!["FRA"]);
        let e = evaluate_all(&nodes("capital_scope = { owned_by = ROOT }"), &snap);
        assert_eq!(matched(&e), vec!["FRA"]);
        // A NOT of a wrong region is a hard Yes (no leaked Unknown).
        let e = evaluate_all(&nodes("capital_scope = { NOT = { region = z_region } }"), &snap);
        assert_eq!(matched(&e).len(), 3);
        assert!(e.unevaluated.is_empty());
        // An unmodeled province condition inside capital_scope stays honest.
        let e = evaluate_all(&nodes("capital_scope = { has_port = yes }"), &snap);
        assert!(e.unevaluated.contains(&"has_port".to_string()));
    }

    #[test]
    fn hre_membership_emperor_and_always() {
        let mut snap = fixture();
        snap.countries.get_mut("FRA").unwrap().is_part_of_hre = true;
        snap.countries.get_mut("ENG").unwrap().is_emperor = true;
        // is_part_of_hre → only FRA; is_emperor → only ENG; both decidable.
        assert_eq!(matched(&evaluate_all(&nodes("is_part_of_hre = yes"), &snap)), vec!["FRA"]);
        assert_eq!(matched(&evaluate_all(&nodes("is_emperor = yes"), &snap)), vec!["ENG"]);
        let e = evaluate_all(&nodes("NOT = { is_emperor = yes }"), &snap);
        let mut m = matched(&e);
        m.sort();
        assert_eq!(m, vec!["FRA", "OTT"]);
        assert!(e.unevaluated.is_empty());
        // always = yes → everyone; always = no → nobody; never a badge key.
        assert_eq!(matched(&evaluate_all(&nodes("always = yes"), &snap)).len(), 3);
        let e = evaluate_all(&nodes("always = no"), &snap);
        assert!(matched(&e).is_empty());
        assert!(e.unevaluated.is_empty());
    }

    #[test]
    fn tag_scope_change_evaluates_inner_against_that_country() {
        let snap = fixture();
        // ENG = { religion = catholic } is true for everyone (ENG is catholic),
        // regardless of the evaluated country.
        let e = evaluate_all(&nodes("ENG = { religion = catholic }"), &snap);
        assert_eq!(matched(&e).len(), 3);
        assert!(e.unevaluated.is_empty());
    }

    // --- real vanilla decision spot-check (no-op if install absent) ----------

    #[test]
    fn vanilla_form_france_matches_french_countries() {
        if !Path::new(INSTALL).join("map/provinces.bmp").is_file() {
            return;
        }
        let vfs = Vfs::new(INSTALL, None).unwrap();
        let loc = crate::loc::build(&vfs);
        let at = crate::date::DEFAULT_START;
        let snap = build_snapshot(&vfs, &loc, at);

        // Derive by hand: French-culture-group catholic countries at 1444.
        // The formable France decision gates on french culture group; spot-check
        // a few tags whose primary culture is in the french group.
        let french = |tag: &str| {
            snap.countries.get(tag).and_then(|c| c.culture.as_ref()).and_then(|cu| {
                snap.culture_group.get(cu)
            }) == Some(&"french".to_string())
        };
        // FRA, ORL (Orleans), PRO (Provence) are french-culture-group at 1444.
        assert!(french("FRA"), "FRA should be french culture group");

        // Evaluate a minimal france-formation trigger against the snapshot.
        let script = "culture_group = french\nNOT = { tag = FRA }\nis_subject = no";
        let e = evaluate_all(&nodes(script), &snap);
        let yes: HashSet<&str> = e
            .verdicts
            .iter()
            .filter(|c| c.verdict == "yes")
            .map(|c| c.tag.as_str())
            .collect();
        // FRA is excluded by NOT tag = FRA; a french minor like ORL should match
        // if it exists and is independent at 1444.
        assert!(!yes.contains("FRA"));
        // Every matched country is genuinely french culture group (hand rule).
        assert!(yes.iter().all(|t| french(t)), "a non-french tag matched: {yes:?}");
    }

    /// The user-reported case: View ▸ Missions for Aachen (AAC) at 1444.11.11.
    /// With `capital_scope`/geography/`is_part_of_hre`/`is_emperor` modeled, the
    /// obviously-irrelevant African + generic-European series turn from
    /// "possibly (approximate)" into a hard No, while the Westphalian series stay
    /// a definite Yes. Prints the residual (still-Unknown) series so the report
    /// can name what genuinely can't be decided.
    #[test]
    fn aachen_mission_potentials_bucket_correctly() {
        if !Path::new(INSTALL).join("map/provinces.bmp").is_file() {
            return;
        }
        let vfs = Vfs::new(INSTALL, None).unwrap();
        let loc = crate::loc::store(&vfs, INSTALL, None);
        let at = crate::date::DEFAULT_START;
        let snap = build_snapshot(&vfs, &loc, at);

        // Snapshot models Aachen: republic free city, capital 1878 in the HRE and
        // in Europe, not the emperor (Habsburg holds the throne at 1444).
        let aac = snap.countries.get("AAC").expect("AAC exists at 1444");
        assert_eq!(aac.capital, Some(1878), "AAC capital is province 1878");
        assert!(aac.is_part_of_hre, "AAC capital is in the HRE");
        assert!(!aac.is_emperor, "AAC is not the emperor");
        assert_eq!(aac.government.as_deref(), Some("republic"));
        assert!(
            snap.continents.get("europe").unwrap().contains(&1878),
            "province 1878 is on the europe continent"
        );
        assert!(snap.prov_region.contains_key(&1878), "1878 rolls up to a region");
        // Habsburg is the emperor at 1444 (spot-check the emperor loader).
        assert!(
            snap.countries.get("HAB").map_or(false, |c| c.is_emperor),
            "HAB holds the imperial throne at 1444"
        );

        // Bucket every mission series' potential for AAC.
        let mut file_cache: HashMap<String, Vec<u8>> = HashMap::new();
        let mut verdict_of: HashMap<String, Verdict> = HashMap::new();
        for series in crate::missions::load_series(&vfs, &loc) {
            let bytes = file_cache
                .entry(series.file.clone())
                .or_insert_with(|| vfs.read(&series.file).unwrap_or_default());
            let nodes = if series.has_potential {
                script_tree::build_nodes(bytes, &series.potential_path)
            } else {
                Vec::new()
            };
            let mut un = HashSet::new();
            let v = eval_nodes(&nodes, aac, &snap, &mut un, 0);
            verdict_of.insert(series.key.clone(), v);
        }
        let is = |k: &str| verdict_of.get(k).copied().unwrap_or(Verdict::Unknown);

        // Westphalian series: definite Yes (AAC is in their tag list, hessian).
        for k in ["westfalian_group_1", "westfalian_group_2", "westfalian_group_3"] {
            assert_eq!(is(k), Verdict::Yes, "{k} should be a definite Yes for AAC");
        }
        // The whole African trio: definite No (capital region is European).
        for (k, v) in &verdict_of {
            if k.starts_with("central_africa")
                || k.starts_with("east_african")
                || k.starts_with("gen_horn_of_africa")
            {
                assert_eq!(*v, Verdict::No, "{k} should be a definite No for AAC");
            }
        }
        // Generic European gate resolves; AAC (republic free city in the HRE, not
        // emperor) receives gen_europe + the HRE-republic series; the emperor/
        // elector/ban/theocracy variants hard-No on the government/emperor gate.
        assert_eq!(is("gen_europe"), Verdict::Yes);
        assert_eq!(is("gen_europe_hre_republic"), Verdict::Yes);
        assert_eq!(is("gen_europe_hre"), Verdict::No);
        assert_eq!(is("gen_europe_hre_ban"), Verdict::No);
        assert_eq!(is("gen_europe_hre_theocracy"), Verdict::No);
        // The always=yes generic series receive every country.
        for k in ["military_missions", "diplomatic_missions", "administrative_missions"] {
            assert_eq!(is(k), Verdict::Yes, "{k} (always=yes) should be Yes");
        }

        // Residual: only series with genuinely unmodeled potential conditions.
        let total = verdict_of.len();
        let mut residual: Vec<&String> = verdict_of
            .iter()
            .filter(|(_, v)| **v == Verdict::Unknown)
            .map(|(k, _)| k)
            .collect();
        residual.sort();
        let (yes, no) = (
            verdict_of.values().filter(|v| **v == Verdict::Yes).count(),
            verdict_of.values().filter(|v| **v == Verdict::No).count(),
        );
        println!(
            "[aachen] {total} series → {yes} receives / {no} definite-no / {} possibly",
            residual.len()
        );
        println!("[aachen] residual (unmodeled) series: {residual:?}");
        // The residual is a small minority — the overwhelming majority now decide.
        assert!(
            residual.len() * 3 < total,
            "residual {} of {total} should be a small minority",
            residual.len()
        );
    }
}
