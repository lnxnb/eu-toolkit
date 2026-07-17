//! Registers a mod project with the game: writes the pointer .mod file into
//! Documents\Paradox Interactive\Europa Universalis IV\mod\, which the
//! Paradox launcher scans on startup.

use std::path::Path;

use crate::{paradox, vfs};

/// "v1.37.5.0" (launcher-settings.json rawVersion) -> "1.37.*".
pub fn detect_game_version(install: &Path) -> Option<String> {
    let text = std::fs::read_to_string(install.join("launcher-settings.json")).ok()?;
    let idx = text.find("\"rawVersion\"")?;
    let rest = &text[idx + "\"rawVersion\"".len()..];
    let rest = rest[rest.find(':')? + 1..].trim_start();
    let rest = rest.strip_prefix('"')?;
    let raw = rest[..rest.find('"')?].trim_start_matches('v');
    let mut parts = raw.split('.');
    let (major, minor) = (parts.next()?, parts.next()?);
    if major.is_empty() || minor.is_empty() {
        return None;
    }
    Some(format!("{major}.{minor}.*"))
}

/// The project's display name (descriptor `name`, else the folder name).
pub fn resolve_name(mod_dir: &Path) -> Option<String> {
    vfs::read_descriptor(mod_dir)
        .map(|text| paradox::parse(&text))
        .and_then(|b| b.get_scalar("name").map(str::to_string))
        .filter(|s| !s.trim().is_empty())
        .or_else(|| mod_dir.file_name().map(|n| n.to_string_lossy().into_owned()))
}

/// The launcher pointer file's base name (project name with filename-illegal
/// characters replaced by `_`), matching what [`write_game_pointer`] writes.
pub fn pointer_stem(name: &str) -> String {
    name.chars()
        .map(|c| if r#"\/:*?"<>|"#.contains(c) { '_' } else { c })
        .collect()
}

/// The `enabled_mods` entry the game reads from `dlc_load.json` for this
/// project: `mod/<stem>.mod`, relative to the user data folder (Sprint 30.5).
pub fn enabled_mods_entry(mod_dir: &Path) -> Result<String, String> {
    let name = resolve_name(mod_dir).ok_or("Cannot determine the project's name")?;
    Ok(format!("mod/{}.mod", pointer_stem(&name)))
}

/// Writes (or overwrites) the pointer file, named after the project.
/// Returns the project name used.
pub fn write_game_pointer(
    documents: &Path,
    install: &Path,
    mod_dir: &Path,
) -> Result<String, String> {
    let block = vfs::read_descriptor(mod_dir).map(|text| paradox::parse(&text));
    let name = resolve_name(mod_dir).ok_or("Cannot determine the project's name")?;
    let version = block
        .as_ref()
        .and_then(|b| b.get_scalar("supported_version").map(str::to_string))
        .or_else(|| detect_game_version(install));

    let mod_folder = documents
        .join("Paradox Interactive")
        .join("Europa Universalis IV")
        .join("mod");
    std::fs::create_dir_all(&mod_folder)
        .map_err(|e| format!("Failed to create {}: {e}", mod_folder.display()))?;

    let file_stem: String = pointer_stem(&name);

    // Paradox convention: forward slashes in .mod paths.
    let path_fwd = mod_dir.to_string_lossy().replace('\\', "/");
    let mut content = format!("name=\"{name}\"\n");
    if let Some(v) = &version {
        content.push_str(&format!("supported_version=\"{v}\"\n"));
    }
    content.push_str(&format!("path=\"{path_fwd}\"\n"));

    let pointer = mod_folder.join(format!("{file_stem}.mod"));
    std::fs::write(&pointer, content)
        .map_err(|e| format!("Failed to write {}: {e}", pointer.display()))?;
    Ok(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_and_overwrites_pointer() {
        let root = std::env::temp_dir().join("eu_toolkit_export_test");
        let _ = std::fs::remove_dir_all(&root);
        let docs = root.join("docs");
        let install = root.join("game");
        let mod_dir = root.join("mymod");
        std::fs::create_dir_all(&docs).unwrap();
        std::fs::create_dir_all(&install).unwrap();
        std::fs::create_dir_all(&mod_dir).unwrap();
        std::fs::write(
            install.join("launcher-settings.json"),
            r#"{ "gameId": "eu4", "rawVersion": "v1.37.5.0" }"#,
        )
        .unwrap();
        std::fs::write(mod_dir.join("descriptor.mod"), "name=\"My Mod: Reborn\"\n").unwrap();

        let name = write_game_pointer(&docs, &install, &mod_dir).unwrap();
        assert_eq!(name, "My Mod: Reborn");

        // ':' is illegal in filenames and must be sanitized.
        let pointer = docs
            .join("Paradox Interactive/Europa Universalis IV/mod/My Mod_ Reborn.mod");
        let content = std::fs::read_to_string(&pointer).unwrap();
        assert!(content.contains("name=\"My Mod: Reborn\""));
        assert!(content.contains("supported_version=\"1.37.*\""));
        assert!(content.contains(&format!(
            "path=\"{}\"",
            mod_dir.to_string_lossy().replace('\\', "/")
        )));

        // Re-export overwrites rather than duplicating.
        write_game_pointer(&docs, &install, &mod_dir).unwrap();
        let count = std::fs::read_dir(pointer.parent().unwrap()).unwrap().count();
        assert_eq!(count, 1);
    }

    #[test]
    fn detects_real_game_version() {
        let install =
            Path::new(r"C:\Program Files (x86)\Steam\steamapps\common\Europa Universalis IV");
        if !install.join("launcher-settings.json").is_file() {
            return;
        }
        let v = detect_game_version(install).expect("version detected");
        assert!(v.starts_with("1."), "unexpected version {v}");
        assert!(v.ends_with(".*"));
    }
}
