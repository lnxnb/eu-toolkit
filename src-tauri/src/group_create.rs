//! S2.3 / S2.4 — religion & culture GROUP creation scaffolds.
//!
//! Both the create-religion (5.4) / create-culture (6.4) flows and the panels'
//! move-to-group dropdowns can target a brand-new group. This module builds the
//! new group's block text, copying sensible group-level defaults from a chosen
//! **sibling group** (never invented — preserve-unknown means copy what's there):
//!
//! * Religion groups copy every group-level *scalar* and *simple bare block*
//!   (`defender_of_faith = yes`, `can_form_personal_unions = yes`,
//!   `center_of_religion = 118`, `flag_emblem_index_range = { 1 57 }`,
//!   `crusade_name`, `harmonized_modifier`, …). Religion sub-blocks (identified by
//!   a `color`/`icon`) and complex nested registries (`religious_schools`, whose
//!   keyed children would collide) are skipped.
//! * Culture groups take a required `graphical_culture` (the game crashes
//!   renderers without one) plus the sibling's `second_graphical_culture` if any,
//!   and the three name pools (`male_names`/`female_names`/`dynasty_names`) copied
//!   **byte-faithfully** from the sibling group. When the sibling defines its
//!   pools per-member (Anbennar's `elven`), the first member culture that carries
//!   each pool is copied instead — so generated characters always have names.
//!
//! The block is authored at column 0 with **no members**; the create/move flow
//! then inserts the religion/culture into it as a second same-file edit that
//! composes on the evolving buffer (AGENTS.md list-creation ordering). Pool text
//! is extracted as Latin-1 (each source byte → one `char`) so Windows-1252
//! high bytes in accented names round-trip exactly when re-encoded on write.

use std::collections::HashSet;

use crate::game_data::extract_named_block;
use crate::paradox::{self, Block, Value};
use crate::vfs::Vfs;

const RELIGIONS_DIR: &str = "common/religions";
const CULTURES_DIR: &str = "common/cultures";

/// Group-level helper keys inside a culture group that are NOT member cultures.
const CULTURE_GROUP_KEYS: &[&str] = &[
    "male_names",
    "female_names",
    "dynasty_names",
    "country",
    "province",
    "graphical_culture",
    "second_graphical_culture",
];

const CULTURE_POOLS: &[&str] = &["male_names", "female_names", "dynasty_names"];

/// Fallback pools guaranteeing a scaffolded group is never nameless (only used
/// when a sibling group and all its members carry nothing for that pool).
const FALLBACK_MALE: &str = "Aldric Berin Cael Doran Edran";
const FALLBACK_FEMALE: &str = "Alia Bryn Cera Dela Enna";
const FALLBACK_DYNASTY: &str = "Aldering Berreth Caelmor";

/// A scaffolded new group, ready to insert as one top-level statement.
#[derive(Debug, serde::Serialize)]
pub struct GroupScaffold {
    /// Slugified, collision-free group key.
    pub group_key: String,
    /// Display name (== the requested name) — for the group's loc override.
    pub group_name: String,
    /// The new group block text, authored at column 0, with no members yet:
    /// `newkey = {\n\t<copied group-level keys>\n}`.
    pub block: String,
    /// Game-relative file the sibling group lives in — the suggested target so a
    /// create-flow inserts the group and its first member into one file.
    pub source_file: String,
}

fn parse_bytes(bytes: &[u8]) -> Block {
    paradox::parse(&String::from_utf8_lossy(bytes))
}

/// Slug: lowercase, non-alphanumerics → `_`, trimmed; empty falls back.
fn slugify(name: &str, fallback: &str) -> String {
    let mut out = String::new();
    let mut prev_us = false;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_us = false;
        } else if !prev_us {
            out.push('_');
            prev_us = true;
        }
    }
    let trimmed = out.trim_matches('_');
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.to_string()
    }
}

/// Disambiguate `base` against `taken` (`base`, `base_2`, `base_3`, …).
fn unique_key(base: &str, taken: &HashSet<String>) -> String {
    if !taken.contains(base) {
        return base.to_string();
    }
    let mut i = 2;
    loop {
        let cand = format!("{base}_{i}");
        if !taken.contains(&cand) {
            return cand;
        }
        i += 1;
    }
}

/// Every top-level group key in a merged dir (mod shadows base).
fn existing_group_keys(vfs: &Vfs, dir: &str) -> HashSet<String> {
    let mut out = HashSet::new();
    for (fname, path) in vfs.list_dir(dir) {
        if !fname.to_lowercase().ends_with(".txt") {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        for (k, _b) in parse_bytes(&bytes).key_blocks() {
            out.insert(k.to_string());
        }
    }
    out
}

/// Locate the file + raw bytes + parsed block of `group_key` in `dir`.
fn find_group(vfs: &Vfs, dir: &str, group_key: &str) -> Option<(String, Vec<u8>, Block)> {
    for (fname, path) in vfs.list_dir(dir) {
        if !fname.to_lowercase().ends_with(".txt") {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let block = parse_bytes(&bytes);
        if let Some(gb) = block.get_block(group_key) {
            return Some((format!("{dir}/{fname}"), bytes, gb.clone()));
        }
    }
    None
}

/// A religion sub-block is any group child carrying a `color` or `icon`.
fn is_religion_block(b: &Block) -> bool {
    b.get_block("color").is_some() || b.get_scalar("icon").is_some()
}

/// A "simple" block is a flat bare-scalar list (`{ 1 57 }`) — no keyed children.
fn is_simple_bare_block(b: &Block) -> bool {
    !b.items.is_empty() && b.items.iter().all(|(k, v)| k.is_none() && matches!(v, Value::Scalar(_)))
}

/// Build the new religion group block from a sibling's group-level keys.
pub fn scaffold_religion_group(
    vfs: &Vfs,
    sibling_group_key: &str,
    name: &str,
    extra_taken: &[String],
) -> Result<GroupScaffold, String> {
    let (source_file, _bytes, gb) = find_group(vfs, RELIGIONS_DIR, sibling_group_key)
        .ok_or_else(|| format!("Sibling religion group not found: {sibling_group_key}"))?;

    let mut taken = existing_group_keys(vfs, RELIGIONS_DIR);
    taken.extend(extra_taken.iter().cloned());
    let group_key = unique_key(&slugify(name, "new_religion_group"), &taken);

    let mut lines: Vec<String> = Vec::new();
    for (k, v) in &gb.items {
        let Some(k) = k else { continue };
        match v {
            Value::Scalar(s) => lines.push(format!("\t{k} = {s}")),
            Value::Block(b) => {
                if is_religion_block(b) {
                    continue; // a member religion, not a group default
                }
                if is_simple_bare_block(b) {
                    let inner: Vec<&str> = b.bare_scalars().collect();
                    lines.push(format!("\t{k} = {{ {} }}", inner.join(" ")));
                }
                // else: complex nested block (religious_schools, …) — skip.
            }
        }
    }

    let body = if lines.is_empty() {
        String::new()
    } else {
        format!("{}\n", lines.join("\n"))
    };
    let block = format!("{group_key} = {{\n{body}}}");
    Ok(GroupScaffold {
        group_key,
        group_name: name.to_string(),
        block,
        source_file,
    })
}

/// Non-empty parsed pool? (bare scalars include both quoted and bare tokens).
fn pool_nonempty(b: Option<&Block>) -> bool {
    b.map(|b| b.bare_scalars().next().is_some()).unwrap_or(false)
}

/// The byte-faithful `<pool> = { ... }` text to copy for `pool`: the group-level
/// pool if non-empty, else the first member culture that carries it. Returns
/// `None` when neither exists (caller supplies a fallback).
fn copy_pool_text(bytes: &[u8], gb: &Block, group_key: &str, pool: &str) -> Option<String> {
    if pool_nonempty(gb.get_block(pool)) {
        return extract_named_block(bytes, &[group_key, pool]);
    }
    for (ck, cb) in gb.key_blocks() {
        if CULTURE_GROUP_KEYS.contains(&ck) {
            continue;
        }
        if pool_nonempty(cb.get_block(pool)) {
            return extract_named_block(bytes, &[group_key, ck, pool]);
        }
    }
    None
}

/// Re-indent an extracted `<key> = { ... }` block one tab in as a group child.
/// Only the first line lacks source indentation; the rest keep the source's
/// (cosmetic — the game ignores whitespace, and this is fresh appended content).
fn indent_pool(text: &str) -> String {
    format!("\t{text}")
}

/// Build the new culture group block: required `graphical_culture`, the sibling's
/// `second_graphical_culture` if any, and the three name pools copied from the
/// sibling group (byte-faithfully; member-culture fallback when group-level pools
/// are absent).
pub fn scaffold_culture_group(
    vfs: &Vfs,
    sibling_group_key: &str,
    name: &str,
    graphical_culture: &str,
    extra_taken: &[String],
) -> Result<GroupScaffold, String> {
    if graphical_culture.trim().is_empty() {
        return Err("A graphical culture is required for a new culture group".into());
    }
    let (source_file, bytes, gb) = find_group(vfs, CULTURES_DIR, sibling_group_key)
        .ok_or_else(|| format!("Sibling culture group not found: {sibling_group_key}"))?;

    let mut taken = existing_group_keys(vfs, CULTURES_DIR);
    taken.extend(extra_taken.iter().cloned());
    let group_key = unique_key(&slugify(name, "new_culture_group"), &taken);

    let mut lines: Vec<String> = Vec::new();
    lines.push(format!("\tgraphical_culture = {}", graphical_culture.trim()));
    if let Some(sg) = gb.get_scalar("second_graphical_culture") {
        lines.push(format!("\tsecond_graphical_culture = {sg}"));
    }
    for pool in CULTURE_POOLS {
        let text = copy_pool_text(&bytes, &gb, sibling_group_key, pool).unwrap_or_else(|| {
            let fallback = match *pool {
                "male_names" => FALLBACK_MALE,
                "female_names" => FALLBACK_FEMALE,
                _ => FALLBACK_DYNASTY,
            };
            format!("{pool} = {{ {fallback} }}")
        });
        lines.push(indent_pool(&text));
    }

    let block = format!("{group_key} = {{\n{}\n}}", lines.join("\n"));
    Ok(GroupScaffold {
        group_key,
        group_name: name.to_string(),
        block,
        source_file,
    })
}

// --- Tauri commands ------------------------------------------------------

#[tauri::command(async)]
pub fn prepare_religion_group_scaffold(
    install_path: String,
    mod_path: Option<String>,
    sibling_group_key: String,
    name: String,
    existing_keys: Option<Vec<String>>,
) -> Result<GroupScaffold, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    scaffold_religion_group(&vfs, &sibling_group_key, &name, &existing_keys.unwrap_or_default())
}

#[tauri::command(async)]
pub fn prepare_culture_group_scaffold(
    install_path: String,
    mod_path: Option<String>,
    sibling_group_key: String,
    name: String,
    graphical_culture: String,
    existing_keys: Option<Vec<String>>,
) -> Result<GroupScaffold, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    scaffold_culture_group(
        &vfs,
        &sibling_group_key,
        &name,
        &graphical_culture,
        &existing_keys.unwrap_or_default(),
    )
}

// --- Tests ---------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    const INSTALL: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Europa Universalis IV";
    const ANBENNAR: &str = r"C:\Users\User\projects\anbennar-eu4-dev";

    fn install_present() -> bool {
        Path::new(INSTALL).join("map/provinces.bmp").is_file()
    }

    fn setup(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("eu_toolkit_groupcreate_test_{name}"));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    /// Parse `key = { ... }` back out of a scaffold block to prove it round-trips.
    fn reparse(block_text: &str) -> Block {
        parse_bytes(block_text.as_bytes())
    }

    // --- Synthetic fixtures ---

    fn synth_religions(root: &Path) -> Vfs {
        let base = root.join("base");
        std::fs::create_dir_all(base.join("common/religions")).unwrap();
        std::fs::write(
            base.join("common/religions/00_religion.txt"),
            "christian = {\n\
             \tdefender_of_faith = yes\n\
             \tcan_form_personal_unions = yes\n\
             \tcenter_of_religion = 118\n\
             \tflags_with_emblem_percentage = 33\n\
             \tflag_emblem_index_range = { 1 57 }\n\
             \treligious_schools = { hanafi_school = { x = 1 } }\n\
             \tcatholic = {\n\
             \t\tcolor = { 204 204 0 }\n\
             \t\ticon = 1\n\
             \t\theretic = { LOLLARD }\n\
             \t}\n\
             \tharmonized_modifier = harmonized_christian\n\
             \tcrusade_name = CRUSADE\n\
             }\n",
        )
        .unwrap();
        Vfs::new(base.to_str().unwrap(), None).unwrap()
    }

    #[test]
    fn religion_group_copies_scalar_and_simple_keys_only() {
        let root = setup("rel_copy");
        let vfs = synth_religions(&root);
        let s = scaffold_religion_group(&vfs, "christian", "Solar Faiths", &[]).unwrap();
        assert_eq!(s.group_key, "solar_faiths");
        assert_eq!(s.group_name, "Solar Faiths");
        assert_eq!(s.source_file, "common/religions/00_religion.txt");

        // Scaffold parses back into exactly one group with the copied keys.
        let b = reparse(&s.block);
        let gb = b.get_block("solar_faiths").expect("group parses back");
        assert_eq!(gb.get_scalar("defender_of_faith"), Some("yes"));
        assert_eq!(gb.get_scalar("can_form_personal_unions"), Some("yes"));
        assert_eq!(gb.get_scalar("center_of_religion"), Some("118"));
        assert_eq!(gb.get_scalar("crusade_name"), Some("CRUSADE"));
        assert_eq!(gb.get_scalar("harmonized_modifier"), Some("harmonized_christian"));
        // Simple bare block copied.
        let range: Vec<&str> = gb.get_block("flag_emblem_index_range").unwrap().bare_scalars().collect();
        assert_eq!(range, vec!["1", "57"]);
        // Member religion NOT copied.
        assert!(gb.get_block("catholic").is_none(), "member religion must not be copied");
        // Complex nested registry NOT copied.
        assert!(gb.get_block("religious_schools").is_none(), "religious_schools must be skipped");
    }

    #[test]
    fn religion_group_key_collision_disambiguated() {
        let root = setup("rel_collide");
        let vfs = synth_religions(&root);
        // "christian" already exists; slug of "Christian" collides → _2. Also honor
        // pending (extra_taken) keys created earlier this session.
        let s = scaffold_religion_group(&vfs, "christian", "Christian", &["christian_2".into()]).unwrap();
        assert_eq!(s.group_key, "christian_3");
    }

    #[test]
    fn religion_group_from_sibling_without_defaults_is_empty_but_valid() {
        let root = setup("rel_empty");
        let base = root.join("base");
        std::fs::create_dir_all(base.join("common/religions")).unwrap();
        std::fs::write(
            base.join("common/religions/00_religion.txt"),
            "pagan = {\n\treformed_pagan = { color = { 1 2 3 } icon = 9 }\n}\n",
        )
        .unwrap();
        let vfs = Vfs::new(base.to_str().unwrap(), None).unwrap();
        let s = scaffold_religion_group(&vfs, "pagan", "New Cults", &[]).unwrap();
        // No group-level defaults to copy → empty group, still valid script.
        let b = reparse(&s.block);
        assert!(b.get_block("new_cults").is_some());
    }

    fn synth_cultures(root: &Path) -> Vfs {
        let base = root.join("base");
        std::fs::create_dir_all(base.join("common/cultures")).unwrap();
        std::fs::write(
            base.join("common/cultures/00_cultures.txt"),
            "germanic = {\n\
             \tgraphical_culture = westerngfx\n\
             \tsecond_graphical_culture = easterngfx\n\
             \tmale_names = { Hans Otto Karl }\n\
             \tfemale_names = { Anna Greta }\n\
             \tdynasty_names = { Habsburg Hohenzollern }\n\
             \tprussian = { primary = PRU }\n\
             }\n",
        )
        .unwrap();
        Vfs::new(base.to_str().unwrap(), None).unwrap()
    }

    #[test]
    fn culture_group_requires_graphical_culture() {
        let root = setup("cul_gfx_req");
        let vfs = synth_cultures(&root);
        assert!(scaffold_culture_group(&vfs, "germanic", "New Group", "", &[]).is_err());
    }

    #[test]
    fn culture_group_copies_gfx_and_group_pools() {
        let root = setup("cul_copy");
        let vfs = synth_cultures(&root);
        let s = scaffold_culture_group(&vfs, "germanic", "Sky People", "muslimgfx", &[]).unwrap();
        assert_eq!(s.group_key, "sky_people");
        let b = reparse(&s.block);
        let gb = b.get_block("sky_people").expect("group parses back");
        assert_eq!(gb.get_scalar("graphical_culture"), Some("muslimgfx"));
        assert_eq!(gb.get_scalar("second_graphical_culture"), Some("easterngfx"));
        let male: Vec<&str> = gb.get_block("male_names").unwrap().bare_scalars().collect();
        assert_eq!(male, vec!["Hans", "Otto", "Karl"]);
        let dyn_: Vec<&str> = gb.get_block("dynasty_names").unwrap().bare_scalars().collect();
        assert_eq!(dyn_, vec!["Habsburg", "Hohenzollern"]);
        // A member culture is NOT dragged along.
        assert!(gb.get_block("prussian").is_none());
    }

    #[test]
    fn culture_group_pools_fall_back_to_member_when_group_has_none() {
        let root = setup("cul_member_pool");
        let base = root.join("base");
        std::fs::create_dir_all(base.join("common/cultures")).unwrap();
        // Group has gfx but no group-level pools; a member does.
        std::fs::write(
            base.join("common/cultures/00_cultures.txt"),
            "elvish = {\n\
             \tgraphical_culture = elvengfx\n\
             \tmoon_elf = { primary = MON }\n\
             \tsun_elf = {\n\
             \t\tmale_names = { Adrahel Elrian Taelar }\n\
             \t\tfemale_names = { Amarien Narawen }\n\
             \t\tdynasty_names = { Kelazuir Elrazuir }\n\
             \t}\n\
             }\n",
        )
        .unwrap();
        let vfs = Vfs::new(base.to_str().unwrap(), None).unwrap();
        let s = scaffold_culture_group(&vfs, "elvish", "New Elves", "elvengfx", &[]).unwrap();
        let b = reparse(&s.block);
        let gb = b.get_block("new_elves").unwrap();
        let male: Vec<&str> = gb.get_block("male_names").unwrap().bare_scalars().collect();
        assert!(male.contains(&"Adrahel"), "member pool copied when group pool absent");
    }

    #[test]
    fn culture_group_falls_back_to_starter_pools_when_totally_absent() {
        let root = setup("cul_no_pool");
        let base = root.join("base");
        std::fs::create_dir_all(base.join("common/cultures")).unwrap();
        std::fs::write(
            base.join("common/cultures/00_cultures.txt"),
            "spare = {\n\tgraphical_culture = westerngfx\n\tloner = { primary = LON }\n}\n",
        )
        .unwrap();
        let vfs = Vfs::new(base.to_str().unwrap(), None).unwrap();
        let s = scaffold_culture_group(&vfs, "spare", "Nameless", "westerngfx", &[]).unwrap();
        let b = reparse(&s.block);
        let gb = b.get_block("nameless").unwrap();
        // Non-empty male + dynasty pools guaranteed so rulers can be named.
        assert!(gb.get_block("male_names").unwrap().bare_scalars().next().is_some());
        assert!(gb.get_block("dynasty_names").unwrap().bare_scalars().next().is_some());
    }

    // --- Real-install / Anbennar (no-op if absent) ---

    #[test]
    fn real_religion_group_from_christian() {
        if !install_present() {
            return;
        }
        let vfs = Vfs::new(INSTALL, None).unwrap();
        let s = scaffold_religion_group(&vfs, "christian", "Test Faith", &[]).unwrap();
        let b = reparse(&s.block);
        let gb = b.get_block("test_faith").expect("scaffold parses");
        assert_eq!(gb.get_scalar("defender_of_faith"), Some("yes"));
        // No stray member religions leaked in.
        assert!(gb.get_block("catholic").is_none());
        assert!(gb.get_block("protestant").is_none());
    }

    #[test]
    fn anbennar_smoke_elven_pools_survive_copy() {
        if !install_present() || !Path::new(ANBENNAR).is_dir() {
            return;
        }
        let vfs = Vfs::new(INSTALL, Some(ANBENNAR)).unwrap();
        // The `elven` group defines pools per member (sun_elf); the scaffold must
        // fall back to a member and keep a known elven name.
        let s = scaffold_culture_group(&vfs, "elven", "New Elven", "elvengfx", &[]).unwrap();
        assert_eq!(s.source_file, "common/cultures/anb_cultures.txt");
        let b = reparse(&s.block);
        let gb = b.get_block("new_elven").expect("scaffold parses");
        assert_eq!(gb.get_scalar("graphical_culture"), Some("elvengfx"));
        let male: Vec<String> = gb
            .get_block("male_names")
            .expect("male pool present")
            .bare_scalars()
            .map(str::to_string)
            .collect();
        assert!(
            male.iter().any(|n| n == "Adrahel"),
            "known elven name should survive the copy; got {male:?}"
        );
    }

    #[test]
    fn anbennar_group_lists_still_include_custom_groups() {
        if !install_present() || !Path::new(ANBENNAR).is_dir() {
            return;
        }
        let vfs = Vfs::new(INSTALL, Some(ANBENNAR)).unwrap();
        let cul = existing_group_keys(&vfs, CULTURES_DIR);
        // Anbennar's custom groups appear alongside vanilla ones.
        assert!(cul.contains("elven"), "Anbennar elven group listed");
        // A newly-scaffolded key never collides with an Anbennar group.
        let s = scaffold_culture_group(&vfs, "elven", "Elven", "elvengfx", &[]).unwrap();
        assert!(!cul.contains(&s.group_key), "scaffold key is fresh: {}", s.group_key);
    }
}
