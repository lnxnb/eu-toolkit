//! Recent projects (Sprint 18.1): the launch screen's most-recently-opened list.
//!
//! **Storage shape**: one JSON array under the settings-DB key `recent_projects`
//! (see `db.rs`). A single value — not prefix-scanned rows — because the list is
//! read and rewritten as a whole every time (dedupe/cap/sort are list-wide
//! operations), so one atomic value is simpler than N rows to reconcile.
//!
//! Each entry records the session that was opened: the project folder (absent for
//! base-game sessions), the install it was opened against, a display name, a
//! last-opened timestamp (unix millis), and a pin flag. Identity for dedupe is the
//! project path (case-insensitively, slash-normalized), or — for base sessions,
//! which have no project — the install path. Reopening the same identity bumps its
//! timestamp in place instead of adding a duplicate row.
//!
//! `missing` is *not* persisted meaningfully: it's recomputed on every list from
//! the live filesystem (project folder still a mod? install still valid?) so a
//! stored value is always overwritten before the list reaches the UI.

use std::path::Path;

use crate::{installations, paradox, vfs};

pub const RECENTS_KEY: &str = "recent_projects";

/// Cap on unpinned rows. Pinned rows are always kept; the oldest unpinned rows
/// beyond this fall off.
const MAX_UNPINNED: usize = 15;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentProject {
    /// The mod folder; `None` for base-game sessions.
    pub project_path: Option<String>,
    pub install_path: String,
    pub display_name: String,
    /// Unix time in milliseconds.
    pub last_opened: i64,
    #[serde(default)]
    pub pinned: bool,
    /// Recomputed at list time from the filesystem; never trusted from storage.
    #[serde(default)]
    pub missing: bool,
}

/// Slash-normalized, trailing-separator-trimmed, lowercased — Windows paths are
/// case-insensitive and reach the same folder via `/` or `\`.
fn normalize(path: &str) -> String {
    path.replace('/', "\\")
        .trim_end_matches('\\')
        .to_lowercase()
}

/// Dedupe identity: the project folder, or (for base sessions) the install.
fn identity(project_path: &Option<String>, install_path: &str) -> String {
    match project_path {
        Some(p) => normalize(p),
        None => format!("__base__|{}", normalize(install_path)),
    }
}

/// Insert `entry` (or bump the existing row with the same identity), then sort
/// (pinned first, each group most-recent-first) and cap unpinned rows. A bump
/// inherits the prior row's pin state so recording an open never silently
/// unpins. `entry.last_opened` is taken as-is (caller stamps "now").
pub fn upsert(mut list: Vec<RecentProject>, mut entry: RecentProject) -> Vec<RecentProject> {
    let id = identity(&entry.project_path, &entry.install_path);
    list.retain(|e| {
        if identity(&e.project_path, &e.install_path) == id {
            entry.pinned = entry.pinned || e.pinned;
            false
        } else {
            true
        }
    });
    list.push(entry);
    sort(&mut list);
    cap(&mut list);
    list
}

/// Remove the row with `project_path`/`install_path`'s identity, if present.
pub fn remove(mut list: Vec<RecentProject>, project_path: &Option<String>, install_path: &str) -> Vec<RecentProject> {
    let id = identity(project_path, install_path);
    list.retain(|e| identity(&e.project_path, &e.install_path) != id);
    list
}

/// Set the pin flag on the matching row, then re-sort and re-cap (pinning can
/// pull a row above the cap; unpinning can push it below).
pub fn set_pinned(
    mut list: Vec<RecentProject>,
    project_path: &Option<String>,
    install_path: &str,
    pinned: bool,
) -> Vec<RecentProject> {
    let id = identity(project_path, install_path);
    for e in list.iter_mut() {
        if identity(&e.project_path, &e.install_path) == id {
            e.pinned = pinned;
        }
    }
    sort(&mut list);
    cap(&mut list);
    list
}

/// Pinned rows first; within each group, most-recently-opened first.
fn sort(list: &mut [RecentProject]) {
    list.sort_by(|a, b| {
        b.pinned
            .cmp(&a.pinned)
            .then(b.last_opened.cmp(&a.last_opened))
    });
}

/// Keep every pinned row plus the most-recent `MAX_UNPINNED` unpinned rows.
/// Assumes `list` is already sorted (unpinned in most-recent-first order).
fn cap(list: &mut Vec<RecentProject>) {
    let mut unpinned = 0usize;
    list.retain(|e| {
        if e.pinned {
            return true;
        }
        unpinned += 1;
        unpinned <= MAX_UNPINNED
    });
}

/// Refreshes each row's `missing` flag from the live filesystem: a project row is
/// missing if its folder is no longer a mod project; any row is missing if its
/// install path is no longer a valid EU4 installation.
pub fn annotate_missing(list: &mut [RecentProject]) {
    for e in list.iter_mut() {
        let install_ok = installations::is_valid_installation(Path::new(&e.install_path));
        let project_ok = match &e.project_path {
            Some(p) => vfs::is_mod_project(Path::new(p)),
            None => true,
        };
        e.missing = !install_ok || !project_ok;
    }
}

/// Display name for a recorded session: base sessions read "Base game @ <install>";
/// project sessions use the descriptor `name`, falling back to the folder name.
pub fn display_name(project_path: &Option<String>, install_path: &str) -> String {
    match project_path {
        None => format!("Base game @ {install_path}"),
        Some(p) => {
            let dir = Path::new(p);
            vfs::read_descriptor(dir)
                .map(|text| paradox::parse(&text))
                .and_then(|b| b.get_scalar("name").map(str::to_string))
                .filter(|s| !s.trim().is_empty())
                .or_else(|| dir.file_name().map(|n| n.to_string_lossy().into_owned()))
                .unwrap_or_else(|| p.clone())
        }
    }
}

pub fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Parse the stored JSON array; a missing/corrupt value yields an empty list
/// rather than an error (the recents list is best-effort UI state).
pub fn parse(stored: Option<String>) -> Vec<RecentProject> {
    stored
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn serialize(list: &[RecentProject]) -> Result<String, String> {
    serde_json::to_string(list).map_err(|e| format!("Failed to serialize recent projects: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: Option<&str>, install: &str, ts: i64) -> RecentProject {
        RecentProject {
            project_path: path.map(str::to_string),
            install_path: install.to_string(),
            display_name: path.unwrap_or("base").to_string(),
            last_opened: ts,
            pinned: false,
            missing: false,
        }
    }

    #[test]
    fn orders_most_recent_first() {
        let mut list = Vec::new();
        list = upsert(list, entry(Some("a"), "I", 100));
        list = upsert(list, entry(Some("b"), "I", 300));
        list = upsert(list, entry(Some("c"), "I", 200));
        let names: Vec<_> = list.iter().map(|e| e.display_name.as_str()).collect();
        assert_eq!(names, vec!["b", "c", "a"]);
    }

    #[test]
    fn dedupes_by_path_and_bumps_timestamp() {
        let mut list = Vec::new();
        list = upsert(list, entry(Some("a"), "I", 100));
        list = upsert(list, entry(Some("b"), "I", 200));
        // Reopen "a" later — bumps, not duplicates.
        list = upsert(list, entry(Some("a"), "I", 500));
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].display_name, "a");
        assert_eq!(list[0].last_opened, 500);
    }

    #[test]
    fn dedupe_is_path_normalized() {
        let mut list = Vec::new();
        list = upsert(list, entry(Some(r"C:\Mods\MyMod"), "I", 100));
        list = upsert(list, entry(Some("c:/mods/mymod/"), "I", 200));
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].last_opened, 200);
    }

    #[test]
    fn base_sessions_dedupe_by_install() {
        let mut list = Vec::new();
        list = upsert(list, entry(None, "InstallA", 100));
        list = upsert(list, entry(None, "InstallA", 200));
        list = upsert(list, entry(None, "InstallB", 150));
        assert_eq!(list.len(), 2);
        // Two distinct base rows, one per install.
        assert!(list.iter().any(|e| e.install_path == "InstallB"));
    }

    #[test]
    fn pinned_sort_above_unpinned_both_recent_first() {
        let mut list = vec![
            entry(Some("a"), "I", 100),
            entry(Some("b"), "I", 400),
            entry(Some("c"), "I", 300),
        ];
        sort(&mut list);
        // "b" newest first among unpinned.
        assert_eq!(list[0].display_name, "b");
        // Pin the oldest.
        list = set_pinned(list, &Some("a".into()), "I", true);
        assert_eq!(list[0].display_name, "a");
        assert!(list[0].pinned);
        // Remaining unpinned still most-recent-first.
        assert_eq!(list[1].display_name, "b");
        assert_eq!(list[2].display_name, "c");
    }

    #[test]
    fn upsert_preserves_pin_on_bump() {
        let mut list = vec![entry(Some("a"), "I", 100)];
        list = set_pinned(list, &Some("a".into()), "I", true);
        // Recording another open of "a" (pinned=false in the fresh entry).
        list = upsert(list, entry(Some("a"), "I", 900));
        assert!(list[0].pinned);
        assert_eq!(list[0].last_opened, 900);
    }

    #[test]
    fn caps_unpinned_but_keeps_all_pinned() {
        let mut list = Vec::new();
        // 20 unpinned, newest timestamps last.
        for i in 0..20 {
            list = upsert(list, entry(Some(&format!("p{i}")), "I", i as i64));
        }
        let unpinned = list.iter().filter(|e| !e.pinned).count();
        assert_eq!(unpinned, MAX_UNPINNED);
        // The newest survive; oldest ("p0") fell off.
        assert!(list.iter().all(|e| e.display_name != "p0"));
        assert!(list.iter().any(|e| e.display_name == "p19"));

        // Pin an old one back in, then add more — pinned stays regardless of cap.
        list.push(RecentProject {
            pinned: true,
            ..entry(Some("kept"), "I", -1)
        });
        for i in 20..40 {
            list = upsert(list, entry(Some(&format!("p{i}")), "I", i as i64));
        }
        assert!(list.iter().any(|e| e.display_name == "kept" && e.pinned));
        assert_eq!(list.iter().filter(|e| !e.pinned).count(), MAX_UNPINNED);
    }

    #[test]
    fn remove_by_identity() {
        let mut list = vec![entry(Some("a"), "I", 100), entry(None, "I", 200)];
        list = remove(list, &Some("a".into()), "I");
        assert_eq!(list.len(), 1);
        assert!(list[0].project_path.is_none());
        // Removing the base row too.
        list = remove(list, &None, "I");
        assert!(list.is_empty());
    }

    #[test]
    fn json_round_trip() {
        let mut list = Vec::new();
        list = upsert(list, entry(Some("a"), "I", 100));
        list = set_pinned(list, &Some("a".into()), "I", true);
        list = upsert(list, entry(None, "I", 200));
        let json = serialize(&list).unwrap();
        let back = parse(Some(json));
        assert_eq!(back, list);
    }

    #[test]
    fn parse_handles_missing_and_corrupt() {
        assert!(parse(None).is_empty());
        assert!(parse(Some("not json".into())).is_empty());
        assert!(parse(Some("{}".into())).is_empty());
    }

    #[test]
    fn annotate_missing_flags_absent_paths() {
        let root = std::env::temp_dir().join("eu_toolkit_recents_missing_test");
        let _ = std::fs::remove_dir_all(&root);
        let install = root.join("install");
        let project = root.join("project");
        // Valid install: map/provinces.bmp present.
        std::fs::create_dir_all(install.join("map")).unwrap();
        std::fs::write(install.join("map").join("provinces.bmp"), b"x").unwrap();
        // Valid project: has a descriptor.
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(project.join("descriptor.mod"), "name=\"P\"\n").unwrap();

        let install_s = install.to_string_lossy().into_owned();
        let mut list = vec![
            // present project + present install
            entry(Some(&project.to_string_lossy()), &install_s, 1),
            // absent project, present install
            entry(Some(&root.join("gone").to_string_lossy()), &install_s, 2),
            // base session, absent install
            entry(None, &root.join("noinstall").to_string_lossy(), 3),
        ];
        annotate_missing(&mut list);
        assert!(!list[0].missing, "present project + install should be ok");
        assert!(list[1].missing, "absent project folder is missing");
        assert!(list[2].missing, "absent install is missing");

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn display_name_derives_from_descriptor_then_folder_then_base() {
        let root = std::env::temp_dir().join("eu_toolkit_recents_name_test");
        let _ = std::fs::remove_dir_all(&root);
        let named = root.join("named");
        let unnamed = root.join("PlainFolder");
        std::fs::create_dir_all(&named).unwrap();
        std::fs::create_dir_all(&unnamed).unwrap();
        std::fs::write(named.join("descriptor.mod"), "name=\"Fancy Mod\"\n").unwrap();
        std::fs::write(unnamed.join("descriptor.mod"), "version=\"1\"\n").unwrap();

        assert_eq!(
            display_name(&Some(named.to_string_lossy().into_owned()), "I"),
            "Fancy Mod"
        );
        assert_eq!(
            display_name(&Some(unnamed.to_string_lossy().into_owned()), "I"),
            "PlainFolder"
        );
        assert_eq!(
            display_name(&None, r"C:\Games\EU4"),
            r"Base game @ C:\Games\EU4"
        );

        std::fs::remove_dir_all(&root).unwrap();
    }
}
