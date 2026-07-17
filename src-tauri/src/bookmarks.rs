//! Sprint 12.1 — start-date bookmarks (`common/bookmarks/*.txt`).
//!
//! A bookmark defines a playable start: a localized name/description, a date, an
//! optional map `center` province, and a set of highlighted `country` tags. One
//! bookmark may carry `default = yes` (the grand-campaign start). Files are read
//! through the [`Vfs`], so a total conversion that `replace_path`s
//! `common/bookmarks` (Anbennar) hides the vanilla set and supplies its own.
//!
//! ## Effective start date
//! The whole editor's default "view/edit at" date is derived here: the default
//! bookmark's date, else the earliest bookmark's date, else `1444.11.11`. Every
//! date-parameterized command falls back to this when the frontend passes no
//! explicit date (see [`resolve_date`]).

use crate::date::{self, Date, DEFAULT_START};
use crate::loc::LocStore;
use crate::paradox::{self, Value};
use crate::vfs::Vfs;

/// Prefix for toolkit-scaffolded bookmark files (so they collate after vanilla).
pub const SCAFFOLD_PREFIX: &str = "zz_eutoolkit_";

/// An unmodeled `key = value` inside a bookmark block (preserve-unknown).
#[derive(Debug, Clone, serde::Serialize)]
pub struct RawKv {
    pub key: String,
    pub value: String,
}

/// One parsed bookmark, loc-resolved.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bookmark {
    /// Game-relative file the bookmark lives in.
    pub file: String,
    /// `name` loc key exactly as written.
    pub name_key: String,
    /// Resolved localized name (falls back to the key prettified).
    pub name: String,
    /// `desc` loc key exactly as written.
    pub desc_key: String,
    /// Resolved localized description.
    pub desc: String,
    /// `date` as written, e.g. `1444.11.11`.
    pub date: String,
    /// `default = yes`.
    pub is_default: bool,
    /// `center = <province>`.
    pub center: Option<u32>,
    /// Highlighted `country` tags, in file order.
    pub countries: Vec<String>,
    /// Unmodeled scalar keys in the block (e.g. `easy_country`), preserved.
    pub extras: Vec<RawKv>,
}

const MODELED: &[&str] = &["name", "desc", "date", "default", "center", "country"];

/// Parses every `bookmark = { … }` block in one file's bytes.
fn parse_file(rel: &str, bytes: &[u8], loc: &LocStore) -> Vec<Bookmark> {
    let block = paradox::parse(&String::from_utf8_lossy(bytes));
    let mut out = Vec::new();
    for (key, b) in block.key_blocks() {
        if key != "bookmark" {
            continue;
        }
        let name_key = b.get_scalar("name").unwrap_or_default().to_string();
        let desc_key = b.get_scalar("desc").unwrap_or_default().to_string();
        let date = b.get_scalar("date").unwrap_or_default().to_string();
        let is_default = b.get_scalar("default") == Some("yes");
        let center = b.get_scalar("center").and_then(|s| s.parse().ok());
        let countries = b
            .items
            .iter()
            .filter_map(|(k, v)| match (k, v) {
                (Some(k), Value::Scalar(s)) if k == "country" => Some(s.clone()),
                _ => None,
            })
            .collect();
        let extras = b
            .items
            .iter()
            .filter_map(|(k, v)| match (k, v) {
                (Some(k), Value::Scalar(s)) if !MODELED.contains(&k.as_str()) => Some(RawKv {
                    key: k.clone(),
                    value: s.clone(),
                }),
                _ => None,
            })
            .collect();
        out.push(Bookmark {
            file: rel.to_string(),
            name: if name_key.is_empty() {
                String::new()
            } else {
                loc.resolve(&name_key)
            },
            name_key,
            desc: if desc_key.is_empty() {
                String::new()
            } else {
                loc.resolve(&desc_key)
            },
            desc_key,
            date,
            is_default,
            center,
            countries,
            extras,
        });
    }
    out
}

/// Every bookmark in `common/bookmarks`, loc-resolved, sorted by date (undated
/// last), then by name for stability.
pub fn all_bookmarks(vfs: &Vfs, loc: &LocStore) -> Vec<Bookmark> {
    let mut out = Vec::new();
    for (name, path) in vfs.list_dir("common/bookmarks") {
        if !name.to_lowercase().ends_with(".txt") {
            continue;
        }
        if let Ok(bytes) = std::fs::read(&path) {
            out.extend(parse_file(&format!("common/bookmarks/{name}"), &bytes, loc));
        }
    }
    out.sort_by(|a, b| {
        let da = date::parse_date(&a.date);
        let db = date::parse_date(&b.date);
        // Some(date) sorts before None (undated), then by date, then by name.
        match (da, db) {
            (Some(x), Some(y)) => x.cmp(&y).then_with(|| a.name.cmp(&b.name)),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => a.name.cmp(&b.name),
        }
    });
    out
}

/// The effective start date for this session: the default bookmark's date, else
/// the earliest bookmark's date, else `1444.11.11`. Reads no localisation.
pub fn effective_start_date(vfs: &Vfs) -> Date {
    let mut dates: Vec<Date> = Vec::new();
    let mut default_date: Option<Date> = None;
    for (name, path) in vfs.list_dir("common/bookmarks") {
        if !name.to_lowercase().ends_with(".txt") {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let block = paradox::parse(&String::from_utf8_lossy(&bytes));
        for (key, b) in block.key_blocks() {
            if key != "bookmark" {
                continue;
            }
            let Some(d) = b.get_scalar("date").and_then(date::parse_date) else {
                continue;
            };
            dates.push(d);
            if b.get_scalar("default") == Some("yes") {
                // Earliest default wins if (pathologically) several are marked.
                default_date = Some(default_date.map_or(d, |cur| cur.min(d)));
            }
        }
    }
    default_date
        .or_else(|| dates.into_iter().min())
        .unwrap_or(DEFAULT_START)
}

/// Resolves a command's optional `date` argument: the parsed value if given,
/// else the session's [`effective_start_date`]. The single choke point every
/// date-parameterized command shares.
pub fn resolve_date(vfs: &Vfs, date: Option<&str>) -> Result<Date, String> {
    match date {
        Some(s) => date::parse_date(s).ok_or_else(|| format!("Invalid date: {s}")),
        None => Ok(effective_start_date(vfs)),
    }
}

/// Tauri command: all bookmarks (base + mod through the Vfs), loc-resolved,
/// sorted by date.
#[tauri::command(async)]
pub fn get_bookmarks(
    install_path: String,
    mod_path: Option<String>,
) -> Result<Vec<Bookmark>, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    let loc = crate::loc::store(&vfs, &install_path, mod_path.as_deref());
    Ok(all_bookmarks(&vfs, &loc))
}

// --- Bookmark scaffold (12.1) ----------------------------------------------

/// The result of preparing a new-start-date bookmark: the file + its content
/// (the frontend queues a `CreateFile` for it), the loc keys it references (the
/// frontend queues `LocOverride`s for name/desc), and whether the date needs a
/// defines override to be playable.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BookmarkScaffold {
    /// `common/bookmarks/zz_eutoolkit_<key>.txt`.
    pub file: String,
    /// Full file text for a `CreateFile` edit (references `name_key`/`desc_key`).
    pub text: String,
    /// Loc key the frontend must `LocOverride` with the display name.
    pub name_key: String,
    /// Loc key for the description.
    pub desc_key: String,
    /// The requested date is outside the effective `START_DATE`/`END_DATE` — the
    /// frontend must ALSO write a defines override so the date is playable.
    pub out_of_range: bool,
    /// Effective playable start bound (for the range message).
    pub range_start: String,
    /// Effective playable end bound.
    pub range_end: String,
}

/// Normalizes a user key into a filesystem/loc-safe token.
fn sanitize_key(key: &str) -> String {
    let s: String = key
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
        .collect();
    let trimmed = s.trim_matches('_');
    if trimmed.is_empty() {
        "bookmark".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Builds a new-bookmark scaffold. `date`/`center`/`countries`/`make_default`
/// shape the block; the loc keys are derived from `key`.
pub fn build_scaffold(
    vfs: &Vfs,
    key: &str,
    date: &str,
    center: Option<u32>,
    countries: &[String],
    make_default: bool,
) -> Result<BookmarkScaffold, String> {
    let parsed = date::parse_date(date).ok_or_else(|| format!("Invalid date: {date}"))?;
    let sanitized = sanitize_key(key);
    let file = format!("common/bookmarks/{SCAFFOLD_PREFIX}{sanitized}.txt");
    let name_key = format!("BM_EUTK_{}_NAME", sanitized.to_uppercase());
    let desc_key = format!("BM_EUTK_{}_DESC", sanitized.to_uppercase());

    let mut text = String::new();
    text.push_str("bookmark = {\n");
    text.push_str(&format!("\tname = \"{name_key}\"\n"));
    text.push_str(&format!("\tdesc = \"{desc_key}\"\n"));
    text.push_str(&format!("\tdate = {date}\n"));
    if let Some(c) = center {
        text.push_str(&format!("\tcenter = {c}\n"));
    }
    if make_default {
        text.push_str("\tdefault = yes\n");
    }
    for tag in countries {
        text.push_str(&format!("\tcountry = {tag}\n"));
    }
    text.push_str("}\n");

    let bounds = crate::defines::defines_dates(vfs);
    let start = date::parse_date(&bounds.start_date);
    let end = date::parse_date(&bounds.end_date);
    let out_of_range = start.is_some_and(|s| parsed < s) || end.is_some_and(|e| parsed > e);

    Ok(BookmarkScaffold {
        file,
        text,
        name_key,
        desc_key,
        out_of_range,
        range_start: bounds.start_date,
        range_end: bounds.end_date,
    })
}

/// Tauri command: prepare a new-start-date bookmark scaffold.
#[tauri::command(async)]
pub fn scaffold_bookmark(
    install_path: String,
    mod_path: Option<String>,
    key: String,
    date: String,
    center: Option<u32>,
    countries: Option<Vec<String>>,
    make_default: Option<bool>,
) -> Result<BookmarkScaffold, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    build_scaffold(
        &vfs,
        &key,
        &date,
        center,
        &countries.unwrap_or_default(),
        make_default.unwrap_or(false),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    const INSTALL: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Europa Universalis IV";
    const ANBENNAR: &str = r"C:\Users\User\projects\anbennar-eu4-dev";
    /// Imperium Universalis 3.1.2 workshop item (classical-era total conversion).
    const IMPERIUM: &str =
        r"C:\Program Files (x86)\Steam\steamapps\workshop\content\236850\679204773";

    fn loc_empty() -> LocStore {
        LocStore::from_pairs(&[])
    }

    /// A synthetic install with the given bookmark files. One dir per test.
    fn synthetic(name: &str, files: &[(&str, &str)]) -> (PathBuf, Vfs) {
        let root = std::env::temp_dir().join(format!("eu_toolkit_bookmarks_test_{name}"));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("map")).unwrap();
        std::fs::create_dir_all(root.join("common/bookmarks")).unwrap();
        std::fs::write(root.join("map/provinces.bmp"), b"x").unwrap();
        for (rel, contents) in files {
            std::fs::write(root.join(rel), contents).unwrap();
        }
        let vfs = Vfs::new(root.to_str().unwrap(), None).unwrap();
        (root, vfs)
    }

    #[test]
    fn parses_fields_and_sorts_by_date() {
        let (_r, vfs) = synthetic(
            "fields",
            &[
                (
                    "common/bookmarks/a.txt",
                    "bookmark = {\n\tname = \"GC\"\n\tdesc = \"GC_DESC\"\n\tdate = 1444.11.11\n\tcenter = 112\n\tdefault = yes\n\tcountry = TUR\n\tcountry = CAS\n\teasy_country = TUR\n}\n",
                ),
                (
                    "common/bookmarks/b.txt",
                    "bookmark = {\n\tname = \"FOB\"\n\tdesc = \"FOB_DESC\"\n\tdate = 1453.5.29\n\tcountry = TUR\n}\n",
                ),
            ],
        );
        let bms = all_bookmarks(&vfs, &loc_empty());
        assert_eq!(bms.len(), 2);
        // Sorted by date.
        assert_eq!(bms[0].date, "1444.11.11");
        assert_eq!(bms[1].date, "1453.5.29");
        let gc = &bms[0];
        assert_eq!(gc.name_key, "GC");
        assert_eq!(gc.center, Some(112));
        assert!(gc.is_default);
        assert_eq!(gc.countries, vec!["TUR".to_string(), "CAS".to_string()]);
        // easy_country preserved as an extra, not dropped or treated as country.
        assert!(gc.extras.iter().any(|e| e.key == "easy_country" && e.value == "TUR"));
    }

    #[test]
    fn effective_start_prefers_default_then_earliest() {
        // Default wins even if a later date than another bookmark.
        let (_r, vfs) = synthetic(
            "default",
            &[
                ("common/bookmarks/a.txt", "bookmark = {\n\tdate = 1490.1.1\n\tdefault = yes\n}\n"),
                ("common/bookmarks/b.txt", "bookmark = {\n\tdate = 1444.11.11\n}\n"),
            ],
        );
        assert_eq!(effective_start_date(&vfs), (1490, 1, 1));

        // No default: earliest wins.
        let (_r2, vfs2) = synthetic(
            "earliest",
            &[
                ("common/bookmarks/a.txt", "bookmark = {\n\tdate = 1490.1.1\n}\n"),
                ("common/bookmarks/b.txt", "bookmark = {\n\tdate = 1444.11.11\n}\n"),
            ],
        );
        assert_eq!(effective_start_date(&vfs2), (1444, 11, 11));

        // No bookmarks: vanilla fallback.
        let (_r3, vfs3) = synthetic("none", &[]);
        assert_eq!(effective_start_date(&vfs3), DEFAULT_START);
    }

    #[test]
    fn resolve_date_uses_arg_or_effective() {
        let (_r, vfs) = synthetic("resolve", &[("common/bookmarks/a.txt", "bookmark = {\n\tdate = 1444.11.11\n\tdefault = yes\n}\n")]);
        assert_eq!(resolve_date(&vfs, Some("1453.5.29")).unwrap(), (1453, 5, 29));
        assert_eq!(resolve_date(&vfs, None).unwrap(), (1444, 11, 11));
        assert!(resolve_date(&vfs, Some("garbage")).is_err());
    }

    #[test]
    fn scaffold_references_loc_keys_and_flags_range() {
        let (_r, vfs) = synthetic("scaffold", &[]);
        // No defines.lua → vanilla fallback range 1444.11.11..1821.1.2.
        let s = build_scaffold(&vfs, "My Start!", "1300.1.1", Some(1), &["FRA".into()], true).unwrap();
        assert_eq!(s.file, "common/bookmarks/zz_eutoolkit_my_start.txt");
        assert_eq!(s.name_key, "BM_EUTK_MY_START_NAME");
        assert_eq!(s.desc_key, "BM_EUTK_MY_START_DESC");
        assert!(s.text.contains("name = \"BM_EUTK_MY_START_NAME\""));
        assert!(s.text.contains("date = 1300.1.1"));
        assert!(s.text.contains("center = 1"));
        assert!(s.text.contains("default = yes"));
        assert!(s.text.contains("country = FRA"));
        // 1300 is before the vanilla start bound → out of range.
        assert!(s.out_of_range);

        // In-range date is not flagged.
        let s2 = build_scaffold(&vfs, "mid", "1500.1.1", None, &[], false).unwrap();
        assert!(!s2.out_of_range);
    }

    #[test]
    fn real_vanilla_bookmark_set() {
        if !Path::new(INSTALL).join("map/provinces.bmp").is_file() {
            return;
        }
        let vfs = Vfs::new(INSTALL, None).unwrap();
        let loc = crate::loc::build(&vfs);
        let bms = all_bookmarks(&vfs, &loc);
        // The grand-campaign default is rise_of_the_ottomans @ 1444.11.11.
        let def = bms.iter().find(|b| b.is_default).expect("a default bookmark");
        assert_eq!(def.date, "1444.11.11");
        assert!(def.file.contains("rise_of_the_ottomans"));
        // The fall of Byzantium bookmark is dated 1453.5.29.
        assert!(bms.iter().any(|b| b.date == "1453.5.29"));
        // Four bookmarks share the 1444.11.11 start (GC trio + rise_of_the_ottomans).
        assert_eq!(bms.iter().filter(|b| b.date == "1444.11.11").count(), 4);
        // Effective start = the default's date.
        assert_eq!(effective_start_date(&vfs), (1444, 11, 11));
    }

    #[test]
    fn anbennar_custom_bookmarks_via_replace_path() {
        if !Path::new(INSTALL).join("map/provinces.bmp").is_file() || !Path::new(ANBENNAR).is_dir() {
            return;
        }
        let vfs = Vfs::new(INSTALL, Some(ANBENNAR)).unwrap();
        let loc = crate::loc::build(&vfs);
        let bms = all_bookmarks(&vfs, &loc);
        // Anbennar replace_paths common/bookmarks: we must see its custom set, and
        // NOT the vanilla files (rise_of_the_ottomans is hidden).
        assert!(!bms.is_empty(), "expected Anbennar bookmarks");
        assert!(
            !bms.iter().any(|b| b.file.contains("rise_of_the_ottomans")),
            "vanilla bookmark leaked past replace_path"
        );
        // An effective start still resolves.
        assert!(date::parse_date(&date::format_date(effective_start_date(&vfs))).is_some());
    }

    #[test]
    fn imperium_universalis_bookmarks_parse() {
        if !Path::new(INSTALL).join("map/provinces.bmp").is_file()
            || !Path::new(IMPERIUM).join("descriptor.mod").is_file()
        {
            return; // game or IU workshop item absent: no-op
        }
        let vfs = Vfs::new(INSTALL, Some(IMPERIUM)).unwrap();
        let loc = crate::loc::build(&vfs);
        let bms = all_bookmarks(&vfs, &loc);
        // IU ships a full classical-era bookmark set (Alexander, Punic wars, …).
        assert!(bms.len() > 5, "expected IU's bookmark set, got {}", bms.len());
        // Every bookmark carries a parseable, classical-era (pre-1444) date.
        assert!(bms.iter().all(|b| date::parse_date(&b.date).is_some()));
        // Alexander the Great's campaign bookmark is dated 418.1.1 (AUC).
        let alex = bms
            .iter()
            .find(|b| b.name_key == "ALEXANDER_TITLE")
            .expect("Alexander bookmark");
        assert_eq!(alex.date, "418.1.1");
        // The effective start is the earliest of these, far before the vanilla start.
        assert!(effective_start_date(&vfs) < (1444, 11, 11));
    }
}
