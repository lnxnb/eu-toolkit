//! Sprint 19.3 — Government names (dynamic country names & ruler titles).
//!
//! `common/government_names/*.txt` holds naming *schemes*: for a matching country
//! it supplies the localized country name (by government rank) and the ruler /
//! consort titles (by rank + gender). One scheme:
//!
//! ```text
//! russian_feudal_monarchy = {
//!     rank          = { 1 = PRINCEDOM 2 = GRAND_PRINCIPALITY 3 = EMPIRE }  # loc KEYS
//!     ruler_male    = { 1 = KNIAZ 2 = GREAT_KNIAZ 3 = EMPEROR }
//!     ruler_female  = { 1 = KNIAGINIA 2 = GREAT_KNIAGINIA 3 = EMPRESS }
//!     consort_male  = { … }
//!     consort_female= { … }
//!     heir_male     = { … }   # unmodeled → preserved raw
//!     heir_female   = { … }   # unmodeled → preserved raw
//!     trigger       = { government = monarchy OR = { culture_group = east_slavic … } }
//! }
//! ```
//!
//! **First valid entry in file order wins** (per the file's own leading comment,
//! "Will pick the first valid one it finds in list"). Directories load additively
//! (every file, mod files add to / shadow the base set); within a file, block
//! order is precedence.
//!
//! # Editing model (existing typed-edit vocabulary only)
//! * A rank×role **cell** is a loc KEY — editing the displayed string writes a
//!   `LocOverride` on that key; the government_names file is never touched.
//! * The scheme's **trigger** is edited through the 14.2 tree editor
//!   (`SetScalar`/`SetBlock`/`InsertStatement`/`RemoveStatement` under
//!   `[key, "trigger"]`).
//! * **Reorder** within a file swaps two adjacent schemes' block bodies with two
//!   `SetBlock`s (the 19.2 pattern) — byte-surgical; every other byte round-trips.
//!   The block KEY is a purely internal label the game ignores (it matches by
//!   content), so a body swap flips precedence cleanly.
//! * **Create** appends a scaffold block into the project `zz_` file + queues a
//!   `LocOverride` for every cell (zero-manual-fixes: loads in game immediately).
//! * Unmodeled keys (`heir_male`/`heir_female`/anything custom) are preserved
//!   untouched and surfaced read-only.

use crate::date::Date;
use crate::loc::LocStore;
use crate::paradox::{self, Block};
use crate::script_tree;
use crate::trigger_eval::{self, CountryState, Verdict};
use crate::vfs::Vfs;

pub const DIR: &str = "common/government_names";
/// Project-owned file new schemes scaffold into (additive; never shadows vanilla).
pub const PROJECT_FILE: &str = "common/government_names/zz_eutoolkit_government_names.txt";

/// The modeled rank×gender role blocks, in table-column order. `rank` is the
/// country-name column; the others are ruler / consort titles.
pub const ROLES: &[&str] = &[
    "rank",
    "ruler_male",
    "ruler_female",
    "consort_male",
    "consort_female",
];

/// Top-level keys the editor models. Everything else → `raw_extra` (preserve).
const KNOWN_KEYS: &[&str] = &[
    "rank",
    "ruler_male",
    "ruler_female",
    "consort_male",
    "consort_female",
    "trigger",
];

// ---------------------------------------------------------------------------
// Payload types (serialize snake_case; mirrored by src/lib/governmentNames.ts).
// ---------------------------------------------------------------------------

/// One rank×role cell: a loc key + its resolved display string.
#[derive(serde::Serialize, Clone, Debug)]
pub struct GovNameCell {
    /// One of [`ROLES`].
    pub role: String,
    /// Government rank 1..3.
    pub rank: u32,
    /// The loc key written in the file (e.g. `PRINCEDOM`).
    pub loc_key: String,
    /// Resolved display string (falls back to the prettified key).
    pub resolved: String,
}

/// One naming scheme block.
#[derive(serde::Serialize, Clone, Debug)]
pub struct GovNameScheme {
    pub key: String,
    /// Game-relative source file.
    pub file: String,
    /// `base` | `mod` (origin badge).
    pub origin: String,
    /// Whether a `trigger = { … }` sub-block is present.
    pub has_trigger: bool,
    /// Present rank×role cells (absent ranks simply don't appear).
    pub cells: Vec<GovNameCell>,
    /// Unmodeled top-level keys (heir_male/heir_female/custom) — preserve-unknown.
    pub raw_extra: Vec<String>,
    /// Braces-inclusive raw block body text (drives the reorder body swap).
    pub raw: String,
}

#[derive(serde::Serialize, Clone, Debug)]
pub struct GovernmentNamesData {
    pub dir: String,
    pub project_file: String,
    pub schemes: Vec<GovNameScheme>,
}

/// A brand-new scheme scaffold: the block text + the loc keys/defaults the
/// frontend queues as `LocOverride`s.
#[derive(serde::Serialize, Clone, Debug)]
pub struct GovNameScaffold {
    pub key: String,
    pub file: String,
    pub text: String,
    pub cells: Vec<GovNameCell>,
}

/// The country-panel preview: which scheme this country currently resolves to at
/// the selected date, and the resolved rank-appropriate strings.
#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GovNamePreview {
    pub tag: String,
    /// The country's government rank (1..3) at the date.
    pub rank: u32,
    /// The first fully-matching scheme's key, or None if nothing matched.
    pub matched_key: Option<String>,
    pub matched_file: Option<String>,
    /// Resolved country name (the `rank` cell at the country's rank).
    pub country_name: Option<String>,
    /// Resolved ruler title (`ruler_male`, else `ruler_female`, at the rank).
    pub ruler_name: Option<String>,
    /// True when at least one scheme BEFORE the match couldn't be evaluated —
    /// the shown result "may not be exact".
    pub approximate: bool,
    /// Scheme keys skipped as unevaluable before the match (for the note).
    pub skipped: Vec<String>,
}

// ---------------------------------------------------------------------------
// Parse.
// ---------------------------------------------------------------------------

/// Reads every scheme across the government_names directory, in file order
/// (== precedence). `origin` is `mod` when the file physically lives under the
/// mod layer, else `base`.
pub fn load(vfs: &Vfs, loc: &LocStore) -> GovernmentNamesData {
    let mod_dir = vfs.mod_dir();
    let mut schemes: Vec<GovNameScheme> = Vec::new();
    for (name, path) in vfs.list_dir(DIR) {
        if !name.to_lowercase().ends_with(".txt") {
            continue;
        }
        let rel = format!("{DIR}/{name}");
        let Ok(bytes) = vfs.read(&rel) else { continue };
        let origin = if mod_dir.map_or(false, |m| path.starts_with(m)) {
            "mod"
        } else {
            "base"
        };
        let text = String::from_utf8_lossy(&bytes);
        let block = paradox::parse(&text);
        for (key, b) in block.key_blocks() {
            schemes.push(parse_scheme(&bytes, key, b, loc, &rel, origin));
        }
    }
    GovernmentNamesData {
        dir: DIR.to_string(),
        project_file: PROJECT_FILE.to_string(),
        schemes,
    }
}

fn parse_scheme(
    file_bytes: &[u8],
    key: &str,
    b: &Block,
    loc: &LocStore,
    file: &str,
    origin: &str,
) -> GovNameScheme {
    let mut cells: Vec<GovNameCell> = Vec::new();
    for &role in ROLES {
        let Some(rb) = b.get_block(role) else { continue };
        for rank in 1u32..=3 {
            if let Some(lk) = rb.get_scalar(&rank.to_string()) {
                let lk = lk.trim().trim_matches('"').to_string();
                if lk.is_empty() {
                    continue;
                }
                cells.push(GovNameCell {
                    role: role.to_string(),
                    rank,
                    resolved: loc.resolve(&lk),
                    loc_key: lk,
                });
            }
        }
    }

    let mut raw_extra: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (k, _) in &b.items {
        if let Some(k) = k.as_deref() {
            if !KNOWN_KEYS.contains(&k) && seen.insert(k.to_string()) {
                raw_extra.push(k.to_string());
            }
        }
    }

    let raw = crate::mod_writer::block_span(file_bytes, &[key.to_string()])
        .map(|(s, e)| String::from_utf8_lossy(&file_bytes[s..e]).into_owned())
        .unwrap_or_default();

    GovNameScheme {
        key: key.to_string(),
        file: file.to_string(),
        origin: origin.to_string(),
        has_trigger: b.get_block("trigger").is_some(),
        cells,
        raw_extra,
        raw,
    }
}

// ---------------------------------------------------------------------------
// Scaffold (unit-tested to parse back with the keys the game requires).
// ---------------------------------------------------------------------------

/// The default display string for a fresh scaffold cell (a sensible, immediately
/// game-loadable placeholder the user then edits).
fn tier_default(role: &str, rank: u32) -> &'static str {
    match (role, rank) {
        ("rank", 1) => "Duchy",
        ("rank", 2) => "Kingdom",
        ("rank", 3) => "Empire",
        ("ruler_male" | "consort_male", 1) => "Duke",
        ("ruler_male" | "consort_male", 2) => "King",
        ("ruler_male" | "consort_male", 3) => "Emperor",
        ("ruler_female" | "consort_female", 1) => "Duchess",
        ("ruler_female" | "consort_female", 2) => "Queen",
        ("ruler_female" | "consort_female", 3) => "Empress",
        _ => "",
    }
}

/// The loc key a scaffold cell uses: `<UPPERKEY>_<UPPERROLE>_<rank>`.
pub fn cell_loc_key(scheme_key: &str, role: &str, rank: u32) -> String {
    format!("{}_{}_{}", scheme_key.to_uppercase(), role.to_uppercase(), rank)
}

/// Builds a full scheme scaffold: all five roles × three ranks referencing
/// generated loc keys, plus an empty trigger (an empty trigger is always-true —
/// the user narrows it in the tree editor). Authored at column 0.
pub fn scaffold(key: &str) -> GovNameScaffold {
    let mut cells: Vec<GovNameCell> = Vec::new();
    let mut body = String::new();
    body.push_str(&format!("{key} = {{\n"));
    for &role in ROLES {
        body.push_str(&format!("\t{role} = {{\n"));
        for rank in 1u32..=3 {
            let lk = cell_loc_key(key, role, rank);
            body.push_str(&format!("\t\t{rank} = {lk}\n"));
            cells.push(GovNameCell {
                role: role.to_string(),
                rank,
                resolved: tier_default(role, rank).to_string(),
                loc_key: lk,
            });
        }
        body.push_str("\t}\n\n");
    }
    body.push_str("\ttrigger = {\n\t}\n}");

    GovNameScaffold {
        key: key.to_string(),
        file: PROJECT_FILE.to_string(),
        text: body,
        cells,
    }
}

// ---------------------------------------------------------------------------
// Preview (14.3 evaluator, first valid match in file order).
// ---------------------------------------------------------------------------

/// Resolves which scheme a country currently uses at `date`, evaluating each
/// scheme's trigger in file order and taking the first fully-valid match. A
/// scheme with no trigger (or an empty one) is always-valid.
pub fn preview(vfs: &Vfs, loc: &LocStore, tag: &str, date: Date) -> GovNamePreview {
    let snap = trigger_eval::build_snapshot(vfs, loc, date);
    let cs: CountryState = snap.countries.get(tag).cloned().unwrap_or(CountryState {
        tag: tag.to_string(),
        ..Default::default()
    });

    // The country's government rank at the date (default 1, EU4's default).
    let rank = crate::game_data::country_details_at(vfs, loc, tag, date)
        .ok()
        .and_then(|d| d.government_rank)
        .unwrap_or(1)
        .clamp(1, 3) as u32;

    let data = load(vfs, loc);
    let mut skipped: Vec<String> = Vec::new();
    let mut matched: Option<&GovNameScheme> = None;

    // Cache file bytes so each scheme's trigger tree builds without re-reading.
    let mut file_cache: std::collections::HashMap<String, Vec<u8>> = std::collections::HashMap::new();
    for scheme in &data.schemes {
        let verdict = if scheme.has_trigger {
            let bytes = file_cache
                .entry(scheme.file.clone())
                .or_insert_with(|| vfs.read(&scheme.file).unwrap_or_default());
            let nodes =
                script_tree::build_nodes(bytes, &[scheme.key.clone(), "trigger".to_string()]);
            trigger_eval::evaluate_for_state(&nodes, &cs, &snap).0
        } else {
            // No trigger block → unconditional (always matches).
            Verdict::Yes
        };
        match verdict {
            Verdict::Yes => {
                matched = Some(scheme);
                break;
            }
            Verdict::Unknown => skipped.push(scheme.key.clone()),
            Verdict::No => {}
        }
    }

    let (country_name, ruler_name) = match matched {
        Some(s) => {
            let cell = |role: &str| {
                s.cells
                    .iter()
                    .find(|c| c.role == role && c.rank == rank)
                    .map(|c| c.resolved.clone())
            };
            (
                cell("rank"),
                cell("ruler_male").or_else(|| cell("ruler_female")),
            )
        }
        None => (None, None),
    };

    GovNamePreview {
        tag: tag.to_string(),
        rank,
        matched_key: matched.map(|s| s.key.clone()),
        matched_file: matched.map(|s| s.file.clone()),
        country_name,
        ruler_name,
        approximate: !skipped.is_empty(),
        skipped,
    }
}

// ---------------------------------------------------------------------------
// Commands (registered in lib.rs).
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_government_names(
    install_path: String,
    mod_path: Option<String>,
) -> Result<GovernmentNamesData, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    let loc = crate::loc::store(&vfs, &install_path, mod_path.as_deref());
    Ok(load(&vfs, &loc))
}

#[tauri::command]
pub fn scaffold_government_name(key: String) -> GovNameScaffold {
    scaffold(&key)
}

#[tauri::command]
pub fn preview_government_name(
    install_path: String,
    mod_path: Option<String>,
    date: Option<String>,
    tag: String,
) -> Result<GovNamePreview, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    let loc = crate::loc::store(&vfs, &install_path, mod_path.as_deref());
    let at = crate::bookmarks::resolve_date(&vfs, date.as_deref())?;
    Ok(preview(&vfs, &loc, &tag, at))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mod_writer::{apply, Edit};
    use std::path::Path;

    const INSTALL: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Europa Universalis IV";
    const ANBENNAR: &str = r"C:\Users\User\projects\anbennar-eu4-dev";

    fn synthetic(name: &str, files: &[(&str, &str)]) -> (std::path::PathBuf, Vfs) {
        let root = std::env::temp_dir().join(format!("eu_toolkit_govnames_test_{name}"));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        for (rel, contents) in files {
            let path = root.join(rel);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(path, contents).unwrap();
        }
        let vfs = Vfs::new(root.to_str().unwrap(), None).unwrap();
        (root, vfs)
    }

    // Two schemes: a specific culture-gated one (fully evaluable) and a generic
    // unconditional fallback. Scheme A carries an unmodeled heir_male block +
    // consort roles; scheme B has no trigger (always matches).
    const SRC: &str = "\
alpha_monarchy = {\n\
\trank = {\n\t\t1 = A_DUCHY\n\t\t2 = A_KINGDOM\n\t\t3 = A_EMPIRE\n\t}\n\
\truler_male = {\n\t\t1 = A_DUKE\n\t\t2 = A_KING\n\t\t3 = A_EMPEROR\n\t}\n\
\truler_female = {\n\t\t1 = A_DUCHESS\n\t\t2 = A_QUEEN\n\t\t3 = A_EMPRESS\n\t}\n\
\tconsort_male = {\n\t\t1 = A_DUKE\n\t\t2 = A_KING\n\t\t3 = A_EMPEROR\n\t}\n\
\tconsort_female = {\n\t\t1 = A_DUCHESS\n\t\t2 = A_QUEEN\n\t\t3 = A_EMPRESS\n\t}\n\
\their_male = {\n\t\t1 = A_HEIR\n\t\t2 = A_HEIR\n\t\t3 = A_HEIR\n\t}\n\
\ttrigger = {\n\t\tgovernment = monarchy\n\t\tprimary_culture = french\n\t}\n\
}\n\
generic_fallback = {\n\
\trank = {\n\t\t1 = G_DUCHY\n\t\t2 = G_KINGDOM\n\t\t3 = G_EMPIRE\n\t}\n\
\truler_male = {\n\t\t1 = G_DUKE\n\t\t2 = G_KING\n\t\t3 = G_EMPEROR\n\t}\n\
}\n";

    fn fixture(name: &str) -> (std::path::PathBuf, Vfs) {
        synthetic(name, &[("common/government_names/00_gov.txt", SRC)])
    }

    #[test]
    fn parses_schemes_roles_and_raw_extra() {
        let (_root, vfs) = fixture("parse");
        let loc = crate::loc::LocStore::from_pairs(&[("A_DUCHY", "Duchy of A"), ("A_KING", "King of A")]);
        let data = load(&vfs, &loc);
        assert_eq!(data.schemes.len(), 2);
        let a = &data.schemes[0];
        assert_eq!(a.key, "alpha_monarchy");
        assert!(a.has_trigger);
        assert_eq!(a.origin, "base");
        // rank + 4 title roles × 3 = 15 modeled cells (heir excluded).
        assert_eq!(a.cells.len(), 15, "rank + 4 title roles × 3 ranks");
        let c = |role: &str, rank: u32| a.cells.iter().find(|c| c.role == role && c.rank == rank);
        assert_eq!(c("rank", 1).unwrap().loc_key, "A_DUCHY");
        assert_eq!(c("rank", 1).unwrap().resolved, "Duchy of A");
        assert_eq!(c("ruler_male", 2).unwrap().loc_key, "A_KING");
        assert_eq!(c("ruler_male", 2).unwrap().resolved, "King of A");
        // heir_male is unmodeled → preserved raw, not a cell.
        assert!(a.raw_extra.contains(&"heir_male".to_string()));
        assert!(a.cells.iter().all(|c| c.role != "heir_male"));
        // The generic fallback has no trigger.
        let g = &data.schemes[1];
        assert!(!g.has_trigger);
        assert_eq!(g.key, "generic_fallback");
    }

    #[test]
    fn cell_edit_is_loc_only_file_untouched() {
        // Editing a cell is a LocOverride on the loc key — no government_names
        // file edit is emitted. Prove the on-disk override lands and the scheme
        // file bytes are unaffected by any file-level edit (there are none).
        let dir = std::env::temp_dir().join("eu_toolkit_govnames_cell_loc");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let rel = crate::loc::write_overrides(&dir, &[("A_DUCHY".into(), "Palatinate".into())]).unwrap();
        assert_eq!(rel, crate::loc::OVERRIDE_REL);
        let bytes = std::fs::read(dir.join(crate::loc::OVERRIDE_REL)).unwrap();
        let mut m = std::collections::HashMap::new();
        crate::loc::parse_into(&String::from_utf8_lossy(&bytes), &mut m);
        assert_eq!(m.get("A_DUCHY").map(String::as_str), Some("Palatinate"));
    }

    #[test]
    fn trigger_edit_is_byte_surgical() {
        // Editing a leaf in the trigger tree changes only that value; all cells
        // and the sibling scheme round-trip.
        let out = apply(
            SRC.as_bytes(),
            &Edit::SetScalar {
                path: vec!["alpha_monarchy".into(), "trigger".into(), "government".into()],
                value: "republic".into(),
                quoted: false,
            },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("government = republic"));
        assert!(text.contains("primary_culture = french"));
        assert!(text.contains("1 = A_DUCHY"));
        assert!(text.contains("generic_fallback = {"));
        assert!(text.contains("1 = G_DUCHY"));
    }

    #[test]
    fn reorder_swaps_bodies_byte_surgically() {
        // Swap alpha_monarchy and generic_fallback bodies (two SetBlocks), flipping
        // precedence. The rest of the file is untouched.
        let (_root, vfs) = fixture("reorder");
        let loc = crate::loc::LocStore::from_pairs(&[]);
        let data = load(&vfs, &loc);
        let inner = |raw: &str| {
            let s = raw.find('{').unwrap() + 1;
            let e = raw.rfind('}').unwrap();
            raw[s..e].to_string()
        };
        let a_inner = inner(&data.schemes[0].raw);
        let b_inner = inner(&data.schemes[1].raw);
        let out = apply(
            SRC.as_bytes(),
            &Edit::SetBlock {
                path: vec!["alpha_monarchy".into()],
                value: b_inner.clone(),
            },
        )
        .unwrap();
        let out = apply(
            &out,
            &Edit::SetBlock {
                path: vec!["generic_fallback".into()],
                value: a_inner.clone(),
            },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        // The generic (G_) content now sits under alpha_monarchy (first),
        // and the alpha (A_) content under generic_fallback (second).
        let alpha_pos = text.find("alpha_monarchy").unwrap();
        let generic_pos = text.find("generic_fallback").unwrap();
        let g_duchy = text.find("1 = G_DUCHY").unwrap();
        let a_duchy = text.find("1 = A_DUCHY").unwrap();
        assert!(alpha_pos < g_duchy && g_duchy < generic_pos, "G content under first key");
        assert!(generic_pos < a_duchy, "A content under second key");
        // Swapping back restores the original content order.
        let back = apply(
            SRC.as_bytes(),
            &Edit::SetBlock {
                path: vec!["alpha_monarchy".into()],
                value: a_inner.clone(),
            },
        )
        .unwrap();
        assert!(String::from_utf8(back).unwrap().contains("1 = A_DUCHY"));
    }

    #[test]
    fn scaffold_parses_with_required_keys() {
        let s = scaffold("my_scheme");
        // 5 roles × 3 ranks = 15 cells, each with a generated loc key + default.
        assert_eq!(s.cells.len(), 15);
        assert_eq!(s.file, PROJECT_FILE);
        assert_eq!(cell_loc_key("my_scheme", "rank", 2), "MY_SCHEME_RANK_2");
        let block = paradox::parse(&s.text);
        let e = block.get_block("my_scheme").expect("scaffold parses as a block");
        for role in ROLES {
            let rb = e.get_block(role).unwrap_or_else(|| panic!("missing {role}"));
            assert_eq!(rb.get_scalar("1").map(str::trim), Some(cell_loc_key("my_scheme", role, 1).as_str()));
            assert!(rb.get_scalar("3").is_some());
        }
        assert!(e.get_block("trigger").is_some(), "has a trigger block");
        // The rank cell defaults are the tier names.
        let rank2 = s.cells.iter().find(|c| c.role == "rank" && c.rank == 2).unwrap();
        assert_eq!(rank2.resolved, "Kingdom");
    }

    #[test]
    fn scaffold_create_then_delete_is_identity() {
        let base = "existing = {\n\trank = { 1 = X }\n}\n";
        let s = scaffold("brand_new");
        let appended = apply(base.as_bytes(), &Edit::Append { text: s.text }).unwrap();
        let text = String::from_utf8(appended.clone()).unwrap();
        assert!(text.contains("brand_new = {"));
        let deleted = apply(
            &appended,
            &Edit::RemoveStatement {
                block_path: vec![],
                key: "brand_new".into(),
                value: None,
            },
        )
        .unwrap();
        assert_eq!(deleted, base.as_bytes(), "create then delete restores the source");
    }

    // --- preview algorithm (synthetic) ---------------------------------------

    #[test]
    fn preview_skips_unevaluable_and_marks_approximate() {
        // A world where FRA is french monarchy. Scheme order: an unevaluable
        // has_reform-gated scheme (Unknown → skipped, approximate), then the
        // french-gated scheme (Yes → match). Mirrors the vanilla russian case.
        let src = "\
reform_gated = {\n\
\trank = { 1 = R_DUCHY 2 = R_KINGDOM 3 = R_EMPIRE }\n\
\truler_male = { 1 = R_DUKE 2 = R_KING 3 = R_EMPEROR }\n\
\ttrigger = { has_reform = some_reform }\n\
}\n\
french_monarchy = {\n\
\trank = { 1 = F_DUCHY 2 = F_KINGDOM 3 = F_EMPIRE }\n\
\truler_male = { 1 = F_DUKE 2 = F_KING 3 = F_EMPEROR }\n\
\ttrigger = { government = monarchy primary_culture = french }\n\
}\n";
        let (root, _v) = synthetic(
            "preview_algo",
            &[
                ("map/provinces.bmp", "x"),
                ("common/government_names/00_gov.txt", src),
            ],
        );
        // Register FRA in country_tags so build_snapshot enumerates it.
        std::fs::create_dir_all(root.join("common/country_tags")).unwrap();
        std::fs::write(
            root.join("common/country_tags/00.txt"),
            "FRA = \"countries/France.txt\"\n",
        )
        .unwrap();
        std::fs::create_dir_all(root.join("common/countries")).unwrap();
        std::fs::write(root.join("common/countries/France.txt"), "color = { 1 2 3 }\n").unwrap();
        // A minimal country history so build_snapshot derives FRA's fields.
        std::fs::create_dir_all(root.join("history/countries")).unwrap();
        std::fs::write(
            root.join("history/countries/FRA - France.txt"),
            "government = monarchy\nprimary_culture = french\ngovernment_rank = 2\ncapital = 183\n",
        )
        .unwrap();
        // Province 183 owned by FRA so it "exists".
        std::fs::create_dir_all(root.join("history/provinces")).unwrap();
        std::fs::write(
            root.join("history/provinces/183 - Paris.txt"),
            "owner = FRA\ncontroller = FRA\nculture = french\nreligion = catholic\n",
        )
        .unwrap();
        // Culture group so culture_group triggers (unused here) don't panic.
        std::fs::create_dir_all(root.join("common/cultures")).unwrap();
        std::fs::write(
            root.join("common/cultures/00_cultures.txt"),
            "french_group = {\n\tfrench = {\n\t}\n}\n",
        )
        .unwrap();

        let vfs = Vfs::new(root.to_str().unwrap(), None).unwrap();
        let loc = crate::loc::LocStore::from_pairs(&[
            ("F_KINGDOM", "Kingdom of France"),
            ("F_KING", "King"),
        ]);
        let p = preview(&vfs, &loc, "FRA", crate::date::DEFAULT_START);
        assert_eq!(p.rank, 2);
        assert_eq!(p.matched_key.as_deref(), Some("french_monarchy"));
        assert_eq!(p.country_name.as_deref(), Some("Kingdom of France"));
        assert_eq!(p.ruler_name.as_deref(), Some("King"));
        assert!(p.approximate, "reform_gated was skipped as unevaluable");
        assert!(p.skipped.contains(&"reform_gated".to_string()));
    }

    // --- real install + Anbennar ---------------------------------------------

    #[test]
    fn vanilla_muscovy_preview_matches_hand_derivation() {
        if !Path::new(INSTALL).join("map/provinces.bmp").is_file() {
            return;
        }
        let vfs = Vfs::new(INSTALL, None).unwrap();
        let loc = crate::loc::store(&vfs, INSTALL, None);
        let p = preview(&vfs, &loc, "MOS", crate::date::DEFAULT_START);
        // Muscovy at 1444: government_rank = 1, russian (east_slavic) monarchy.
        assert_eq!(p.rank, 1, "MOS is a rank-1 principality at 1444");
        // russian_monarchy (has_reform-gated → the evaluator can't decide) is
        // skipped as unevaluable; russian_feudal_monarchy (culture-gated, fully
        // modeled) is the first hard match → approximate.
        assert_eq!(p.matched_key.as_deref(), Some("russian_feudal_monarchy"));
        assert!(p.approximate, "an earlier has_reform-gated scheme was skipped");
        assert!(p.skipped.contains(&"russian_monarchy".to_string()));
        // Rank-1 cells of russian_feudal_monarchy: PRINCEDOM / KNIAZ.
        assert_eq!(p.country_name.as_deref(), Some(loc.resolve("PRINCEDOM").as_str()));
        assert_eq!(p.ruler_name.as_deref(), Some(loc.resolve("KNIAZ").as_str()));
        println!(
            "[govnames:MOS] rank {} → {:?} / {:?} (approx {}, skipped {})",
            p.rank, p.country_name, p.ruler_name, p.approximate, p.skipped.len()
        );
    }

    #[test]
    fn vanilla_loads_all_schemes() {
        if !Path::new(INSTALL).join("map/provinces.bmp").is_file() {
            return;
        }
        let vfs = Vfs::new(INSTALL, None).unwrap();
        let loc = crate::loc::store(&vfs, INSTALL, None);
        let data = load(&vfs, &loc);
        assert!(data.schemes.len() > 100, "only {} schemes", data.schemes.len());
        let rus = data.schemes.iter().find(|s| s.key == "russian_feudal_monarchy").unwrap();
        assert!(rus.has_trigger);
        assert_eq!(
            rus.cells.iter().find(|c| c.role == "rank" && c.rank == 3).unwrap().loc_key,
            "EMPIRE"
        );
        assert!(data.schemes.iter().all(|s| s.origin == "base"));
    }

    #[test]
    fn anbennar_government_names_smoke() {
        if !Path::new(INSTALL).join("map/provinces.bmp").is_file() || !Path::new(ANBENNAR).is_dir() {
            return;
        }
        let vfs = Vfs::new(INSTALL, Some(ANBENNAR)).unwrap();
        let loc = crate::loc::store(&vfs, INSTALL, Some(ANBENNAR));
        let data = load(&vfs, &loc);
        assert!(!data.schemes.is_empty(), "Anbennar should have government_names");
        // Its Magocracy-style schemes parse.
        assert!(
            data.schemes.iter().any(|s| s.key.contains("magocracy")),
            "expected a magocracy scheme"
        );
        // Some schemes come from the mod layer (origin badge works).
        assert!(data.schemes.iter().any(|s| s.origin == "mod"), "expected mod-origin schemes");
        // A preview evaluation runs without panic for a real Anbennar tag (A38 =
        // Anbenncóst), whatever it resolves to.
        let p = preview(&vfs, &loc, "A38", crate::date::DEFAULT_START);
        println!(
            "[govnames:anbennar] {} schemes; A38 → {:?} (rank {}, approx {})",
            data.schemes.len(), p.matched_key, p.rank, p.approximate
        );
    }
}
