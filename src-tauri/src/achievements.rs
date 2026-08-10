//! Achievements editor (View ▸ Achievements…).
//!
//! `common/achievements.txt` is a SINGLE canonical file (not an additive
//! directory): 373 vanilla blocks `key = { id localization possible happened
//! [visible] [provinces_to_highlight] }`. Verified format (v1.37.5):
//!
//! * `id` — integer, the Steam/console achievement mapping index
//!   (`msgrdk_achievements.json` maps `pdx_id` → console ids by this key).
//! * `localization` — a loc-key STEM: display name is `<stem>_NAME`, description
//!   `<stem>_DESC` (`NEW_ACHIEVEMENT_1_2` → `NEW_ACHIEVEMENT_1_2_NAME`).
//! * `possible` / `happened` — trigger blocks (start preconditions / completion).
//! * `visible` — optional trigger gating listing in the achievements window.
//! * `provinces_to_highlight` — optional province-shaped trigger block.
//! * Icon: `gfx/interface/achievements/<key>.dds`, named EXACTLY after the block
//!   key (373/373 in vanilla) — the `modifier_icon` filename convention, resolved
//!   through the Vfs in `icons.rs` (uncompressed 32-bpp BGRA, decodes with the
//!   existing strip decoder; Anbennar's custom icons use the same format).
//!
//! Modding reality (why the editor exists at all): a mod can shadow the whole
//! file — Anbennar blanks it with `#Overwritten` and builds its own achievement
//! system out of paired `triggered_modifiers` (`ach_x_g` in-progress / `ach_x`
//! done), which the mechanics overlay already edits. Editing/adding entries here
//! affects the in-game achievements window; it can never grant STEAM
//! achievements (the Steam award is keyed to vanilla's compiled id mapping) —
//! the UI says so rather than promising otherwise.
//!
//! Editing = existing typed-edits only: `id`/`localization` are `SetScalar` (or
//! `InsertStatement` when absent), trigger blocks go through the 14.2 tree
//! editor at `[key, name]`, name/desc are `LocOverride`s on the stem keys, a new
//! achievement is one `AppendText` composite + loc overrides, delete is a
//! root-level `RemoveStatement`. The file is copy-on-write like everything else.

use crate::loc::{self, LocStore};
use crate::mod_writer;
use crate::paradox::{self, Block};
use crate::vfs::Vfs;

pub const ACHIEVEMENTS_FILE: &str = "common/achievements.txt";

/// Trigger blocks of an achievement, in canonical display order. All four are
/// trigger-shaped (the 14.2 "triggers" registry).
static TRIGGER_BLOCKS: &[&str] = &["possible", "happened", "visible", "provinces_to_highlight"];

// ---------------------------------------------------------------------------
// Payload types (serialize camelCase; mirrored by src/lib/achievements.ts).
// ---------------------------------------------------------------------------

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ScriptBlockRef {
    pub name: String,
    pub present: bool,
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Achievement {
    pub key: String,
    pub file: String,
    pub origin: String,
    pub id: Option<i64>,
    /// The `localization` loc-key stem, verbatim (None when the key is absent).
    pub localization: Option<String>,
    /// List-display label (resolved `<stem>_NAME`, else prettified key).
    pub name: String,
    pub name_key: String,
    pub name_loc: Option<String>,
    pub desc_key: String,
    pub desc_loc: Option<String>,
    pub script_blocks: Vec<ScriptBlockRef>,
    /// `gfx/interface/achievements/<key>.dds` resolves through the Vfs.
    pub has_icon: bool,
    pub raw_extra: Vec<String>,
    pub raw: String,
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AchievementsData {
    pub achievements: Vec<Achievement>,
    pub file: String,
}

// ---------------------------------------------------------------------------
// Parse.
// ---------------------------------------------------------------------------

fn parse_achievement(
    file_bytes: &[u8],
    key: &str,
    b: &Block,
    loc: &LocStore,
    origin: &str,
    has_icon: bool,
) -> Achievement {
    let id = b.get_scalar("id").and_then(|s| s.trim().parse::<i64>().ok());
    let localization = b.get_scalar("localization").map(|s| s.trim().to_string());

    let script_blocks = TRIGGER_BLOCKS
        .iter()
        .map(|name| ScriptBlockRef {
            name: name.to_string(),
            present: b.get_block(name).is_some(),
        })
        .collect();

    // Preserve-unknown: every top-level key not modeled above.
    let mut raw_extra: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (k, _) in &b.items {
        if let Some(k) = k.as_deref() {
            if k != "id"
                && k != "localization"
                && !TRIGGER_BLOCKS.contains(&k)
                && seen.insert(k.to_string())
            {
                raw_extra.push(k.to_string());
            }
        }
    }

    let raw = mod_writer::block_span(file_bytes, &[key.to_string()])
        .map(|(s, e)| String::from_utf8_lossy(&file_bytes[s..e]).into_owned())
        .unwrap_or_default();

    // Name/desc keys derive from the `localization` stem; a block without one
    // falls back to the achievement key so the editor can still queue loc.
    let stem = localization.as_deref().unwrap_or(key);
    let name_key = format!("{stem}_NAME");
    let desc_key = format!("{stem}_DESC");
    let name = loc
        .get(&name_key)
        .map(str::to_string)
        .unwrap_or_else(|| loc::prettify(key));

    Achievement {
        key: key.to_string(),
        file: ACHIEVEMENTS_FILE.to_string(),
        origin: origin.to_string(),
        id,
        localization,
        name,
        name_loc: loc.get(&name_key).map(str::to_string),
        name_key,
        desc_loc: loc.get(&desc_key).map(str::to_string),
        desc_key,
        script_blocks,
        has_icon,
        raw_extra,
        raw,
    }
}

pub fn load(vfs: &Vfs, loc: &LocStore) -> AchievementsData {
    let mut achievements = Vec::new();
    if let Ok(bytes) = vfs.read(ACHIEVEMENTS_FILE) {
        let origin = if vfs
            .resolve(ACHIEVEMENTS_FILE)
            .is_some_and(|p| vfs.mod_dir().is_some_and(|m| p.starts_with(m)))
        {
            "mod"
        } else {
            "base"
        };
        let block = paradox::parse(&String::from_utf8_lossy(&bytes));
        for (key, b) in block.key_blocks() {
            let has_icon = crate::icons::achievement_icon_rel(key)
                .and_then(|rel| vfs.resolve(&rel))
                .is_some();
            achievements.push(parse_achievement(&bytes, key, b, loc, origin, has_icon));
        }
    }
    AchievementsData {
        achievements,
        file: ACHIEVEMENTS_FILE.to_string(),
    }
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
    /// Loc keys to queue as `LocOverride`s (name/desc on the stem = the key).
    pub loc_entries: Vec<LocEntry>,
}

/// Minimal game-valid achievement. `localization` stem = the key itself, so the
/// queued `<key>_NAME`/`<key>_DESC` overrides are the ones the game reads.
/// `happened = { always = no }` is an honest editable placeholder — a new
/// achievement should never complete until the modder writes its condition.
pub fn scaffold_achievement(key: &str, next_id: i64) -> Scaffold {
    let text = format!(
        "{key} = {{\n\
\tid = {next_id}\n\
\tlocalization = {key}\n\
\t\n\
\tpossible = {{\n\
\t\tnormal_or_historical_nations = yes\n\
\t\tnormal_province_values = yes\n\
\t\tironman = yes\n\
\t}}\n\
\t\n\
\thappened = {{\n\
\t\talways = no\n\
\t}}\n\
}}"
    );
    let pretty = loc::prettify(key);
    Scaffold {
        key: key.to_string(),
        file: ACHIEVEMENTS_FILE.to_string(),
        text,
        loc_entries: vec![
            LocEntry { key: format!("{key}_NAME"), value: pretty.clone() },
            LocEntry { key: format!("{key}_DESC"), value: format!("{pretty}.") },
        ],
    }
}

/// Next free `id`: one past the highest id in the resolved file (ids only
/// matter to Steam's compiled mapping, which mods can't extend, but every
/// vanilla block carries one so the scaffold stays shape-identical).
pub fn next_achievement_id(vfs: &Vfs) -> i64 {
    let Ok(bytes) = vfs.read(ACHIEVEMENTS_FILE) else {
        return 1;
    };
    let block = paradox::parse(&String::from_utf8_lossy(&bytes));
    block
        .key_blocks()
        .filter_map(|(_, b)| b.get_scalar("id").and_then(|s| s.trim().parse::<i64>().ok()))
        .max()
        .map_or(1, |m| m + 1)
}

// ---------------------------------------------------------------------------
// Commands.
// ---------------------------------------------------------------------------

#[tauri::command(async)]
pub fn get_achievements(
    install_path: String,
    mod_path: Option<String>,
) -> Result<AchievementsData, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    let loc = loc::store(&vfs, &install_path, mod_path.as_deref());
    Ok(load(&vfs, &loc))
}

#[tauri::command(async)]
pub fn scaffold_achievement_cmd(
    install_path: String,
    mod_path: Option<String>,
    key: String,
) -> Result<Scaffold, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    Ok(scaffold_achievement(&key, next_achievement_id(&vfs)))
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
        let root = std::env::temp_dir().join(format!("eu_toolkit_achievements_test_{name}"));
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

    const ACH_SRC: &str = "\
achievement_test_one = {\n\
\tid = 41\n\
\tlocalization = TEST_ACH_1\n\
\t\n\
\tpossible = {\n\
\t\tironman = yes\n\
\t\tstart_date = 1444.11.11\n\
\t}\n\
\t\n\
\tvisible = {\n\
\t\treligion = catholic\n\
\t}\n\
\t\n\
\tprovinces_to_highlight = {\n\
\t\tregion = australia_region\n\
\t}\n\
\t\n\
\thappened = {\n\
\t\tis_emperor = yes\n\
\t}\n\
\t\n\
\tsome_future_key = yes\n\
}\n\
\n\
achievement_test_two = {\n\
\tid = 7\n\
\tlocalization = TEST_ACH_2\n\
\tpossible = {\n\
\t\tironman = yes\n\
\t}\n\
\thappened = {\n\
\t\talways = no\n\
\t}\n\
}\n";

    fn fixture(name: &str) -> (std::path::PathBuf, Vfs) {
        synthetic(name, &[("common/achievements.txt", ACH_SRC)])
    }

    #[test]
    fn parses_scalars_triggers_loc_and_raw_extra() {
        let (_root, vfs) = fixture("parse");
        let loc = LocStore::from_pairs(&[
            ("TEST_ACH_1_NAME", "Test the First"),
            ("TEST_ACH_1_DESC", "Do the first thing."),
        ]);
        let data = load(&vfs, &loc);
        assert_eq!(data.achievements.len(), 2);
        let a = &data.achievements[0];
        assert_eq!(a.key, "achievement_test_one");
        assert_eq!(a.id, Some(41));
        assert_eq!(a.localization.as_deref(), Some("TEST_ACH_1"));
        assert_eq!(a.name, "Test the First");
        assert_eq!(a.name_key, "TEST_ACH_1_NAME");
        assert_eq!(a.desc_loc.as_deref(), Some("Do the first thing."));
        let present: Vec<_> = a
            .script_blocks
            .iter()
            .filter(|s| s.present)
            .map(|s| s.name.as_str())
            .collect();
        assert_eq!(present, ["possible", "happened", "visible", "provinces_to_highlight"]);
        assert_eq!(a.raw_extra, ["some_future_key"]);
        // `raw` is the braces-inclusive block VALUE span (same as every module).
        assert!(a.raw.starts_with('{') && a.raw.contains("id = 41"));
        assert!(!a.has_icon);
        assert_eq!(a.origin, "base");

        // No `visible`/`provinces_to_highlight` on the second — present = false.
        let b = &data.achievements[1];
        assert_eq!(b.name, "Achievement Test Two"); // no loc → prettified key
        let absent: Vec<_> = b
            .script_blocks
            .iter()
            .filter(|s| !s.present)
            .map(|s| s.name.as_str())
            .collect();
        assert_eq!(absent, ["visible", "provinces_to_highlight"]);
    }

    #[test]
    fn edits_are_byte_surgical() {
        let (_root, vfs) = fixture("edit");
        let bytes = vfs.read(ACHIEVEMENTS_FILE).unwrap();
        // Change one id; every other byte must survive.
        let out = apply(
            &bytes,
            &Edit::SetScalar {
                path: vec!["achievement_test_two".into(), "id".into()],
                value: "374".into(),
                quoted: false,
            },
        )
        .unwrap();
        let expected = String::from_utf8_lossy(&bytes).replace("id = 7", "id = 374");
        assert_eq!(String::from_utf8_lossy(&out), expected);

        // Root-level delete removes exactly the second block.
        let out = apply(
            &bytes,
            &Edit::RemoveStatement {
                block_path: vec![],
                key: "achievement_test_two".into(),
                value: None,
            },
        )
        .unwrap();
        let s = String::from_utf8_lossy(&out);
        assert!(s.contains("achievement_test_one"));
        assert!(!s.contains("achievement_test_two"));
    }

    #[test]
    fn scaffold_parses_back_and_allocates_next_id() {
        let (_root, vfs) = fixture("scaffold");
        assert_eq!(next_achievement_id(&vfs), 42);
        let sc = scaffold_achievement("my_mod_achievement", 42);
        let block = paradox::parse(&sc.text);
        let (key, b) = block.key_blocks().next().unwrap();
        assert_eq!(key, "my_mod_achievement");
        assert_eq!(b.get_scalar("id").unwrap().trim(), "42");
        assert_eq!(b.get_scalar("localization").unwrap().trim(), "my_mod_achievement");
        assert!(b.get_block("possible").is_some());
        assert!(b.get_block("happened").is_some());
        assert_eq!(sc.loc_entries[0].key, "my_mod_achievement_NAME");

        // Appended scaffold loads as a third achievement.
        let bytes = vfs.read(ACHIEVEMENTS_FILE).unwrap();
        let appended = format!("{}\n{}\n", String::from_utf8_lossy(&bytes), sc.text);
        std::fs::write(_root.join("common/achievements.txt"), appended).unwrap();
        let data = load(&vfs, &LocStore::from_pairs(&[]));
        assert_eq!(data.achievements.len(), 3);
        assert_eq!(data.achievements[2].key, "my_mod_achievement");
    }

    #[test]
    fn missing_file_yields_empty() {
        let (_root, vfs) = synthetic("nofile", &[("common/other.txt", "x = yes\n")]);
        assert!(load(&vfs, &LocStore::from_pairs(&[])).achievements.is_empty());
        assert_eq!(next_achievement_id(&vfs), 1);
    }

    #[test]
    fn vanilla_achievements_load_with_ids_triggers_and_icons() {
        if !install_present() {
            return;
        }
        let vfs = Vfs::new(INSTALL, None).unwrap();
        let loc = crate::loc::store(&vfs, INSTALL, None);
        let data = load(&vfs, &loc);
        assert_eq!(data.achievements.len(), 373);
        for a in &data.achievements {
            assert!(a.id.is_some(), "{} lacks id", a.key);
            assert!(a.localization.is_some(), "{} lacks localization", a.key);
            let possible = a.script_blocks.iter().find(|s| s.name == "possible").unwrap();
            let happened = a.script_blocks.iter().find(|s| s.name == "happened").unwrap();
            assert!(possible.present && happened.present, "{} lacks triggers", a.key);
            assert!(a.has_icon, "{} has no icon file", a.key);
        }
        let glory = data
            .achievements
            .iter()
            .find(|a| a.key == "achievement_for_the_glory")
            .unwrap();
        assert_eq!(glory.name, "For the Glory");
        assert_eq!(glory.id, Some(1));
    }

    #[test]
    fn anbennar_blanked_file_yields_zero() {
        if !install_present() || !Path::new(ANBENNAR).is_dir() {
            return;
        }
        let vfs = Vfs::new(INSTALL, Some(ANBENNAR)).unwrap();
        let loc = crate::loc::store(&vfs, INSTALL, Some(ANBENNAR));
        let data = load(&vfs, &loc);
        assert!(
            data.achievements.is_empty(),
            "Anbennar blanks achievements.txt (#Overwritten); got {}",
            data.achievements.len()
        );
    }
}
