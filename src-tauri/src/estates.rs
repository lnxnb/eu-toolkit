//! Sprint 20 — Estates subsystem (View ▸ Estates).
//!
//! Three sibling definition kinds, one per `common/` directory, share nearly the
//! same shape — a top-level `key = { … }` block per entity across a directory of
//! `.txt` files (merged through the [`Vfs`], mod shadows base):
//!
//! * **estates** (`common/estates/*.txt`): `icon` (strip index), `color`, a
//!   validity `trigger`, the three loyalty-tier country modifier blocks
//!   (`country_modifier_happy/neutral/angry`), `land_ownership_modifier`,
//!   `base_influence`/`influence_from_dev_modifier`/`contributes_to_curia_treasury`
//!   scalars, and the `privileges`/`agendas` id lists. Repeated
//!   `influence_modifier`/`loyalty_modifier`/`custom_name`/`province_independence_weight`
//!   sub-blocks are preserve-unknown (round-trip untouched, shown read-only).
//! * **privileges** (`common/estate_privileges/*.txt`): `icon` (a `privilege_*`
//!   sprite), scalars `land_share`/`max_absolutism`/`loyalty`/`influence`/`cooldown_years`,
//!   `benefits`/`penalties`/`modifier_by_land_ownership` modifier blocks, and the
//!   `is_valid`/`can_select`/`can_revoke` triggers + `on_granted`/`on_revoked`/`on_invalid`
//!   effects. Repeated `conditional_modifier`, `mechanics`, `ai_will_do`, etc. are
//!   preserve-unknown.
//! * **agendas** (`common/estate_agendas/*.txt`): `max_days_active`, the `modifier`
//!   block, the `can_select`/`task_requirements`/`fail_if`/`invalid_trigger`/
//!   `provinces_to_highlight` triggers + `pre_effect`/`immediate_effect`/
//!   `task_completed_effect`/`failing_effect`/`on_invalid` effects. `selection_weight`
//!   is preserve-unknown.
//!
//! # Editing model (existing typed-edit vocabulary only)
//! * **Scalars** → `SetScalar` (present) / `InsertStatement` (absent) at `[key]`.
//! * **Modifier blocks** → the typed `ModifierEditor`; a whole-block `SetBlock`
//!   rewrite (create-when-absent via `InsertStatement`). Only enabled for *flat*
//!   blocks (all rows are `k = scalar`); a block carrying nested content is shown
//!   read-only so a rewrite never drops it.
//! * **Trigger / effect blocks** → the 14.2 `ScriptTreeEditor`
//!   (`parse_script_block_with_edits` + the tree edits).
//! * **Loc name/desc** → `LocOverride` on `<key>` / `<key>_desc`.
//! * **Icon** → `SetScalar` at `[key, "icon"]` (sprite name for privileges, strip
//!   index for estates).
//! * Everything unmodeled (`raw_extra`) round-trips untouched, read-only.
//!
//! # Country-history estate state (verified against real files)
//! The start-date privilege grant in `history/countries/<TAG> - Name.txt` is
//! **`set_estate_privilege = <priv>`** (NOT `add_estate_privilege`, which is an
//! in-game effect that never appears in history). It occurs top-level or inside a
//! dated block; a country may carry several. There is **no** `estates = { … }`
//! reference key on a privilege — a privilege is offered by an estate by being
//! listed in that estate's `privileges = { … }` block (the reverse direction), so
//! creating a privilege also `AddId`s its key into the chosen estate's list.

use crate::date::{self, Date};
use crate::loc::{self, LocStore};
use crate::mod_writer;
use crate::paradox::{self, Block, Value};
use crate::vfs::Vfs;

pub const ESTATES_DIR: &str = "common/estates";
pub const PRIVILEGES_DIR: &str = "common/estate_privileges";
pub const AGENDAS_DIR: &str = "common/estate_agendas";

pub const ESTATES_PROJECT_FILE: &str = "common/estates/zz_eutoolkit_estates.txt";
pub const PRIVILEGES_PROJECT_FILE: &str =
    "common/estate_privileges/zz_eutoolkit_estate_privileges.txt";
pub const AGENDAS_PROJECT_FILE: &str = "common/estate_agendas/zz_eutoolkit_estate_agendas.txt";

/// The country-history effect that grants a starting privilege.
pub const SET_PRIVILEGE: &str = "set_estate_privilege";

// ---------------------------------------------------------------------------
// Per-kind schema.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum ScalarKind {
    Num,
    Int,
    Bool,
}

impl ScalarKind {
    fn as_str(self) -> &'static str {
        match self {
            ScalarKind::Num => "num",
            ScalarKind::Int => "int",
            ScalarKind::Bool => "bool",
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum IconKind {
    None,
    Index,
    Sprite,
}

impl IconKind {
    fn as_str(self) -> &'static str {
        match self {
            IconKind::None => "none",
            IconKind::Index => "index",
            IconKind::Sprite => "sprite",
        }
    }
}

struct Schema {
    kind: &'static str,
    dir: &'static str,
    icon: IconKind,
    has_color: bool,
    has_lists: bool,
    scalars: &'static [(&'static str, ScalarKind)],
    modifiers: &'static [&'static str],
    triggers: &'static [&'static str],
    effects: &'static [&'static str],
}

static ESTATE_SCHEMA: Schema = Schema {
    kind: "estate",
    dir: ESTATES_DIR,
    icon: IconKind::Index,
    has_color: true,
    has_lists: true,
    scalars: &[
        ("base_influence", ScalarKind::Num),
        ("influence_from_dev_modifier", ScalarKind::Num),
        ("contributes_to_curia_treasury", ScalarKind::Bool),
    ],
    modifiers: &[
        "country_modifier_happy",
        "country_modifier_neutral",
        "country_modifier_angry",
        "land_ownership_modifier",
    ],
    triggers: &["trigger"],
    effects: &[],
};

static PRIVILEGE_SCHEMA: Schema = Schema {
    kind: "privilege",
    dir: PRIVILEGES_DIR,
    icon: IconKind::Sprite,
    has_color: false,
    has_lists: false,
    scalars: &[
        ("land_share", ScalarKind::Num),
        ("max_absolutism", ScalarKind::Num),
        ("loyalty", ScalarKind::Num),
        ("influence", ScalarKind::Num),
        ("cooldown_years", ScalarKind::Int),
    ],
    modifiers: &["benefits", "penalties", "modifier_by_land_ownership"],
    triggers: &["is_valid", "can_select", "can_revoke"],
    effects: &["on_granted", "on_revoked", "on_invalid"],
};

static AGENDA_SCHEMA: Schema = Schema {
    kind: "agenda",
    dir: AGENDAS_DIR,
    icon: IconKind::None,
    has_color: false,
    has_lists: false,
    scalars: &[("max_days_active", ScalarKind::Int)],
    modifiers: &["modifier"],
    triggers: &[
        "can_select",
        "task_requirements",
        "fail_if",
        "invalid_trigger",
        "provinces_to_highlight",
    ],
    effects: &[
        "pre_effect",
        "immediate_effect",
        "task_completed_effect",
        "failing_effect",
        "on_invalid",
    ],
};

// ---------------------------------------------------------------------------
// Payload types (serialize camelCase; mirrored by src/lib/estates.ts).
// ---------------------------------------------------------------------------

#[derive(serde::Serialize, Clone, Debug)]
pub struct ModRow {
    pub key: String,
    pub value: String,
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ModifierBlock {
    pub name: String,
    pub present: bool,
    /// All rows are flat `k = scalar` (safe to rewrite via the typed editor).
    pub flat: bool,
    pub rows: Vec<ModRow>,
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ScriptBlockRef {
    pub name: String,
    /// `triggers` | `effects` — which registry the tree editor uses.
    pub registry: String,
    pub present: bool,
}

#[derive(serde::Serialize, Clone, Debug)]
pub struct Scalar {
    pub key: String,
    pub kind: String,
    pub present: bool,
    pub value: String,
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct EstateObject {
    /// `estate` | `privilege` | `agenda`.
    pub kind: String,
    pub key: String,
    pub file: String,
    pub origin: String,
    pub name: String,
    pub loc_key: String,
    pub desc_key: String,
    pub desc_loc: Option<String>,
    pub icon: Option<String>,
    pub icon_kind: String,
    pub color: Option<[u8; 3]>,
    pub scalars: Vec<Scalar>,
    pub modifier_blocks: Vec<ModifierBlock>,
    pub script_blocks: Vec<ScriptBlockRef>,
    /// Estate only: the privilege id list.
    pub privileges: Vec<String>,
    /// Estate only: the agenda id list.
    pub agendas: Vec<String>,
    pub raw_extra: Vec<String>,
    pub raw: String,
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct EstatesData {
    pub estates: Vec<EstateObject>,
    pub privileges: Vec<EstateObject>,
    pub agendas: Vec<EstateObject>,
    pub estates_project_file: String,
    pub privileges_project_file: String,
    pub agendas_project_file: String,
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

fn modifier_block(b: &Block, name: &str) -> ModifierBlock {
    match b.get_block(name) {
        None => ModifierBlock {
            name: name.to_string(),
            present: false,
            flat: true,
            rows: Vec::new(),
        },
        Some(inner) => {
            let mut rows = Vec::new();
            let mut flat = true;
            for (k, v) in &inner.items {
                match (k, v) {
                    (Some(k), Value::Scalar(s)) => rows.push(ModRow {
                        key: k.clone(),
                        value: s.clone(),
                    }),
                    // A nested block or bare token means a plain SetBlock rewrite
                    // from typed rows would drop content — mark non-flat.
                    _ => flat = false,
                }
            }
            ModifierBlock {
                name: name.to_string(),
                present: true,
                flat,
                rows,
            }
        }
    }
}

fn parse_object(file_bytes: &[u8], key: &str, b: &Block, loc: &LocStore, file: &str, origin: &str, schema: &Schema) -> EstateObject {
    // Icon.
    let icon = b.get_scalar("icon").map(|s| s.trim().to_string());

    // Color (estates).
    let color = if schema.has_color {
        b.get_block("color").and_then(paradox::color_from_block)
    } else {
        None
    };

    // Scalars.
    let scalars = schema
        .scalars
        .iter()
        .map(|(name, kind)| {
            let val = b.get_scalar(name).map(|s| s.trim().to_string());
            Scalar {
                key: name.to_string(),
                kind: kind.as_str().to_string(),
                present: val.is_some(),
                value: val.unwrap_or_default(),
            }
        })
        .collect();

    // Modifier blocks.
    let modifier_blocks = schema
        .modifiers
        .iter()
        .map(|name| modifier_block(b, name))
        .collect();

    // Trigger + effect blocks.
    let mut script_blocks: Vec<ScriptBlockRef> = Vec::new();
    for name in schema.triggers {
        script_blocks.push(ScriptBlockRef {
            name: name.to_string(),
            registry: "triggers".to_string(),
            present: b.get_block(name).is_some(),
        });
    }
    for name in schema.effects {
        script_blocks.push(ScriptBlockRef {
            name: name.to_string(),
            registry: "effects".to_string(),
            present: b.get_block(name).is_some(),
        });
    }

    // Id lists (estates).
    let (privileges, agendas) = if schema.has_lists {
        (list_ids(b, "privileges"), list_ids(b, "agendas"))
    } else {
        (Vec::new(), Vec::new())
    };

    // Preserve-unknown: every top-level key not modeled above.
    let mut modeled: std::collections::HashSet<&str> = std::collections::HashSet::new();
    modeled.insert("icon");
    if schema.has_color {
        modeled.insert("color");
    }
    if schema.has_lists {
        modeled.insert("privileges");
        modeled.insert("agendas");
    }
    for (n, _) in schema.scalars {
        modeled.insert(n);
    }
    for n in schema.modifiers {
        modeled.insert(n);
    }
    for n in schema.triggers {
        modeled.insert(n);
    }
    for n in schema.effects {
        modeled.insert(n);
    }
    let mut raw_extra: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
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

    let desc_key = format!("{key}_desc");
    EstateObject {
        kind: schema.kind.to_string(),
        key: key.to_string(),
        file: file.to_string(),
        origin: origin.to_string(),
        name: loc.resolve(key),
        loc_key: key.to_string(),
        desc_loc: loc.get(&desc_key).map(str::to_string),
        desc_key,
        icon,
        icon_kind: schema.icon.as_str().to_string(),
        color,
        scalars,
        modifier_blocks,
        script_blocks,
        privileges,
        agendas,
        raw_extra,
        raw,
    }
}

/// Bare-token id list under `name` (e.g. `privileges = { a b c }`).
fn list_ids(b: &Block, name: &str) -> Vec<String> {
    match b.get_block(name) {
        None => Vec::new(),
        Some(inner) => inner
            .items
            .iter()
            .filter_map(|(k, v)| match (k, v) {
                (None, Value::Scalar(s)) => Some(s.clone()),
                _ => None,
            })
            .collect(),
    }
}

fn load_kind(vfs: &Vfs, loc: &LocStore, schema: &Schema) -> Vec<EstateObject> {
    let mut out = Vec::new();
    for (name, path) in vfs.list_dir(schema.dir) {
        if !name.to_lowercase().ends_with(".txt") {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let origin = origin_of(vfs, &path);
        let block = paradox::parse(&String::from_utf8_lossy(&bytes));
        let rel = format!("{}/{}", schema.dir, name);
        for (key, b) in block.key_blocks() {
            out.push(parse_object(&bytes, key, b, loc, &rel, origin, schema));
        }
    }
    out
}

pub fn load(vfs: &Vfs, loc: &LocStore) -> EstatesData {
    EstatesData {
        estates: load_kind(vfs, loc, &ESTATE_SCHEMA),
        privileges: load_kind(vfs, loc, &PRIVILEGE_SCHEMA),
        agendas: load_kind(vfs, loc, &AGENDA_SCHEMA),
        estates_project_file: ESTATES_PROJECT_FILE.to_string(),
        privileges_project_file: PRIVILEGES_PROJECT_FILE.to_string(),
        agendas_project_file: AGENDAS_PROJECT_FILE.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Scaffolds (copied from vanilla minimal siblings; unit-tested to parse back).
// ---------------------------------------------------------------------------

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Scaffold {
    pub key: String,
    pub file: String,
    pub text: String,
    /// The loc keys to queue as `LocOverride`s (name + desc).
    pub loc_name_key: String,
    pub loc_desc_key: String,
    pub loc_name: String,
}

/// Modeled on `estate_special` (999_special.txt) — the minimal game-valid estate.
pub fn scaffold_estate(key: &str) -> Scaffold {
    let text = format!(
        "{key} = {{\n\
\ticon = 1\n\
\tcolor = {{ 128 128 128 }}\n\
\ttrigger = {{\n\t\talways = yes\n\t}}\n\
\tcountry_modifier_happy = {{\n\t}}\n\
\tcountry_modifier_neutral = {{\n\t}}\n\
\tcountry_modifier_angry = {{\n\t}}\n\
\tland_ownership_modifier = {{\n\t}}\n\
\tprovince_independence_weight = {{\n\t\tfactor = 1\n\t}}\n\
\tbase_influence = 20\n\
\tinfluence_from_dev_modifier = 1.0\n\
\tprivileges = {{\n\t}}\n\
\tagendas = {{\n\t}}\n\
}}"
    );
    scaffold_common(key, ESTATES_PROJECT_FILE, text)
}

/// Modeled on `estate_church_land_rights` reduced to the required minimum.
pub fn scaffold_privilege(key: &str) -> Scaffold {
    let text = format!(
        "{key} = {{\n\
\ticon = privilege_grant_autonomy\n\
\tland_share = 0\n\
\tmax_absolutism = 0\n\
\tloyalty = 0.05\n\
\tinfluence = 0.05\n\
\tis_valid = {{\n\t}}\n\
\tcan_select = {{\n\t}}\n\
\ton_granted = {{\n\t}}\n\
\ton_revoked = {{\n\t}}\n\
\tbenefits = {{\n\t}}\n\
\tpenalties = {{\n\t}}\n\
\tai_will_do = {{\n\t\tfactor = 1\n\t}}\n\
}}"
    );
    scaffold_common(key, PRIVILEGES_PROJECT_FILE, text)
}

/// Modeled on `estate_church_hire_advisor` reduced to the required minimum.
/// `estate` is the owning estate key (its agendas roll up loyalty to it).
pub fn scaffold_agenda(key: &str, estate: &str) -> Scaffold {
    let text = format!(
        "{key} = {{\n\
\tcan_select = {{\n\t}}\n\
\tselection_weight = {{\n\t\tfactor = 1\n\t}}\n\
\ttask_requirements = {{\n\t}}\n\
\ttask_completed_effect = {{\n\
\t\tadd_estate_loyalty = {{\n\t\t\testate = {estate}\n\t\t\tloyalty = 10\n\t\t}}\n\
\t}}\n\
\tfailing_effect = {{\n\
\t\tadd_estate_loyalty = {{\n\t\t\testate = {estate}\n\t\t\tloyalty = -5\n\t\t}}\n\
\t}}\n\
}}"
    );
    scaffold_common(key, AGENDAS_PROJECT_FILE, text)
}

fn scaffold_common(key: &str, file: &str, text: String) -> Scaffold {
    Scaffold {
        key: key.to_string(),
        file: file.to_string(),
        text,
        loc_name_key: key.to_string(),
        loc_desc_key: format!("{key}_desc"),
        loc_name: loc::prettify(key),
    }
}

// ---------------------------------------------------------------------------
// Privilege availability (which countries grant it at start).
// ---------------------------------------------------------------------------

#[derive(serde::Serialize, Clone, Debug)]
pub struct PrivilegeHolder {
    pub tag: String,
    pub name: String,
    /// `None` = granted top-level; else the dated block's date string.
    pub date: Option<String>,
}

/// Scans every `history/countries/*` file for `set_estate_privilege = <priv>`
/// (top-level or in any dated block) and returns the granting countries.
pub fn privilege_holders(vfs: &Vfs, loc: &LocStore, privilege: &str) -> Vec<PrivilegeHolder> {
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
        let mut date: Option<String> = None;
        let mut found = false;
        for (k, v) in &block.items {
            match (k.as_deref(), v) {
                (Some(SET_PRIVILEGE), Value::Scalar(s)) if s == privilege => {
                    found = true;
                }
                (Some(dk), Value::Block(db)) if is_date_key(dk) => {
                    for (ik, iv) in &db.items {
                        if let (Some(SET_PRIVILEGE), Value::Scalar(s)) = (ik.as_deref(), iv) {
                            if s == privilege {
                                found = true;
                                if date.is_none() {
                                    date = Some(dk.to_string());
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        if found {
            out.push(PrivilegeHolder {
                tag: tag.clone(),
                name: loc.resolve(&tag),
                date,
            });
        }
    }
    out.sort_by(|a, b| a.tag.cmp(&b.tag));
    out
}

fn is_date_key(k: &str) -> bool {
    date::parse_date(k).is_some()
}

// ---------------------------------------------------------------------------
// Country-history starting estate state (Sprint 12 date-aware read).
// ---------------------------------------------------------------------------

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct StartingPrivilege {
    pub privilege: String,
    pub name: String,
    /// Owning estate key resolved from the estate catalog (if known).
    pub estate: Option<String>,
    /// `None` = top-level grant; else the dated block it came from.
    pub date: Option<String>,
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct EstateBrief {
    pub key: String,
    pub name: String,
    pub icon: Option<String>,
    /// The privileges this estate offers (key + resolved name).
    pub privileges: Vec<PrivilegeBrief>,
}

#[derive(serde::Serialize, Clone, Debug)]
pub struct PrivilegeBrief {
    pub key: String,
    pub name: String,
    pub file: String,
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CountryEstates {
    pub tag: String,
    pub file: String,
    /// Start-date privilege grants folded to `date`.
    pub starting: Vec<StartingPrivilege>,
    /// The full estate catalog for the per-estate privilege picker.
    pub estates: Vec<EstateBrief>,
}

/// Dated blocks with `block_date <= at`, in file order (mirrors game_data).
fn dated_blocks_le<'a>(history: &'a Block, at: Date) -> Vec<(&'a str, &'a Block)> {
    let mut out = Vec::new();
    for (k, v) in &history.items {
        if let (Some(k), Value::Block(b)) = (k.as_deref(), v) {
            if let Some(d) = date::parse_date(k) {
                if d <= at {
                    out.push((k, b));
                }
            }
        }
    }
    out
}

pub fn country_estates(vfs: &Vfs, loc: &LocStore, tag: &str, date: Date) -> Result<CountryEstates, String> {
    let (name, bytes) = crate::game_data::country_history_file(vfs, tag)
        .ok_or_else(|| format!("No history file for {tag}"))?;
    let file = format!("history/countries/{name}");
    let block = paradox::parse(&String::from_utf8_lossy(&bytes));

    // Build the estate catalog + privilege -> estate map.
    let data = load(vfs, loc);
    let mut priv_to_estate: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let estates: Vec<EstateBrief> = data
        .estates
        .iter()
        .map(|e| {
            for p in &e.privileges {
                priv_to_estate.entry(p.clone()).or_insert_with(|| e.key.clone());
            }
            EstateBrief {
                key: e.key.clone(),
                name: e.name.clone(),
                icon: e.icon.clone(),
                privileges: e
                    .privileges
                    .iter()
                    .map(|pk| {
                        let file = data
                            .privileges
                            .iter()
                            .find(|p| &p.key == pk)
                            .map(|p| p.file.clone())
                            .unwrap_or_default();
                        PrivilegeBrief {
                            key: pk.clone(),
                            name: loc.resolve(pk),
                            file,
                        }
                    })
                    .collect(),
            }
        })
        .collect();

    let mut starting: Vec<StartingPrivilege> = Vec::new();
    let push = |privilege: &str, date: Option<String>, starting: &mut Vec<StartingPrivilege>| {
        starting.push(StartingPrivilege {
            estate: priv_to_estate.get(privilege).cloned(),
            name: loc.resolve(privilege),
            privilege: privilege.to_string(),
            date,
        });
    };
    for (k, v) in &block.items {
        if let (Some(SET_PRIVILEGE), Value::Scalar(s)) = (k.as_deref(), v) {
            push(s, None, &mut starting);
        }
    }
    for (dstr, db) in dated_blocks_le(&block, date) {
        for (k, v) in &db.items {
            if let (Some(SET_PRIVILEGE), Value::Scalar(s)) = (k.as_deref(), v) {
                push(s, Some(dstr.to_string()), &mut starting);
            }
        }
    }

    Ok(CountryEstates {
        tag: tag.to_string(),
        file,
        starting,
        estates,
    })
}

// ---------------------------------------------------------------------------
// Commands.
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_estates(install_path: String, mod_path: Option<String>) -> Result<EstatesData, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    let loc = loc::store(&vfs, &install_path, mod_path.as_deref());
    Ok(load(&vfs, &loc))
}

#[tauri::command]
pub fn scaffold_estate_object(kind: String, key: String, estate: Option<String>) -> Result<Scaffold, String> {
    match kind.as_str() {
        "estate" => Ok(scaffold_estate(&key)),
        "privilege" => Ok(scaffold_privilege(&key)),
        "agenda" => Ok(scaffold_agenda(&key, estate.as_deref().unwrap_or("estate_nobles"))),
        _ => Err(format!("Unknown estate object kind: {kind}")),
    }
}

#[tauri::command]
pub fn get_privilege_holders(
    install_path: String,
    mod_path: Option<String>,
    privilege: String,
) -> Result<Vec<PrivilegeHolder>, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    let loc = loc::store(&vfs, &install_path, mod_path.as_deref());
    Ok(privilege_holders(&vfs, &loc, &privilege))
}

#[tauri::command]
pub fn get_country_estates(
    install_path: String,
    mod_path: Option<String>,
    date: Option<String>,
    tag: String,
) -> Result<CountryEstates, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    let loc = loc::store(&vfs, &install_path, mod_path.as_deref());
    let at = crate::bookmarks::resolve_date(&vfs, date.as_deref())?;
    country_estates(&vfs, &loc, &tag, at)
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
        let root = std::env::temp_dir().join(format!("eu_toolkit_estates_test_{name}"));
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

    const ESTATE_SRC: &str = "\
estate_test = {\n\
\ticon = 3\n\
\tcolor = { 200 150 0 }\n\
\ttrigger = {\n\t\treligion_group = christian\n\t}\n\
\tcountry_modifier_happy = {\n\t\tglobal_tax_modifier = 0.2\n\t\tdevotion = 0.5\n\t}\n\
\tcountry_modifier_neutral = {\n\t}\n\
\tcountry_modifier_angry = {\n\t\tglobal_unrest = 2\n\t}\n\
\tland_ownership_modifier = {\n\t}\n\
\tbase_influence = 10\n\
\tinfluence_modifier = {\n\t\tdesc = X\n\t\ttrigger = { tag = HED }\n\t\tinfluence = 20\n\t}\n\
\tprivileges = {\n\t\testate_test_land_rights\n\t\testate_test_more_rights\n\t}\n\
\tagendas = {\n\t\testate_test_do_x\n\t}\n\
\tinfluence_from_dev_modifier = 1.0\n\
}\n";

    const PRIV_SRC: &str = "\
estate_test_land_rights = {\n\
\ticon = privilege_grant_autonomy\n\
\tland_share = 5\n\
\tmax_absolutism = -5\n\
\tloyalty = 0.05\n\
\tinfluence = 0.05\n\
\tis_valid = {\n\t}\n\
\tcan_select = {\n\t\ttag = SWE\n\t}\n\
\ton_granted = {\n\t}\n\
\tbenefits = {\n\t\tgoverning_capacity = 100\n\t}\n\
\tpenalties = {\n\t}\n\
\tconditional_modifier = {\n\t\ttrigger = { always = yes }\n\t\tmodifier = { max_absolutism = 1 }\n\t}\n\
\tai_will_do = {\n\t\tfactor = 5\n\t}\n\
}\n";

    fn estate_fixture(name: &str) -> (std::path::PathBuf, Vfs) {
        synthetic(
            name,
            &[
                ("common/estates/00_test.txt", ESTATE_SRC),
                ("common/estate_privileges/00_test.txt", PRIV_SRC),
                ("common/estate_agendas/00_test.txt", "estate_test_do_x = {\n\tcan_select = {\n\t}\n\tmodifier = {\n\t\tprestige = 1\n\t}\n}\n"),
            ],
        )
    }

    #[test]
    fn parses_estate_scalars_modifiers_triggers_lists_and_raw() {
        let (_root, vfs) = estate_fixture("parse_estate");
        let loc = LocStore::from_pairs(&[("estate_test", "Test Estate")]);
        let data = load(&vfs, &loc);
        assert_eq!(data.estates.len(), 1);
        let e = &data.estates[0];
        assert_eq!(e.kind, "estate");
        assert_eq!(e.name, "Test Estate");
        assert_eq!(e.icon.as_deref(), Some("3"));
        assert_eq!(e.icon_kind, "index");
        assert_eq!(e.color, Some([200, 150, 0]));
        // scalars present
        let sc = |k: &str| e.scalars.iter().find(|s| s.key == k).unwrap();
        assert!(sc("base_influence").present);
        assert_eq!(sc("base_influence").value, "10");
        assert!(sc("influence_from_dev_modifier").present);
        assert!(!sc("contributes_to_curia_treasury").present);
        // modifier blocks
        let mb = |k: &str| e.modifier_blocks.iter().find(|m| m.name == k).unwrap();
        let happy = mb("country_modifier_happy");
        assert!(happy.present && happy.flat);
        assert_eq!(happy.rows.len(), 2);
        assert_eq!(happy.rows[0].key, "global_tax_modifier");
        assert_eq!(happy.rows[0].value, "0.2");
        assert!(mb("land_ownership_modifier").present);
        assert!(mb("land_ownership_modifier").rows.is_empty());
        // trigger present
        assert!(e.script_blocks.iter().find(|s| s.name == "trigger").unwrap().present);
        // lists
        assert_eq!(e.privileges, vec!["estate_test_land_rights", "estate_test_more_rights"]);
        assert_eq!(e.agendas, vec!["estate_test_do_x"]);
        // preserve-unknown: influence_modifier surfaced raw
        assert!(e.raw_extra.contains(&"influence_modifier".to_string()));
        // raw is the braces-inclusive block body span.
        assert!(e.raw.starts_with('{') && e.raw.ends_with('}'));
        assert!(e.raw.contains("estate_test_land_rights"));
    }

    #[test]
    fn parses_privilege_typed_and_raw() {
        let (_root, vfs) = estate_fixture("parse_priv");
        let loc = LocStore::from_pairs(&[]);
        let data = load(&vfs, &loc);
        let p = data.privileges.iter().find(|p| p.key == "estate_test_land_rights").unwrap();
        assert_eq!(p.kind, "privilege");
        assert_eq!(p.icon.as_deref(), Some("privilege_grant_autonomy"));
        assert_eq!(p.icon_kind, "sprite");
        let sc = |k: &str| p.scalars.iter().find(|s| s.key == k).unwrap();
        assert_eq!(sc("land_share").value, "5");
        assert_eq!(sc("max_absolutism").value, "-5");
        let benefits = p.modifier_blocks.iter().find(|m| m.name == "benefits").unwrap();
        assert!(benefits.present && benefits.flat);
        assert_eq!(benefits.rows[0].key, "governing_capacity");
        // can_select trigger present, is_valid present (empty), can_revoke absent
        let sb = |k: &str| p.script_blocks.iter().find(|s| s.name == k).unwrap();
        assert!(sb("can_select").present);
        assert!(sb("is_valid").present);
        assert!(!sb("can_revoke").present);
        assert_eq!(sb("can_select").registry, "triggers");
        assert_eq!(sb("on_granted").registry, "effects");
        // conditional_modifier + ai_will_do preserved raw
        assert!(p.raw_extra.contains(&"conditional_modifier".to_string()));
        assert!(p.raw_extra.contains(&"ai_will_do".to_string()));
    }

    #[test]
    fn estate_scalar_edit_is_byte_surgical() {
        let out = apply(
            ESTATE_SRC.as_bytes(),
            &Edit::SetScalar {
                path: vec!["estate_test".into(), "base_influence".into()],
                value: "25".into(),
                quoted: false,
            },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("base_influence = 25"));
        // untouched siblings round-trip
        assert!(text.contains("icon = 3"));
        assert!(text.contains("global_tax_modifier = 0.2"));
        assert!(text.contains("influence = 20"));
    }

    #[test]
    fn estate_modifier_block_rewrite_is_byte_surgical() {
        // Rewrite country_modifier_happy from typed rows; only that block changes.
        let out = apply(
            ESTATE_SRC.as_bytes(),
            &Edit::SetBlock {
                path: vec!["estate_test".into(), "country_modifier_happy".into()],
                value: "global_tax_modifier = 0.3 devotion = 0.5".into(),
            },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("global_tax_modifier = 0.3"));
        // neutral/angry blocks + trigger untouched
        assert!(text.contains("global_unrest = 2"));
        assert!(text.contains("religion_group = christian"));
        assert!(text.contains("influence = 20"));
    }

    #[test]
    fn privilege_trigger_edit_is_byte_surgical() {
        let out = apply(
            PRIV_SRC.as_bytes(),
            &Edit::SetScalar {
                path: vec!["estate_test_land_rights".into(), "can_select".into(), "tag".into()],
                value: "DAN".into(),
                quoted: false,
            },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("tag = DAN"));
        assert!(text.contains("land_share = 5"));
        assert!(text.contains("governing_capacity = 100"));
    }

    #[test]
    fn agenda_modifier_round_trip() {
        let src = "estate_test_do_x = {\n\tcan_select = {\n\t}\n\tmodifier = {\n\t\tprestige = 1\n\t}\n}\n";
        let out = apply(
            src.as_bytes(),
            &Edit::SetBlock {
                path: vec!["estate_test_do_x".into(), "modifier".into()],
                value: "prestige = 2 legitimacy = 1".into(),
            },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("prestige = 2 legitimacy = 1"));
        assert!(text.contains("can_select = {"));
    }

    #[test]
    fn scaffolds_parse_back_with_required_keys() {
        // Estate scaffold.
        let s = scaffold_estate("estate_mytest");
        assert_eq!(s.file, ESTATES_PROJECT_FILE);
        let b = paradox::parse(&s.text);
        let e = b.get_block("estate_mytest").expect("estate scaffold parses");
        assert!(e.get_scalar("icon").is_some());
        assert!(e.get_block("trigger").is_some());
        assert!(e.get_block("country_modifier_happy").is_some());
        assert!(e.get_block("privileges").is_some());
        assert!(e.get_block("agendas").is_some());
        assert_eq!(e.get_scalar("base_influence"), Some("20"));

        // Privilege scaffold.
        let p = scaffold_privilege("estate_test_my_priv");
        let pb = paradox::parse(&p.text);
        let pe = pb.get_block("estate_test_my_priv").expect("privilege scaffold parses");
        assert_eq!(pe.get_scalar("icon"), Some("privilege_grant_autonomy"));
        assert!(pe.get_block("can_select").is_some());
        assert!(pe.get_block("benefits").is_some());
        assert!(pe.get_scalar("land_share").is_some());

        // Agenda scaffold references its estate in the completed effect.
        let a = scaffold_agenda("estate_test_my_agenda", "estate_test");
        let ab = paradox::parse(&a.text);
        let ae = ab.get_block("estate_test_my_agenda").expect("agenda scaffold parses");
        assert!(ae.get_block("task_completed_effect").is_some());
        assert!(a.text.contains("estate = estate_test"));
    }

    #[test]
    fn scaffold_create_then_delete_is_identity() {
        let base = "existing = {\n\ticon = 1\n}\n";
        let s = scaffold_privilege("brand_new_priv");
        let appended = apply(base.as_bytes(), &Edit::Append { text: s.text }).unwrap();
        assert!(String::from_utf8_lossy(&appended).contains("brand_new_priv = {"));
        let deleted = apply(
            &appended,
            &Edit::RemoveStatement {
                block_path: vec![],
                key: "brand_new_priv".into(),
                value: None,
            },
        )
        .unwrap();
        assert_eq!(deleted, base.as_bytes(), "create then delete restores source");
    }

    #[test]
    fn create_privilege_adds_to_estate_list() {
        // Creating a privilege registers it in the owning estate's privileges list.
        let out = apply(
            ESTATE_SRC.as_bytes(),
            &Edit::AddId {
                list_path: vec!["estate_test".into(), "privileges".into()],
                id: "estate_test_new_priv".into(),
            },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("estate_test_new_priv"));
        // existing members untouched
        assert!(text.contains("estate_test_land_rights"));
        assert!(text.contains("estate_test_more_rights"));
    }

    // --- Country-history starting privileges ---------------------------------

    fn country_fixture(name: &str, history: &str) -> (std::path::PathBuf, Vfs) {
        synthetic(
            name,
            &[
                ("common/estates/00_test.txt", ESTATE_SRC),
                ("common/estate_privileges/00_test.txt", PRIV_SRC),
                (
                    &format!("history/countries/TST - Test.txt"),
                    history,
                ),
            ],
        )
    }

    #[test]
    fn country_starting_privileges_top_level_and_dated() {
        let history = "government = monarchy\n\
set_estate_privilege = estate_test_land_rights\n\
1450.1.1 = {\n\tset_estate_privilege = estate_test_more_rights\n}\n\
1600.1.1 = {\n\tset_estate_privilege = estate_test_future\n}\n";
        let (_root, vfs) = country_fixture("country_start", history);
        let loc = LocStore::from_pairs(&[]);
        // At 1500, the 1450 dated grant is in, the 1600 one is not.
        let ce = country_estates(&vfs, &loc, "TST", (1500, 1, 1)).unwrap();
        let keys: Vec<&str> = ce.starting.iter().map(|s| s.privilege.as_str()).collect();
        assert!(keys.contains(&"estate_test_land_rights"));
        assert!(keys.contains(&"estate_test_more_rights"));
        assert!(!keys.contains(&"estate_test_future"), "1600 grant excluded at 1500");
        // estate resolution from the catalog
        let lr = ce.starting.iter().find(|s| s.privilege == "estate_test_land_rights").unwrap();
        assert_eq!(lr.estate.as_deref(), Some("estate_test"));
        assert!(lr.date.is_none(), "top-level grant has no date");
        let mr = ce.starting.iter().find(|s| s.privilege == "estate_test_more_rights").unwrap();
        assert_eq!(mr.date.as_deref(), Some("1450.1.1"));
        // catalog carries the estate + its privileges
        let est = ce.estates.iter().find(|e| e.key == "estate_test").unwrap();
        assert_eq!(est.privileges.len(), 2);
    }

    #[test]
    fn country_history_privilege_add_remove_round_trip_top_level() {
        // Add a top-level set_estate_privilege, then remove it → identity.
        let history = "government = monarchy\ncapital = 1\n";
        let added = apply(
            history.as_bytes(),
            &Edit::InsertStatement {
                block_path: vec![],
                statement: "set_estate_privilege = estate_test_land_rights".into(),
            },
        )
        .unwrap();
        assert!(String::from_utf8_lossy(&added).contains("set_estate_privilege = estate_test_land_rights"));
        let removed = apply(
            &added,
            &Edit::RemoveStatement {
                block_path: vec![],
                key: SET_PRIVILEGE.into(),
                value: Some("estate_test_land_rights".into()),
            },
        )
        .unwrap();
        assert_eq!(removed, history.as_bytes(), "add then remove restores history");
    }

    #[test]
    fn country_history_privilege_dated_insert() {
        let history = "government = monarchy\n";
        let added = apply(
            history.as_bytes(),
            &Edit::InsertDatedBlock {
                date: "1500.1.1".into(),
                statement: "1500.1.1 = { set_estate_privilege = estate_test_land_rights }".into(),
            },
        )
        .unwrap();
        let text = String::from_utf8(added).unwrap();
        assert!(text.contains("1500.1.1 = {"));
        assert!(text.contains("set_estate_privilege = estate_test_land_rights"));
    }

    // --- Real install ---------------------------------------------------------

    #[test]
    fn vanilla_loads_estates_privileges_agendas() {
        if !install_present() {
            return;
        }
        let vfs = Vfs::new(INSTALL, None).unwrap();
        let loc = loc::build(&vfs);
        let data = load(&vfs, &loc);
        assert!(data.estates.len() >= 10, "estates: {}", data.estates.len());
        assert!(data.privileges.len() > 100, "privileges: {}", data.privileges.len());
        assert!(data.agendas.len() > 100, "agendas: {}", data.agendas.len());
        // estate_church is fully modeled.
        let church = data.estates.iter().find(|e| e.key == "estate_church").expect("estate_church");
        assert_eq!(church.icon.as_deref(), Some("1"));
        assert_eq!(church.color, Some([200, 150, 0]));
        assert!(church.script_blocks.iter().find(|s| s.name == "trigger").unwrap().present);
        let happy = church.modifier_blocks.iter().find(|m| m.name == "country_modifier_happy").unwrap();
        assert!(happy.present && happy.flat && !happy.rows.is_empty());
        assert!(church.privileges.contains(&"estate_church_land_rights".to_string()));
        // influence_modifier/loyalty_modifier/custom_name preserved raw.
        assert!(church.raw_extra.contains(&"influence_modifier".to_string()));
        assert!(church.raw_extra.contains(&"loyalty_modifier".to_string()));
        // A privilege loads with typed scalars.
        let lr = data.privileges.iter().find(|p| p.key == "estate_church_land_rights").expect("land_rights");
        assert_eq!(lr.icon_kind, "sprite");
        assert!(lr.scalars.iter().find(|s| s.key == "land_share").unwrap().present);
        assert!(lr.origin == "base");
    }

    #[test]
    fn vanilla_privilege_holders_scan() {
        if !install_present() {
            return;
        }
        let vfs = Vfs::new(INSTALL, None).unwrap();
        let loc = loc::build(&vfs);
        // BYZ grants estate_church_union_of_the_churches at start (verified in file).
        let holders = privilege_holders(&vfs, &loc, "estate_church_union_of_the_churches");
        assert!(
            holders.iter().any(|h| h.tag == "BYZ"),
            "expected BYZ among union_of_the_churches holders, got {:?}",
            holders.iter().map(|h| &h.tag).collect::<Vec<_>>()
        );
        // Poland grants the golden liberty (in a 1444.10.1 dated block).
        let gl = privilege_holders(&vfs, &loc, "estate_nobles_golden_liberty");
        assert!(gl.iter().any(|h| h.tag == "POL"), "expected POL golden_liberty holder");
        let pol = gl.iter().find(|h| h.tag == "POL").unwrap();
        assert!(pol.date.is_some(), "POL grants it in a dated block");
    }

    #[test]
    fn vanilla_country_estates_byz() {
        if !install_present() {
            return;
        }
        let vfs = Vfs::new(INSTALL, None).unwrap();
        let loc = loc::build(&vfs);
        let ce = country_estates(&vfs, &loc, "BYZ", crate::date::DEFAULT_START).unwrap();
        let keys: Vec<&str> = ce.starting.iter().map(|s| s.privilege.as_str()).collect();
        assert!(keys.contains(&"estate_church_union_of_the_churches"), "BYZ starting: {keys:?}");
        // The church privilege resolves to estate_church via the catalog.
        let u = ce.starting.iter().find(|s| s.privilege == "estate_church_union_of_the_churches").unwrap();
        assert_eq!(u.estate.as_deref(), Some("estate_church"));
        assert!(ce.estates.iter().any(|e| e.key == "estate_church"));
    }

    #[test]
    fn anbennar_estates_smoke() {
        if !install_present() || !Path::new(ANBENNAR).is_dir() {
            return;
        }
        let vfs = Vfs::new(INSTALL, Some(ANBENNAR)).unwrap();
        let loc = loc::build(&vfs);
        let data = load(&vfs, &loc);
        assert!(!data.estates.is_empty());
        assert!(!data.privileges.is_empty());
        assert!(!data.agendas.is_empty());
        // Anbennar contributes mod-origin estate content.
        let mod_estate = data.estates.iter().any(|e| e.origin == "mod")
            || data.privileges.iter().any(|p| p.origin == "mod")
            || data.agendas.iter().any(|a| a.origin == "mod");
        assert!(mod_estate, "Anbennar should contribute mod-origin estate content");
        // Every object round-trips its raw block span (no parse crash) and a
        // sample privilege round-trips a scalar edit.
        if let Some(p) = data.privileges.iter().find(|p| p.origin == "mod" && !p.scalars.iter().all(|s| !s.present)) {
            let bytes = vfs.read(&p.file).unwrap();
            let out = apply(
                &bytes,
                &Edit::SetScalar {
                    path: vec![p.key.clone(), "loyalty".into()],
                    value: "0.1".into(),
                    quoted: false,
                },
            );
            // Either the key is present (edit applies) or absent (edit errors) —
            // both are fine; we only assert no panic and, when present, success.
            if p.scalars.iter().any(|s| s.key == "loyalty" && s.present) {
                assert!(out.is_ok(), "mod privilege loyalty edit should apply");
            }
        }
        println!(
            "[estates:anbennar] {} estates, {} privileges, {} agendas",
            data.estates.len(),
            data.privileges.len(),
            data.agendas.len()
        );
    }
}
