use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Debug, serde::Serialize)]
pub struct Installation {
    pub path: String,
    pub source: String,
}

/// A real game install always has the map data; the user's Documents
/// "Paradox Interactive/Europa Universalis IV" folder (saves, mods, settings)
/// does not, so this check also keeps that folder out of the list.
pub fn is_valid_installation(path: &Path) -> bool {
    path.join("map").join("provinces.bmp").is_file()
}

pub fn detect() -> Vec<Installation> {
    let mut found = Vec::new();
    let mut seen = HashSet::new();

    for lib in steam_libraries() {
        let candidate = lib
            .join("steamapps")
            .join("common")
            .join("Europa Universalis IV");
        add_candidate(&mut found, &mut seen, candidate, "Steam library");
    }

    for common in [
        r"C:\Program Files (x86)\Steam\steamapps\common\Europa Universalis IV",
        r"C:\Program Files\Steam\steamapps\common\Europa Universalis IV",
    ] {
        add_candidate(&mut found, &mut seen, PathBuf::from(common), "Steam");
    }

    found
}

fn add_candidate(
    found: &mut Vec<Installation>,
    seen: &mut HashSet<String>,
    path: PathBuf,
    source: &str,
) {
    if !is_valid_installation(&path) {
        return;
    }
    // The same install can be reached via different spellings (the registry
    // reports Steam's path lowercase with forward slashes; libraryfolders.vdf
    // uses backslashes), so dedupe on the canonical filesystem path.
    let key = std::fs::canonicalize(&path)
        .map(|p| p.to_string_lossy().to_lowercase())
        .unwrap_or_else(|_| normalize_display(&path).to_lowercase());
    if seen.insert(key) {
        found.push(Installation {
            path: normalize_display(&path),
            source: source.to_string(),
        });
    }
}

fn normalize_display(path: &Path) -> String {
    let s = path.to_string_lossy().replace('/', "\\");
    if let Some(first) = s.chars().next().filter(|c| c.is_ascii_lowercase()) {
        if s.chars().nth(1) == Some(':') {
            return format!("{}{}", first.to_ascii_uppercase(), &s[1..]);
        }
    }
    s
}

fn steam_libraries() -> Vec<PathBuf> {
    let mut libs = Vec::new();
    let Some(root) = steam_root() else {
        return libs;
    };
    libs.push(root.clone());
    if let Ok(vdf) = std::fs::read_to_string(root.join("steamapps").join("libraryfolders.vdf")) {
        for path in parse_library_paths(&vdf) {
            libs.push(path);
        }
    }
    libs
}

#[cfg(windows)]
fn steam_root() -> Option<PathBuf> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;
    if let Ok(key) = RegKey::predef(HKEY_CURRENT_USER).open_subkey(r"Software\Valve\Steam") {
        if let Ok(path) = key.get_value::<String, _>("SteamPath") {
            return Some(PathBuf::from(path));
        }
    }
    let fallback = PathBuf::from(r"C:\Program Files (x86)\Steam");
    fallback.is_dir().then_some(fallback)
}

#[cfg(not(windows))]
fn steam_root() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let path = PathBuf::from(home).join(".steam").join("steam");
    path.is_dir().then_some(path)
}

/// Pulls every `"path" "..."` value out of Steam's libraryfolders.vdf.
fn parse_library_paths(vdf: &str) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for line in vdf.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("\"path\"") else {
            continue;
        };
        let Some(start) = rest.find('"') else { continue };
        let Some(len) = rest[start + 1..].find('"') else {
            continue;
        };
        let raw = &rest[start + 1..start + 1 + len];
        paths.push(PathBuf::from(raw.replace("\\\\", "\\")));
    }
    paths
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_display_paths() {
        assert_eq!(
            normalize_display(Path::new("c:/program files (x86)/steam\\steamapps")),
            r"C:\program files (x86)\steam\steamapps"
        );
        assert_eq!(
            normalize_display(Path::new(r"D:\SteamLibrary")),
            r"D:\SteamLibrary"
        );
    }

    #[test]
    fn deduplicates_path_spellings() {
        // Same real directory reached via two spellings must yield one entry.
        let dir = std::env::temp_dir().join("eu_toolkit_dedupe_test");
        std::fs::create_dir_all(dir.join("map")).unwrap();
        std::fs::write(dir.join("map").join("provinces.bmp"), b"x").unwrap();

        let lower = PathBuf::from(dir.to_string_lossy().to_lowercase().replace('\\', "/"));
        let mut found = Vec::new();
        let mut seen = HashSet::new();
        add_candidate(&mut found, &mut seen, dir.clone(), "Steam library");
        add_candidate(&mut found, &mut seen, lower, "Steam library");
        assert_eq!(found.len(), 1);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn parses_libraryfolders_vdf() {
        let vdf = r#"
"libraryfolders"
{
	"0"
	{
		"path"		"C:\\Program Files (x86)\\Steam"
		"label"		""
	}
	"1"
	{
		"path"		"D:\\SteamLibrary"
	}
}
"#;
        let paths = parse_library_paths(vdf);
        assert_eq!(
            paths,
            vec![
                PathBuf::from(r"C:\Program Files (x86)\Steam"),
                PathBuf::from(r"D:\SteamLibrary"),
            ]
        );
    }
}
