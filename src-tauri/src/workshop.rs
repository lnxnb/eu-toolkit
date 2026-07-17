//! Steam Workshop detection + forking (Sprint 18.2 / 18.4).
//!
//! Workshop mods live under `<library>\steamapps\workshop\content\236850\<id>`,
//! a folder Steam **overwrites on every update** — editing one in place loses
//! work. "Forking" copies such a mod into the game's user mod folder
//! (`Documents\Paradox Interactive\Europa Universalis IV\mod\<slug>`), where it
//! is a normal, editable project the launcher also discovers.
//!
//! The copy runs on a worker thread and reports progress via Tauri events
//! (`fork-progress` / `fork-finished`); a shared cancel flag lets the UI abort,
//! after which the partial destination is removed. The heavy lifting here is
//! split into pure helpers (`is_workshop_path`, `rewrite_descriptor`,
//! `should_skip`, `has_enough_space`, `slugify`) so it is unit-testable without
//! touching the OS or the app handle.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

/// EU4's Steam AppID; its workshop items live under `content\236850`.
const EU4_APPID: &str = "236850";

/// One in-flight fork at a time (the UI is modal). `RUNNING` guards re-entry;
/// `CANCEL` is polled by the copy loop and set by `cancel_fork`.
static FORK_RUNNING: AtomicBool = AtomicBool::new(false);
static FORK_CANCEL: AtomicBool = AtomicBool::new(false);

// --- Detection ------------------------------------------------------------

/// True if `path` is (under) an EU4 workshop item folder. Case-insensitive and
/// slash-agnostic so it holds for any drive and either separator.
pub fn is_workshop_path(path: &Path) -> bool {
    let norm = path.to_string_lossy().replace('\\', "/").to_lowercase();
    norm.contains(&format!("steamapps/workshop/content/{EU4_APPID}"))
}

/// The `workshop\content\236850` folder for the Steam library that contains
/// `install` (walking up to the `steamapps` ancestor). `None` if `install` is
/// not inside a `steamapps` tree (e.g. a bare/GOG copy).
pub fn steam_workshop_dir(install: &Path) -> Option<PathBuf> {
    for anc in install.ancestors() {
        let is_steamapps = anc
            .file_name()
            .map(|n| n.to_string_lossy().eq_ignore_ascii_case("steamapps"))
            .unwrap_or(false);
        if is_steamapps {
            return Some(anc.join("workshop").join("content").join(EU4_APPID));
        }
    }
    None
}

/// A "Fork from Steam" install is Steam-backed only if its library actually has
/// a workshop folder for EU4 (i.e. the user is subscribed to something / Steam
/// has provisioned it). No workshop folder → the menu item is disabled.
pub fn is_steam_backed(install: &Path) -> bool {
    steam_workshop_dir(install)
        .map(|d| d.is_dir())
        .unwrap_or(false)
}

// --- Listing --------------------------------------------------------------

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkshopMod {
    pub id: String,
    pub path: String,
    pub name: String,
    pub size_bytes: u64,
}

/// Enumerates the subscribed EU4 workshop items for `install`'s Steam library:
/// folder id, descriptor `name`, and total size on disk. Sorted by name.
pub fn list_workshop_mods(install: &Path) -> Vec<WorkshopMod> {
    let Some(dir) = steam_workshop_dir(install) else {
        return Vec::new();
    };
    let mut mods = Vec::new();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return mods;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let id = entry.file_name().to_string_lossy().into_owned();
        let name = crate::vfs::read_descriptor(&path)
            .map(|text| crate::paradox::parse(&text))
            .and_then(|b| b.get_scalar("name").map(str::to_string))
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| id.clone());
        mods.push(WorkshopMod {
            id,
            name,
            size_bytes: dir_size(&path, true),
            path: path.to_string_lossy().replace('/', "\\"),
        });
    }
    mods.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    mods
}

// --- Copy exclusions & sizing --------------------------------------------

/// Junk we skip unless the user asked for a full copy: version-control folders
/// and Photoshop source files (often the biggest thing in a mod repo).
pub fn should_skip(name: &str, is_dir: bool, full_copy: bool) -> bool {
    if full_copy {
        return false;
    }
    if is_dir {
        return name.eq_ignore_ascii_case(".git") || name.eq_ignore_ascii_case(".github");
    }
    let lower = name.to_lowercase();
    lower.ends_with(".psd")
}

/// Total bytes that a fork with the given `full_copy` setting would copy
/// (mirrors `should_skip`, so progress totals line up with the actual copy).
pub fn dir_size(src: &Path, full_copy: bool) -> u64 {
    let mut total = 0;
    let Ok(entries) = std::fs::read_dir(src) else {
        return 0;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Ok(ft) = entry.file_type() else { continue };
        if should_skip(&name, ft.is_dir(), full_copy) {
            continue;
        }
        if ft.is_dir() {
            total += dir_size(&entry.path(), full_copy);
        } else if let Ok(meta) = entry.metadata() {
            total += meta.len();
        }
    }
    total
}

// --- Free-space preflight -------------------------------------------------

/// Space check with headroom: the destination drive must hold the payload plus
/// a 5% + 64 MiB cushion. Pure so it can be unit-tested without the OS call.
pub fn has_enough_space(required: u64, available: u64) -> bool {
    let cushion = required / 20 + 64 * 1024 * 1024;
    available >= required.saturating_add(cushion)
}

/// Free bytes on the volume that holds (the nearest existing ancestor of)
/// `path`. `None` if the query fails.
#[cfg(windows)]
pub fn available_space(path: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    extern "system" {
        fn GetDiskFreeSpaceExW(
            lpDirectoryName: *const u16,
            lpFreeBytesAvailableToCaller: *mut u64,
            lpTotalNumberOfBytes: *mut u64,
            lpTotalNumberOfFreeBytes: *mut u64,
        ) -> i32;
    }
    // The destination folder does not exist yet; query its nearest live ancestor.
    let dir = path.ancestors().find(|p| p.is_dir())?;
    let wide: Vec<u16> = dir
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut free: u64 = 0;
    let ok = unsafe {
        GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut free,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    (ok != 0).then_some(free)
}

#[cfg(not(windows))]
pub fn available_space(_path: &Path) -> Option<u64> {
    None
}

// --- Slug / collision -----------------------------------------------------

/// Filesystem-safe folder slug from a mod name: lowercase, alphanumerics kept,
/// every other run collapsed to a single `_`. Never empty.
pub fn slugify(name: &str) -> String {
    let mut s = String::new();
    let mut prev_us = false;
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            s.push(c.to_ascii_lowercase());
            prev_us = false;
        } else if !s.is_empty() && !prev_us {
            s.push('_');
            prev_us = true;
        }
    }
    while s.ends_with('_') {
        s.pop();
    }
    if s.is_empty() {
        "fork".to_string()
    } else {
        s
    }
}

/// A slug not already taken under `mod_root`: `slugify(base)`, else with a
/// `-2`, `-3`, … suffix.
pub fn unique_slug(mod_root: &Path, base: &str) -> String {
    let base = slugify(base);
    if !mod_root.join(&base).exists() {
        return base;
    }
    for n in 2.. {
        let cand = format!("{base}-{n}");
        if !mod_root.join(&cand).exists() {
            return cand;
        }
    }
    unreachable!()
}

// --- Descriptor rewrite ---------------------------------------------------

/// Rewrites `name=` and `path=` in a descriptor.mod, byte-for-byte preserving
/// every other line (comments, tags, `remote_file_id`, Windows-1252 bytes). A
/// missing `path=` line is appended. Operates on raw bytes so non-UTF-8
/// descriptors round-trip; the injected name/path are written as Latin-1.
pub fn rewrite_descriptor(bytes: &[u8], name: &str, path_fwd: &str) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len() + name.len() + path_fwd.len());
    let mut saw_path = false;
    let n = bytes.len();
    let mut start = 0;
    while start < n {
        let mut end = start;
        while end < n && bytes[end] != b'\n' {
            end += 1;
        }
        if end < n {
            end += 1; // include the '\n'
        }
        let line = &bytes[start..end];
        let (content, term) = split_terminator(line);
        match leading_key(content) {
            Some(k) if k == "name" => {
                push_kv(&mut out, "name", name);
                out.extend_from_slice(term);
            }
            Some(k) if k == "path" => {
                push_kv(&mut out, "path", path_fwd);
                out.extend_from_slice(term);
                saw_path = true;
            }
            _ => out.extend_from_slice(line),
        }
        start = end;
    }
    if !saw_path {
        if !out.is_empty() && *out.last().unwrap() != b'\n' {
            out.push(b'\n');
        }
        push_kv(&mut out, "path", path_fwd);
        out.push(b'\n');
    }
    out
}

/// Splits a raw line into (content, terminator) where terminator is "", "\n",
/// or "\r\n".
fn split_terminator(line: &[u8]) -> (&[u8], &[u8]) {
    if line.ends_with(b"\r\n") {
        line.split_at(line.len() - 2)
    } else if line.ends_with(b"\n") {
        line.split_at(line.len() - 1)
    } else {
        (line, &[])
    }
}

/// The `key` of a `key = value` line (leading whitespace skipped, ASCII
/// ident, then `=`), or None if the line is not a key assignment.
fn leading_key(content: &[u8]) -> Option<String> {
    let mut i = 0;
    while i < content.len() && (content[i] == b' ' || content[i] == b'\t') {
        i += 1;
    }
    let key_start = i;
    while i < content.len() && (content[i].is_ascii_alphanumeric() || content[i] == b'_') {
        i += 1;
    }
    if i == key_start {
        return None;
    }
    let key = &content[key_start..i];
    while i < content.len() && (content[i] == b' ' || content[i] == b'\t') {
        i += 1;
    }
    if i < content.len() && content[i] == b'=' {
        Some(String::from_utf8_lossy(key).into_owned())
    } else {
        None
    }
}

/// Appends `key="value"` with the value encoded as Latin-1 (chars beyond U+00FF
/// become `?`), matching the game's Windows-1252 files.
fn push_kv(out: &mut Vec<u8>, key: &str, value: &str) {
    out.extend_from_slice(key.as_bytes());
    out.extend_from_slice(b"=\"");
    for c in value.chars() {
        let u = c as u32;
        out.push(if u < 0x100 { u as u8 } else { b'?' });
    }
    out.push(b'"');
}

// --- Fork copy (thread-driven) -------------------------------------------

#[derive(Debug, PartialEq)]
pub enum ForkStatus {
    Completed,
    Canceled,
}

/// Recursively copies `src` into `dst`, honoring exclusions and the cancel
/// flag. `progress(copied, total, file)` fires after each file. On cancel it
/// stops and returns `Canceled` (the caller cleans up); errors propagate.
fn copy_tree(
    src: &Path,
    dst: &Path,
    full_copy: bool,
    cancel: &AtomicBool,
    copied: &mut u64,
    total: u64,
    progress: &mut dyn FnMut(u64, u64, &Path),
) -> Result<ForkStatus, String> {
    std::fs::create_dir_all(dst).map_err(|e| format!("Failed to create {}: {e}", dst.display()))?;
    let mut entries: Vec<_> = std::fs::read_dir(src)
        .map_err(|e| format!("Failed to read {}: {e}", src.display()))?
        .flatten()
        .collect();
    // Deterministic order keeps progress (and tests) stable.
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        if cancel.load(Ordering::Relaxed) {
            return Ok(ForkStatus::Canceled);
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let ft = entry
            .file_type()
            .map_err(|e| format!("Failed to stat {name}: {e}"))?;
        if should_skip(&name, ft.is_dir(), full_copy) {
            continue;
        }
        let from = entry.path();
        let to = dst.join(&name);
        if ft.is_dir() {
            if copy_tree(&from, &to, full_copy, cancel, copied, total, progress)?
                == ForkStatus::Canceled
            {
                return Ok(ForkStatus::Canceled);
            }
        } else {
            std::fs::copy(&from, &to)
                .map_err(|e| format!("Failed to copy {}: {e}", from.display()))?;
            *copied += entry.metadata().map(|m| m.len()).unwrap_or(0);
            progress(*copied, total, &from);
        }
    }
    Ok(ForkStatus::Completed)
}

/// Copies `src` into `dst`; on cancel or error the partial `dst` is removed so a
/// failed fork never leaves a half-written project behind.
pub fn fork_into(
    src: &Path,
    dst: &Path,
    full_copy: bool,
    cancel: &AtomicBool,
    total: u64,
    mut progress: impl FnMut(u64, u64, &Path),
) -> Result<ForkStatus, String> {
    let mut copied = 0u64;
    let result = copy_tree(src, dst, full_copy, cancel, &mut copied, total, &mut progress);
    match result {
        Ok(ForkStatus::Completed) => Ok(ForkStatus::Completed),
        Ok(ForkStatus::Canceled) => {
            let _ = std::fs::remove_dir_all(dst);
            Ok(ForkStatus::Canceled)
        }
        Err(e) => {
            let _ = std::fs::remove_dir_all(dst);
            Err(e)
        }
    }
}

// --- Cancel flag control (called from the Tauri command layer) ------------

/// Marks a fork as starting; false if one is already running.
pub fn begin() -> bool {
    if FORK_RUNNING.swap(true, Ordering::SeqCst) {
        return false;
    }
    FORK_CANCEL.store(false, Ordering::SeqCst);
    true
}

pub fn end() {
    FORK_RUNNING.store(false, Ordering::SeqCst);
}

pub fn request_cancel() {
    FORK_CANCEL.store(true, Ordering::SeqCst);
}

pub fn cancel_flag() -> &'static AtomicBool {
    &FORK_CANCEL
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("eu_toolkit_workshop_test_{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn detects_workshop_paths_any_slash_drive_case() {
        assert!(is_workshop_path(Path::new(
            r"C:\Program Files (x86)\Steam\steamapps\workshop\content\236850\12345"
        )));
        assert!(is_workshop_path(Path::new(
            "D:/SteamLibrary/steamapps/workshop/content/236850/98765"
        )));
        // Mixed case (Steam sometimes reports lowercase).
        assert!(is_workshop_path(Path::new(
            r"e:\STEAMAPPS\Workshop\Content\236850\1"
        )));
        // Not a workshop path.
        assert!(!is_workshop_path(Path::new(
            r"C:\Program Files (x86)\Steam\steamapps\common\Europa Universalis IV"
        )));
        assert!(!is_workshop_path(Path::new(r"C:\Users\me\projects\mymod")));
    }

    #[test]
    fn steam_backed_only_when_workshop_folder_exists() {
        // Install inside a steamapps tree with a workshop folder → Steam-backed.
        let root = tmp("steam_backed");
        let install = root
            .join("steamapps")
            .join("common")
            .join("Europa Universalis IV");
        std::fs::create_dir_all(&install).unwrap();
        std::fs::create_dir_all(root.join("steamapps/workshop/content/236850")).unwrap();
        assert!(is_steam_backed(&install));
        assert_eq!(
            steam_workshop_dir(&install).unwrap(),
            root.join("steamapps/workshop/content/236850")
        );

        // A bare/GOG-style copy with no steamapps ancestor → not Steam-backed.
        let bare = tmp("bare_copy").join("Europa Universalis IV");
        std::fs::create_dir_all(&bare).unwrap();
        assert!(!is_steam_backed(&bare));
        assert!(steam_workshop_dir(&bare).is_none());
    }

    #[test]
    fn steam_backed_false_without_workshop_folder() {
        // steamapps ancestor present but no workshop folder provisioned.
        let root = tmp("steam_no_workshop");
        let install = root
            .join("steamapps")
            .join("common")
            .join("Europa Universalis IV");
        std::fs::create_dir_all(&install).unwrap();
        assert!(!is_steam_backed(&install));
    }

    #[test]
    fn slugify_produces_safe_names() {
        assert_eq!(slugify("My Cool Mod!"), "my_cool_mod");
        assert_eq!(slugify("  spaces  "), "spaces");
        assert_eq!(slugify("A/B:C"), "a_b_c");
        assert_eq!(slugify("***"), "fork");
        assert_eq!(slugify("MixedCASE 42"), "mixedcase_42");
    }

    #[test]
    fn unique_slug_avoids_collisions() {
        let root = tmp("unique_slug");
        assert_eq!(unique_slug(&root, "My Mod"), "my_mod");
        std::fs::create_dir_all(root.join("my_mod")).unwrap();
        assert_eq!(unique_slug(&root, "My Mod"), "my_mod-2");
        std::fs::create_dir_all(root.join("my_mod-2")).unwrap();
        assert_eq!(unique_slug(&root, "My Mod"), "my_mod-3");
    }

    #[test]
    fn has_enough_space_respects_cushion() {
        // Exactly the payload is NOT enough (cushion required).
        assert!(!has_enough_space(1000, 1000));
        // Payload + 5% + 64MiB is enough.
        let need = 1000u64;
        let cushion = need / 20 + 64 * 1024 * 1024;
        assert!(has_enough_space(need, need + cushion));
        assert!(!has_enough_space(need, need + cushion - 1));
    }

    #[test]
    fn rewrite_descriptor_changes_only_name_and_path() {
        let original = b"name=\"Old Name\"\nversion=\"1.2\"\ntags={\n\t\"Gameplay\"\n}\nremote_file_id=\"999\"\npath=\"C:/old/place\"\nsupported_version=\"1.37.*\"\n";
        let out = rewrite_descriptor(original, "New Name", "C:/Users/me/mod/newmod");
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("name=\"New Name\""));
        assert!(text.contains("path=\"C:/Users/me/mod/newmod\""));
        // Everything else byte-identical.
        assert!(text.contains("version=\"1.2\""));
        assert!(text.contains("tags={\n\t\"Gameplay\"\n}"));
        assert!(text.contains("remote_file_id=\"999\""));
        assert!(text.contains("supported_version=\"1.37.*\""));
        // Old values gone.
        assert!(!text.contains("Old Name"));
        assert!(!text.contains("C:/old/place"));
    }

    #[test]
    fn rewrite_descriptor_appends_missing_path() {
        let original = b"name=\"Foo\"\nsupported_version=\"1.37.*\"\n";
        let out = rewrite_descriptor(original, "Foo (Fork)", "D:/mods/foo_fork");
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("name=\"Foo (Fork)\""));
        assert!(text.trim_end().ends_with("path=\"D:/mods/foo_fork\""));
        assert!(text.contains("supported_version=\"1.37.*\""));
    }

    #[test]
    fn rewrite_descriptor_preserves_crlf_and_spacing() {
        // CRLF terminators and `name = "..."` spacing round-trip on other lines.
        let original = b"name = \"A\"\r\nremote_file_id=\"1\"\r\npath=\"x\"\r\n";
        let out = rewrite_descriptor(original, "B", "y");
        assert_eq!(out, b"name=\"B\"\r\nremote_file_id=\"1\"\r\npath=\"y\"\r\n");
    }

    #[test]
    fn copy_skips_exclusions_by_default() {
        let root = tmp("copy_skip");
        let src = root.join("src");
        let dst = root.join("dst");
        std::fs::create_dir_all(src.join(".git")).unwrap();
        std::fs::create_dir_all(src.join(".github")).unwrap();
        std::fs::create_dir_all(src.join("map")).unwrap();
        std::fs::write(src.join(".git/HEAD"), b"ref").unwrap();
        std::fs::write(src.join("art.psd"), b"huge").unwrap();
        std::fs::write(src.join("descriptor.mod"), b"name=\"x\"\n").unwrap();
        std::fs::write(src.join("map/provinces.bmp"), b"bmp").unwrap();

        let cancel = AtomicBool::new(false);
        let total = dir_size(&src, false);
        let status = fork_into(&src, &dst, false, &cancel, total, |_, _, _| {}).unwrap();
        assert_eq!(status, ForkStatus::Completed);
        assert!(dst.join("descriptor.mod").is_file());
        assert!(dst.join("map/provinces.bmp").is_file());
        // Excluded.
        assert!(!dst.join(".git").exists());
        assert!(!dst.join(".github").exists());
        assert!(!dst.join("art.psd").exists());
    }

    #[test]
    fn full_copy_includes_everything() {
        let root = tmp("copy_full");
        let src = root.join("src");
        let dst = root.join("dst");
        std::fs::create_dir_all(src.join(".git")).unwrap();
        std::fs::write(src.join(".git/HEAD"), b"ref").unwrap();
        std::fs::write(src.join("art.psd"), b"huge").unwrap();

        let cancel = AtomicBool::new(false);
        let total = dir_size(&src, true);
        fork_into(&src, &dst, true, &cancel, total, |_, _, _| {}).unwrap();
        assert!(dst.join(".git/HEAD").is_file());
        assert!(dst.join("art.psd").is_file());
    }

    #[test]
    fn cancelled_copy_leaves_no_partial_folder() {
        let root = tmp("copy_cancel");
        let src = root.join("src");
        let dst = root.join("dst");
        std::fs::create_dir_all(&src).unwrap();
        // Many small files; cancel after the first is copied.
        for i in 0..50 {
            std::fs::write(src.join(format!("f{i:03}.txt")), b"data").unwrap();
        }
        let cancel = AtomicBool::new(false);
        let total = dir_size(&src, false);
        let mut count = 0;
        let status = fork_into(&src, &dst, false, &cancel, total, |_, _, _| {
            count += 1;
            if count == 1 {
                cancel.store(true, Ordering::SeqCst);
            }
        })
        .unwrap();
        assert_eq!(status, ForkStatus::Canceled);
        // Partial destination fully removed.
        assert!(!dst.exists());
    }

    #[test]
    fn list_workshop_mods_reads_ids_names_sizes() {
        let root = tmp("list_workshop");
        let install = root
            .join("steamapps")
            .join("common")
            .join("Europa Universalis IV");
        std::fs::create_dir_all(&install).unwrap();
        let ws = root.join("steamapps/workshop/content/236850");
        std::fs::create_dir_all(ws.join("111")).unwrap();
        std::fs::create_dir_all(ws.join("222")).unwrap();
        std::fs::write(ws.join("111/descriptor.mod"), b"name=\"Zeta Mod\"\n").unwrap();
        std::fs::write(ws.join("111/data.txt"), b"0123456789").unwrap();
        // 222 has no descriptor → falls back to its id as the name.
        std::fs::write(ws.join("222/data.txt"), b"x").unwrap();

        let mods = list_workshop_mods(&install);
        assert_eq!(mods.len(), 2);
        // Sorted by name: "222" (id fallback) sorts before "Zeta Mod".
        assert_eq!(mods[0].id, "222");
        assert_eq!(mods[0].name, "222");
        assert_eq!(mods[1].id, "111");
        assert_eq!(mods[1].name, "Zeta Mod");
        assert!(mods[1].size_bytes >= 10);
    }
}
