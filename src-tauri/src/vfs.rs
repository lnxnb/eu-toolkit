//! Virtual file system: resolves game-relative paths through an optional mod
//! project layer, falling back to the base installation. This is how a mod
//! "contains" a whole game: its files override same-path base files, and
//! descriptor `replace_path` entries disable base fallthrough per folder.

use std::path::{Path, PathBuf};

use crate::paradox::{self, Value};

pub struct Vfs {
    base: PathBuf,
    layer: Option<ModLayer>,
}

struct ModLayer {
    dir: PathBuf,
    /// Normalized (forward-slash, lowercase) folder prefixes from replace_path.
    replace_paths: Vec<String>,
}

fn normalize(rel: &str) -> String {
    rel.replace('\\', "/").to_lowercase()
}

impl ModLayer {
    fn replaces(&self, rel: &str) -> bool {
        let rel = normalize(rel);
        self.replace_paths
            .iter()
            .any(|rp| rel == *rp || rel.starts_with(&format!("{rp}/")))
    }
}

impl Vfs {
    pub fn new(base: &str, mod_dir: Option<&str>) -> Result<Vfs, String> {
        let base = PathBuf::from(base);
        if !base.is_dir() {
            return Err(format!("Base game not found: {}", base.display()));
        }
        let layer = match mod_dir {
            None => None,
            Some(dir) => {
                let dir = PathBuf::from(dir);
                if !dir.is_dir() {
                    return Err(format!("Mod project not found: {}", dir.display()));
                }
                Some(ModLayer {
                    replace_paths: read_replace_paths(&dir),
                    dir,
                })
            }
        };
        Ok(Vfs { base, layer })
    }

    /// The base installation directory.
    pub fn base_dir(&self) -> &Path {
        &self.base
    }

    /// The mod project directory, if this session has a mod layer. Used by the
    /// few readers that must see *both* layers (e.g. `common/defines.lua`, which
    /// the game loads additively rather than shadow-replacing).
    pub fn mod_dir(&self) -> Option<&Path> {
        self.layer.as_ref().map(|l| l.dir.as_path())
    }

    /// The mod layer's normalized (forward-slash, lowercase) `replace_path`
    /// folder prefixes, or empty for a base-only session. These base folders are
    /// hidden entirely by the mod (Sprint 30.4 diff classification).
    pub fn replace_dirs(&self) -> &[String] {
        self.layer.as_ref().map(|l| l.replace_paths.as_slice()).unwrap_or(&[])
    }

    /// True if `rel` (a folder or a file under one) is masked by a `replace_path`.
    pub fn is_replaced(&self, rel: &str) -> bool {
        self.layer.as_ref().is_some_and(|l| l.replaces(rel))
    }

    /// Recursively lists every file visible through the overlay under `rel_root`
    /// (`""` = whole tree) as `(game-relative path, absolute path, origin)`,
    /// where origin is `"mod"` or `"base"`. Mod files shadow same-path base
    /// files; a `replace_path`'d folder hides the base files beneath it. Sorted
    /// by relative path. This is the enumeration behind project-wide search
    /// (Sprint 30.3) and the mod-vs-base diff classification (Sprint 30.4).
    pub fn walk(&self, rel_root: &str) -> Vec<(String, PathBuf, &'static str)> {
        let root = rel_root.trim_matches('/');
        let mut map: std::collections::BTreeMap<String, (PathBuf, &'static str)> =
            std::collections::BTreeMap::new();

        // Mod layer first so its files claim their relative paths (shadowing).
        if let Some(layer) = &self.layer {
            let mut mod_files = Vec::new();
            collect_rel_files(&layer.dir, root, &mut mod_files);
            for (rel, abs) in mod_files {
                map.insert(rel, (abs, "mod"));
            }
        }

        // Base layer: fill in only paths not shadowed and not replaced.
        let mut base_files = Vec::new();
        collect_rel_files(&self.base, root, &mut base_files);
        for (rel, abs) in base_files {
            if self.is_replaced(&rel) {
                continue;
            }
            map.entry(rel).or_insert((abs, "base"));
        }

        map.into_iter().map(|(rel, (abs, o))| (rel, abs, o)).collect()
    }

    /// Absolute path for a game-relative file, mod layer first.
    pub fn resolve(&self, rel: &str) -> Option<PathBuf> {
        if let Some(layer) = &self.layer {
            let p = layer.dir.join(rel);
            if p.is_file() {
                return Some(p);
            }
            if layer.replaces(rel) {
                return None;
            }
        }
        let p = self.base.join(rel);
        p.is_file().then_some(p)
    }

    pub fn read(&self, rel: &str) -> Result<Vec<u8>, String> {
        let path = self
            .resolve(rel)
            .ok_or_else(|| format!("File not found: {rel}"))?;
        std::fs::read(&path).map_err(|e| format!("Failed to read {rel}: {e}"))
    }

    /// Merged directory listing: mod files shadow base files with the same
    /// name; a replace_path'd folder hides the base folder entirely.
    /// Returns (file_name, absolute_path) sorted by name.
    pub fn list_dir(&self, rel_dir: &str) -> Vec<(String, PathBuf)> {
        let mut entries: std::collections::BTreeMap<String, PathBuf> =
            std::collections::BTreeMap::new();
        let replaced = self
            .layer
            .as_ref()
            .is_some_and(|l| l.replaces(rel_dir));
        if !replaced {
            collect_dir(&self.base.join(rel_dir), &mut entries);
        }
        if let Some(layer) = &self.layer {
            collect_dir(&layer.dir.join(rel_dir), &mut entries);
        }
        entries.into_iter().collect()
    }

    /// All localisation files whose name ends with `suffix` (e.g.
    /// `_l_english.yml`), returned in the game's load order so that later files
    /// win when the same key appears twice: base files first, then the mod's
    /// files, with the mod's `localisation/replace/` files last (they override
    /// regardless of file-name collation). Recurses into subdirectories, as the
    /// game does, and honors a `replace_path` on `localisation` (which hides the
    /// base loc entirely). Within each group files are sorted for determinism.
    pub fn localisation_files(&self, suffix: &str) -> Vec<PathBuf> {
        let suffix = suffix.to_lowercase();
        let matches = |p: &Path| {
            p.file_name()
                .map(|n| n.to_string_lossy().to_lowercase().ends_with(&suffix))
                .unwrap_or(false)
        };
        let mut out = Vec::new();

        // Base layer (unless the mod replaces the whole localisation folder).
        let base_hidden = self
            .layer
            .as_ref()
            .is_some_and(|l| l.replaces("localisation"));
        if !base_hidden {
            let mut base = Vec::new();
            collect_files_recursive(&self.base.join("localisation"), &matches, &mut base);
            base.sort();
            out.extend(base);
        }

        // Mod layer: non-replace files first, then replace/ files (which win).
        if let Some(layer) = &self.layer {
            let root = layer.dir.join("localisation");
            let replace_root = root.join("replace");
            let mut all = Vec::new();
            collect_files_recursive(&root, &matches, &mut all);
            let (mut replace, mut normal): (Vec<PathBuf>, Vec<PathBuf>) =
                all.into_iter().partition(|p| p.starts_with(&replace_root));
            normal.sort();
            replace.sort();
            out.extend(normal);
            out.extend(replace);
        }
        out
    }
}

/// Recursively collects files under `dir` for which `matches` is true.
fn collect_files_recursive(
    dir: &Path,
    matches: &impl Fn(&Path) -> bool,
    out: &mut Vec<PathBuf>,
) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in read.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files_recursive(&path, matches, out);
        } else if path.is_file() && matches(&path) {
            out.push(path);
        }
    }
}

/// Recursively collects `(game-relative path, absolute path)` for every file
/// under `<layer_root>/<sub>`. `sub` = "" walks the whole layer. Relative paths
/// use forward slashes and are rooted at the layer (so they line up across the
/// base and mod layers).
fn collect_rel_files(layer_root: &Path, sub: &str, out: &mut Vec<(String, PathBuf)>) {
    let start = if sub.is_empty() { layer_root.to_path_buf() } else { layer_root.join(sub) };
    fn recurse(dir: &Path, layer_root: &Path, out: &mut Vec<(String, PathBuf)>) {
        let Ok(read) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in read.flatten() {
            let path = entry.path();
            if path.is_dir() {
                recurse(&path, layer_root, out);
            } else if path.is_file() {
                if let Ok(rel) = path.strip_prefix(layer_root) {
                    out.push((rel.to_string_lossy().replace('\\', "/"), path));
                }
            }
        }
    }
    recurse(&start, layer_root, out);
}

fn collect_dir(
    dir: &Path,
    entries: &mut std::collections::BTreeMap<String, PathBuf>,
) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in read.flatten() {
        let path = entry.path();
        if path.is_file() {
            entries.insert(entry.file_name().to_string_lossy().into_owned(), path);
        }
    }
}

/// Finds the mod's descriptor (descriptor.mod, else any *.mod) and pulls out
/// its replace_path entries.
fn read_replace_paths(mod_dir: &Path) -> Vec<String> {
    let Some(text) = read_descriptor(mod_dir) else {
        return Vec::new();
    };
    let block = paradox::parse(&text);
    block
        .items
        .iter()
        .filter_map(|(k, v)| match (k.as_deref(), v) {
            (Some("replace_path"), Value::Scalar(s)) => Some(normalize(s)),
            _ => None,
        })
        .collect()
}

pub fn read_descriptor(mod_dir: &Path) -> Option<String> {
    let canonical = mod_dir.join("descriptor.mod");
    if canonical.is_file() {
        return std::fs::read(&canonical)
            .ok()
            .map(|b| String::from_utf8_lossy(&b).into_owned());
    }
    for entry in std::fs::read_dir(mod_dir).ok()?.flatten() {
        let path = entry.path();
        if path.is_file() && path.extension().is_some_and(|e| e == "mod") {
            if let Ok(bytes) = std::fs::read(&path) {
                return Some(String::from_utf8_lossy(&bytes).into_owned());
            }
        }
    }
    None
}

/// A folder counts as a mod project if it has a descriptor or any of the
/// game's content folders.
pub fn is_mod_project(dir: &Path) -> bool {
    if !dir.is_dir() {
        return false;
    }
    if read_descriptor(dir).is_some() {
        return true;
    }
    ["common", "map", "history", "events", "localisation", "gfx"]
        .iter()
        .any(|d| dir.join(d).is_dir())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Each test gets its own root: tests run in parallel, and a shared dir
    // would let one test's cleanup race another's setup.
    fn setup(name: &str) -> (PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!("eu_toolkit_vfs_test_{name}"));
        let base = root.join("base");
        let mod_dir = root.join("mymod");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(base.join("common/countries")).unwrap();
        std::fs::create_dir_all(base.join("history/provinces")).unwrap();
        std::fs::create_dir_all(mod_dir.join("common/countries")).unwrap();
        std::fs::create_dir_all(mod_dir.join("history/provinces")).unwrap();

        std::fs::write(base.join("common/countries/France.txt"), "base france").unwrap();
        std::fs::write(base.join("common/countries/Spain.txt"), "base spain").unwrap();
        std::fs::write(base.join("history/provinces/1 - One.txt"), "base one").unwrap();
        std::fs::write(mod_dir.join("common/countries/France.txt"), "mod france").unwrap();
        std::fs::write(mod_dir.join("history/provinces/2 - Two.txt"), "mod two").unwrap();
        std::fs::write(
            mod_dir.join("descriptor.mod"),
            "name=\"Test Mod\"\nreplace_path=\"history/provinces\"\n",
        )
        .unwrap();
        (base, mod_dir)
    }

    #[test]
    fn overlay_resolution() {
        let (base, mod_dir) = setup("overlay");
        let vfs = Vfs::new(
            base.to_str().unwrap(),
            Some(mod_dir.to_str().unwrap()),
        )
        .unwrap();

        // Mod file shadows base file.
        assert_eq!(
            vfs.read("common/countries/France.txt").unwrap(),
            b"mod france"
        );
        // Fallthrough to base.
        assert_eq!(
            vfs.read("common/countries/Spain.txt").unwrap(),
            b"base spain"
        );
        // replace_path: base file is hidden even though the mod lacks it.
        assert!(vfs.resolve("history/provinces/1 - One.txt").is_none());

        // Merged listing: shadowing + union.
        let common: Vec<String> = vfs
            .list_dir("common/countries")
            .into_iter()
            .map(|(n, _)| n)
            .collect();
        assert_eq!(common, vec!["France.txt", "Spain.txt"]);
        assert_eq!(
            vfs.read("common/countries/France.txt").unwrap(),
            b"mod france"
        );
        // replace_path'd dir lists only mod files.
        let hist: Vec<String> = vfs
            .list_dir("history/provinces")
            .into_iter()
            .map(|(n, _)| n)
            .collect();
        assert_eq!(hist, vec!["2 - Two.txt"]);
    }

    #[test]
    fn walk_merges_shadows_and_replace() {
        let (base, mod_dir) = setup("walk");
        let vfs = Vfs::new(base.to_str().unwrap(), Some(mod_dir.to_str().unwrap())).unwrap();
        let all: Vec<(String, &str)> = vfs
            .walk("")
            .into_iter()
            .map(|(rel, _, o)| (rel, o))
            .filter(|(rel, _)| rel != "descriptor.mod")
            .collect();
        // France shadowed by mod; Spain from base; province 2 mod-only; province
        // 1 hidden by replace_path on history/provinces.
        assert!(all.contains(&("common/countries/France.txt".into(), "mod")));
        assert!(all.contains(&("common/countries/Spain.txt".into(), "base")));
        assert!(all.contains(&("history/provinces/2 - Two.txt".into(), "mod")));
        assert!(!all.iter().any(|(rel, _)| rel == "history/provinces/1 - One.txt"));

        // Sub-tree walk scopes to a folder.
        let common: Vec<String> = vfs.walk("common").into_iter().map(|(r, _, _)| r).collect();
        assert!(common.iter().all(|r| r.starts_with("common/")));
        assert_eq!(common.len(), 2);
    }

    #[test]
    fn base_only() {
        let (base, _) = setup("base_only");
        let vfs = Vfs::new(base.to_str().unwrap(), None).unwrap();
        assert_eq!(
            vfs.read("common/countries/France.txt").unwrap(),
            b"base france"
        );
        assert_eq!(vfs.list_dir("history/provinces").len(), 1);
    }
}
