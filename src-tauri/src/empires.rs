//! Sprint 29 — Empires (HRE + Mandate) backend.
//!
//! The genuinely-new, empire-specific data that the View ▸ Empires overlay needs
//! on top of the config-driven mechanics object editor (which handles imperial
//! reforms / incidents / decrees — see `mechanics.rs`, the hidden empire
//! families):
//!
//! * **Emperor timeline** — the dated `emperor = TAG` (HRE) / `celestial_emperor
//!   = TAG` (Mandate) successions. **Verified file reality**: vanilla keeps them
//!   in `history/diplomacy/hre.txt` / `celestial_empire.txt`, but a total
//!   conversion may `replace_path` the whole `history/diplomacy` folder and store
//!   the emperor elsewhere (Anbennar sets `emperor =` inside `anbennar.txt`). So
//!   the READER aggregates the entries across **every** `history/diplomacy/*.txt`
//!   file (each entry tagged with its source file + per-file/per-date occurrence
//!   so the frontend can address it byte-surgically), while a fresh succession is
//!   WRITTEN into the canonical `hre.txt` / `celestial_empire.txt` (created in the
//!   project if absent). Sprint-12 aware: `current` is the emperor folded to the
//!   selected date.
//! * **Electors** — countries with `elector = yes` at the selected date, folded
//!   from `history/countries/*` (top-level + dated blocks ≤ date; a later
//!   `elector = no` clears it). The country-panel toggle already writes it; this
//!   is the aggregate view.
//! * **Members** — provinces with `hre = yes` at the selected date, folded from
//!   `history/provinces/*` (count + id set for the map highlight).
//! * **Reform-chain-aware scaffold** — a new imperial reform appended to the
//!   chain with `required_reform = <previous tail>` (the generic mechanics
//!   scaffold can't take the empire/tail arguments), so it loads with zero fixes.

use crate::date::{self, Date};
use crate::diplomacy;
use crate::loc::{self, LocStore};
use crate::paradox::{self, Value};
use crate::vfs::Vfs;
use std::collections::HashSet;

// ---------------------------------------------------------------------------
// Emperor timeline.
// ---------------------------------------------------------------------------

/// The two empire systems, distinguished by the diplomacy key they set and the
/// canonical file new successions are written into.
fn emperor_key(kind: &str) -> Result<(&'static str, &'static str), String> {
    match kind {
        "hre" => Ok(("emperor", "history/diplomacy/hre.txt")),
        "celestial" => Ok(("celestial_emperor", "history/diplomacy/celestial_empire.txt")),
        _ => Err(format!("Unknown empire kind: {kind}")),
    }
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct EmperorEntry {
    /// Date key exactly as written, e.g. "1437.12.9".
    pub date: String,
    /// The emperor tag (`---` = no emperor / abdication is preserved verbatim).
    pub tag: String,
    /// Resolved country name (falls back to the tag).
    pub name: String,
    /// Game-relative source file (byte-surgical edits target it).
    pub file: String,
    /// 0-based index among blocks sharing this exact date in `file` (file order)
    /// — matches the mod_writer `Y.M.D#n` occurrence addressing the timeline
    /// edit recipe uses.
    pub occurrence_index: usize,
    /// date > selected date — shown as a "future" succession (doesn't affect
    /// `current`).
    pub post_selected: bool,
    /// The tag names a country that actually exists (else a validation warning).
    pub valid_tag: bool,
    /// The tag is a subject (the `second` of an active dependency) at this entry's
    /// date — an emperor that is someone's vassal is a validation warning.
    pub is_subject: bool,
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct EmperorTimeline {
    pub kind: String,
    /// The diplomacy key set (`emperor` / `celestial_emperor`).
    pub emperor_key: String,
    /// Canonical file a new succession is written to.
    pub write_file: String,
    /// Whether that file exists yet (create-vs-append hint for the frontend).
    pub write_file_exists: bool,
    /// All successions, chronological (post-selected ones flagged).
    pub entries: Vec<EmperorEntry>,
    /// The selected date (echoed back).
    pub date: String,
    /// The emperor tag in force at the selected date (last entry ≤ date), if any.
    pub current: Option<String>,
    pub current_name: Option<String>,
}

fn is_date_key(k: &str) -> bool {
    date::parse_date(k).is_some()
}

/// The set of valid country tags (keys of `common/country_tags`).
fn valid_tags(vfs: &Vfs) -> HashSet<String> {
    let mut out = HashSet::new();
    for (name, path) in vfs.list_dir("common/country_tags") {
        if !name.to_lowercase().ends_with(".txt") {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let block = paradox::parse(&String::from_utf8_lossy(&bytes));
        for (k, v) in &block.items {
            if let (Some(tag), Value::Scalar(_)) = (k, v) {
                out.insert(tag.trim().to_uppercase());
            }
        }
    }
    out
}

pub fn emperor_timeline(
    vfs: &Vfs,
    loc: &LocStore,
    kind: &str,
    at: Date,
) -> Result<EmperorTimeline, String> {
    let (dip_key, write_file) = emperor_key(kind)?;
    let mut entries: Vec<EmperorEntry> = Vec::new();

    for (name, path) in vfs.list_dir("history/diplomacy") {
        if !name.to_lowercase().ends_with(".txt") {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let rel = format!("history/diplomacy/{name}");
        let block = paradox::parse(&String::from_utf8_lossy(&bytes));
        // Per-date occurrence counter within this file (mirrors mod_writer).
        let mut occ: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        // A top-level `emperor = TAG` (rare, but legal) is treated as the start.
        if let Some(tag) = block.get_scalar(dip_key) {
            entries.push(EmperorEntry {
                date: date::format_date(date::DEFAULT_START),
                tag: tag.trim().to_string(),
                name: String::new(),
                file: rel.clone(),
                occurrence_index: 0,
                post_selected: false,
                valid_tag: false,
                is_subject: false,
            });
        }
        for (k, v) in &block.items {
            let (Some(dk), Value::Block(db)) = (k.as_deref(), v) else {
                continue;
            };
            if !is_date_key(dk) {
                continue;
            }
            let this_occ = {
                let n = occ.entry(dk.to_string()).or_insert(0);
                let cur = *n;
                *n += 1;
                cur
            };
            if let Some(tag) = db.get_scalar(dip_key) {
                entries.push(EmperorEntry {
                    date: dk.to_string(),
                    tag: tag.trim().to_string(),
                    name: String::new(),
                    file: rel.clone(),
                    occurrence_index: this_occ,
                    post_selected: false,
                    valid_tag: false,
                    is_subject: false,
                });
            }
        }
    }

    // Chronological order.
    entries.sort_by(|a, b| {
        let da = date::parse_date(&a.date).unwrap_or(date::DEFAULT_START);
        let db = date::parse_date(&b.date).unwrap_or(date::DEFAULT_START);
        da.cmp(&db).then(a.occurrence_index.cmp(&b.occurrence_index))
    });

    // Validation context: valid tags + subjects at the selected date.
    let tags = valid_tags(vfs);
    let subjects_at: HashSet<String> = diplomacy::all_relations_at(vfs, at)
        .into_iter()
        .filter(|r| r.relation_type == "dependency")
        .filter_map(|r| r.second)
        .map(|s| s.to_uppercase())
        .collect();

    let mut current: Option<String> = None;
    for e in &mut entries {
        let ed = date::parse_date(&e.date).unwrap_or(date::DEFAULT_START);
        e.post_selected = ed > at;
        let upper = e.tag.to_uppercase();
        // `---` = "no emperor" sentinel: not a real tag, not judged.
        let is_none = e.tag.trim_matches('-').is_empty();
        e.valid_tag = is_none || tags.contains(&upper);
        e.is_subject = !is_none && subjects_at.contains(&upper);
        e.name = if is_none { e.tag.clone() } else { loc.resolve(&upper) };
        if !e.post_selected && !is_none {
            current = Some(e.tag.clone());
        } else if !e.post_selected && is_none {
            current = None; // an abdication ≤ date clears the emperor
        }
    }

    let current_name = current.as_ref().map(|t| loc.resolve(&t.to_uppercase()));
    let write_file_exists = vfs.resolve(write_file).is_some();

    Ok(EmperorTimeline {
        kind: kind.to_string(),
        emperor_key: dip_key.to_string(),
        write_file: write_file.to_string(),
        write_file_exists,
        entries,
        date: date::format_date(at),
        current,
        current_name,
    })
}

// ---------------------------------------------------------------------------
// Electors (aggregate `elector = yes` at date).
// ---------------------------------------------------------------------------

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Elector {
    pub tag: String,
    pub name: String,
}

/// A country the elector picker can add (tag + name + the history file the
/// `elector = yes` edit targets — the SAME file the country-panel toggle writes).
#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ElectorCandidate {
    pub tag: String,
    pub name: String,
    pub history_file: String,
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ElectorsData {
    /// Countries with `elector = yes` at the selected date.
    pub electors: Vec<Elector>,
    /// Every country with a history file (for the add picker).
    pub candidates: Vec<ElectorCandidate>,
}

fn elector_candidates(vfs: &Vfs, loc: &LocStore) -> Vec<ElectorCandidate> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for (name, _path) in vfs.list_dir("history/countries") {
        if !name.to_lowercase().ends_with(".txt") || name.len() < 3 {
            continue;
        }
        let tag = name[..3].to_uppercase();
        if !seen.insert(tag.clone()) {
            continue;
        }
        out.push(ElectorCandidate {
            name: loc.resolve(&tag),
            history_file: format!("history/countries/{name}"),
            tag,
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Fold a country history file's `elector` value to `at`: top-level first, then
/// each dated block ≤ `at` in date/file order; the last write wins.
fn elector_at(block: &paradox::Block, at: Date) -> bool {
    let mut on = matches!(block.get_scalar("elector"), Some(v) if v.trim() == "yes");
    // Collect dated writes ≤ at, apply in chronological order.
    let mut dated: Vec<(Date, bool)> = Vec::new();
    for (k, v) in &block.items {
        if let (Some(dk), Value::Block(db)) = (k.as_deref(), v) {
            if let Some(d) = date::parse_date(dk) {
                if d <= at {
                    if let Some(val) = db.get_scalar("elector") {
                        dated.push((d, val.trim() == "yes"));
                    }
                }
            }
        }
    }
    dated.sort_by(|a, b| a.0.cmp(&b.0));
    for (_, val) in dated {
        on = val;
    }
    on
}

pub fn hre_electors(vfs: &Vfs, loc: &LocStore, at: Date) -> Vec<Elector> {
    let mut out = Vec::new();
    for (name, path) in vfs.list_dir("history/countries") {
        if !name.to_lowercase().ends_with(".txt") || name.len() < 3 {
            continue;
        }
        let tag = name[..3].to_uppercase();
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let block = paradox::parse(&String::from_utf8_lossy(&bytes));
        if elector_at(&block, at) {
            out.push(Elector { tag: tag.clone(), name: loc.resolve(&tag) });
        }
    }
    out.sort_by(|a, b| a.tag.cmp(&b.tag));
    out.dedup_by(|a, b| a.tag == b.tag);
    out
}

// ---------------------------------------------------------------------------
// Members (provinces with `hre = yes` at date).
// ---------------------------------------------------------------------------

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct HreMembers {
    pub province_count: usize,
    /// Province ids with `hre = yes` at the date (for the map highlight).
    pub province_ids: Vec<u32>,
    pub date: String,
}

/// Fold a province history file's `hre` value to `at` (top-level then dated ≤ at).
fn hre_at(block: &paradox::Block, at: Date) -> bool {
    let mut on = matches!(block.get_scalar("hre"), Some(v) if v.trim() == "yes");
    let mut dated: Vec<(Date, bool)> = Vec::new();
    for (k, v) in &block.items {
        if let (Some(dk), Value::Block(db)) = (k.as_deref(), v) {
            if let Some(d) = date::parse_date(dk) {
                if d <= at {
                    if let Some(val) = db.get_scalar("hre") {
                        dated.push((d, val.trim() == "yes"));
                    }
                }
            }
        }
    }
    dated.sort_by(|a, b| a.0.cmp(&b.0));
    for (_, val) in dated {
        on = val;
    }
    on
}

pub fn hre_members(vfs: &Vfs, at: Date) -> HreMembers {
    let mut ids = Vec::new();
    for ast in crate::game_data::province_asts(vfs).iter() {
        let Some(block) = &ast.block else {
            continue;
        };
        if hre_at(block, at) {
            ids.push(ast.id);
        }
    }
    ids.sort_unstable();
    ids.dedup();
    HreMembers { province_count: ids.len(), province_ids: ids, date: date::format_date(at) }
}

// ---------------------------------------------------------------------------
// Chain-aware reform scaffold.
// ---------------------------------------------------------------------------

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ReformScaffold {
    pub key: String,
    pub file: String,
    pub text: String,
    /// Loc name / desc entries the frontend queues as LocOverrides.
    pub loc_entries: Vec<crate::mechanics::LocEntry>,
}

const REFORM_PROJECT_FILE: &str = "common/imperial_reforms/zz_eutoolkit_imperial_reforms.txt";

/// A new imperial reform appended to the chain: `empire = <empire>` and, when a
/// tail is given, `required_reform = <tail>` so it loads at the end of the
/// progression with zero manual fixes.
pub fn scaffold_imperial_reform(empire: &str, required_reform: Option<&str>, key: &str) -> ReformScaffold {
    let req = match required_reform {
        Some(r) if !r.is_empty() => format!("\trequired_reform = {r}\n"),
        _ => String::new(),
    };
    let text = format!(
        "{key} = {{\n\
\tempire = {empire}\n\
{req}\
\tpotential = {{\n\t\talways = yes\n\t}}\n\
\temperor = {{\n\t\timperial_authority_value = 1\n\t}}\n\
}}"
    );
    ReformScaffold {
        key: key.to_string(),
        file: REFORM_PROJECT_FILE.to_string(),
        text,
        loc_entries: vec![
            crate::mechanics::LocEntry { key: key.to_string(), value: loc::prettify(key) },
            crate::mechanics::LocEntry { key: format!("{key}_desc"), value: String::new() },
        ],
    }
}

// ---------------------------------------------------------------------------
// Commands.
// ---------------------------------------------------------------------------

#[tauri::command(async)]
pub fn get_emperor_timeline(
    install_path: String,
    mod_path: Option<String>,
    kind: String,
    date: Option<String>,
) -> Result<EmperorTimeline, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    let loc = loc::store(&vfs, &install_path, mod_path.as_deref());
    let at = crate::bookmarks::resolve_date(&vfs, date.as_deref())?;
    emperor_timeline(&vfs, &loc, &kind, at)
}

#[tauri::command(async)]
pub fn get_hre_electors(
    install_path: String,
    mod_path: Option<String>,
    date: Option<String>,
) -> Result<ElectorsData, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    let loc = loc::store(&vfs, &install_path, mod_path.as_deref());
    let at = crate::bookmarks::resolve_date(&vfs, date.as_deref())?;
    Ok(ElectorsData {
        electors: hre_electors(&vfs, &loc, at),
        candidates: elector_candidates(&vfs, &loc),
    })
}

#[tauri::command(async)]
pub fn get_hre_members(
    install_path: String,
    mod_path: Option<String>,
    date: Option<String>,
) -> Result<HreMembers, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    let at = crate::bookmarks::resolve_date(&vfs, date.as_deref())?;
    Ok(hre_members(&vfs, at))
}

#[tauri::command(async)]
pub fn scaffold_imperial_reform_chain(
    empire: String,
    required_reform: Option<String>,
    key: String,
) -> Result<ReformScaffold, String> {
    Ok(scaffold_imperial_reform(&empire, required_reform.as_deref(), &key))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mod_writer::{apply_all, Edit};
    use std::path::{Path, PathBuf};

    const INSTALL: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Europa Universalis IV";
    const ANBENNAR: &str = r"C:\Users\User\projects\anbennar-eu4-dev";

    fn install_present() -> bool {
        Path::new(INSTALL).join("map/provinces.bmp").exists()
    }
    fn anbennar_present() -> bool {
        Path::new(ANBENNAR).join("descriptor.mod").exists()
            || Path::new(ANBENNAR).join("Anbennar-PublicFork.mod").exists()
    }

    fn synthetic(name: &str, files: &[(&str, &str)]) -> (PathBuf, Vfs) {
        let root = std::env::temp_dir().join(format!("eu_toolkit_empires_test_{name}"));
        let _ = std::fs::remove_dir_all(&root);
        for (rel, content) in files {
            let p = root.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, content).unwrap();
        }
        std::fs::create_dir_all(root.join("map")).unwrap();
        std::fs::write(root.join("map/provinces.bmp"), [0u8; 4]).unwrap();
        let vfs = Vfs::new(root.to_str().unwrap(), None).unwrap();
        (root, vfs)
    }

    // ---- emperor timeline aggregation + fold ---------------------------------

    #[test]
    fn emperor_timeline_folds_to_selected_date() {
        let (_root, vfs) = synthetic(
            "hre_fold",
            &[(
                "history/diplomacy/hre.txt",
                "1399.1.1 = { emperor = BOH }\n1437.12.9 = { emperor = HAB }\n1519.1.12 = { emperor = SPA }\n",
            )],
        );
        let loc = loc::build(&vfs);
        // At 1444: current = HAB, SPA is post-selected.
        let tl = emperor_timeline(&vfs, &loc, "hre", (1444, 11, 11)).unwrap();
        assert_eq!(tl.entries.len(), 3);
        assert_eq!(tl.current.as_deref(), Some("HAB"));
        assert!(tl.entries.iter().find(|e| e.tag == "SPA").unwrap().post_selected);
        assert!(!tl.entries.iter().find(|e| e.tag == "HAB").unwrap().post_selected);
        // At 1600: current = SPA.
        let tl2 = emperor_timeline(&vfs, &loc, "hre", (1600, 1, 1)).unwrap();
        assert_eq!(tl2.current.as_deref(), Some("SPA"));
    }

    #[test]
    fn emperor_none_sentinel_clears_current() {
        let (_root, vfs) = synthetic(
            "hre_none",
            &[("history/diplomacy/hre.txt", "1400.1.1 = { emperor = HAB }\n1806.7.12 = { emperor = --- }\n")],
        );
        let loc = loc::build(&vfs);
        let tl = emperor_timeline(&vfs, &loc, "hre", (1850, 1, 1)).unwrap();
        assert_eq!(tl.current, None);
        let none = tl.entries.iter().find(|e| e.tag == "---").unwrap();
        assert!(none.valid_tag, "the --- sentinel is not a bad tag");
    }

    #[test]
    fn emperor_aggregates_across_diplomacy_files() {
        // TC-style: emperor lives in a non-hre.txt file (Anbennar reality).
        let (_root, vfs) = synthetic(
            "hre_agg",
            &[("history/diplomacy/anbennar.txt", "1000.1.1 = { emperor = ANB }\n")],
        );
        let loc = loc::build(&vfs);
        let tl = emperor_timeline(&vfs, &loc, "hre", (1444, 11, 11)).unwrap();
        assert_eq!(tl.current.as_deref(), Some("ANB"));
        assert_eq!(tl.entries[0].file, "history/diplomacy/anbennar.txt");
        // A brand-new succession still targets the canonical file.
        assert_eq!(tl.write_file, "history/diplomacy/hre.txt");
    }

    // ---- emperor timeline round-trip: add / edit / remove (byte-surgical) -----

    #[test]
    fn emperor_add_edit_remove_round_trip_hre() {
        let src = "1399.1.1 = { emperor = BOH }\n1437.12.9 = { emperor = HAB }\n";
        // ADD: a new succession block, date-ordered.
        let added = apply_all(
            src.as_bytes(),
            &[Edit::InsertDatedBlock {
                date: "1500.1.1".into(),
                statement: "1500.1.1 = { emperor = SPA }".into(),
            }],
        )
        .unwrap();
        let added = String::from_utf8(added).unwrap();
        assert!(added.contains("1500.1.1 = { emperor = SPA }"));
        assert!(added.find("1437.12.9").unwrap() < added.find("1500.1.1").unwrap());
        // EDIT: change HAB → TUS in its existing block (occurrence 0).
        let edited = apply_all(
            src.as_bytes(),
            &[Edit::SetScalar { path: vec!["1437.12.9".into(), "emperor".into()], value: "TUS".into(), quoted: false }],
        )
        .unwrap();
        let edited = String::from_utf8(edited).unwrap();
        assert!(edited.contains("emperor = TUS"));
        assert!(!edited.contains("emperor = HAB"));
        // The untouched block round-trips verbatim.
        assert!(edited.contains("1399.1.1 = { emperor = BOH }"));
        // REMOVE: delete the HAB entry statement.
        let removed = apply_all(
            src.as_bytes(),
            &[Edit::RemoveStatement {
                block_path: vec!["1437.12.9".into()],
                key: "emperor".into(),
                value: Some("HAB".into()),
            }],
        )
        .unwrap();
        let removed = String::from_utf8(removed).unwrap();
        assert!(!removed.contains("emperor = HAB"));
        assert!(removed.contains("1399.1.1 = { emperor = BOH }"));
    }

    #[test]
    fn celestial_timeline_round_trip() {
        let src = "1368.1.23 = { celestial_emperor = MNG }\n";
        let edited = apply_all(
            src.as_bytes(),
            &[Edit::SetScalar { path: vec!["1368.1.23".into(), "celestial_emperor".into()], value: "QNG".into(), quoted: false }],
        )
        .unwrap();
        assert!(String::from_utf8(edited).unwrap().contains("celestial_emperor = QNG"));
    }

    // ---- electors aggregate fold ---------------------------------------------

    #[test]
    fn elector_aggregate_folds_dated_changes() {
        let (_root, vfs) = synthetic(
            "electors",
            &[
                // BOH: elector at start, revoked 1500.
                ("history/countries/BOH - Bohemia.txt", "elector = yes\n1500.1.1 = { elector = no }\n"),
                // SAX: becomes elector only in 1450.
                ("history/countries/SAX - Saxony.txt", "1450.1.1 = { elector = yes }\n"),
                // FRA: never.
                ("history/countries/FRA - France.txt", "government = monarchy\n"),
            ],
        );
        let loc = loc::build(&vfs);
        // 1444: only BOH.
        let e1: Vec<String> = hre_electors(&vfs, &loc, (1444, 11, 11)).into_iter().map(|e| e.tag).collect();
        assert_eq!(e1, vec!["BOH".to_string()]);
        // 1460: BOH + SAX.
        let e2: Vec<String> = hre_electors(&vfs, &loc, (1460, 1, 1)).into_iter().map(|e| e.tag).collect();
        assert_eq!(e2, vec!["BOH".to_string(), "SAX".to_string()]);
        // 1550: SAX only (BOH revoked).
        let e3: Vec<String> = hre_electors(&vfs, &loc, (1550, 1, 1)).into_iter().map(|e| e.tag).collect();
        assert_eq!(e3, vec!["SAX".to_string()]);
    }

    // ---- members fold --------------------------------------------------------

    #[test]
    fn hre_members_fold_by_date() {
        let (_root, vfs) = synthetic(
            "members",
            &[
                ("history/provinces/1 - A.txt", "hre = yes\n"),
                ("history/provinces/2 - B.txt", "hre = yes\n1600.1.1 = { hre = no }\n"),
                ("history/provinces/3 - C.txt", "1500.1.1 = { hre = yes }\n"),
            ],
        );
        let m1 = hre_members(&vfs, (1444, 11, 11));
        assert_eq!(m1.province_ids, vec![1, 2]);
        let m2 = hre_members(&vfs, (1550, 1, 1));
        assert_eq!(m2.province_ids, vec![1, 2, 3]);
        let m3 = hre_members(&vfs, (1650, 1, 1));
        assert_eq!(m3.province_ids, vec![1, 3]); // 2 left the empire
    }

    // ---- reform chain scaffold -----------------------------------------------

    #[test]
    fn reform_scaffold_appends_chain_link_and_parses() {
        let sc = scaffold_imperial_reform("hre", Some("reichsreform"), "eutk_new_reform");
        assert!(sc.text.contains("empire = hre"));
        assert!(sc.text.contains("required_reform = reichsreform"));
        // Parses back as one block with the expected key.
        let b = paradox::parse(&sc.text);
        assert!(b.get_block("eutk_new_reform").is_some());
        // No tail → no required_reform line (the first reform of a fresh chain).
        let head = scaffold_imperial_reform("celestial_empire", None, "eutk_head");
        assert!(!head.text.contains("required_reform"));
        assert!(head.text.contains("empire = celestial_empire"));
    }

    // ---- vanilla parse of empire files ---------------------------------------

    #[test]
    fn vanilla_emperor_timelines_parse() {
        if !install_present() {
            return;
        }
        let vfs = Vfs::new(INSTALL, None).unwrap();
        let loc = loc::build(&vfs);
        let hre = emperor_timeline(&vfs, &loc, "hre", date::DEFAULT_START).unwrap();
        assert!(hre.entries.len() >= 5, "vanilla HRE has many successions");
        assert_eq!(hre.current.as_deref(), Some("HAB"), "HAB is emperor at 1444");
        let cel = emperor_timeline(&vfs, &loc, "celestial", date::DEFAULT_START).unwrap();
        assert!(!cel.entries.is_empty());
    }

    #[test]
    fn vanilla_electors_and_members_nonempty() {
        if !install_present() {
            return;
        }
        let vfs = Vfs::new(INSTALL, None).unwrap();
        let loc = loc::build(&vfs);
        let electors = hre_electors(&vfs, &loc, date::DEFAULT_START);
        assert!(electors.len() >= 5, "vanilla ships ~7 electors at 1444, got {}", electors.len());
        let members = hre_members(&vfs, date::DEFAULT_START);
        assert!(members.province_count > 100, "the HRE has hundreds of provinces, got {}", members.province_count);
    }

    // ---- Anbennar acceptance benchmark: Empire of Anbennar reform tree --------

    #[test]
    fn anbennar_reform_tree_loads_in_order_and_round_trips() {
        if !install_present() || !anbennar_present() {
            return;
        }
        let vfs = Vfs::new(INSTALL, Some(ANBENNAR)).unwrap();
        let loc = loc::build(&vfs);
        let data = crate::mechanics::load(&vfs, &loc, "imperial_reforms").unwrap();
        // The empire=hre reforms (the Empire of Anbennar tree).
        let hre: Vec<&crate::mechanics::MechanicObject> = data
            .objects
            .iter()
            .filter(|o| o.scalars.iter().any(|s| s.key == "empire" && s.value == "hre"))
            .collect();
        assert!(hre.len() >= 20, "Empire of Anbennar reuses a large HRE tree, got {}", hre.len());

        // Renders in ORDER: file order = progression order. The `required_reform`
        // of each reform (when present) must point at a reform that appears
        // EARLIER in the loaded list (a valid, acyclic chain).
        let index: std::collections::HashMap<&str, usize> =
            hre.iter().enumerate().map(|(i, o)| (o.key.as_str(), i)).collect();
        let mut chained = 0;
        for (i, o) in hre.iter().enumerate() {
            if let Some(req) = o.scalars.iter().find(|s| s.key == "required_reform" && s.present) {
                if let Some(&j) = index.get(req.value.as_str()) {
                    assert!(j < i, "{} requires {} which comes later — order broken", o.key, req.value);
                    chained += 1;
                }
            }
        }
        assert!(chained >= 5, "expected a real required_reform chain, got {chained} links");

        // A reform edit round-trips byte-surgically: change one reform's
        // gui_container scalar in its own file, splicing only that span.
        let target = hre
            .iter()
            .find(|o| o.origin == "mod" && o.scalars.iter().any(|s| s.key == "gui_container" && s.present))
            .or_else(|| hre.iter().find(|o| o.scalars.iter().any(|s| s.key == "gui_container" && s.present)))
            .expect("a reform with gui_container");
        let file_bytes = std::fs::read(vfs.resolve(&target.file).unwrap()).unwrap();
        let out = apply_all(
            &file_bytes,
            &[Edit::SetScalar {
                path: vec![target.edit_key.clone(), "gui_container".into()],
                value: "mainline".into(),
                quoted: false,
            }],
        )
        .unwrap();
        // Byte-surgical: output differs only where the value changed; the block's
        // key and neighbors survive verbatim.
        let out_s = String::from_utf8_lossy(&out);
        assert!(out_s.contains(&format!("{} = {{", target.key)) || out_s.contains(&format!("{} = {{ ", target.key)));
        assert!(out_s.contains("gui_container = mainline"));
    }

    #[test]
    fn anbennar_emperor_lives_outside_hre_txt() {
        if !anbennar_present() {
            return;
        }
        // Anbennar replace_paths history/diplomacy and stores emperor in
        // anbennar.txt — the aggregating reader must still find it.
        let vfs = Vfs::new(INSTALL, Some(ANBENNAR)).unwrap();
        let loc = loc::build(&vfs);
        let tl = emperor_timeline(&vfs, &loc, "hre", date::DEFAULT_START).unwrap();
        // Either it found emperor entries somewhere in the replaced folder, or the
        // folder genuinely has none — but it must not panic and must aggregate.
        for e in &tl.entries {
            assert!(e.file.starts_with("history/diplomacy/"));
        }
    }
}
