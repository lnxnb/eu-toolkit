//! Sprint 27 — bespoke color-pool editor for the two NON-KEYED `common/`
//! registries that don't fit the mechanics.rs keyed-entry [`Family`] model:
//!
//!   * `common/custom_country_colors/*.txt` — nation-designer country colors
//!     (repeated `color = { r g b }`), custom-flag `flag_color = { r g b }`
//!     blocks, a `num_symbols = N` scalar, and a trailing `textures = { … }`
//!     block (preserved, shown read-only).
//!   * `common/dynasty_colors/*.txt` — repeated `color = { r g b }` only.
//!
//! Neither has a per-entry key / loc / scaffold, so the keyed Family core can't
//! host them. Everything here reads/writes through the [`Vfs`]; edits are
//! byte-surgical via the shared occurrence-indexed (`color#n`) typed-edit
//! vocabulary in [`crate::mod_writer`], so comments, formatting, the `textures`
//! block, and Windows-1252 bytes all round-trip untouched.
//!
//! # Load semantics
//! The game loads every file in each directory additively and concatenates the
//! color lists. A mod file that shares a vanilla file's name (`00_…txt`) shadows
//! it (same-name shadow rule); a differently-named file (`zz_eutoolkit_…txt`)
//! ADDS its colors to the base pool. The frontend surfaces both facts: editing an
//! existing base file copies it into the project (copy-on-write, all colors
//! preserved) and shadows base; creating a fresh `zz_` file adds to the pool.

use crate::paradox::{self, Value};
use crate::vfs::Vfs;

/// (pool id, directory) for the two registries.
const POOLS: &[(&str, &str)] = &[
    ("custom_country_colors", "common/custom_country_colors"),
    ("dynasty_colors", "common/dynasty_colors"),
];

/// The two repeated block keys that carry an RGB triple.
const COLOR_KEYS: &[&str] = &["color", "flag_color"];

#[derive(serde::Serialize)]
pub struct ColorEntry {
    /// `color` or `flag_color` — the repeated block key.
    pub key: String,
    /// 0-based occurrence within this key group. The `key#n` edit index that
    /// `mod_writer` occurrence addressing resolves (bare `key` for n = 0).
    pub occ: usize,
    pub r: u8,
    pub g: u8,
    pub b: u8,
    /// Best-effort trailing `# name` comment, for the swatch label.
    pub comment: Option<String>,
}

#[derive(serde::Serialize)]
pub struct ScalarEntry {
    pub key: String,
    pub value: String,
}

#[derive(serde::Serialize)]
pub struct PoolFile {
    pub pool: String,
    /// Vfs-relative path, forward-slashed (the edit target).
    pub rel: String,
    /// `base` or `mod`.
    pub origin: String,
    pub colors: Vec<ColorEntry>,
    pub scalars: Vec<ScalarEntry>,
    /// Unmodeled top-level block keys preserved on write (e.g. `textures`).
    pub extra_keys: Vec<String>,
}

#[derive(serde::Serialize)]
pub struct PoolGroup {
    pub pool: String,
    pub dir: String,
    /// Suggested project path for a brand-new additive pool file.
    pub new_file: String,
    pub files: Vec<PoolFile>,
}

#[derive(serde::Serialize)]
pub struct ColorPools {
    pub groups: Vec<PoolGroup>,
}

fn origin_of(vfs: &Vfs, path: &std::path::Path) -> &'static str {
    if vfs.mod_dir().is_some_and(|m| path.starts_with(m)) {
        "mod"
    } else {
        "base"
    }
}

/// Best-effort trailing-comment scan: for each single-line `key = { … } # cmt`
/// top-level statement, in file order, capture the comment text (or `None`).
/// Only single-line color blocks are recognized — a multi-line block would break
/// occurrence alignment, so the caller drops a key's comments entirely when the
/// scan count disagrees with the parser's color count for that key.
fn scan_comments(text: &str) -> std::collections::HashMap<&'static str, Vec<Option<String>>> {
    let mut out: std::collections::HashMap<&'static str, Vec<Option<String>>> =
        std::collections::HashMap::new();
    for raw in text.lines() {
        let line = raw.trim_start();
        for &key in COLOR_KEYS {
            let Some(rest) = line.strip_prefix(key) else {
                continue;
            };
            // The char after the key must be whitespace or `=`, else it's a
            // longer identifier (`colorful = …`) that merely shares the prefix.
            match rest.chars().next() {
                Some(c) if c.is_whitespace() || c == '=' => {}
                _ => continue,
            }
            if !rest.contains('=') || !rest.contains('{') || !rest.contains('}') {
                continue;
            }
            // Comment (if any) starts at the first `#` after the closing brace.
            let after = &raw[raw.rfind('}').unwrap() + 1..];
            let comment = after
                .find('#')
                .map(|i| after[i + 1..].trim().to_string())
                .filter(|s| !s.is_empty());
            out.entry(key).or_default().push(comment);
            break;
        }
    }
    out
}

fn parse_pool_file(
    pool: &str,
    rel: &str,
    origin: &str,
    bytes: &[u8],
) -> PoolFile {
    let text = String::from_utf8_lossy(bytes);
    let block = paradox::parse(&text);
    let scanned = scan_comments(&text);

    // First pass: count colors per key so we can validate comment alignment.
    let mut counts: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for (k, v) in &block.items {
        if let (Some(key), Value::Block(b)) = (k, v) {
            if COLOR_KEYS.contains(&key.as_str()) && paradox::color_from_block(b).is_some() {
                *counts.entry(key.as_str()).or_default() += 1;
            }
        }
    }

    let mut colors = Vec::new();
    let mut scalars = Vec::new();
    let mut extra_keys = Vec::new();
    let mut occ: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();

    for (k, v) in &block.items {
        match (k, v) {
            (Some(key), Value::Block(b)) if COLOR_KEYS.contains(&key.as_str()) => {
                let Some(rgb) = paradox::color_from_block(b) else {
                    // A malformed `color = { … }` we can't read as RGB: preserve
                    // it as an unknown key rather than lose track of it.
                    if !extra_keys.iter().any(|e| e == key) {
                        extra_keys.push(key.clone());
                    }
                    continue;
                };
                let n = occ.entry(key.as_str()).or_default();
                let this = *n;
                *n += 1;
                // Only trust the scanned comment when its count matches the
                // parser's, i.e. every color of this key is single-line.
                let aligned = scanned
                    .get(key.as_str())
                    .map(|v| v.len() == counts.get(key.as_str()).copied().unwrap_or(0))
                    .unwrap_or(false);
                let comment = if aligned {
                    scanned
                        .get(key.as_str())
                        .and_then(|v| v.get(this).cloned())
                        .flatten()
                } else {
                    None
                };
                colors.push(ColorEntry {
                    key: key.clone(),
                    occ: this,
                    r: rgb[0],
                    g: rgb[1],
                    b: rgb[2],
                    comment,
                });
            }
            (Some(key), Value::Scalar(s)) => scalars.push(ScalarEntry {
                key: key.clone(),
                value: s.clone(),
            }),
            (Some(key), Value::Block(_)) => {
                if !extra_keys.iter().any(|e| e == key) {
                    extra_keys.push(key.clone());
                }
            }
            _ => {}
        }
    }

    PoolFile {
        pool: pool.to_string(),
        rel: rel.to_string(),
        origin: origin.to_string(),
        colors,
        scalars,
        extra_keys,
    }
}

fn load(vfs: &Vfs) -> ColorPools {
    let mut groups = Vec::new();
    for &(pool, dir) in POOLS {
        let mut files = Vec::new();
        for (name, path) in vfs.list_dir(dir) {
            if !name.to_lowercase().ends_with(".txt") {
                continue;
            }
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            let rel = format!("{dir}/{name}");
            let origin = origin_of(vfs, &path);
            files.push(parse_pool_file(pool, &rel, origin, &bytes));
        }
        groups.push(PoolGroup {
            pool: pool.to_string(),
            dir: dir.to_string(),
            new_file: format!("{dir}/zz_eutoolkit_{pool}.txt"),
            files,
        });
    }
    ColorPools { groups }
}

#[tauri::command]
pub fn get_color_pools(
    install_path: String,
    mod_path: Option<String>,
) -> Result<ColorPools, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    Ok(load(&vfs))
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

    const FIXTURE: &str = "num_symbols = 3\n\
        # Whites\n\
        color = { 235 235 235 }\t# White\n\
        color = { 240 255 240 }\t# Honeydew\n\
        color = { 10 20 30 }\n\
        \n\
        flag_color = { 200 0 0 }\t#0  Red\n\
        flag_color = { 0 200 0 }\t#1  Green\n\
        textures = {\n\
        \ttexture = { file = \"a.tga\" }\n\
        }\n";

    #[test]
    fn parses_fixture_shape() {
        let f = parse_pool_file("custom_country_colors", "rel.txt", "mod", FIXTURE.as_bytes());
        let colors: Vec<_> = f.colors.iter().filter(|c| c.key == "color").collect();
        let flags: Vec<_> = f.colors.iter().filter(|c| c.key == "flag_color").collect();
        assert_eq!(colors.len(), 3);
        assert_eq!(flags.len(), 2);
        // Occurrences are per-key and sequential.
        assert_eq!(colors.iter().map(|c| c.occ).collect::<Vec<_>>(), vec![0, 1, 2]);
        assert_eq!(flags.iter().map(|c| c.occ).collect::<Vec<_>>(), vec![0, 1]);
        assert_eq!((colors[0].r, colors[0].g, colors[0].b), (235, 235, 235));
        assert_eq!(colors[0].comment.as_deref(), Some("White"));
        assert_eq!(colors[2].comment, None);
        assert_eq!(f.scalars.len(), 1);
        assert_eq!(f.scalars[0].key, "num_symbols");
        assert_eq!(f.scalars[0].value, "3");
        assert_eq!(f.extra_keys, vec!["textures".to_string()]);
    }

    #[test]
    fn rgb_edit_is_byte_surgical_and_occurrence_indexed() {
        // Edit the 2nd color (occ 1, "Honeydew") only.
        let out = apply(
            FIXTURE.as_bytes(),
            &Edit::SetBlock {
                path: vec!["color#1".to_string()],
                value: "1 2 3".to_string(),
            },
        )
        .unwrap();
        let f = parse_pool_file("p", "rel.txt", "mod", &out);
        let colors: Vec<_> = f.colors.iter().filter(|c| c.key == "color").collect();
        assert_eq!((colors[1].r, colors[1].g, colors[1].b), (1, 2, 3));
        // Neighbors untouched.
        assert_eq!((colors[0].r, colors[0].g, colors[0].b), (235, 235, 235));
        assert_eq!((colors[2].r, colors[2].g, colors[2].b), (10, 20, 30));
        // Byte-surgical: only the one triple changed; the White comment survives,
        // and the flag_color occ-1 is unaffected (separate key group).
        let text = String::from_utf8_lossy(&out);
        assert!(text.contains("color = { 1 2 3 }\t# Honeydew"));
        assert!(text.contains("# White"));
        assert!(text.contains("flag_color = { 0 200 0 }"));
    }

    #[test]
    fn add_color_round_trips() {
        let out = apply(
            FIXTURE.as_bytes(),
            &Edit::InsertStatement {
                block_path: vec![],
                statement: "color = { 5 6 7 }".to_string(),
            },
        )
        .unwrap();
        let f = parse_pool_file("p", "rel.txt", "mod", &out);
        let colors: Vec<_> = f.colors.iter().filter(|c| c.key == "color").collect();
        assert_eq!(colors.len(), 4);
        let last = colors.last().unwrap();
        assert_eq!((last.r, last.g, last.b), (5, 6, 7));
        assert_eq!(last.occ, 3);
    }

    #[test]
    fn remove_color_by_occurrence_round_trips() {
        // Remove the 1st color (occ 0, "White").
        let out = apply(
            FIXTURE.as_bytes(),
            &Edit::RemoveStatement {
                block_path: vec![],
                key: "color#0".to_string(),
                value: None,
            },
        )
        .unwrap();
        let f = parse_pool_file("p", "rel.txt", "mod", &out);
        let colors: Vec<_> = f.colors.iter().filter(|c| c.key == "color").collect();
        assert_eq!(colors.len(), 2);
        // Honeydew is now first; flag_colors untouched.
        assert_eq!(colors[0].comment.as_deref(), Some("Honeydew"));
        assert_eq!(f.colors.iter().filter(|c| c.key == "flag_color").count(), 2);
    }

    #[test]
    fn scalar_edit_round_trips() {
        let out = apply(
            FIXTURE.as_bytes(),
            &Edit::SetScalar {
                path: vec!["num_symbols".to_string()],
                value: "9".to_string(),
                quoted: false,
            },
        )
        .unwrap();
        let f = parse_pool_file("p", "rel.txt", "mod", &out);
        assert_eq!(f.scalars[0].value, "9");
    }

    #[test]
    fn create_file_additive_pool() {
        let text = "color = { 12 34 56 }\ncolor = { 78 90 12 }\n";
        let out = apply(b"", &Edit::CreateFile { text: text.to_string() }).unwrap();
        let f = parse_pool_file("dynasty_colors", "rel.txt", "mod", &out);
        assert_eq!(f.colors.len(), 2);
        assert_eq!((f.colors[0].r, f.colors[0].g, f.colors[0].b), (12, 34, 56));
    }

    #[test]
    fn vanilla_parses_both_pools() {
        if !install_present() {
            return;
        }
        let vfs = Vfs::new(INSTALL, None).unwrap();
        let pools = load(&vfs);
        assert_eq!(pools.groups.len(), 2);

        let custom = pools
            .groups
            .iter()
            .find(|g| g.pool == "custom_country_colors")
            .unwrap();
        assert!(!custom.files.is_empty());
        let cf = &custom.files[0];
        assert_eq!(cf.origin, "base");
        let n_color = cf.colors.iter().filter(|c| c.key == "color").count();
        let n_flag = cf.colors.iter().filter(|c| c.key == "flag_color").count();
        assert!(n_color > 0, "expected country colors");
        assert!(n_flag > 0, "expected flag colors");
        assert!(cf.scalars.iter().any(|s| s.key == "num_symbols"));
        assert!(cf.extra_keys.iter().any(|k| k == "textures"));
        // At least one color carried a readable name comment.
        assert!(cf.colors.iter().any(|c| c.comment.is_some()));

        let dyn_ = pools
            .groups
            .iter()
            .find(|g| g.pool == "dynasty_colors")
            .unwrap();
        assert!(!dyn_.files.is_empty());
        assert!(dyn_.files[0].colors.iter().all(|c| c.key == "color"));
        assert!(dyn_.files[0].colors.len() > 100);
    }

    #[test]
    fn vanilla_round_trip_first_country_color() {
        if !install_present() {
            return;
        }
        let vfs = Vfs::new(INSTALL, None).unwrap();
        let pools = load(&vfs);
        let cf = &pools
            .groups
            .iter()
            .find(|g| g.pool == "custom_country_colors")
            .unwrap()
            .files[0];
        let bytes = std::fs::read(vfs.resolve(&cf.rel).unwrap()).unwrap();
        let out = apply(
            &bytes,
            &Edit::SetBlock {
                path: vec!["color#0".to_string()],
                value: "1 2 3".to_string(),
            },
        )
        .unwrap();
        let f = parse_pool_file("custom_country_colors", &cf.rel, "base", &out);
        let first = f.colors.iter().find(|c| c.key == "color").unwrap();
        assert_eq!((first.r, first.g, first.b), (1, 2, 3));
        // Everything else preserved: same total counts.
        assert_eq!(f.colors.len(), cf.colors.len());
        assert_eq!(f.extra_keys, cf.extra_keys);
    }

    #[test]
    fn anbennar_smoke() {
        if !install_present() || !Path::new(ANBENNAR).is_dir() {
            return;
        }
        let vfs = Vfs::new(INSTALL, Some(ANBENNAR)).unwrap();
        let pools = load(&vfs);
        // Anbennar ships no color-pool files of its own, so the base files load
        // and every file reads as base origin. Parse must still succeed.
        for g in &pools.groups {
            for f in &g.files {
                assert!(!f.colors.is_empty(), "{}: empty", f.rel);
            }
        }
    }
}
