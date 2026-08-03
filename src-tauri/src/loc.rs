//! Localisation system: parses the game's `*_l_english.yml` files through the
//! Vfs, resolves keys with a mod-over-base-over-prettified fallback chain, and
//! writes toolkit-owned overrides for renames.
//!
//! Loading every english loc file is a few MB across hundreds of files, so it
//! must not happen per hover. A process-level cache memoizes the built
//! [`LocStore`] keyed by `(install_path, mod_path)`; the toolkit invalidates it
//! whenever it writes loc. Commands stay stateless — they just ask the cache.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

use crate::vfs::Vfs;

/// English loc file suffix. Only english is needed.
const ENGLISH_SUFFIX: &str = "_l_english.yml";

/// The single toolkit-owned override file. Lives under `localisation/replace/`
/// with a `zz_` prefix so it collates last and wins over any other mod file.
pub const OVERRIDE_REL: &str = "localisation/replace/zz_eutoolkit_l_english.yml";

/// Resolved key -> localized string. Built once per (install, mod) and cached.
pub struct LocStore {
    map: HashMap<String, String>,
}

impl LocStore {
    /// Raw lookup: the localized string for `key`, or None if no file defines it.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.map.get(key).map(String::as_str)
    }

    /// Fallback chain: mod loc -> base loc -> prettified raw key.
    pub fn resolve(&self, key: &str) -> String {
        match self.map.get(key) {
            Some(v) => v.clone(),
            None => prettify(key),
        }
    }

    /// Like [`resolve`], but falls back to `default` instead of prettifying —
    /// used where a better human-readable default exists (e.g. a file stem).
    pub fn resolve_or(&self, key: &str, default: &str) -> String {
        self.map
            .get(key)
            .map(String::clone)
            .unwrap_or_else(|| default.to_string())
    }

    #[cfg(test)]
    pub fn from_pairs(pairs: &[(&str, &str)]) -> LocStore {
        LocStore {
            map: pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        }
    }
}

/// `TAG_ideas` -> "Tag Ideas". Splits on `_` and title-cases each word; matches
/// the frontend's `pretty()` so raw-key display stays consistent everywhere.
pub fn prettify(key: &str) -> String {
    key.split('_')
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Parses one loc file's UTF-8 text into `map` (later keys overwrite earlier).
/// Tolerant of: a UTF-8 BOM, the `l_english:` header, optional `:0` version
/// numbers, `#` comment lines, embedded quotes in values, and CRLF vs LF.
pub fn parse_into(text: &str, map: &mut HashMap<String, String>) {
    for raw in text.lines() {
        // Strip a BOM if the first line carries one; trim trailing CR (CRLF).
        let line = raw.trim_start_matches('\u{feff}').trim_end_matches('\r');
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        // key : [version] "value"  — key is everything before the first colon.
        let Some(colon) = line.find(':') else {
            continue;
        };
        let key = line[..colon].trim();
        if key.is_empty() {
            continue;
        }
        let rest = &line[colon + 1..];
        // Value is between the first and last quote on the line — this lets
        // embedded quotes survive without a real escape grammar. Lines with no
        // quote (the `l_english:` header, blank keys) are skipped here.
        let Some(q1) = rest.find('"') else {
            continue;
        };
        let after = &rest[q1 + 1..];
        let Some(q2) = after.rfind('"') else {
            continue;
        };
        map.insert(key.to_string(), after[..q2].to_string());
    }
}

// --- Process-level cache -------------------------------------------------

type CacheKey = (String, Option<String>);
type Cache = HashMap<CacheKey, Arc<LocStore>>;

fn cache() -> &'static Mutex<Cache> {
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The memoized [`LocStore`] for this session. Builds it (reading every english
/// loc file through the Vfs) on first use, then serves the cached copy.
pub fn store(vfs: &Vfs, install_path: &str, mod_path: Option<&str>) -> Arc<LocStore> {
    let key: CacheKey = (install_path.to_string(), mod_path.map(str::to_string));
    if let Some(hit) = cache().lock().unwrap().get(&key) {
        return hit.clone();
    }
    let built = Arc::new(build(vfs));
    cache().lock().unwrap().insert(key, built.clone());
    built
}

/// Drops the whole cache. Called after the toolkit writes a loc override so the
/// next lookup reflects the new value.
pub fn invalidate_all() {
    cache().lock().unwrap().clear();
}

/// Builds a [`LocStore`] directly from the Vfs, bypassing the cache. Prefer
/// [`store`] in commands; this is for one-off/test use.
pub fn build(vfs: &Vfs) -> LocStore {
    let mut map = HashMap::new();
    // Files come back in game load order (base, then mod, then mod replace/),
    // so later inserts correctly win.
    for path in vfs.localisation_files(ENGLISH_SUFFIX) {
        if let Ok(bytes) = std::fs::read(&path) {
            // Loc files are UTF-8; lossy tolerates any stray byte.
            parse_into(&String::from_utf8_lossy(&bytes), &mut map);
        }
    }
    LocStore { map }
}

// --- Writer --------------------------------------------------------------

/// Writes localized-name overrides into the project's toolkit-owned loc file
/// (`localisation/replace/zz_eutoolkit_l_english.yml`, UTF-8 **with BOM** — the
/// one exception to the Windows-1252 rule). Existing keys are updated in place;
/// every other line in the file is preserved. Returns the game-relative path.
/// Invalidates the loc cache so subsequent lookups see the change.
pub fn write_overrides(project_dir: &Path, entries: &[(String, String)]) -> Result<String, String> {
    let path = project_dir.join(OVERRIDE_REL);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create {}: {e}", parent.display()))?;
    }

    // Existing lines (BOM/header stripped for editing; re-added on write).
    let mut lines: Vec<String> = match std::fs::read(&path) {
        Ok(bytes) => String::from_utf8_lossy(&bytes)
            .trim_start_matches('\u{feff}')
            .lines()
            .map(|l| l.trim_end_matches('\r').to_string())
            .collect(),
        Err(_) => Vec::new(),
    };
    if lines.first().map(|l| l.trim()) != Some("l_english:") {
        lines.insert(0, "l_english:".to_string());
    }

    for (key, value) in entries {
        let new_line = format!(" {}:0 \"{}\"", key, value.replace('"', "'"));
        // Replace the existing line for this key, else append.
        match lines.iter().position(|l| line_key(l) == Some(key.as_str())) {
            Some(i) => lines[i] = new_line,
            None => lines.push(new_line),
        }
    }

    let mut out = String::from('\u{feff}');
    out.push_str(&lines.join("\n"));
    out.push('\n');
    std::fs::write(&path, out.as_bytes())
        .map_err(|e| format!("Failed to write {OVERRIDE_REL}: {e}"))?;
    invalidate_all();
    Ok(OVERRIDE_REL.to_string())
}

/// Removes localized-name override keys from the project's toolkit-owned loc file
/// (Sprint S2.1 country deletion). Only the project's own
/// `localisation/replace/zz_eutoolkit_l_english.yml` is touched — a base-game loc
/// file that defines a tag is never edited. Returns the game-relative path when
/// the file was rewritten, or `None` when there was nothing to do (file absent or
/// no matching key). Invalidates the loc cache on any change.
pub fn remove_overrides(project_dir: &Path, keys: &[String]) -> Result<Option<String>, String> {
    let path = project_dir.join(OVERRIDE_REL);
    let Ok(bytes) = std::fs::read(&path) else {
        return Ok(None); // no toolkit loc file: nothing to remove
    };
    let drop: std::collections::HashSet<&str> = keys.iter().map(String::as_str).collect();

    let mut lines: Vec<String> = String::from_utf8_lossy(&bytes)
        .trim_start_matches('\u{feff}')
        .lines()
        .map(|l| l.trim_end_matches('\r').to_string())
        .collect();

    let before = lines.len();
    lines.retain(|l| line_key(l).map_or(true, |k| !drop.contains(k)));
    if lines.len() == before {
        return Ok(None); // no matching key present
    }
    if lines.first().map(|l| l.trim()) != Some("l_english:") {
        lines.insert(0, "l_english:".to_string());
    }

    let mut out = String::from('\u{feff}');
    out.push_str(&lines.join("\n"));
    out.push('\n');
    std::fs::write(&path, out.as_bytes())
        .map_err(|e| format!("Failed to write {OVERRIDE_REL}: {e}"))?;
    invalidate_all();
    Ok(Some(OVERRIDE_REL.to_string()))
}

// --- Calendar loc (Sprint 12.4) ---------------------------------------------

/// The 12 month loc keys, in order. Plain loc keys the game (and mods, e.g.
/// Anbennar's "Castanmark(1)") localize.
pub const MONTH_KEYS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// Resolved calendar strings so the toolkit renders dates with the mod's own
/// calendar (custom month names + era/year template).
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarLoc {
    /// Resolved January..December.
    pub months: Vec<String>,
    /// The raw `WORLD_YEAR` template (e.g. `"The world $YEAR$ AD"`), if defined.
    pub world_year: Option<String>,
}

/// Tauri command: the 12 resolved month names + the raw `WORLD_YEAR` template,
/// so custom calendars (e.g. Imperium Universalis's "AUC" era) render everywhere.
#[tauri::command(async)]
pub fn get_calendar_loc(
    install_path: String,
    mod_path: Option<String>,
) -> Result<CalendarLoc, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    let store = store(&vfs, &install_path, mod_path.as_deref());
    Ok(CalendarLoc {
        months: MONTH_KEYS.iter().map(|k| store.resolve(k)).collect(),
        world_year: store.get("WORLD_YEAR").map(str::to_string),
    })
}

// --- Localisation browser (Sprint 28) ---------------------------------------

/// One matched localisation entry (browser search result).
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocHit {
    pub key: String,
    pub value: String,
    /// Game-relative source file.
    pub file: String,
    /// `base` | `mod`.
    pub origin: String,
    /// Loc language (`english`).
    pub language: String,
}

/// A capped, paginated slice of a loc search over the whole VFS.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocSearchResult {
    /// The requested page of matches.
    pub hits: Vec<LocHit>,
    /// Total number of matches across every file (for "N more…" paging).
    pub total: usize,
    pub offset: usize,
    pub limit: usize,
}

/// Extracts `(key, value)` from one loc line (the same grammar as [`parse_into`]),
/// or `None` for the header/comments/blank lines.
fn parse_loc_line(raw: &str) -> Option<(&str, &str)> {
    let line = raw.trim_start_matches('\u{feff}').trim_end_matches('\r');
    let trimmed = line.trim_start();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    let colon = line.find(':')?;
    let key = line[..colon].trim();
    if key.is_empty() {
        return None;
    }
    let rest = &line[colon + 1..];
    let q1 = rest.find('"')?;
    let after = &rest[q1 + 1..];
    let q2 = after.rfind('"')?;
    Some((key, &after[..q2]))
}

/// The language token of an english-suffixed loc file name (`x_l_english.yml`).
fn language_of(file_name: &str) -> String {
    file_name
        .rsplit_once("_l_")
        .and_then(|(_, tail)| tail.strip_suffix(".yml"))
        .unwrap_or("english")
        .to_string()
}

/// Game-relative path + origin for an absolute loc file path, via the Vfs layers.
fn rel_and_origin(vfs: &Vfs, abs: &Path) -> (String, &'static str) {
    if let Some(m) = vfs.mod_dir() {
        if let Ok(r) = abs.strip_prefix(m) {
            return (r.to_string_lossy().replace('\\', "/"), "mod");
        }
    }
    let rel = abs
        .strip_prefix(vfs.base_dir())
        .map(|r| r.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| abs.to_string_lossy().replace('\\', "/"));
    (rel, "base")
}

/// Searches every english loc file (base + mod, in load order) for `query`
/// (case-insensitive substring of the key OR the value; empty query = browse
/// all). Streams file-by-file, counting the full total but only materializing the
/// hits in the `[offset, offset+limit)` window so hundreds of files stay cheap.
pub fn search(vfs: &Vfs, query: &str, offset: usize, limit: usize) -> LocSearchResult {
    let needle = query.trim().to_lowercase();
    let mut total = 0usize;
    let mut hits = Vec::new();
    let end = offset.saturating_add(limit);
    for abs in vfs.localisation_files(ENGLISH_SUFFIX) {
        let Ok(bytes) = std::fs::read(&abs) else {
            continue;
        };
        let (rel, origin) = rel_and_origin(vfs, &abs);
        let language = abs
            .file_name()
            .map(|n| language_of(&n.to_string_lossy()))
            .unwrap_or_else(|| "english".into());
        let text = String::from_utf8_lossy(&bytes);
        for raw in text.lines() {
            let Some((key, value)) = parse_loc_line(raw) else {
                continue;
            };
            let matches = needle.is_empty()
                || key.to_lowercase().contains(&needle)
                || value.to_lowercase().contains(&needle);
            if !matches {
                continue;
            }
            if total >= offset && total < end {
                hits.push(LocHit {
                    key: key.to_string(),
                    value: value.to_string(),
                    file: rel.clone(),
                    origin: origin.to_string(),
                    language: language.clone(),
                });
            }
            total += 1;
        }
    }
    LocSearchResult { hits, total, offset, limit }
}

/// Tauri command: paginated loc search across the whole VFS.
#[tauri::command(async)]
pub fn search_loc(
    install_path: String,
    mod_path: Option<String>,
    query: String,
    offset: usize,
    limit: usize,
) -> Result<LocSearchResult, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    Ok(search(&vfs, &query, offset, limit))
}

/// One expected-but-unresolved loc key on project-created content.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MissingLoc {
    /// The unresolved loc key (`my_decision_title`).
    pub key: String,
    /// Content family: `decision` | `event` | `mission`.
    pub kind: String,
    /// The entity that expects it (decision/mission key or event id).
    pub entity: String,
    /// Game-relative file the entity is defined in (always mod-origin).
    pub file: String,
}

/// Scans **project-created** content (decisions / events / missions defined in the
/// MOD layer) for loc keys with no resolved value in the merged loc store. A
/// base-only session (no mod) has no project-created content → an empty report.
///
/// "Project-created" = the entity's source file is mod-origin. We reuse each
/// family's existing loader (which already resolves the loc keys per its pattern:
/// decisions/missions use `<key>_title`/`<key>_desc`; events use their `title`/
/// `desc` loc-KEY values), then flag every mod-origin entity whose title/desc did
/// not resolve.
pub fn missing_report(vfs: &Vfs, store: &LocStore) -> Vec<MissingLoc> {
    let mut out = Vec::new();

    // Decisions: <key>_title / <key>_desc.
    for d in crate::decisions::load_decisions(vfs, store) {
        if d.origin != "mod" {
            continue;
        }
        if d.title_loc.is_none() {
            out.push(MissingLoc { key: d.title_key.clone(), kind: "decision".into(), entity: d.key.clone(), file: d.file.clone() });
        }
        if d.desc_loc.is_none() {
            out.push(MissingLoc { key: d.desc_key.clone(), kind: "decision".into(), entity: d.key.clone(), file: d.file.clone() });
        }
    }

    // Events: the `title`/`desc` quoted VALUES are the loc keys. Only flag events
    // that actually declare a title/desc key (a hidden event legitimately omits them).
    for e in crate::events::load_events(vfs, store) {
        if e.origin != "mod" {
            continue;
        }
        if let Some(tk) = &e.title_key {
            if e.title_loc.is_none() {
                out.push(MissingLoc { key: tk.clone(), kind: "event".into(), entity: e.id.clone(), file: e.file.clone() });
            }
        }
        if let Some(dk) = &e.desc_key {
            if e.desc_loc.is_none() {
                out.push(MissingLoc { key: dk.clone(), kind: "event".into(), entity: e.id.clone(), file: e.file.clone() });
            }
        }
    }

    // Missions: <key>_title / <key>_desc.
    for s in crate::missions::load_series(vfs, store) {
        if s.origin != "mod" {
            continue;
        }
        for m in &s.missions {
            if m.title_loc.is_none() {
                out.push(MissingLoc { key: m.title_key.clone(), kind: "mission".into(), entity: m.key.clone(), file: s.file.clone() });
            }
            if m.desc_loc.is_none() {
                out.push(MissingLoc { key: m.desc_key.clone(), kind: "mission".into(), entity: m.key.clone(), file: s.file.clone() });
            }
        }
    }

    out
}

/// Tauri command: the missing-loc report for project-created content.
#[tauri::command(async)]
pub fn missing_loc_report(
    install_path: String,
    mod_path: Option<String>,
) -> Result<Vec<MissingLoc>, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    let store = store(&vfs, &install_path, mod_path.as_deref());
    Ok(missing_report(&vfs, &store))
}

/// The loc key a line defines, if any (its text before the first colon, when a
/// quoted value follows). `None` for the header, comments, and blank lines.
fn line_key(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    let colon = line.find(':')?;
    let key = line[..colon].trim();
    if key.is_empty() || !line[colon + 1..].contains('"') {
        return None;
    }
    Some(key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn parse(text: &str) -> HashMap<String, String> {
        let mut m = HashMap::new();
        parse_into(text, &mut m);
        m
    }

    #[test]
    fn bom_header_and_basic_keys() {
        let m = parse("\u{feff}l_english:\n SWE:0 \"Sweden\"\n FRA:0 \"France\"\n");
        assert_eq!(m.get("SWE").map(String::as_str), Some("Sweden"));
        assert_eq!(m.get("FRA").map(String::as_str), Some("France"));
        // The header must never become a key.
        assert!(!m.contains_key("l_english"));
    }

    #[test]
    fn missing_version_number() {
        let m = parse("l_english:\n SWE: \"Sweden\"\n ADJ:12 \"Swedish\"\n");
        assert_eq!(m.get("SWE").map(String::as_str), Some("Sweden"));
        assert_eq!(m.get("ADJ").map(String::as_str), Some("Swedish"));
    }

    #[test]
    fn comments_and_blank_lines() {
        let m = parse("l_english:\n# a comment\n\n   # indented comment\n K:0 \"V\"\n");
        assert_eq!(m.len(), 1);
        assert_eq!(m.get("K").map(String::as_str), Some("V"));
    }

    #[test]
    fn embedded_quotes_in_value() {
        let m = parse("l_english:\n Q:0 \"He said \"hi\" loudly\"\n");
        assert_eq!(m.get("Q").map(String::as_str), Some("He said \"hi\" loudly"));
    }

    #[test]
    fn crlf_and_lf() {
        let crlf = parse("l_english:\r\n A:0 \"one\"\r\n B:0 \"two\"\r\n");
        assert_eq!(crlf.get("A").map(String::as_str), Some("one"));
        assert_eq!(crlf.get("B").map(String::as_str), Some("two"));
    }

    #[test]
    fn value_containing_colon() {
        let m = parse("l_english:\n K:0 \"Ratio 2:1 odds\"\n");
        assert_eq!(m.get("K").map(String::as_str), Some("Ratio 2:1 odds"));
    }

    #[test]
    fn later_keys_win() {
        // Mimics load order: a later file overrides an earlier key.
        let mut m = HashMap::new();
        parse_into(" SWE:0 \"Sweden\"\n", &mut m);
        parse_into(" SWE:0 \"Sverige\"\n", &mut m);
        assert_eq!(m.get("SWE").map(String::as_str), Some("Sverige"));
    }

    #[test]
    fn prettify_matches_frontend() {
        // Mirrors the frontend `pretty()`: uppercase the first char of each
        // underscore-separated word, leave the rest untouched (acronyms stay).
        assert_eq!(prettify("primary_culture"), "Primary Culture");
        assert_eq!(prettify("western"), "Western");
        assert_eq!(prettify("TAG_ideas"), "TAG Ideas");
        assert_eq!(prettify(""), "");
    }

    #[test]
    fn resolve_fallback_chain() {
        let store = LocStore::from_pairs(&[("SWE", "Sweden")]);
        assert_eq!(store.resolve("SWE"), "Sweden"); // found
        assert_eq!(store.resolve("some_unknown_key"), "Some Unknown Key"); // prettified
        assert_eq!(store.resolve_or("SWE", "Fallback"), "Sweden");
        assert_eq!(store.resolve_or("MISSING", "Fallback"), "Fallback");
    }

    // --- Writer tests ---

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("eu_toolkit_loc_test_{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn writer_creates_file_with_bom() {
        let dir = temp("writer_bom");
        let rel = write_overrides(&dir, &[("SWE".into(), "Svealand".into())]).unwrap();
        assert_eq!(rel, OVERRIDE_REL);
        let bytes = std::fs::read(dir.join(OVERRIDE_REL)).unwrap();
        assert_eq!(&bytes[..3], &[0xEF, 0xBB, 0xBF], "file must start with UTF-8 BOM");
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.contains("l_english:"));
        assert!(text.contains(" SWE:0 \"Svealand\""));
    }

    #[test]
    fn writer_updates_key_in_place_preserving_others() {
        let dir = temp("writer_update");
        write_overrides(
            &dir,
            &[("SWE".into(), "Sweden".into()), ("FRA".into(), "France".into())],
        )
        .unwrap();
        // Update one key; the other must survive untouched.
        write_overrides(&dir, &[("SWE".into(), "Svealand".into())]).unwrap();
        let text = std::fs::read_to_string(dir.join(OVERRIDE_REL)).unwrap();
        assert!(text.contains(" SWE:0 \"Svealand\""));
        assert!(!text.contains("\"Sweden\""), "old value should be gone");
        assert!(text.contains(" FRA:0 \"France\""), "other key preserved");
        // Exactly one SWE line (updated in place, not appended).
        assert_eq!(text.matches(" SWE:").count(), 1);
    }

    #[test]
    fn remove_overrides_drops_only_named_keys() {
        let dir = temp("remove_keys");
        write_overrides(
            &dir,
            &[
                ("FRA".into(), "France".into()),
                ("FRA_ADJ".into(), "French".into()),
                ("SWE".into(), "Sweden".into()),
            ],
        )
        .unwrap();
        // Remove the FRA pair; SWE must survive.
        let rel = remove_overrides(&dir, &["FRA".into(), "FRA_ADJ".into()]).unwrap();
        assert_eq!(rel.as_deref(), Some(OVERRIDE_REL));
        let text = std::fs::read_to_string(dir.join(OVERRIDE_REL)).unwrap();
        assert!(!text.contains(" FRA:"), "FRA line removed: {text}");
        assert!(!text.contains(" FRA_ADJ:"), "FRA_ADJ line removed: {text}");
        assert!(text.contains(" SWE:0 \"Sweden\""), "SWE preserved");
        assert!(text.starts_with('\u{feff}'), "BOM preserved");
    }

    #[test]
    fn remove_overrides_is_noop_when_absent() {
        let dir = temp("remove_absent");
        // No toolkit loc file at all → None (no error, no file created).
        assert_eq!(remove_overrides(&dir, &["FRA".into()]).unwrap(), None);
        // File exists but the key isn't in it → None (unchanged).
        write_overrides(&dir, &[("SWE".into(), "Sweden".into())]).unwrap();
        assert_eq!(remove_overrides(&dir, &["FRA".into()]).unwrap(), None);
        let text = std::fs::read_to_string(dir.join(OVERRIDE_REL)).unwrap();
        assert!(text.contains(" SWE:0 \"Sweden\""));
    }

    #[test]
    fn writer_round_trip_parses_back() {
        let dir = temp("writer_roundtrip");
        write_overrides(
            &dir,
            &[("A".into(), "Alpha".into()), ("B".into(), "Beta".into())],
        )
        .unwrap();
        let bytes = std::fs::read(dir.join(OVERRIDE_REL)).unwrap();
        let mut m = HashMap::new();
        parse_into(&String::from_utf8_lossy(&bytes), &mut m);
        assert_eq!(m.get("A").map(String::as_str), Some("Alpha"));
        assert_eq!(m.get("B").map(String::as_str), Some("Beta"));
    }

    // --- Resolution / fallback with synthetic Vfs layering ---

    #[test]
    fn mod_overrides_base_via_vfs() {
        let root = temp("mod_override");
        let base = root.join("base");
        let mod_dir = root.join("mymod");
        std::fs::create_dir_all(base.join("map")).unwrap();
        std::fs::create_dir_all(base.join("localisation")).unwrap();
        std::fs::create_dir_all(mod_dir.join("localisation/replace")).unwrap();
        std::fs::write(base.join("map/provinces.bmp"), b"x").unwrap();
        std::fs::write(
            base.join("localisation/countries_l_english.yml"),
            "\u{feff}l_english:\n SWE:0 \"Sweden\"\n NOR:0 \"Norway\"\n",
        )
        .unwrap();
        // A non-replace mod file overrides NOR; a replace/ file overrides SWE.
        std::fs::write(
            mod_dir.join("localisation/extra_l_english.yml"),
            "\u{feff}l_english:\n NOR:0 \"Noreg\"\n",
        )
        .unwrap();
        std::fs::write(
            mod_dir.join("localisation/replace/zz_l_english.yml"),
            "\u{feff}l_english:\n SWE:0 \"Svealand\"\n",
        )
        .unwrap();

        let vfs = Vfs::new(base.to_str().unwrap(), Some(mod_dir.to_str().unwrap())).unwrap();
        let store = build(&vfs);
        assert_eq!(store.resolve("SWE"), "Svealand");
        assert_eq!(store.resolve("NOR"), "Noreg");
        assert_eq!(store.resolve("MISSING"), "MISSING"); // prettify leaves acronyms
    }

    // --- Localisation browser: search + missing report (Sprint 28) ----------

    /// A base+mod fixture: a UTF-8-BOM loc file coexisting with a Windows-1252
    /// decisions script file, a mod-origin decision/event/mission, and some loc.
    fn missing_fixture(name: &str) -> Vfs {
        let root = std::env::temp_dir().join(format!("eu_toolkit_loc_missing_{name}"));
        let _ = std::fs::remove_dir_all(&root);
        let base = root.join("base");
        let m = root.join("mod");
        std::fs::create_dir_all(base.join("map")).unwrap();
        std::fs::write(base.join("map/provinces.bmp"), b"x").unwrap();
        // Base loc (UTF-8 BOM).
        std::fs::create_dir_all(base.join("localisation")).unwrap();
        std::fs::write(
            base.join("localisation/base_l_english.yml"),
            "\u{feff}l_english:\n SWE:0 \"Sweden\"\n form_sweden_title:0 \"Form Sweden\"\n",
        )
        .unwrap();
        // Mod loc: provides only the mission title (decision + event stay missing).
        std::fs::create_dir_all(m.join("localisation")).unwrap();
        std::fs::write(
            m.join("localisation/mod_l_english.yml"),
            "\u{feff}l_english:\n my_mission_title:0 \"My Mission\"\n",
        )
        .unwrap();
        // A mod-origin decision (Windows-1252 script; a high byte in a comment).
        std::fs::create_dir_all(m.join("decisions")).unwrap();
        let mut dec = b"country_decisions = {\n\tmy_decision = {\n\t\tpotential = { tag = SWE } # caf".to_vec();
        dec.push(0xE9); // é in Windows-1252
        dec.extend_from_slice(b"\n\t\tallow = { adm = 1 }\n\t\teffect = { add_prestige = 1 }\n\t}\n}\n");
        std::fs::write(m.join("decisions/MyDec.txt"), &dec).unwrap();
        // A mod-origin event whose title loc is missing.
        std::fs::create_dir_all(m.join("events")).unwrap();
        std::fs::write(
            m.join("events/MyEvt.txt"),
            b"namespace = myns\ncountry_event = {\n\tid = myns.1\n\ttitle = \"myns.1.t\"\n\tdesc = \"myns.1.d\"\n\toption = { name = \"myns.1.a\" }\n}\n",
        )
        .unwrap();
        // A mod-origin mission whose title resolves but desc is missing.
        std::fs::create_dir_all(m.join("missions")).unwrap();
        std::fs::write(
            m.join("missions/MyMis.txt"),
            b"my_series = {\n\tslot = 1\n\tgeneric = no\n\tmy_mission = {\n\t\ticon = mission_icon\n\t\ttrigger = { }\n\t\teffect = { }\n\t}\n}\n",
        )
        .unwrap();
        std::fs::write(m.join("descriptor.mod"), "name=\"m\"\n").unwrap();
        Vfs::new(base.to_str().unwrap(), Some(m.to_str().unwrap())).unwrap()
    }

    #[test]
    fn search_matches_key_and_value_with_origin_and_paging() {
        let vfs = missing_fixture("search");
        // Value match: "Sweden" appears in the base file (origin=base).
        let r = search(&vfs, "sweden", 0, 50);
        assert!(r.hits.iter().any(|h| h.key == "SWE" && h.origin == "base" && h.language == "english"));
        // Key match: "form_sweden_title".
        let r = search(&vfs, "form_sweden", 0, 50);
        assert!(r.hits.iter().any(|h| h.key == "form_sweden_title"));
        // Mod-origin hit carries origin=mod + its file.
        let r = search(&vfs, "my_mission", 0, 50);
        let hit = r.hits.iter().find(|h| h.key == "my_mission_title").unwrap();
        assert_eq!(hit.origin, "mod");
        assert!(hit.file.ends_with("mod_l_english.yml"));
        // Paging: total counts all, hits are the requested window.
        let all = search(&vfs, "", 0, 1);
        assert!(all.total >= 3);
        assert_eq!(all.hits.len(), 1);
        let page2 = search(&vfs, "", 1, 1);
        assert_ne!(all.hits[0].key, page2.hits[0].key);
    }

    #[test]
    fn missing_report_flags_unresolved_project_loc() {
        let vfs = missing_fixture("report");
        let store = build(&vfs);
        let report = missing_report(&vfs, &store);
        // Decision: both title + desc unresolved.
        assert!(report.iter().any(|m| m.key == "my_decision_title" && m.kind == "decision" && m.entity == "my_decision"));
        assert!(report.iter().any(|m| m.key == "my_decision_desc" && m.kind == "decision"));
        // Event: title + desc loc keys unresolved.
        assert!(report.iter().any(|m| m.key == "myns.1.t" && m.kind == "event" && m.entity == "myns.1"));
        assert!(report.iter().any(|m| m.key == "myns.1.d" && m.kind == "event"));
        // Mission: title resolves (present in mod loc), desc is missing.
        assert!(report.iter().any(|m| m.key == "my_mission_desc" && m.kind == "mission" && m.entity == "my_mission"));
        assert!(!report.iter().any(|m| m.key == "my_mission_title"), "resolved mission title not flagged");
    }

    #[test]
    fn missing_report_empty_without_mod_layer() {
        // Base-only: nothing is project-created, so the report is empty.
        let root = std::env::temp_dir().join("eu_toolkit_loc_missing_baseonly");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("map")).unwrap();
        std::fs::write(root.join("map/provinces.bmp"), b"x").unwrap();
        std::fs::create_dir_all(root.join("decisions")).unwrap();
        std::fs::write(
            root.join("decisions/D.txt"),
            b"country_decisions = {\n\tbase_dec = {\n\t\tpotential = { }\n\t\tallow = { }\n\t\teffect = { }\n\t}\n}\n",
        )
        .unwrap();
        let vfs = Vfs::new(root.to_str().unwrap(), None).unwrap();
        let store = build(&vfs);
        assert!(missing_report(&vfs, &store).is_empty(), "base-only content is not project-created");
    }

    // --- Real-install smoke tests (no-op when the game/Anbennar is absent) ---

    const INSTALL: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Europa Universalis IV";
    const ANBENNAR: &str = r"C:\Users\User\projects\anbennar-eu4-dev";
    /// Imperium Universalis 3.1.2 workshop item (classical-era total conversion:
    /// START_DATE 142.7.10, "The world $YEAR$ AUC" era). Absent on most machines.
    const IMPERIUM: &str =
        r"C:\Program Files (x86)\Steam\steamapps\workshop\content\236850\679204773";

    #[test]
    fn real_install_resolves_known_names() {
        if !Path::new(INSTALL).join("map/provinces.bmp").is_file() {
            return; // game not present: no-op
        }
        let vfs = Vfs::new(INSTALL, None).unwrap();
        let store = build(&vfs);
        assert_eq!(store.resolve("SWE"), "Sweden");
        assert_eq!(store.resolve("SWE_ADJ"), "Swedish");
        assert_eq!(store.resolve("PROV1"), "Stockholm");
        assert_eq!(store.resolve("catholic"), "Catholic");
    }

    #[test]
    fn anbennar_custom_tag_resolves() {
        if !Path::new(INSTALL).join("map/provinces.bmp").is_file() {
            return;
        }
        if !Path::new(ANBENNAR).is_dir() {
            return; // Anbennar checkout absent: no-op
        }
        let vfs = Vfs::new(INSTALL, Some(ANBENNAR)).unwrap();
        let store = build(&vfs);
        // A38 is an Anbennar custom country tag (Anbenncóst).
        assert_eq!(store.resolve("A38"), "Anbenncóst");
    }

    // --- Calendar loc (Sprint 12.4/12.5) ------------------------------------

    #[test]
    fn month_name_loc_override_round_trips() {
        // A custom-calendar month rename (Anbennar-style "Castanmark(1)") writes
        // through the standard override writer under the "January" key and parses
        // back as the January month name the calendar editor reads.
        let dir = temp("calendar_month");
        write_overrides(&dir, &[(MONTH_KEYS[0].into(), "Castanmark(1)".into())]).unwrap();
        let bytes = std::fs::read(dir.join(OVERRIDE_REL)).unwrap();
        let mut m = HashMap::new();
        parse_into(&String::from_utf8_lossy(&bytes), &mut m);
        assert_eq!(m.get("January").map(String::as_str), Some("Castanmark(1)"));
        // And resolves through a LocStore exactly as the calendar editor renders it.
        let store = LocStore { map: m };
        let months: Vec<String> = MONTH_KEYS.iter().map(|k| store.resolve(k)).collect();
        assert_eq!(months[0], "Castanmark(1)");
        assert_eq!(months[11], "December"); // untouched month falls back to the key
    }

    #[test]
    fn anbennar_castanmark_months_through_calendar_loc() {
        if !Path::new(INSTALL).join("map/provinces.bmp").is_file() || !Path::new(ANBENNAR).is_dir() {
            return; // game or Anbennar checkout absent: no-op
        }
        let vfs = Vfs::new(INSTALL, Some(ANBENNAR)).unwrap();
        let store = build(&vfs);
        let months: Vec<String> = MONTH_KEYS.iter().map(|k| store.resolve(k)).collect();
        // Anbennar reskins the calendar months (localisation/core_l_english.yml).
        assert_eq!(months[0], "Castanmark(1)");
        assert_eq!(months[1], "Esmarment(2)");
        assert_eq!(months[2], "Bloomsdawn(3)");
    }

    #[test]
    fn imperium_universalis_auc_era_readable() {
        if !Path::new(INSTALL).join("map/provinces.bmp").is_file()
            || !Path::new(IMPERIUM).join("descriptor.mod").is_file()
        {
            return; // game or IU workshop item absent: no-op
        }
        let vfs = Vfs::new(INSTALL, Some(IMPERIUM)).unwrap();
        let store = build(&vfs);
        // IU brands the year label with an ab-urbe-condita era. The effective value
        // (last loc file wins) is "The World $YEAR$ Ab Urbe Condita"; it must carry
        // the $YEAR$ token and a non-empty era suffix the toolkit can surface.
        let wy = store.get("WORLD_YEAR").expect("IU defines WORLD_YEAR");
        assert!(wy.contains("$YEAR$"), "era template must reference the year: {wy}");
        let era = wy.split("$YEAR$").nth(1).map(str::trim).unwrap_or("");
        assert!(!era.is_empty(), "expected an era suffix after $YEAR$: {wy}");
        assert!(era.contains("Ab Urbe Condita") || era.contains("AUC"), "AUC era: {wy}");
        // It does NOT rename months, so they resolve to the vanilla names.
        let months: Vec<String> = MONTH_KEYS.iter().map(|k| store.resolve(k)).collect();
        assert_eq!(months[0], "January");
        assert_eq!(months[11], "December");
    }
}
