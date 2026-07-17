//! Sprint 19 — Colonial Regions & Trade Companies.
//!
//! Two categorical membership map modes sharing ONE implementation: they differ
//! only in the `common/` directory they live in and in which top-level keys are
//! modeled. Both are directories that load additively (many files, mod files add
//! to the base set — unlike the single-canonical map files of geography.rs).
//!
//! # Ground-truth file shapes (verified against the real install)
//!
//! `common/colonial_regions/*.txt`:
//! ```text
//! colonial_alaska = {
//!     color = { 225 225 225 }
//!     tax_income = 0
//!     native_size = 8
//!     native_ferocity = 1
//!     native_hostileness = 4
//!     trade_goods = { fur = 10 fish = 3 … }   # colonization outcome weights
//!     culture     = { inuit = 10 aleutian = 8 }
//!     religion    = { shamanism = 10 }
//!     provinces   = { 979 978 … }
//!     names = { trigger = { … } name = "COLONIAL_ALASKA_Alyeska" }  # repeated
//!     names = { name = "COLONIAL_ALASKA_Aleutia" }                   # no trigger
//! }
//! ```
//! `common/trade_companies/*.txt` is the same skeleton minus the RNW colonization
//! fields (color / provinces / names only).
//!
//! # Membership edit recipes (bare-token, no new mod_writer kinds needed)
//!
//! Province membership lives in the nested `provinces = { <ids> }` list, so the
//! id-list splices work exactly like geography's `[region,"areas"]`:
//!   * add:   `AddId    { file, list_path: [key, "provinces"], id: prov }`
//!   * remove:`RemoveId { file, list_path: [key, "provinces"], id: prov }`
//!   * steal: `ListMove` both paths `[key, "provinces"]`.
//! Create: `AppendText`/`CreateFile` a scaffold block into a project `zz_` file +
//! a `LocOverride` for the name loc key. Delete: `RemoveStatement` the whole
//! block (its provinces become unassigned).
//!
//! Naming rules (`names = { trigger + name }`, first match in file order wins —
//! verified against the file's own "# Specific" / "# Generic" ordering comments),
//! weight-table rows, and the native/tax scalars are all edited by the frontend
//! panel through the existing typed-edit vocabulary (`InsertStatement` /
//! `RemoveStatement` / `SetScalar` / `SetBlock`, occurrence-qualified as needed).

use crate::loc::LocStore;
use crate::map_renderer::hash_color;
use crate::paradox::{self, Block, Value};
use crate::vfs::Vfs;

pub const COLONIAL_REGIONS_DIR: &str = "common/colonial_regions";
pub const TRADE_COMPANIES_DIR: &str = "common/trade_companies";
/// Project-owned files new entries scaffold into (additive; never shadows a
/// vanilla file — the directories load every file, so a fresh file just adds).
pub const COLONIAL_REGIONS_PROJECT_FILE: &str =
    "common/colonial_regions/zz_eutoolkit_colonial_regions.txt";
pub const TRADE_COMPANIES_PROJECT_FILE: &str =
    "common/trade_companies/zz_eutoolkit_trade_companies.txt";

/// Modeled top-level keys of a colonial region (anything else → `raw_extra`).
const COLONIAL_KNOWN_KEYS: &[&str] = &[
    "color",
    "tax_income",
    "native_size",
    "native_ferocity",
    "native_hostileness",
    "trade_goods",
    "culture",
    "religion",
    "provinces",
    "names",
];
/// Modeled top-level keys of a trade company.
const TRADE_COMPANY_KNOWN_KEYS: &[&str] = &["color", "provinces", "names"];

/// Resolves a mode id to its directory + whether it carries the colonial-region
/// weight tables / RNW scalars.
pub fn kind_info(kind: &str) -> Option<(&'static str, &'static str, bool)> {
    match kind {
        "colonial_regions" => Some((COLONIAL_REGIONS_DIR, COLONIAL_REGIONS_PROJECT_FILE, true)),
        "trade_companies" => Some((TRADE_COMPANIES_DIR, TRADE_COMPANIES_PROJECT_FILE, false)),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Payload types (serialize snake_case; mirrored by src/lib/colonial.ts).
// ---------------------------------------------------------------------------

#[derive(serde::Serialize, Clone, Debug)]
pub struct ColonialData {
    /// "colonial_regions" | "trade_companies".
    pub kind: String,
    /// The directory entries live in (e.g. `common/colonial_regions`).
    pub dir: String,
    /// The project file new entries scaffold into.
    pub project_file: String,
    /// True for colonial regions (weight tables + native/tax steppers exist).
    pub has_weight_tables: bool,
    pub entries: Vec<ColonialEntry>,
}

#[derive(serde::Serialize, Clone, Debug)]
pub struct ColonialEntry {
    pub key: String,
    /// Resolved display name — the FIRST naming rule's loc, else the prettified
    /// key. (Colonial entries have no single `name = …`; they carry `names`
    /// rules; the panel edits each rule's loc.)
    pub name: String,
    /// Explicit `color = { r g b }`, else a stable hash color (kept in sync with
    /// the renderer / mode_data, which use the same fallback).
    pub color: [u8; 3],
    /// Whether an explicit `color` block exists (governs the swatch editor's
    /// set-vs-insert edit).
    pub has_color: bool,
    pub provinces: Vec<u32>,
    /// The `names = { trigger + name }` rules in file order (== match order).
    pub names: Vec<NamingRule>,
    // --- colonial-region only (None / empty for trade companies) -----------
    pub tax_income: Option<f64>,
    pub native_size: Option<f64>,
    pub native_ferocity: Option<f64>,
    pub native_hostileness: Option<f64>,
    pub trade_goods: Vec<WeightRow>,
    pub culture: Vec<WeightRow>,
    pub religion: Vec<WeightRow>,
    /// Unmodeled top-level keys (preserve-unknown; shown read-only).
    pub raw_extra: Vec<String>,
    pub source_file: String,
}

/// One `names = { [trigger = { … }] name = "LOC_KEY" }` rule.
#[derive(serde::Serialize, Clone, Debug)]
pub struct NamingRule {
    /// 0-based occurrence of this `names` block within the entry — the byte
    /// address suffix (`names#index`) for trigger / reorder / delete edits.
    pub index: usize,
    /// The `name = "LOC_KEY"` loc key (or "" when malformed).
    pub name_key: String,
    /// The resolved display string for `name_key` (falls back to the key).
    pub name: String,
    /// Whether a `trigger = { … }` sub-block is present.
    pub has_trigger: bool,
    /// The braces-inclusive raw text of this `names` block — used by the reorder
    /// swap (two `SetBlock`s exchanging bodies keeps every other byte identical).
    pub raw: String,
}

/// One `key = weight` row of a colonization outcome table (trade_goods / culture
/// / religion). `weight` is the number as written (integers keep no decimal).
#[derive(serde::Serialize, Clone, Debug)]
pub struct WeightRow {
    pub key: String,
    pub weight: f64,
}

// ---------------------------------------------------------------------------
// Parse.
// ---------------------------------------------------------------------------

fn num(b: &Block, key: &str) -> Option<f64> {
    b.get_scalar(key).and_then(|s| s.trim().parse::<f64>().ok())
}

/// Weight rows of a `key = { <k> = <w> … }` sub-block, in file order.
fn weight_rows(b: &Block, key: &str) -> Vec<WeightRow> {
    let Some(tbl) = b.get_block(key) else {
        return Vec::new();
    };
    tbl.items
        .iter()
        .filter_map(|(k, v)| match (k, v) {
            (Some(k), Value::Scalar(s)) => s
                .trim()
                .parse::<f64>()
                .ok()
                .map(|w| WeightRow { key: k.clone(), weight: w }),
            _ => None,
        })
        .collect()
}

/// Reads every entry of a colonial-regions / trade-companies directory through
/// the Vfs (mod files add to / shadow base files by name; a replace_path on the
/// folder hides the base entirely — `list_dir` already honors all of that).
pub fn load(vfs: &Vfs, loc: &LocStore, kind: &str) -> Result<ColonialData, String> {
    let (dir, project_file, has_weight_tables) =
        kind_info(kind).ok_or_else(|| format!("Unknown colonial kind: {kind}"))?;

    let mut entries: Vec<ColonialEntry> = Vec::new();
    for (name, _path) in vfs.list_dir(dir) {
        if !name.to_lowercase().ends_with(".txt") {
            continue;
        }
        let rel = format!("{dir}/{name}");
        let Ok(bytes) = vfs.read(&rel) else { continue };
        let text = String::from_utf8_lossy(&bytes);
        let block = paradox::parse(&text);
        for (key, b) in block.key_blocks() {
            entries.push(parse_entry(
                &bytes,
                key,
                b,
                loc,
                &rel,
                has_weight_tables,
            ));
        }
    }

    Ok(ColonialData {
        kind: kind.to_string(),
        dir: dir.to_string(),
        project_file: project_file.to_string(),
        has_weight_tables,
        entries,
    })
}

fn parse_entry(
    file_bytes: &[u8],
    key: &str,
    b: &Block,
    loc: &LocStore,
    source_file: &str,
    has_weight_tables: bool,
) -> ColonialEntry {
    let explicit = b.get_block("color").and_then(paradox::color_from_block);
    let color = explicit.unwrap_or_else(|| hash_color(key));

    // Naming rules, in file order, with their occurrence index + raw span.
    let mut names: Vec<NamingRule> = Vec::new();
    let mut occ = 0usize;
    for (k, v) in &b.items {
        if let (Some(k), Value::Block(nb)) = (k.as_deref(), v) {
            if k == "names" {
                let name_key = nb.get_scalar("name").unwrap_or("").to_string();
                let raw = crate::mod_writer::block_span(
                    file_bytes,
                    &[key.to_string(), format!("names#{occ}")],
                )
                .map(|(s, e)| String::from_utf8_lossy(&file_bytes[s..e]).into_owned())
                .unwrap_or_default();
                names.push(NamingRule {
                    index: occ,
                    name: if name_key.is_empty() {
                        String::new()
                    } else {
                        loc.resolve(&name_key)
                    },
                    name_key,
                    has_trigger: nb.get_block("trigger").is_some(),
                    raw,
                });
                occ += 1;
            }
        }
    }

    // Display name: first rule's loc, else prettified key.
    let name = names
        .iter()
        .find(|r| !r.name.is_empty())
        .map(|r| r.name.clone())
        .unwrap_or_else(|| loc.resolve(key));

    let known: &[&str] = if has_weight_tables {
        COLONIAL_KNOWN_KEYS
    } else {
        TRADE_COMPANY_KNOWN_KEYS
    };
    let mut raw_extra: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (k, _) in &b.items {
        if let Some(k) = k.as_deref() {
            if !known.contains(&k) && seen.insert(k.to_string()) {
                raw_extra.push(k.to_string());
            }
        }
    }

    let provinces = b.get_block("provinces").map(|p| p.bare_ids()).unwrap_or_default();

    ColonialEntry {
        key: key.to_string(),
        name,
        color,
        has_color: explicit.is_some(),
        provinces,
        names,
        tax_income: if has_weight_tables { num(b, "tax_income") } else { None },
        native_size: if has_weight_tables { num(b, "native_size") } else { None },
        native_ferocity: if has_weight_tables { num(b, "native_ferocity") } else { None },
        native_hostileness: if has_weight_tables { num(b, "native_hostileness") } else { None },
        trade_goods: if has_weight_tables { weight_rows(b, "trade_goods") } else { Vec::new() },
        culture: if has_weight_tables { weight_rows(b, "culture") } else { Vec::new() },
        religion: if has_weight_tables { weight_rows(b, "religion") } else { Vec::new() },
        raw_extra,
        source_file: source_file.to_string(),
    }
}

/// Light `(key, color, province ids)` list for the renderer + mode_data (no loc,
/// no naming rules). Uses the explicit color, else a stable hash — the renderer
/// and mode_data both call this so their colors agree.
pub fn membership(vfs: &Vfs, kind: &str) -> Vec<(String, [u8; 3], Vec<u32>)> {
    let Some((dir, _, _)) = kind_info(kind) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (name, _path) in vfs.list_dir(dir) {
        if !name.to_lowercase().ends_with(".txt") {
            continue;
        }
        let rel = format!("{dir}/{name}");
        let Ok(bytes) = vfs.read(&rel) else { continue };
        let block = paradox::parse(&String::from_utf8_lossy(&bytes));
        for (key, b) in block.key_blocks() {
            let color = b
                .get_block("color")
                .and_then(paradox::color_from_block)
                .unwrap_or_else(|| hash_color(key));
            let ids = b.get_block("provinces").map(|p| p.bare_ids()).unwrap_or_default();
            out.push((key.to_string(), color, ids));
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Scaffolds (single source of truth; unit-tested to parse back with the keys
// the game requires — zero-manual-fixes bar).
// ---------------------------------------------------------------------------

/// A brand-new colonial region / trade company block. Authored at column 0.
/// Includes the full vanilla shape so it loads with zero manual fixes; the RNW
/// weight tables ship empty (they only feed random-new-world colonization).
pub fn scaffold(kind: &str, key: &str, provinces: &[u32], name_key: &str) -> String {
    let ids = provinces
        .iter()
        .map(|id| id.to_string())
        .collect::<Vec<_>>()
        .join(" ");
    let has_weight_tables = kind_info(kind).map(|(_, _, w)| w).unwrap_or(true);
    if has_weight_tables {
        format!(
            "{key} = {{\n\
             \tcolor = {{ 150 150 150 }}\n\n\
             \ttax_income = 0\n\
             \tnative_size = 0\n\
             \tnative_ferocity = 0\n\
             \tnative_hostileness = 0\n\n\
             \ttrade_goods = {{\n\t}}\n\n\
             \tculture = {{\n\t}}\n\n\
             \treligion = {{\n\t}}\n\n\
             \tprovinces = {{\n\t\t{ids}\n\t}}\n\n\
             \tnames = {{\n\t\tname = \"{name_key}\"\n\t}}\n\
             }}"
        )
    } else {
        format!(
            "{key} = {{\n\
             \tcolor = {{ 150 150 150 }}\n\n\
             \tprovinces = {{\n\t\t{ids}\n\t}}\n\n\
             \tnames = {{\n\t\tname = \"{name_key}\"\n\t}}\n\
             }}"
        )
    }
}

// ---------------------------------------------------------------------------
// Commands (registered in lib.rs).
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_colonial_data(
    kind: String,
    install_path: String,
    mod_path: Option<String>,
) -> Result<ColonialData, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    let loc = crate::loc::store(&vfs, &install_path, mod_path.as_deref());
    load(&vfs, &loc, &kind)
}

#[tauri::command]
pub fn scaffold_colonial_block(
    kind: String,
    key: String,
    provinces: Vec<u32>,
    name_key: String,
) -> String {
    scaffold(&kind, &key, &provinces, &name_key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mod_writer::{apply, Edit};
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

    fn synthetic(name: &str, files: &[(&str, &str)]) -> (std::path::PathBuf, Vfs) {
        let root = std::env::temp_dir().join(format!("eu_toolkit_colonial_test_{name}"));
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

    // Two colonial regions + one trade company fixture. Region A has explicit
    // color, weight tables, native/tax scalars, and three naming rules (specific
    // trigger, tag trigger, generic). Region B shares province 5 with A (overlap).
    const CR_SRC: &str = "colonial_a = {\n\
        \tcolor = { 10 20 30 }\n\n\
        \ttax_income = 2\n\
        \tnative_size = 8\n\
        \tnative_ferocity = 1\n\
        \tnative_hostileness = 4\n\n\
        \ttrade_goods = {\n\t\tfur = 10\n\t\tfish = 3\n\t}\n\n\
        \tculture = {\n\t\tinuit = 10\n\t}\n\n\
        \treligion = {\n\t\tshamanism = 10\n\t}\n\n\
        \tprovinces = {\n\t\t1 2 3 5\n\t}\n\n\
        \tnames = {\n\t\ttrigger = {\n\t\t\tprimary_culture = russian\n\t\t}\n\t\tname = \"COL_A_Alyeska\"\n\t}\n\
        \tnames = {\n\t\ttrigger = {\n\t\t\ttag = SPA\n\t\t}\n\t\tname = \"COL_A_Pacifico\"\n\t}\n\
        \tnames = {\n\t\tname = \"COL_A_Generic\"\n\t}\n\
        }\n\
        colonial_b = {\n\
        \tcolor = { 200 0 0 }\n\
        \tprovinces = {\n\t\t5 6 7\n\t}\n\
        \tnames = {\n\t\tname = \"COL_B_Name\"\n\t}\n\
        }\n";

    const TC_SRC: &str = "trade_company_x = {\n\
        \tcolor = { 50 0 200 }\n\
        \tprovinces = {\n\t\t100 101 102\n\t}\n\
        \tnames = {\n\t\tname = \"TC_X_Name\"\n\t}\n\
        }\n";

    fn fixture(name: &str) -> (std::path::PathBuf, Vfs) {
        synthetic(
            name,
            &[
                ("common/colonial_regions/00_colonial.txt", CR_SRC),
                ("common/trade_companies/00_tc.txt", TC_SRC),
            ],
        )
    }

    #[test]
    fn parses_colonial_regions() {
        let (_root, vfs) = fixture("parse_cr");
        let loc = crate::loc::build(&vfs);
        let data = load(&vfs, &loc, "colonial_regions").unwrap();
        assert!(data.has_weight_tables);
        assert_eq!(data.entries.len(), 2);
        let a = data.entries.iter().find(|e| e.key == "colonial_a").unwrap();
        assert_eq!(a.color, [10, 20, 30]);
        assert!(a.has_color);
        assert_eq!(a.provinces, vec![1, 2, 3, 5]);
        assert_eq!(a.tax_income, Some(2.0));
        assert_eq!(a.native_size, Some(8.0));
        assert_eq!(a.trade_goods.len(), 2);
        assert_eq!(a.trade_goods[0].key, "fur");
        assert_eq!(a.trade_goods[0].weight, 10.0);
        assert_eq!(a.culture[0].key, "inuit");
        assert_eq!(a.religion[0].key, "shamanism");
        assert_eq!(a.names.len(), 3);
        assert_eq!(a.names[0].name_key, "COL_A_Alyeska");
        assert!(a.names[0].has_trigger);
        assert!(!a.names[2].has_trigger);
        assert_eq!(a.names[2].name_key, "COL_A_Generic");
        // Raw spans reach the right block (occurrence-addressed).
        assert!(a.names[0].raw.contains("primary_culture = russian"));
        assert!(a.names[1].raw.contains("tag = SPA"));
    }

    #[test]
    fn parses_trade_companies_without_weight_tables() {
        let (_root, vfs) = fixture("parse_tc");
        let loc = crate::loc::build(&vfs);
        let data = load(&vfs, &loc, "trade_companies").unwrap();
        assert!(!data.has_weight_tables);
        let x = &data.entries[0];
        assert_eq!(x.key, "trade_company_x");
        assert_eq!(x.color, [50, 0, 200]);
        assert_eq!(x.provinces, vec![100, 101, 102]);
        assert!(x.tax_income.is_none());
        assert!(x.trade_goods.is_empty());
        assert_eq!(x.names[0].name_key, "TC_X_Name");
    }

    #[test]
    fn membership_matches_render_colors() {
        let (_root, vfs) = fixture("membership");
        let m = membership(&vfs, "colonial_regions");
        let a = m.iter().find(|(k, _, _)| k == "colonial_a").unwrap();
        assert_eq!(a.1, [10, 20, 30]);
        assert_eq!(a.2, vec![1, 2, 3, 5]);
    }

    // --- membership add/remove/steal round-trips (byte-surgical) ----------

    #[test]
    fn province_add_remove_only_touches_list() {
        // Add province 9 to colonial_a, then remove it — restores exact bytes.
        let out = apply(
            CR_SRC.as_bytes(),
            &Edit::AddId {
                list_path: vec!["colonial_a".into(), "provinces".into()],
                id: "9".into(),
            },
        )
        .unwrap();
        let text = String::from_utf8(out.clone()).unwrap();
        assert!(text.contains("1 2 3 5\n\t\t9") || text.contains("1 2 3 5 9"), "9 added: {text}");
        // color + weight tables + names untouched.
        assert!(text.contains("color = { 10 20 30 }"));
        assert!(text.contains("trigger = {\n\t\t\tprimary_culture = russian\n\t\t}"));
        let back = apply(
            &out,
            &Edit::RemoveId {
                list_path: vec!["colonial_a".into(), "provinces".into()],
                id: "9".into(),
            },
        )
        .unwrap();
        assert_eq!(back, CR_SRC.as_bytes(), "add then remove is identity");
    }

    #[test]
    fn province_steals_between_regions() {
        // Move province 6 from colonial_b to colonial_a.
        let out = apply(
            CR_SRC.as_bytes(),
            &Edit::RemoveId {
                list_path: vec!["colonial_b".into(), "provinces".into()],
                id: "6".into(),
            },
        )
        .unwrap();
        let out = apply(
            &out,
            &Edit::AddId {
                list_path: vec!["colonial_a".into(), "provinces".into()],
                id: "6".into(),
            },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("colonial_b = {\n\tcolor = { 200 0 0 }\n\tprovinces = {\n\t\t5 7\n\t}"), "6 left b: {text}");
        assert!(text.contains("1 2 3 5\n\t\t6") || text.contains("1 2 3 5 6"), "6 joined a: {text}");
    }

    // --- naming-rule add / edit-loc / reorder / delete --------------------

    #[test]
    fn naming_rule_add_then_remove_is_identity() {
        let stmt = "names = {\n\tname = \"COL_A_New\"\n}";
        let added = apply(
            CR_SRC.as_bytes(),
            &Edit::InsertStatement {
                block_path: vec!["colonial_a".into()],
                statement: stmt.into(),
            },
        )
        .unwrap();
        let text = String::from_utf8(added.clone()).unwrap();
        let count = paradox::parse(&text)
            .get_block("colonial_a")
            .unwrap()
            .items
            .iter()
            .filter(|(k, _)| k.as_deref() == Some("names"))
            .count();
        assert_eq!(count, 4, "fourth names added: {text}");
        // Occurrence #3 removal restores the original bytes.
        let removed = apply(
            &added,
            &Edit::RemoveStatement {
                block_path: vec!["colonial_a".into()],
                key: "names#3".into(),
                value: None,
            },
        )
        .unwrap();
        assert_eq!(removed, CR_SRC.as_bytes(), "add then remove is identity");
    }

    #[test]
    fn naming_rule_delete_middle_keeps_others() {
        // Remove the second rule (tag = SPA); the specific + generic rules stay.
        let out = apply(
            CR_SRC.as_bytes(),
            &Edit::RemoveStatement {
                block_path: vec!["colonial_a".into()],
                key: "names#1".into(),
                value: None,
            },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(!text.contains("tag = SPA"), "second rule removed: {text}");
        assert!(text.contains("COL_A_Alyeska"), "first rule intact");
        assert!(text.contains("COL_A_Generic"), "third rule intact");
    }

    #[test]
    fn naming_rule_reorder_swaps_bodies_byte_surgically() {
        // Swap rules #0 and #1 by exchanging their block bodies (two SetBlocks).
        // The frontend sends each block's inner text (braces stripped).
        let inner = |raw: &str| {
            let s = raw.find('{').unwrap() + 1;
            let e = raw.rfind('}').unwrap();
            raw[s..e].to_string()
        };
        let loc = crate::loc::LocStore::from_pairs(&[]);
        let (_root, vfs) = fixture("reorder");
        let data = load(&vfs, &loc, "colonial_regions").unwrap();
        let a = data.entries.iter().find(|e| e.key == "colonial_a").unwrap();
        let body0 = inner(&a.names[0].raw);
        let body1 = inner(&a.names[1].raw);
        let out = apply(
            CR_SRC.as_bytes(),
            &Edit::SetBlock {
                path: vec!["colonial_a".into(), "names#0".into()],
                value: body1.clone(),
            },
        )
        .unwrap();
        let out = apply(
            &out,
            &Edit::SetBlock {
                path: vec!["colonial_a".into(), "names#1".into()],
                value: body0.clone(),
            },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        let a_block = text.split("colonial_a").nth(1).unwrap();
        let first = a_block.find("tag = SPA").unwrap();
        let second = a_block.find("primary_culture = russian").unwrap();
        assert!(first < second, "SPA rule now precedes russian rule: {text}");
        // colonial_b untouched.
        assert!(text.contains("colonial_b = {\n\tcolor = { 200 0 0 }"));
    }

    // --- weight-table edit round-trip -------------------------------------

    #[test]
    fn weight_edit_add_remove() {
        // Set fur weight, add a new good, remove fish — all byte-surgical.
        let out = apply(
            CR_SRC.as_bytes(),
            &Edit::SetScalar {
                path: vec!["colonial_a".into(), "trade_goods".into(), "fur".into()],
                value: "20".into(),
                quoted: false,
            },
        )
        .unwrap();
        let out = apply(
            &out,
            &Edit::InsertStatement {
                block_path: vec!["colonial_a".into(), "trade_goods".into()],
                statement: "cloth = 5".into(),
            },
        )
        .unwrap();
        let out = apply(
            &out,
            &Edit::RemoveStatement {
                block_path: vec!["colonial_a".into(), "trade_goods".into()],
                key: "fish".into(),
                value: None,
            },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("fur = 20"));
        assert!(text.contains("cloth = 5"));
        assert!(!text.contains("fish = 3"));
        // culture / religion tables untouched.
        assert!(text.contains("culture = {\n\t\tinuit = 10\n\t}"));
    }

    #[test]
    fn native_and_tax_steppers() {
        let out = apply(
            CR_SRC.as_bytes(),
            &Edit::SetScalar {
                path: vec!["colonial_a".into(), "tax_income".into()],
                value: "7".into(),
                quoted: false,
            },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("tax_income = 7"));
        assert!(text.contains("native_size = 8"), "other scalars intact");
    }

    // --- scaffold parses back + has required keys -------------------------

    #[test]
    fn scaffold_colonial_region_is_loadable() {
        let s = scaffold("colonial_regions", "colonial_new", &[42, 43], "COLONIAL_NEW_Name");
        let block = paradox::parse(&s);
        let e = block.get_block("colonial_new").expect("parses as a block");
        assert!(e.get_block("color").is_some(), "has color");
        assert_eq!(e.get_block("provinces").unwrap().bare_ids(), vec![42, 43]);
        assert!(e.get_block("trade_goods").is_some());
        assert!(e.get_block("culture").is_some());
        assert!(e.get_block("religion").is_some());
        let names = e.items.iter().filter(|(k, _)| k.as_deref() == Some("names")).count();
        assert_eq!(names, 1, "has one naming rule");
    }

    #[test]
    fn scaffold_trade_company_is_loadable() {
        let s = scaffold("trade_companies", "tc_new", &[7], "TC_NEW_Name");
        let block = paradox::parse(&s);
        let e = block.get_block("tc_new").expect("parses as a block");
        assert!(e.get_block("color").is_some());
        assert_eq!(e.get_block("provinces").unwrap().bare_ids(), vec![7]);
        assert!(e.get_block("trade_goods").is_none(), "no RNW tables");
    }

    #[test]
    fn scaffold_create_then_delete_is_identity() {
        let s = scaffold("colonial_regions", "colonial_new", &[42], "COLONIAL_NEW_Name");
        let appended = apply(CR_SRC.as_bytes(), &Edit::Append { text: s }).unwrap();
        let text = String::from_utf8(appended.clone()).unwrap();
        assert!(text.contains("colonial_new = {"));
        let deleted = apply(
            &appended,
            &Edit::RemoveStatement {
                block_path: vec![],
                key: "colonial_new".into(),
                value: None,
            },
        )
        .unwrap();
        // Append guarantees a separating newline; the original had a trailing \n,
        // so delete restores exactly the source.
        assert_eq!(deleted, CR_SRC.as_bytes(), "create then delete is identity");
    }

    // --- real install + Anbennar smoke ------------------------------------

    #[test]
    fn parses_vanilla_colonial_regions() {
        let Some(vfs) = real_install() else { return };
        let loc = crate::loc::store(&vfs, INSTALL, None);
        let data = load(&vfs, &loc, "colonial_regions").unwrap();
        // Vanilla has ~20 colonial regions; every one has provinces + names.
        assert!(data.entries.len() > 15, "only {} regions", data.entries.len());
        let alaska = data.entries.iter().find(|e| e.key == "colonial_alaska").unwrap();
        assert!(!alaska.provinces.is_empty());
        assert!(!alaska.names.is_empty());
        assert!(alaska.has_color);
        assert!(alaska.trade_goods.iter().any(|w| w.key == "fur"));
        // Most entries carry provinces; some ship empty (the zero-province case
        // the validation domain warns about — expedition/placeholder regions).
        let with_prov = data.entries.iter().filter(|e| !e.provinces.is_empty()).count();
        let empty: Vec<&str> = data
            .entries
            .iter()
            .filter(|e| e.provinces.is_empty())
            .map(|e| e.key.as_str())
            .collect();
        println!("[colonial:vanilla] {} entries, {with_prov} with provinces, empty: {empty:?}", data.entries.len());
        // Vanilla ships ~12 real regions + a bank of empty `colonial_placeholder_N`
        // reserved for RNW; the placeholders are exactly the zero-province warning
        // case, so we only assert the real regions parsed.
        assert!(with_prov >= 10, "expected the real colonial regions, got {with_prov}");
        assert!(empty.iter().all(|k| k.starts_with("colonial_placeholder")), "unexpected empty region");

        let tc = load(&vfs, &loc, "trade_companies").unwrap();
        assert!(tc.entries.len() > 5, "only {} trade companies", tc.entries.len());
        assert!(tc.entries.iter().all(|e| !e.names.is_empty()));
    }

    #[test]
    fn anbennar_colonial_regions_smoke() {
        if !Path::new(INSTALL).join("map/provinces.bmp").is_file() || !Path::new(ANBENNAR).is_dir() {
            return;
        }
        let vfs = Vfs::new(INSTALL, Some(ANBENNAR)).unwrap();
        let loc = crate::loc::store(&vfs, INSTALL, Some(ANBENNAR));
        let data = load(&vfs, &loc, "colonial_regions").unwrap();
        println!("[colonial:anbennar] {} colonial regions", data.entries.len());
        // Anbennar's Aelantir colonial regions replace vanilla's; expect entries.
        assert!(!data.entries.is_empty(), "anbennar should have colonial regions");
        assert!(data.entries.iter().all(|e| !e.key.is_empty()));
    }
}
