//! Typed pending-edit queue applier.
//!
//! The frontend keeps one ordered queue of composite operations per session
//! (each composite = one undo unit). On Save it flattens the queue into an
//! ordered list of [`TypedEdit`]s and hands it here. Every edit kind maps onto
//! the byte-surgical [`crate::mod_writer`] toolkit, [`crate::loc::write_overrides`],
//! or a raw binary write — the base install is never touched.
//!
//! Edits are grouped per target file in queue order and applied sequentially on
//! the file's *evolving* buffer, so when several edits hit one file in a single
//! save, later edits see the byte offsets shifted by earlier ones. Two separate
//! groups in the queue that target the same file are merged (in order) into one
//! group, so they still compose on the same evolving buffer.

use std::collections::HashMap;
use std::path::Path;

use crate::game_data;
use crate::loc;
use crate::mod_writer::{self, Edit};
use crate::vfs::Vfs;

/// One typed, serializable pending edit. Mirrored 1:1 by the frontend
/// `TypedEdit` union. Serialized internally-tagged on `kind` with camelCase
/// variant and field names (e.g. `{ "kind": "setScalar", "file": …, … }`).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum TypedEdit {
    /// Replace the scalar value at `path` inside `file` (last path element is
    /// the scalar key). `quoted` wraps the value in `"`.
    SetScalar {
        file: String,
        path: Vec<String>,
        value: String,
        #[serde(default)]
        quoted: bool,
    },
    /// Replace the `{ … }` block value at `path` in `file` (last path element is
    /// the block key). Byte-surgical; for `color`/`revolutionary_colors` blocks.
    SetBlock {
        file: String,
        path: Vec<String>,
        value: String,
    },
    /// Remove a statement (scalar line or whole `{ … }` block) named `key` from
    /// the block at `block_path` in `file`, optionally filtering by scalar value.
    RemoveStatement {
        file: String,
        block_path: Vec<String>,
        key: String,
        #[serde(default)]
        value: Option<String>,
    },
    /// Insert a pre-formatted statement into the block at `block_path` in `file`.
    InsertStatement {
        file: String,
        block_path: Vec<String>,
        statement: String,
    },
    /// Insert a new top-level `Y.M.D = { ... }` block into `file` in date order
    /// among the existing dated blocks (Sprint 12.3 edit-at-date). To merge into
    /// an existing exact-date block, emit an `InsertStatement` on `["Y.M.D"]`.
    InsertDatedBlock {
        file: String,
        date: String,
        statement: String,
    },
    /// Override an `NDefines.<namespace>.<key>` define (Sprint 12.4 dates,
    /// generalized to any namespace by Sprint 28's Defines editor). The project's
    /// own `common/defines.lua` is extended if present, else a minimal additive
    /// `common/defines/zz_eutoolkit_defines.lua` is created. `namespace` defaults
    /// to `NGame` (the date bounds) when omitted.
    SetDefine {
        key: String,
        value: String,
        #[serde(default)]
        namespace: Option<String>,
    },
    /// Add a bare id to the id-list at `list_path` in `file`.
    AddId {
        file: String,
        list_path: Vec<String>,
        id: String,
    },
    /// Remove a bare id from the id-list at `list_path` in `file`.
    RemoveId {
        file: String,
        list_path: Vec<String>,
        id: String,
    },
    /// Move an id out of one id-list and into another. The two lists may live in
    /// the same file or in two different files (climate/area/tradenode membership).
    ListMove {
        from_file: String,
        from_path: Vec<String>,
        to_file: String,
        to_path: Vec<String>,
        id: String,
    },
    /// Append raw text to the end of `file` (a new relation block, tag mapping…).
    AppendText { file: String, text: String },
    /// Create/overwrite `file` wholesale with `text` (a brand-new game file).
    CreateFile { file: String, text: String },
    /// Line-surgical rewrite of a semicolon CSV (`map/adjacencies.csv`, Sprint
    /// 25). `rows` is the FULL desired row list; unchanged origin rows re-emit
    /// their exact original bytes, so untouched lines round-trip byte-identical.
    /// The frontend emits exactly one of these per adjacencies save.
    CsvRewrite {
        file: String,
        rows: Vec<crate::adjacencies::RowInput>,
    },
    /// Delete `file` from the project folder (Sprint 13.2 war deletion of a
    /// toolkit-created file). Only ever removes a file **inside the project**;
    /// the path is validated to be relative with no `..` escape, and the base
    /// install is never touched. A no-op if the project has no such file.
    DeleteFile { file: String },
    /// Localisation override: `key` -> `value` in the project's toolkit-owned
    /// `_l_english.yml` (UTF-8 BOM).
    LocOverride { key: String, value: String },
    /// Remove a localisation override `key` from the project's toolkit-owned loc
    /// file (Sprint S2.1 country deletion). Only ever touches the project's own
    /// `localisation/replace/zz_eutoolkit_l_english.yml`; base-game loc that
    /// defines a tag elsewhere is never edited. A no-op if the file or key is
    /// absent. Applied AFTER any `LocOverride`s so a create-then-delete nets out.
    LocRemove { key: String },
    /// Write raw bytes to `file` (e.g. a generated flag TGA). Bytes cross IPC as
    /// a JSON number array.
    BinaryAsset { file: String, bytes: Vec<u8> },
    /// Rewrite `map/provinces.bmp` by replaying color-space pixel ops against the
    /// copy-on-write base bitmap (Province Colors mode add/expand/dissolve). The
    /// frontend ships semantic ops, never the 34 MB bitmap; the re-encode happens
    /// once in [`crate::province_edit::apply_ops`]. Multiple queued groups compose
    /// on the evolving bitmap like any other file.
    ProvinceBmp {
        file: String,
        ops: Vec<crate::province_edit::BmpOp>,
    },
    /// Rename the starting ruler (latest dated monarch <= 1444.11.11) of the
    /// country tagged `tag`. Resolved to that country's history file and applied
    /// with [`mod_writer::rename_ruler`]; the frontend need not know the date.
    RenameRuler { tag: String, name: String },
}

/// A resolved operation on a single file's bytes.
enum FileOp {
    /// A byte-surgical toolkit edit.
    Edit(Edit),
    /// Rename the starting ruler on the whole-file buffer.
    RenameRuler(String),
    /// Replace the whole buffer with raw bytes.
    Binary(Vec<u8>),
    /// Line-surgical CSV rewrite (adjacencies.csv) over the resolved base bytes.
    Csv(Vec<crate::adjacencies::RowInput>),
    /// Replay color-space pixel ops against the resolved base `provinces.bmp`.
    Bmp(Vec<crate::province_edit::BmpOp>),
}

/// Records `op` against `file`, preserving first-seen file order and merging
/// repeated files into one ordered group.
fn push_op(
    order: &mut Vec<String>,
    groups: &mut HashMap<String, Vec<FileOp>>,
    file: String,
    op: FileOp,
) {
    if !groups.contains_key(&file) {
        order.push(file.clone());
    }
    groups.entry(file).or_default().push(op);
}

/// Applies the flattened pending-edit queue to `project_dir`, copy-on-write from
/// the sources resolved through `vfs`. Returns the game-relative paths written
/// (each file once, plus the loc override file if any loc edits were queued).
pub fn apply_queue(
    vfs: &Vfs,
    project_dir: &Path,
    edits: &[TypedEdit],
) -> Result<Vec<String>, String> {
    let mut order: Vec<String> = Vec::new();
    let mut groups: HashMap<String, Vec<FileOp>> = HashMap::new();
    let mut loc_entries: Vec<(String, String)> = Vec::new();
    let mut loc_removes: Vec<String> = Vec::new();
    let mut define_entries: Vec<(String, String, String)> = Vec::new();
    let mut delete_files: Vec<String> = Vec::new();

    for edit in edits {
        match edit {
            TypedEdit::SetScalar {
                file,
                path,
                value,
                quoted,
            } => push_op(
                &mut order,
                &mut groups,
                file.clone(),
                FileOp::Edit(Edit::SetScalar {
                    path: path.clone(),
                    value: value.clone(),
                    quoted: *quoted,
                }),
            ),
            TypedEdit::SetBlock { file, path, value } => push_op(
                &mut order,
                &mut groups,
                file.clone(),
                FileOp::Edit(Edit::SetBlock {
                    path: path.clone(),
                    value: value.clone(),
                }),
            ),
            TypedEdit::RemoveStatement {
                file,
                block_path,
                key,
                value,
            } => push_op(
                &mut order,
                &mut groups,
                file.clone(),
                FileOp::Edit(Edit::RemoveStatement {
                    block_path: block_path.clone(),
                    key: key.clone(),
                    value: value.clone(),
                }),
            ),
            TypedEdit::InsertStatement {
                file,
                block_path,
                statement,
            } => push_op(
                &mut order,
                &mut groups,
                file.clone(),
                FileOp::Edit(Edit::InsertStatement {
                    block_path: block_path.clone(),
                    statement: statement.clone(),
                }),
            ),
            TypedEdit::InsertDatedBlock {
                file,
                date,
                statement,
            } => push_op(
                &mut order,
                &mut groups,
                file.clone(),
                FileOp::Edit(Edit::InsertDatedBlock {
                    date: date.clone(),
                    statement: statement.clone(),
                }),
            ),
            TypedEdit::SetDefine { key, value, namespace } => {
                define_entries.push((
                    namespace.clone().unwrap_or_else(|| "NGame".to_string()),
                    key.clone(),
                    value.clone(),
                ));
            }
            TypedEdit::AddId {
                file,
                list_path,
                id,
            } => push_op(
                &mut order,
                &mut groups,
                file.clone(),
                FileOp::Edit(Edit::AddId {
                    list_path: list_path.clone(),
                    id: id.clone(),
                }),
            ),
            TypedEdit::RemoveId {
                file,
                list_path,
                id,
            } => push_op(
                &mut order,
                &mut groups,
                file.clone(),
                FileOp::Edit(Edit::RemoveId {
                    list_path: list_path.clone(),
                    id: id.clone(),
                }),
            ),
            TypedEdit::ListMove {
                from_file,
                from_path,
                to_file,
                to_path,
                id,
            } => {
                // Two edits, possibly spanning two files. Remove first so that a
                // same-file, same-list move nets out correctly.
                push_op(
                    &mut order,
                    &mut groups,
                    from_file.clone(),
                    FileOp::Edit(Edit::RemoveId {
                        list_path: from_path.clone(),
                        id: id.clone(),
                    }),
                );
                push_op(
                    &mut order,
                    &mut groups,
                    to_file.clone(),
                    FileOp::Edit(Edit::AddId {
                        list_path: to_path.clone(),
                        id: id.clone(),
                    }),
                );
            }
            TypedEdit::AppendText { file, text } => push_op(
                &mut order,
                &mut groups,
                file.clone(),
                FileOp::Edit(Edit::Append { text: text.clone() }),
            ),
            TypedEdit::CreateFile { file, text } => push_op(
                &mut order,
                &mut groups,
                file.clone(),
                FileOp::Edit(Edit::CreateFile { text: text.clone() }),
            ),
            TypedEdit::CsvRewrite { file, rows } => push_op(
                &mut order,
                &mut groups,
                file.clone(),
                FileOp::Csv(rows.clone()),
            ),
            TypedEdit::DeleteFile { file } => delete_files.push(file.clone()),
            TypedEdit::BinaryAsset { file, bytes } => push_op(
                &mut order,
                &mut groups,
                file.clone(),
                FileOp::Binary(bytes.clone()),
            ),
            TypedEdit::ProvinceBmp { file, ops } => push_op(
                &mut order,
                &mut groups,
                file.clone(),
                FileOp::Bmp(ops.clone()),
            ),
            TypedEdit::RenameRuler { tag, name } => {
                let (file_name, _) = game_data::country_history_file(vfs, tag)
                    .ok_or_else(|| format!("No country history file found for {tag}"))?;
                let rel = format!("history/countries/{file_name}");
                push_op(
                    &mut order,
                    &mut groups,
                    rel,
                    FileOp::RenameRuler(name.clone()),
                );
            }
            TypedEdit::LocOverride { key, value } => {
                loc_entries.push((key.clone(), value.clone()));
            }
            TypedEdit::LocRemove { key } => {
                loc_removes.push(key.clone());
            }
        }
    }

    let mut written = Vec::new();
    for rel in &order {
        // Resolve the copy-on-write source (mod shadows base; a missing file
        // starts empty, e.g. for CreateFile / a first Append).
        let mut bytes = match vfs.resolve(rel) {
            Some(p) => std::fs::read(&p).map_err(|e| format!("Failed to read {rel}: {e}"))?,
            None => Vec::new(),
        };
        for op in &groups[rel] {
            bytes = match op {
                FileOp::Edit(e) => mod_writer::apply(&bytes, e).map_err(|e| format!("{rel}: {e}"))?,
                FileOp::RenameRuler(name) => {
                    mod_writer::rename_ruler(&bytes, name).map_err(|e| format!("{rel}: {e}"))?
                }
                FileOp::Binary(b) => b.clone(),
                FileOp::Csv(rows) => {
                    crate::adjacencies::rewrite(&bytes, rows).map_err(|e| format!("{rel}: {e}"))?
                }
                FileOp::Bmp(ops) => {
                    crate::province_edit::apply_ops(&bytes, ops).map_err(|e| format!("{rel}: {e}"))?
                }
            };
        }
        written.push(mod_writer::write_scaffold(project_dir, rel, &bytes)?);
    }

    // File deletions (toolkit-created war files). Guarded to the project folder:
    // the path must be relative with no parent-dir escape, so a delete can never
    // reach outside `project_dir` (and the base install is never touched). Runs
    // after writes so a create-then-delete in one queue nets to "gone".
    for rel in &delete_files {
        let p = Path::new(rel);
        let unsafe_path = p.is_absolute()
            || p.components().any(|c| {
                matches!(
                    c,
                    std::path::Component::ParentDir
                        | std::path::Component::RootDir
                        | std::path::Component::Prefix(_)
                )
            });
        if unsafe_path {
            return Err(format!("Refusing to delete unsafe path: {rel}"));
        }
        let dest = project_dir.join(rel);
        if dest.exists() {
            std::fs::remove_file(&dest).map_err(|e| format!("Failed to delete {rel}: {e}"))?;
            written.push(rel.clone());
        }
    }

    // Localisation overrides -> project's toolkit-owned loc file (UTF-8 BOM).
    // write_overrides invalidates the loc cache so later reads see the change.
    if !loc_entries.is_empty() {
        written.push(loc::write_overrides(project_dir, &loc_entries)?);
    }

    // Localisation override REMOVALS (country deletion). Applied after the adds so
    // a scaffold-then-delete in one queue nets to "gone". No-op if the toolkit loc
    // file (or the key) is absent — base-game loc is never touched.
    if !loc_removes.is_empty() {
        if let Some(rel) = loc::remove_overrides(project_dir, &loc_removes)? {
            if !written.contains(&rel) {
                written.push(rel);
            }
        }
    }

    // Playable-date-range overrides -> the project's defines.lua (copy-on-write;
    // extends an existing project file, else an additive minimal override).
    if !define_entries.is_empty() {
        written.push(crate::defines::write_overrides(project_dir, &define_entries)?);
    }

    // Disk changed: drop every session read cache (see cache.rs) so the next
    // read re-derives from the files just written.
    crate::cache::invalidate_all();

    Ok(written)
}

/// Folds every queued edit that targets `file` onto that single file's
/// copy-on-write buffer (resolved through `vfs`, empty if the file doesn't exist
/// yet), applying them in queue order on the evolving buffer — exactly as
/// [`apply_queue`] does per file, but purely in memory and for one file only.
///
/// This is the honest way to show *pending* script state in the 14.2 tree editor:
/// `parse_script_block` reads the SAVED file, but the queue holds edits not yet
/// written, so [`crate::script_tree::parse_script_block_with_edits`] previews the
/// file here before parsing. Edits targeting other files are ignored; loc/define/
/// delete edits (which never mutate this file's bytes) are skipped.
pub fn preview_file(vfs: &Vfs, file: &str, edits: &[TypedEdit]) -> Result<Vec<u8>, String> {
    let mut bytes = match vfs.resolve(file) {
        Some(p) => std::fs::read(&p).map_err(|e| format!("Failed to read {file}: {e}"))?,
        None => Vec::new(),
    };
    let apply = |b: &[u8], e: &Edit| mod_writer::apply(b, e).map_err(|m| format!("{file}: {m}"));

    for edit in edits {
        match edit {
            TypedEdit::SetScalar { file: f, path, value, quoted } if f == file => {
                bytes = apply(&bytes, &Edit::SetScalar { path: path.clone(), value: value.clone(), quoted: *quoted })?;
            }
            TypedEdit::SetBlock { file: f, path, value } if f == file => {
                bytes = apply(&bytes, &Edit::SetBlock { path: path.clone(), value: value.clone() })?;
            }
            TypedEdit::RemoveStatement { file: f, block_path, key, value } if f == file => {
                bytes = apply(&bytes, &Edit::RemoveStatement { block_path: block_path.clone(), key: key.clone(), value: value.clone() })?;
            }
            TypedEdit::InsertStatement { file: f, block_path, statement } if f == file => {
                bytes = apply(&bytes, &Edit::InsertStatement { block_path: block_path.clone(), statement: statement.clone() })?;
            }
            TypedEdit::InsertDatedBlock { file: f, date, statement } if f == file => {
                bytes = apply(&bytes, &Edit::InsertDatedBlock { date: date.clone(), statement: statement.clone() })?;
            }
            TypedEdit::AddId { file: f, list_path, id } if f == file => {
                bytes = apply(&bytes, &Edit::AddId { list_path: list_path.clone(), id: id.clone() })?;
            }
            TypedEdit::RemoveId { file: f, list_path, id } if f == file => {
                bytes = apply(&bytes, &Edit::RemoveId { list_path: list_path.clone(), id: id.clone() })?;
            }
            TypedEdit::ListMove { from_file, from_path, to_file, to_path, id } => {
                // Remove-then-add, matching apply_queue's ordering; either side may
                // (or may not) touch this file.
                if from_file == file {
                    bytes = apply(&bytes, &Edit::RemoveId { list_path: from_path.clone(), id: id.clone() })?;
                }
                if to_file == file {
                    bytes = apply(&bytes, &Edit::AddId { list_path: to_path.clone(), id: id.clone() })?;
                }
            }
            TypedEdit::AppendText { file: f, text } if f == file => {
                bytes = apply(&bytes, &Edit::Append { text: text.clone() })?;
            }
            TypedEdit::CreateFile { file: f, text } if f == file => {
                bytes = apply(&bytes, &Edit::CreateFile { text: text.clone() })?;
            }
            TypedEdit::BinaryAsset { file: f, bytes: b } if f == file => {
                bytes = b.clone();
            }
            TypedEdit::CsvRewrite { file: f, rows } if f == file => {
                bytes = crate::adjacencies::rewrite(&bytes, rows)
                    .map_err(|m| format!("{file}: {m}"))?;
            }
            TypedEdit::ProvinceBmp { file: f, ops } if f == file => {
                bytes = crate::province_edit::apply_ops(&bytes, ops)
                    .map_err(|m| format!("{file}: {m}"))?;
            }
            TypedEdit::RenameRuler { tag, name } => {
                if let Some((file_name, _)) = game_data::country_history_file(vfs, tag) {
                    if format!("history/countries/{file_name}") == file {
                        bytes = mod_writer::rename_ruler(&bytes, name)
                            .map_err(|m| format!("{file}: {m}"))?;
                    }
                }
            }
            // Edits that never mutate this file's bytes (loc/define/delete, or an
            // edit targeting a different file) are ignored.
            _ => {}
        }
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A base install + empty project, both under a per-test temp root.
    fn setup(name: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let root = std::env::temp_dir().join(format!("eu_toolkit_editq_test_{name}"));
        let base = root.join("base");
        let project = root.join("project");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(base.join("map")).unwrap();
        std::fs::write(base.join("map/provinces.bmp"), b"x").unwrap();
        (base, project)
    }

    fn write_base(base: &Path, rel: &str, bytes: &[u8]) {
        let p = base.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, bytes).unwrap();
    }

    fn read_project(project: &Path, rel: &str) -> Vec<u8> {
        std::fs::read(project.join(rel)).unwrap()
    }

    #[test]
    fn mixed_queue_writes_all_files() {
        let (base, project) = setup("mixed");
        write_base(
            &base,
            "history/provinces/1.txt",
            b"owner = FRA\ncontroller = FRA\nbase_tax = 3\n",
        );
        write_base(
            &base,
            "map/climate.txt",
            b"tropical = {\n\t746\n}\narctic = {\n\t100 200\n}\n",
        );
        write_base(&base, "common/countries/France.txt", b"color = { 1 1 1 }\n");

        let vfs = Vfs::new(base.to_str().unwrap(), None).unwrap();
        let edits = vec![
            // scalar set
            TypedEdit::SetScalar {
                file: "history/provinces/1.txt".into(),
                path: vec!["base_tax".into()],
                value: "5".into(),
                quoted: false,
            },
            // list move (arctic -> tropical, single file here)
            TypedEdit::ListMove {
                from_file: "map/climate.txt".into(),
                from_path: vec!["arctic".into()],
                to_file: "map/climate.txt".into(),
                to_path: vec!["tropical".into()],
                id: "100".into(),
            },
            // block insert
            TypedEdit::InsertStatement {
                file: "common/countries/France.txt".into(),
                block_path: vec![],
                statement: "graphical_culture = westerngfx".into(),
            },
            // new file
            TypedEdit::CreateFile {
                file: "common/country_tags/zz_new.txt".into(),
                text: "ZZZ = \"countries/Newland.txt\"\n".into(),
            },
            // loc override
            TypedEdit::LocOverride {
                key: "ZZZ".into(),
                value: "Newland".into(),
            },
        ];

        let written = apply_queue(&vfs, &project, &edits).unwrap();

        assert!(written.contains(&"history/provinces/1.txt".to_string()));
        assert!(written.contains(&"map/climate.txt".to_string()));
        assert!(written.contains(&"common/countries/France.txt".to_string()));
        assert!(written.contains(&"common/country_tags/zz_new.txt".to_string()));
        assert!(written.contains(&loc::OVERRIDE_REL.to_string()));

        let prov = String::from_utf8(read_project(&project, "history/provinces/1.txt")).unwrap();
        assert!(prov.contains("base_tax = 5"));
        assert!(prov.contains("owner = FRA"));

        let climate = String::from_utf8(read_project(&project, "map/climate.txt")).unwrap();
        assert!(climate.contains("tropical = {\n\t746\n\t100\n}"));
        assert!(climate.contains("arctic = {\n\t200\n}"));

        let country =
            String::from_utf8(read_project(&project, "common/countries/France.txt")).unwrap();
        assert!(country.contains("graphical_culture = westerngfx"));

        let tags =
            String::from_utf8(read_project(&project, "common/country_tags/zz_new.txt")).unwrap();
        assert_eq!(tags, "ZZZ = \"countries/Newland.txt\"\n");

        let loc = String::from_utf8(read_project(&project, loc::OVERRIDE_REL)).unwrap();
        assert!(loc.contains(" ZZZ:0 \"Newland\""));
    }

    #[test]
    fn two_edits_same_file_offset_shift() {
        // Two edits to the same file in one save: the second must apply to the
        // buffer already changed by the first (byte offsets have shifted).
        let (base, project) = setup("offset");
        write_base(&base, "history/provinces/1.txt", b"base_tax = 3\nbase_production = 3\n");
        let vfs = Vfs::new(base.to_str().unwrap(), None).unwrap();

        let edits = vec![
            // Growing the first value shifts every later byte.
            TypedEdit::SetScalar {
                file: "history/provinces/1.txt".into(),
                path: vec!["base_tax".into()],
                value: "100".into(),
                quoted: false,
            },
            TypedEdit::SetScalar {
                file: "history/provinces/1.txt".into(),
                path: vec!["base_production".into()],
                value: "77".into(),
                quoted: false,
            },
        ];
        let written = apply_queue(&vfs, &project, &edits).unwrap();
        // The file is written exactly once even though two edits targeted it.
        assert_eq!(
            written.iter().filter(|w| *w == "history/provinces/1.txt").count(),
            1
        );
        let out = String::from_utf8(read_project(&project, "history/provinces/1.txt")).unwrap();
        assert_eq!(out, "base_tax = 100\nbase_production = 77\n");
    }

    #[test]
    fn two_groups_same_file_merge() {
        // Same file appears twice in the queue with another file in between.
        let (base, project) = setup("merge");
        write_base(&base, "a.txt", b"x = 1\ny = 2\n");
        write_base(&base, "b.txt", b"z = 3\n");
        let vfs = Vfs::new(base.to_str().unwrap(), None).unwrap();
        let edits = vec![
            TypedEdit::SetScalar {
                file: "a.txt".into(),
                path: vec!["x".into()],
                value: "11".into(),
                quoted: false,
            },
            TypedEdit::SetScalar {
                file: "b.txt".into(),
                path: vec!["z".into()],
                value: "33".into(),
                quoted: false,
            },
            TypedEdit::SetScalar {
                file: "a.txt".into(),
                path: vec!["y".into()],
                value: "22".into(),
                quoted: false,
            },
        ];
        let written = apply_queue(&vfs, &project, &edits).unwrap();
        assert_eq!(written.iter().filter(|w| *w == "a.txt").count(), 1);
        let a = String::from_utf8(read_project(&project, "a.txt")).unwrap();
        assert_eq!(a, "x = 11\ny = 22\n");
    }

    #[test]
    fn binary_asset_round_trips_bytes() {
        let (base, project) = setup("binary");
        let vfs = Vfs::new(base.to_str().unwrap(), None).unwrap();
        let raw: Vec<u8> = vec![0x00, 0x01, 0xFF, 0x80, 0x7F, 0xE9];
        let edits = vec![TypedEdit::BinaryAsset {
            file: "gfx/flags/ZZZ.tga".into(),
            bytes: raw.clone(),
        }];
        let written = apply_queue(&vfs, &project, &edits).unwrap();
        assert_eq!(written, vec!["gfx/flags/ZZZ.tga".to_string()]);
        assert_eq!(read_project(&project, "gfx/flags/ZZZ.tga"), raw);
    }

    #[test]
    fn province_bmp_edit_repaints_and_round_trips() {
        // A queued ProvinceBmp edit decodes the copy-on-write base bitmap, paints
        // a pixel, and re-encodes into the project — the base install is never
        // touched, and the result decodes back to the edited pixels.
        use std::io::Cursor;
        let (base, project) = setup("province_bmp");
        // Build a real 2x2 BMP base (the shared `setup` writes a bogus "x").
        let mut img = image::RgbImage::new(2, 2);
        img.put_pixel(0, 0, image::Rgb([10, 10, 10]));
        img.put_pixel(1, 0, image::Rgb([20, 20, 20]));
        img.put_pixel(0, 1, image::Rgb([30, 30, 30]));
        img.put_pixel(1, 1, image::Rgb([40, 40, 40]));
        let mut bmp = Vec::new();
        img.write_to(&mut Cursor::new(&mut bmp), image::ImageFormat::Bmp)
            .unwrap();
        std::fs::write(base.join("map/provinces.bmp"), &bmp).unwrap();

        let vfs = Vfs::new(base.to_str().unwrap(), None).unwrap();
        let edits = vec![TypedEdit::ProvinceBmp {
            file: "map/provinces.bmp".into(),
            ops: vec![crate::province_edit::BmpOp::Paint {
                pixels: vec![1], // top-down idx 1 = pixel (1,0)
                color: [99, 88, 77],
            }],
        }];
        let written = apply_queue(&vfs, &project, &edits).unwrap();
        assert!(written.contains(&"map/provinces.bmp".to_string()));

        // The base install file is unchanged.
        assert_eq!(std::fs::read(base.join("map/provinces.bmp")).unwrap(), bmp);

        // The project copy decodes back with the painted pixel and neighbours intact.
        let out = std::fs::read(project.join("map/provinces.bmp")).unwrap();
        let back = image::load_from_memory(&out).unwrap().to_rgb8();
        assert_eq!(back.get_pixel(0, 0).0, [10, 10, 10]);
        assert_eq!(back.get_pixel(1, 0).0, [99, 88, 77]);
        assert_eq!(back.get_pixel(0, 1).0, [30, 30, 30]);
        assert_eq!(back.get_pixel(1, 1).0, [40, 40, 40]);
    }

    #[test]
    fn list_move_across_two_files() {
        let (base, project) = setup("listmove2");
        write_base(&base, "common/tradenodes/00_a.txt", b"genoa = {\n\tmembers = { 1 2 3 }\n}\n");
        write_base(&base, "common/tradenodes/00_b.txt", b"venice = {\n\tmembers = { 4 5 }\n}\n");
        let vfs = Vfs::new(base.to_str().unwrap(), None).unwrap();
        let edits = vec![TypedEdit::ListMove {
            from_file: "common/tradenodes/00_a.txt".into(),
            from_path: vec!["genoa".into(), "members".into()],
            to_file: "common/tradenodes/00_b.txt".into(),
            to_path: vec!["venice".into(), "members".into()],
            id: "2".into(),
        }];
        let written = apply_queue(&vfs, &project, &edits).unwrap();
        assert_eq!(written.len(), 2);
        let a = String::from_utf8(read_project(&project, "common/tradenodes/00_a.txt")).unwrap();
        let b = String::from_utf8(read_project(&project, "common/tradenodes/00_b.txt")).unwrap();
        assert_eq!(a, "genoa = {\n\tmembers = { 1 3 }\n}\n");
        assert_eq!(b, "venice = {\n\tmembers = { 4 5 2 }\n}\n");
    }

    #[test]
    fn rename_ruler_through_queue() {
        let (base, project) = setup("ruler");
        write_base(
            &base,
            "history/countries/FRA - France.txt",
            b"government = monarchy\n1422.10.21 = {\n\tmonarch = {\n\t\tname = \"Charles VII\"\n\t}\n}\n",
        );
        let vfs = Vfs::new(base.to_str().unwrap(), None).unwrap();
        let edits = vec![TypedEdit::RenameRuler {
            tag: "FRA".into(),
            name: "Charles the Renamed".into(),
        }];
        let written = apply_queue(&vfs, &project, &edits).unwrap();
        assert_eq!(written, vec!["history/countries/FRA - France.txt".to_string()]);
        let out =
            String::from_utf8(read_project(&project, "history/countries/FRA - France.txt")).unwrap();
        assert!(out.contains("\"Charles the Renamed\""));
        assert!(out.contains("government = monarchy"));
    }

    #[test]
    fn add_then_remove_province_round_trip() {
        // Mirrors the frontend add/remove-province edit generation (Sprint 1.4):
        // an owned province is added to FRA (owner/controller replaced, FRA core
        // inserted), then removed (owner/controller lines deleted, FRA core
        // deleted). Everything else — the other core, culture, religion, goods,
        // comments, and a Windows-1252 byte — must survive byte-for-byte.
        let (base, project) = setup("add_remove_prov");
        let rel = "history/provinces/1 - Test.txt";
        // 0xE9 = é in Windows-1252, inside a comment; must round-trip.
        let mut original =
            b"owner = CAS\ncontroller = CAS\nadd_core = CAS\nculture = castilian # caf".to_vec();
        original.push(0xE9);
        original.extend_from_slice(b"\nreligion = catholic\ntrade_goods = wool\nbase_tax = 3\n");
        write_base(&base, rel, &original);
        let vfs = Vfs::new(base.to_str().unwrap(), None).unwrap();

        let edits = vec![
            // --- ADD to FRA (owner+controller present -> replace; FRA core new) ---
            TypedEdit::SetScalar {
                file: rel.into(),
                path: vec!["owner".into()],
                value: "FRA".into(),
                quoted: false,
            },
            TypedEdit::SetScalar {
                file: rel.into(),
                path: vec!["controller".into()],
                value: "FRA".into(),
                quoted: false,
            },
            TypedEdit::InsertStatement {
                file: rel.into(),
                block_path: vec![],
                statement: "add_core = FRA".into(),
            },
            // --- REMOVE from FRA (owner/controller present, FRA core present) ---
            TypedEdit::RemoveStatement {
                file: rel.into(),
                block_path: vec![],
                key: "owner".into(),
                value: None,
            },
            TypedEdit::RemoveStatement {
                file: rel.into(),
                block_path: vec![],
                key: "controller".into(),
                value: None,
            },
            TypedEdit::RemoveStatement {
                file: rel.into(),
                block_path: vec![],
                key: "add_core".into(),
                value: Some("FRA".into()),
            },
        ];

        apply_queue(&vfs, &project, &edits).unwrap();
        let out = read_project(&project, rel);

        // owner/controller gone; the original CAS core survives (other owner's
        // core is never touched); culture/religion/goods intact.
        let expected = {
            let mut e = b"add_core = CAS\nculture = castilian # caf".to_vec();
            e.push(0xE9);
            e.extend_from_slice(b"\nreligion = catholic\ntrade_goods = wool\nbase_tax = 3\n");
            e
        };
        assert_eq!(out, expected);
    }

    #[test]
    fn add_province_on_uncolonized_inserts_keys() {
        // An uncolonized province (no owner/controller/core) added to FRA: the
        // generator inserts all three keys; other keys are untouched.
        let (base, project) = setup("add_uncolonized");
        let rel = "history/provinces/2 - Empty.txt";
        write_base(&base, rel, b"culture = swedish\nreligion = catholic\ntrade_goods = grain\n");
        let vfs = Vfs::new(base.to_str().unwrap(), None).unwrap();

        let edits = vec![
            TypedEdit::InsertStatement {
                file: rel.into(),
                block_path: vec![],
                statement: "owner = FRA".into(),
            },
            TypedEdit::InsertStatement {
                file: rel.into(),
                block_path: vec![],
                statement: "controller = FRA".into(),
            },
            TypedEdit::InsertStatement {
                file: rel.into(),
                block_path: vec![],
                statement: "add_core = FRA".into(),
            },
        ];
        apply_queue(&vfs, &project, &edits).unwrap();
        let out = String::from_utf8(read_project(&project, rel)).unwrap();
        assert_eq!(
            out,
            "culture = swedish\nreligion = catholic\ntrade_goods = grain\nowner = FRA\ncontroller = FRA\nadd_core = FRA\n"
        );
    }

    #[test]
    fn country_identity_and_government_edits_round_trip() {
        // Mirrors the CountryPanel Identity + Government edit generation
        // (Sprint 1.2). One country history file + one common/countries file; a
        // representative edit of each shape. Every untouched byte — comments,
        // sibling keys, a Windows-1252 high byte, formatting — must survive.
        let (base, project) = setup("country_edits");
        let hist = "history/countries/FRA - France.txt";
        let cty = "common/countries/France.txt";
        // 0xE9 = é in a comment; must round-trip.
        let mut hist_src =
            b"government = monarchy\nprimary_culture = cosmopolitan_french\nadd_accepted_culture = gascon\nadd_government_reform = feudal_france_reform\nreligion = catholic\ntechnology_group = western\ncapital = 183 # Paris caf".to_vec();
        hist_src.push(0xE9);
        hist_src.extend_from_slice(
            b"\nnational_focus = DIP\nhistorical_rival = HAB\n",
        );
        write_base(&base, hist, &hist_src);
        write_base(
            &base,
            cty,
            b"graphical_culture = westerngfx\ncolor = { 20 50 210 }\nrevolutionary_colors = { 15 0 16 }\n",
        );
        let vfs = Vfs::new(base.to_str().unwrap(), None).unwrap();

        let edits = vec![
            // Government type: existing key -> setScalar.
            TypedEdit::SetScalar {
                file: hist.into(),
                path: vec!["government".into()],
                value: "republic".into(),
                quoted: false,
            },
            // Government rank: absent key -> insertStatement.
            TypedEdit::InsertStatement {
                file: hist.into(),
                block_path: vec![],
                statement: "government_rank = 2".into(),
            },
            // Reform add + remove (value-disambiguated).
            TypedEdit::InsertStatement {
                file: hist.into(),
                block_path: vec![],
                statement: "add_government_reform = dutch_republic".into(),
            },
            TypedEdit::RemoveStatement {
                file: hist.into(),
                block_path: vec![],
                key: "add_government_reform".into(),
                value: Some("feudal_france_reform".into()),
            },
            // Accepted culture add + remove.
            TypedEdit::InsertStatement {
                file: hist.into(),
                block_path: vec![],
                statement: "add_accepted_culture = occitain".into(),
            },
            TypedEdit::RemoveStatement {
                file: hist.into(),
                block_path: vec![],
                key: "add_accepted_culture".into(),
                value: Some("gascon".into()),
            },
            // Elector toggle on (absent -> insert).
            TypedEdit::InsertStatement {
                file: hist.into(),
                block_path: vec![],
                statement: "elector = yes".into(),
            },
            // National focus change: present -> setScalar.
            TypedEdit::SetScalar {
                file: hist.into(),
                path: vec!["national_focus".into()],
                value: "ADM".into(),
                quoted: false,
            },
            // Rival remove + friend add.
            TypedEdit::RemoveStatement {
                file: hist.into(),
                block_path: vec![],
                key: "historical_rival".into(),
                value: Some("HAB".into()),
            },
            TypedEdit::InsertStatement {
                file: hist.into(),
                block_path: vec![],
                statement: "historical_friend = ENG".into(),
            },
            // Map color (block replace) + revolutionary colors (block replace).
            TypedEdit::SetBlock {
                file: cty.into(),
                path: vec!["color".into()],
                value: "10 20 30".into(),
            },
            TypedEdit::SetBlock {
                file: cty.into(),
                path: vec!["revolutionary_colors".into()],
                value: "8 1 8".into(),
            },
            // Graphical culture: present -> setScalar.
            TypedEdit::SetScalar {
                file: cty.into(),
                path: vec!["graphical_culture".into()],
                value: "muslimgfx".into(),
                quoted: false,
            },
        ];

        apply_queue(&vfs, &project, &edits).unwrap();

        let h = read_project(&project, hist);
        let htext = String::from_utf8_lossy(&h);
        assert!(htext.contains("government = republic\n"));
        assert!(htext.contains("government_rank = 2\n"));
        assert!(htext.contains("add_government_reform = dutch_republic\n"));
        assert!(!htext.contains("feudal_france_reform"));
        assert!(htext.contains("add_accepted_culture = occitain\n"));
        assert!(!htext.contains("gascon"));
        assert!(htext.contains("elector = yes\n"));
        assert!(htext.contains("national_focus = ADM\n"));
        assert!(!htext.contains("historical_rival"));
        assert!(htext.contains("historical_friend = ENG\n"));
        // Untouched keys + the comment with the high byte survive.
        assert!(htext.contains("primary_culture = cosmopolitan_french\n"));
        assert!(htext.contains("religion = catholic\n"));
        assert!(htext.contains("technology_group = western\n"));
        assert!(htext.contains("capital = 183 # Paris caf"));
        assert!(h.windows(2).any(|w| w == b"\xE9\n"), "high byte preserved");

        let c = String::from_utf8(read_project(&project, cty)).unwrap();
        assert!(c.contains("color = { 10 20 30 }\n"));
        assert!(c.contains("revolutionary_colors = { 8 1 8 }\n"));
        assert!(c.contains("graphical_culture = muslimgfx\n"));
    }

    #[test]
    fn elector_toggle_off_removes_key() {
        let (base, project) = setup("elector_off");
        let hist = "history/countries/BRA - Brandenburg.txt";
        write_base(&base, hist, b"government = monarchy\nelector = yes\ncapital = 50\n");
        let vfs = Vfs::new(base.to_str().unwrap(), None).unwrap();
        let edits = vec![TypedEdit::RemoveStatement {
            file: hist.into(),
            block_path: vec![],
            key: "elector".into(),
            value: None,
        }];
        apply_queue(&vfs, &project, &edits).unwrap();
        let out = String::from_utf8(read_project(&project, hist)).unwrap();
        assert_eq!(out, "government = monarchy\ncapital = 50\n");
    }

    #[test]
    fn province_full_edit_round_trip_byte_identical_elsewhere() {
        // Sprint 2.4 acceptance: on one vanilla-shaped province file, edit dev,
        // add a core, add a dated entry, delete a dated entry, and edit inside a
        // DUPLICATE-date block via its occurrence index. Everything else — the
        // comment, the untouched dated blocks, a Windows-1252 byte — round-trips.
        let (base, project) = setup("prov_full");
        let rel = "history/provinces/151 - Constantinople.txt";
        // 0xE9 = é in a comment; must survive.
        let mut src = b"owner = TUR\ncontroller = TUR\nbase_tax = 8\nadd_core = TUR\n# note caf".to_vec();
        src.push(0xE9);
        src.extend_from_slice(
            b"\n1481.6.1 = { unrest = 6 }\n1482.7.26 = { unrest = 0 }\n1481.6.1 = { unrest = 9 }\n1502.1.1 = { remove_core = BYZ }\n",
        );
        write_base(&base, rel, &src);
        let vfs = Vfs::new(base.to_str().unwrap(), None).unwrap();

        let edits = vec![
            // Dev edit (existing scalar).
            TypedEdit::SetScalar {
                file: rel.into(),
                path: vec!["base_tax".into()],
                value: "10".into(),
                quoted: false,
            },
            // Add a core.
            TypedEdit::InsertStatement {
                file: rel.into(),
                block_path: vec![],
                statement: "add_core = GRE".into(),
            },
            // Add a dated entry (append-only whole block).
            TypedEdit::InsertStatement {
                file: rel.into(),
                block_path: vec![],
                statement: "1600.1.1 = { religion = orthodox }".into(),
            },
            // Delete a dated entry's contents (unique-date block).
            TypedEdit::RemoveStatement {
                file: rel.into(),
                block_path: vec!["1482.7.26".into()],
                key: "unrest".into(),
                value: None,
            },
            // Edit inside the SECOND 1481.6.1 block via occurrence index.
            TypedEdit::SetScalar {
                file: rel.into(),
                path: vec!["1481.6.1#1".into(), "unrest".into()],
                value: "4".into(),
                quoted: false,
            },
        ];
        apply_queue(&vfs, &project, &edits).unwrap();
        let raw = read_project(&project, rel);
        let out = String::from_utf8_lossy(&raw).into_owned();

        assert!(out.contains("base_tax = 10\n"));
        assert!(out.contains("add_core = TUR\n"));
        assert!(out.contains("add_core = GRE\n"));
        assert!(out.contains("1600.1.1 = { religion = orthodox }\n"));
        // First 1481.6.1 untouched; second edited.
        assert!(out.contains("1481.6.1 = { unrest = 6 }\n"));
        assert!(out.contains("1481.6.1 = { unrest = 4 }\n"));
        // 1482.7.26 block kept (now empty inner); the other block intact.
        assert!(out.contains("1502.1.1 = { remove_core = BYZ }\n"));
        // The comment + high byte survived.
        assert!(raw.windows(2).any(|w| w == b"\xE9\n"));
    }

    const INSTALL: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Europa Universalis IV";
    const ANBENNAR: &str = r"C:\Users\User\projects\anbennar-eu4-dev";

    #[test]
    fn anbennar_province_edit_round_trip() {
        if !Path::new(INSTALL).join("map/provinces.bmp").is_file()
            || !Path::new(ANBENNAR).is_dir()
        {
            return;
        }
        // Edit province 1's dev in the Anbennar total conversion; the edit lands
        // in the project copy-on-write, everything else in the file is preserved.
        let project = std::env::temp_dir().join("eu_toolkit_editq_test_anbennar_prov");
        let _ = std::fs::remove_dir_all(&project);
        let vfs = Vfs::new(INSTALL, Some(ANBENNAR)).unwrap();
        // Resolve prov 1's history file through the (base+Anbennar) Vfs.
        let details = crate::province_details::province_details(
            &vfs,
            &crate::loc::build(&vfs),
            1,
        )
        .unwrap();
        if !details.exists {
            return;
        }
        let before = std::fs::read(vfs.resolve(&details.file).unwrap()).unwrap();
        let edits = vec![TypedEdit::SetScalar {
            file: details.file.clone(),
            path: vec!["base_tax".into()],
            value: "7".into(),
            quoted: false,
        }];
        // Only proceed if base_tax exists in the file (else the set would error).
        if !before.windows(9).any(|w| w == b"base_tax ") {
            return;
        }
        apply_queue(&vfs, &project, &edits).unwrap();
        let out = std::fs::read(project.join(&details.file)).unwrap();
        assert!(String::from_utf8_lossy(&out).contains("base_tax = 7"));
        // The base install file is never written.
        assert_eq!(
            std::fs::read(vfs.resolve(&details.file).unwrap()).unwrap(),
            before
        );
    }

    // --- Sprint 11.1 climate painting (two independent slots) ----------------

    #[test]
    fn climate_paint_moves_zone_only_other_slot_untouched() {
        // Province 100 is BOTH `arctic` (zone slot) and `severe_winter` (winter
        // slot). Painting the zone `arid` must only move 100 between zone lists;
        // its winter membership and the monsoon block round-trip byte-for-byte.
        let (base, project) = setup("climate_zone_paint");
        write_base(
            &base,
            "map/climate.txt",
            b"tropical = {\n\t746\n}\narid = {\n\t900\n}\narctic = {\n\t100 200\n}\nsevere_winter = {\n\t100\n}\nmonsoon = {\n\t500 501\n}\n",
        );
        let vfs = Vfs::new(base.to_str().unwrap(), None).unwrap();
        let edits = vec![TypedEdit::ListMove {
            from_file: "map/climate.txt".into(),
            from_path: vec!["arctic".into()],
            to_file: "map/climate.txt".into(),
            to_path: vec!["arid".into()],
            id: "100".into(),
        }];
        apply_queue(&vfs, &project, &edits).unwrap();
        let out = String::from_utf8(read_project(&project, "map/climate.txt")).unwrap();
        assert!(out.contains("arctic = {\n\t200\n}"), "100 left arctic: {out}");
        assert!(out.contains("arid = {\n\t900\n\t100\n}"), "100 joined arid: {out}");
        // Winter slot + monsoon untouched — painting a zone never clobbers them.
        assert!(out.contains("severe_winter = {\n\t100\n}"), "winter slot intact");
        assert!(out.contains("monsoon = {\n\t500 501\n}"), "monsoon intact");
    }

    #[test]
    fn climate_paint_winter_create_list_when_absent() {
        // Province 200 is `arctic` (zone) with no winter membership, and the file
        // has no `mild_winter` list yet. Painting mild winter creates the empty
        // block then adds the id; the zone membership is untouched.
        let (base, project) = setup("climate_winter_create");
        write_base(
            &base,
            "map/climate.txt",
            b"arctic = {\n\t100 200\n}\nsevere_winter = {\n\t100\n}\n",
        );
        let vfs = Vfs::new(base.to_str().unwrap(), None).unwrap();
        // Frontend emits an InsertStatement to create the absent list, then AddId.
        let edits = vec![
            TypedEdit::InsertStatement {
                file: "map/climate.txt".into(),
                block_path: vec![],
                statement: "mild_winter = { }".into(),
            },
            TypedEdit::AddId {
                file: "map/climate.txt".into(),
                list_path: vec!["mild_winter".into()],
                id: "200".into(),
            },
        ];
        apply_queue(&vfs, &project, &edits).unwrap();
        let out = String::from_utf8(read_project(&project, "map/climate.txt")).unwrap();
        assert!(out.contains("mild_winter = { 200 }"), "winter list created + id added: {out}");
        // Zone slot untouched.
        assert!(out.contains("arctic = {\n\t100 200\n}"), "zone slot intact");
        assert!(out.contains("severe_winter = {\n\t100\n}"), "other winter list intact");
    }

    // --- Sprint 11.2 terrain_override painting -------------------------------

    #[test]
    fn terrain_override_paint_and_erase_round_trip() {
        // Move province 96 marsh→mountain (steal between override lists), add a
        // fresh id to mountain, and erase 893 from marsh. Color blocks, the
        // `terrain` palette section, and untouched ids all round-trip.
        let (base, project) = setup("terrain_paint");
        write_base(
            &base,
            "map/terrain.txt",
            b"categories = {\n\tgrasslands = { type = plains movement_cost = 1.0 }\n\tmarsh = { color = { 13 189 130 } type = marsh terrain_override = { 96 893 } }\n\tmountain = { color = { 105 24 4 } type = mountains terrain_override = { 4175 } }\n}\nterrain = {\n\tgrasslands = { type = grasslands color = { 0 } }\n}\n",
        );
        let vfs = Vfs::new(base.to_str().unwrap(), None).unwrap();
        let edits = vec![
            // Steal 96 from marsh into mountain.
            TypedEdit::ListMove {
                from_file: "map/terrain.txt".into(),
                from_path: vec!["categories".into(), "marsh".into(), "terrain_override".into()],
                to_file: "map/terrain.txt".into(),
                to_path: vec!["categories".into(), "mountain".into(), "terrain_override".into()],
                id: "96".into(),
            },
            // Paint a new province into mountain.
            TypedEdit::AddId {
                file: "map/terrain.txt".into(),
                list_path: vec!["categories".into(), "mountain".into(), "terrain_override".into()],
                id: "5".into(),
            },
            // Erase 893 (marsh → reverts to bmp auto class in the app).
            TypedEdit::RemoveId {
                file: "map/terrain.txt".into(),
                list_path: vec!["categories".into(), "marsh".into(), "terrain_override".into()],
                id: "893".into(),
            },
        ];
        apply_queue(&vfs, &project, &edits).unwrap();
        let out = String::from_utf8(read_project(&project, "map/terrain.txt")).unwrap();
        // marsh lost both 96 and 893; mountain gained 96 and 5.
        assert!(out.contains("marsh = { color = { 13 189 130 } type = marsh terrain_override = { } }"), "marsh emptied: {out}");
        assert!(out.contains("terrain_override = { 4175 96 5 }"), "mountain gained ids: {out}");
        // Untouched sections round-trip byte-for-byte.
        assert!(out.contains("grasslands = { type = plains movement_cost = 1.0 }"));
        assert!(out.contains("terrain = {\n\tgrasslands = { type = grasslands color = { 0 } }\n}"));
    }

    #[test]
    fn terrain_override_create_list_when_absent() {
        // grasslands has no terrain_override block. Painting a province onto it
        // creates the nested block then adds the id; sibling keys are preserved.
        let (base, project) = setup("terrain_create");
        write_base(
            &base,
            "map/terrain.txt",
            b"categories = {\n\tgrasslands = { type = plains movement_cost = 1.0 }\n\tmarsh = { type = marsh terrain_override = { 96 } }\n}\n",
        );
        let vfs = Vfs::new(base.to_str().unwrap(), None).unwrap();
        let edits = vec![
            TypedEdit::InsertStatement {
                file: "map/terrain.txt".into(),
                block_path: vec!["categories".into(), "grasslands".into()],
                statement: "terrain_override = { }".into(),
            },
            TypedEdit::AddId {
                file: "map/terrain.txt".into(),
                list_path: vec!["categories".into(), "grasslands".into(), "terrain_override".into()],
                id: "10".into(),
            },
        ];
        apply_queue(&vfs, &project, &edits).unwrap();
        let out = String::from_utf8(read_project(&project, "map/terrain.txt")).unwrap();
        assert!(
            out.contains("grasslands = { type = plains movement_cost = 1.0 terrain_override = { 10 } }"),
            "override block created inline + id added: {out}"
        );
        assert!(out.contains("marsh = { type = marsh terrain_override = { 96 } }"), "sibling intact");
    }

    #[test]
    fn anbennar_climate_paint_round_trips() {
        // Total-conversion smoke: paint an arid-zone id onto a real Anbennar
        // province through its copy-on-write climate.txt; the base file is never
        // written and the edit lands in the project.
        if !Path::new(INSTALL).join("map/provinces.bmp").is_file() || !Path::new(ANBENNAR).is_dir() {
            return;
        }
        let project = std::env::temp_dir().join("eu_toolkit_editq_test_anbennar_climate");
        let _ = std::fs::remove_dir_all(&project);
        let vfs = Vfs::new(INSTALL, Some(ANBENNAR)).unwrap();
        let payload = crate::game_data::climate_payload(&vfs);
        // Need at least one arctic province to move into the arid list.
        let Some(entry) = payload.zones.iter().find(|e| e.key == "arctic") else {
            return;
        };
        // arid must exist as a list to receive the id (vanilla + Anbennar both have it).
        if !payload.existing_lists.iter().any(|k| k == "arid") {
            return;
        }
        let before = std::fs::read(vfs.resolve("map/climate.txt").unwrap()).unwrap();
        let edits = vec![TypedEdit::ListMove {
            from_file: "map/climate.txt".into(),
            from_path: vec!["arctic".into()],
            to_file: "map/climate.txt".into(),
            to_path: vec!["arid".into()],
            id: entry.id.to_string(),
        }];
        apply_queue(&vfs, &project, &edits).unwrap();
        let out = std::fs::read(project.join("map/climate.txt")).unwrap();
        assert_ne!(out, before, "the paint changed the project copy");
        // Base install file never written.
        assert_eq!(std::fs::read(vfs.resolve("map/climate.txt").unwrap()).unwrap(), before);
    }

    #[test]
    fn insert_dated_block_through_queue_is_date_ordered() {
        let (base, project) = setup("dated_insert");
        let rel = "history/provinces/1 - Test.txt";
        write_base(
            &base,
            rel,
            b"owner = FRA\n1450.1.1 = { unrest = 5 }\n1460.1.1 = { unrest = 6 }\n",
        );
        let vfs = Vfs::new(base.to_str().unwrap(), None).unwrap();
        let edits = vec![TypedEdit::InsertDatedBlock {
            file: rel.into(),
            date: "1455.1.1".into(),
            statement: "1455.1.1 = { owner = ENG }".into(),
        }];
        apply_queue(&vfs, &project, &edits).unwrap();
        let out = String::from_utf8(read_project(&project, rel)).unwrap();
        assert_eq!(
            out,
            "owner = FRA\n1450.1.1 = { unrest = 5 }\n1455.1.1 = { owner = ENG }\n1460.1.1 = { unrest = 6 }\n"
        );
    }

    #[test]
    fn edit_at_date_merges_into_existing_dated_block() {
        // Sprint 12.3 edit-at-date: painting owner at a LATER date whose block
        // already exists merges the statement into that block (the frontend
        // `editAtDate` helper emits an InsertStatement targeting ["1450.1.1"]).
        // The top level and every other block round-trip byte-for-byte.
        let (base, project) = setup("edit_at_date_merge");
        let rel = "history/provinces/1 - Test.txt";
        write_base(
            &base,
            rel,
            b"owner = FRA\ncontroller = FRA\n1450.1.1 = { unrest = 5 }\n1460.1.1 = { unrest = 6 }\n",
        );
        let vfs = Vfs::new(base.to_str().unwrap(), None).unwrap();
        let edits = vec![TypedEdit::InsertStatement {
            file: rel.into(),
            block_path: vec!["1450.1.1".into()],
            statement: "owner = ENG".into(),
        }];
        apply_queue(&vfs, &project, &edits).unwrap();
        let out = String::from_utf8(read_project(&project, rel)).unwrap();
        // Merged into the existing 1450 block; top level + the 1460 block intact.
        assert_eq!(
            out,
            "owner = FRA\ncontroller = FRA\n1450.1.1 = { unrest = 5 owner = ENG }\n1460.1.1 = { unrest = 6 }\n"
        );
    }

    #[test]
    fn edit_at_date_inserts_new_block_top_level_untouched() {
        // The other editAtDate branch: painting at a later date with no block for
        // that date inserts a fresh, date-ordered block (the InsertDatedBlock the
        // frontend emits), leaving the top-level base state untouched.
        let (base, project) = setup("edit_at_date_insert");
        let rel = "history/provinces/1 - Test.txt";
        write_base(
            &base,
            rel,
            b"owner = FRA\ncontroller = FRA\nbase_tax = 3\n1460.1.1 = { unrest = 6 }\n",
        );
        let vfs = Vfs::new(base.to_str().unwrap(), None).unwrap();
        let edits = vec![TypedEdit::InsertDatedBlock {
            file: rel.into(),
            date: "1450.1.1".into(),
            statement: "1450.1.1 = { owner = ENG controller = ENG }".into(),
        }];
        apply_queue(&vfs, &project, &edits).unwrap();
        let out = String::from_utf8(read_project(&project, rel)).unwrap();
        assert_eq!(
            out,
            "owner = FRA\ncontroller = FRA\nbase_tax = 3\n1450.1.1 = { owner = ENG controller = ENG }\n1460.1.1 = { unrest = 6 }\n"
        );
    }

    #[test]
    fn timeline_write_at_start_emits_dated_block_in_date_order() {
        // The timeline-mod defect: a province whose top level is the file's
        // BASELINE EPOCH (year 2), not the start state. Writing the owner at the
        // 1302.9.1 start date must emit a dated block after the last pre-start
        // block, leaving the epoch baseline untouched — a top-level write would
        // be overridden by the 1204 block and never reach the player's world.
        // Modeled on Extended Timeline's "167 - Caux.txt".
        let (base, project) = setup("timeline_write_at_start");
        let rel = "history/provinces/167 - Caux.txt";
        write_base(
            &base,
            rel,
            b"#167 - Caux\n\nowner = ROM\ncontroller = ROM\ncapital = \"Rotomagus\"\n\
              1066.12.25 = { owner = ENG controller = ENG }\n\
              1204.6.24 = { owner = FRA controller = FRA }\n\
              1450.1.1 = { unrest = 3 }\n",
        );
        let vfs = Vfs::new(base.to_str().unwrap(), None).unwrap();
        let edits = vec![TypedEdit::InsertDatedBlock {
            file: rel.into(),
            date: "1302.9.1".into(),
            statement: "1302.9.1 = { owner = ENG controller = ENG add_core = ENG }".into(),
        }];
        apply_queue(&vfs, &project, &edits).unwrap();
        let out = String::from_utf8(read_project(&project, rel)).unwrap();
        assert_eq!(
            out,
            "#167 - Caux\n\nowner = ROM\ncontroller = ROM\ncapital = \"Rotomagus\"\n\
              1066.12.25 = { owner = ENG controller = ENG }\n\
              1204.6.24 = { owner = FRA controller = FRA }\n\
              1302.9.1 = { owner = ENG controller = ENG add_core = ENG }\n\
              1450.1.1 = { unrest = 3 }\n"
        );
        // The baseline epoch is untouched, and the effective owner at the start
        // date is now the written value rather than the 1204 block's.
        let block = crate::paradox::parse(&out);
        assert_eq!(block.get_scalar("owner"), Some("ROM"));
        let states = crate::game_data::province_history_at(&vfs2(&project), (1302, 9, 1));
        assert_eq!(states.get(&167).and_then(|s| s.owner.as_deref()), Some("ENG"));
    }

    #[test]
    fn timeline_write_at_start_merges_into_existing_block_for_that_date() {
        // Same shape, but the file already carries a block on the start date:
        // the write merges into it instead of adding a second block.
        let (base, project) = setup("timeline_write_merge");
        let rel = "history/provinces/167 - Caux.txt";
        write_base(
            &base,
            rel,
            b"owner = ROM\n1204.6.24 = { owner = FRA }\n1302.9.1 = { unrest = 2 }\n",
        );
        let vfs = Vfs::new(base.to_str().unwrap(), None).unwrap();
        let edits = vec![TypedEdit::InsertStatement {
            file: rel.into(),
            block_path: vec!["1302.9.1".into()],
            statement: "owner = ENG".into(),
        }];
        apply_queue(&vfs, &project, &edits).unwrap();
        let out = String::from_utf8(read_project(&project, rel)).unwrap();
        assert_eq!(
            out,
            "owner = ROM\n1204.6.24 = { owner = FRA }\n1302.9.1 = { unrest = 2 owner = ENG }\n"
        );
    }

    /// A Vfs rooted at the project folder, for re-deriving state from what was
    /// actually written.
    fn vfs2(project: &std::path::Path) -> Vfs {
        Vfs::new(project.to_str().unwrap(), None).unwrap()
    }

    #[test]
    fn set_define_through_queue_creates_additive_override() {
        // No project defines.lua → an additive override file is written; the base
        // install is never touched.
        let (base, project) = setup("set_define");
        std::fs::create_dir_all(base.join("common")).unwrap();
        std::fs::write(base.join(crate::defines::MAIN_REL), "NDefines = { NGame = { START_DATE = \"1444.11.11\", END_DATE = \"1821.1.2\", } }\n").unwrap();
        let vfs = Vfs::new(base.to_str().unwrap(), None).unwrap();
        let edits = vec![
            TypedEdit::SetDefine { key: "START_DATE".into(), value: "1300.1.1".into(), namespace: None },
            TypedEdit::SetDefine { key: "END_DATE".into(), value: "1850.1.1".into(), namespace: None },
            // Sprint 28: an arbitrary-namespace numeric override rides the same queue.
            TypedEdit::SetDefine { key: "MAX_COLONIAL_NATIONS".into(), value: "120".into(), namespace: Some("NGame".into()) },
        ];
        let written = apply_queue(&vfs, &project, &edits).unwrap();
        assert!(written.contains(&crate::defines::OVERRIDE_REL.to_string()));
        let out = std::fs::read_to_string(project.join(crate::defines::OVERRIDE_REL)).unwrap();
        assert!(out.contains("NDefines.NGame.START_DATE = \"1300.1.1\""));
        assert!(out.contains("NDefines.NGame.END_DATE = \"1850.1.1\""));
        assert!(out.contains("NDefines.NGame.MAX_COLONIAL_NATIONS = 120"));
        // Base install's defines.lua untouched.
        let base_defines = std::fs::read_to_string(base.join(crate::defines::MAIN_REL)).unwrap();
        assert!(base_defines.contains("\"1444.11.11\""));
    }

    #[test]
    fn delete_file_removes_only_project_file() {
        // A toolkit-created war file in the project is deleted; the base install
        // is never touched, and an absent project file is a silent no-op.
        let (base, project) = setup("delete_file");
        // A base file with the same relative path must survive.
        write_base(&base, "history/wars/BaseWar.txt", b"name = \"Base\"\n");
        std::fs::create_dir_all(project.join("history/wars")).unwrap();
        std::fs::write(
            project.join("history/wars/zz_eutoolkit_mywar.txt"),
            b"name = \"Mine\"\n",
        )
        .unwrap();
        let vfs = Vfs::new(base.to_str().unwrap(), None).unwrap();

        let written = apply_queue(
            &vfs,
            &project,
            &[
                TypedEdit::DeleteFile {
                    file: "history/wars/zz_eutoolkit_mywar.txt".into(),
                },
                // Absent in the project → no-op, no error.
                TypedEdit::DeleteFile {
                    file: "history/wars/BaseWar.txt".into(),
                },
            ],
        )
        .unwrap();
        assert!(written.contains(&"history/wars/zz_eutoolkit_mywar.txt".to_string()));
        assert!(!project.join("history/wars/zz_eutoolkit_mywar.txt").exists());
        // Base file untouched (never in the project, never deleted from base).
        assert!(base.join("history/wars/BaseWar.txt").exists());
    }

    #[test]
    fn delete_file_rejects_escaping_path() {
        let (base, project) = setup("delete_escape");
        let vfs = Vfs::new(base.to_str().unwrap(), None).unwrap();
        let err = apply_queue(
            &vfs,
            &project,
            &[TypedEdit::DeleteFile {
                file: "../outside.txt".into(),
            }],
        );
        assert!(err.is_err(), "expected an unsafe-path rejection");
    }

    #[test]
    fn preview_file_folds_pending_edits_in_memory() {
        // preview_file must show the file as it WOULD be after the pending edits,
        // without writing anything — the substrate for parse_script_block_with_edits.
        let (base, _project) = setup("preview");
        let rel = "decisions/Demo.txt";
        write_base(
            &base,
            rel,
            b"country_decisions = {\n\td = {\n\t\tpotential = { tag = ENG }\n\t}\n}\n",
        );
        let vfs = Vfs::new(base.to_str().unwrap(), None).unwrap();
        let edits = vec![
            TypedEdit::SetScalar {
                file: rel.into(),
                path: vec!["country_decisions".into(), "d".into(), "potential".into(), "tag".into()],
                value: "FRA".into(),
                quoted: false,
            },
            // An edit to a DIFFERENT file must be ignored.
            TypedEdit::SetScalar {
                file: "other.txt".into(),
                path: vec!["x".into()],
                value: "1".into(),
                quoted: false,
            },
        ];
        let out = preview_file(&vfs, rel, &edits).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("tag = FRA"));
        assert!(!text.contains("tag = ENG"));
        // The base file on disk was NOT touched.
        assert_eq!(
            std::fs::read(base.join(rel)).unwrap(),
            b"country_decisions = {\n\td = {\n\t\tpotential = { tag = ENG }\n\t}\n}\n"
        );
    }

    #[test]
    fn preview_file_starts_from_empty_for_pending_create() {
        // A pending CreateFile for a not-yet-saved file previews from empty bytes.
        let (base, _project) = setup("preview_create");
        let vfs = Vfs::new(base.to_str().unwrap(), None).unwrap();
        let rel = "decisions/zz_eutoolkit_decisions.txt";
        let edits = vec![
            TypedEdit::CreateFile {
                file: rel.into(),
                text: "country_decisions = {\n}\n".into(),
            },
            TypedEdit::InsertStatement {
                file: rel.into(),
                block_path: vec!["country_decisions".into()],
                statement: "my_dec = {\n\tpotential = { }\n}".into(),
            },
        ];
        let out = String::from_utf8(preview_file(&vfs, rel, &edits).unwrap()).unwrap();
        assert!(out.contains("my_dec = {"));
        assert!(out.contains("country_decisions = {"));
    }

    #[test]
    fn create_group_then_member_composes_same_file() {
        // S2.3/S2.4 create-in-new-group composite: append a brand-new empty group
        // at the top level, then insert the new religion into it. The second edit
        // must find the group in the evolving buffer.
        let (base, project) = setup("group_create_member");
        let rel = "common/religions/00_religion.txt";
        write_base(
            &base,
            rel,
            b"christian = {\n\tdefender_of_faith = yes\n\tcatholic = { color = { 1 2 3 } icon = 1 }\n}\n",
        );
        let vfs = Vfs::new(base.to_str().unwrap(), None).unwrap();
        let scaffold_block = "solar_faiths = {\n\tdefender_of_faith = yes\n\tflag_emblem_index_range = { 1 57 }\n}";
        let member = "sun_worship = {\n\tcolor = { 200 180 40 }\n\ticon = 1\n\theretic = { }\n}";
        let edits = vec![
            TypedEdit::InsertStatement {
                file: rel.into(),
                block_path: vec![],
                statement: scaffold_block.into(),
            },
            TypedEdit::InsertStatement {
                file: rel.into(),
                block_path: vec!["solar_faiths".into()],
                statement: member.into(),
            },
        ];
        apply_queue(&vfs, &project, &edits).unwrap();
        let out = read_project(&project, rel);
        let root = crate::paradox::parse(&String::from_utf8_lossy(&out));
        // The original group survives.
        assert!(root.get_block("christian").is_some());
        // The new group exists with its copied default and the member nested INSIDE it.
        let g = root.get_block("solar_faiths").expect("new group present");
        assert_eq!(g.get_scalar("defender_of_faith"), Some("yes"));
        let m = g.get_block("sun_worship").expect("member nested in new group");
        assert_eq!(m.get_scalar("icon"), Some("1"));
        // The member is NOT at the top level.
        assert!(root.get_block("sun_worship").is_none());
    }

    #[test]
    fn move_existing_member_into_pending_group_composes() {
        // Panel "+ New group" move composite: append the new group, remove the
        // culture from its old group, insert its faithful block into the new group.
        let (base, project) = setup("group_move_pending");
        let rel = "common/cultures/00_cultures.txt";
        write_base(
            &base,
            rel,
            b"germanic = {\n\tgraphical_culture = westerngfx\n\tprussian = { primary = PRU }\n}\n",
        );
        let vfs = Vfs::new(base.to_str().unwrap(), None).unwrap();
        let new_group = "sky_people = {\n\tgraphical_culture = muslimgfx\n\tmale_names = { Aldric Berin }\n}";
        let member = "prussian = { primary = PRU }";
        let edits = vec![
            TypedEdit::InsertStatement {
                file: rel.into(),
                block_path: vec![],
                statement: new_group.into(),
            },
            TypedEdit::RemoveStatement {
                file: rel.into(),
                block_path: vec!["germanic".into()],
                key: "prussian".into(),
                value: None,
            },
            TypedEdit::InsertStatement {
                file: rel.into(),
                block_path: vec!["sky_people".into()],
                statement: member.into(),
            },
        ];
        apply_queue(&vfs, &project, &edits).unwrap();
        let out = read_project(&project, rel);
        let root = crate::paradox::parse(&String::from_utf8_lossy(&out));
        // Old group no longer holds the culture.
        let old = root.get_block("germanic").expect("old group present");
        assert!(old.get_block("prussian").is_none(), "moved out of old group");
        // New group now holds it.
        let new = root.get_block("sky_people").expect("new group present");
        assert!(new.get_block("prussian").is_some(), "moved into new group");
        assert_eq!(new.get_scalar("graphical_culture"), Some("muslimgfx"));
    }

    // --- S3.1 superregion + continent create/delete/rename through the queue ---

    #[test]
    fn create_superregion_through_queue_round_trips() {
        // Mirror the RegionPanel "+ Create Superregion" composite: steal the region
        // out of its old superregion, append the new block, add its loc override.
        let (base, project) = setup("create_superregion");
        write_base(
            &base,
            "map/superregion.txt",
            b"old_super = {\n\tx_region\n}\n",
        );
        let vfs = Vfs::new(base.to_str().unwrap(), None).unwrap();
        let edits = vec![
            TypedEdit::RemoveId {
                file: "map/superregion.txt".into(),
                list_path: vec!["old_super".into()],
                id: "x_region".into(),
            },
            TypedEdit::AppendText {
                file: "map/superregion.txt".into(),
                text: "new_super = {\n\tx_region\n}".into(),
            },
            TypedEdit::LocOverride {
                key: "new_super".into(),
                value: "New Super".into(),
            },
        ];
        apply_queue(&vfs, &project, &edits).unwrap();
        let out = String::from_utf8(read_project(&project, "map/superregion.txt")).unwrap();
        // x_region left old_super (now empty) and lives in new_super.
        assert!(out.contains("old_super = {\n\n}") || out.contains("old_super = {\n}"), "old emptied: {out}");
        let root = crate::paradox::parse(&out);
        assert_eq!(
            root.get_block("new_super").unwrap().bare_scalars().collect::<Vec<_>>(),
            vec!["x_region"]
        );
        let loc = String::from_utf8(read_project(&project, loc::OVERRIDE_REL)).unwrap();
        assert!(loc.contains("new_super:0 \"New Super\""));
    }

    #[test]
    fn create_continent_through_queue_round_trips() {
        // Mirror the province-panel create-continent composite: steal the province
        // from its old continent, append an EMPTY continent block, AddId it in.
        let (base, project) = setup("create_continent");
        write_base(
            &base,
            "map/continent.txt",
            b"europe = {\n\t1 2 3\n}\nnew_world = {\n}\n",
        );
        let vfs = Vfs::new(base.to_str().unwrap(), None).unwrap();
        let edits = vec![
            TypedEdit::RemoveId {
                file: "map/continent.txt".into(),
                list_path: vec!["europe".into()],
                id: "2".into(),
            },
            TypedEdit::AppendText {
                file: "map/continent.txt".into(),
                text: "aelantir = {\n\t\n}".into(),
            },
            TypedEdit::AddId {
                file: "map/continent.txt".into(),
                list_path: vec!["aelantir".into()],
                id: "2".into(),
            },
            TypedEdit::LocOverride {
                key: "aelantir".into(),
                value: "Aelantir".into(),
            },
        ];
        apply_queue(&vfs, &project, &edits).unwrap();
        let out = String::from_utf8(read_project(&project, "map/continent.txt")).unwrap();
        let root = crate::paradox::parse(&out);
        assert_eq!(root.get_block("europe").unwrap().bare_ids(), vec![1, 3]);
        assert_eq!(root.get_block("aelantir").unwrap().bare_ids(), vec![2]);
        // new_world (empty) round-trips untouched.
        assert!(out.contains("new_world = {\n}"));
    }

    #[test]
    fn blank_world_full_geo_tree_from_empty_files_parses() {
        // 18.3 blank-world smoke: build the whole area→region→superregion tree and
        // a continent from EMPTY project files (no base map files at all). Each
        // create appends a scaffold; the outputs must parse back correctly.
        let (base, project) = setup("blank_world_geo");
        // Base has ONLY provinces.bmp (a valid but otherwise-empty install).
        let vfs = Vfs::new(base.to_str().unwrap(), None).unwrap();
        let edits = vec![
            // area with two provinces
            TypedEdit::AppendText {
                file: "map/area.txt".into(),
                text: "new_area = {\n\t10 11\n}".into(),
            },
            TypedEdit::LocOverride { key: "new_area".into(), value: "New Area".into() },
            // region containing the area
            TypedEdit::AppendText {
                file: "map/region.txt".into(),
                text: "new_region = {\n\tareas = {\n\t\tnew_area\n\t}\n}".into(),
            },
            TypedEdit::LocOverride { key: "new_region".into(), value: "New Region".into() },
            // superregion containing the region
            TypedEdit::AppendText {
                file: "map/superregion.txt".into(),
                text: "new_super = {\n\tnew_region\n}".into(),
            },
            TypedEdit::LocOverride { key: "new_super".into(), value: "New Super".into() },
            // continent (empty block + AddId)
            TypedEdit::AppendText {
                file: "map/continent.txt".into(),
                text: "new_continent = {\n\t\n}".into(),
            },
            TypedEdit::AddId {
                file: "map/continent.txt".into(),
                list_path: vec!["new_continent".into()],
                id: "10".into(),
            },
            TypedEdit::LocOverride { key: "new_continent".into(), value: "New Continent".into() },
        ];
        apply_queue(&vfs, &project, &edits).unwrap();

        // Load the resulting tree back through the geography loader over a
        // base+project overlay (the project's new map files shadow the empty base).
        let pvfs = Vfs::new(base.to_str().unwrap(), Some(project.to_str().unwrap())).unwrap();
        let loc = crate::loc::build(&pvfs);
        let net = crate::geography::load_network(&pvfs, &loc);
        let a = net.areas.iter().find(|a| a.key == "new_area").expect("area");
        assert_eq!(a.provinces, vec![10, 11]);
        assert_eq!(a.region.as_deref(), Some("new_region"));
        let r = net.regions.iter().find(|r| r.key == "new_region").expect("region");
        assert_eq!(r.areas, vec!["new_area"]);
        assert_eq!(r.superregion.as_deref(), Some("new_super"));
        assert_eq!(net.superregions.iter().find(|s| s.key == "new_super").unwrap().regions, vec!["new_region"]);
        // Continent parses as a bare-id list with the added province.
        let cont = crate::paradox::parse(&String::from_utf8_lossy(&read_project(&project, "map/continent.txt")));
        assert_eq!(cont.get_block("new_continent").unwrap().bare_ids(), vec![10]);
    }

    #[test]
    fn deserializes_camel_case_from_json() {
        // Proves the wire shape the frontend serializes matches the enum.
        let json = r#"[
            {"kind":"setScalar","file":"a.txt","path":["base_tax"],"value":"5","quoted":false},
            {"kind":"setBlock","file":"c.txt","path":["color"],"value":"1 2 3"},
            {"kind":"listMove","fromFile":"m.txt","fromPath":["a"],"toFile":"m.txt","toPath":["b"],"id":"7"},
            {"kind":"renameRuler","tag":"FRA","name":"Bob"},
            {"kind":"locOverride","key":"FRA","value":"France"},
            {"kind":"binaryAsset","file":"f.tga","bytes":[1,2,3]},
            {"kind":"insertDatedBlock","file":"p.txt","date":"1450.1.1","statement":"1450.1.1 = { owner = ENG }"},
            {"kind":"setDefine","key":"START_DATE","value":"1300.1.1"},
            {"kind":"deleteFile","file":"history/wars/zz_eutoolkit_x.txt"}
        ]"#;
        let parsed: Vec<TypedEdit> = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.len(), 9);
        assert!(matches!(parsed[0], TypedEdit::SetScalar { .. }));
        assert!(matches!(parsed[1], TypedEdit::SetBlock { .. }));
        assert!(matches!(parsed[2], TypedEdit::ListMove { .. }));
        assert!(matches!(parsed[3], TypedEdit::RenameRuler { .. }));
        assert!(matches!(parsed[4], TypedEdit::LocOverride { .. }));
        assert!(matches!(parsed[5], TypedEdit::BinaryAsset { .. }));
        assert!(matches!(parsed[6], TypedEdit::InsertDatedBlock { .. }));
        assert!(matches!(parsed[7], TypedEdit::SetDefine { .. }));
        assert!(matches!(parsed[8], TypedEdit::DeleteFile { .. }));
    }
}
