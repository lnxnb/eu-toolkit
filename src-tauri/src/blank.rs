//! Blank-project scaffold (SPRINT2 18.3): a mod that keeps the base map and
//! all definitions (terrain, climate, religions, cultures, trade goods,
//! governments, …) but empties the world. It `replace_path`s the folders whose
//! base contents populate the 1444 setup — `history/{provinces,countries,
//! diplomacy,wars}` and `common/country_tags` — and ships an otherwise-empty
//! `country_tags` file registering only the engine-required special tags
//! (REB/PIR/NAT). Those tags' country files (`countries/Rebels.txt`, etc.) are
//! NOT replace_path'd, so they resolve through the Vfs to the base install and
//! nothing is copied.
//!
//! **Ground-truth equivalence.** We cannot boot the game, so the scaffold mirrors
//! a structure proven to boot: Anbennar (a total conversion that reaches the 1444
//! lobby) replace_path's `history/{provinces,countries,diplomacy,wars}` and ships
//! its own `common/country_tags` files. Vanilla's own
//! `common/country_tags/00_countries.txt` opens with exactly
//! `REB`/`PIR`/`NAT` — the three special tags — before any real country, and
//! Anbennar's shadowing `00_countries.txt` keeps that same header. So REB/PIR/NAT
//! is the proven engine-required minimal set; every real country tag is omitted,
//! which is what empties the world. (Anbennar does not replace_path
//! `common/country_tags`; we must, because it ships its own thousands of tags
//! additively, whereas we want ONLY the specials to exist.)

use std::path::Path;

use crate::export;

/// The engine-required special country tags and their (base-resolved) files.
/// Every real nation is deliberately omitted — that is what empties the world.
pub const SPECIAL_TAGS: &[(&str, &str)] = &[
    ("REB", "countries/Rebels.txt"),
    ("PIR", "countries/Pirates.txt"),
    ("NAT", "countries/Natives.txt"),
];

/// Folders whose base contents populate the 1444 world; the blank scaffold
/// `replace_path`s each and ships it empty (except `common/country_tags`, which
/// carries only the special-tag file below).
pub const REPLACE_PATHS: &[&str] = &[
    "history/provinces",
    "history/countries",
    "history/diplomacy",
    "history/wars",
    "common/country_tags",
];

/// The `.mod` descriptor for a blank project: name, supported version (best
/// effort), and one `replace_path` per emptied folder.
pub fn descriptor_text(name: &str, install: &Path) -> String {
    let mut out = format!("name=\"{name}\"\n");
    if let Some(v) = export::detect_game_version(install) {
        out.push_str(&format!("supported_version=\"{v}\"\n"));
    }
    for rp in REPLACE_PATHS {
        out.push_str(&format!("replace_path=\"{rp}\"\n"));
    }
    out
}

/// The lone non-empty file in the scaffold: `common/country_tags/00_countries.txt`,
/// registering only the special tags. Pure ASCII, so the UTF-8 bytes written to
/// disk are identical to the Windows-1252/Latin-1 the game expects.
pub fn tag_file_text() -> String {
    let mut out =
        String::from("# Special countries — the only tags a blank world registers.\n");
    for (tag, file) in SPECIAL_TAGS {
        out.push_str(&format!("{tag} = \"{file}\"\n"));
    }
    out
}

/// Scaffolds a blank-world mod project into `project` (created if needed) over
/// base install `install`. Returns the project name (its folder name). Errors if
/// the install is invalid or the folder already holds a mod (never clobbers).
pub fn scaffold_blank(install: &Path, project: &Path) -> Result<String, String> {
    // Same installation-validity check the rest of the toolkit uses.
    if !install.join("map").join("provinces.bmp").is_file() {
        return Err(format!(
            "Not a valid EU4 installation (map/provinces.bmp missing): {}",
            install.display()
        ));
    }
    if crate::vfs::read_descriptor(project).is_some() {
        return Err(format!(
            "That folder already contains a mod project: {}",
            project.display()
        ));
    }
    std::fs::create_dir_all(project)
        .map_err(|e| format!("Failed to create project folder: {e}"))?;

    let name = project
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Blank Mod".to_string());

    // Empty the world: each replace_path'd folder exists but stays empty
    // (an empty folder still hides the base folder via the descriptor).
    for rp in REPLACE_PATHS {
        std::fs::create_dir_all(project.join(rp))
            .map_err(|e| format!("Failed to create {rp}: {e}"))?;
    }

    std::fs::write(
        project.join("descriptor.mod"),
        descriptor_text(&name, install),
    )
    .map_err(|e| format!("Failed to write descriptor.mod: {e}"))?;

    std::fs::write(
        project.join("common/country_tags/00_countries.txt"),
        tag_file_text(),
    )
    .map_err(|e| format!("Failed to write common/country_tags/00_countries.txt: {e}"))?;

    Ok(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paradox::{self, Value};
    use std::path::PathBuf;

    const REAL_INSTALL: &str =
        r"C:\Program Files (x86)\Steam\steamapps\common\Europa Universalis IV";

    fn temp_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("eu_toolkit_blank_test_{name}"));
        let _ = std::fs::remove_dir_all(&root);
        root
    }

    /// A fake base that passes the install-validity check but has no real data.
    fn fake_base(root: &Path) -> PathBuf {
        let base = root.join("base");
        std::fs::create_dir_all(base.join("map")).unwrap();
        std::fs::write(base.join("map/provinces.bmp"), b"x").unwrap();
        base
    }

    /// The descriptor and tag file round-trip through the real parsers: the
    /// descriptor yields exactly the five replace_paths (in order), and the tag
    /// file parses to exactly the special tags mapping to their base files.
    #[test]
    fn scaffold_descriptor_and_tags_round_trip() {
        let root = temp_root("round_trip");
        let base = fake_base(&root);
        let project = root.join("Blank World");

        let name = scaffold_blank(&base, &project).unwrap();
        assert_eq!(name, "Blank World");

        // Descriptor: parse and collect replace_path scalars in file order.
        let desc = std::fs::read_to_string(project.join("descriptor.mod")).unwrap();
        let block = paradox::parse(&desc);
        assert_eq!(block.get_scalar("name"), Some("Blank World"));
        let replace_paths: Vec<&str> = block
            .items
            .iter()
            .filter_map(|(k, v)| match (k.as_deref(), v) {
                (Some("replace_path"), Value::Scalar(s)) => Some(s.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(replace_paths, REPLACE_PATHS);

        // Every emptied folder exists on disk and is empty.
        for rp in REPLACE_PATHS {
            let dir = project.join(rp);
            assert!(dir.is_dir(), "{rp} folder missing");
            let entries = std::fs::read_dir(&dir).unwrap().count();
            let expected = if *rp == "common/country_tags" { 1 } else { 0 };
            assert_eq!(entries, expected, "{rp} should hold {expected} file(s)");
        }

        // Tag file: parses to exactly the three special tags -> base files.
        let tags = std::fs::read_to_string(
            project.join("common/country_tags/00_countries.txt"),
        )
        .unwrap();
        let tag_block = paradox::parse(&tags);
        let pairs: Vec<(&str, &str)> = tag_block
            .items
            .iter()
            .filter_map(|(k, v)| match (k.as_deref(), v) {
                (Some(k), Value::Scalar(s)) => Some((k, s.as_str())),
                _ => None,
            })
            .collect();
        assert_eq!(pairs, SPECIAL_TAGS.to_vec());
    }

    /// The scaffold refuses to overwrite an existing mod project.
    #[test]
    fn refuses_to_clobber_existing_project() {
        let root = temp_root("no_clobber");
        let base = fake_base(&root);
        let project = root.join("Existing");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(project.join("descriptor.mod"), "name=\"Mine\"\n").unwrap();
        assert!(scaffold_blank(&base, &project).is_err());
        // Left untouched.
        let desc = std::fs::read_to_string(project.join("descriptor.mod")).unwrap();
        assert_eq!(desc, "name=\"Mine\"\n");
    }

    /// Over the real install, a blank project's political mode_data renders with
    /// NO owner groups (every land province uncolonized) and does not panic. The
    /// replace_path'd `history/provinces` is empty, so no owner is folded in,
    /// while the base map/definitions still resolve through the Vfs.
    #[test]
    fn blank_political_mode_data_is_all_uncolonized() {
        let install = Path::new(REAL_INSTALL);
        if !install.join("map/provinces.bmp").is_file() {
            return; // No real install on this machine; skip silently.
        }
        let root = temp_root("mode_data");
        let project = root.join("Empty Earth");
        scaffold_blank(install, &project).unwrap();

        let vfs = crate::vfs::Vfs::new(
            REAL_INSTALL,
            Some(project.to_str().unwrap()),
        )
        .unwrap();

        // The world folders are emptied but the base map still resolves.
        assert!(vfs.list_dir("history/provinces").is_empty());
        assert!(vfs.resolve("map/provinces.bmp").is_some());
        // Only the special tags exist in the (replaced) country_tags folder.
        let tag_files = vfs.list_dir("common/country_tags");
        assert_eq!(tag_files.len(), 1);

        let loc = crate::loc::store(&vfs, REAL_INSTALL, Some(project.to_str().unwrap()));
        let data = crate::game_data::mode_data(&vfs, &loc, "political").unwrap();
        assert_eq!(data.kind, "categorical");
        // No country owns anything -> no political groups at all.
        assert!(
            data.groups.is_empty(),
            "expected no owner groups, got {}",
            data.groups.len()
        );
        // Every province value is the uncolonized/none sentinel.
        assert!(data.max_id > 0);
        assert!(data.values.iter().all(|&v| v == crate::game_data::NONE_GROUP));
    }
}
