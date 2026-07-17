//! Mod-vs-base diff browser (Sprint 30.4): classifies every file in the mod
//! project against the base install, and produces a per-file line diff for text
//! files. Complements the Edits panel (Sprint 30.1) — that panel is "this
//! session, unsaved"; this is "the whole mod vs vanilla".
//!
//! Classification:
//!   • **added**   — a mod file with no base counterpart at the same path
//!   • **shadows** — a mod file that overrides a same-path base file
//!   • **hidden**  — a base *folder* masked by a descriptor `replace_path`
//!
//! Line diff is a plain LCS (no external crate — none is in Cargo.toml).

use crate::search::decode;
use crate::vfs::Vfs;

/// One project file classified against the base install.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileClass {
    /// Game-relative path (forward slashes).
    pub rel: String,
    /// `added` | `shadows`.
    pub class: String,
    /// True for non-text assets (bmp/dds/tga/png/…): diffed by size/type only.
    pub binary: bool,
    /// File size in bytes (the mod copy).
    pub size: u64,
}

/// A base folder wholly masked by a `replace_path`, plus a summary of what it
/// contained (so the UI can explain what the mod is hiding).
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HiddenFolder {
    /// The replace_path folder (game-relative).
    pub rel: String,
    /// Number of base files under it that are now hidden.
    pub base_file_count: usize,
}

/// The whole-project classification payload.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectChanges {
    pub files: Vec<FileClass>,
    pub hidden: Vec<HiddenFolder>,
}

/// Binary asset extensions — diffed as a size/type note only.
const BINARY_EXTS: &[&str] = &[
    "bmp", "dds", "tga", "png", "jpg", "jpeg", "wav", "ogg", "mp3", "mesh",
    "anim", "bank", "ttf", "otf", "ico", "webp",
];

fn is_binary(rel: &str) -> bool {
    match rel.rsplit_once('.') {
        Some((_, ext)) => BINARY_EXTS.contains(&ext.to_ascii_lowercase().as_str()),
        None => false,
    }
}

/// Classifies every file in the mod layer and lists the folders its
/// `replace_path`s hide. A base-only session has an empty mod layer → nothing.
pub fn classify(vfs: &Vfs) -> ProjectChanges {
    let mut files = Vec::new();
    let Some(mod_dir) = vfs.mod_dir() else {
        return ProjectChanges { files, hidden: Vec::new() };
    };

    // Enumerate mod-origin files only (walk returns mod entries with origin=mod).
    for (rel, abs, origin) in vfs.walk("") {
        if origin != "mod" {
            continue;
        }
        // Does the *base* install carry the same path?
        let base_has = vfs.base_dir().join(&rel).is_file();
        let size = std::fs::metadata(&abs).map(|m| m.len()).unwrap_or(0);
        files.push(FileClass {
            class: if base_has { "shadows" } else { "added" }.to_string(),
            binary: is_binary(&rel),
            size,
            rel,
        });
    }
    files.sort_by(|a, b| a.rel.cmp(&b.rel));

    // Hidden folders: each replace_path that actually masks a base folder.
    let mut hidden = Vec::new();
    for rp in vfs.replace_dirs() {
        let base_folder = vfs.base_dir().join(rp);
        if base_folder.is_dir() {
            let count = count_files(&base_folder);
            hidden.push(HiddenFolder { rel: rp.clone(), base_file_count: count });
        }
    }
    hidden.sort_by(|a, b| a.rel.cmp(&b.rel));
    let _ = mod_dir; // (kept for clarity; walk already scoped to the layer)

    ProjectChanges { files, hidden }
}

fn count_files(dir: &std::path::Path) -> usize {
    let mut n = 0;
    if let Ok(read) = std::fs::read_dir(dir) {
        for entry in read.flatten() {
            let p = entry.path();
            if p.is_dir() {
                n += count_files(&p);
            } else if p.is_file() {
                n += 1;
            }
        }
    }
    n
}

/// One line of a text diff.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffLine {
    /// `same` | `add` (mod-only) | `del` (base-only).
    pub tag: String,
    pub text: String,
}

/// A file's diff vs the base install.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDiff {
    pub rel: String,
    /// True when there is no base counterpart (the whole file is an addition).
    pub added: bool,
    pub binary: bool,
    /// Size of the base copy (0 when absent) and the mod copy.
    pub base_size: u64,
    pub mod_size: u64,
    /// The line diff (empty for binary files or a pure addition — the UI notes
    /// those specially).
    pub lines: Vec<DiffLine>,
}

/// LCS line diff of `base` vs `modv`, emitting del(base-only)/add(mod-only)/same.
pub fn line_diff(base: &[&str], modv: &[&str]) -> Vec<DiffLine> {
    let n = base.len();
    let m = modv.len();
    // DP table of LCS lengths.
    let mut dp = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i][j] = if base[i] == modv[j] {
                dp[i + 1][j + 1] + 1
            } else {
                dp[i + 1][j].max(dp[i][j + 1])
            };
        }
    }
    let mut out = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if base[i] == modv[j] {
            out.push(DiffLine { tag: "same".into(), text: base[i].to_string() });
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            out.push(DiffLine { tag: "del".into(), text: base[i].to_string() });
            i += 1;
        } else {
            out.push(DiffLine { tag: "add".into(), text: modv[j].to_string() });
            j += 1;
        }
    }
    while i < n {
        out.push(DiffLine { tag: "del".into(), text: base[i].to_string() });
        i += 1;
    }
    while j < m {
        out.push(DiffLine { tag: "add".into(), text: modv[j].to_string() });
        j += 1;
    }
    out
}

/// Computes the diff of one mod-layer file vs its base counterpart.
pub fn diff_file(vfs: &Vfs, rel: &str) -> Result<FileDiff, String> {
    let mod_dir = vfs.mod_dir().ok_or("No mod project in this session")?;
    let mod_path = mod_dir.join(rel);
    if !mod_path.is_file() {
        return Err(format!("Not a project file: {rel}"));
    }
    let base_path = vfs.base_dir().join(rel);
    let base_present = base_path.is_file();
    let binary = is_binary(rel);
    let mod_size = std::fs::metadata(&mod_path).map(|m| m.len()).unwrap_or(0);
    let base_size = if base_present {
        std::fs::metadata(&base_path).map(|m| m.len()).unwrap_or(0)
    } else {
        0
    };

    if binary || !base_present {
        return Ok(FileDiff {
            rel: rel.to_string(),
            added: !base_present,
            binary,
            base_size,
            mod_size,
            lines: Vec::new(),
        });
    }

    let loc = rel.to_ascii_lowercase().ends_with(".yml");
    let base_text = decode(&std::fs::read(&base_path).map_err(|e| e.to_string())?, loc);
    let mod_text = decode(&std::fs::read(&mod_path).map_err(|e| e.to_string())?, loc);
    let base_lines: Vec<&str> = base_text.lines().collect();
    let mod_lines: Vec<&str> = mod_text.lines().collect();
    Ok(FileDiff {
        rel: rel.to_string(),
        added: false,
        binary: false,
        base_size,
        mod_size,
        lines: line_diff(&base_lines, &mod_lines),
    })
}

/// Tauri command: classify the whole project against the base install.
#[tauri::command(async)]
pub fn get_project_changes(
    install_path: String,
    mod_path: Option<String>,
) -> Result<ProjectChanges, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    Ok(classify(&vfs))
}

/// Tauri command: line diff of one project file vs base.
#[tauri::command(async)]
pub fn get_file_diff(
    install_path: String,
    mod_path: Option<String>,
    rel: String,
) -> Result<FileDiff, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    diff_file(&vfs, &rel)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture(name: &str) -> (PathBuf, Vfs) {
        let root = std::env::temp_dir().join(format!("eu_toolkit_diff_test_{name}"));
        let _ = std::fs::remove_dir_all(&root);
        let base = root.join("base");
        let m = root.join("mod");
        std::fs::create_dir_all(base.join("common/religions")).unwrap();
        std::fs::create_dir_all(base.join("history/provinces")).unwrap();
        std::fs::create_dir_all(base.join("gfx/flags")).unwrap();
        std::fs::create_dir_all(m.join("common/religions")).unwrap();
        std::fs::create_dir_all(m.join("common/cultures")).unwrap();
        std::fs::create_dir_all(m.join("gfx/flags")).unwrap();

        std::fs::write(
            base.join("common/religions/00_religion.txt"),
            b"catholic = { icon = 1 }\nprotestant = { icon = 2 }\northo = { icon = 3 }\n",
        )
        .unwrap();
        std::fs::write(base.join("history/provinces/1 - One.txt"), b"owner = SWE\n").unwrap();
        // Mod shadows the religion file (edits the middle line).
        std::fs::write(
            m.join("common/religions/00_religion.txt"),
            b"catholic = { icon = 1 }\nprotestant = { icon = 99 }\northo = { icon = 3 }\n",
        )
        .unwrap();
        // Mod-only added file.
        std::fs::write(m.join("common/cultures/00_cultures.txt"), b"euro = { }\n").unwrap();
        // A binary added asset.
        std::fs::write(m.join("gfx/flags/A38.tga"), &[0u8, 1, 2, 3, 4, 5]).unwrap();
        // replace_path hides history/provinces.
        std::fs::write(
            m.join("descriptor.mod"),
            "name=\"m\"\nreplace_path=\"history/provinces\"\n",
        )
        .unwrap();
        let vfs = Vfs::new(base.to_str().unwrap(), Some(m.to_str().unwrap())).unwrap();
        (root, vfs)
    }

    #[test]
    fn classifies_added_shadows_hidden_and_binary() {
        let (_root, vfs) = fixture("classify");
        let c = classify(&vfs);
        let find = |r: &str| c.files.iter().find(|f| f.rel == r).unwrap();
        assert_eq!(find("common/religions/00_religion.txt").class, "shadows");
        assert_eq!(find("common/cultures/00_cultures.txt").class, "added");
        let flag = find("gfx/flags/A38.tga");
        assert_eq!(flag.class, "added");
        assert!(flag.binary);
        assert_eq!(flag.size, 6);
        // descriptor.mod is an added file (no base counterpart).
        assert_eq!(find("descriptor.mod").class, "added");
        // Hidden folder from replace_path.
        let h = c.hidden.iter().find(|h| h.rel == "history/provinces").unwrap();
        assert_eq!(h.base_file_count, 1);
    }

    #[test]
    fn line_diff_marks_edits() {
        let (_root, vfs) = fixture("diff");
        let d = diff_file(&vfs, "common/religions/00_religion.txt").unwrap();
        assert!(!d.binary && !d.added);
        // The changed middle line appears as a del + add pair; the outer lines same.
        assert!(d.lines.iter().any(|l| l.tag == "del" && l.text.contains("icon = 2")));
        assert!(d.lines.iter().any(|l| l.tag == "add" && l.text.contains("icon = 99")));
        assert_eq!(d.lines.iter().filter(|l| l.tag == "same").count(), 2);
    }

    #[test]
    fn added_and_binary_have_no_line_diff() {
        let (_root, vfs) = fixture("added");
        let d = diff_file(&vfs, "common/cultures/00_cultures.txt").unwrap();
        assert!(d.added && d.lines.is_empty());
        let b = diff_file(&vfs, "gfx/flags/A38.tga").unwrap();
        assert!(b.binary && b.lines.is_empty() && b.mod_size == 6);
    }

    #[test]
    fn lcs_pure_add_and_del() {
        let d = line_diff(&["a", "b", "c"], &["a", "x", "b", "c"]);
        assert_eq!(d.iter().filter(|l| l.tag == "add").count(), 1);
        assert_eq!(d.iter().filter(|l| l.tag == "del").count(), 0);
        assert_eq!(d.iter().filter(|l| l.tag == "same").count(), 3);
    }
}
