//! Sprint 3 — `history/diplomacy` relations: parse, 1444-active filtering, and
//! the addressing a byte-surgical edit/delete needs.
//!
//! ## File format (verified against vanilla + Anbennar)
//! Files under `history/diplomacy/*.txt` are grouped by **region**, not by
//! relation type; every kind is mixed within a file. Each relation is a repeated
//! same-key top-level block:
//! ```text
//! alliance = { first = FRA second = SCO start_date = 1428.7.6 end_date = 1560.1.1 }
//! ```
//! Recognized top-level relation keys:
//!   * `alliance`, `guarantee` (first = guarantor), `royal_marriage`, `warning`
//!   * dependency shortcuts `vassal` / `union` / `march` (subject_type implied by
//!     the key), and the generic
//!     `dependency = { subject_type = "tributary_state" first second start_date end_date }`.
//! Everything else top-level — `hre.txt`'s `emperor`, `celestial_empire.txt`'s
//! `celestial_emperor`, and any dated `YYYY.M.D = { … }` block — is NOT a relation
//! and is skipped. In vanilla every relation carries first, second, start_date and
//! end_date; the only unmodeled key seen is `trade_league` on Hanseatic alliances,
//! preserved verbatim in `raw_extra` (never dropped on write — byte-surgical edits
//! only touch the one scalar they target).
//!
//! ## Missing-date semantics
//! A missing `start_date` means "active from the beginning of time" (lower bound
//! satisfied); a missing `end_date` means "never ends" (upper bound satisfied). A
//! block with no dates at all is therefore always active. `active_at_start` is
//! `start_date ≤ 1444.11.11 ≤ end_date`.
//!
//! ## Addressing (byte-surgical edits — the frontend generates the TypedEdits)
//! Each relation records its `file`, its raw `block_key` (the key as it appears in
//! the file), and `block_index` — the 0-based occurrence of that key among the
//! file's top-level items in file order. That is exactly the `key#n` occurrence
//! addressing `mod_writer::locate_block` (and `remove_statement`, extended this
//! sprint) understands:
//!   * **edit a date** → `SetScalar { file, path: ["<block_key>#<i>", "start_date"|"end_date"], value, quoted: false }`
//!     (both dates are present in every editable vanilla block; if a date key were
//!     absent the frontend inserts it instead).
//!   * **delete** → `RemoveStatement { file, block_path: [], key: "<block_key>#<i>", value: None }`
//!     removes the whole `<block_key>` block, braces and all.
//! Both partners' Diplomacy tabs derive from the same parsed blocks, so an edit or
//! delete from either side hits the one block.
//!
//! ## New relations
//! Appended (`AppendText`) to a single project-owned file,
//! `history/diplomacy/zz_eutoolkit_diplomacy.txt` (see `NEW_RELATION_FILE`) — so
//! we never copy a multi-KB vanilla region file just to add one line, and so it
//! works whether the base is vanilla (the game loads every diplomacy file
//! additively) or a total conversion that `replace_path`s `history/diplomacy`
//! (Anbennar): the new file lands in the project's own diplomacy folder either
//! way, resolved copy-on-write through the [`Vfs`]. New dependency relations use
//! the generic `dependency = { subject_type = "<key>" … }` form (unambiguous for
//! every subject type in `common/subject_types`); alliances/RMs/guarantees/warnings
//! use their plain block key.

use crate::date::{self, Date};
#[cfg(test)]
use crate::date::DEFAULT_START;
use crate::loc::LocStore;
use crate::paradox::{self, Block, Value};
use crate::vfs::Vfs;

/// Project-owned file that new relations are appended to. Loads additively over
/// vanilla and lands inside a `replace_path history/diplomacy` mod's own folder.
#[cfg(test)]
pub const NEW_RELATION_FILE: &str = "history/diplomacy/zz_eutoolkit_diplomacy.txt";

/// An unmodeled `key = value` carried through untouched (preserve-unknown).
#[derive(Debug, Clone, serde::Serialize)]
pub struct RawKv {
    pub key: String,
    pub value: String,
}

/// One diplomatic relation, addressable for byte-surgical edit/delete.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Relation {
    /// Normalized: `alliance` | `guarantee` | `warning` | `royal_marriage` | `dependency`.
    pub relation_type: String,
    /// Subject type for dependencies (`vassal`/`union`/`march`/`tributary_state`/…); `None` otherwise.
    pub subject_type: Option<String>,
    pub first: Option<String>,
    pub second: Option<String>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    /// `start_date ≤ 1444.11.11 ≤ end_date` (missing dates are unbounded).
    pub active_at_start: bool,
    /// Game-relative file the block lives in.
    pub file: String,
    /// The raw top-level key as written (`alliance`/`vassal`/`dependency`/…).
    pub block_key: String,
    /// 0-based occurrence of `block_key` among the file's top-level items.
    pub block_index: usize,
    /// Unmodeled keys inside the block (e.g. `trade_league`).
    pub raw_extra: Vec<RawKv>,
}

/// The top-level keys we treat as relations (everything else is skipped).
const RELATION_KEYS: &[&str] = &[
    "alliance",
    "guarantee",
    "royal_marriage",
    "warning",
    "vassal",
    "union",
    "march",
    "dependency",
];

use date::parse_date;

/// `start ≤ at ≤ end`, treating a missing bound as unbounded.
fn active_at(start: Option<&str>, end: Option<&str>, at: Date) -> bool {
    let lower = start.and_then(parse_date).map_or(true, |s| s <= at);
    let upper = end.and_then(parse_date).map_or(true, |e| at <= e);
    lower && upper
}

/// Normalizes a raw block key to `(relation_type, subject_type)`, or `None` when
/// the key isn't a relation.
fn normalize(block_key: &str, block: &Block) -> Option<(String, Option<String>)> {
    Some(match block_key {
        "alliance" => ("alliance".into(), None),
        "guarantee" => ("guarantee".into(), None),
        "royal_marriage" => ("royal_marriage".into(), None),
        "warning" => ("warning".into(), None),
        "vassal" | "union" | "march" => ("dependency".into(), Some(block_key.to_string())),
        "dependency" => (
            "dependency".into(),
            block.get_scalar("subject_type").map(str::to_string),
        ),
        _ => return None,
    })
}

/// Turns one top-level `block_key = { … }` into a [`Relation`], carrying its file
/// and occurrence index. Returns `None` for non-relation keys.
fn relation_from_block(
    file: &str,
    block_key: &str,
    block_index: usize,
    block: &Block,
    at: Date,
) -> Option<Relation> {
    let (relation_type, subject_type) = normalize(block_key, block)?;
    let first = block.get_scalar("first").map(str::to_string);
    let second = block.get_scalar("second").map(str::to_string);
    let start_date = block.get_scalar("start_date").map(str::to_string);
    let end_date = block.get_scalar("end_date").map(str::to_string);
    let active = active_at(start_date.as_deref(), end_date.as_deref(), at);

    // Preserve-unknown: any scalar key that isn't a modeled field.
    let modeled = ["first", "second", "start_date", "end_date", "subject_type"];
    let raw_extra = block
        .items
        .iter()
        .filter_map(|(k, v)| match (k, v) {
            (Some(k), Value::Scalar(s)) if !modeled.contains(&k.as_str()) => Some(RawKv {
                key: k.clone(),
                value: s.clone(),
            }),
            _ => None,
        })
        .collect();

    Some(Relation {
        relation_type,
        subject_type,
        first,
        second,
        start_date,
        end_date,
        active_at_start: active,
        file: file.to_string(),
        block_key: block_key.to_string(),
        block_index,
        raw_extra,
    })
}

/// Parses one diplomacy file's bytes into its relations, computing per-key
/// occurrence indices in file order (matching `mod_writer` occurrence addressing).
fn relations_in_file(file: &str, bytes: &[u8], at: Date) -> Vec<Relation> {
    let block = paradox::parse(&String::from_utf8_lossy(bytes));
    let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut out = Vec::new();
    for (key, value) in &block.items {
        let (Some(key), Value::Block(b)) = (key, value) else {
            continue;
        };
        if !RELATION_KEYS.contains(&key.as_str()) {
            continue;
        }
        let idx = counts.entry(key.clone()).or_insert(0);
        let block_index = *idx;
        *idx += 1;
        if let Some(rel) = relation_from_block(file, key, block_index, b, at) {
            out.push(rel);
        }
    }
    out
}

/// Every relation in `history/diplomacy`, with `active_at_start` computed at the
/// effective start date. Pre-Sprint-12 signature; used by tests.
#[cfg(test)]
pub fn all_relations(vfs: &Vfs) -> Vec<Relation> {
    all_relations_at(vfs, DEFAULT_START)
}

/// Every relation in `history/diplomacy` (all files, in file order), with the
/// `active_at_start` flag evaluated at `at` (Sprint 12.2). Used by
/// [`get_diplomacy`] (then filtered by tag) and by the validation domain.
pub fn all_relations_at(vfs: &Vfs, at: Date) -> Vec<Relation> {
    let mut out = Vec::new();
    for (name, path) in vfs.list_dir("history/diplomacy") {
        if !name.to_lowercase().ends_with(".txt") {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let rel = format!("history/diplomacy/{name}");
        out.extend(relations_in_file(&rel, &bytes, at));
    }
    out
}

/// All relations involving `tag` (as `first` or `second`).
// Registered by the orchestrator in lib.rs; unused until then.
#[allow(dead_code)]
#[tauri::command]
pub fn get_diplomacy(
    install_path: String,
    mod_path: Option<String>,
    tag: String,
    date: Option<String>,
) -> Result<Vec<Relation>, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    let at = crate::bookmarks::resolve_date(&vfs, date.as_deref())?;
    Ok(all_relations_at(&vfs, at)
        .into_iter()
        .filter(|r| r.first.as_deref() == Some(&tag) || r.second.as_deref() == Some(&tag))
        .collect())
}

// --- Validation domain: "diplomacy" (Sprint 3.4) -------------------------
//
// Whole-game checks over every relation (jump targets are countries):
//   * self-relations (first == second)                       → Error
//   * end_date < start_date                                  → Error
//   * duplicate active same-type same-pair                   → Error
//   * a country with two active overlords                    → Error
//   * an overlord chain that loops                           → Error
//   * an active subject that also holds an active alliance   → Warning (3.4)
// Vanilla is clean of every Error (verified: 172 active relations, 0 self,
// 0 end<start, 0 double-overlord, 0 dup, 0 loop) and ships 8 subject-with-alliance
// warnings, which the game tolerates — so those are surfaced, not errored.

use crate::validation::{JumpTarget, Severity, ValidationIssue};
use std::collections::{HashMap, HashSet};

/// The `diplomacy` validation domain at the effective start date (kept for the
/// existing test call sites); delegates at 1444.11.11.
#[cfg(test)]
pub fn check_diplomacy(vfs: &Vfs, loc: &LocStore) -> Vec<ValidationIssue> {
    check_diplomacy_at(vfs, loc, DEFAULT_START)
}

/// The `diplomacy` validation domain evaluated at `at` (active-relation checks
/// use the selected date). Registered via an adapter in `validation`.
pub fn check_diplomacy_at(vfs: &Vfs, loc: &LocStore, at: Date) -> Vec<ValidationIssue> {
    let rels = all_relations_at(vfs, at);
    let mut issues = Vec::new();

    let name = |tag: &str| {
        let n = loc.resolve_or(tag, tag);
        if n == tag {
            tag.to_string()
        } else {
            format!("{n} ({tag})")
        }
    };
    let pair_label = |a: &str, b: &str| format!("{} and {}", name(a), name(b));

    // 1. Self-relations.
    for r in &rels {
        if let (Some(f), Some(s)) = (&r.first, &r.second) {
            if f == s {
                issues.push(ValidationIssue {
                    severity: Severity::Error,
                    message: format!(
                        "{} relation has the same country on both sides ({})",
                        r.relation_type,
                        name(f)
                    ),
                    jump: Some(JumpTarget::Country(f.clone())),
                });
            }
        }
    }

    // 2. end_date < start_date.
    for r in &rels {
        if let (Some(s), Some(e)) = (
            r.start_date.as_deref().and_then(parse_date),
            r.end_date.as_deref().and_then(parse_date),
        ) {
            if e < s {
                let who = r.first.clone().or_else(|| r.second.clone());
                issues.push(ValidationIssue {
                    severity: Severity::Error,
                    message: format!(
                        "{} relation between {} ends ({}) before it starts ({})",
                        r.relation_type,
                        pair_label(
                            r.first.as_deref().unwrap_or("?"),
                            r.second.as_deref().unwrap_or("?")
                        ),
                        r.end_date.as_deref().unwrap_or("?"),
                        r.start_date.as_deref().unwrap_or("?"),
                    ),
                    jump: who.map(JumpTarget::Country),
                });
            }
        }
    }

    // Active relations drive the remaining checks.
    let active: Vec<&Relation> = rels.iter().filter(|r| r.active_at_start).collect();

    // 3. Duplicate active same-type same-pair (alliances/RMs are unordered).
    let mut seen: HashSet<(String, String, String)> = HashSet::new();
    for r in &active {
        let (Some(f), Some(s)) = (&r.first, &r.second) else {
            continue;
        };
        let (a, b) = if matches!(r.relation_type.as_str(), "alliance" | "royal_marriage") {
            if f <= s {
                (f.clone(), s.clone())
            } else {
                (s.clone(), f.clone())
            }
        } else {
            (f.clone(), s.clone())
        };
        let key = (r.relation_type.clone(), a, b);
        if !seen.insert(key) {
            issues.push(ValidationIssue {
                severity: Severity::Error,
                message: format!(
                    "Duplicate active {} between {}",
                    r.relation_type,
                    pair_label(f, s)
                ),
                jump: Some(JumpTarget::Country(f.clone())),
            });
        }
    }

    // 4. Two active overlords for one country (subject = `second` of a dependency).
    let mut overlords: HashMap<String, Vec<String>> = HashMap::new();
    for r in &active {
        if r.relation_type == "dependency" {
            if let (Some(f), Some(s)) = (&r.first, &r.second) {
                overlords.entry(s.clone()).or_default().push(f.clone());
            }
        }
    }
    let mut multi: Vec<(&String, &Vec<String>)> =
        overlords.iter().filter(|(_, v)| v.len() > 1).collect();
    multi.sort_by(|a, b| a.0.cmp(b.0));
    for (subject, lords) in multi {
        issues.push(ValidationIssue {
            severity: Severity::Error,
            message: format!(
                "{} is a subject of {} overlords at once ({})",
                name(subject),
                lords.len(),
                lords.iter().map(|t| name(t)).collect::<Vec<_>>().join(", ")
            ),
            jump: Some(JumpTarget::Country(subject.clone())),
        });
    }

    // 5. Overlord chain loops. Edge: subject -> its (first) overlord.
    let mut lord_of: HashMap<&str, &str> = HashMap::new();
    for r in &active {
        if r.relation_type == "dependency" {
            if let (Some(f), Some(s)) = (&r.first, &r.second) {
                lord_of.entry(s.as_str()).or_insert(f.as_str());
            }
        }
    }
    let mut loop_reported: HashSet<&str> = HashSet::new();
    for &start in lord_of.keys() {
        // Walk up the chain; a revisit within this walk is a loop.
        let mut seen_walk: HashSet<&str> = HashSet::new();
        let mut node = start;
        while let Some(&up) = lord_of.get(node) {
            if !seen_walk.insert(node) {
                if loop_reported.insert(node) {
                    issues.push(ValidationIssue {
                        severity: Severity::Error,
                        message: format!(
                            "Subject overlord chain loops through {}",
                            name(node)
                        ),
                        jump: Some(JumpTarget::Country(node.to_string())),
                    });
                }
                break;
            }
            node = up;
        }
    }

    // 6. Active subject that also holds an active alliance (game ignores it).
    let subjects: HashSet<&str> = active
        .iter()
        .filter(|r| r.relation_type == "dependency")
        .filter_map(|r| r.second.as_deref())
        .collect();
    let mut warned: HashSet<(String, String)> = HashSet::new();
    for r in &active {
        if r.relation_type != "alliance" {
            continue;
        }
        let (Some(f), Some(s)) = (&r.first, &r.second) else {
            continue;
        };
        let subject_side = if subjects.contains(f.as_str()) {
            Some(f)
        } else if subjects.contains(s.as_str()) {
            Some(s)
        } else {
            None
        };
        if let Some(sub) = subject_side {
            let ordered = if f <= s {
                (f.clone(), s.clone())
            } else {
                (s.clone(), f.clone())
            };
            if warned.insert(ordered) {
                issues.push(ValidationIssue {
                    severity: Severity::Warning,
                    message: format!(
                        "Alliance between {} — {} is a subject, so the game ignores this alliance",
                        pair_label(f, s),
                        name(sub)
                    ),
                    jump: Some(JumpTarget::Country(sub.clone())),
                });
            }
        }
    }

    issues
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    const INSTALL: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Europa Universalis IV";
    const ANBENNAR: &str = r"C:\Users\User\projects\anbennar-eu4-dev";

    /// A synthetic install with the given files. One dir per test.
    fn synthetic(name: &str, files: &[(&str, &str)]) -> (PathBuf, Vfs) {
        let root = std::env::temp_dir().join(format!("eu_toolkit_diplo_test_{name}"));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("map")).unwrap();
        std::fs::create_dir_all(root.join("history/diplomacy")).unwrap();
        std::fs::write(root.join("map/provinces.bmp"), b"x").unwrap();
        for (rel, contents) in files {
            let path = root.join(rel);
            if let Some(p) = path.parent() {
                std::fs::create_dir_all(p).unwrap();
            }
            std::fs::write(path, contents).unwrap();
        }
        let vfs = Vfs::new(root.to_str().unwrap(), None).unwrap();
        (root, vfs)
    }

    fn get(rels: &[Relation], ty: &str, first: &str, second: &str) -> Relation {
        rels.iter()
            .find(|r| {
                r.relation_type == ty
                    && r.first.as_deref() == Some(first)
                    && r.second.as_deref() == Some(second)
            })
            .unwrap_or_else(|| panic!("no {ty} {first}->{second} in {rels:?}"))
            .clone()
    }

    const MIXED: &str = "\
alliance = { first = FRA second = SCO start_date = 1428.7.6 end_date = 1560.1.1 }
royal_marriage = { first = FRA second = CAS start_date = 1440.1.1 end_date = 9999.1.1 }
guarantee = { first = VEN second = KNI start_date = 1444.1.1 end_date = 1522.12.21 }
warning = { first = ENG second = FRA start_date = 1440.1.1 end_date = 1450.1.1 }
vassal = { first = BOH second = GLG start_date = 1444.1.1 end_date = 1526.8.29 }
union = { first = POL second = LIT start_date = 1444.1.1 end_date = 9999.1.1 }
march = { first = ALG second = KBA start_date = 1542.4.1 end_date = 1609.1.1 }
dependency = { subject_type = \"tributary_state\" first = AVA second = KAL start_date = 1444.1.1 end_date = 1502.1.1 }
alliance = { first = HSA second = RIG trade_league = 1 start_date = 1380.1.1 end_date = 1200.1.1 }
";

    #[test]
    fn parses_every_relation_type() {
        let (_r, vfs) = synthetic("types", &[("history/diplomacy/mixed.txt", MIXED)]);
        let rels = all_relations(&vfs);
        assert_eq!(get(&rels, "alliance", "FRA", "SCO").subject_type, None);
        assert_eq!(get(&rels, "royal_marriage", "FRA", "CAS").block_key, "royal_marriage");
        assert_eq!(get(&rels, "guarantee", "VEN", "KNI").block_key, "guarantee");
        assert_eq!(get(&rels, "warning", "ENG", "FRA").relation_type, "warning");
        // Dependency shortcuts normalize to relation_type=dependency + subject_type.
        assert_eq!(get(&rels, "dependency", "BOH", "GLG").subject_type.as_deref(), Some("vassal"));
        assert_eq!(get(&rels, "dependency", "POL", "LIT").subject_type.as_deref(), Some("union"));
        assert_eq!(get(&rels, "dependency", "ALG", "KBA").subject_type.as_deref(), Some("march"));
        // Generic dependency reads its explicit (quoted) subject_type.
        assert_eq!(get(&rels, "dependency", "AVA", "KAL").subject_type.as_deref(), Some("tributary_state"));
    }

    #[test]
    fn active_at_start_and_missing_dates() {
        let (_r, vfs) = synthetic("active", &[("history/diplomacy/mixed.txt", MIXED)]);
        let rels = all_relations(&vfs);
        // FRA-SCO 1428..1560 spans 1444 → active.
        assert!(get(&rels, "alliance", "FRA", "SCO").active_at_start);
        // ALG-KBA march starts 1542 → future, inactive.
        assert!(!get(&rels, "dependency", "ALG", "KBA").active_at_start);
        // HSA-RIG ends 1200 (before start) → inactive (expired).
        assert!(!get(&rels, "alliance", "HSA", "RIG").active_at_start);
        // trade_league is preserved as raw_extra, not lost.
        let hsa = get(&rels, "alliance", "HSA", "RIG");
        assert!(hsa.raw_extra.iter().any(|k| k.key == "trade_league" && k.value == "1"));

        // Missing-date semantics: no start = active from beginning; no end = forever.
        let (_r2, vfs2) = synthetic(
            "nodate",
            &[(
                "history/diplomacy/n.txt",
                "alliance = { first = A second = B }\nalliance = { first = C second = D start_date = 1500.1.1 }\nalliance = { first = E second = F end_date = 1400.1.1 }\n",
            )],
        );
        let r2 = all_relations(&vfs2);
        assert!(get(&r2, "alliance", "A", "B").active_at_start, "no dates = always active");
        assert!(!get(&r2, "alliance", "C", "D").active_at_start, "future start only");
        assert!(!get(&r2, "alliance", "E", "F").active_at_start, "past end only");
    }

    #[test]
    fn active_at_filter_is_date_parameterized() {
        // An alliance spanning 1428..1460: active in that window, not before/after.
        let (_r, vfs) = synthetic(
            "activeat",
            &[(
                "history/diplomacy/d.txt",
                "alliance = { first = FRA second = SCO start_date = 1428.7.6 end_date = 1460.1.1 }\n",
            )],
        );
        // Before it starts (1420): inactive.
        assert!(!all_relations_at(&vfs, (1420, 1, 1))[0].active_at_start);
        // Inside the window (1444.11.11, 1455): active.
        assert!(all_relations_at(&vfs, DEFAULT_START)[0].active_at_start);
        assert!(all_relations_at(&vfs, (1455, 1, 1))[0].active_at_start);
        // After it ends (1470): inactive.
        assert!(!all_relations_at(&vfs, (1470, 1, 1))[0].active_at_start);
    }

    #[test]
    fn block_index_tracks_per_key_occurrence() {
        // Two vassal blocks and a union between them: vassal#0, union#0, vassal#1.
        let src = "\
vassal = { first = A second = B start_date = 1444.1.1 end_date = 1500.1.1 }
union = { first = C second = D start_date = 1444.1.1 end_date = 1500.1.1 }
vassal = { first = E second = F start_date = 1444.1.1 end_date = 1500.1.1 }
";
        let (_r, vfs) = synthetic("blockidx", &[("history/diplomacy/d.txt", src)]);
        let rels = all_relations(&vfs);
        assert_eq!(get(&rels, "dependency", "A", "B").block_index, 0);
        assert_eq!(get(&rels, "dependency", "E", "F").block_index, 1);
        assert_eq!(get(&rels, "dependency", "C", "D").block_index, 0); // union counts separately
    }

    // --- edit / delete round-trips via occurrence addressing -----------------

    use crate::mod_writer::{apply, Edit};

    const DUP: &[u8] = b"\
alliance = { first = FRA second = SCO start_date = 1428.7.6 end_date = 1560.1.1 }
vassal = { first = BOH second = GLG start_date = 1444.1.1 end_date = 1526.8.29 }
alliance = { first = ENG second = POR start_date = 1386.5.9 end_date = 9999.1.1 }
vassal = { first = HAB second = OPL start_date = 1526.8.30 end_date = 1675.11.21 }
";

    #[test]
    fn edit_date_of_nth_block_is_byte_surgical() {
        // Change end_date of the SECOND alliance (alliance#1) only.
        let out = apply(
            DUP,
            &Edit::SetScalar {
                path: vec!["alliance#1".into(), "end_date".into()],
                value: "1500.1.1".into(),
                quoted: false,
            },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("first = ENG second = POR start_date = 1386.5.9 end_date = 1500.1.1"));
        // First alliance and both vassals untouched.
        assert!(text.contains("first = FRA second = SCO start_date = 1428.7.6 end_date = 1560.1.1"));
        assert!(text.contains("first = BOH second = GLG start_date = 1444.1.1 end_date = 1526.8.29"));
        assert!(text.contains("first = HAB second = OPL start_date = 1526.8.30 end_date = 1675.11.21"));
    }

    #[test]
    fn delete_nth_block_removes_only_that_block() {
        // Delete the SECOND vassal (vassal#1); everything else byte-identical.
        let out = apply(
            DUP,
            &Edit::RemoveStatement {
                block_path: vec![],
                key: "vassal#1".into(),
                value: None,
            },
        )
        .unwrap();
        let expected = b"\
alliance = { first = FRA second = SCO start_date = 1428.7.6 end_date = 1560.1.1 }
vassal = { first = BOH second = GLG start_date = 1444.1.1 end_date = 1526.8.29 }
alliance = { first = ENG second = POR start_date = 1386.5.9 end_date = 9999.1.1 }
"
        .to_vec();
        assert_eq!(out, expected);
    }

    #[test]
    fn delete_first_block_bare_key_still_first_match() {
        // Bare key (no #) keeps first-match semantics — deletes alliance#0.
        let out = apply(
            DUP,
            &Edit::RemoveStatement {
                block_path: vec![],
                key: "alliance".into(),
                value: None,
            },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(!text.contains("FRA"));
        assert!(text.contains("first = ENG second = POR"));
    }

    #[test]
    fn append_new_relation_respects_replace_path() {
        // A base install + a mod that replace_paths history/diplomacy (Anbennar
        // shape). Appending a new relation must land in the PROJECT's own file,
        // sourced copy-on-write — never falling through to a base file.
        let root = std::env::temp_dir().join("eu_toolkit_diplo_test_replacepath");
        let base = root.join("base");
        let project = root.join("project");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(base.join("map")).unwrap();
        std::fs::create_dir_all(base.join("history/diplomacy")).unwrap();
        std::fs::write(base.join("map/provinces.bmp"), b"x").unwrap();
        std::fs::write(
            base.join("history/diplomacy/vanilla.txt"),
            "alliance = { first = FRA second = SCO start_date = 1428.7.6 end_date = 1560.1.1 }\n",
        )
        .unwrap();
        // Mod replaces the whole history/diplomacy folder.
        std::fs::create_dir_all(project.join("history/diplomacy")).unwrap();
        std::fs::write(
            project.join("descriptor.mod"),
            "name=\"m\"\nreplace_path=\"history/diplomacy\"\n",
        )
        .unwrap();

        let vfs = Vfs::new(base.to_str().unwrap(), Some(project.to_str().unwrap())).unwrap();
        // The base vanilla.txt must be hidden by replace_path.
        assert!(
            all_relations(&vfs).is_empty(),
            "replace_path should hide base diplomacy"
        );

        // Append a new relation via the same edit path the frontend uses.
        let edits = vec![crate::edits::TypedEdit::AppendText {
            file: NEW_RELATION_FILE.into(),
            text: "alliance = { first = SWE second = DAN start_date = 1444.11.11 end_date = 9999.1.1 }".into(),
        }];
        let written = crate::edits::apply_queue(&vfs, &project, &edits).unwrap();
        assert!(written.contains(&NEW_RELATION_FILE.to_string()));
        // Landed in the project folder, not the base.
        let saved = std::fs::read_to_string(project.join(NEW_RELATION_FILE)).unwrap();
        assert!(saved.contains("first = SWE second = DAN"));
        assert!(!base.join(NEW_RELATION_FILE).exists());

        // Re-reading through the Vfs now sees the appended relation.
        let vfs2 = Vfs::new(base.to_str().unwrap(), Some(project.to_str().unwrap())).unwrap();
        let rels = all_relations(&vfs2);
        assert_eq!(rels.len(), 1);
        assert_eq!(rels[0].block_index, 0);
        assert!(rels[0].active_at_start);
    }

    // --- validation domain ---------------------------------------------------

    fn loc_for(vfs: &Vfs) -> LocStore {
        crate::loc::build(vfs)
    }

    fn has_error(issues: &[ValidationIssue]) -> bool {
        issues.iter().any(|i| i.severity == Severity::Error)
    }

    #[test]
    fn validation_flags_self_relation() {
        let (_r, vfs) = synthetic(
            "self",
            &[(
                "history/diplomacy/d.txt",
                "alliance = { first = FRA second = FRA start_date = 1444.1.1 end_date = 9999.1.1 }\n",
            )],
        );
        let issues = check_diplomacy(&vfs, &loc_for(&vfs));
        assert!(issues.iter().any(|i| i.message.contains("same country on both sides")));
        assert!(has_error(&issues));
    }

    #[test]
    fn validation_flags_end_before_start() {
        let (_r, vfs) = synthetic(
            "endstart",
            &[(
                "history/diplomacy/d.txt",
                "alliance = { first = A second = B start_date = 1500.1.1 end_date = 1400.1.1 }\n",
            )],
        );
        let issues = check_diplomacy(&vfs, &loc_for(&vfs));
        assert!(issues.iter().any(|i| i.message.contains("before it starts")));
    }

    #[test]
    fn validation_flags_two_active_overlords() {
        let (_r, vfs) = synthetic(
            "twolords",
            &[(
                "history/diplomacy/d.txt",
                "vassal = { first = AAA second = SUB start_date = 1444.1.1 end_date = 9999.1.1 }\nvassal = { first = BBB second = SUB start_date = 1444.1.1 end_date = 9999.1.1 }\n",
            )],
        );
        let issues = check_diplomacy(&vfs, &loc_for(&vfs));
        assert!(issues.iter().any(|i| i.message.contains("subject of 2 overlords")));
    }

    #[test]
    fn validation_flags_overlord_loop() {
        // A is B's overlord, B is A's overlord → loop.
        let (_r, vfs) = synthetic(
            "loop",
            &[(
                "history/diplomacy/d.txt",
                "vassal = { first = A second = B start_date = 1444.1.1 end_date = 9999.1.1 }\nvassal = { first = B second = A start_date = 1444.1.1 end_date = 9999.1.1 }\n",
            )],
        );
        let issues = check_diplomacy(&vfs, &loc_for(&vfs));
        assert!(issues.iter().any(|i| i.message.contains("loops")));
    }

    #[test]
    fn validation_flags_duplicate_active_alliance() {
        let (_r, vfs) = synthetic(
            "dup",
            &[(
                "history/diplomacy/d.txt",
                "alliance = { first = A second = B start_date = 1444.1.1 end_date = 9999.1.1 }\nalliance = { first = B second = A start_date = 1440.1.1 end_date = 9999.1.1 }\n",
            )],
        );
        let issues = check_diplomacy(&vfs, &loc_for(&vfs));
        assert!(issues.iter().any(|i| i.message.contains("Duplicate active")));
    }

    #[test]
    fn validation_warns_subject_alliance_not_error() {
        let (_r, vfs) = synthetic(
            "subally",
            &[(
                "history/diplomacy/d.txt",
                "vassal = { first = OVL second = SUB start_date = 1444.1.1 end_date = 9999.1.1 }\nalliance = { first = SUB second = FRA start_date = 1444.1.1 end_date = 9999.1.1 }\n",
            )],
        );
        let issues = check_diplomacy(&vfs, &loc_for(&vfs));
        let w = issues.iter().find(|i| i.message.contains("game ignores this alliance"));
        assert!(w.is_some(), "expected subject-alliance warning: {issues:?}");
        assert_eq!(w.unwrap().severity, Severity::Warning);
    }

    #[test]
    fn validation_clean_fixture_no_issues() {
        let (_r, vfs) = synthetic(
            "clean",
            &[(
                "history/diplomacy/d.txt",
                "alliance = { first = FRA second = SCO start_date = 1428.1.1 end_date = 1560.1.1 }\nvassal = { first = BOH second = GLG start_date = 1444.1.1 end_date = 1526.1.1 }\n",
            )],
        );
        let issues = check_diplomacy(&vfs, &loc_for(&vfs));
        assert!(issues.is_empty(), "expected clean, got {issues:?}");
    }

    // --- real install / Anbennar (no-op when absent) -------------------------

    fn real_vfs() -> Option<Vfs> {
        Path::new(INSTALL)
            .join("map/provinces.bmp")
            .is_file()
            .then(|| Vfs::new(INSTALL, None).unwrap())
    }

    #[test]
    fn real_anglo_portuguese_alliance_active_at_1444() {
        let Some(vfs) = real_vfs() else { return };
        let rels = all_relations(&vfs);
        // The Anglo-Portuguese alliance (ENG-POR, 1373-1701) spans 1444.
        let ep = rels.iter().find(|r| {
            r.relation_type == "alliance"
                && ((r.first.as_deref() == Some("ENG") && r.second.as_deref() == Some("POR"))
                    || (r.first.as_deref() == Some("POR") && r.second.as_deref() == Some("ENG")))
        });
        assert!(ep.is_some(), "expected an ENG-POR alliance in vanilla");
        assert!(ep.unwrap().active_at_start, "ENG-POR alliance should be active at 1444");
        // Every real relation has a resolvable file + non-empty block_key.
        assert!(rels.iter().all(|r| r.file.starts_with("history/diplomacy/") && !r.block_key.is_empty()));
    }

    #[test]
    fn real_expired_medieval_relations_are_filtered() {
        let Some(vfs) = real_vfs() else { return };
        let rels = all_relations(&vfs);
        let active = rels.iter().filter(|r| r.active_at_start).count();
        // There are far more historical relations than active-at-1444 ones.
        assert!(active > 100 && active < rels.len(), "active={active} total={}", rels.len());
        // At least one expired (pre-1444-end) relation exists and is filtered out.
        assert!(rels.iter().any(|r| !r.active_at_start), "expected some expired relations");
    }

    #[test]
    fn real_diplomacy_validation_clean_report() {
        let Some(vfs) = real_vfs() else { return };
        let loc = crate::loc::store(&vfs, INSTALL, None);
        let issues = check_diplomacy(&vfs, &loc);
        let errs = issues.iter().filter(|i| i.severity == Severity::Error).count();
        let warns = issues.iter().filter(|i| i.severity == Severity::Warning).count();
        println!("[validation:diplomacy] {} issues ({errs} error, {warns} warning)", issues.len());
        for i in &issues {
            println!("    {:?}: {}", i.severity, i.message);
        }
        assert_eq!(errs, 0, "vanilla diplomacy produced errors: {issues:?}");
    }

    #[test]
    fn anbennar_diplomacy_parses_and_validates() {
        if !Path::new(INSTALL).join("map/provinces.bmp").is_file() || !Path::new(ANBENNAR).is_dir() {
            return;
        }
        let vfs = Vfs::new(INSTALL, Some(ANBENNAR)).unwrap();
        let loc = crate::loc::store(&vfs, INSTALL, Some(ANBENNAR));
        let rels = all_relations(&vfs);
        let issues = check_diplomacy(&vfs, &loc);
        let errs = issues.iter().filter(|i| i.severity == Severity::Error).count();
        println!(
            "[diplomacy:anbennar] {} relations, {} validation issues ({errs} error)",
            rels.len(),
            issues.len()
        );
        // Anbennar replace_paths history/diplomacy; we must read its custom graph.
        assert!(!rels.is_empty(), "expected Anbennar diplomacy relations");
    }
}
