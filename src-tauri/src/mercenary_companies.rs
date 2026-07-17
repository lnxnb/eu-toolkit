//! Sprint 23.2 — Mercenary companies, province-anchored.
//!
//! `common/mercenary_companies/*.txt` holds one company block per top-level key
//! (`merc_black_army`, `twenty_good_men`, …), anchored to a province via
//! `home_province = <id>` (a company with no `home_province` is a local/global
//! company — not surfaced in the province panel). So the editor lives in the
//! **province panel** (`MercenariesSection`). Verified key inventory (all vanilla
//! files scanned):
//!
//! * Number scalars: `regiments_per_development`, `cost_modifier`,
//!   `cavalry_weight`, `artillery_weight`.
//! * Int scalars: `cavalry_cap`, `artillery_cap`, `min_size`, `max_size`,
//!   `manpower_pool`, `home_province` (the anchor).
//! * Bool scalars: `counts_towards_force_limit`, `no_additional_manpower_from_max_size`.
//! * String: `mercenary_desc_key` (a loc key reference).
//! * `sprites = { … }` — a bare-token list of unit sprite packs, edited as a
//!   space-joined string (setBlock on `[key,"sprites"]`).
//! * `trigger = { … }` — recruitment condition (14.2 tree).
//! * `modifier = { … }` — the company's combat modifier block (typed flat rows).
//! * Everything unmodeled round-trips untouched (`raw_extra`).
//!
//! Loc name is the bare block key (`merc_black_army:0 "Black Army"`).

use crate::loc::{self, LocStore};
use crate::mod_writer;
use crate::paradox::{self, Block, Value};
use crate::vfs::Vfs;

pub const MERC_DIR: &str = "common/mercenary_companies";
pub const MERC_PROJECT_FILE: &str = "common/mercenary_companies/zz_eutoolkit_mercenaries.txt";

#[derive(Clone, Copy)]
enum ScalarKind {
    Num,
    Int,
    Bool,
    Str,
}

impl ScalarKind {
    fn as_str(self) -> &'static str {
        match self {
            ScalarKind::Num => "num",
            ScalarKind::Int => "int",
            ScalarKind::Bool => "bool",
            ScalarKind::Str => "str",
        }
    }
}

struct ScalarSpec {
    key: &'static str,
    kind: ScalarKind,
}

const fn s(key: &'static str, kind: ScalarKind) -> ScalarSpec {
    ScalarSpec { key, kind }
}

static SCALARS: &[ScalarSpec] = &[
    s("home_province", ScalarKind::Int),
    s("regiments_per_development", ScalarKind::Num),
    s("cost_modifier", ScalarKind::Num),
    s("cavalry_weight", ScalarKind::Num),
    s("artillery_weight", ScalarKind::Num),
    s("cavalry_cap", ScalarKind::Int),
    s("artillery_cap", ScalarKind::Int),
    s("min_size", ScalarKind::Int),
    s("max_size", ScalarKind::Int),
    s("manpower_pool", ScalarKind::Int),
    s("counts_towards_force_limit", ScalarKind::Bool),
    s("no_additional_manpower_from_max_size", ScalarKind::Bool),
    s("mercenary_desc_key", ScalarKind::Str),
];

/// Modeled keys (excluded from `raw_extra`).
static MODELED_EXTRA: &[&str] = &["sprites", "trigger", "modifier"];

// ---------------------------------------------------------------------------
// Payload types (serialize camelCase; mirrored by src/lib/mercenaries.ts).
// ---------------------------------------------------------------------------

#[derive(serde::Serialize, Clone, Debug)]
pub struct ModRow {
    pub key: String,
    pub value: String,
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Scalar {
    pub key: String,
    pub kind: String,
    pub present: bool,
    pub value: String,
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ModifierBlockRef {
    pub name: String,
    pub present: bool,
    pub flat: bool,
    pub rows: Vec<ModRow>,
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct MercenaryCompany {
    pub key: String,
    pub file: String,
    pub origin: String,
    pub home_province: u32,
    pub name_key: String,
    pub name_loc: Option<String>,
    pub scalars: Vec<Scalar>,
    /// Space-joined `sprites = { … }` tokens (empty when absent).
    pub sprites: String,
    pub sprites_present: bool,
    /// `trigger` block presence (edited via the 14.2 tree).
    pub trigger_present: bool,
    pub modifier: ModifierBlockRef,
    pub raw_extra: Vec<String>,
    pub raw: String,
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ProvinceMercenaries {
    pub companies: Vec<MercenaryCompany>,
    pub project_file: String,
}

// ---------------------------------------------------------------------------
// Parse.
// ---------------------------------------------------------------------------

fn origin_of(vfs: &Vfs, path: &std::path::Path) -> &'static str {
    if vfs.mod_dir().is_some_and(|m| path.starts_with(m)) {
        "mod"
    } else {
        "base"
    }
}

/// Flat `k = scalar` rows of a block, or `None` if it holds any nested block.
fn flat_rows(b: &Block) -> Option<Vec<ModRow>> {
    let mut rows = Vec::new();
    for (k, v) in &b.items {
        match (k, v) {
            (Some(k), Value::Scalar(s)) => rows.push(ModRow {
                key: k.clone(),
                value: s.trim().to_string(),
            }),
            _ => return None,
        }
    }
    Some(rows)
}

fn parse_company(
    file_bytes: &[u8],
    key: &str,
    b: &Block,
    loc: &LocStore,
    file: &str,
    origin: &str,
) -> MercenaryCompany {
    let home_province = b
        .get_scalar("home_province")
        .and_then(|s| s.trim().parse::<u32>().ok())
        .unwrap_or(0);

    let scalars = SCALARS
        .iter()
        .map(|spec| {
            let val = b.get_scalar(spec.key).map(|s| s.trim().to_string());
            Scalar {
                key: spec.key.to_string(),
                kind: spec.kind.as_str().to_string(),
                present: val.is_some(),
                value: val.unwrap_or_default(),
            }
        })
        .collect();

    let sprites_block = b.get_block("sprites");
    let sprites = sprites_block
        .map(|sb| sb.bare_scalars().collect::<Vec<_>>().join(" "))
        .unwrap_or_default();

    let modifier = match b.get_block("modifier") {
        None => ModifierBlockRef { name: "modifier".into(), present: false, flat: true, rows: Vec::new() },
        Some(mb) => match flat_rows(mb) {
            Some(rows) => ModifierBlockRef { name: "modifier".into(), present: true, flat: true, rows },
            None => ModifierBlockRef { name: "modifier".into(), present: true, flat: false, rows: Vec::new() },
        },
    };

    let mut raw_extra = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let modeled: std::collections::HashSet<&str> =
        SCALARS.iter().map(|s| s.key).chain(MODELED_EXTRA.iter().copied()).collect();
    for (k, _) in &b.items {
        if let Some(k) = k.as_deref() {
            if !modeled.contains(k) && seen.insert(k.to_string()) {
                raw_extra.push(k.to_string());
            }
        }
    }

    let raw = mod_writer::block_span(file_bytes, &[key.to_string()])
        .map(|(s, e)| String::from_utf8_lossy(&file_bytes[s..e]).into_owned())
        .unwrap_or_default();

    MercenaryCompany {
        key: key.to_string(),
        file: file.to_string(),
        origin: origin.to_string(),
        home_province,
        name_loc: loc.get(key).map(str::to_string),
        name_key: key.to_string(),
        scalars,
        sprites,
        sprites_present: sprites_block.is_some(),
        trigger_present: b.get_block("trigger").is_some(),
        modifier,
        raw_extra,
        raw,
    }
}

#[cfg(test)]
pub fn load(vfs: &Vfs, loc: &LocStore) -> Vec<MercenaryCompany> {
    let mut out = Vec::new();
    for (name, path) in vfs.list_dir(MERC_DIR) {
        if !name.to_lowercase().ends_with(".txt") {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let origin = origin_of(vfs, &path);
        let block = paradox::parse(&String::from_utf8_lossy(&bytes));
        let rel = format!("{MERC_DIR}/{name}");
        for (key, b) in block.key_blocks() {
            out.push(parse_company(&bytes, key, b, loc, &rel, origin));
        }
    }
    out
}

/// The mercenary files (disk), plus the project file even when it exists only as
/// a pending `CreateFile`.
fn merc_files(vfs: &Vfs) -> Vec<String> {
    let mut v: Vec<String> = vfs
        .list_dir(MERC_DIR)
        .into_iter()
        .filter(|(n, _)| n.to_lowercase().ends_with(".txt"))
        .map(|(n, _)| format!("{MERC_DIR}/{n}"))
        .collect();
    if !v.iter().any(|f| f == MERC_PROJECT_FILE) {
        v.push(MERC_PROJECT_FILE.to_string());
    }
    v.sort();
    v.dedup();
    v
}

/// Like [`load`], but each file is previewed through the pending edit queue so
/// created/deleted/edited companies show live (survives panel remounts).
pub fn load_with_edits(
    vfs: &Vfs,
    loc: &LocStore,
    edits: &[crate::edits::TypedEdit],
) -> Vec<MercenaryCompany> {
    let mut out = Vec::new();
    for rel in merc_files(vfs) {
        let Ok(bytes) = crate::edits::preview_file(vfs, &rel, edits) else {
            continue;
        };
        if bytes.is_empty() {
            continue;
        }
        let origin = match vfs.resolve(&rel) {
            Some(p) => origin_of(vfs, &p),
            None => "mod",
        };
        let block = paradox::parse(&String::from_utf8_lossy(&bytes));
        for (key, b) in block.key_blocks() {
            out.push(parse_company(&bytes, key, b, loc, &rel, origin));
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Scaffold.
// ---------------------------------------------------------------------------

#[derive(serde::Serialize, Clone, Debug)]
pub struct LocEntry {
    pub key: String,
    pub value: String,
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Scaffold {
    pub key: String,
    pub file: String,
    pub text: String,
    pub loc_entries: Vec<LocEntry>,
}

/// Minimal game-valid mercenary company anchored to `province_id`.
/// `westerngfx_sprite_pack` is a base (non-DLC) sprite pack, so the sprites list
/// resolves without any DLC — meeting the loads-in-game bar.
pub fn scaffold_company(key: &str, province_id: u32) -> Scaffold {
    let text = format!(
        "{key} = {{\n\
\tregiments_per_development = 0.04\n\
\thome_province = {province_id}\n\
\tcavalry_weight = 0.2\n\
\tcavalry_cap = 4\n\
\tcost_modifier = 1.0\n\
\tsprites = {{ westerngfx_sprite_pack }}\n\
\ttrigger = {{\n\t\tis_allowed_to_recruit_mercenaries = yes\n\t}}\n\
\tmodifier = {{\n\t\tland_morale = 0.05\n\t}}\n\
}}"
    );
    let pretty = loc::prettify(key);
    Scaffold {
        key: key.to_string(),
        file: MERC_PROJECT_FILE.to_string(),
        text,
        loc_entries: vec![LocEntry { key: key.to_string(), value: pretty }],
    }
}

// ---------------------------------------------------------------------------
// Commands.
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_province_mercenaries(
    install_path: String,
    mod_path: Option<String>,
    id: u32,
    edits: Option<Vec<crate::edits::TypedEdit>>,
) -> Result<ProvinceMercenaries, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    let loc = loc::store(&vfs, &install_path, mod_path.as_deref());
    let edits = edits.unwrap_or_default();
    let companies = load_with_edits(&vfs, &loc, &edits)
        .into_iter()
        .filter(|c| c.home_province == id)
        .collect();
    Ok(ProvinceMercenaries {
        companies,
        project_file: MERC_PROJECT_FILE.to_string(),
    })
}

#[tauri::command]
pub fn scaffold_mercenary_company(key: String, province_id: u32) -> Result<Scaffold, String> {
    Ok(scaffold_company(&key, province_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mod_writer::{apply, Edit};
    use std::path::Path;

    const INSTALL: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Europa Universalis IV";
    const ANBENNAR: &str = r"C:\Users\User\projects\anbennar-eu4-dev";

    fn install_present() -> bool {
        Path::new(INSTALL).join("map/provinces.bmp").is_file()
    }

    fn synthetic(name: &str, files: &[(&str, &str)]) -> (std::path::PathBuf, Vfs) {
        let root = std::env::temp_dir().join(format!("eu_toolkit_merc_test_{name}"));
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

    const MERC_SRC: &str = "\
merc_black_army = {\n\
\tregiments_per_development = 0.04\n\
\tmercenary_desc_key = FREE_OF_ARMY_PROFESSIONALISM_COST\n\
\tcavalry_weight = 0.2\n\
\tartillery_weight = 0.4\n\
\tcavalry_cap = 4\n\
\thome_province = 153\n\
\tcost_modifier = 1.5\n\
\tsprites = { dlc056_hun_sprite_pack sav_base_sprite_pack westerngfx_sprite_pack }\n\
\ttrigger = {\n\t\tis_allowed_to_recruit_mercenaries = yes\n\t\ttag = HUN\n\t}\n\
\tmodifier = {\n\t\tinfantry_power = 0.05\n\t\tdiscipline = 0.05\n\t}\n\
\tsome_unknown_key = yes\n\
}\n";

    fn merc_fixture(name: &str) -> (std::path::PathBuf, Vfs) {
        synthetic(name, &[("common/mercenary_companies/00_test.txt", MERC_SRC)])
    }

    #[test]
    fn parses_company_scalars_sprites_modifier_and_raw() {
        let (_root, vfs) = merc_fixture("parse");
        let loc = LocStore::from_pairs(&[("merc_black_army", "Black Army")]);
        let all = load(&vfs, &loc);
        assert_eq!(all.len(), 1);
        let c = &all[0];
        assert_eq!(c.key, "merc_black_army");
        assert_eq!(c.home_province, 153);
        assert_eq!(c.name_loc.as_deref(), Some("Black Army"));
        let sc = |k: &str| c.scalars.iter().find(|s| s.key == k).unwrap();
        assert_eq!(sc("cost_modifier").value, "1.5");
        assert_eq!(sc("cost_modifier").kind, "num");
        assert_eq!(sc("cavalry_cap").value, "4");
        assert_eq!(sc("cavalry_cap").kind, "int");
        assert_eq!(sc("mercenary_desc_key").value, "FREE_OF_ARMY_PROFESSIONALISM_COST");
        assert!(!sc("min_size").present); // absent int
        // Sprites collapsed to a space-joined string.
        assert!(c.sprites_present);
        assert_eq!(c.sprites, "dlc056_hun_sprite_pack sav_base_sprite_pack westerngfx_sprite_pack");
        // Modifier block is flat + typed.
        assert!(c.modifier.present && c.modifier.flat);
        assert_eq!(c.modifier.rows.len(), 2);
        assert_eq!(c.modifier.rows[0].key, "infantry_power");
        // Trigger present.
        assert!(c.trigger_present);
        // Preserve-unknown.
        assert!(c.raw_extra.contains(&"some_unknown_key".to_string()));
        assert!(c.raw.starts_with('{') && c.raw.ends_with('}'));
    }

    #[test]
    fn scalar_edit_is_byte_surgical() {
        let out = apply(
            MERC_SRC.as_bytes(),
            &Edit::SetScalar {
                path: vec!["merc_black_army".into(), "cost_modifier".into()],
                value: "0.75".into(),
                quoted: false,
            },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("cost_modifier = 0.75"));
        assert!(text.contains("mercenary_desc_key = FREE_OF_ARMY_PROFESSIONALISM_COST"));
        assert!(text.contains("some_unknown_key = yes"));
    }

    #[test]
    fn modifier_block_edit_round_trips() {
        let out = apply(
            MERC_SRC.as_bytes(),
            &Edit::SetBlock {
                path: vec!["merc_black_army".into(), "modifier".into()],
                value: "infantry_power = 0.1 discipline = 0.05 land_morale = 0.1".into(),
            },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("infantry_power = 0.1 discipline = 0.05 land_morale = 0.1"));
        // trigger untouched.
        assert!(text.contains("is_allowed_to_recruit_mercenaries = yes"));
    }

    #[test]
    fn sprites_edit_round_trips() {
        let out = apply(
            MERC_SRC.as_bytes(),
            &Edit::SetBlock {
                path: vec!["merc_black_army".into(), "sprites".into()],
                value: "westerngfx_sprite_pack".into(),
            },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("sprites = { westerngfx_sprite_pack }"));
    }

    #[test]
    fn scaffold_parses_back_with_required_keys() {
        let s = scaffold_company("merc_my_company", 42);
        assert_eq!(s.file, MERC_PROJECT_FILE);
        let b = paradox::parse(&s.text);
        let c = b.get_block("merc_my_company").expect("scaffold parses");
        assert_eq!(c.get_scalar("home_province"), Some("42"));
        assert_eq!(c.get_scalar("regiments_per_development"), Some("0.04"));
        assert!(c.get_block("sprites").is_some());
        assert!(c.get_block("trigger").is_some());
        assert!(c.get_block("modifier").is_some());
        // Sprite pack is a base (non-DLC) pack.
        assert!(s.text.contains("westerngfx_sprite_pack"));
        let keys: Vec<&str> = s.loc_entries.iter().map(|e| e.key.as_str()).collect();
        assert!(keys.contains(&"merc_my_company"));
    }

    #[test]
    fn scaffold_create_then_delete_is_identity() {
        let base = "existing_merc = {\n\thome_province = 1\n}\n";
        let s = scaffold_company("merc_new", 7);
        let appended = apply(base.as_bytes(), &Edit::Append { text: "\n".to_string() + &s.text + "\n" }).unwrap();
        assert!(String::from_utf8_lossy(&appended).contains("merc_new = {"));
        let deleted = apply(
            &appended,
            &Edit::RemoveStatement {
                block_path: vec![],
                key: "merc_new".into(),
                value: None,
            },
        )
        .unwrap();
        assert_eq!(String::from_utf8_lossy(&deleted).trim_end(), base.trim_end());
    }

    #[test]
    fn vanilla_loads_all_mercenary_companies() {
        if !install_present() {
            return;
        }
        let vfs = Vfs::new(INSTALL, None).unwrap();
        let loc = loc::build(&vfs);
        let all = load(&vfs, &loc);
        assert!(all.len() >= 100, "companies: {}", all.len());
        // Known case: the Black Army (Hungary) is home_province = 153.
        let ba = all.iter().find(|c| c.key == "merc_black_army").expect("merc_black_army");
        assert_eq!(ba.home_province, 153);
        assert!(ba.trigger_present);
        assert!(ba.modifier.present);
        // twenty_good_men is home_province = 4365.
        let tgm = all.iter().find(|c| c.key == "twenty_good_men").expect("twenty_good_men");
        assert_eq!(tgm.home_province, 4365);
    }

    #[test]
    fn anbennar_mercenaries_smoke() {
        if !install_present() || !Path::new(ANBENNAR).is_dir() {
            return;
        }
        let vfs = Vfs::new(INSTALL, Some(ANBENNAR)).unwrap();
        let loc = loc::build(&vfs);
        let all = load(&vfs, &loc);
        assert!(!all.is_empty());
        if let Some(c) = all.iter().find(|c| c.scalars.iter().any(|s| s.key == "regiments_per_development" && s.present)) {
            let bytes = vfs.read(&c.file).unwrap();
            let out = apply(
                &bytes,
                &Edit::SetScalar {
                    path: vec![c.key.clone(), "regiments_per_development".into()],
                    value: "0.05".into(),
                    quoted: false,
                },
            );
            assert!(out.is_ok(), "mod merc edit should apply for {}", c.key);
        }
        println!("[mercenaries:anbennar] {} companies", all.len());
    }
}
