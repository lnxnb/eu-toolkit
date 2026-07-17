//! Sprint 12.1/12.4 — `common/defines.lua` playable-date bounds
//! (`NDefines.NGame.START_DATE` / `END_DATE`).
//!
//! defines.lua is **Lua**, not Clausewitz script, so `paradox.rs` can't parse
//! it. We only need two string fields, so a line-oriented extractor is enough:
//! find the assignment whose left-hand side is (or ends with) the key and pull
//! the quoted `Y.M.D` value.
//!
//! ## Load order (why we read both layers)
//! The game applies defines files additively as Lua: base `common/defines.lua`
//! first, then `common/defines/*.lua`, then the same from each mod — later
//! assignments override earlier ones per key. A mod can therefore ship a file
//! that overrides only `START_DATE` while leaving every other define to the
//! base. So we read **both** the base and the mod layer (via
//! [`Vfs::base_dir`]/[`Vfs::mod_dir`], not `Vfs::read` which would shadow the
//! base) and take the last value seen for each key.
//!
//! ## Writing an override (copy-on-write, never touches base)
//! * If the project already owns `common/defines.lua` (a total conversion like
//!   Anbennar ships a full copy), we edit that file's date line **byte-surgical**.
//! * Otherwise we create/extend `common/defines/zz_eutoolkit_defines.lua` with an
//!   **additive** dotted assignment (`NDefines.NGame.START_DATE = "…"`). A file in
//!   the `common/defines/` folder loads after — and overrides — the base
//!   `defines.lua`, so a minimal override never wipes the thousands of other
//!   defines (which a bare `common/defines.lua` containing only two keys could).

use std::path::Path;

use crate::vfs::Vfs;

/// Toolkit-owned additive defines file, used when the project has no
/// `common/defines.lua` of its own.
pub const OVERRIDE_REL: &str = "common/defines/zz_eutoolkit_defines.lua";

/// The whole-file `common/defines.lua` (a total conversion's own copy).
pub const MAIN_REL: &str = "common/defines.lua";

/// Resolved playable-date bounds, plus the vanilla fallbacks used when a key is
/// undefined everywhere (which never happens in practice, but keeps the reader
/// total).
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DefinesDates {
    /// `NDefines.NGame.START_DATE`, e.g. `"1444.11.11"`.
    pub start_date: String,
    /// `NDefines.NGame.END_DATE`, e.g. `"1821.1.2"`.
    pub end_date: String,
}

/// Extracts the quoted value of a `KEY = "…"` Lua assignment from one line, where
/// the left-hand side is either the bare key (nested table form, indented) or a
/// dotted path ending in the key (additive form, `NDefines.NGame.KEY`). Lua `--`
/// comment lines are ignored.
fn extract_key(line: &str, key: &str) -> Option<String> {
    let trimmed = line.trim_start();
    if trimmed.starts_with("--") {
        return None;
    }
    let (lhs, rhs) = line.split_once('=')?;
    let lhs = lhs.trim();
    let matches = lhs == key || lhs.ends_with(&format!(".{key}"));
    if !matches {
        return None;
    }
    let q1 = rhs.find('"')?;
    let after = &rhs[q1 + 1..];
    let q2 = after.find('"')?;
    Some(after[..q2].to_string())
}

/// Scans one file's text for the last assignment of `key`, if any.
fn scan_text(text: &str, key: &str) -> Option<String> {
    let mut found = None;
    for line in text.lines() {
        if let Some(v) = extract_key(line, key) {
            found = Some(v);
        }
    }
    found
}

/// Every defines file in one layer directory, in game load order:
/// `common/defines.lua` first, then `common/defines/*.lua` sorted.
fn layer_defines_files(layer: &Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let main = layer.join("common/defines.lua");
    if main.is_file() {
        out.push(main);
    }
    let sub = layer.join("common/defines");
    if let Ok(read) = std::fs::read_dir(&sub) {
        let mut subs: Vec<_> = read
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.is_file()
                    && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("lua"))
            })
            .collect();
        subs.sort();
        out.extend(subs);
    }
    out
}

/// The value of `key` across both layers (base then mod), last-wins.
fn resolve_key(vfs: &Vfs, key: &str, fallback: &str) -> String {
    let mut value = fallback.to_string();
    let mut layers = vec![vfs.base_dir().to_path_buf()];
    if let Some(m) = vfs.mod_dir() {
        layers.push(m.to_path_buf());
    }
    for layer in layers {
        for path in layer_defines_files(&layer) {
            if let Ok(bytes) = std::fs::read(&path) {
                if let Some(v) = scan_text(&String::from_utf8_lossy(&bytes), key) {
                    value = v;
                }
            }
        }
    }
    value
}

/// Reads the effective playable-date bounds for this session.
pub fn defines_dates(vfs: &Vfs) -> DefinesDates {
    DefinesDates {
        start_date: resolve_key(vfs, "START_DATE", "1444.11.11"),
        end_date: resolve_key(vfs, "END_DATE", "1821.1.2"),
    }
}

/// Tauri command: the effective `START_DATE`/`END_DATE` for the calendar editor
/// and out-of-range bookmark checks.
#[tauri::command]
pub fn get_defines_dates(
    install_path: String,
    mod_path: Option<String>,
) -> Result<DefinesDates, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    Ok(defines_dates(&vfs))
}

// --- Full extraction (Sprint 28 Defines editor) -----------------------------

/// One extracted `NDefines.<NS>.<KEY>` scalar define, with its effective value,
/// detected type, and — for the diff view — the base-game value it overrides.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DefineEntry {
    /// The namespace (`NGame`, `NCountry`, …).
    pub namespace: String,
    /// The define key (`START_DATE`, `MAX_COLONIAL_NATIONS`).
    pub key: String,
    /// The full dotted path (`NDefines.NGame.START_DATE`).
    pub dotted: String,
    /// Effective value (quotes stripped for strings), base+mod last-wins.
    pub value: String,
    /// `number` | `string` | `bool`.
    pub value_type: String,
    /// The base-game value (quotes stripped), or `None` when the key is only
    /// defined by the mod (a project-added define).
    pub base_value: Option<String>,
    /// Whether the project (mod layer) overrides the base value.
    pub overridden: bool,
}

/// A single `(namespace, key) -> raw_value_token` map for one layer, plus a first
/// -seen ordering of namespaces for stable display.
type DefineMap = std::collections::HashMap<(String, String), String>;

/// Cleans a right-hand-side value token: drops a trailing Lua `--` comment and a
/// trailing comma, then trims. Returns the raw token (quotes retained).
fn clean_rhs(rhs: &str) -> String {
    let mut s = rhs;
    if let Some(c) = s.find("--") {
        s = &s[..c];
    }
    let s = s.trim();
    s.strip_suffix(',').unwrap_or(s).trim().to_string()
}

/// Types + de-quotes a cleaned value token → (`value_type`, display value).
fn type_value(raw: &str) -> (&'static str, String) {
    let t = raw.trim();
    if let Some(inner) = t.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
        return ("string", inner.to_string());
    }
    if t.eq_ignore_ascii_case("true") || t.eq_ignore_ascii_case("false") {
        return ("bool", t.to_ascii_lowercase());
    }
    if t.parse::<f64>().is_ok() {
        return ("number", t.to_string());
    }
    ("string", t.to_string())
}

/// Scans one defines file's text into `map` (last assignment wins). Handles both
/// the nested-table form (a `N<NS> = {` header at column 0, then indented
/// `KEY = value` scalar lines) and the dotted additive form
/// (`NDefines.<NS>.<KEY> = value`). Table-valued keys (`KEY = { … }`) are skipped
/// — only scalar defines are extracted for typed editing.
fn scan_all(text: &str, map: &mut DefineMap, ns_order: &mut Vec<String>) {
    let mut current_ns: Option<String> = None;
    for raw_line in text.lines() {
        let line = raw_line.trim_end_matches('\r');
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("--") {
            continue;
        }
        let leading_ws = line.len() - line.trim_start().len() > 0;

        // Namespace header: a column-0 `N<Name> = {` with no scalar value.
        if !leading_ws {
            if let Some((lhs, rhs)) = trimmed.split_once('=') {
                let lhs = lhs.trim();
                let rhs = rhs.trim();
                if rhs == "{" && lhs.starts_with('N') && lhs.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                    if lhs == "NDefines" {
                        // Root wrapper — keep the current namespace context.
                    } else {
                        current_ns = Some(lhs.to_string());
                    }
                    continue;
                }
            }
        }

        // Dotted additive form: `NDefines.<NS>.<KEY> = value`.
        if trimmed.starts_with("NDefines.") {
            if let Some((lhs, rhs)) = trimmed.split_once('=') {
                let parts: Vec<&str> = lhs.trim().split('.').collect();
                if parts.len() >= 3 && parts[0] == "NDefines" {
                    let ns = parts[1].to_string();
                    let key = parts[parts.len() - 1].to_string();
                    let val = clean_rhs(rhs);
                    if !val.starts_with('{') && !val.is_empty() {
                        remember_ns(ns_order, &ns);
                        map.insert((ns, key), val);
                    }
                }
            }
            continue;
        }

        // Indented scalar `KEY = value` under the current namespace.
        if let (Some(ns), true) = (&current_ns, leading_ws) {
            if let Some((lhs, rhs)) = line.split_once('=') {
                let key = lhs.trim();
                if !key.is_empty() && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                    let val = clean_rhs(rhs);
                    if !val.starts_with('{') && !val.is_empty() {
                        remember_ns(ns_order, ns);
                        map.insert((ns.clone(), key.to_string()), val);
                    }
                }
            }
        }
    }
}

fn remember_ns(order: &mut Vec<String>, ns: &str) {
    if !order.iter().any(|n| n == ns) {
        order.push(ns.to_string());
    }
}

/// Reads every defines file in one layer directory into a fresh map.
fn layer_map(layer: &Path, ns_order: &mut Vec<String>) -> DefineMap {
    let mut map = DefineMap::new();
    for path in layer_defines_files(layer) {
        if let Ok(bytes) = std::fs::read(&path) {
            scan_all(&String::from_utf8_lossy(&bytes), &mut map, ns_order);
        }
    }
    map
}

/// Extracts every scalar `NDefines` define across base + mod layers (mod
/// last-wins), with base-vs-effective diff classification. Sorted by namespace
/// (first-seen order) then key.
pub fn extract_all(vfs: &Vfs) -> Vec<DefineEntry> {
    let mut ns_order: Vec<String> = Vec::new();
    let base = layer_map(vfs.base_dir(), &mut ns_order);
    let modl = match vfs.mod_dir() {
        Some(m) => layer_map(m, &mut ns_order),
        None => DefineMap::new(),
    };

    // Union of keys.
    let mut keys: std::collections::BTreeSet<(String, String)> = std::collections::BTreeSet::new();
    keys.extend(base.keys().cloned());
    keys.extend(modl.keys().cloned());

    let ns_rank = |ns: &str| ns_order.iter().position(|n| n == ns).unwrap_or(usize::MAX);

    let mut out: Vec<DefineEntry> = keys
        .into_iter()
        .map(|(ns, key)| {
            let base_raw = base.get(&(ns.clone(), key.clone()));
            let mod_raw = modl.get(&(ns.clone(), key.clone()));
            let effective_raw = mod_raw.or(base_raw).cloned().unwrap_or_default();
            let (vt, value) = type_value(&effective_raw);
            let base_value = base_raw.map(|r| type_value(r).1);
            let overridden = mod_raw.is_some();
            DefineEntry {
                dotted: format!("NDefines.{ns}.{key}"),
                namespace: ns,
                key,
                value,
                value_type: vt.to_string(),
                base_value,
                overridden,
            }
        })
        .collect();

    out.sort_by(|a, b| {
        ns_rank(&a.namespace)
            .cmp(&ns_rank(&b.namespace))
            .then_with(|| a.namespace.cmp(&b.namespace))
            .then_with(|| a.key.cmp(&b.key))
    });
    out
}

/// Tauri command: the full searchable NDefines tree with typed values + diff
/// (project overrides vs base).
#[tauri::command]
pub fn get_defines(
    install_path: String,
    mod_path: Option<String>,
) -> Result<Vec<DefineEntry>, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    Ok(extract_all(&vfs))
}

// --- Writer -----------------------------------------------------------------

/// Formats a define value for Lua: booleans + numbers bare, everything else
/// (dates, string values) double-quoted. This preserves the type — writing a
/// numeric define like `MAX_COLONIAL_NATIONS = 75` unquoted, a date quoted.
fn format_value(value: &str) -> String {
    let t = value.trim();
    if t.eq_ignore_ascii_case("true") || t.eq_ignore_ascii_case("false") {
        return t.to_ascii_lowercase();
    }
    if t.parse::<f64>().is_ok() {
        return t.to_string();
    }
    format!("\"{t}\"")
}

/// Whether a Lua assignment line assigns `key` (nested `KEY = …` or dotted
/// `….KEY = …`), independent of the value's quoting (unlike [`extract_key`],
/// which only matches quoted values).
fn line_assigns_key(logical: &str, key: &str) -> bool {
    let t = logical.trim_start();
    if t.starts_with("--") {
        return false;
    }
    let Some((lhs, _)) = logical.split_once('=') else {
        return false;
    };
    let lhs = lhs.trim();
    lhs == key || lhs.ends_with(&format!(".{key}"))
}

/// The dotted additive assignment for a namespaced key, type-aware.
fn dotted(namespace: &str, key: &str, value: &str) -> String {
    format!("NDefines.{namespace}.{key} = {}", format_value(value))
}

/// Sets `key`'s value (in namespace `namespace`) in Lua `bytes` byte-surgically:
/// if a matching assignment line exists (nested `KEY = …` or dotted `…KEY = …`),
/// only its value token is replaced (trailing comma + `--` comment preserved),
/// re-typed via [`format_value`]; otherwise an additive dotted line is appended.
/// Every other byte round-trips. `bytes` may be empty (fresh file).
pub fn set_define(bytes: &[u8], namespace: &str, key: &str, value: &str) -> Vec<u8> {
    let text = String::from_utf8_lossy(bytes);
    let mut offset = 0usize;
    for line in text.split_inclusive('\n') {
        let line_no_nl = line.strip_suffix('\n').unwrap_or(line);
        let logical = line_no_nl.strip_suffix('\r').unwrap_or(line_no_nl);
        if line_assigns_key(logical, key) {
            if let Some(eq) = logical.find('=') {
                // Value token = after '=', trimmed, up to a `--` comment or a
                // trailing comma, whichever comes first.
                let after = &logical[eq + 1..];
                let ws = after.len() - after.trim_start().len();
                let content = after.trim_start();
                let mut vlen = content.len();
                if let Some(c) = content.find("--") {
                    vlen = vlen.min(c);
                }
                let core = content[..vlen].trim_end();
                let mut core_len = core.len();
                if core.ends_with(',') {
                    core_len -= 1;
                }
                let val_start = offset + eq + 1 + ws;
                let val_end = val_start + core_len;
                let formatted = format_value(value);
                let mut out = Vec::with_capacity(bytes.len() + formatted.len());
                out.extend_from_slice(&bytes[..val_start]);
                out.extend_from_slice(formatted.as_bytes());
                out.extend_from_slice(&bytes[val_end..]);
                return out;
            }
        }
        offset += line.len();
    }
    // Not found: append a dotted additive assignment.
    let mut out = bytes.to_vec();
    if !out.is_empty() && *out.last().unwrap() != b'\n' {
        out.push(b'\n');
    }
    out.extend_from_slice(dotted(namespace, key, value).as_bytes());
    out.push(b'\n');
    out
}

/// Applies define overrides copy-on-write into `project_dir`. Each entry is
/// `(namespace, key, value)`. Returns the game-relative path written. If the
/// project owns `common/defines.lua`, that file is extended in place; otherwise
/// the additive `common/defines/zz_eutoolkit_defines.lua` is created/extended.
pub fn write_overrides(
    project_dir: &Path,
    entries: &[(String, String, String)],
) -> Result<String, String> {
    if entries.is_empty() {
        return Ok(String::new());
    }
    let owns_main = project_dir.join(MAIN_REL).is_file();
    let rel = if owns_main { MAIN_REL } else { OVERRIDE_REL };
    let path = project_dir.join(rel);

    let mut bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(_) => {
            // Fresh additive file gets a header comment.
            b"-- EU Toolkit defines override (additive; loads over the base defines).\n"
                .to_vec()
        }
    };
    for (namespace, key, value) in entries {
        bytes = set_define(&bytes, namespace, key, value);
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create {}: {e}", parent.display()))?;
    }
    std::fs::write(&path, bytes).map_err(|e| format!("Failed to write {rel}: {e}"))?;
    Ok(rel.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    const INSTALL: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Europa Universalis IV";
    const ANBENNAR: &str = r"C:\Users\User\projects\anbennar-eu4-dev";
    /// Imperium Universalis 3.1.2 workshop item (classical-era total conversion).
    const IMPERIUM: &str =
        r"C:\Program Files (x86)\Steam\steamapps\workshop\content\236850\679204773";

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("eu_toolkit_defines_test_{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    const NESTED: &str = "NDefines = {\n\nNGame = {\n\tSTART_DATE = \"1444.11.11\",\n\tEND_DATE = \"1821.1.2\",\n\tMAX_RANDOM_NEW_WORLD = \"1492.1.1\",\n}\n}\n";

    #[test]
    fn extract_nested_and_dotted() {
        assert_eq!(scan_text(NESTED, "START_DATE").as_deref(), Some("1444.11.11"));
        assert_eq!(scan_text(NESTED, "END_DATE").as_deref(), Some("1821.1.2"));
        // Dotted additive form.
        let dotted_txt = "NDefines.NGame.START_DATE = \"142.7.10\"\n";
        assert_eq!(scan_text(dotted_txt, "START_DATE").as_deref(), Some("142.7.10"));
        // Comment line ignored.
        assert_eq!(scan_text("-- START_DATE = \"9.9.9\"\n", "START_DATE"), None);
        // Later assignment wins.
        let two = "START_DATE = \"1.1.1\"\nNDefines.NGame.START_DATE = \"2.2.2\"\n";
        assert_eq!(scan_text(two, "START_DATE").as_deref(), Some("2.2.2"));
    }

    #[test]
    fn set_define_byte_surgical_replace() {
        let out = set_define(NESTED.as_bytes(), "NGame", "START_DATE", "1300.1.1");
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("START_DATE = \"1300.1.1\","));
        // Only the value changed — the sibling keys + trailing comma survive.
        assert!(text.contains("END_DATE = \"1821.1.2\","));
        assert!(text.contains("MAX_RANDOM_NEW_WORLD = \"1492.1.1\","));
    }

    #[test]
    fn set_define_numeric_stays_unquoted() {
        // A numeric define round-trips unquoted (a quoted number would break Lua).
        let src = b"NDefines = {\nNGame = {\n\tMAX_COLONIAL_NATIONS = 75,\t-- Max is 100\n}\n}\n";
        let out = set_define(src, "NGame", "MAX_COLONIAL_NATIONS", "90");
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("MAX_COLONIAL_NATIONS = 90,"), "value replaced unquoted: {text}");
        // The trailing comment survives.
        assert!(text.contains("-- Max is 100"));
    }

    #[test]
    fn set_define_appends_when_absent() {
        let out = set_define(b"-- header\n", "NGame", "END_DATE", "1900.1.1");
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("-- header\n"));
        assert!(text.contains("NDefines.NGame.END_DATE = \"1900.1.1\"\n"));
        // Numeric append is unquoted, namespaced.
        let out = set_define(b"", "NCountry", "MAX_ARMY", "40");
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("NDefines.NCountry.MAX_ARMY = 40\n"));
    }

    #[test]
    fn writer_creates_additive_file_when_no_main() {
        let dir = temp("additive");
        let rel = write_overrides(
            &dir,
            &[
                ("NGame".into(), "START_DATE".into(), "1300.1.1".into()),
                ("NGame".into(), "END_DATE".into(), "1850.1.1".into()),
            ],
        )
        .unwrap();
        assert_eq!(rel, OVERRIDE_REL);
        let text = std::fs::read_to_string(dir.join(OVERRIDE_REL)).unwrap();
        assert!(text.contains("NDefines.NGame.START_DATE = \"1300.1.1\""));
        assert!(text.contains("NDefines.NGame.END_DATE = \"1850.1.1\""));
        // A second override round-trips in place (extends, not duplicates).
        write_overrides(&dir, &[("NGame".into(), "START_DATE".into(), "1200.1.1".into())]).unwrap();
        let text = std::fs::read_to_string(dir.join(OVERRIDE_REL)).unwrap();
        assert!(text.contains("\"1200.1.1\""));
        assert_eq!(text.matches("START_DATE").count(), 1, "extended in place: {text}");
    }

    #[test]
    fn writer_extends_existing_main_file() {
        let dir = temp("main");
        std::fs::create_dir_all(dir.join("common")).unwrap();
        std::fs::write(dir.join(MAIN_REL), NESTED).unwrap();
        let rel = write_overrides(&dir, &[("NGame".into(), "START_DATE".into(), "1300.1.1".into())]).unwrap();
        assert_eq!(rel, MAIN_REL);
        let text = std::fs::read_to_string(dir.join(MAIN_REL)).unwrap();
        assert!(text.contains("START_DATE = \"1300.1.1\","));
        assert!(text.contains("END_DATE = \"1821.1.2\","));
    }

    // --- Full extraction (Sprint 28) ----------------------------------------

    const MULTI_NS: &str = "NDefines = {\n\nNGame = {\n\tSTART_DATE = \"1444.11.11\",\n\tMAX_COLONIAL_NATIONS = 75,\t-- Max is 100\n\tSOME_FLAG = true,\n}\n\nNCountry = {\n\tBASE_VALUES = { 1 2 3 },\n\tMAX_CROWNLAND = 0.9,\n}\n}\n";

    #[test]
    fn extract_all_types_and_skips_tables() {
        let mut map = DefineMap::new();
        let mut order = Vec::new();
        scan_all(MULTI_NS, &mut map, &mut order);
        assert_eq!(map.get(&("NGame".into(), "START_DATE".into())).unwrap(), "\"1444.11.11\"");
        assert_eq!(map.get(&("NGame".into(), "MAX_COLONIAL_NATIONS".into())).unwrap(), "75");
        assert_eq!(map.get(&("NCountry".into(), "MAX_CROWNLAND".into())).unwrap(), "0.9");
        // Table-valued key skipped.
        assert!(!map.contains_key(&("NCountry".into(), "BASE_VALUES".into())));
        // Namespace order preserved.
        assert_eq!(order, vec!["NGame", "NCountry"]);
        // Types.
        assert_eq!(type_value("\"1444.11.11\""), ("string", "1444.11.11".to_string()));
        assert_eq!(type_value("75"), ("number", "75".to_string()));
        assert_eq!(type_value("true"), ("bool", "true".to_string()));
    }

    #[test]
    fn extract_all_diff_base_vs_mod_override() {
        // Base defines both; a mod additively overrides one + adds a new key.
        let dir = temp("extract_diff");
        let base = dir.join("base");
        let m = dir.join("mod");
        std::fs::create_dir_all(base.join("map")).unwrap();
        std::fs::write(base.join("map/provinces.bmp"), b"x").unwrap();
        std::fs::create_dir_all(base.join("common")).unwrap();
        std::fs::write(base.join(MAIN_REL), MULTI_NS).unwrap();
        std::fs::create_dir_all(m.join("common/defines")).unwrap();
        std::fs::write(
            m.join("common/defines/zz.lua"),
            "NDefines.NGame.MAX_COLONIAL_NATIONS = 90\nNDefines.NCustom.NEW_KEY = 5\n",
        )
        .unwrap();
        std::fs::write(m.join("descriptor.mod"), "name=\"m\"\n").unwrap();
        let vfs = Vfs::new(base.to_str().unwrap(), Some(m.to_str().unwrap())).unwrap();

        let all = extract_all(&vfs);
        // Unoverridden base define.
        let start = all.iter().find(|d| d.key == "START_DATE").unwrap();
        assert_eq!(start.value, "1444.11.11");
        assert_eq!(start.value_type, "string");
        assert!(!start.overridden);
        assert_eq!(start.base_value.as_deref(), Some("1444.11.11"));
        // Overridden numeric define: effective = mod value, base recorded, flagged.
        let mcn = all.iter().find(|d| d.key == "MAX_COLONIAL_NATIONS").unwrap();
        assert_eq!(mcn.value, "90");
        assert_eq!(mcn.value_type, "number");
        assert!(mcn.overridden);
        assert_eq!(mcn.base_value.as_deref(), Some("75"));
        assert_eq!(mcn.dotted, "NDefines.NGame.MAX_COLONIAL_NATIONS");
        // Mod-added define (no base): overridden, base None.
        let nk = all.iter().find(|d| d.key == "NEW_KEY").unwrap();
        assert_eq!(nk.namespace, "NCustom");
        assert!(nk.overridden);
        assert!(nk.base_value.is_none());
    }

    #[test]
    fn define_override_round_trips_through_extract() {
        // Write an override, then extract sees it as overridden with the new value.
        let dir = temp("override_roundtrip");
        let base = dir.join("base");
        let project = dir.join("project");
        std::fs::create_dir_all(base.join("map")).unwrap();
        std::fs::write(base.join("map/provinces.bmp"), b"x").unwrap();
        std::fs::create_dir_all(base.join("common")).unwrap();
        std::fs::write(base.join(MAIN_REL), MULTI_NS).unwrap();
        // Project is a mod layer over base; write an override into it.
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(project.join("descriptor.mod"), "name=\"p\"\n").unwrap();
        write_overrides(&project, &[("NGame".into(), "MAX_COLONIAL_NATIONS".into(), "120".into())]).unwrap();

        let vfs = Vfs::new(base.to_str().unwrap(), Some(project.to_str().unwrap())).unwrap();
        let all = extract_all(&vfs);
        let mcn = all.iter().find(|d| d.key == "MAX_COLONIAL_NATIONS").unwrap();
        assert_eq!(mcn.value, "120");
        assert!(mcn.overridden);
        assert_eq!(mcn.base_value.as_deref(), Some("75"));
    }

    #[test]
    fn extract_all_real_vanilla_known_defines() {
        if !Path::new(INSTALL).join("map/provinces.bmp").is_file() {
            return;
        }
        let vfs = Vfs::new(INSTALL, None).unwrap();
        let all = extract_all(&vfs);
        // Vanilla defines.lua has thousands of scalar defines.
        assert!(all.len() > 1500, "expected thousands of defines, got {}", all.len());
        // A few known ones with their types.
        let start = all.iter().find(|d| d.namespace == "NGame" && d.key == "START_DATE").unwrap();
        assert_eq!(start.value, "1444.11.11");
        assert_eq!(start.value_type, "string");
        let mcn = all.iter().find(|d| d.key == "MAX_COLONIAL_NATIONS").unwrap();
        assert_eq!(mcn.value_type, "number");
        assert_eq!(mcn.value, "75");
        // Nothing is overridden in a base-only session.
        assert!(all.iter().all(|d| !d.overridden));
        // Multiple namespaces present.
        let namespaces: std::collections::HashSet<_> = all.iter().map(|d| d.namespace.as_str()).collect();
        assert!(namespaces.len() > 5, "expected several NDefines namespaces");
    }

    #[test]
    fn mod_additive_override_wins_per_key() {
        // Base defines.lua sets both; a mod ships an additive common/defines/*.lua
        // overriding only START_DATE. The mod's START wins; the base's END stands.
        let dir = temp("mergemod");
        let base = dir.join("base");
        let m = dir.join("mod");
        std::fs::create_dir_all(base.join("map")).unwrap();
        std::fs::create_dir_all(base.join("common")).unwrap();
        std::fs::create_dir_all(m.join("common/defines")).unwrap();
        std::fs::write(base.join("map/provinces.bmp"), b"x").unwrap();
        std::fs::write(base.join(MAIN_REL), NESTED).unwrap();
        std::fs::write(
            m.join("common/defines/zz_dates.lua"),
            "NDefines.NGame.START_DATE = \"1300.1.1\"\n",
        )
        .unwrap();
        std::fs::write(m.join("descriptor.mod"), "name=\"m\"\n").unwrap();

        let vfs = Vfs::new(base.to_str().unwrap(), Some(m.to_str().unwrap())).unwrap();
        let d = defines_dates(&vfs);
        assert_eq!(d.start_date, "1300.1.1", "mod additive START override wins");
        assert_eq!(d.end_date, "1821.1.2", "base END unchanged");
    }

    #[test]
    fn real_vanilla_dates() {
        if !Path::new(INSTALL).join("map/provinces.bmp").is_file() {
            return;
        }
        let vfs = Vfs::new(INSTALL, None).unwrap();
        let d = defines_dates(&vfs);
        assert_eq!(d.start_date, "1444.11.11");
        assert_eq!(d.end_date, "1821.1.2");
    }

    #[test]
    fn anbennar_dates_read_through_mod_layer() {
        if !Path::new(INSTALL).join("map/provinces.bmp").is_file() || !Path::new(ANBENNAR).is_dir() {
            return;
        }
        let vfs = Vfs::new(INSTALL, Some(ANBENNAR)).unwrap();
        let d = defines_dates(&vfs);
        // Anbennar ships a full defines.lua; START/END must still resolve.
        assert!(crate::date::parse_date(&d.start_date).is_some());
        assert!(crate::date::parse_date(&d.end_date).is_some());
    }

    #[test]
    fn imperium_universalis_classical_range_through_mod_layer() {
        if !Path::new(INSTALL).join("map/provinces.bmp").is_file()
            || !Path::new(IMPERIUM).join("descriptor.mod").is_file()
        {
            return; // game or IU workshop item absent: no-op
        }
        let vfs = Vfs::new(INSTALL, Some(IMPERIUM)).unwrap();
        let d = defines_dates(&vfs);
        // IU ships common/defines/Imperium_Universalis.lua overriding the bounds to
        // the classical era (ab urbe condita): 142.7.10 .. 1128.1.1.
        assert_eq!(d.start_date, "142.7.10");
        assert_eq!(d.end_date, "1128.1.1");
    }
}
