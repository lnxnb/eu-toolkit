//! Sprint 21 — Rebel factions subsystem (View ▸ Rebels).
//!
//! `common/rebel_types/*.txt` holds one faction block per top-level key
//! (`nationalist_rebels`, `anti_tax_rebels`, …). The shape mirrors the Sprint 20
//! estates pattern (directory-loaded registry + typed-key editor) but is a single
//! kind. Verified key inventory (all 50 vanilla files scanned):
//!
//! * **Enum scalars** (small, knowable value sets — dropdowns):
//!   `area` (nation / nation_rebel_tag / nation_religion),
//!   `government` (any / monarchy / republic / theocracy / anti),
//!   `defection` + `independence` (none / culture / culture_group / religion /
//!   any / nation_rebel_tag), `gfx_type` (culture_province / culture_owner).
//! * **Bool scalars** (toggles): `unit_transfer`, `will_relocate`, `resilient`,
//!   `reinforcing`, `general`, `smart`, `dynasty`, `disband_on_leader_death`,
//!   `revolutionary`, and the `handle_action_*` set.
//! * **Number scalars** (steppers): `defect_delay` (int), `artillery`,
//!   `infantry`, `cavalry`, `morale` (unit composition weights).
//! * **String scalars**: `religion` (a religion key), `demands_description` (a loc
//!   key reference).
//! * **`color`** block (edited like estates).
//! * **Trigger blocks** (14.2 tree): `siege_won_trigger`, `can_negotiate_trigger`,
//!   `can_enforce_trigger`.
//! * **Effect blocks** (14.2 tree): `siege_won_effect`, `demands_enforced_effect`.
//! * **Weight blocks** (14.2 tree; leaves are trigger conditions inside
//!   `modifier = { factor … }` rows): `spawn_chance`, `movement_evaluation`.
//! * Everything unmodeled (`raw_extra`, e.g. `has_reform`) round-trips untouched
//!   and is shown read-only.
//!
//! Loc keys are formed from the block key: `<key>_name` (display name, may hold
//! `$vars`), `<key>_title` (short category label), `<key>_desc`. `<key>_title` is
//! the list label; the editor edits `_name`/`_title`/`_desc` as loc overrides.

use crate::date::{self, Date};
use crate::loc::{self, LocStore};
use crate::mod_writer;
use crate::paradox::{self, Block, Value};
use crate::vfs::Vfs;

pub const REBELS_DIR: &str = "common/rebel_types";
pub const REBELS_PROJECT_FILE: &str = "common/rebel_types/zz_eutoolkit_rebel_types.txt";

// ---------------------------------------------------------------------------
// Schema.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum ScalarKind {
    Num,
    Int,
    Bool,
    Enum,
    Str,
}

impl ScalarKind {
    fn as_str(self) -> &'static str {
        match self {
            ScalarKind::Num => "num",
            ScalarKind::Int => "int",
            ScalarKind::Bool => "bool",
            ScalarKind::Enum => "enum",
            ScalarKind::Str => "str",
        }
    }
}

struct ScalarSpec {
    key: &'static str,
    kind: ScalarKind,
    /// Non-empty for `Enum` kinds.
    options: &'static [&'static str],
}

const fn s(key: &'static str, kind: ScalarKind) -> ScalarSpec {
    ScalarSpec { key, kind, options: &[] }
}
const fn e(key: &'static str, options: &'static [&'static str]) -> ScalarSpec {
    ScalarSpec { key, kind: ScalarKind::Enum, options }
}

static SCALARS: &[ScalarSpec] = &[
    // Target / behavior.
    e("area", &["nation", "nation_rebel_tag", "nation_religion"]),
    e("government", &["any", "monarchy", "republic", "theocracy", "anti"]),
    e(
        "defection",
        &["none", "culture", "culture_group", "religion", "any", "nation_rebel_tag"],
    ),
    e(
        "independence",
        &["none", "culture", "culture_group", "religion", "any", "nation_rebel_tag"],
    ),
    e("gfx_type", &["culture_province", "culture_owner"]),
    s("defect_delay", ScalarKind::Int),
    // Flags.
    s("unit_transfer", ScalarKind::Bool),
    s("will_relocate", ScalarKind::Bool),
    s("resilient", ScalarKind::Bool),
    s("reinforcing", ScalarKind::Bool),
    s("general", ScalarKind::Bool),
    s("smart", ScalarKind::Bool),
    s("dynasty", ScalarKind::Bool),
    s("disband_on_leader_death", ScalarKind::Bool),
    s("revolutionary", ScalarKind::Bool),
    s("handle_action_negotiate", ScalarKind::Bool),
    s("handle_action_stability", ScalarKind::Bool),
    s("handle_action_build_core", ScalarKind::Bool),
    s("handle_action_send_missionary", ScalarKind::Bool),
    s("handle_action_change_culture", ScalarKind::Bool),
    // Unit composition (weights that should sum to ~1) + morale.
    s("artillery", ScalarKind::Num),
    s("infantry", ScalarKind::Num),
    s("cavalry", ScalarKind::Num),
    s("morale", ScalarKind::Num),
    // References.
    s("religion", ScalarKind::Str),
    s("demands_description", ScalarKind::Str),
];

/// Trigger blocks (14.2 registry "triggers").
static TRIGGERS: &[&str] = &["siege_won_trigger", "can_negotiate_trigger", "can_enforce_trigger"];
/// Effect blocks (14.2 registry "effects").
static EFFECTS: &[&str] = &["siege_won_effect", "demands_enforced_effect"];
/// Weight/AI blocks; leaves are trigger conditions, so they use the "triggers"
/// registry for key suggestions (the tree editor preserves `factor`/`modifier`
/// rows raw regardless).
static WEIGHTS: &[&str] = &["spawn_chance", "movement_evaluation"];

// ---------------------------------------------------------------------------
// Payload types (serialize camelCase; mirrored by src/lib/rebels.ts).
// ---------------------------------------------------------------------------

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Scalar {
    pub key: String,
    pub kind: String,
    pub present: bool,
    pub value: String,
    /// Enum option set (empty for non-enum kinds).
    pub options: Vec<String>,
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ScriptBlockRef {
    pub name: String,
    /// `triggers` | `effects` — which known-key registry the tree editor uses.
    pub registry: String,
    pub present: bool,
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RebelFaction {
    pub key: String,
    pub file: String,
    pub origin: String,
    /// List-display label (resolved `<key>_title`, else prettified key).
    pub title: String,
    pub name_key: String,
    pub name_loc: Option<String>,
    pub title_key: String,
    pub title_loc: Option<String>,
    pub desc_key: String,
    pub desc_loc: Option<String>,
    pub color: Option<[u8; 3]>,
    pub scalars: Vec<Scalar>,
    pub script_blocks: Vec<ScriptBlockRef>,
    pub raw_extra: Vec<String>,
    pub raw: String,
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RebelsData {
    pub factions: Vec<RebelFaction>,
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

fn parse_faction(
    file_bytes: &[u8],
    key: &str,
    b: &Block,
    loc: &LocStore,
    file: &str,
    origin: &str,
) -> RebelFaction {
    let color = b.get_block("color").and_then(paradox::color_from_block);

    let scalars = SCALARS
        .iter()
        .map(|spec| {
            let val = b.get_scalar(spec.key).map(|s| s.trim().to_string());
            Scalar {
                key: spec.key.to_string(),
                kind: spec.kind.as_str().to_string(),
                present: val.is_some(),
                value: val.unwrap_or_default(),
                options: spec.options.iter().map(|o| o.to_string()).collect(),
            }
        })
        .collect();

    let mut script_blocks: Vec<ScriptBlockRef> = Vec::new();
    for name in TRIGGERS {
        script_blocks.push(ScriptBlockRef {
            name: name.to_string(),
            registry: "triggers".to_string(),
            present: b.get_block(name).is_some(),
        });
    }
    for name in EFFECTS {
        script_blocks.push(ScriptBlockRef {
            name: name.to_string(),
            registry: "effects".to_string(),
            present: b.get_block(name).is_some(),
        });
    }
    for name in WEIGHTS {
        script_blocks.push(ScriptBlockRef {
            name: name.to_string(),
            registry: "triggers".to_string(),
            present: b.get_block(name).is_some(),
        });
    }

    // Preserve-unknown: every top-level key not modeled above.
    let mut modeled: std::collections::HashSet<&str> = std::collections::HashSet::new();
    modeled.insert("color");
    for spec in SCALARS {
        modeled.insert(spec.key);
    }
    for n in TRIGGERS.iter().chain(EFFECTS).chain(WEIGHTS) {
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

    let name_key = format!("{key}_name");
    let title_key = format!("{key}_title");
    let desc_key = format!("{key}_desc");
    let title = loc
        .get(&title_key)
        .map(str::to_string)
        .unwrap_or_else(|| loc::prettify(key));

    RebelFaction {
        key: key.to_string(),
        file: file.to_string(),
        origin: origin.to_string(),
        title,
        name_loc: loc.get(&name_key).map(str::to_string),
        name_key,
        title_loc: loc.get(&title_key).map(str::to_string),
        title_key,
        desc_loc: loc.get(&desc_key).map(str::to_string),
        desc_key,
        color,
        scalars,
        script_blocks,
        raw_extra,
        raw,
    }
}

pub fn load(vfs: &Vfs, loc: &LocStore) -> RebelsData {
    let mut factions = Vec::new();
    for (name, path) in vfs.list_dir(REBELS_DIR) {
        if !name.to_lowercase().ends_with(".txt") {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let origin = origin_of(vfs, &path);
        let block = paradox::parse(&String::from_utf8_lossy(&bytes));
        let rel = format!("{REBELS_DIR}/{name}");
        for (key, b) in block.key_blocks() {
            factions.push(parse_faction(&bytes, key, b, loc, &rel, origin));
        }
    }
    RebelsData {
        factions,
        project_file: REBELS_PROJECT_FILE.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Scaffold (modeled on a minimal vanilla sibling; unit-tested to parse back).
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
    /// Loc keys to queue as `LocOverride`s (name/title/desc/demand).
    pub loc_entries: Vec<LocEntry>,
}

/// Minimal game-valid rebel type (reduced from `anti_tax_rebels`).
pub fn scaffold_faction(key: &str) -> Scaffold {
    let text = format!(
        "{key} = {{\n\
\tcolor = {{ 150 150 150 }}\n\
\tarea = nation\n\
\tgovernment = any\n\
\tdefection = none\n\
\tindependence = none\n\
\tdefect_delay = 120\n\
\tgfx_type = culture_province\n\
\tunit_transfer = no\n\
\twill_relocate = yes\n\
\tresilient = no\n\
\treinforcing = yes\n\
\tgeneral = yes\n\
\tsmart = yes\n\
\tartillery = 0.0\n\
\tinfantry = 0.7\n\
\tcavalry = 0.3\n\
\tmorale = 1.0\n\
\thandle_action_negotiate = yes\n\
\thandle_action_stability = yes\n\
\thandle_action_build_core = yes\n\
\thandle_action_send_missionary = yes\n\
\tspawn_chance = {{\n\t\tfactor = 1\n\t}}\n\
\tmovement_evaluation = {{\n\t\tfactor = 1\n\t}}\n\
\tsiege_won_trigger = {{\n\t\talways = yes\n\t}}\n\
\tsiege_won_effect = {{\n\t\tadd_local_autonomy = 10\n\t}}\n\
\tcan_negotiate_trigger = {{\n\t\talways = yes\n\t}}\n\
\tcan_enforce_trigger = {{\n\t\talways = yes\n\t}}\n\
\tdemands_description = \"{key}_demand\"\n\
\tdemands_enforced_effect = {{\n\t\tadd_prestige = -10\n\t}}\n\
}}"
    );
    let pretty = loc::prettify(key);
    Scaffold {
        key: key.to_string(),
        file: REBELS_PROJECT_FILE.to_string(),
        text,
        loc_entries: vec![
            LocEntry { key: format!("{key}_name"), value: pretty.clone() },
            LocEntry { key: format!("{key}_title"), value: pretty.clone() },
            LocEntry { key: format!("{key}_desc"), value: format!("{pretty} rebels.") },
            LocEntry { key: format!("{key}_demand"), value: format!("{pretty} Demands") },
        ],
    }
}

// ---------------------------------------------------------------------------
// Context: provinces with a revolt of a faction active at a date.
// ---------------------------------------------------------------------------

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RebelProvince {
    pub id: u32,
    pub name: String,
    /// `None` = the revolt is set at the top level (base state); else the dated
    /// block it was (last) set in.
    pub date: Option<String>,
}

/// The `type = <faction>` inside a `revolt = { … }` block value, if any.
fn revolt_type(b: &Block) -> Option<&str> {
    b.get_scalar("type")
}

/// Is the block an empty `revolt = {}` (revolt cleared)?
fn revolt_empty(b: &Block) -> bool {
    b.items.is_empty()
}

/// Scans `history/provinces/*` for provinces whose folded revolt state at `at`
/// is a revolt of `faction`. Folding rule: the top-level `revolt` applies first,
/// then every dated block with date ≤ `at` in file order overrides it; an empty
/// `revolt = {}` clears it. Vanilla has zero active revolts at 1444.11.11.
pub fn rebel_provinces(vfs: &Vfs, loc: &LocStore, faction: &str, at: Date) -> Vec<RebelProvince> {
    let mut out = Vec::new();
    for (name, path) in vfs.list_dir("history/provinces") {
        if !name.to_lowercase().ends_with(".txt") {
            continue;
        }
        let digits: String = name.chars().take_while(|c| c.is_ascii_digit()).collect();
        let Ok(id) = digits.parse::<u32>() else {
            continue;
        };
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let block = paradox::parse(&String::from_utf8_lossy(&bytes));

        // Fold: (is_active, type_matches, date). Track the last revolt statement
        // that applies at `at`, in file order (top level first, then dated ≤ at).
        let mut active: Option<(bool, Option<String>)> = None; // (matches_faction, date)
        if let Some(rb) = block.get_block("revolt") {
            // A present top-level revolt sets the baseline: active iff it is
            // non-empty and its type is this faction.
            active = Some((!revolt_empty(rb) && revolt_type(rb) == Some(faction), None));
        }
        for (k, v) in &block.items {
            let (Some(k), Value::Block(db)) = (k.as_deref(), v) else {
                continue;
            };
            let Some(d) = date::parse_date(k) else { continue };
            if d > at {
                continue;
            }
            if let Some(rb) = db.get_block("revolt") {
                if revolt_empty(rb) {
                    active = Some((false, Some(k.to_string())));
                } else {
                    active = Some((revolt_type(rb) == Some(faction), Some(k.to_string())));
                }
            }
        }

        if let Some((true, date)) = active {
            let prov_name = loc
                .get(&format!("PROV{id}"))
                .map(str::to_string)
                .unwrap_or_else(|| province_name_from_file(&name));
            out.push(RebelProvince { id, name: prov_name, date });
        }
    }
    out.sort_by_key(|p| p.id);
    out
}

/// `"1016 - Ha Tinh.txt"` -> `"Ha Tinh"`; `"1-Uppland.txt"` -> `"Uppland"`.
fn province_name_from_file(file: &str) -> String {
    let stem = file.strip_suffix(".txt").unwrap_or(file);
    if let Some((_, rest)) = stem.split_once(" - ") {
        return rest.trim().to_string();
    }
    if let Some((_, rest)) = stem.split_once('-') {
        return rest.trim().to_string();
    }
    stem.to_string()
}

// ---------------------------------------------------------------------------
// Commands.
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_rebels(install_path: String, mod_path: Option<String>) -> Result<RebelsData, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    let loc = loc::store(&vfs, &install_path, mod_path.as_deref());
    Ok(load(&vfs, &loc))
}

#[tauri::command]
pub fn scaffold_rebel_faction(key: String) -> Result<Scaffold, String> {
    Ok(scaffold_faction(&key))
}

#[tauri::command]
pub fn get_rebel_provinces(
    install_path: String,
    mod_path: Option<String>,
    date: Option<String>,
    faction: String,
) -> Result<Vec<RebelProvince>, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    let loc = loc::store(&vfs, &install_path, mod_path.as_deref());
    let at = crate::bookmarks::resolve_date(&vfs, date.as_deref())?;
    Ok(rebel_provinces(&vfs, &loc, &faction, at))
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
        let root = std::env::temp_dir().join(format!("eu_toolkit_rebels_test_{name}"));
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

    const REBEL_SRC: &str = "\
nationalist_rebels = {\n\
\tcolor = { 25 180 25 }\n\
\tarea = nation_rebel_tag\n\
\tgovernment = any\n\
\tdefection = nation_rebel_tag\n\
\tindependence = nation_rebel_tag\n\
\tdefect_delay = 60\n\
\tunit_transfer = yes\n\
\tgfx_type = culture_province\n\
\twill_relocate = yes\n\
\tresilient = no\n\
\treinforcing = yes\n\
\tgeneral = yes\n\
\tsmart = yes\n\
\tartillery = 0.1\n\
\tinfantry = 0.6\n\
\tcavalry = 0.3\n\
\tmorale = 1.0\n\
\thandle_action_negotiate = yes\n\
\tspawn_chance = {\n\t\tfactor = 10\n\t\tmodifier = { factor = 5 is_core = owner }\n\t}\n\
\tmovement_evaluation = {\n\t\tfactor = 1\n\t}\n\
\tsiege_won_trigger = {\n\t\tculture = REB\n\t}\n\
\tsiege_won_effect = {\n\t\tadd_nationalism = 10\n\t}\n\
\tcan_negotiate_trigger = {\n\t\tis_at_war = no\n\t}\n\
\tcan_enforce_trigger = {\n\t\talways = yes\n\t}\n\
\tdemands_description = \"nationalist_rebels_demand\"\n\
\tdemands_enforced_effect = {\n\t\tadd_prestige = -10\n\t}\n\
\tsome_unknown_key = yes\n\
}\n";

    fn rebel_fixture(name: &str) -> (std::path::PathBuf, Vfs) {
        synthetic(name, &[("common/rebel_types/00_test.txt", REBEL_SRC)])
    }

    #[test]
    fn parses_rebel_scalars_enums_scripts_and_raw() {
        let (_root, vfs) = rebel_fixture("parse");
        let loc = LocStore::from_pairs(&[
            ("nationalist_rebels_title", "Separatists"),
            ("nationalist_rebels_name", "$INDEP$ Separatists"),
        ]);
        let data = load(&vfs, &loc);
        assert_eq!(data.factions.len(), 1);
        let f = &data.factions[0];
        assert_eq!(f.key, "nationalist_rebels");
        assert_eq!(f.title, "Separatists");
        assert_eq!(f.name_loc.as_deref(), Some("$INDEP$ Separatists"));
        assert_eq!(f.color, Some([25, 180, 25]));
        let sc = |k: &str| f.scalars.iter().find(|s| s.key == k).unwrap();
        // Enum scalar carries options + present value.
        let area = sc("area");
        assert_eq!(area.kind, "enum");
        assert_eq!(area.value, "nation_rebel_tag");
        assert!(area.options.contains(&"nation".to_string()));
        assert_eq!(sc("defect_delay").kind, "int");
        assert_eq!(sc("defect_delay").value, "60");
        assert_eq!(sc("morale").kind, "num");
        assert!(sc("unit_transfer").present && sc("unit_transfer").kind == "bool");
        assert_eq!(sc("unit_transfer").value, "yes");
        assert!(sc("reinforcing").present); // reinforcing IS present in the fixture
        assert!(!sc("disband_on_leader_death").present); // absent bool
        // Script blocks present + registries.
        let sb = |k: &str| f.script_blocks.iter().find(|s| s.name == k).unwrap();
        assert!(sb("siege_won_trigger").present && sb("siege_won_trigger").registry == "triggers");
        assert!(sb("demands_enforced_effect").present && sb("demands_enforced_effect").registry == "effects");
        assert!(sb("spawn_chance").present && sb("spawn_chance").registry == "triggers");
        assert!(sb("movement_evaluation").present);
        // Preserve-unknown.
        assert!(f.raw_extra.contains(&"some_unknown_key".to_string()));
        assert!(f.raw.starts_with('{') && f.raw.ends_with('}'));
    }

    #[test]
    fn rebel_scalar_edit_is_byte_surgical() {
        let out = apply(
            REBEL_SRC.as_bytes(),
            &Edit::SetScalar {
                path: vec!["nationalist_rebels".into(), "defect_delay".into()],
                value: "48".into(),
                quoted: false,
            },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("defect_delay = 48"));
        assert!(text.contains("color = { 25 180 25 }"));
        assert!(text.contains("add_nationalism = 10"));
    }

    #[test]
    fn rebel_enum_and_color_edit_round_trip() {
        let out = apply(
            REBEL_SRC.as_bytes(),
            &Edit::SetScalar {
                path: vec!["nationalist_rebels".into(), "government".into()],
                value: "republic".into(),
                quoted: false,
            },
        )
        .unwrap();
        let out = apply(
            &out,
            &Edit::SetBlock {
                path: vec!["nationalist_rebels".into(), "color".into()],
                value: "10 20 30".into(),
            },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("government = republic"));
        assert!(text.contains("color = { 10 20 30 }"));
        assert!(text.contains("area = nation_rebel_tag"));
    }

    #[test]
    fn rebel_trigger_edit_is_byte_surgical() {
        let out = apply(
            REBEL_SRC.as_bytes(),
            &Edit::SetScalar {
                path: vec!["nationalist_rebels".into(), "can_negotiate_trigger".into(), "is_at_war".into()],
                value: "yes".into(),
                quoted: false,
            },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("is_at_war = yes"));
        assert!(text.contains("demands_description = \"nationalist_rebels_demand\""));
    }

    #[test]
    fn scaffold_parses_back_with_required_keys() {
        let s = scaffold_faction("my_rebels");
        assert_eq!(s.file, REBELS_PROJECT_FILE);
        let b = paradox::parse(&s.text);
        let f = b.get_block("my_rebels").expect("scaffold parses");
        assert!(f.get_block("color").is_some());
        assert_eq!(f.get_scalar("area"), Some("nation"));
        assert_eq!(f.get_scalar("government"), Some("any"));
        assert!(f.get_block("spawn_chance").is_some());
        assert!(f.get_block("siege_won_effect").is_some());
        assert!(f.get_block("demands_enforced_effect").is_some());
        assert!(f.get_block("can_enforce_trigger").is_some());
        assert_eq!(f.get_scalar("demands_description"), Some("my_rebels_demand"));
        // Loc entries include name/title/desc/demand.
        let keys: Vec<&str> = s.loc_entries.iter().map(|e| e.key.as_str()).collect();
        assert!(keys.contains(&"my_rebels_name"));
        assert!(keys.contains(&"my_rebels_title"));
        assert!(keys.contains(&"my_rebels_desc"));
        assert!(keys.contains(&"my_rebels_demand"));
    }

    #[test]
    fn scaffold_create_then_delete_is_identity() {
        let base = "existing_rebels = {\n\tarea = nation\n}\n";
        let s = scaffold_faction("brand_new_rebels");
        let appended = apply(base.as_bytes(), &Edit::Append { text: s.text }).unwrap();
        assert!(String::from_utf8_lossy(&appended).contains("brand_new_rebels = {"));
        let deleted = apply(
            &appended,
            &Edit::RemoveStatement {
                block_path: vec![],
                key: "brand_new_rebels".into(),
                value: None,
            },
        )
        .unwrap();
        assert_eq!(deleted, base.as_bytes(), "create then delete restores source");
    }

    // --- Province revolt round-trips (top-level + dated) ----------------------

    #[test]
    fn province_revolt_add_edit_remove_top_level() {
        let hist = "owner = SWE\ncontroller = SWE\nculture = swedish\n";
        // Add a top-level revolt block.
        let added = apply(
            hist.as_bytes(),
            &Edit::InsertStatement {
                block_path: vec![],
                statement: "revolt = { type = pretender_rebels size = 1 leader = \"Karl\" }".into(),
            },
        )
        .unwrap();
        let atext = String::from_utf8_lossy(&added);
        assert!(atext.contains("revolt = { type = pretender_rebels size = 1 leader = \"Karl\" }"));
        // Edit its size.
        let edited = apply(
            &added,
            &Edit::SetScalar {
                path: vec!["revolt".into(), "size".into()],
                value: "3".into(),
                quoted: false,
            },
        )
        .unwrap();
        assert!(String::from_utf8_lossy(&edited).contains("size = 3"));
        // Remove it → identity.
        let removed = apply(
            &added,
            &Edit::RemoveStatement {
                block_path: vec![],
                key: "revolt".into(),
                value: None,
            },
        )
        .unwrap();
        assert_eq!(removed, hist.as_bytes(), "add then remove restores history");
    }

    #[test]
    fn province_revolt_dated_insert() {
        let hist = "owner = SWE\n";
        let added = apply(
            hist.as_bytes(),
            &Edit::InsertDatedBlock {
                date: "1500.1.1".into(),
                statement: "1500.1.1 = { revolt = { type = nationalist_rebels size = 2 } controller = REB }".into(),
            },
        )
        .unwrap();
        let text = String::from_utf8(added).unwrap();
        assert!(text.contains("1500.1.1 = {"));
        assert!(text.contains("revolt = { type = nationalist_rebels size = 2 }"));
    }

    // --- Context scan ---------------------------------------------------------

    #[test]
    fn rebel_provinces_folds_active_at_date() {
        // Province 1: revolt set 1436, cleared 1438 → not active at 1444.
        // Province 2: revolt set 1450 → active at 1500 only.
        // Province 3: top-level revolt of the faction → active at start.
        let (_root, vfs) = synthetic(
            "context",
            &[
                (
                    "history/provinces/1 - Uppland.txt",
                    "owner = SWE\n1436.4.28 = { revolt = { type = pretender_rebels size = 1 } controller = REB }\n1438.3.6 = { revolt = {} controller = SWE }\n",
                ),
                (
                    "history/provinces/2 - Genoa.txt",
                    "owner = GEN\n1450.1.1 = { revolt = { type = pretender_rebels size = 2 } controller = REB }\n",
                ),
                (
                    "history/provinces/3 - Rebelburg.txt",
                    "owner = FRA\nrevolt = { type = pretender_rebels size = 1 }\n",
                ),
            ],
        );
        let loc = LocStore::from_pairs(&[("PROV3", "Rebelburg")]);
        // At the vanilla start: only province 3 (top-level) is active.
        let at_start = rebel_provinces(&vfs, &loc, "pretender_rebels", (1444, 11, 11));
        let ids: Vec<u32> = at_start.iter().map(|p| p.id).collect();
        assert_eq!(ids, vec![3], "only the top-level revolt is active at start");
        assert_eq!(at_start[0].name, "Rebelburg");
        assert!(at_start[0].date.is_none());
        // At 1500: provinces 2 and 3 active (1 was cleared, 2 now set).
        let at_1500 = rebel_provinces(&vfs, &loc, "pretender_rebels", (1500, 1, 1));
        let ids: Vec<u32> = at_1500.iter().map(|p| p.id).collect();
        assert_eq!(ids, vec![2, 3]);
        let p2 = at_1500.iter().find(|p| p.id == 2).unwrap();
        assert_eq!(p2.date.as_deref(), Some("1450.1.1"));
        // A different faction matches nothing.
        assert!(rebel_provinces(&vfs, &loc, "nationalist_rebels", (1500, 1, 1)).is_empty());
    }

    // --- Real install ---------------------------------------------------------

    #[test]
    fn vanilla_loads_rebel_types() {
        if !install_present() {
            return;
        }
        let vfs = Vfs::new(INSTALL, None).unwrap();
        let loc = loc::build(&vfs);
        let data = load(&vfs, &loc);
        assert!(data.factions.len() >= 40, "factions: {}", data.factions.len());
        let nat = data.factions.iter().find(|f| f.key == "nationalist_rebels").expect("nationalist_rebels");
        assert_eq!(nat.color, Some([25, 180, 25]));
        assert_eq!(nat.scalars.iter().find(|s| s.key == "area").unwrap().value, "nation_rebel_tag");
        assert!(nat.script_blocks.iter().find(|s| s.name == "demands_enforced_effect").unwrap().present);
        assert!(nat.script_blocks.iter().find(|s| s.name == "spawn_chance").unwrap().present);
        // Title loc resolves.
        assert!(!nat.title.is_empty());
    }

    #[test]
    fn vanilla_active_revolts_at_start() {
        if !install_present() {
            return;
        }
        let vfs = Vfs::new(INSTALL, None).unwrap();
        let loc = loc::build(&vfs);
        // Vanilla ground truth (verified in the province files): Marwar (514),
        // Semien (2771) and Barmer (4506) each start in a nationalist revolt set
        // in a dated block dated ≤ 1444.11.11 and not cleared until later.
        let nat = rebel_provinces(&vfs, &loc, "nationalist_rebels", crate::date::DEFAULT_START);
        let ids: Vec<u32> = nat.iter().map(|p| p.id).collect();
        for expect in [514u32, 2771, 4506] {
            assert!(ids.contains(&expect), "nationalist active-at-start should include {expect}, got {ids:?}");
        }
        // The revolt came from a dated block, so `date` is populated.
        let marwar = nat.iter().find(|p| p.id == 514).unwrap();
        assert_eq!(marwar.date.as_deref(), Some("1444.1.1"));
        // Marwar's revolt is cleared at 1459.1.1, so it is NOT active by 1500.
        let nat_1500 = rebel_provinces(&vfs, &loc, "nationalist_rebels", (1500, 1, 1));
        assert!(!nat_1500.iter().any(|p| p.id == 514), "Marwar cleared by 1500");
        // A faction with no start-date revolt yields an empty set.
        let anti = rebel_provinces(&vfs, &loc, "anti_tax_rebels", crate::date::DEFAULT_START);
        assert!(anti.is_empty(), "anti_tax has no start-date revolts, got {anti:?}");
    }

    #[test]
    fn anbennar_rebels_smoke() {
        if !install_present() || !Path::new(ANBENNAR).is_dir() {
            return;
        }
        let vfs = Vfs::new(INSTALL, Some(ANBENNAR)).unwrap();
        let loc = loc::build(&vfs);
        let data = load(&vfs, &loc);
        assert!(!data.factions.is_empty());
        // Every faction round-trips its raw block span without panicking, and a
        // sample faction round-trips a scalar edit.
        if let Some(f) = data.factions.iter().find(|f| f.scalars.iter().any(|s| s.key == "morale" && s.present)) {
            let bytes = vfs.read(&f.file).unwrap();
            let out = apply(
                &bytes,
                &Edit::SetScalar {
                    path: vec![f.key.clone(), "morale".into()],
                    value: "1.5".into(),
                    quoted: false,
                },
            );
            assert!(out.is_ok(), "mod rebel morale edit should apply for {}", f.key);
        }
        println!("[rebels:anbennar] {} factions", data.factions.len());
    }
}
