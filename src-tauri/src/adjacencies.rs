//! `map/adjacencies.csv` — straits & land/canal/lake adjacencies (Sprint 25).
//!
//! Semicolon CSV, **NOT** Clausewitz script (never run `paradox.rs` on it). Header:
//!   `From;To;Type;Through;start_x;start_y;stop_x;stop_y;Comment`
//! Coordinate fields are usually `-1` (auto). A `-1;-1;;-1;…` sentinel row
//! terminates the data. Types (lowercase, as-found): `sea` (straits), `land`,
//! `canal`, `lake`. Comments are literal Windows-1252 text and may contain
//! spaces / dashes / accents (e.g. `Sk\xE5ne-Sjaelland`) and — because the
//! comment is the last field — even embedded `;`.
//!
//! Writing is **line-surgical**: the header, the terminator, any trailing blank
//! lines, and every row whose fields are unchanged round-trip **byte-identical**
//! (their original bytes are re-emitted verbatim); only edited or newly-added
//! rows are re-serialized. Read decodes bytes as Latin-1 and write re-encodes
//! Latin-1, so any single byte 0x00–0xFF round-trips exactly (matching the
//! codebase's "write new-name text as Latin-1" rule). CRLF/LF is preserved as
//! found. This module is the only writer for this file; it never touches the
//! base install (copy-on-write happens in `edits::apply_queue`).

use std::collections::{HashMap, HashSet};

/// One parsed data row. `from`/`to`/`through` are province ids (or `-1`); the
/// coordinate fields default to `-1` (auto). Serialized camelCase for the wire
/// (`from,to,kind,through,startX,startY,stopX,stopY,comment`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdjRow {
    pub from: i64,
    pub to: i64,
    /// The `Type` column: `sea` / `land` / `canal` / `lake` (casing preserved).
    pub kind: String,
    pub through: i64,
    pub start_x: i64,
    pub start_y: i64,
    pub stop_x: i64,
    pub stop_y: i64,
    pub comment: String,
}

/// A desired row for [`rewrite`]: `origin` is the index of the base data row it
/// came from (unchanged origin rows re-emit their exact original bytes); `None`
/// means a brand-new row (always freshly serialized).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RowInput {
    #[serde(default)]
    pub origin: Option<usize>,
    #[serde(flatten)]
    pub row: AdjRow,
}

/// A validation finding (Sprint 25). `row` indexes into the validated row list.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AdjIssue {
    /// `"error"` or `"warning"`.
    pub severity: String,
    pub message: String,
    pub row: usize,
}

// ── Latin-1 codec (byte 0x00–0xFF ⇄ codepoint U+0000–U+00FF) ──────────────────

fn decode_latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| b as char).collect()
}

fn encode_latin1(s: &str) -> Vec<u8> {
    s.chars()
        .map(|c| if (c as u32) <= 0xFF { c as u8 } else { b'?' })
        .collect()
}

// ── Parsing ───────────────────────────────────────────────────────────────────

/// The first `;`-separated field of a line's content, trimmed.
fn field0(content: &[u8]) -> &[u8] {
    let end = content.iter().position(|&b| b == b';').unwrap_or(content.len());
    let mut f = &content[..end];
    while f.first().is_some_and(|b| b.is_ascii_whitespace()) {
        f = &f[1..];
    }
    while f.last().is_some_and(|b| b.is_ascii_whitespace()) {
        f = &f[..f.len() - 1];
    }
    f
}

fn is_blank(content: &[u8]) -> bool {
    content.iter().all(|b| b.is_ascii_whitespace())
}

fn is_terminator(content: &[u8]) -> bool {
    field0(content) == b"-1"
}

fn parse_i64(s: &str, default: i64) -> i64 {
    s.trim().parse::<i64>().unwrap_or(default)
}

/// Parses one data-row line (content only, no newline) into an [`AdjRow`].
/// Returns `None` for blank lines. Splits into at most 9 fields so a comment
/// containing `;` stays whole.
fn parse_row(content: &[u8]) -> Option<AdjRow> {
    if is_blank(content) {
        return None;
    }
    let text = decode_latin1(content);
    let fields: Vec<&str> = text.splitn(9, ';').collect();
    if fields.len() < 4 {
        return None;
    }
    let get = |i: usize| fields.get(i).copied().unwrap_or("");
    Some(AdjRow {
        from: parse_i64(get(0), -1),
        to: parse_i64(get(1), -1),
        kind: get(2).trim().to_string(),
        through: parse_i64(get(3), -1),
        start_x: parse_i64(get(4), -1),
        start_y: parse_i64(get(5), -1),
        stop_x: parse_i64(get(6), -1),
        stop_y: parse_i64(get(7), -1),
        // The comment field keeps its exact text (leading/trailing spaces and any
        // embedded `;`); only trailing CR was already stripped by the line split.
        comment: fields.get(8).copied().unwrap_or("").to_string(),
    })
}

/// Splits `bytes` into physical lines, returning each line's start byte offset
/// and its content (the bytes up to but excluding the terminating `\n`, with a
/// trailing `\r` stripped so field parsing is newline-agnostic).
fn split_lines(bytes: &[u8]) -> Vec<(usize, &[u8])> {
    let mut out = Vec::new();
    let mut start = 0usize;
    for i in 0..bytes.len() {
        if bytes[i] == b'\n' {
            let mut end = i;
            if end > start && bytes[end - 1] == b'\r' {
                end -= 1;
            }
            out.push((start, &bytes[start..end]));
            start = i + 1;
        }
    }
    if start <= bytes.len() {
        // Final line (no trailing newline), or an empty trailing segment.
        if start < bytes.len() || bytes.is_empty() || bytes.last() != Some(&b'\n') {
            let mut end = bytes.len();
            if end > start && bytes[end - 1] == b'\r' {
                end -= 1;
            }
            out.push((start, &bytes[start..end]));
        }
    }
    out
}

/// Parses the data rows of a full-file buffer (header + rows + terminator),
/// in file order. Trailing blank lines and the terminator are not rows.
pub fn parse_rows(bytes: &[u8]) -> Vec<AdjRow> {
    let lines = split_lines(bytes);
    let mut rows = Vec::new();
    for &(_, content) in lines.iter().skip(1) {
        if is_terminator(content) {
            break;
        }
        if let Some(r) = parse_row(content) {
            rows.push(r);
        }
    }
    rows
}

// ── Serialization / line-surgical rewrite ─────────────────────────────────────

fn serialize_row(r: &AdjRow) -> Vec<u8> {
    let text = format!(
        "{};{};{};{};{};{};{};{};{}",
        r.from, r.to, r.kind, r.through, r.start_x, r.start_y, r.stop_x, r.stop_y, r.comment
    );
    encode_latin1(&text)
}

/// Rewrites the CSV: emits the header, then one line per entry in `rows`
/// (unchanged origin rows re-emit their exact original bytes; changed or new
/// rows are freshly serialized), then the terminator + any trailing blank lines
/// verbatim. Deleting a row = omitting it from `rows`; reordering = reordering
/// `rows`. Every untouched byte round-trips identically.
pub fn rewrite(base: &[u8], rows: &[RowInput]) -> Result<Vec<u8>, String> {
    let newline: &[u8] = if base.windows(2).any(|w| w == b"\r\n") {
        b"\r\n"
    } else {
        b"\n"
    };
    let lines = split_lines(base);
    if lines.is_empty() {
        return Err("adjacencies.csv is empty".into());
    }
    let header = lines[0].1;

    // Terminator line index (first `-1;…` row), else end-of-file.
    let term_idx = lines
        .iter()
        .enumerate()
        .skip(1)
        .find(|(_, (_, c))| is_terminator(c))
        .map(|(i, _)| i)
        .unwrap_or(lines.len());

    // Peel trailing blank lines just before the terminator into the suffix, so
    // Anbennar's blank-line-before-terminator round-trips verbatim.
    let mut suffix_line = term_idx;
    while suffix_line > 1 && is_blank(lines[suffix_line - 1].1) {
        suffix_line -= 1;
    }

    // Original data rows (raw bytes + parsed) for byte-identical unchanged rows.
    let mut orig_raw: Vec<&[u8]> = Vec::new();
    let mut orig_parsed: Vec<Option<AdjRow>> = Vec::new();
    for &(_, content) in &lines[1..suffix_line] {
        orig_raw.push(content);
        orig_parsed.push(parse_row(content));
    }

    // Suffix = everything from the first trailing-blank/terminator line to EOF,
    // verbatim (terminator, blank lines, and the final newline-or-not).
    let suffix: &[u8] = if suffix_line < lines.len() {
        &base[lines[suffix_line].0..]
    } else {
        &[]
    };

    let mut out: Vec<u8> = Vec::with_capacity(base.len() + 64);
    out.extend_from_slice(header);
    out.extend_from_slice(newline);
    for r in rows {
        let line: Vec<u8> = match r.origin {
            Some(i) if i < orig_raw.len() && orig_parsed[i].as_ref() == Some(&r.row) => {
                orig_raw[i].to_vec()
            }
            _ => serialize_row(&r.row),
        };
        out.extend_from_slice(&line);
        out.extend_from_slice(newline);
    }
    out.extend_from_slice(suffix);
    Ok(out)
}

// ── Validation ────────────────────────────────────────────────────────────────

/// Validates the effective (folded) row list against the water + coastal-land
/// province sets. Sea straits whose `through` isn't water are errors; sea-strait
/// endpoints that aren't coastal, and any duplicate From/To pair (either
/// direction), are warnings. Non-data sentinel rows (`from == -1`) are skipped.
pub fn validate(rows: &[AdjRow], water: &HashSet<u32>, coastal: &HashSet<u32>) -> Vec<AdjIssue> {
    let is_water = |id: i64| id >= 0 && water.contains(&(id as u32));
    let is_coastal = |id: i64| id >= 0 && coastal.contains(&(id as u32));

    let mut issues = Vec::new();
    let mut seen: HashMap<(i64, i64), usize> = HashMap::new();

    for (i, r) in rows.iter().enumerate() {
        if r.from < 0 && r.to < 0 {
            continue; // sentinel/blank
        }
        if r.kind == "sea" {
            if !is_water(r.through) {
                issues.push(AdjIssue {
                    severity: "error".into(),
                    message: format!(
                        "Sea strait {}→{}: through province {} is not a water province",
                        r.from, r.to, r.through
                    ),
                    row: i,
                });
            }
            for &ep in &[r.from, r.to] {
                if !is_coastal(ep) {
                    issues.push(AdjIssue {
                        severity: "warning".into(),
                        message: format!(
                            "Sea strait {}→{}: endpoint {} is not a coastal land province",
                            r.from, r.to, ep
                        ),
                        row: i,
                    });
                }
            }
        }
        // Duplicate detection ignores direction.
        let key = if r.from <= r.to {
            (r.from, r.to)
        } else {
            (r.to, r.from)
        };
        if let Some(&first) = seen.get(&key) {
            issues.push(AdjIssue {
                severity: "warning".into(),
                message: format!(
                    "Duplicate adjacency {}↔{} (also row {})",
                    r.from, r.to, first
                ),
                row: i,
            });
        } else {
            seen.insert(key, i);
        }
    }
    issues
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEADER: &str = "From;To;Type;Through;start_x;start_y;stop_x;stop_y;Comment";

    fn sample() -> Vec<u8> {
        // Mixed: auto-coord row, explicit-coord row with accented comment, canal,
        // lake, then the vanilla-style terminator (no trailing newline).
        let body = format!(
            "{HEADER}\n\
             333;4559;sea;1300;-1;-1;-1;-1;Majorca-Minorca\n\
             6;12;sea;1258;3008;1633;3000;1630;Sk\u{00e5}ne-Sjaelland\n\
             1268;1258;canal;1775;-1;-1;-1;-1;kiel_canal\n\
             31;4152;lake;4139;3232;1827;3235;1835;Savolax - Kuusamo\n\
             -1;-1;;-1;-1;-1;-1;-1;-1;"
        );
        // Emulate a Windows-1252 file: the å is a single byte 0xE5.
        encode_latin1(&body)
    }

    fn ident_inputs(bytes: &[u8]) -> Vec<RowInput> {
        parse_rows(bytes)
            .into_iter()
            .enumerate()
            .map(|(i, row)| RowInput {
                origin: Some(i),
                row,
            })
            .collect()
    }

    #[test]
    fn parses_all_rows_and_accented_comment() {
        let bytes = sample();
        let rows = parse_rows(&bytes);
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0].from, 333);
        assert_eq!(rows[0].to, 4559);
        assert_eq!(rows[0].kind, "sea");
        assert_eq!(rows[0].through, 1300);
        assert_eq!(rows[1].comment, "Sk\u{00e5}ne-Sjaelland");
        assert_eq!(rows[2].kind, "canal");
        assert_eq!(rows[3].kind, "lake");
        assert_eq!(rows[3].comment, "Savolax - Kuusamo");
    }

    #[test]
    fn identity_rewrite_is_byte_for_byte() {
        let bytes = sample();
        let out = rewrite(&bytes, &ident_inputs(&bytes)).unwrap();
        assert_eq!(out, bytes, "no-op rewrite must reproduce the file exactly");
    }

    #[test]
    fn edit_one_row_touches_only_that_line() {
        let bytes = sample();
        let mut inputs = ident_inputs(&bytes);
        inputs[0].row.through = 9999;
        inputs[0].row.comment = "Edited".into();
        let out = rewrite(&bytes, &inputs).unwrap();
        let reparsed = parse_rows(&out);
        assert_eq!(reparsed[0].through, 9999);
        assert_eq!(reparsed[0].comment, "Edited");
        // Untouched rows still byte-identical (accented comment survives).
        assert_eq!(reparsed[1].comment, "Sk\u{00e5}ne-Sjaelland");
        assert!(out.windows(b"Sk\xe5ne-Sjaelland".len()).any(|w| w == b"Sk\xe5ne-Sjaelland"));
        // The terminator line is preserved verbatim.
        assert!(out.ends_with(b"-1;-1;;-1;-1;-1;-1;-1;-1;"));
    }

    #[test]
    fn add_row_appends_before_terminator() {
        let bytes = sample();
        let mut inputs = ident_inputs(&bytes);
        inputs.push(RowInput {
            origin: None,
            row: AdjRow {
                from: 100,
                to: 200,
                kind: "sea".into(),
                through: 1300,
                start_x: -1,
                start_y: -1,
                stop_x: -1,
                stop_y: -1,
                comment: "New-Strait".into(),
            },
        });
        let out = rewrite(&bytes, &inputs).unwrap();
        let rows = parse_rows(&out);
        assert_eq!(rows.len(), 5);
        assert_eq!(rows[4].from, 100);
        assert_eq!(rows[4].comment, "New-Strait");
        assert!(out.ends_with(b"-1;-1;;-1;-1;-1;-1;-1;-1;"));
        assert!(String::from_utf8_lossy(&out).contains("100;200;sea;1300;-1;-1;-1;-1;New-Strait\n"));
    }

    #[test]
    fn delete_row_drops_only_that_line() {
        let bytes = sample();
        let inputs: Vec<RowInput> = ident_inputs(&bytes)
            .into_iter()
            .filter(|r| r.row.from != 1268) // drop the canal
            .collect();
        let out = rewrite(&bytes, &inputs).unwrap();
        let rows = parse_rows(&out);
        assert_eq!(rows.len(), 3);
        assert!(rows.iter().all(|r| r.kind != "canal"));
        assert!(!String::from_utf8_lossy(&out).contains("kiel_canal"));
    }

    #[test]
    fn crlf_and_trailing_blank_line_round_trip() {
        // Anbennar-shape: CRLF, a blank line just before the terminator.
        let body = format!(
            "{HEADER}\r\n\
             2;276;sea;1272;2848;1531;2854;1527;Eargate-Damescross\r\n\
             \r\n\
             -1;-1;;-1;-1;-1;-1;-1;-1;\r\n"
        );
        let bytes = encode_latin1(&body);
        let rows = parse_rows(&bytes);
        assert_eq!(rows.len(), 1);
        let out = rewrite(&bytes, &ident_inputs(&bytes)).unwrap();
        assert_eq!(out, bytes, "CRLF + trailing blank line must round-trip");
    }

    #[test]
    fn validate_flags_bad_through_endpoints_and_dupes() {
        let water: HashSet<u32> = [1300u32, 1258].into_iter().collect();
        let coastal: HashSet<u32> = [333u32, 4559].into_iter().collect();
        let rows = vec![
            // Good sea strait: through water, both endpoints coastal.
            AdjRow {
                from: 333,
                to: 4559,
                kind: "sea".into(),
                through: 1300,
                start_x: -1,
                start_y: -1,
                stop_x: -1,
                stop_y: -1,
                comment: String::new(),
            },
            // Bad: through 999 is not water (error) + endpoints not coastal (2 warns).
            AdjRow {
                from: 10,
                to: 11,
                kind: "sea".into(),
                through: 999,
                start_x: -1,
                start_y: -1,
                stop_x: -1,
                stop_y: -1,
                comment: String::new(),
            },
            // Duplicate of row 0 (reversed direction) → warning.
            AdjRow {
                from: 4559,
                to: 333,
                kind: "sea".into(),
                through: 1300,
                start_x: -1,
                start_y: -1,
                stop_x: -1,
                stop_y: -1,
                comment: String::new(),
            },
        ];
        let issues = validate(&rows, &water, &coastal);
        assert_eq!(
            issues.iter().filter(|i| i.severity == "error").count(),
            1,
            "one bad-through error"
        );
        assert!(issues.iter().any(|i| i.row == 1 && i.message.contains("not a water")));
        assert_eq!(
            issues.iter().filter(|i| i.row == 1 && i.severity == "warning").count(),
            2,
            "both endpoints of row 1 non-coastal"
        );
        assert!(issues
            .iter()
            .any(|i| i.row == 2 && i.message.contains("Duplicate")));
    }

    // ── Real-file tests (no-op silently when the install/TC is absent) ──────

    const INSTALL: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Europa Universalis IV";
    const ANBENNAR: &str = r"C:\Users\User\projects\anbennar-eu4-dev";

    fn read_via_vfs(base: &str, mod_dir: Option<&str>) -> Option<Vec<u8>> {
        use std::path::Path;
        if !Path::new(base).join("map").join("provinces.bmp").is_file() {
            return None;
        }
        if let Some(m) = mod_dir {
            if !Path::new(m).is_dir() {
                return None;
            }
        }
        let vfs = crate::vfs::Vfs::new(base, mod_dir).unwrap();
        vfs.read("map/adjacencies.csv").ok()
    }

    #[test]
    fn vanilla_full_file_parses_and_round_trips() {
        let Some(bytes) = read_via_vfs(INSTALL, None) else {
            return;
        };
        let rows = parse_rows(&bytes);
        // 110 real data rows in the shipped file (verified by grep).
        assert_eq!(rows.len(), 110, "vanilla adjacency row count");
        // Known strait: Majorca–Minorca, ids 333→4559 through sea province 1300.
        let mm = rows
            .iter()
            .find(|r| r.comment == "Majorca-Minorca")
            .expect("Majorca-Minorca strait present");
        assert_eq!((mm.from, mm.to, mm.kind.as_str(), mm.through), (333, 4559, "sea", 1300));
        // The three named canals are present.
        assert_eq!(rows.iter().filter(|r| r.kind == "canal").count(), 3);
        // Full no-op rewrite is byte-identical to the shipped file.
        let out = rewrite(&bytes, &ident_inputs(&bytes)).unwrap();
        assert_eq!(out, bytes, "vanilla adjacencies.csv must round-trip byte-for-byte");
    }

    #[test]
    fn anbennar_full_file_parses_and_round_trips() {
        let Some(bytes) = read_via_vfs(INSTALL, Some(ANBENNAR)) else {
            return;
        };
        let rows = parse_rows(&bytes);
        assert!(rows.len() > 100, "Anbennar replaces the file with many rows");
        assert!(rows.iter().any(|r| r.kind == "lake"), "Anbennar uses lake adjacencies");
        // Edit one row, everything else byte-identical.
        let mut inputs = ident_inputs(&bytes);
        inputs[0].row.comment = "eutoolkit-smoke".into();
        let out = rewrite(&bytes, &inputs).unwrap();
        assert_eq!(parse_rows(&out)[0].comment, "eutoolkit-smoke");
        assert_eq!(parse_rows(&out).len(), rows.len());
        // Re-emitting with the original comment restores the exact bytes.
        let out2 = rewrite(&bytes, &ident_inputs(&bytes)).unwrap();
        assert_eq!(out2, bytes, "Anbennar adjacencies.csv round-trips byte-for-byte");
    }

    #[test]
    fn row_input_deserializes_from_camelcase_wire() {
        // The frontend sends flattened camelCase JSON; verify serde(flatten) +
        // rename_all round-trips the wire shape the IPC layer actually uses.
        let json = r#"{"origin":3,"from":333,"to":4559,"kind":"sea","through":1300,
            "startX":-1,"startY":-1,"stopX":-1,"stopY":-1,"comment":"Majorca-Minorca"}"#;
        let ri: RowInput = serde_json::from_str(json).expect("wire row deserializes");
        assert_eq!(ri.origin, Some(3));
        assert_eq!(ri.row.from, 333);
        assert_eq!(ri.row.start_x, -1);
        assert_eq!(ri.row.comment, "Majorca-Minorca");
        // A new row (origin omitted) defaults to None.
        let json2 = r#"{"from":1,"to":2,"kind":"land","through":-1,
            "startX":-1,"startY":-1,"stopX":-1,"stopY":-1,"comment":""}"#;
        let ri2: RowInput = serde_json::from_str(json2).expect("new-row wire deserializes");
        assert_eq!(ri2.origin, None);
        assert_eq!(ri2.row.kind, "land");
    }

    #[test]
    fn good_row_has_no_issues() {
        let water: HashSet<u32> = [1300u32].into_iter().collect();
        let coastal: HashSet<u32> = [333u32, 4559].into_iter().collect();
        let rows = vec![AdjRow {
            from: 333,
            to: 4559,
            kind: "sea".into(),
            through: 1300,
            start_x: -1,
            start_y: -1,
            stop_x: -1,
            stop_y: -1,
            comment: "Majorca-Minorca".into(),
        }];
        assert!(validate(&rows, &water, &coastal).is_empty());
    }
}
