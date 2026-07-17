//! Sprint 17 — the mission-tree designer backend (View ▸ Missions).
//!
//! Mission trees live in `missions/*.txt`. Each file is a flat list of top-level
//! **series** blocks (e.g. `consolidate_expand_russia_missions = { … }`). A series
//! carries layout + gating metadata and a set of **mission** child blocks:
//!
//! ```text
//! <series_key> = {
//!     slot = 1                     # which of the 5 mission-tab columns (series-level only)
//!     generic = no                 # a non-generic series overrides a generic one for its slot
//!     ai = yes
//!     has_country_shield = yes
//!     potential = { … }            # 14.3-evaluable gate: which countries receive the series
//!     potential_on_load = { … }    # (preserved, unmodeled)
//!     <mission_key> = {
//!         icon = mission_build_up_to_force_limit   # a GFX sprite NAME (bare, not GFX_-prefixed)
//!         position = 5             # the ROW within the slot column; ABSENT ⇒ ordinal index
//!         required_missions = { <keys> }           # prerequisite mission keys (bare tokens)
//!         completed_by = 1478.1.15
//!         provinces_to_highlight = { … }           # a TRIGGER block (not a bare id list)
//!         trigger = { … }
//!         effect = { … }
//!     }
//! }
//! ```
//!
//! ## Ground-truth notes (verified against the vanilla install)
//! * **`slot` is series-level only** — there is no mission-level slot (a stray
//!   double-indented `slot` in `GC_Spanish_Missions.txt` is just a series slot).
//!   So a mission's COLUMN is its series' slot; dragging a node is vertical
//!   (position) only — horizontal drag = changing series and is out of scope.
//! * **`position`** is the row; when absent it defaults to the mission's 1-based
//!   ordinal within the series (`effective_position`). The frontend board resolves
//!   any residual row collisions visually.
//! * mission loc keys follow the decisions convention: `<key>_title` / `<key>_desc`.
//! * the `icon` value is a sprite `name` verbatim (e.g. `mission_...`), so the
//!   14.4 sprite picker filters on the `mission_` prefix.
//!
//! Every unmodeled key (`potential_on_load`, custom mechanics, …) round-trips
//! untouched — editing is always a byte-surgical splice of one targeted span, and
//! `required_missions` links are bare-token `AddId`/`RemoveId` list edits.

use std::collections::{HashMap, HashSet};

use crate::loc::{self};
use crate::paradox::{self, Block, Value};
use crate::trigger_eval::{self};
use crate::vfs::Vfs;

/// The game location holding mission files.
const MISSIONS_DIR: &str = "missions";

/// Series-level block keys that are NOT missions (so they're skipped when
/// collecting a series' mission children). Everything else block-valued = mission.
const SERIES_BLOCK_KEYS: &[&str] = &["potential", "potential_on_load"];

/// One mission node inside a series, ready for the board + node editor.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MissionEntry {
    /// The mission script key (e.g. `invade_novgorod_mission`).
    pub key: String,
    /// The `icon` sprite name, if present (a bare `mission_*` GFX name).
    pub icon: Option<String>,
    /// The explicit `position` (row), if written.
    pub position: Option<u32>,
    /// 1-based index of this mission within the series (the absent-position default).
    pub ordinal: u32,
    /// `position` when present, else `ordinal` — the row the board lays it out at.
    pub effective_position: u32,
    /// Prerequisite mission keys from `required_missions` (bare tokens).
    pub required_missions: Vec<String>,
    /// The `completed_by` date scalar, if present.
    pub completed_by: Option<String>,
    /// Loc-resolved title (`<key>_title`, else the prettified key).
    pub title: String,
    /// The `<key>_title` loc key.
    pub title_key: String,
    /// The `<key>_desc` loc key.
    pub desc_key: String,
    /// The raw `<key>_title` loc value if defined (for `LocOverride` edits).
    pub title_loc: Option<String>,
    /// The raw `<key>_desc` loc value if defined.
    pub desc_loc: Option<String>,
    /// Byte-surgical path to the mission block (`["<series>", "<mission>"]`,
    /// occurrence-qualified where a key repeats).
    pub path: Vec<String>,
    /// Path to the `trigger` block (present iff `has_trigger`).
    pub trigger_path: Vec<String>,
    /// Path to the `effect` block (present iff `has_effect`).
    pub effect_path: Vec<String>,
    /// Path to the `provinces_to_highlight` block (present iff `has_provinces`).
    pub provinces_path: Vec<String>,
    /// Path to the `required_missions` id-list (present iff `has_required_block`).
    pub required_path: Vec<String>,
    pub has_trigger: bool,
    pub has_effect: bool,
    pub has_provinces: bool,
    /// Whether a `required_missions = { … }` block is written (so links `AddId`
    /// into it rather than creating a duplicate — create-when-absent guard).
    pub has_required_block: bool,
}

/// One mission series (top-level block in a `missions/*.txt` file).
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MissionSeries {
    /// The series script key (e.g. `consolidate_expand_russia_missions`).
    pub key: String,
    /// Game-relative file the series was found in.
    pub file: String,
    /// `base` or `mod` — which layer the source file came from.
    pub origin: String,
    /// The series' `slot` (1–5), if written.
    pub slot: Option<u32>,
    pub generic: bool,
    pub ai: bool,
    pub has_country_shield: bool,
    pub has_potential: bool,
    /// Byte-surgical path to the series block (`["<series>"]`).
    pub path: Vec<String>,
    /// Path to the `potential` block (present iff `has_potential`).
    pub potential_path: Vec<String>,
    /// The series' missions, in file order.
    pub missions: Vec<MissionEntry>,
}

/// A `#n` occurrence-qualified path segment (bare when the occurrence is 0).
fn segment(key: &str, occ: usize) -> String {
    if occ > 0 {
        format!("{key}#{occ}")
    } else {
        key.to_string()
    }
}

/// Extracts every series from one parsed file's `block`, pushing a
/// [`MissionSeries`] per top-level block. `loc` resolves mission titles.
fn collect_file(
    block: &Block,
    file: &str,
    origin: &str,
    loc: &loc::LocStore,
    out: &mut Vec<MissionSeries>,
) {
    let mut series_occ: HashMap<String, usize> = HashMap::new();
    for (key, value) in &block.items {
        let (Some(series_key), Value::Block(sb)) = (key, value) else {
            continue;
        };
        let occ = series_occ.entry(series_key.clone()).or_insert(0);
        let series_seg = segment(series_key, *occ);
        *occ += 1;

        let series_path = vec![series_seg.clone()];
        let flag = |k: &str| sb.get_scalar(k) == Some("yes");
        let slot = sb.get_scalar("slot").and_then(|s| s.parse().ok());
        let has_potential = sb.get_block("potential").is_some();

        // Collect mission children (every block that isn't a series-level block).
        let mut missions = Vec::new();
        let mut mission_occ: HashMap<String, usize> = HashMap::new();
        for (mk, mv) in &sb.items {
            let (Some(mission_key), Value::Block(mb)) = (mk, mv) else {
                continue;
            };
            if SERIES_BLOCK_KEYS.contains(&mission_key.as_str()) {
                continue;
            }
            let mocc = mission_occ.entry(mission_key.clone()).or_insert(0);
            let mission_seg = segment(mission_key, *mocc);
            *mocc += 1;

            let ordinal = (missions.len() + 1) as u32;
            let mut path = series_path.clone();
            path.push(mission_seg);
            let sub = |name: &str| {
                let mut p = path.clone();
                p.push(name.to_string());
                p
            };

            let position = mb.get_scalar("position").and_then(|s| s.parse().ok());
            let required_block = mb.get_block("required_missions");
            let required_missions = required_block
                .map(|b| b.bare_scalars().map(str::to_string).collect())
                .unwrap_or_default();

            let title_key = format!("{mission_key}_title");
            let desc_key = format!("{mission_key}_desc");
            let title = loc
                .get(&title_key)
                .map(str::to_string)
                .unwrap_or_else(|| loc::prettify(mission_key));

            missions.push(MissionEntry {
                icon: mb.get_scalar("icon").map(str::to_string),
                effective_position: position.unwrap_or(ordinal),
                position,
                ordinal,
                required_missions,
                completed_by: mb.get_scalar("completed_by").map(str::to_string),
                title,
                title_loc: loc.get(&title_key).map(str::to_string),
                desc_loc: loc.get(&desc_key).map(str::to_string),
                title_key,
                desc_key,
                trigger_path: sub("trigger"),
                effect_path: sub("effect"),
                provinces_path: sub("provinces_to_highlight"),
                required_path: sub("required_missions"),
                has_trigger: mb.get_block("trigger").is_some(),
                has_effect: mb.get_block("effect").is_some(),
                has_provinces: mb.get_block("provinces_to_highlight").is_some(),
                has_required_block: required_block.is_some(),
                key: mission_key.clone(),
                path,
            });
        }

        out.push(MissionSeries {
            key: series_key.clone(),
            file: file.to_string(),
            origin: origin.to_string(),
            slot,
            generic: flag("generic"),
            ai: flag("ai"),
            has_country_shield: flag("has_country_shield"),
            has_potential,
            potential_path: {
                let mut p = series_path.clone();
                p.push("potential".to_string());
                p
            },
            path: series_path,
            missions,
        });
    }
}

/// Loads every mission series across the VFS-merged `missions/` files.
pub fn load_series(vfs: &Vfs, loc: &loc::LocStore) -> Vec<MissionSeries> {
    let mut out = Vec::new();
    let mod_dir = vfs.mod_dir();
    for (file_name, path) in vfs.list_dir(MISSIONS_DIR) {
        if !file_name.to_lowercase().ends_with(".txt") {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let origin = if mod_dir.is_some_and(|md| path.starts_with(md)) {
            "mod"
        } else {
            "base"
        };
        let block = paradox::parse(&String::from_utf8_lossy(&bytes));
        let rel = format!("{MISSIONS_DIR}/{file_name}");
        collect_file(&block, &rel, origin, loc, &mut out);
    }
    out
}

/// Tauri command: list all mission series (base + mod) for the Missions overlay.
#[tauri::command]
pub fn get_mission_series(
    install_path: String,
    mod_path: Option<String>,
) -> Result<Vec<MissionSeries>, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    let loc = loc::store(&vfs, &install_path, mod_path.as_deref());
    Ok(load_series(&vfs, &loc))
}

// ---------------------------------------------------------------------------
// Series-potential batch evaluation (which countries receive each series)
// ---------------------------------------------------------------------------

/// One series' `potential` verdict, batched: the tags that pass (`yes`) and the
/// tags whose verdict is `unknown` (the "possibly — approximate" section), plus
/// the condition keys the evaluator couldn't decide (the approximate badge).
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeriesPotential {
    /// The series key (matches [`MissionSeries::key`]).
    pub key: String,
    /// The series' source file (a key alone can repeat across files).
    pub file: String,
    /// Tags whose `potential` evaluates `yes`.
    pub yes: Vec<String>,
    /// Tags whose `potential` evaluates `unknown`.
    pub unknown: Vec<String>,
    /// Condition keys not decided by the evaluator (for the approximate badge).
    pub unevaluated: Vec<String>,
}

/// Evaluates every series' `potential` against the world at `date` — building the
/// world snapshot ONCE and reusing it for all series (no per-country-per-series
/// duplicated work).
///
/// `tag` selects the evaluation shape:
/// * `None` — the **batch** path: a verdict per *existing* country (a series with
///   no `potential` receives every existing country). Used by anything that needs
///   all countries at once.
/// * `Some(tag)` — the **per-tag** path used by the per-country mission board,
///   which only needs one tag's verdicts: evaluate every series against JUST that
///   tag, **including tags that don't exist at the date** (formables like ARB).
///   Semantics: "the tree this tag would receive if playing it at this date" —
///   existence-dependent conditions still evaluate honestly (`exists = ARB` → No
///   at 1444; `tag = ARB` → Yes), and modeled country fields (religion/culture/…)
///   come from the tag's country/history file where present. A series with no
///   `potential` is received unconditionally (`yes`).
#[tauri::command]
pub fn evaluate_series_potential(
    install_path: String,
    mod_path: Option<String>,
    date: Option<String>,
    tag: Option<String>,
) -> Result<Vec<SeriesPotential>, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    let loc = loc::store(&vfs, &install_path, mod_path.as_deref());
    let at = crate::bookmarks::resolve_date(&vfs, date.as_deref())?;
    let snap = trigger_eval::build_snapshot(&vfs, &loc, at);

    // The per-tag country state (may not exist at the date — that's fine). If the
    // tag has no country definition at all, fall back to a default state but keep
    // its tag set so `tag = X` still resolves.
    let tag_state = tag.as_deref().map(|t| {
        let mut cs = snap.countries.get(t).cloned().unwrap_or_default();
        if cs.tag.is_empty() {
            cs.tag = t.to_string();
        }
        cs
    });

    let mut out = Vec::new();
    // One file read per file (series in the same file share bytes).
    let mut file_cache: HashMap<String, Vec<u8>> = HashMap::new();
    for series in load_series(&vfs, &loc) {
        let bytes = match file_cache.get(&series.file) {
            Some(b) => b,
            None => {
                let b = vfs.read(&series.file).unwrap_or_default();
                file_cache.entry(series.file.clone()).or_insert(b)
            }
        };
        // No potential ⇒ no constraint ⇒ received unconditionally.
        let nodes = if series.has_potential {
            crate::script_tree::build_nodes(bytes, &series.potential_path)
        } else {
            Vec::new()
        };
        let (yes, unknown, unevaluated) = match &tag_state {
            // Per-tag: one honest verdict for the requested tag (existence ignored).
            Some(cs) => {
                let (v, un) = trigger_eval::evaluate_for_state(&nodes, cs, &snap);
                let t = cs.tag.clone();
                match v {
                    trigger_eval::Verdict::Yes => (vec![t], Vec::new(), un),
                    trigger_eval::Verdict::Unknown => (Vec::new(), vec![t], un),
                    trigger_eval::Verdict::No => (Vec::new(), Vec::new(), un),
                }
            }
            // Batch: a verdict per existing country.
            None => {
                let eval = trigger_eval::evaluate_all(&nodes, &snap);
                let mut yes = Vec::new();
                let mut unknown = Vec::new();
                for v in eval.verdicts {
                    match v.verdict.as_str() {
                        "yes" => yes.push(v.tag),
                        "unknown" => unknown.push(v.tag),
                        _ => {}
                    }
                }
                (yes, unknown, eval.unevaluated)
            }
        };
        out.push(SeriesPotential {
            key: series.key,
            file: series.file,
            yes,
            unknown,
            unevaluated,
        });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// required_missions cycle detection
// ---------------------------------------------------------------------------

/// The set of missions `start` (transitively) requires, following
/// `required_missions` edges. `start` itself is not included.
fn requires_transitive(edges: &HashMap<String, Vec<String>>, start: &str) -> HashSet<String> {
    let mut seen = HashSet::new();
    let mut stack: Vec<String> = edges.get(start).cloned().unwrap_or_default();
    while let Some(m) = stack.pop() {
        if !seen.insert(m.clone()) {
            continue;
        }
        if let Some(deps) = edges.get(&m) {
            stack.extend(deps.iter().cloned());
        }
    }
    seen
}

/// Whether adding "`dependent` requires `prereq`" would create a cycle in the
/// `required_missions` graph `edges` (mission key → its required keys). True when
/// `prereq == dependent`, or when `prereq` already (transitively) requires
/// `dependent`. Pure — the testable core the link editor rejects cycles with.
pub fn creates_cycle(
    edges: &HashMap<String, Vec<String>>,
    dependent: &str,
    prereq: &str,
) -> bool {
    dependent == prereq || requires_transitive(edges, prereq).contains(dependent)
}

/// Builds the `required_missions` edge map for one series (mission key → required).
fn series_edges(series: &MissionSeries) -> HashMap<String, Vec<String>> {
    series
        .missions
        .iter()
        .map(|m| (m.key.clone(), m.required_missions.clone()))
        .collect()
}

/// Checks whether adding `dependent` requires `prereq` in `series` creates a
/// cycle (Tauri wrapper over [`creates_cycle`]).
#[tauri::command]
pub fn mission_link_creates_cycle(
    install_path: String,
    mod_path: Option<String>,
    series_key: String,
    file: String,
    dependent: String,
    prereq: String,
) -> Result<bool, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    let loc = loc::store(&vfs, &install_path, mod_path.as_deref());
    let series = load_series(&vfs, &loc)
        .into_iter()
        .find(|s| s.key == series_key && s.file == file)
        .ok_or_else(|| format!("Series not found: {series_key} in {file}"))?;
    Ok(creates_cycle(&series_edges(&series), &dependent, &prereq))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    const INSTALL: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Europa Universalis IV";
    const ANBENNAR: &str = r"C:\Users\User\projects\anbennar-eu4-dev";

    fn install_present() -> bool {
        Path::new(INSTALL).join("map/provinces.bmp").is_file()
    }

    const SAMPLE: &[u8] = br#"russia_missions = {
	slot = 1
	generic = no
	ai = yes
	potential = {
		OR = {
			tag = MOS
			tag = RUS
		}
	}
	has_country_shield = yes
	invade_novgorod_mission = {
		icon = mission_build_up_to_force_limit
		required_missions = {  } #None
		completed_by = 1478.1.15
		provinces_to_highlight = {
			owned_by = NOV
		}
		trigger = {
			owns_core_province = 310
		}
		effect = {
			add_permanent_claim = ROOT
		}
	}
	subjugate_novgorod = {
		icon = mission_have_two_subjects
		required_missions = { invade_novgorod_mission }
		trigger = {
			owns = 310
		}
		effect = {
			add_prestige = 10
		}
	}
	partition_poland = {
		icon = mission_assemble_an_army
		required_missions = { subjugate_novgorod }
		position = 5
		effect = {
			add_treasury = 100
		}
	}
}
"#;

    fn parse_sample() -> Vec<MissionSeries> {
        let block = paradox::parse(&String::from_utf8_lossy(SAMPLE));
        let loc = loc::LocStore::from_pairs(&[
            ("invade_novgorod_mission_title", "Invade Novgorod"),
            ("invade_novgorod_mission_desc", "Prepare an invasion."),
        ]);
        let mut out = Vec::new();
        collect_file(&block, "missions/Sample.txt", "base", &loc, &mut out);
        out
    }

    #[test]
    fn collects_series_flags_missions_positions_and_loc() {
        let out = parse_sample();
        assert_eq!(out.len(), 1);
        let s = &out[0];
        assert_eq!(s.key, "russia_missions");
        assert_eq!(s.slot, Some(1));
        assert!(!s.generic && s.ai && s.has_country_shield && s.has_potential);
        assert_eq!(s.path, vec!["russia_missions"]);
        assert_eq!(s.potential_path, vec!["russia_missions", "potential"]);
        assert_eq!(s.missions.len(), 3);

        let m0 = &s.missions[0];
        assert_eq!(m0.key, "invade_novgorod_mission");
        assert_eq!(m0.icon.as_deref(), Some("mission_build_up_to_force_limit"));
        assert_eq!(m0.title, "Invade Novgorod");
        assert_eq!(m0.title_key, "invade_novgorod_mission_title");
        assert_eq!(m0.desc_key, "invade_novgorod_mission_desc");
        assert_eq!(m0.completed_by.as_deref(), Some("1478.1.15"));
        // Empty required block: no requirements, but the block IS present.
        assert!(m0.required_missions.is_empty());
        assert!(m0.has_required_block);
        assert!(m0.has_trigger && m0.has_effect && m0.has_provinces);
        // No explicit position ⇒ ordinal 1 ⇒ effective 1.
        assert_eq!(m0.position, None);
        assert_eq!(m0.ordinal, 1);
        assert_eq!(m0.effective_position, 1);
        assert_eq!(m0.path, vec!["russia_missions", "invade_novgorod_mission"]);

        let m1 = &s.missions[1];
        assert_eq!(m1.required_missions, vec!["invade_novgorod_mission"]);
        assert_eq!(m1.ordinal, 2);
        assert_eq!(m1.effective_position, 2);
        // No title loc → prettified key.
        assert_eq!(m1.title, "Subjugate Novgorod");

        // Explicit position wins over ordinal.
        let m2 = &s.missions[2];
        assert_eq!(m2.position, Some(5));
        assert_eq!(m2.ordinal, 3);
        assert_eq!(m2.effective_position, 5);
        assert_eq!(m2.required_missions, vec!["subjugate_novgorod"]);
    }

    #[test]
    fn mission_paths_feed_the_spans_api() {
        let s = &parse_sample()[0];
        let m0 = &s.missions[0];
        let trig = crate::script_tree::build_script_block(SAMPLE, &m0.trigger_path).unwrap();
        assert!(trig.nodes.iter().any(|n| n.key.as_deref() == Some("owns_core_province")));
        let eff = crate::script_tree::build_script_block(SAMPLE, &m0.effect_path).unwrap();
        assert!(eff.nodes.iter().any(|n| n.key.as_deref() == Some("add_permanent_claim")));
        let prov = crate::script_tree::build_script_block(SAMPLE, &m0.provinces_path).unwrap();
        assert!(prov.nodes.iter().any(|n| n.key.as_deref() == Some("owned_by")));
        // The potential block resolves too.
        let pot = crate::script_tree::build_script_block(SAMPLE, &s.potential_path).unwrap();
        assert!(pot.nodes.iter().any(|n| n.group_kind == "or" || n.key.is_some()));
    }

    #[test]
    fn move_node_position_edit_is_byte_surgical() {
        use crate::mod_writer::{apply, Edit};
        // partition_poland has position = 5; changing it to 7 splices ONLY that
        // scalar — every other byte (siblings, trigger, effect) round-trips.
        let out = apply(
            SAMPLE,
            &Edit::SetScalar {
                path: vec!["russia_missions".into(), "partition_poland".into(), "position".into()],
                value: "7".into(),
                quoted: false,
            },
        )
        .unwrap();
        let marker = b"position = 5";
        let mpos = SAMPLE.windows(marker.len()).position(|w| w == marker).unwrap();
        let vstart = mpos + b"position = ".len();
        let vend = vstart + b"5".len();
        assert_eq!(&SAMPLE[..vstart], &out[..vstart], "prefix byte-identical");
        assert_eq!(&out[vstart..vstart + 1], b"7", "value changed to 7");
        assert_eq!(&SAMPLE[vend..], &out[vstart + 1..], "suffix byte-identical");
    }

    #[test]
    fn add_position_when_absent_is_insert() {
        use crate::mod_writer::{apply, Edit};
        // A mission with no position gets one via InsertStatement into its block.
        let out = apply(
            SAMPLE,
            &Edit::InsertStatement {
                block_path: vec!["russia_missions".into(), "invade_novgorod_mission".into()],
                statement: "position = 2".into(),
            },
        )
        .unwrap();
        let text = String::from_utf8_lossy(&out);
        assert!(text.contains("position = 2"));
        // partition_poland's explicit position is untouched.
        assert!(text.contains("position = 5"));
    }

    #[test]
    fn required_missions_graph_matches_links() {
        // The requirement arrows the board draws come straight from
        // required_missions: partition←subjugate←invade is a linear chain.
        let s = &parse_sample()[0];
        let edges = series_edges(s);
        assert_eq!(edges["invade_novgorod_mission"], Vec::<String>::new());
        assert_eq!(edges["subjugate_novgorod"], vec!["invade_novgorod_mission"]);
        assert_eq!(edges["partition_poland"], vec!["subjugate_novgorod"]);
    }

    #[test]
    fn link_edit_cycle_rejection() {
        // Chain invade → subjugate → partition. Adding "invade requires partition"
        // closes the loop and must be rejected; a fresh forward link is fine.
        let s = &parse_sample()[0];
        let edges = series_edges(s);
        // partition already transitively requires invade, so invade⇒partition cycles.
        assert!(creates_cycle(&edges, "invade_novgorod_mission", "partition_poland"));
        // Self-links always cycle.
        assert!(creates_cycle(&edges, "subjugate_novgorod", "subjugate_novgorod"));
        // subjugate already requires invade, so "invade requires subjugate" cycles too.
        assert!(creates_cycle(&edges, "invade_novgorod_mission", "subjugate_novgorod"));
        // "partition requires invade" is a redundant forward link, NOT a cycle.
        assert!(!creates_cycle(&edges, "partition_poland", "invade_novgorod_mission"));
        // A brand-new mission (not yet in the graph) requiring an existing one is acyclic.
        assert!(!creates_cycle(&edges, "brand_new_mission", "invade_novgorod_mission"));
    }

    #[test]
    fn link_add_and_unlink_round_trip() {
        use crate::mod_writer::{apply, Edit};
        // Linking = AddId into required_missions; unlinking = RemoveId. Add
        // invade→partition's list then remove it: the file returns byte-identical.
        let list_path = vec![
            "russia_missions".to_string(),
            "invade_novgorod_mission".to_string(),
            "required_missions".to_string(),
        ];
        let added = apply(
            SAMPLE,
            &Edit::AddId { list_path: list_path.clone(), id: "subjugate_novgorod".into() },
        )
        .unwrap();
        // Re-parse: invade now requires subjugate.
        let block = paradox::parse(&String::from_utf8_lossy(&added));
        let loc = loc::LocStore::from_pairs(&[]);
        let mut parsed = Vec::new();
        collect_file(&block, "missions/Sample.txt", "base", &loc, &mut parsed);
        let invade = parsed[0].missions.iter().find(|m| m.key == "invade_novgorod_mission").unwrap();
        assert_eq!(invade.required_missions, vec!["subjugate_novgorod"]);

        let removed = apply(
            &added,
            &Edit::RemoveId { list_path, id: "subjugate_novgorod".into() },
        )
        .unwrap();
        // Re-parse: the link is gone again.
        let block2 = paradox::parse(&String::from_utf8_lossy(&removed));
        let mut parsed2 = Vec::new();
        collect_file(&block2, "missions/Sample.txt", "base", &loc, &mut parsed2);
        let invade2 = parsed2[0].missions.iter().find(|m| m.key == "invade_novgorod_mission").unwrap();
        assert!(invade2.required_missions.is_empty(), "link removed");
    }

    #[test]
    fn scaffold_series_parses_back() {
        // The exact shape "+ New series" writes into
        // missions/zz_eutoolkit_missions.txt: a series wrapper (slot/generic/ai/
        // potential) around one starter mission. It must parse back as one series
        // with one mission whose blocks reach through the spans API.
        let scaffold = b"my_new_missions = {\n\tslot = 1\n\tgeneric = no\n\tai = yes\n\thas_country_shield = yes\n\tpotential = {\n\t\ttag = FRA\n\t}\n\tmy_first_mission = {\n\t\ticon = mission_locked_treasure_chest\n\t\tposition = 1\n\t\trequired_missions = { }\n\t\ttrigger = {\n\t\t}\n\t\teffect = {\n\t\t}\n\t}\n}\n";
        let block = paradox::parse(&String::from_utf8_lossy(scaffold));
        let loc = loc::LocStore::from_pairs(&[]);
        let mut out = Vec::new();
        collect_file(&block, "missions/zz_eutoolkit_missions.txt", "mod", &loc, &mut out);
        assert_eq!(out.len(), 1);
        let s = &out[0];
        assert_eq!(s.key, "my_new_missions");
        assert_eq!(s.slot, Some(1));
        assert!(s.has_potential);
        assert_eq!(s.missions.len(), 1);
        let m = &s.missions[0];
        assert_eq!(m.key, "my_first_mission");
        assert_eq!(m.position, Some(1));
        assert!(m.has_trigger && m.has_effect && m.has_required_block);
        assert!(crate::script_tree::build_script_block(scaffold, &m.trigger_path).is_ok());
        assert!(crate::script_tree::build_script_block(scaffold, &s.potential_path).is_ok());
    }

    // --- Real-install smoke tests (no-op if the game/Anbennar is absent) ------

    #[test]
    fn vanilla_lists_series_and_finds_russia_tree() {
        if !install_present() {
            return;
        }
        let vfs = Vfs::new(INSTALL, None).unwrap();
        let loc = loc::build(&vfs);
        let series = load_series(&vfs, &loc);
        assert!(series.len() > 100, "expected many vanilla series, got {}", series.len());

        let russia = series
            .iter()
            .find(|s| s.key == "consolidate_expand_russia_missions")
            .expect("consolidate_expand_russia_missions exists");
        assert_eq!(russia.slot, Some(1));
        assert_eq!(russia.origin, "base");
        assert!(russia.has_country_shield);
        assert!(russia.file.starts_with("missions/"));

        let invade = russia
            .missions
            .iter()
            .find(|m| m.key == "invade_novgorod_mission")
            .expect("invade_novgorod_mission exists");
        assert_eq!(invade.icon.as_deref(), Some("mission_build_up_to_force_limit"));
        assert_eq!(invade.title, "Invade Novgorod");
        // The second Russian mission requires invade_novgorod_mission.
        assert!(
            russia
                .missions
                .iter()
                .any(|m| m.required_missions.iter().any(|r| r == "invade_novgorod_mission")),
            "some Russian mission requires invade_novgorod_mission"
        );
    }

    #[test]
    fn vanilla_series_potential_matches_tag() {
        // A tag-gated Russia tree must land MOS in a definite bucket and hard-
        // exclude ENG (its OR{tag} fails → a decisive No, in neither bucket). Which
        // Russia tree MOS gets is DLC-dependent: `consolidate_expand_russia_missions`
        // is the pre-DLC fallback, gated OFF `has_dlc = "Third Rome"/"Domination"`,
        // so with either installed it hard-Nos and MOS receives the DLC tree instead.
        if !install_present() {
            return;
        }
        let dlc = trigger_eval::installed_dlc(Path::new(INSTALL));
        let pots = evaluate_series_potential(INSTALL.to_string(), None, None, None).unwrap();
        let get = |key: &str| pots.iter().find(|p| p.key == key);
        let receives = |key: &str, tag: &str| {
            get(key).is_some_and(|p| p.yes.iter().chain(&p.unknown).any(|t| t == tag))
        };
        let yes = |key: &str, tag: &str| get(key).is_some_and(|p| p.yes.iter().any(|t| t == tag));

        // The series MOS actually receives (definite yes) depends on the DLC set.
        let (received, fallback_excluded) = if dlc.contains("Domination") {
            ("mos_rus_handle_succession", true)
        } else if dlc.contains("Third Rome") {
            ("tr_russia_conquest_1", true)
        } else {
            ("consolidate_expand_russia_missions", false)
        };
        assert!(yes(received, "MOS"), "MOS should receive {received} (definite yes)");
        if fallback_excluded {
            // The pre-DLC fallback hard-Nos for MOS when a Russia DLC is installed.
            assert!(!receives("consolidate_expand_russia_missions", "MOS"));
        }
        // ENG receives none of these (off-tag → decisive No, neither bucket).
        assert!(!receives(received, "ENG"), "ENG must not receive MOS's tree");
    }

    #[test]
    fn vanilla_byzantium_receives_its_tree_and_hard_excludes_others() {
        // Regression for the "everything is approximate" bug: with has_dlc,
        // map_setup and scripted triggers evaluated, BYZ's real mission tree lands
        // in the definite-`yes` bucket and off-tag / culture-gated trees hard-No.
        if !install_present() {
            return;
        }
        let dlc = trigger_eval::installed_dlc(Path::new(INSTALL));
        let pots = evaluate_series_potential(INSTALL.to_string(), None, None, None).unwrap();
        let find = |key: &str| pots.iter().find(|p| p.key == key);
        let yes = |key: &str| find(key).is_some_and(|p| p.yes.iter().any(|t| t == "BYZ"));
        let unknown = |key: &str| find(key).is_some_and(|p| p.unknown.iter().any(|t| t == "BYZ"));

        // The Byzantine tree the game actually grants depends on the DLC set.
        if dlc.contains("King of Kings") {
            // KoK ships a replacement tree gated ON `has_dlc = "King of Kings"`;
            // the base byz_* series are gated OFF it, so they hard-No for BYZ.
            assert!(yes("MEE_BYZ_conquest_1"), "BYZ should receive its KoK tree (yes)");
            assert!(!yes("byz_western") && !unknown("byz_western"),
                "byz_western is excluded by NOT has_dlc King of Kings → definite No");
        } else {
            assert!(yes("byz_western"), "BYZ should receive byz_western (yes)");
        }

        // iroquoians_1 gates on `is_iroquois = yes` (a tag-list scripted trigger)
        // — BYZ is not in it, so a decisive No regardless of DLC.
        assert!(!yes("iroquoians_1") && !unknown("iroquoians_1"),
            "iroquoians_1 must hard-No for BYZ (is_iroquois → No), not stay approximate");
        // A tag-gated Japanese tree hard-Nos on `tag = JAP`.
        assert!(!yes("DOM_japanse_missions_1") && !unknown("DOM_japanse_missions_1"),
            "a tag=JAP Japanese series must hard-No for BYZ");
    }

    /// Part B regression — England's board must not be empty. The evaluator was
    /// never the ENG blocker (that was a frontend race); this pins that ENG's tree
    /// lands in the definite-`yes` bucket per the DLC set. Potentials read from the
    /// vanilla files (this install has Domination + Rule Britannia):
    ///   DOM_Britain_Missions `eng_row_slot_1` (governs when Domination present):
    ///     has_dlc = "Domination"  (Yes, installed)
    ///     OR = { tag = ENG  tag = AVE  tag = GBR }  (Yes for ENG)
    ///     NOT = { map_setup = map_setup_random }  (Yes)   → definite Yes.
    ///   Pre-Domination `English_Missions`/`RB_English_Missions` are gated
    ///   `NOT = { has_dlc = "Domination" }` → definite No when Domination present.
    #[test]
    fn vanilla_eng_per_tag_receives_its_tree() {
        if !install_present() {
            return;
        }
        let dlc = trigger_eval::installed_dlc(Path::new(INSTALL));
        let pots =
            evaluate_series_potential(INSTALL.to_string(), None, None, Some("ENG".to_string()))
                .unwrap();
        let yes = |k: &str| {
            pots.iter()
                .find(|p| p.key == k)
                .is_some_and(|p| p.yes.iter().any(|t| t == "ENG"))
        };
        let received = pots.iter().filter(|p| p.yes.iter().any(|t| t == "ENG")).count()
            + pots.iter().filter(|p| p.unknown.iter().any(|t| t == "ENG")).count();
        // The whole point of the bug: England's board is NOT empty.
        assert!(received > 0, "ENG must receive at least one series (board not empty)");
        // The ubiquitous always=yes generic series are a definite Yes.
        assert!(yes("military_missions"), "ENG receives the always=yes generic missions");

        // ENG's real, DLC-appropriate tree lands in the definite-yes bucket.
        if dlc.contains("Domination") {
            assert!(yes("eng_row_slot_1"), "ENG receives its Domination Britain tree (yes)");
        } else if dlc.contains("Rule Britannia") {
            assert!(yes("eng_british_naval_conq"), "ENG receives its Rule Britannia tree (yes)");
        }
    }

    /// Part C — a FORMABLE (ARB does not exist at 1444) receives its tag-gated tree
    /// via the per-tag filter. `KoK_ARB_hedjaz_najd_bedouin_1` potential (KoK
    /// installed here) is:
    ///   ARB_hedjaz_najd_bedouin_potential = yes   # scripted OR{ tag = ARB … }
    ///   has_dlc = "King of Kings"  (Yes)
    ///   NOT = { map_setup = map_setup_random }  (Yes)
    /// Evaluating for ARB, the scripted trigger inlines `tag = ARB` → Yes, so the
    /// series is a definite Yes — but only after the mixed-case scripted-trigger
    /// lookup fix (it previously leaked Unknown). ENG must NOT receive it.
    #[test]
    fn vanilla_arb_formable_per_tag_receives_tag_gated_tree() {
        if !install_present() {
            return;
        }
        let dlc = trigger_eval::installed_dlc(Path::new(INSTALL));
        // No panic on a non-existent-at-date tag.
        let arb =
            evaluate_series_potential(INSTALL.to_string(), None, None, Some("ARB".to_string()))
                .unwrap();
        let arb_yes = |k: &str| {
            arb.iter()
                .find(|p| p.key == k)
                .is_some_and(|p| p.yes.iter().any(|t| t == "ARB"))
        };
        if dlc.contains("King of Kings") {
            assert!(
                arb_yes("KoK_ARB_hedjaz_najd_bedouin_1"),
                "ARB (formable) must receive its KoK tag-gated tree as a definite Yes"
            );
            // ENG must NOT receive ARB's tag-gated series.
            let eng =
                evaluate_series_potential(INSTALL.to_string(), None, None, Some("ENG".to_string()))
                    .unwrap();
            let eng_touches = eng.iter().find(|p| p.key == "KoK_ARB_hedjaz_najd_bedouin_1").is_some_and(
                |p| p.yes.iter().chain(&p.unknown).any(|t| t == "ENG"),
            );
            assert!(!eng_touches, "ENG must not receive ARB's tag-gated tree (decisive No)");
        }
    }

    #[test]
    fn anbennar_missions_smoke() {
        if !install_present() || !Path::new(ANBENNAR).is_dir() {
            return;
        }
        let vfs = Vfs::new(INSTALL, Some(ANBENNAR)).unwrap();
        let loc = loc::build(&vfs);
        let series = load_series(&vfs, &loc);
        assert!(!series.is_empty());
        // Anbennar replaces missions via replace_path; at least one series must be
        // mod-origin and carry missions.
        assert!(
            series.iter().any(|s| s.origin == "mod" && !s.missions.is_empty()),
            "Anbennar should contribute mod-origin mission series"
        );
    }
}
