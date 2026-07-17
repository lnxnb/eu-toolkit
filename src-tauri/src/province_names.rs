//! Dynamic province names (Sprint 24): the `common/province_names/<key>.txt`
//! files that rename provinces per culture, culture group, or country TAG.
//!
//! **File reality (verified against vanilla + Anbennar):** each file is keyed by
//! its stem (`swedish.txt` = culture, `latin.txt`/`germanic.txt` = culture group,
//! `ROM.txt`/`BPI.txt` = country TAG). Entries are top-level `<id> = value` where
//! value is either a single quoted string (`236 = "Londra"`) or the capital-pair
//! variant `<id> = { "Name" "Capital" }` (`4775 = { "Lippe" "Detmold" }`). The
//! strings are LITERAL Windows-1252 text, never loc keys, and may carry trailing
//! `# comments` (Anbennar uses them heavily).
//!
//! Reading is byte-faithful (each byte mapped 1:1 to a `char`, matching
//! `game_data::pool_names`) so accented names round-trip on display; writing is
//! byte-surgical through `mod_writer` (the caller emits `SetScalar`/`SetBlock`/
//! `InsertStatement`/`RemoveStatement`/`CreateFile`).

use std::collections::HashSet;

use crate::loc::LocStore;
use crate::vfs::Vfs;

/// One `<id> = ...` rename entry.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ProvinceNameEntry {
    pub id: u32,
    /// The province name (first string of the pair variant).
    pub name: String,
    /// The capital-city name, present only for the `{ "Name" "Capital" }` variant.
    pub capital: Option<String>,
}

/// The rename list for one key (culture/group/tag), plus where it lives and
/// whether the file already exists (drives create-vs-append on the first edit).
#[derive(Debug, serde::Serialize)]
pub struct ProvinceNamesFile {
    pub key: String,
    /// Game-relative path edits target (the real file if present, else the
    /// canonical `common/province_names/<key>.txt`).
    pub source_file: String,
    pub exists: bool,
    pub entries: Vec<ProvinceNameEntry>,
}

/// One assignment of a name to a given province, for the province-panel reverse
/// view ("renamed by N cultures").
#[derive(Debug, serde::Serialize)]
pub struct ProvinceNameAssignment {
    /// The file's key (culture/group/tag).
    pub key: String,
    /// Localized label for `key` (falls back to the raw key).
    pub label: String,
    /// "culture" | "group" | "tag".
    pub kind: String,
    pub name: String,
    pub capital: Option<String>,
    pub source_file: String,
}

// --- Byte-faithful tokenizer/parser (mirrors game_data::pool_names) ----------

enum PTok {
    /// A bare or quoted word; contents decoded 1:1 from Windows-1252 bytes.
    Word(String),
    Eq,
    Open,
    Close,
}

fn tokenize(bytes: &[u8]) -> Vec<PTok> {
    let mut out = Vec::new();
    let n = bytes.len();
    let mut i = 0;
    while i < n {
        let c = bytes[i];
        match c {
            b'#' => {
                while i < n && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'{' => {
                out.push(PTok::Open);
                i += 1;
            }
            b'}' => {
                out.push(PTok::Close);
                i += 1;
            }
            b'=' => {
                out.push(PTok::Eq);
                i += 1;
            }
            b'"' => {
                i += 1;
                let start = i;
                while i < n && bytes[i] != b'"' {
                    i += 1;
                }
                out.push(PTok::Word(bytes[start..i].iter().map(|&b| b as char).collect()));
                if i < n {
                    i += 1; // closing quote
                }
            }
            _ if c.is_ascii_whitespace() => i += 1,
            _ => {
                let start = i;
                while i < n
                    && !bytes[i].is_ascii_whitespace()
                    && !matches!(bytes[i], b'{' | b'}' | b'=' | b'#' | b'"')
                {
                    i += 1;
                }
                out.push(PTok::Word(bytes[start..i].iter().map(|&b| b as char).collect()));
            }
        }
    }
    out
}

/// Parses `<id> = "Name"` / `<id> = { "Name" "Capital" }` entries in file order.
/// Malformed / non-numeric ids are skipped rather than erroring (preserve-unknown
/// mods carry oddities; only real entries are surfaced).
pub fn parse_entries(bytes: &[u8]) -> Vec<ProvinceNameEntry> {
    let toks = tokenize(bytes);
    let mut out = Vec::new();
    let mut i = 0;
    while i < toks.len() {
        // Expect: Word(id) Eq value
        let PTok::Word(id_s) = &toks[i] else {
            i += 1;
            continue;
        };
        if i + 2 >= toks.len() || !matches!(toks[i + 1], PTok::Eq) {
            i += 1;
            continue;
        }
        let id = id_s.parse::<u32>().ok();
        match &toks[i + 2] {
            PTok::Open => {
                // Pair variant: collect the words up to the closing brace.
                let mut words = Vec::new();
                let mut j = i + 3;
                while j < toks.len() && !matches!(toks[j], PTok::Close) {
                    if let PTok::Word(w) = &toks[j] {
                        words.push(w.clone());
                    }
                    j += 1;
                }
                if let Some(id) = id {
                    out.push(ProvinceNameEntry {
                        id,
                        name: words.first().cloned().unwrap_or_default(),
                        capital: words.get(1).cloned(),
                    });
                }
                i = if j < toks.len() { j + 1 } else { j };
            }
            PTok::Word(v) => {
                if let Some(id) = id {
                    out.push(ProvinceNameEntry {
                        id,
                        name: v.clone(),
                        capital: None,
                    });
                }
                i += 3;
            }
            _ => i += 1,
        }
    }
    out
}

// --- File resolution ---------------------------------------------------------

/// Resolves the `<key>.txt` file: (game-relative path, bytes-if-present). The
/// real filename is used when a file exists (case-insensitive stem match, for
/// clean diffs against TAG files whose stem is uppercase); otherwise the
/// canonical lowercase-extension path is synthesized for a first-edit scaffold.
fn resolve_file(vfs: &Vfs, key: &str) -> (String, Option<Vec<u8>>) {
    for (fname, path) in vfs.list_dir("common/province_names") {
        let Some(stem) = fname.strip_suffix(".txt").or_else(|| fname.strip_suffix(".TXT")) else {
            continue;
        };
        if stem.eq_ignore_ascii_case(key) {
            if let Ok(bytes) = std::fs::read(&path) {
                return (format!("common/province_names/{fname}"), Some(bytes));
            }
        }
    }
    (format!("common/province_names/{key}.txt"), None)
}

/// The rename list for one key. Missing file → `exists: false`, empty entries.
pub fn province_names_for(vfs: &Vfs, key: &str) -> ProvinceNamesFile {
    let (source_file, bytes) = resolve_file(vfs, key);
    let exists = bytes.is_some();
    let entries = bytes.as_deref().map(parse_entries).unwrap_or_default();
    ProvinceNamesFile {
        key: key.to_string(),
        source_file,
        exists,
        entries,
    }
}

// --- Reverse lookup ----------------------------------------------------------

/// Every province_names file that assigns a name to `id`, classified by key kind
/// (culture / culture group / country tag). Scans all files once; culture and
/// group keys are learned from `common/cultures` so mod content classifies too.
pub fn assignments_for(vfs: &Vfs, loc: &LocStore, id: u32) -> Vec<ProvinceNameAssignment> {
    // Learn culture and group keys from the merged culture tree (covers mods).
    let cultures = crate::game_data::culture_list(vfs, loc);
    let culture_keys: HashSet<String> = cultures.iter().map(|e| e.key.clone()).collect();
    let group_keys: HashSet<String> = cultures.iter().map(|e| e.group.clone()).collect();

    let mut out = Vec::new();
    for (fname, path) in vfs.list_dir("common/province_names") {
        let Some(stem) = fname.strip_suffix(".txt").or_else(|| fname.strip_suffix(".TXT")) else {
            continue;
        };
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let Some(entry) = parse_entries(&bytes).into_iter().find(|e| e.id == id) else {
            continue;
        };
        let kind = if culture_keys.contains(stem) {
            "culture"
        } else if group_keys.contains(stem) {
            "group"
        } else {
            "tag"
        };
        out.push(ProvinceNameAssignment {
            label: loc.resolve(stem),
            key: stem.to_string(),
            kind: kind.to_string(),
            name: entry.name,
            capital: entry.capital,
            source_file: format!("common/province_names/{fname}"),
        });
    }
    // Stable, readable order: cultures, then groups, then tags, each by key.
    out.sort_by(|a, b| {
        let rank = |k: &str| match k {
            "culture" => 0,
            "group" => 1,
            _ => 2,
        };
        rank(&a.kind)
            .cmp(&rank(&b.kind))
            .then_with(|| a.key.cmp(&b.key))
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mod_writer::{apply, apply_all, Edit};
    use std::path::Path;

    const INSTALL: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Europa Universalis IV";
    const ANBENNAR: &str = r"C:\Users\User\projects\anbennar-eu4-dev";

    fn real_install() -> Option<Vfs> {
        Path::new(INSTALL)
            .join("map")
            .join("provinces.bmp")
            .is_file()
            .then(|| Vfs::new(INSTALL, None).unwrap())
    }

    const SINGLE_AND_PAIR: &[u8] = b"# a comment\n236 = \"Londra\"\n4775 = { \"Lippe\" \"Detmold\" }\n183 = \"Paris\" # capital\n";

    #[test]
    fn parses_single_and_pair_variants() {
        let e = parse_entries(SINGLE_AND_PAIR);
        assert_eq!(
            e,
            vec![
                ProvinceNameEntry { id: 236, name: "Londra".into(), capital: None },
                ProvinceNameEntry { id: 4775, name: "Lippe".into(), capital: Some("Detmold".into()) },
                ProvinceNameEntry { id: 183, name: "Paris".into(), capital: None },
            ]
        );
    }

    #[test]
    fn decodes_latin1_faithfully() {
        // 0xE5 = å in Windows-1252.
        let src = b"6 = \"Sk\xE5ne\"\n";
        let e = parse_entries(src);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].name, "Sk\u{e5}ne");
    }

    #[test]
    fn skips_inline_comment_bytes() {
        // 0xE9 = é. Anbennar-style trailing comment must not leak into the name.
        let src = b"2151 = \"Qaw\xE9qmayt\xE9\"\t#Watcher\n";
        let e = parse_entries(src);
        assert_eq!(e[0].id, 2151);
        assert_eq!(e[0].name, "Qaw\u{e9}qmayt\u{e9}");
    }

    // --- Byte-surgical round-trips (the writer path the frontend emits) ------

    /// Editing one single-variant name replaces only that value; every other
    /// byte — including a non-ASCII name in an untouched entry — is identical.
    #[test]
    fn edit_single_name_round_trip_keeps_other_bytes() {
        // Second entry carries a Windows-1252 å (0xE5) and must round-trip byte-identical.
        let mut src = b"236 = \"Londra\"\n6 = \"Sk".to_vec();
        src.push(0xE5);
        src.extend_from_slice(b"ne\"\n");
        let out = apply(
            &src,
            &Edit::SetScalar {
                path: vec!["236".into()],
                value: "Londres".into(),
                quoted: true,
            },
        )
        .unwrap();
        assert_eq!(&out, b"236 = \"Londres\"\n6 = \"Sk\xE5ne\"\n");
        let e = parse_entries(&out);
        assert_eq!(e[0], ProvinceNameEntry { id: 236, name: "Londres".into(), capital: None });
        // The untouched non-ASCII entry survived.
        assert_eq!(e[1].name, "Sk\u{e5}ne");
    }

    /// A new name with an accented char is written as Latin-1 (0xE9), never UTF-8.
    #[test]
    fn edit_name_to_non_ascii_encodes_latin1() {
        let src = b"183 = \"Paris\"\n";
        let out = apply(
            src,
            &Edit::SetScalar {
                path: vec!["183".into()],
                value: "Par\u{ed}s".into(), // í = 0xED
                quoted: true,
            },
        )
        .unwrap();
        assert_eq!(out, b"183 = \"Par\xEDs\"\n");
        assert_eq!(parse_entries(&out)[0].name, "Par\u{ed}s");
    }

    #[test]
    fn add_entry_round_trip() {
        let src = b"236 = \"Londra\"\n";
        let out = apply(
            src,
            &Edit::InsertStatement {
                block_path: vec![],
                statement: "183 = \"Parigi\"".into(),
            },
        )
        .unwrap();
        let e = parse_entries(&out);
        assert_eq!(e.len(), 2);
        assert_eq!(e[1], ProvinceNameEntry { id: 183, name: "Parigi".into(), capital: None });
    }

    #[test]
    fn remove_entry_round_trip() {
        let src = b"236 = \"Londra\"\n183 = \"Parigi\"\n";
        let out = apply(
            src,
            &Edit::RemoveStatement {
                block_path: vec![],
                key: "236".into(),
                value: None,
            },
        )
        .unwrap();
        assert_eq!(out, b"183 = \"Parigi\"\n");
    }

    /// Editing the pair variant rewrites just that `{ ... }` value.
    #[test]
    fn edit_pair_variant_round_trip() {
        let src = b"4775 = { \"Lippe\" \"Detmold\" }\n236 = \"Londra\"\n";
        let out = apply(
            src,
            &Edit::SetBlock {
                path: vec!["4775".into()],
                value: "\"Lippe\" \"Lemgo\"".into(),
            },
        )
        .unwrap();
        assert_eq!(out, b"4775 = { \"Lippe\" \"Lemgo\" }\n236 = \"Londra\"\n");
        let e = parse_entries(&out);
        assert_eq!(e[0], ProvinceNameEntry { id: 4775, name: "Lippe".into(), capital: Some("Lemgo".into()) });
    }

    /// Adding a capital to a single entry (shape change scalar→block) is a
    /// remove + insert; the result parses as the pair variant.
    #[test]
    fn single_to_pair_round_trip() {
        let src = b"236 = \"Londra\"\n";
        let out = apply_all(
            src,
            &[
                Edit::RemoveStatement { block_path: vec![], key: "236".into(), value: None },
                Edit::InsertStatement {
                    block_path: vec![],
                    statement: "236 = { \"Londra\" \"City of London\" }".into(),
                },
            ],
        )
        .unwrap();
        let e = parse_entries(&out);
        assert_eq!(
            e[0],
            ProvinceNameEntry { id: 236, name: "Londra".into(), capital: Some("City of London".into()) }
        );
    }

    /// A brand-new file scaffolded via CreateFile parses back to its entries.
    #[test]
    fn scaffold_new_file_parses() {
        let out = apply(
            &[],
            &Edit::CreateFile {
                text: "236 = \"Londra\"\n4775 = { \"Lippe\" \"Detmold\" }\n".into(),
            },
        )
        .unwrap();
        let e = parse_entries(&out);
        assert_eq!(e.len(), 2);
        assert_eq!(e[1].capital.as_deref(), Some("Detmold"));
    }

    // --- Real vanilla + Anbennar data ---------------------------------------

    #[test]
    fn real_province_names_for_culture_and_tag() {
        let Some(vfs) = real_install() else { return };
        let cornish = province_names_for(&vfs, "cornish");
        assert!(cornish.exists);
        assert_eq!(cornish.source_file, "common/province_names/cornish.txt");
        let e236 = cornish.entries.iter().find(|e| e.id == 236).unwrap();
        assert_eq!(e236.name, "Loundres");
        assert_eq!(e236.capital, None);

        // TAG-keyed file (uppercase stem).
        let rom = province_names_for(&vfs, "ROM");
        assert!(rom.exists);
        assert_eq!(rom.entries.iter().find(|e| e.id == 236).unwrap().name, "Londinium");
    }

    #[test]
    fn real_missing_file_reports_absent() {
        let Some(vfs) = real_install() else { return };
        let none = province_names_for(&vfs, "zzz_not_a_real_key");
        assert!(!none.exists);
        assert!(none.entries.is_empty());
        assert_eq!(none.source_file, "common/province_names/zzz_not_a_real_key.txt");
    }

    /// Province 236 (London) is renamed by many files — assert one culture, one
    /// group, and one tag are classified correctly.
    #[test]
    fn real_reverse_view_classifies_kinds() {
        let Some(vfs) = real_install() else { return };
        let loc = crate::loc::build(&vfs);
        let asg = assignments_for(&vfs, &loc, 236);
        assert!(asg.len() >= 10, "236 renamed by many files, got {}", asg.len());

        let cornish = asg.iter().find(|a| a.key == "cornish").unwrap();
        assert_eq!(cornish.kind, "culture");
        assert_eq!(cornish.name, "Loundres");

        let latin = asg.iter().find(|a| a.key == "latin").unwrap();
        assert_eq!(latin.kind, "group");
        assert_eq!(latin.name, "Londra");

        let rom = asg.iter().find(|a| a.key == "ROM").unwrap();
        assert_eq!(rom.kind, "tag");
        assert_eq!(rom.name, "Londinium");

        // Cultures sort before groups before tags.
        assert_eq!(asg.first().unwrap().kind, "culture");
    }

    #[test]
    fn real_pair_variant_parses() {
        let Some(vfs) = real_install() else { return };
        let breton = province_names_for(&vfs, "breton");
        // 172 = { "Bro Naoned" "Naoned" } in vanilla.
        let e = breton.entries.iter().find(|e| e.id == 172).unwrap();
        assert_eq!(e.name, "Bro Naoned");
        assert_eq!(e.capital.as_deref(), Some("Naoned"));
    }

    #[test]
    fn anbennar_smoke_all_parse_and_one_round_trip() {
        if !Path::new(ANBENNAR).join("descriptor.mod").is_file() {
            return;
        }
        let Some(_) = real_install() else { return };
        let vfs = Vfs::new(INSTALL, Some(ANBENNAR)).unwrap();

        // Every province_names file parses to at least one entry (Anbennar uses
        // them extensively — heavy non-ASCII and trailing comments).
        let mut total = 0usize;
        let mut sample: Option<(String, ProvinceNameEntry)> = None;
        for (fname, path) in vfs.list_dir("common/province_names") {
            if !fname.to_lowercase().ends_with(".txt") {
                continue;
            }
            let bytes = std::fs::read(&path).unwrap();
            let entries = parse_entries(&bytes);
            total += entries.len();
            if sample.is_none() {
                if let Some(e) = entries.into_iter().find(|e| e.capital.is_none()) {
                    sample = Some((format!("common/province_names/{fname}"), e));
                }
            }
        }
        assert!(total > 0, "Anbennar province_names parsed no entries");

        // One byte-surgical round-trip on a real Anbennar entry.
        let (rel, entry) = sample.expect("no single-name entry found");
        let src = vfs.read(&rel).unwrap();
        let out = apply(
            &src,
            &Edit::SetScalar {
                path: vec![entry.id.to_string()],
                value: "Toolkit Test".into(),
                quoted: true,
            },
        )
        .unwrap();
        let re = parse_entries(&out);
        assert_eq!(re.iter().find(|e| e.id == entry.id).unwrap().name, "Toolkit Test");
        // Length delta equals exactly the name-length change (surgical).
        let delta = out.len() as isize - src.len() as isize;
        assert_eq!(delta, "Toolkit Test".len() as isize - entry.name.len() as isize);
    }
}
