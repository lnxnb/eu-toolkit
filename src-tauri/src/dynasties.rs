//! Sprint 1.3 — dynasty scan/usages across history/countries.
//!
//! EU4 has no central dynasty registry: dynasties are just `dynasty = "..."`
//! strings on monarch/heir/queen blocks in `history/countries` files (both at
//! the file top level and inside dated blocks). This module scans every country
//! file once through the [`Vfs`] and aggregates the dynasties with their usages,
//! so the frontend "Choose Dynasty" modal can:
//!   * list every dynasty in game+mod with a usage count, and
//!   * generate typed edits (rename via `setScalar`, delete via
//!     `removeStatement`) for each usage without re-deriving file paths.
//!
//! ## Path adequacy vs. the surgical writer
//! Each usage carries explicit block-path segments (`path`) addressing the
//! holder block, e.g. `["1440.1.1", "monarch"]` (dated) or `["monarch"]`
//! (top-level). The frontend builds:
//!   * setScalar    → `path = [...usage.path, "dynasty"]`
//!   * removeStatement → `blockPath = usage.path, key = "dynasty"`
//!
//! `mod_writer` addresses the **first** match along a path. That is exact for
//! the common cases: each dated block key is normally unique, and a holder block
//! holds exactly one `dynasty`. Two collisions are *possible* and are a
//! documented limitation:
//!   1. Two dated blocks with the *same date* in one file — both usages share
//!      `["<date>", "monarch"]`, so only the first is addressable. The frontend
//!      dedupes edits by (file, path) so a mass edit touches the first once
//!      rather than failing on a double-remove; the exotic second block is left
//!      untouched.
//!   2. Two holders of the same kind in one dated block — same reasoning.
//! Both are extremely rare in practice (vanilla has none of the same date+holder
//! carrying different dynasties that need independent editing).

use serde::Serialize;

use crate::paradox::{self, Block, Value};
use crate::vfs::Vfs;

/// The character-block keys that carry a `dynasty` string.
const HOLDERS: [&str; 3] = ["monarch", "heir", "queen"];

/// One place a dynasty string is used.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DynastyUsage {
    /// Owning country tag, derived from the history file name ("HAB").
    pub tag: String,
    /// Game-relative file path, e.g. `history/countries/HAB - Austria.txt`.
    pub file: String,
    /// Dated-block key (`"1440.1.1"`) or `null` when the holder is top-level.
    pub date: Option<String>,
    /// `"monarch"` | `"heir"` | `"queen"`.
    pub holder: String,
    /// The holder's `name`, when present (context for the confirm dialog).
    pub holder_name: Option<String>,
    /// Block-path segments to the holder block (see module docs). The frontend
    /// appends `"dynasty"` for setScalar, or uses this as the removeStatement
    /// `blockPath` with key `"dynasty"`. E.g. `["1440.1.1", "monarch"]` or
    /// `["monarch"]`.
    pub path: Vec<String>,
}

/// A dynasty aggregated across the whole game+mod.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DynastyEntry {
    /// The dynasty string exactly as written in the files (e.g. "von Habsburg").
    pub name: String,
    /// Number of usages (holder blocks referencing this dynasty).
    pub count: usize,
    /// Every usage, in scan order.
    pub usages: Vec<DynastyUsage>,
}

/// True for a `YYYY.M.D` dated-block key (three non-empty numeric components).
fn is_date(key: &str) -> bool {
    let parts: Vec<&str> = key.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
}

/// Canonical holder name if `key` names a monarch/heir/queen (case-insensitive).
fn holder_canonical(key: &str) -> Option<&'static str> {
    HOLDERS.iter().copied().find(|h| key.eq_ignore_ascii_case(h))
}

/// Country tag from the history file name: the leading alphanumeric run,
/// uppercased ("HAB - Austria.txt" -> "HAB").
fn tag_from_filename(name: &str) -> String {
    name.chars()
        .take_while(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_uppercase()
}

/// Records the `dynasty` (if any) of one holder block into `out`.
fn collect_holder(
    holder: &'static str,
    block: &Block,
    tag: &str,
    file: &str,
    date: Option<&str>,
    path: Vec<String>,
    out: &mut Vec<(String, DynastyUsage)>,
) {
    let Some(dynasty) = block.get_scalar("dynasty") else {
        return;
    };
    out.push((
        dynasty.to_string(),
        DynastyUsage {
            tag: tag.to_string(),
            file: file.to_string(),
            date: date.map(str::to_string),
            holder: holder.to_string(),
            holder_name: block.get_scalar("name").map(str::to_string),
            path,
        },
    ));
}

/// Walks one parsed country file, collecting `(dynasty, usage)` pairs from
/// top-level and dated-block monarch/heir/queen holders.
fn collect_file(tag: &str, file: &str, root: &Block, out: &mut Vec<(String, DynastyUsage)>) {
    for (key, value) in &root.items {
        let (Some(key), Value::Block(block)) = (key, value) else {
            continue;
        };
        if let Some(holder) = holder_canonical(key) {
            // Top-level holder (e.g. a bare `monarch = { ... }`).
            collect_holder(holder, block, tag, file, None, vec![key.clone()], out);
        } else if is_date(key) {
            // Dated block: scan its holder children.
            for (hk, hb) in block.key_blocks() {
                if let Some(holder) = holder_canonical(hk) {
                    collect_holder(
                        holder,
                        hb,
                        tag,
                        file,
                        Some(key),
                        vec![key.clone(), hk.to_string()],
                        out,
                    );
                }
            }
        }
    }
}

/// Scans every `history/countries/*.txt` (mod overlays base) and aggregates the
/// dynasties. One pass, one parse per file. Entries are sorted case-insensitively
/// by name for a stable, deterministic order (the modal re-sorts as it likes).
pub fn scan(vfs: &Vfs) -> Vec<DynastyEntry> {
    let mut raw: Vec<(String, DynastyUsage)> = Vec::new();
    for (name, path) in vfs.list_dir("history/countries") {
        if !name.to_lowercase().ends_with(".txt") {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        // Decode as Latin-1 (each byte -> the same code point), not lossy UTF-8:
        // that is the exact inverse of the toolkit's `encode_latin1` writer, so a
        // dynasty name with a Windows-1252 high byte (e.g. 0xE9 -> 'é') displays
        // correctly AND round-trips byte-for-byte when written back via setScalar.
        let text: String = bytes.iter().map(|&b| b as char).collect();
        let block = paradox::parse(&text);
        let tag = tag_from_filename(&name);
        let file = format!("history/countries/{name}");
        collect_file(&tag, &file, &block, &mut raw);
    }

    let mut map: std::collections::HashMap<String, Vec<DynastyUsage>> =
        std::collections::HashMap::new();
    for (dynasty, usage) in raw {
        map.entry(dynasty).or_default().push(usage);
    }
    let mut entries: Vec<DynastyEntry> = map
        .into_iter()
        .map(|(name, usages)| DynastyEntry {
            count: usages.len(),
            name,
            usages,
        })
        .collect();
    entries.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.name.cmp(&b.name))
    });
    entries
}

/// Scans all dynasties across `history/countries` in the game+mod session.
#[tauri::command(async)]
pub fn scan_dynasties(
    install_path: String,
    mod_path: Option<String>,
) -> Result<Vec<DynastyEntry>, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    Ok(scan(&vfs))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};
    use std::time::Instant;

    const INSTALL: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Europa Universalis IV";
    const ANBENNAR: &str = r"C:\Users\User\projects\anbennar-eu4-dev";

    /// A synthetic base install with the given country history files.
    /// One dir per test — parallel tests must not share a temp dir.
    fn synthetic(name: &str, files: &[(&str, &str)]) -> (PathBuf, Vfs) {
        let root = std::env::temp_dir().join(format!("eu_toolkit_dynasty_test_{name}"));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("map")).unwrap();
        std::fs::create_dir_all(root.join("history/countries")).unwrap();
        std::fs::write(root.join("map/provinces.bmp"), b"x").unwrap();
        for (fname, body) in files {
            std::fs::write(root.join("history/countries").join(fname), body).unwrap();
        }
        let vfs = Vfs::new(root.to_str().unwrap(), None).unwrap();
        (root, vfs)
    }

    fn find<'a>(entries: &'a [DynastyEntry], name: &str) -> Option<&'a DynastyEntry> {
        entries.iter().find(|e| e.name == name)
    }

    #[test]
    fn top_level_monarch_dynasty() {
        let (_root, vfs) = synthetic(
            "toplevel",
            &[(
                "ABC - Land.txt",
                "government = monarchy\nmonarch = {\n\tname = \"Bob\"\n\tdynasty = \"Solo\"\n}\n",
            )],
        );
        let entries = scan(&vfs);
        let e = find(&entries, "Solo").expect("Solo dynasty present");
        assert_eq!(e.count, 1);
        let u = &e.usages[0];
        assert_eq!(u.tag, "ABC");
        assert_eq!(u.date, None);
        assert_eq!(u.holder, "monarch");
        assert_eq!(u.holder_name.as_deref(), Some("Bob"));
        assert_eq!(u.path, vec!["monarch".to_string()]);
        assert_eq!(u.file, "history/countries/ABC - Land.txt");
    }

    #[test]
    fn dated_monarch_heir_queen() {
        let body = "\
1440.1.1 = {
\tmonarch = {
\t\tname = \"Karl\"
\t\tdynasty = \"von Habsburg\"
\t}
\their = {
\t\tname = \"Max\"
\t\tdynasty = \"von Habsburg\"
\t}
\tqueen = {
\t\tname = \"Bianca\"
\t\tdynasty = \"Sforza\"
\t}
}
";
        let (_root, vfs) = synthetic("dated", &[("HAB - Austria.txt", body)]);
        let entries = scan(&vfs);

        let hab = find(&entries, "von Habsburg").expect("von Habsburg present");
        assert_eq!(hab.count, 2, "monarch + heir");
        // Holders present.
        let holders: Vec<&str> = hab.usages.iter().map(|u| u.holder.as_str()).collect();
        assert!(holders.contains(&"monarch"));
        assert!(holders.contains(&"heir"));
        // Dated path segments.
        let monarch = hab.usages.iter().find(|u| u.holder == "monarch").unwrap();
        assert_eq!(monarch.date.as_deref(), Some("1440.1.1"));
        assert_eq!(
            monarch.path,
            vec!["1440.1.1".to_string(), "monarch".to_string()]
        );

        let sforza = find(&entries, "Sforza").expect("Sforza present");
        assert_eq!(sforza.count, 1);
        assert_eq!(sforza.usages[0].holder, "queen");
        assert_eq!(sforza.usages[0].holder_name.as_deref(), Some("Bianca"));
        assert_eq!(
            sforza.usages[0].path,
            vec!["1440.1.1".to_string(), "queen".to_string()]
        );
    }

    #[test]
    fn duplicate_dynasty_across_countries_aggregates() {
        let a = "1440.1.1 = { monarch = { name = \"A\" dynasty = \"Shared\" } }\n";
        let b = "1441.1.1 = { monarch = { name = \"B\" dynasty = \"Shared\" } }\n";
        let (_root, vfs) = synthetic(
            "agg",
            &[("AAA - Aland.txt", a), ("BBB - Bland.txt", b)],
        );
        let entries = scan(&vfs);
        let shared = find(&entries, "Shared").expect("Shared present");
        assert_eq!(shared.count, 2);
        let tags: Vec<&str> = shared.usages.iter().map(|u| u.tag.as_str()).collect();
        assert!(tags.contains(&"AAA"));
        assert!(tags.contains(&"BBB"));
    }

    #[test]
    fn dynastyless_holder_is_ignored() {
        // A republican doge with no dynasty produces no usage.
        let body = "1440.1.1 = { monarch = { name = \"Doge\" } }\n";
        let (_root, vfs) = synthetic("nodyn", &[("VEN - Venice.txt", body)]);
        let entries = scan(&vfs);
        assert!(entries.is_empty(), "no dynasty -> no entry");
    }

    #[test]
    fn entries_sorted_by_name() {
        let body = "\
monarch = { name = \"z\" dynasty = \"Zeta\" }
1441.1.1 = { heir = { name = \"a\" dynasty = \"alpha\" } }
1442.1.1 = { queen = { name = \"m\" dynasty = \"Mu\" } }
";
        let (_root, vfs) = synthetic("sorted", &[("XXX - X.txt", body)]);
        let entries = scan(&vfs);
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["alpha", "Mu", "Zeta"]);
    }

    #[test]
    fn windows_1252_dynasty_roundtrips() {
        // 0xE9 = é in Windows-1252.
        let body = b"monarch = { name = \"x\" dynasty = \"d\xE9 Valois\" }\n";
        let (root, _vfs) = synthetic("w1252", &[]);
        std::fs::write(
            root.join("history/countries/FRA - France.txt"),
            body,
        )
        .unwrap();
        let vfs = Vfs::new(root.to_str().unwrap(), None).unwrap();
        let entries = scan(&vfs);
        assert!(find(&entries, "dé Valois").is_some(), "é decoded");
    }

    // --- real-install / Anbennar (no-op silently when absent) -----------

    fn real_install() -> Option<Vfs> {
        Path::new(INSTALL)
            .join("map")
            .join("provinces.bmp")
            .is_file()
            .then(|| Vfs::new(INSTALL, None).unwrap())
    }

    #[test]
    fn real_scan_has_von_habsburg_and_is_fast() {
        let Some(vfs) = real_install() else {
            return;
        };
        let t0 = Instant::now();
        let entries = scan(&vfs);
        let elapsed = t0.elapsed();
        eprintln!(
            "scan_dynasties: {} dynasties across history/countries in {:?}",
            entries.len(),
            elapsed
        );
        let hab = find(&entries, "von Habsburg").expect("von Habsburg exists in vanilla");
        // Austria alone uses it dozens of times across its rulers/heirs.
        assert!(
            hab.count >= 10,
            "von Habsburg count implausibly low: {}",
            hab.count
        );
        assert!(
            hab.usages.iter().any(|u| u.tag == "HAB"),
            "Austria (HAB) among von Habsburg users"
        );
        assert!(
            elapsed.as_secs_f64() < 2.0,
            "scan took {elapsed:?} (>2s budget)"
        );
    }

    #[test]
    fn anbennar_scan_smoke() {
        if !Path::new(INSTALL).join("map/provinces.bmp").is_file()
            || !Path::new(ANBENNAR).is_dir()
        {
            return;
        }
        let vfs = Vfs::new(INSTALL, Some(ANBENNAR)).unwrap();
        let entries = scan(&vfs);
        assert!(!entries.is_empty(), "Anbennar dynasties collected");
        // Every usage points at a real file path and a known holder kind.
        for e in &entries {
            assert_eq!(e.count, e.usages.len());
            for u in &e.usages {
                assert!(u.file.starts_with("history/countries/"));
                assert!(HOLDERS.contains(&u.holder.as_str()));
                assert_eq!(u.path.is_empty(), false);
            }
        }
    }
}
