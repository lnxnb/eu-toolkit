//! Sprint 23.1 — Great projects (monuments), province-anchored.
//!
//! `common/great_projects/*.txt` holds one great-project block per top-level key
//! (`stonehenge`, `hagia_sophia`, `kiel_canal`, …), each anchored to a province
//! via `start = <province id>`. So the editor lives in the **province panel**
//! (`MonumentsSection`), not a View overlay. Verified key inventory (all vanilla
//! files scanned):
//!
//! * Entry scalars: `start` (the anchor province, read-only here), `date`,
//!   `time = { months }`, `build_cost`, `can_be_moved` (bool), `starting_tier`
//!   (int), `type` (`monument` / `canal`), `move_days_per_unit_distance` (int).
//! * Entry trigger blocks (14.2): `build_trigger`, `can_use_modifiers_trigger`,
//!   `can_upgrade_trigger`, `keep_trigger`.
//! * Entry effect blocks (14.2): `on_built`, `on_destroyed`.
//! * `tier_0`..`tier_3`, each: `upgrade_time = { months }`,
//!   `cost_to_upgrade = { factor }`, the typed modifier blocks
//!   `province_modifiers` / `area_modifier` / `country_modifiers`, the effect
//!   block `on_upgraded`, and preserve-unknown for anything else
//!   (`conditional_modifier`, …).
//! * Everything unmodeled round-trips untouched (`raw_extra`).
//!
//! The **visual** ("gfx/mesh reference") of a monument is the `GFX_great_project_<key>`
//! sprite in an `interface/*.gfx` file (a 2D `.dds` texturefile; every one of the
//! 136 vanilla monuments has one). There is no per-monument 3D map mesh in the
//! script files — the handful that place a map object do it through
//! `on_built = { show_ambient_object … }`, positioned per-monument in
//! `map/ambient_object.txt` (out of scope; reusing another's object would render
//! at the wrong province). So "copy the gfx/mesh reference from an existing
//! monument" = copy that monument's sprite texturefile into a fresh
//! `GFX_great_project_<new_key>` sprite. The game resolves the sprite by the
//! project key, so a scaffold that ships this sprite renders in game immediately.
//!
//! Loc keys are the bare block key: `<key>` (name) and `<key>_desc`.

use crate::gfx;
use crate::loc::{self, LocStore};
use crate::mod_writer;
use crate::paradox::{self, Block};
use crate::vfs::Vfs;

pub const GP_DIR: &str = "common/great_projects";
pub const GP_PROJECT_FILE: &str = "common/great_projects/zz_eutoolkit_great_projects.txt";

/// Entry-level modeled keys (excluded from `raw_extra`).
static ENTRY_MODELED: &[&str] = &[
    "start",
    "date",
    "time",
    "build_cost",
    "can_be_moved",
    "starting_tier",
    "type",
    "move_days_per_unit_distance",
    "build_trigger",
    "can_use_modifiers_trigger",
    "can_upgrade_trigger",
    "keep_trigger",
    "on_built",
    "on_destroyed",
    "tier_0",
    "tier_1",
    "tier_2",
    "tier_3",
];

/// Tier-level modeled keys (excluded from a tier's `raw_extra`).
static TIER_MODELED: &[&str] = &[
    "upgrade_time",
    "cost_to_upgrade",
    "province_modifiers",
    "area_modifier",
    "country_modifiers",
    "on_upgraded",
];

// ---------------------------------------------------------------------------
// Payload types (serialize camelCase; mirrored by src/lib/monuments.ts).
// ---------------------------------------------------------------------------

#[derive(serde::Serialize, Clone, Debug)]
pub struct ModRow {
    pub key: String,
    pub value: String,
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Scalar {
    /// Display + statement key (last path segment).
    pub key: String,
    /// Path relative to the entry block (`["build_cost"]`, `["time","months"]`,
    /// `["tier_1","cost_to_upgrade","factor"]`).
    pub path: Vec<String>,
    pub kind: String,
    pub present: bool,
    pub value: String,
    pub options: Vec<String>,
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ModifierBlockRef {
    pub name: String,
    pub path: Vec<String>,
    pub present: bool,
    pub flat: bool,
    pub rows: Vec<ModRow>,
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ScriptBlockRef {
    pub name: String,
    pub path: Vec<String>,
    pub registry: String,
    pub present: bool,
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Tier {
    pub index: u32,
    pub present: bool,
    pub scalars: Vec<Scalar>,
    pub modifier_blocks: Vec<ModifierBlockRef>,
    pub script_blocks: Vec<ScriptBlockRef>,
    pub raw_extra: Vec<String>,
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GreatProject {
    pub key: String,
    pub file: String,
    pub origin: String,
    /// The anchor province (`start`).
    pub start: u32,
    /// `type` value (`monument` / `canal`).
    pub project_type: String,
    pub name_key: String,
    pub name_loc: Option<String>,
    pub desc_key: String,
    pub desc_loc: Option<String>,
    pub scalars: Vec<Scalar>,
    pub script_blocks: Vec<ScriptBlockRef>,
    pub tiers: Vec<Tier>,
    /// `GFX_great_project_<key>` texturefile, if any (display + copy source).
    pub sprite: Option<String>,
    pub raw_extra: Vec<String>,
    pub raw: String,
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ProvinceMonuments {
    pub monuments: Vec<GreatProject>,
    pub project_file: String,
}

/// One monument in the copy-source picker.
#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct MonumentBrief {
    pub key: String,
    pub name: String,
    pub start: u32,
    pub project_type: String,
    pub sprite: Option<String>,
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

/// Flat `k = scalar` rows of a block, or `None` if it holds any nested block or
/// bare (keyless) value (so the frontend gates the typed editor to flat blocks).
fn flat_rows(b: &Block) -> Option<Vec<ModRow>> {
    let mut rows = Vec::new();
    for (k, v) in &b.items {
        match (k, v) {
            (Some(k), paradox::Value::Scalar(s)) => rows.push(ModRow {
                key: k.clone(),
                value: s.trim().to_string(),
            }),
            _ => return None,
        }
    }
    Some(rows)
}

fn scalar_spec(b: &Block, key: &str, path: Vec<String>, kind: &str, options: &[&str]) -> Scalar {
    let val = b.get_scalar(key).map(|s| s.trim().to_string());
    Scalar {
        key: key.to_string(),
        path,
        kind: kind.to_string(),
        present: val.is_some(),
        value: val.unwrap_or_default(),
        options: options.iter().map(|o| o.to_string()).collect(),
    }
}

/// A `{ months }` / `{ factor }` inner scalar, addressed at `[parent, inner]`.
fn nested_scalar(b: &Block, entry_key: &str, parent: &str, inner: &str) -> Scalar {
    let val = b
        .get_block(parent)
        .and_then(|pb| pb.get_scalar(inner))
        .map(|s| s.trim().to_string());
    let _ = entry_key;
    Scalar {
        key: inner.to_string(),
        path: vec![parent.to_string(), inner.to_string()],
        kind: "int".to_string(),
        present: val.is_some(),
        value: val.unwrap_or_default(),
        options: Vec::new(),
    }
}

fn modifier_block(b: &Block, name: &str, path: Vec<String>) -> ModifierBlockRef {
    let inner = b.get_block(name);
    let (present, flat, rows) = match inner {
        None => (false, true, Vec::new()),
        Some(ib) => match flat_rows(ib) {
            Some(rows) => (true, true, rows),
            None => (true, false, Vec::new()),
        },
    };
    ModifierBlockRef {
        name: name.to_string(),
        path,
        present,
        flat,
        rows,
    }
}

fn raw_extra_of(b: &Block, modeled: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (k, _) in &b.items {
        if let Some(k) = k.as_deref() {
            if !modeled.contains(&k) && seen.insert(k.to_string()) {
                out.push(k.to_string());
            }
        }
    }
    out
}

fn parse_tier(entry_key: &str, b: &Block, index: u32) -> Tier {
    let tier_key = format!("tier_{index}");
    let tb = b.get_block(&tier_key);
    let present = tb.is_some();
    let empty = Block { items: Vec::new() };
    let tb = tb.unwrap_or(&empty);

    let scalars = vec![
        nested_scalar_rel(tb, &tier_key, "upgrade_time", "months"),
        nested_scalar_rel(tb, &tier_key, "cost_to_upgrade", "factor"),
    ];
    let modifier_blocks = ["province_modifiers", "area_modifier", "country_modifiers"]
        .iter()
        .map(|n| {
            modifier_block(
                tb,
                n,
                vec![tier_key.clone(), n.to_string()],
            )
        })
        .collect();
    let script_blocks = vec![ScriptBlockRef {
        name: "on_upgraded".to_string(),
        path: vec![tier_key.clone(), "on_upgraded".to_string()],
        registry: "effects".to_string(),
        present: tb.get_block("on_upgraded").is_some(),
    }];
    let raw_extra = if present {
        raw_extra_of(tb, TIER_MODELED)
    } else {
        Vec::new()
    };
    let _ = entry_key;
    Tier {
        index,
        present,
        scalars,
        modifier_blocks,
        script_blocks,
        raw_extra,
    }
}

/// A `{ months }` / `{ factor }` inner scalar of a tier, addressed relative to
/// the tier block at `[parent, inner]`.
fn nested_scalar_rel(tb: &Block, tier_key: &str, parent: &str, inner: &str) -> Scalar {
    let val = tb
        .get_block(parent)
        .and_then(|pb| pb.get_scalar(inner))
        .map(|s| s.trim().to_string());
    Scalar {
        key: inner.to_string(),
        path: vec![tier_key.to_string(), parent.to_string(), inner.to_string()],
        kind: "int".to_string(),
        present: val.is_some(),
        value: val.unwrap_or_default(),
        options: Vec::new(),
    }
}

fn parse_project(
    file_bytes: &[u8],
    key: &str,
    b: &Block,
    loc: &LocStore,
    file: &str,
    origin: &str,
    sprite: Option<String>,
) -> GreatProject {
    let start = b
        .get_scalar("start")
        .and_then(|s| s.trim().parse::<u32>().ok())
        .unwrap_or(0);
    let project_type = b
        .get_scalar("type")
        .map(|s| s.trim().to_string())
        .unwrap_or_default();

    let scalars = vec![
        scalar_spec(b, "date", vec!["date".into()], "str", &[]),
        nested_scalar(b, key, "time", "months"),
        scalar_spec(b, "build_cost", vec!["build_cost".into()], "int", &[]),
        scalar_spec(
            b,
            "can_be_moved",
            vec!["can_be_moved".into()],
            "bool",
            &[],
        ),
        scalar_spec(
            b,
            "starting_tier",
            vec!["starting_tier".into()],
            "int",
            &[],
        ),
        scalar_spec(b, "type", vec!["type".into()], "enum", &["monument", "canal"]),
        scalar_spec(
            b,
            "move_days_per_unit_distance",
            vec!["move_days_per_unit_distance".into()],
            "int",
            &[],
        ),
    ];

    let script_blocks = vec![
        script_ref(b, "build_trigger", "triggers"),
        script_ref(b, "can_use_modifiers_trigger", "triggers"),
        script_ref(b, "can_upgrade_trigger", "triggers"),
        script_ref(b, "keep_trigger", "triggers"),
        script_ref(b, "on_built", "effects"),
        script_ref(b, "on_destroyed", "effects"),
    ];

    let tiers = (0..=3).map(|i| parse_tier(key, b, i)).collect();

    let raw = mod_writer::block_span(file_bytes, &[key.to_string()])
        .map(|(s, e)| String::from_utf8_lossy(&file_bytes[s..e]).into_owned())
        .unwrap_or_default();

    let name_key = key.to_string();
    let desc_key = format!("{key}_desc");

    GreatProject {
        key: key.to_string(),
        file: file.to_string(),
        origin: origin.to_string(),
        start,
        project_type,
        name_loc: loc.get(&name_key).map(str::to_string),
        name_key,
        desc_loc: loc.get(&desc_key).map(str::to_string),
        desc_key,
        scalars,
        script_blocks,
        tiers,
        sprite,
        raw_extra: raw_extra_of(b, ENTRY_MODELED),
        raw,
    }
}

fn script_ref(b: &Block, name: &str, registry: &str) -> ScriptBlockRef {
    ScriptBlockRef {
        name: name.to_string(),
        path: vec![name.to_string()],
        registry: registry.to_string(),
        present: b.get_block(name).is_some(),
    }
}

/// Loads every great project, resolving each key's `GFX_great_project_<key>`
/// sprite texturefile from the interface gfx index.
pub fn load(vfs: &Vfs, loc: &LocStore) -> Vec<GreatProject> {
    let sprites = gfx::sprite_index(vfs);
    let sprite_of = |key: &str| -> Option<String> {
        let name = format!("GFX_great_project_{key}");
        sprites.iter().find(|s| s.name == name).map(|s| s.texturefile.clone())
    };
    let mut out = Vec::new();
    for (name, path) in vfs.list_dir(GP_DIR) {
        if !name.to_lowercase().ends_with(".txt") {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let origin = origin_of(vfs, &path);
        let block = paradox::parse(&String::from_utf8_lossy(&bytes));
        let rel = format!("{GP_DIR}/{name}");
        for (key, b) in block.key_blocks() {
            let sprite = sprite_of(key);
            out.push(parse_project(&bytes, key, b, loc, &rel, origin, sprite));
        }
    }
    out
}

/// The great-project files (disk), plus the project file even when it exists
/// only as a pending `CreateFile`.
fn gp_files(vfs: &Vfs) -> Vec<String> {
    let mut v: Vec<String> = vfs
        .list_dir(GP_DIR)
        .into_iter()
        .filter(|(n, _)| n.to_lowercase().ends_with(".txt"))
        .map(|(n, _)| format!("{GP_DIR}/{n}"))
        .collect();
    if !v.iter().any(|f| f == GP_PROJECT_FILE) {
        v.push(GP_PROJECT_FILE.to_string());
    }
    v.sort();
    v.dedup();
    v
}

/// Like [`load`], but each file is previewed through the pending edit queue so
/// created/deleted/edited monuments show live (survives panel remounts). Sprites
/// still resolve from the on-disk gfx index (a just-created monument's sprite
/// file isn't written yet → `None` until save).
pub fn load_with_edits(
    vfs: &Vfs,
    loc: &LocStore,
    edits: &[crate::edits::TypedEdit],
) -> Vec<GreatProject> {
    let sprites = gfx::sprite_index(vfs);
    let sprite_of = |key: &str| -> Option<String> {
        let name = format!("GFX_great_project_{key}");
        sprites.iter().find(|s| s.name == name).map(|s| s.texturefile.clone())
    };
    let mut out = Vec::new();
    for rel in gp_files(vfs) {
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
            let sprite = sprite_of(key);
            out.push(parse_project(&bytes, key, b, loc, &rel, origin, sprite));
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Scaffold (copies tier structure + gfx sprite from a picked monument).
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
    /// Additive interface gfx file binding `GFX_great_project_<key>`.
    pub gfx_file: String,
    pub gfx_text: String,
    /// The source monument's texturefile the sprite was copied from.
    pub source_sprite: Option<String>,
    pub loc_entries: Vec<LocEntry>,
}

/// Placeholder texture used when the source monument has no sprite.
const PLACEHOLDER_TEX: &str = "gfx/interface/great_projects/great_project_place_holder.dds";

/// Finds a great-project block by key across the directory, returning
/// `(file_bytes, block)`.
fn find_project(vfs: &Vfs, key: &str) -> Option<(Vec<u8>, Block)> {
    for (name, path) in vfs.list_dir(GP_DIR) {
        if !name.to_lowercase().ends_with(".txt") {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let block = paradox::parse(&String::from_utf8_lossy(&bytes));
        if let Some(b) = block.get_block(key) {
            return Some((bytes, b.clone()));
        }
    }
    None
}

/// Extracts the `{ … }` raw text of a child block of `entry_key`, if present.
fn child_raw(file_bytes: &[u8], entry_key: &str, child: &str) -> Option<String> {
    mod_writer::block_span(file_bytes, &[entry_key.to_string(), child.to_string()])
        .map(|(s, e)| String::from_utf8_lossy(&file_bytes[s..e]).into_owned())
}

/// Builds a game-valid monument scaffold anchored to `province_id`, copying the
/// tier structure and the sprite texturefile from `source_key`.
pub fn scaffold_great_project(
    vfs: &Vfs,
    source_key: &str,
    new_key: &str,
    province_id: u32,
) -> Result<Scaffold, String> {
    let (src_bytes, src) =
        find_project(vfs, source_key).ok_or_else(|| format!("Source monument not found: {source_key}"))?;

    // Copied scalars (fall back to safe defaults when the source omits one).
    let build_cost = src.get_scalar("build_cost").unwrap_or("1000").trim().to_string();
    let can_be_moved = src.get_scalar("can_be_moved").unwrap_or("no").trim().to_string();
    let starting_tier = src.get_scalar("starting_tier").unwrap_or("0").trim().to_string();
    let time_raw = child_raw(&src_bytes, source_key, "time").unwrap_or_else(|| "{\n\t\tmonths = 0\n\t}".to_string());

    // Copied tiers (verbatim `{ … }`); a minimal fallback for any missing tier.
    let mut tiers = String::new();
    for i in 0..=3 {
        let raw = child_raw(&src_bytes, source_key, &format!("tier_{i}")).unwrap_or_else(|| {
            format!(
                "{{\n\t\tupgrade_time = {{ months = {} }}\n\t\tcost_to_upgrade = {{ factor = {} }}\n\t\tprovince_modifiers = {{\n\t\t}}\n\t\tarea_modifier = {{\n\t\t}}\n\t\tcountry_modifiers = {{\n\t\t}}\n\t\ton_upgraded = {{\n\t\t}}\n\t}}",
                if i == 0 { 0 } else { 120 },
                if i == 0 { 0 } else { 1000 },
            )
        });
        tiers.push_str(&format!("\ttier_{i} = {raw}\n"));
    }

    let text = format!(
        "{new_key} = {{\n\
\tstart = {province_id}\n\
\tdate = 1444.11.11\n\
\ttime = {time_raw}\n\
\tbuild_cost = {build_cost}\n\
\tcan_be_moved = {can_be_moved}\n\
\tstarting_tier = {starting_tier}\n\
\ttype = monument\n\
\tbuild_trigger = {{\n\t}}\n\
\ton_built = {{\n\t}}\n\
\ton_destroyed = {{\n\t}}\n\
\tcan_use_modifiers_trigger = {{\n\t}}\n\
\tcan_upgrade_trigger = {{\n\t}}\n\
\tkeep_trigger = {{\n\t}}\n\
{tiers}\
}}"
    );

    // Sprite: copy the source's texturefile into a fresh GFX_great_project_<key>.
    let sprites = gfx::sprite_index(vfs);
    let source_sprite = sprites
        .iter()
        .find(|s| s.name == format!("GFX_great_project_{source_key}"))
        .map(|s| s.texturefile.clone());
    let tex = source_sprite.clone().unwrap_or_else(|| PLACEHOLDER_TEX.to_string());
    let gfx_file = format!("interface/zz_eutoolkit_gp_{new_key}.gfx");
    let gfx_text = format!(
        "spriteTypes = {{\n\
\tspriteType = {{\n\
\t\tname = \"GFX_great_project_{new_key}\"\n\
\t\ttexturefile = \"{tex}\"\n\
\t}}\n\
}}\n"
    );

    let pretty = loc::prettify(new_key);
    Ok(Scaffold {
        key: new_key.to_string(),
        file: GP_PROJECT_FILE.to_string(),
        text,
        gfx_file,
        gfx_text,
        source_sprite,
        loc_entries: vec![
            LocEntry { key: new_key.to_string(), value: pretty.clone() },
            LocEntry { key: format!("{new_key}_desc"), value: format!("{pretty} monument.") },
        ],
    })
}

// ---------------------------------------------------------------------------
// Commands.
// ---------------------------------------------------------------------------

#[tauri::command(async)]
pub fn get_province_monuments(
    install_path: String,
    mod_path: Option<String>,
    id: u32,
    edits: Option<Vec<crate::edits::TypedEdit>>,
) -> Result<ProvinceMonuments, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    let loc = loc::store(&vfs, &install_path, mod_path.as_deref());
    let edits = edits.unwrap_or_default();
    let monuments = load_with_edits(&vfs, &loc, &edits)
        .into_iter()
        .filter(|p| p.start == id)
        .collect();
    Ok(ProvinceMonuments {
        monuments,
        project_file: GP_PROJECT_FILE.to_string(),
    })
}

#[tauri::command(async)]
pub fn list_monuments(
    install_path: String,
    mod_path: Option<String>,
) -> Result<Vec<MonumentBrief>, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    let loc = loc::store(&vfs, &install_path, mod_path.as_deref());
    let mut out: Vec<MonumentBrief> = load(&vfs, &loc)
        .into_iter()
        .filter(|p| p.project_type == "monument")
        .map(|p| MonumentBrief {
            name: p.name_loc.clone().unwrap_or_else(|| loc::prettify(&p.key)),
            key: p.key,
            start: p.start,
            project_type: p.project_type,
            sprite: p.sprite,
        })
        .collect();
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(out)
}

#[tauri::command(async)]
pub fn scaffold_great_project_cmd(
    install_path: String,
    mod_path: Option<String>,
    source_key: String,
    new_key: String,
    province_id: u32,
) -> Result<Scaffold, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    scaffold_great_project(&vfs, &source_key, &new_key, province_id)
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
        let root = std::env::temp_dir().join(format!("eu_toolkit_gp_test_{name}"));
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

    const GP_SRC: &str = "\
stonehenge = {\n\
\tstart = 234\n\
\tdate = -2500.01.01\n\
\ttime = { months = 0 }\n\
\tbuild_cost = 0\n\
\tcan_be_moved = yes\n\
\tstarting_tier = 0\n\
\ttype = monument\n\
\tbuild_trigger = {\n\t\talways = yes\n\t}\n\
\ton_built = {\n\t\tshow_ambient_object = stonehenge\n\t}\n\
\ton_destroyed = {\n\t\thide_ambient_object = stonehenge\n\t}\n\
\tcan_use_modifiers_trigger = {\n\t\talways = yes\n\t}\n\
\tcan_upgrade_trigger = {\n\t}\n\
\tkeep_trigger = {\n\t}\n\
\ttier_0 = {\n\
\t\tupgrade_time = { months = 0 }\n\
\t\tcost_to_upgrade = { factor = 0 }\n\
\t\tprovince_modifiers = {\n\t\t\tlocal_defensiveness = 0.1\n\t\t}\n\
\t\tarea_modifier = {\n\t\t}\n\
\t\tcountry_modifiers = {\n\t\t}\n\
\t\ton_upgraded = {\n\t\t}\n\
\t}\n\
\ttier_1 = {\n\
\t\tupgrade_time = { months = 120 }\n\
\t\tcost_to_upgrade = { factor = 1000 }\n\
\t\tprovince_modifiers = {\n\t\t\tlocal_defensiveness = 0.2\n\t\t}\n\
\t\tarea_modifier = {\n\t\t}\n\
\t\tcountry_modifiers = {\n\t\t\tprestige = 1\n\t\t}\n\
\t\ton_upgraded = {\n\t\t\towner = { add_prestige = 10 }\n\t\t}\n\
\t\tconditional_modifier = {\n\t\t\ttrigger = { always = yes }\n\t\t}\n\
\t}\n\
\ttier_2 = {\n\
\t\tupgrade_time = { months = 240 }\n\
\t\tcost_to_upgrade = { factor = 2000 }\n\
\t\tprovince_modifiers = {\n\t\t}\n\
\t\tarea_modifier = {\n\t\t}\n\
\t\tcountry_modifiers = {\n\t\t}\n\
\t\ton_upgraded = {\n\t\t}\n\
\t}\n\
\ttier_3 = {\n\
\t\tupgrade_time = { months = 480 }\n\
\t\tcost_to_upgrade = { factor = 4000 }\n\
\t\tprovince_modifiers = {\n\t\t}\n\
\t\tarea_modifier = {\n\t\t}\n\
\t\tcountry_modifiers = {\n\t\t}\n\
\t\ton_upgraded = {\n\t\t}\n\
\t}\n\
\tsome_unknown_key = yes\n\
}\n";

    const GFX_SRC: &str = "spriteTypes = {\n\
\tspriteType = {\n\
\t\tname = \"GFX_great_project_stonehenge\"\n\
\t\ttexturefile = \"gfx//interface//great_projects//great_project_stone_henge.dds\"\n\
\t}\n\
}\n";

    fn gp_fixture(name: &str) -> (std::path::PathBuf, Vfs) {
        synthetic(
            name,
            &[
                ("common/great_projects/00_test.txt", GP_SRC),
                ("interface/great_project.gfx", GFX_SRC),
            ],
        )
    }

    #[test]
    fn parses_great_project_scalars_tiers_scripts_and_raw() {
        let (_root, vfs) = gp_fixture("parse");
        let loc = LocStore::from_pairs(&[("stonehenge", "Stonehenge"), ("stonehenge_desc", "Ancient stones.")]);
        let all = load(&vfs, &loc);
        assert_eq!(all.len(), 1);
        let p = &all[0];
        assert_eq!(p.key, "stonehenge");
        assert_eq!(p.start, 234);
        assert_eq!(p.project_type, "monument");
        assert_eq!(p.name_loc.as_deref(), Some("Stonehenge"));
        assert_eq!(p.sprite.as_deref(), Some("gfx/interface/great_projects/great_project_stone_henge.dds"));
        // Entry scalars.
        let sc = |k: &str| p.scalars.iter().find(|s| s.key == k).unwrap();
        assert_eq!(sc("build_cost").value, "0");
        assert_eq!(sc("months").value, "0");
        assert_eq!(sc("months").path, vec!["time", "months"]);
        assert_eq!(sc("type").kind, "enum");
        assert!(sc("type").options.contains(&"canal".to_string()));
        // Script blocks.
        let sb = |k: &str| p.script_blocks.iter().find(|s| s.name == k).unwrap();
        assert!(sb("build_trigger").present && sb("build_trigger").registry == "triggers");
        assert!(sb("on_built").present && sb("on_built").registry == "effects");
        // Tiers.
        assert_eq!(p.tiers.len(), 4);
        let t1 = &p.tiers[1];
        assert!(t1.present);
        let ts = |k: &str| t1.scalars.iter().find(|s| s.key == k).unwrap();
        assert_eq!(ts("months").value, "120");
        assert_eq!(ts("months").path, vec!["tier_1", "upgrade_time", "months"]);
        assert_eq!(ts("factor").value, "1000");
        let pm = t1.modifier_blocks.iter().find(|m| m.name == "province_modifiers").unwrap();
        assert!(pm.present && pm.flat);
        assert_eq!(pm.rows.len(), 1);
        assert_eq!(pm.rows[0].key, "local_defensiveness");
        assert_eq!(pm.path, vec!["tier_1", "province_modifiers"]);
        assert!(t1.script_blocks[0].name == "on_upgraded" && t1.script_blocks[0].present);
        // conditional_modifier preserved as tier raw_extra.
        assert!(t1.raw_extra.contains(&"conditional_modifier".to_string()));
        // Entry preserve-unknown.
        assert!(p.raw_extra.contains(&"some_unknown_key".to_string()));
        assert!(p.raw.starts_with('{') && p.raw.ends_with('}'));
    }

    #[test]
    fn scalar_edit_is_byte_surgical() {
        let out = apply(
            GP_SRC.as_bytes(),
            &Edit::SetScalar {
                path: vec!["stonehenge".into(), "build_cost".into()],
                value: "500".into(),
                quoted: false,
            },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("build_cost = 500"));
        assert!(text.contains("show_ambient_object = stonehenge"));
        assert!(text.contains("some_unknown_key = yes"));
    }

    #[test]
    fn tier_month_and_modifier_edit_round_trip() {
        // Nested scalar: tier_1 upgrade_time months.
        let out = apply(
            GP_SRC.as_bytes(),
            &Edit::SetScalar {
                path: vec!["stonehenge".into(), "tier_1".into(), "upgrade_time".into(), "months".into()],
                value: "60".into(),
                quoted: false,
            },
        )
        .unwrap();
        // Nested modifier block: tier_1 province_modifiers whole-block set.
        let out = apply(
            &out,
            &Edit::SetBlock {
                path: vec!["stonehenge".into(), "tier_1".into(), "province_modifiers".into()],
                value: "local_defensiveness = 0.5".into(),
            },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("months = 60"));
        assert!(text.contains("local_defensiveness = 0.5"));
        // tier_2 untouched.
        assert!(text.contains("factor = 2000"));
    }

    #[test]
    fn tier_effect_insert_is_byte_surgical() {
        // Insert an effect into the nested tier_1 on_upgraded block.
        let out = apply(
            GP_SRC.as_bytes(),
            &Edit::InsertStatement {
                block_path: vec!["stonehenge".into(), "tier_1".into(), "on_upgraded".into()],
                statement: "add_prestige = 5".into(),
            },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("add_prestige = 5"));
        // Surrounding structure untouched.
        assert!(text.contains("owner = { add_prestige = 10 }"));
        assert!(text.contains("cost_to_upgrade = { factor = 1000 }"));
    }

    #[test]
    fn scaffold_copies_tier_structure_and_gfx_binding() {
        let (_root, vfs) = gp_fixture("scaffold");
        let s = scaffold_great_project(&vfs, "stonehenge", "my_monument", 1234).unwrap();
        assert_eq!(s.file, GP_PROJECT_FILE);
        assert_eq!(s.gfx_file, "interface/zz_eutoolkit_gp_my_monument.gfx");
        assert_eq!(
            s.source_sprite.as_deref(),
            Some("gfx/interface/great_projects/great_project_stone_henge.dds")
        );
        // Block parses back with the required game keys.
        let b = paradox::parse(&s.text);
        let f = b.get_block("my_monument").expect("scaffold parses");
        assert_eq!(f.get_scalar("start"), Some("1234"));
        assert_eq!(f.get_scalar("type"), Some("monument"));
        assert!(f.get_block("time").is_some());
        assert!(f.get_scalar("build_cost").is_some());
        assert!(f.get_scalar("starting_tier").is_some());
        // Every tier copied with its modifier structure (loads-in-game bar).
        for i in 0..=3 {
            let t = f.get_block(&format!("tier_{i}")).unwrap_or_else(|| panic!("tier_{i}"));
            assert!(t.get_block("upgrade_time").is_some(), "tier_{i} upgrade_time");
            assert!(t.get_block("cost_to_upgrade").is_some(), "tier_{i} cost_to_upgrade");
            assert!(t.get_block("province_modifiers").is_some(), "tier_{i} province_modifiers");
            assert!(t.get_block("country_modifiers").is_some(), "tier_{i} country_modifiers");
        }
        // tier_1's copied province modifier value survived.
        let t1 = f.get_block("tier_1").unwrap();
        assert_eq!(
            t1.get_block("province_modifiers").unwrap().get_scalar("local_defensiveness"),
            Some("0.2")
        );
        // Triggers blanked (no source ambient-object mis-reference).
        assert!(f.get_block("on_built").unwrap().items.is_empty());
        // The gfx binding copies the source texturefile under the new sprite name.
        let g = paradox::parse(&s.gfx_text);
        let st = g.get_block("spriteTypes").unwrap();
        let (_kind, sprite) = st.key_blocks().next().unwrap();
        assert_eq!(sprite.get_scalar("name"), Some("GFX_great_project_my_monument"));
        assert_eq!(
            sprite.get_scalar("texturefile"),
            Some("gfx/interface/great_projects/great_project_stone_henge.dds")
        );
        // Loc entries: name + desc.
        let keys: Vec<&str> = s.loc_entries.iter().map(|e| e.key.as_str()).collect();
        assert!(keys.contains(&"my_monument"));
        assert!(keys.contains(&"my_monument_desc"));
    }

    #[test]
    fn scaffold_create_then_delete_is_identity() {
        let (_root, vfs) = gp_fixture("createdelete");
        let s = scaffold_great_project(&vfs, "stonehenge", "brand_new", 99).unwrap();
        let base = "existing = {\n\tstart = 1\n}\n";
        let appended = apply(base.as_bytes(), &Edit::Append { text: format!("\n{}\n", s.text) }).unwrap();
        assert!(String::from_utf8_lossy(&appended).contains("brand_new = {"));
        let deleted = apply(
            &appended,
            &Edit::RemoveStatement {
                block_path: vec![],
                key: "brand_new".into(),
                value: None,
            },
        )
        .unwrap();
        // Appending "\n{text}\n" then removing the block leaves a trailing blank
        // line; trim to compare structural identity.
        assert_eq!(
            String::from_utf8_lossy(&deleted).trim_end(),
            base.trim_end()
        );
    }

    #[test]
    fn vanilla_loads_all_great_projects() {
        if !install_present() {
            return;
        }
        let vfs = Vfs::new(INSTALL, None).unwrap();
        let loc = loc::build(&vfs);
        let all = load(&vfs, &loc);
        assert!(all.len() >= 130, "great projects: {}", all.len());
        // Known case: Stonehenge, start = 234 (verified in 01_monuments.txt).
        let sh = all.iter().find(|p| p.key == "stonehenge").expect("stonehenge");
        assert_eq!(sh.start, 234);
        assert_eq!(sh.project_type, "monument");
        assert!(sh.sprite.is_some(), "stonehenge has a GFX sprite");
        assert_eq!(sh.tiers.len(), 4);
        // Kiel canal is a canal-type great project anchored at 1775.
        let kiel = all.iter().find(|p| p.key == "kiel_canal").expect("kiel_canal");
        assert_eq!(kiel.start, 1775);
        assert_eq!(kiel.project_type, "canal");
    }

    #[test]
    fn vanilla_scaffold_from_real_monument_parses() {
        if !install_present() {
            return;
        }
        let vfs = Vfs::new(INSTALL, None).unwrap();
        let s = scaffold_great_project(&vfs, "stonehenge", "eutk_test_monument", 1).unwrap();
        assert!(s.source_sprite.is_some());
        let b = paradox::parse(&s.text);
        let f = b.get_block("eutk_test_monument").unwrap();
        assert_eq!(f.get_scalar("start"), Some("1"));
        assert_eq!(f.get_scalar("type"), Some("monument"));
        // All four tiers present with modifier structure.
        for i in 0..=3 {
            assert!(f.get_block(&format!("tier_{i}")).is_some(), "tier_{i}");
        }
    }

    #[test]
    fn anbennar_great_projects_smoke() {
        if !install_present() || !Path::new(ANBENNAR).is_dir() {
            return;
        }
        let vfs = Vfs::new(INSTALL, Some(ANBENNAR)).unwrap();
        let loc = loc::build(&vfs);
        let all = load(&vfs, &loc);
        assert!(!all.is_empty());
        // A sample project round-trips a scalar edit through its file.
        if let Some(p) = all.iter().find(|p| p.scalars.iter().any(|s| s.key == "build_cost" && s.present)) {
            let bytes = vfs.read(&p.file).unwrap();
            let out = apply(
                &bytes,
                &Edit::SetScalar {
                    path: vec![p.key.clone(), "build_cost".into()],
                    value: "1234".into(),
                    quoted: false,
                },
            );
            assert!(out.is_ok(), "mod great-project build_cost edit should apply for {}", p.key);
        }
        println!("[great_projects:anbennar] {} projects", all.len());
    }
}
