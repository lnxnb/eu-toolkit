//! Writes edits back into game files surgically: only the edited byte span
//! changes, so hand-written formatting, comments, and Windows-1252 characters
//! all round-trip untouched (important for git-managed mods).
//!
//! Two layers live here:
//! * A small byte-span toolkit ([`Edit`] + [`apply`]) of pure `&[u8] -> Vec<u8>`
//!   transforms (scalar replace, key/statement add & remove, block insert &
//!   delete, id-list splice, append, whole-file scaffold). Every untouched byte
//!   round-trips; inserted text is encoded as Latin-1, never UTF-8.
//! * A copy-on-write driver (`apply_file_edits`) that resolves a game file
//!   through the [`Vfs`], applies edits, and writes the result into the project
//!   folder — the base install is never written. A typed pending-edit queue is
//!   meant to sit on top of these.

use std::path::Path;

use crate::date::DEFAULT_START;
#[cfg(test)]
use crate::vfs::Vfs;

// ---------------------------------------------------------------------------
// Byte-span tokenizer (shared by the ruler rename and the generic toolkit).
// Every token carries its byte offset(s) so edits can splice precisely and so
// braces can be located for block insert/delete.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
enum Tok {
    /// `{` at this byte offset.
    Open(usize),
    /// `}` at this byte offset.
    Close(usize),
    /// `=` at this byte offset.
    Eq(usize),
    /// Symbol or quoted string; span covers the quotes if present.
    Sym(usize, usize),
}

fn tokenize(src: &[u8]) -> Vec<Tok> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < src.len() {
        match src[i] {
            b'#' => {
                while i < src.len() && src[i] != b'\n' {
                    i += 1;
                }
            }
            b'{' => {
                out.push(Tok::Open(i));
                i += 1;
            }
            b'}' => {
                out.push(Tok::Close(i));
                i += 1;
            }
            b'=' => {
                out.push(Tok::Eq(i));
                i += 1;
            }
            b'"' => {
                let start = i;
                i += 1;
                while i < src.len() && src[i] != b'"' {
                    i += 1;
                }
                i = (i + 1).min(src.len());
                out.push(Tok::Sym(start, i));
            }
            c if c.is_ascii_whitespace() => i += 1,
            _ => {
                let start = i;
                while i < src.len()
                    && !src[i].is_ascii_whitespace()
                    && !matches!(src[i], b'{' | b'}' | b'=' | b'#' | b'"')
                {
                    i += 1;
                }
                out.push(Tok::Sym(start, i));
            }
        }
    }
    out
}

/// Token index just past the block opened at `open_idx` (which must point at
/// the `Tok::Open`).
fn skip_block(toks: &[Tok], open_idx: usize) -> usize {
    let mut depth = 0usize;
    let mut i = open_idx;
    while i < toks.len() {
        match toks[i] {
            Tok::Open(_) => depth += 1,
            Tok::Close(_) => {
                depth -= 1;
                if depth == 0 {
                    return i + 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    toks.len()
}

fn sym_text<'a>(src: &'a [u8], tok: &Tok) -> Option<&'a [u8]> {
    match *tok {
        Tok::Sym(s, e) => Some(&src[s..e]),
        _ => None,
    }
}

fn parse_date(text: &[u8]) -> Option<(u32, u32, u32)> {
    let s = std::str::from_utf8(text).ok()?;
    let mut it = s.split('.');
    let date = (
        it.next()?.parse().ok()?,
        it.next()?.parse().ok()?,
        it.next()?.parse().ok()?,
    );
    it.next().is_none().then_some(date)
}

/// Byte span of the value of `key = <value>` scalars directly inside the
/// block starting at `open_idx`, searching nested `path` of keys.
/// E.g. path = ["monarch", "name"]: finds monarch = { ... name = X ... }.
fn find_value_span(
    src: &[u8],
    toks: &[Tok],
    open_idx: usize,
    path: &[&[u8]],
) -> Option<(usize, usize)> {
    let end = skip_block(toks, open_idx);
    let mut i = open_idx + 1;
    while i + 2 < end {
        if let (Tok::Sym(_, _), Tok::Eq(_)) = (toks[i], toks[i + 1]) {
            let key = sym_text(src, &toks[i])?;
            let key_matches = key.eq_ignore_ascii_case(path[0]);
            match toks[i + 2] {
                Tok::Open(_) => {
                    if key_matches && path.len() > 1 {
                        if let Some(span) = find_value_span(src, toks, i + 2, &path[1..]) {
                            return Some(span);
                        }
                    }
                    i = skip_block(toks, i + 2);
                    continue;
                }
                Tok::Sym(s, e) => {
                    if key_matches && path.len() == 1 {
                        return Some((s, e));
                    }
                    i += 3;
                    continue;
                }
                _ => {}
            }
        }
        i += 1;
    }
    None
}

// ---------------------------------------------------------------------------
// Encoding helpers.
// ---------------------------------------------------------------------------

fn encode_windows_1252(text: &str) -> Vec<u8> {
    // Latin-1 range maps 1:1 to Windows-1252 for our purposes; anything
    // outside becomes '?' rather than corrupting the file.
    text.chars()
        .map(|c| if (c as u32) < 0x100 { c as u8 } else { b'?' })
        .collect()
}

/// Strict Latin-1 encoding for text the toolkit inserts. Unlike
/// [`encode_windows_1252`], this errors on any char outside 0x00-0xFF instead
/// of silently substituting — callers must not smuggle UTF-8 into game files.
fn encode_latin1(text: &str) -> Result<Vec<u8>, String> {
    text.chars()
        .map(|c| {
            let n = c as u32;
            if n < 0x100 {
                Ok(n as u8)
            } else {
                Err(format!(
                    "Character {c:?} (U+{n:04X}) is not encodable as Windows-1252/Latin-1"
                ))
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Ruler rename (original operation; kept working verbatim in behavior).
// ---------------------------------------------------------------------------

/// Renames the starting ruler (the monarch of the latest dated block on or
/// before 1444.11.11) in a country history file. Returns the new file bytes.
pub fn rename_ruler(src: &[u8], new_name: &str) -> Result<Vec<u8>, String> {
    let toks = tokenize(src);
    let mut best: Option<((u32, u32, u32), (usize, usize))> = None;

    let mut i = 0;
    while i + 2 < toks.len() {
        if let (Tok::Sym(_, _), Tok::Eq(_), Tok::Open(_)) = (toks[i], toks[i + 1], toks[i + 2]) {
            let key = sym_text(src, &toks[i]).unwrap_or(b"");
            if let Some(date) = parse_date(key) {
                if date <= DEFAULT_START {
                    if let Some(span) = find_value_span(src, &toks, i + 2, &[b"monarch", b"name"]) {
                        // File order breaks ties, matching the game's behavior.
                        if best.map_or(true, |(d, _)| date >= d) {
                            best = Some((date, span));
                        }
                    }
                }
            }
            i = skip_block(&toks, i + 2);
            continue;
        }
        i += 1;
    }

    let (_, (start, end)) =
        best.ok_or("No starting ruler (monarch before 1444.11.11) found in history file")?;

    let mut out = Vec::with_capacity(src.len() + new_name.len());
    out.extend_from_slice(&src[..start]);
    out.push(b'"');
    out.extend_from_slice(&encode_windows_1252(new_name));
    out.push(b'"');
    out.extend_from_slice(&src[end..]);
    Ok(out)
}

// ---------------------------------------------------------------------------
// Block/statement navigation over the token stream.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
enum ValLoc {
    /// Byte span of a scalar value token.
    Scalar(usize, usize),
    /// (open_tok_idx, close_tok_idx) of a `{ ... }` value.
    Block(usize, usize),
}

/// A single direct child (statement) of a block.
struct Child {
    /// Byte span of the key token, or `None` for a bare list element.
    key: Option<(usize, usize)>,
    val: ValLoc,
    /// First byte of the statement (key start, or value start when bare).
    stmt_start: usize,
    /// One past the last byte of the statement (scalar end, or `}`+1).
    stmt_end: usize,
}

/// The direct children of the block whose contents span token range `[cs, ce)`
/// (exclusive of the block's own braces). Nested blocks are skipped, not
/// descended. Comments produce no tokens, so they never appear as children.
fn children(toks: &[Tok], cs: usize, ce: usize) -> Vec<Child> {
    let mut out = Vec::new();
    let mut i = cs;
    while i < ce {
        match toks[i] {
            Tok::Sym(ks, ke) if i + 1 < ce && matches!(toks[i + 1], Tok::Eq(_)) => {
                if i + 2 >= ce {
                    i += 1;
                    continue;
                }
                match toks[i + 2] {
                    Tok::Open(ob) => {
                        let close_idx = skip_block(toks, i + 2) - 1;
                        let cb = match toks[close_idx] {
                            Tok::Close(b) => b,
                            _ => ob,
                        };
                        out.push(Child {
                            key: Some((ks, ke)),
                            val: ValLoc::Block(i + 2, close_idx),
                            stmt_start: ks,
                            stmt_end: cb + 1,
                        });
                        i = close_idx + 1;
                    }
                    Tok::Sym(vs, ve) => {
                        out.push(Child {
                            key: Some((ks, ke)),
                            val: ValLoc::Scalar(vs, ve),
                            stmt_start: ks,
                            stmt_end: ve,
                        });
                        i += 3;
                    }
                    _ => i += 1,
                }
            }
            Tok::Sym(vs, ve) => {
                out.push(Child {
                    key: None,
                    val: ValLoc::Scalar(vs, ve),
                    stmt_start: vs,
                    stmt_end: ve,
                });
                i += 1;
            }
            Tok::Open(_) => {
                // Anonymous block: skip it whole.
                i = skip_block(toks, i);
            }
            _ => i += 1, // stray `=` / `}`
        }
    }
    out
}

/// A located block: the token range of its contents plus, for a braced block,
/// the byte offsets of its `{` and `}`.
struct Located {
    cs: usize,
    ce: usize,
    /// `(open_byte, close_byte)`; `None` for the whole-file top level.
    braces: Option<(usize, usize)>,
}

// ---------------------------------------------------------------------------
// Occurrence-indexed path addressing (Sprint 2.3).
//
// Duplicate keys are legal — most visibly duplicate DATED blocks in province
// histories (`1481.6.1 = { unrest = 6 }` can appear twice). First-match path
// addressing can't reach the 2nd+ such block, so a path segment may carry an
// optional `#<n>` occurrence suffix (0-based, in file order) that selects the
// nth block child with that key: e.g. `["1481.6.1#1", "unrest"]` reaches the
// `unrest` inside the SECOND `1481.6.1` block. `province_details` surfaces the
// matching `occurrence_index` for each dated block, so the timeline host emits
// occurrence-qualified paths and every dated-block edit stays byte-safe.
//
// `#` starts a comment in Clausewitz script, so no real key ever contains one —
// it is an unambiguous delimiter for toolkit-authored paths. A bare segment
// (no `#`) keeps its old first-match meaning, and `#0` is exactly first-match,
// so existing callers are unaffected.
// ---------------------------------------------------------------------------

/// Splits an optional `#<n>` occurrence suffix off a path segment.
/// `"1500.1.1#2"` -> (`b"1500.1.1"`, `Some(2)`); `"monarch"` -> (`b"monarch"`,
/// `None`). A trailing `#` without a valid number is treated as part of the key
/// (returns `None`), so nothing surprising happens on malformed input.
fn split_occurrence(seg: &[u8]) -> (&[u8], Option<usize>) {
    if let Some(pos) = seg.iter().rposition(|&b| b == b'#') {
        if let Ok(s) = std::str::from_utf8(&seg[pos + 1..]) {
            if let Ok(n) = s.parse::<usize>() {
                return (&seg[..pos], Some(n));
            }
        }
    }
    (seg, None)
}

/// Resolves the block named by `path` (each element names a `key = { ... }`
/// child, optionally occurrence-qualified as `key#n`). An empty path is the
/// whole file (top level).
fn locate_block(src: &[u8], toks: &[Tok], path: &[&[u8]]) -> Option<Located> {
    let mut cs = 0;
    let mut ce = toks.len();
    let mut braces = None;
    for key in path {
        let (base, occ) = split_occurrence(key);
        let kids = children(toks, cs, ce);
        let mut matching = kids.iter().filter(|c| {
            c.key
                .is_some_and(|(ks, ke)| src[ks..ke].eq_ignore_ascii_case(base))
                && matches!(c.val, ValLoc::Block(_, _))
        });
        let child = match occ {
            Some(n) => matching.nth(n)?,
            None => matching.next()?,
        };
        if let ValLoc::Block(open_tok, close_tok) = child.val {
            let ob = match toks[open_tok] {
                Tok::Open(b) => b,
                _ => return None,
            };
            let cb = match toks[close_tok] {
                Tok::Close(b) => b,
                _ => return None,
            };
            cs = open_tok + 1;
            ce = close_tok;
            braces = Some((ob, cb));
        }
    }
    Some(Located { cs, ce, braces })
}

// ---------------------------------------------------------------------------
// Low-level byte helpers.
// ---------------------------------------------------------------------------

fn splice(src: &[u8], start: usize, end: usize, insert: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(src.len() - (end - start) + insert.len());
    out.extend_from_slice(&src[..start]);
    out.extend_from_slice(insert);
    out.extend_from_slice(&src[end..]);
    out
}

/// Start-of-line byte offset for the line containing `pos`.
fn line_start(src: &[u8], pos: usize) -> usize {
    let mut i = pos;
    while i > 0 && src[i - 1] != b'\n' {
        i -= 1;
    }
    i
}

fn is_space_tab(b: u8) -> bool {
    b == b' ' || b == b'\t'
}

fn unquote(b: &[u8]) -> &[u8] {
    if b.len() >= 2 && b[0] == b'"' && b[b.len() - 1] == b'"' {
        &b[1..b.len() - 1]
    } else {
        b
    }
}

// ---------------------------------------------------------------------------
// Operations.
// ---------------------------------------------------------------------------

/// Replaces the scalar value at `path` (block keys, then the scalar key).
/// `quoted` wraps the new value in double quotes.
fn set_scalar(
    src: &[u8],
    toks: &[Tok],
    path: &[&[u8]],
    value: &str,
    quoted: bool,
) -> Result<Vec<u8>, String> {
    let (key, block_path) = path
        .split_last()
        .ok_or("set_scalar requires a non-empty path")?;
    let loc = locate_block(src, toks, block_path).ok_or("target block not found")?;
    let kids = children(toks, loc.cs, loc.ce);
    let target = kids
        .iter()
        .find(|c| {
            c.key
                .is_some_and(|(ks, ke)| src[ks..ke].eq_ignore_ascii_case(key))
                && matches!(c.val, ValLoc::Scalar(_, _))
        })
        .ok_or_else(|| format!("scalar key not found: {}", String::from_utf8_lossy(key)))?;
    let (s, e) = match target.val {
        ValLoc::Scalar(s, e) => (s, e),
        _ => unreachable!(),
    };
    let mut rep = Vec::new();
    if quoted {
        rep.push(b'"');
    }
    rep.extend_from_slice(&encode_latin1(value)?);
    if quoted {
        rep.push(b'"');
    }
    Ok(splice(src, s, e, &rep))
}

/// Appends `text` at the end of the file, guaranteeing a separating newline
/// before it and a trailing newline after.
fn append_text(src: &[u8], text: &str) -> Result<Vec<u8>, String> {
    let enc = encode_latin1(text)?;
    let mut out = src.to_vec();
    if !out.is_empty() && *out.last().unwrap() != b'\n' {
        out.push(b'\n');
    }
    out.extend_from_slice(&enc);
    if !enc.is_empty() && *enc.last().unwrap() != b'\n' {
        out.push(b'\n');
    }
    Ok(out)
}

/// Detects the child indentation of a block: the leading whitespace of its
/// first line-leading child, else the closing brace's indent plus one tab.
fn child_indent(src: &[u8], toks: &[Tok], loc: &Located, cb: usize) -> Vec<u8> {
    for k in children(toks, loc.cs, loc.ce) {
        let ls = line_start(src, k.stmt_start);
        let ind = &src[ls..k.stmt_start];
        if !ind.is_empty() && ind.iter().all(|b| is_space_tab(*b)) {
            return ind.to_vec();
        }
    }
    let cls = line_start(src, cb);
    let mut ind: Vec<u8> = src[cls..cb]
        .iter()
        .copied()
        .take_while(|b| is_space_tab(*b))
        .collect();
    ind.push(b'\t');
    ind
}

/// Builds `text` (authored at column 0) re-indented so each non-empty line is
/// prefixed with `indent` and lines are joined with `nl`.
fn build_indented(text: &str, indent: &[u8], nl: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    for (i, raw) in text.split('\n').enumerate() {
        if i > 0 {
            out.extend_from_slice(nl);
        }
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        if !line.is_empty() {
            out.extend_from_slice(indent);
            out.extend_from_slice(&encode_latin1(line)?);
        }
    }
    Ok(out)
}

/// Inserts `text` as a new statement inside the block at `path` (empty = top
/// level). Multi-line blocks get a fresh indented line before the closing
/// brace; inline blocks get the statement spliced before the brace.
fn insert_into_block(
    src: &[u8],
    toks: &[Tok],
    path: &[&[u8]],
    text: &str,
) -> Result<Vec<u8>, String> {
    let loc = locate_block(src, toks, path).ok_or("target block not found")?;
    match loc.braces {
        None => append_text(src, text),
        Some((ob, cb)) => {
            if src[ob..=cb].contains(&b'\n') {
                let nl: &[u8] = if src[ob..cb].windows(2).any(|w| w == b"\r\n") {
                    b"\r\n"
                } else {
                    b"\n"
                };
                let indent = child_indent(src, toks, &loc, cb);
                let mut body = build_indented(text, &indent, nl)?;
                body.extend_from_slice(nl);
                let pos = line_start(src, cb);
                Ok(splice(src, pos, pos, &body))
            } else {
                let enc = encode_latin1(text)?;
                let mut ins = Vec::new();
                if cb > 0 && !is_space_tab(src[cb - 1]) {
                    ins.push(b' ');
                }
                ins.extend_from_slice(&enc);
                ins.push(b' ');
                Ok(splice(src, cb, cb, &ins))
            }
        }
    }
}

/// Inserts a new top-level `Y.M.D = { ... }` block in date order among the
/// existing top-level dated blocks: before the first later-dated block (so it
/// lands after every block dated ≤ the new date, for a chronologically-ordered
/// file), else appended at EOF. Byte-surgical — only the inserted line is added.
///
/// Merging INTO an existing exact-date block is a different operation: the caller
/// emits an `InsertStatement` targeting `["Y.M.D"]`/`["Y.M.D#n"]` instead.
fn insert_dated_block(
    src: &[u8],
    toks: &[Tok],
    date: &str,
    statement: &str,
) -> Result<Vec<u8>, String> {
    let new_date =
        parse_date(date.as_bytes()).ok_or_else(|| format!("Invalid date for dated block: {date}"))?;
    // First top-level dated block whose date is strictly later than the new date.
    let mut insert_pos: Option<usize> = None;
    for c in children(toks, 0, toks.len()) {
        let (Some((ks, ke)), ValLoc::Block(_, _)) = (c.key, c.val) else {
            continue;
        };
        if let Some(d) = parse_date(&src[ks..ke]) {
            if d > new_date {
                insert_pos = Some(line_start(src, c.stmt_start));
                break;
            }
        }
    }
    match insert_pos {
        None => append_text(src, statement),
        Some(pos) => {
            let nl: &[u8] = if src.windows(2).any(|w| w == b"\r\n") {
                b"\r\n"
            } else {
                b"\n"
            };
            let mut body = build_indented(statement, b"", nl)?;
            body.extend_from_slice(nl);
            Ok(splice(src, pos, pos, &body))
        }
    }
}

/// Deletes a statement's whole physical line (indent + statement + line ending)
/// when it owns its line, else just the statement plus one trailing whitespace
/// run so inline neighbors close up cleanly.
fn delete_statement_span(src: &[u8], s: usize, e: usize) -> Vec<u8> {
    let ls = line_start(src, s);
    let owns_line = src[ls..s].iter().all(|b| is_space_tab(*b) || *b == b'\r');
    if owns_line {
        // Drop the whole physical line, including any trailing comment and the
        // line ending. For a multi-line block value this spans the block.
        let mut del_end = e;
        while del_end < src.len() && src[del_end] != b'\n' {
            del_end += 1;
        }
        if del_end < src.len() {
            del_end += 1; // the '\n'
        }
        splice(src, ls, del_end, b"")
    } else {
        // Inline neighbor: drop just the statement + one trailing whitespace run.
        let mut del_end = e;
        while del_end < src.len() && is_space_tab(src[del_end]) {
            del_end += 1;
        }
        splice(src, s, del_end, b"")
    }
}

/// Removes the first child of `path` named `key` (optionally also matching
/// scalar `value`). Works for scalar lines and whole `{ ... }` blocks (heirs,
/// dated blocks, nested blocks).
fn remove_statement(
    src: &[u8],
    toks: &[Tok],
    path: &[&[u8]],
    key: &[u8],
    value: Option<&[u8]>,
) -> Result<Vec<u8>, String> {
    let loc = locate_block(src, toks, path).ok_or("target block not found")?;
    let kids = children(toks, loc.cs, loc.ce);
    // A `key#n` suffix selects the nth same-key child (0-based, file order) — the
    // same occurrence addressing `locate_block` applies to block paths. This is
    // what lets a repeated same-key top-level block (e.g. duplicate diplomacy
    // relation blocks that value-disambiguation can't reach, since their value is
    // a `{ … }` block, not a scalar) be removed by index. A bare key keeps its
    // first-match meaning and `#0` is exactly first-match, so existing callers are
    // unaffected.
    let (base_key, occ) = split_occurrence(key);
    let mut matching = kids.iter().filter(|c| {
        c.key
            .is_some_and(|(ks, ke)| src[ks..ke].eq_ignore_ascii_case(base_key))
            && match (value, c.val) {
                (None, _) => true,
                (Some(v), ValLoc::Scalar(vs, ve)) => unquote(&src[vs..ve]) == v,
                (Some(_), _) => false,
            }
    });
    let target = match occ {
        Some(n) => matching.nth(n),
        None => matching.next(),
    }
    .ok_or_else(|| format!("statement not found: {}", String::from_utf8_lossy(key)))?;
    Ok(delete_statement_span(src, target.stmt_start, target.stmt_end))
}

/// Adds a bare id as a new element of the named id-list at `path`.
fn add_id(src: &[u8], toks: &[Tok], path: &[&[u8]], id: &str) -> Result<Vec<u8>, String> {
    insert_into_block(src, toks, path, id)
}

/// Removes a bare id from the named id-list at `path`, tightening surrounding
/// whitespace. Ids inside comments are never touched (comments aren't tokens).
fn remove_id(src: &[u8], toks: &[Tok], path: &[&[u8]], id: &[u8]) -> Result<Vec<u8>, String> {
    let loc = locate_block(src, toks, path).ok_or("target list not found")?;
    let kids = children(toks, loc.cs, loc.ce);
    let target = kids
        .iter()
        .find(|c| {
            c.key.is_none()
                && matches!(c.val, ValLoc::Scalar(vs, ve) if &src[vs..ve] == id)
        })
        .ok_or_else(|| format!("id not found in list: {}", String::from_utf8_lossy(id)))?;
    let (s, e) = match target.val {
        ValLoc::Scalar(s, e) => (s, e),
        _ => unreachable!(),
    };

    let mut p = s;
    while p > 0 && is_space_tab(src[p - 1]) {
        p -= 1;
    }
    let before_is_linestart = p == 0 || src[p - 1] == b'\n' || src[p - 1] == b'\r';
    let mut q = e;
    while q < src.len() && is_space_tab(src[q]) {
        q += 1;
    }
    let after_is_lineend = q >= src.len() || src[q] == b'\n' || src[q] == b'\r';

    let (del_start, del_end) = if before_is_linestart && after_is_lineend {
        // Sole id on its own line: drop indent + id + line ending.
        let mut de = q;
        if de < src.len() && src[de] == b'\r' {
            de += 1;
        }
        if de < src.len() && src[de] == b'\n' {
            de += 1;
        }
        (p, de)
    } else if after_is_lineend {
        // Last id on a shared line: drop the preceding whitespace + id.
        (p, e)
    } else {
        // Otherwise drop the id + following whitespace so neighbors close up.
        (s, q)
    };
    Ok(splice(src, del_start, del_end, b""))
}

// ---------------------------------------------------------------------------
// SetBlockValue — replace the `{ ... }` span of a key's block value (Sprint 1.2).
//
// Byte-surgical: only the braces-and-contents of the targeted key change; every
// other byte (comments, sibling keys, Windows-1252 high bytes) round-trips.
// `mod_writer::SetScalar` can't do this because a `color = { r g b }` value is a
// block, not a scalar token. First consumer: the country panel's map color and
// revolutionary-colors editors (`common/countries/<file>`). The replacement is
// emitted canonically as `{ <value> }`; the value string is the inner tokens
// (e.g. "20 50 210"), authored by the caller. Only the block's own span is
// rewritten, so this normalizes just that one value's internal whitespace.
// ---------------------------------------------------------------------------

fn set_block_value(
    src: &[u8],
    toks: &[Tok],
    path: &[&[u8]],
    value: &str,
) -> Result<Vec<u8>, String> {
    let (key, block_path) = path
        .split_last()
        .ok_or("set_block_value requires a non-empty path")?;
    let loc = locate_block(src, toks, block_path).ok_or("target block not found")?;
    let kids = children(toks, loc.cs, loc.ce);
    // A `key#n` suffix selects the nth same-key *block* child (0-based, file
    // order) — the same occurrence addressing `remove_statement`/`locate_block`
    // use. This reaches a repeated same-key block (e.g. a region's multiple
    // `monsoon = { … }` blocks, S2.6) whose value is a `{ … }` block, not a
    // scalar, so value-disambiguation can't select it. A bare key keeps its
    // first-match meaning (`#0` == first match), so existing callers are unchanged.
    let (base_key, occ) = split_occurrence(key);
    let mut matching = kids.iter().filter(|c| {
        c.key
            .is_some_and(|(ks, ke)| src[ks..ke].eq_ignore_ascii_case(base_key))
            && matches!(c.val, ValLoc::Block(_, _))
    });
    let target = match occ {
        Some(n) => matching.nth(n),
        None => matching.next(),
    }
    .ok_or_else(|| format!("block key not found: {}", String::from_utf8_lossy(key)))?;
    let (open_tok, close_tok) = match target.val {
        ValLoc::Block(o, c) => (o, c),
        _ => unreachable!(),
    };
    let ob = match toks[open_tok] {
        Tok::Open(b) => b,
        _ => return Err("expected open brace".into()),
    };
    let cb = match toks[close_tok] {
        Tok::Close(b) => b,
        _ => return Err("expected close brace".into()),
    };
    let mut rep = Vec::new();
    rep.extend_from_slice(b"{ ");
    rep.extend_from_slice(&encode_latin1(value)?);
    rep.extend_from_slice(b" }");
    Ok(splice(src, ob, cb + 1, &rep))
}

// ---------------------------------------------------------------------------
// Spans API (Sprint 14.1) — read-only block/child addressing for the typed
// script tree and the raw/tree toggle. Reuses the offset tokenizer above (no
// second tokenizer). All returned byte spans index the ORIGINAL `src`.
// ---------------------------------------------------------------------------

/// The braces-inclusive byte span of the block addressed by `path` (segments may
/// carry `#n` occurrence suffixes, exactly as [`Edit`] paths do). An empty path
/// is the whole file → `(0, src.len())`. `None` if the path doesn't resolve.
pub fn block_span(src: &[u8], path: &[String]) -> Option<(usize, usize)> {
    let toks = tokenize(src);
    let loc = locate_block(src, &toks, &path_bytes(path))?;
    Some(match loc.braces {
        Some((ob, cb)) => (ob, cb + 1),
        None => (0, src.len()),
    })
}

/// One direct child (statement) of a block, exposed for the typed-tree builder.
#[derive(Debug, Clone)]
pub struct ChildSpan {
    /// Key text, or `None` for a bare list element / anonymous `{ … }` block.
    pub key: Option<String>,
    /// 0-based occurrence of this key among same-*kind* (block vs scalar)
    /// siblings. A block child's occurrence is exactly what `locate_block`'s
    /// `key#n` addressing counts, so a group node addressed as `key#occurrence`
    /// resolves to this child.
    pub occurrence: usize,
    /// Whether the value is a `{ … }` block.
    pub is_block: bool,
    /// Byte span of the value: braces-inclusive for a block, the token span for a
    /// scalar, or the element token span for a bare list element.
    pub value_span: (usize, usize),
    /// Byte span of the whole statement (key start .. value end).
    pub stmt_span: (usize, usize),
}

/// Direct children, in file order, of the block addressed by `path`. `None` if
/// the path doesn't resolve to a block. Anonymous nested blocks are reported as
/// keyless children (they are not path-addressable — the tree builder falls back
/// to a raw edit of the nearest named ancestor for those).
pub fn block_children(src: &[u8], path: &[String]) -> Option<Vec<ChildSpan>> {
    let toks = tokenize(src);
    let loc = locate_block(src, &toks, &path_bytes(path))?;
    let kids = children(&toks, loc.cs, loc.ce);
    let mut block_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    let mut scalar_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    let mut out = Vec::with_capacity(kids.len());
    for c in kids {
        let key = c
            .key
            .map(|(ks, ke)| String::from_utf8_lossy(&src[ks..ke]).into_owned());
        let is_block = matches!(c.val, ValLoc::Block(_, _));
        let occurrence = match &key {
            Some(k) => {
                let counts = if is_block {
                    &mut block_counts
                } else {
                    &mut scalar_counts
                };
                let e = counts.entry(k.clone()).or_insert(0);
                let o = *e;
                *e += 1;
                o
            }
            None => 0,
        };
        let value_span = match c.val {
            ValLoc::Scalar(s, e) => (s, e),
            ValLoc::Block(ot, ct) => {
                let ob = match toks[ot] {
                    Tok::Open(b) => b,
                    _ => c.stmt_start,
                };
                let cb = match toks[ct] {
                    Tok::Close(b) => b,
                    _ => c.stmt_end.saturating_sub(1),
                };
                (ob, cb + 1)
            }
        };
        out.push(ChildSpan {
            key,
            occurrence,
            is_block,
            value_span,
            stmt_span: (c.stmt_start, c.stmt_end),
        });
    }
    Some(out)
}

// ---------------------------------------------------------------------------
// Public toolkit API (the substrate a typed pending-edit queue drives).
// ---------------------------------------------------------------------------

/// One surgical operation targeting a single game file. Paths are ordered key
/// lists: `["1444.1.1", "monarch", "adm"]` reaches the `adm` scalar inside the
/// `monarch` block inside the `1444.1.1` dated block. An empty block path means
/// the whole file (top level).
#[derive(Debug, Clone)]
pub enum Edit {
    /// Replace the scalar value at `path` (last element is the scalar key).
    /// `quoted` wraps the value in `"`.
    SetScalar {
        path: Vec<String>,
        value: String,
        quoted: bool,
    },
    /// Insert a pre-formatted statement (authored at column 0; re-indented to
    /// the block's depth) as a new child of `block_path`. Use for key add,
    /// block insert (heirs, dated blocks), and religion-into-group inserts.
    InsertStatement {
        block_path: Vec<String>,
        statement: String,
    },
    /// Insert a new top-level `Y.M.D = { ... }` block in date order among the
    /// existing dated blocks (Sprint 12.3). `date` is the ordering key; `statement`
    /// is the whole block text. To MERGE into an existing exact-date block, use
    /// `InsertStatement` targeting `["Y.M.D"]` instead.
    InsertDatedBlock {
        date: String,
        statement: String,
    },
    /// Remove the first child of `block_path` named `key` (optionally also
    /// matching scalar `value`). Handles both scalar lines and whole blocks.
    RemoveStatement {
        block_path: Vec<String>,
        key: String,
        value: Option<String>,
    },
    /// Replace the `{ ... }` block value of the key at `path` (last element is
    /// the block key). Byte-surgical replacement of just that value's braces and
    /// contents, emitted as `{ <value> }`. For `color`/`revolutionary_colors`.
    SetBlock {
        path: Vec<String>,
        value: String,
    },
    /// Add a bare id to the named id-list at `list_path`
    /// (climate/area/tradenodes members, terrain_override, discovered_by …).
    AddId {
        list_path: Vec<String>,
        id: String,
    },
    /// Remove a bare id from the named id-list at `list_path`.
    RemoveId {
        list_path: Vec<String>,
        id: String,
    },
    /// Append raw text to the end of the file (new relation block, tag mapping).
    Append { text: String },
    /// Whole-file scaffold for a brand-new file: `src` is ignored and the file
    /// becomes exactly `text` (Latin-1 encoded).
    CreateFile { text: String },
}

fn path_bytes(p: &[String]) -> Vec<&[u8]> {
    p.iter().map(|s| s.as_bytes()).collect()
}

/// Applies one [`Edit`] to `src`, returning the new file bytes. Pure: no I/O.
/// Every byte outside the edited span round-trips identically.
pub fn apply(src: &[u8], edit: &Edit) -> Result<Vec<u8>, String> {
    let toks = tokenize(src);
    match edit {
        Edit::SetScalar {
            path,
            value,
            quoted,
        } => set_scalar(src, &toks, &path_bytes(path), value, *quoted),
        Edit::SetBlock { path, value } => set_block_value(src, &toks, &path_bytes(path), value),
        Edit::InsertStatement {
            block_path,
            statement,
        } => insert_into_block(src, &toks, &path_bytes(block_path), statement),
        Edit::InsertDatedBlock { date, statement } => {
            insert_dated_block(src, &toks, date, statement)
        }
        Edit::RemoveStatement {
            block_path,
            key,
            value,
        } => remove_statement(
            src,
            &toks,
            &path_bytes(block_path),
            key.as_bytes(),
            value.as_deref().map(str::as_bytes),
        ),
        Edit::AddId { list_path, id } => add_id(src, &toks, &path_bytes(list_path), id),
        Edit::RemoveId { list_path, id } => {
            remove_id(src, &toks, &path_bytes(list_path), id.as_bytes())
        }
        Edit::Append { text } => append_text(src, text),
        Edit::CreateFile { text } => encode_latin1(text),
    }
}

/// Applies a sequence of [`Edit`]s to `src` in order.
#[cfg(test)]
pub fn apply_all(src: &[u8], edits: &[Edit]) -> Result<Vec<u8>, String> {
    let mut bytes = src.to_vec();
    for e in edits {
        bytes = apply(&bytes, e)?;
    }
    Ok(bytes)
}

/// Copy-on-write driver: resolves `rel` through the [`Vfs`] (mod shadows base;
/// a missing file starts from empty bytes, e.g. for `CreateFile`/`Append`),
/// applies `edits`, and writes the result into `project_dir` at `rel` (creating
/// parent directories). The base install is never written. Returns `rel`.
#[cfg(test)]
pub fn apply_file_edits(
    vfs: &Vfs,
    project_dir: &Path,
    rel: &str,
    edits: &[Edit],
) -> Result<String, String> {
    let src = match vfs.resolve(rel) {
        Some(p) => std::fs::read(&p).map_err(|e| format!("Failed to read {rel}: {e}"))?,
        None => Vec::new(),
    };
    let out = apply_all(&src, edits)?;
    let dest = project_dir.join(rel);
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create {}: {e}", parent.display()))?;
    }
    std::fs::write(&dest, out).map_err(|e| format!("Failed to write {rel}: {e}"))?;
    Ok(rel.to_string())
}

/// Writes whole-file scaffold `bytes` into `project_dir` at `rel`, creating
/// parent directories. For files the toolkit generates from scratch.
pub fn write_scaffold(project_dir: &Path, rel: &str, bytes: &[u8]) -> Result<String, String> {
    let dest = project_dir.join(rel);
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create {}: {e}", parent.display()))?;
    }
    std::fs::write(&dest, bytes).map_err(|e| format!("Failed to write {rel}: {e}"))?;
    Ok(rel.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &[u8] = br#"government = monarchy
capital = 183 # Paris
1422.10.21 = {
	monarch = {
		name = "Charles VII" # le Victorieux
		dynasty = "de Valois"
		adm = 5
	}
}
1445.1.1 = {
	monarch = {
		name = "Too Late"
	}
}
1461.7.22 = {
	monarch = { name = "Louis XI" }
}
"#;

    #[test]
    fn renames_starting_ruler_only() {
        let out = rename_ruler(SAMPLE, "Karl the Tested").unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("name = \"Karl the Tested\" # le Victorieux"));
        // Later rulers untouched; comments and formatting intact.
        assert!(text.contains("\"Too Late\""));
        assert!(text.contains("\"Louis XI\""));
        assert!(text.contains("capital = 183 # Paris"));
        assert!(text.contains("dynasty = \"de Valois\""));
    }

    #[test]
    fn picks_latest_pre_start_monarch() {
        let src = br#"
1400.1.1 = { monarch = { name = "Old" } }
1440.1.1 = { monarch = { name = "Current" } }
"#;
        let out = rename_ruler(src, "New").unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("name = \"Old\""));
        assert!(text.contains("name = \"New\""));
        assert!(!text.contains("Current"));
    }

    #[test]
    fn errors_without_starting_ruler() {
        let src = br#"1450.1.1 = { monarch = { name = "Post" } }"#;
        assert!(rename_ruler(src, "X").is_err());
    }

    #[test]
    fn preserves_non_ascii_bytes_elsewhere() {
        // 0xE9 = e-acute in Windows-1252; must survive byte-for-byte.
        let src = b"# comment with \xE9\n1444.1.1 = { monarch = { name = \"A\" } }\n";
        let out = rename_ruler(src, "B").unwrap();
        assert!(out.windows(2).any(|w| w == b"\xE9\n"));
    }

    #[test]
    fn encodes_new_name_as_latin1() {
        let src = b"1444.1.1 = { monarch = { name = \"A\" } }".to_vec();
        let out = rename_ruler(&src, "R\u{e9}n\u{e9}").unwrap();
        assert!(out.windows(6).any(|w| w == b"\"R\xE9n\xE9\""));
    }

    // --- toolkit: shared asserts ----------------------------------------

    /// Asserts every byte outside `[start, end)` in `before` equals the
    /// corresponding byte in `after` (accounting for the length delta of the
    /// edited region). Proves surgical edits leave surroundings byte-identical.
    fn assert_outside_identical(before: &[u8], after: &[u8], start: usize, before_end: usize) {
        assert_eq!(&before[..start], &after[..start], "prefix changed");
        let delta = after.len() as isize - before.len() as isize;
        let after_end = (before_end as isize + delta) as usize;
        assert_eq!(
            &before[before_end..],
            &after[after_end..],
            "suffix changed"
        );
    }

    fn find(haystack: &[u8], needle: &[u8]) -> usize {
        haystack
            .windows(needle.len())
            .position(|w| w == needle)
            .unwrap_or_else(|| panic!("not found: {}", String::from_utf8_lossy(needle)))
    }

    // --- set scalar ------------------------------------------------------

    #[test]
    fn set_scalar_top_level_replaces_only_value() {
        let src = b"government = monarchy\ncapital = 183 # Paris\n";
        let out = apply(
            src,
            &Edit::SetScalar {
                path: vec!["government".into()],
                value: "republic".into(),
                quoted: false,
            },
        )
        .unwrap();
        assert_eq!(out, b"government = republic\ncapital = 183 # Paris\n");
        let s = find(src, b"monarchy");
        assert_outside_identical(src, &out, s, s + b"monarchy".len());
    }

    #[test]
    fn set_scalar_nested_and_quoted() {
        let out = apply(
            SAMPLE,
            &Edit::SetScalar {
                path: vec!["1422.10.21".into(), "monarch".into(), "adm".into()],
                value: "3".into(),
                quoted: false,
            },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("adm = 3"));
        // Everything else intact.
        assert!(text.contains("name = \"Charles VII\" # le Victorieux"));
        assert!(text.contains("dynasty = \"de Valois\""));
    }

    #[test]
    fn set_scalar_crlf_preserved() {
        let src = b"government = monarchy\r\ncapital = 183\r\n";
        let out = apply(
            src,
            &Edit::SetScalar {
                path: vec!["capital".into()],
                value: "42".into(),
                quoted: false,
            },
        )
        .unwrap();
        assert_eq!(out, b"government = monarchy\r\ncapital = 42\r\n");
    }

    #[test]
    fn set_scalar_rejects_non_latin1() {
        let src = b"name = x\n";
        let err = apply(
            src,
            &Edit::SetScalar {
                path: vec!["name".into()],
                value: "\u{4e2d}".into(), // CJK char, not encodable
                quoted: true,
            },
        );
        assert!(err.is_err());
    }

    // --- set block value (Sprint 1.2) -----------------------------------

    #[test]
    fn set_block_replaces_only_the_braced_span() {
        // Comments and a Windows-1252 high byte around the color must round-trip.
        let mut src = b"graphical_culture = westerngfx\ncolor = { 20  50  210 } # France blue caf".to_vec();
        src.push(0xE9); // é in Windows-1252, inside the trailing comment
        src.extend_from_slice(b"\nrevolutionary_colors = { 15 0 16 }\n");
        let out = apply(
            &src,
            &Edit::SetBlock {
                path: vec!["color".into()],
                value: "10 20 30".into(),
            },
        )
        .unwrap();
        let text = String::from_utf8_lossy(&out);
        assert!(text.contains("color = { 10 20 30 } # France blue caf"));
        // Sibling block and the surrounding comment/high byte untouched.
        assert!(text.contains("graphical_culture = westerngfx\n"));
        assert!(text.contains("revolutionary_colors = { 15 0 16 }\n"));
        assert!(out.windows(2).any(|w| w == b"\xE9\n"), "high byte preserved");
        // Byte-identical outside the color value's brace span.
        let s = find(&src, b"{ 20  50  210 }");
        assert_outside_identical(&src, &out, s, s + b"{ 20  50  210 }".len());
    }

    #[test]
    fn set_block_revolutionary_colors_indices() {
        let src = b"color = { 1 2 3 }\nrevolutionary_colors = { 5 0 5 }\n";
        let out = apply(
            src,
            &Edit::SetBlock {
                path: vec!["revolutionary_colors".into()],
                value: "8 1 8".into(),
            },
        )
        .unwrap();
        assert_eq!(out, b"color = { 1 2 3 }\nrevolutionary_colors = { 8 1 8 }\n");
    }

    #[test]
    fn set_block_nested_path() {
        let src = b"trigger = {\n\tcolor = { 0 0 0 }\n}\n";
        let out = apply(
            src,
            &Edit::SetBlock {
                path: vec!["trigger".into(), "color".into()],
                value: "9 9 9".into(),
            },
        )
        .unwrap();
        assert_eq!(out, b"trigger = {\n\tcolor = { 9 9 9 }\n}\n");
    }

    #[test]
    fn set_block_occurrence_selects_nth_same_key_block() {
        // Two same-key blocks (like a region's repeated `monsoon = { … }`, S2.6):
        // `#1` edits the second, leaving the first byte-identical.
        let src = b"r = {\n\tmonsoon = { 1 2 }\n\tmonsoon = { 3 4 }\n}\n";
        let out = apply(
            src,
            &Edit::SetBlock {
                path: vec!["r".into(), "monsoon#1".into()],
                value: "7 8".into(),
            },
        )
        .unwrap();
        assert_eq!(out, b"r = {\n\tmonsoon = { 1 2 }\n\tmonsoon = { 7 8 }\n}\n");
        // Bare key (== `#0`) still selects the first.
        let out0 = apply(
            src,
            &Edit::SetBlock {
                path: vec!["r".into(), "monsoon".into()],
                value: "9 9".into(),
            },
        )
        .unwrap();
        assert_eq!(out0, b"r = {\n\tmonsoon = { 9 9 }\n\tmonsoon = { 3 4 }\n}\n");
    }

    #[test]
    fn set_block_missing_key_errors() {
        let src = b"color = { 1 2 3 }\n";
        assert!(apply(
            src,
            &Edit::SetBlock {
                path: vec!["revolutionary_colors".into()],
                value: "1 1 1".into(),
            },
        )
        .is_err());
    }

    // --- key add / remove ------------------------------------------------

    #[test]
    fn add_key_top_level_appends_line() {
        let src = b"government = monarchy\ncapital = 183\n";
        let out = apply(
            src,
            &Edit::InsertStatement {
                block_path: vec![],
                statement: "religion = catholic".into(),
            },
        )
        .unwrap();
        assert_eq!(out, b"government = monarchy\ncapital = 183\nreligion = catholic\n");
    }

    #[test]
    fn add_key_inside_multiline_block_is_indented() {
        let out = apply(
            SAMPLE,
            &Edit::InsertStatement {
                block_path: vec!["1422.10.21".into(), "monarch".into()],
                statement: "dip = 4".into(),
            },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        // New line indented to sibling depth (two tabs), before the block close.
        assert!(text.contains("\t\tadm = 5\n\t\tdip = 4\n\t}"));
        assert!(text.contains("name = \"Charles VII\" # le Victorieux"));
    }

    #[test]
    fn add_key_inside_inline_block() {
        let src = b"monarch = { name = \"Louis XI\" }\n";
        let out = apply(
            src,
            &Edit::InsertStatement {
                block_path: vec!["monarch".into()],
                statement: "adm = 3".into(),
            },
        )
        .unwrap();
        assert_eq!(out, b"monarch = { name = \"Louis XI\" adm = 3 }\n");
    }

    #[test]
    fn remove_key_deletes_whole_line() {
        let src = b"government = monarchy\ncapital = 183 # Paris\nreligion = catholic\n";
        let out = apply(
            src,
            &Edit::RemoveStatement {
                block_path: vec![],
                key: "capital".into(),
                value: None,
            },
        )
        .unwrap();
        assert_eq!(out, b"government = monarchy\nreligion = catholic\n");
    }

    #[test]
    fn remove_key_with_value_filter_among_duplicates() {
        let src = b"add_core = FRA\nadd_core = ENG\nadd_core = CAS\n";
        let out = apply(
            src,
            &Edit::RemoveStatement {
                block_path: vec![],
                key: "add_core".into(),
                value: Some("ENG".into()),
            },
        )
        .unwrap();
        assert_eq!(out, b"add_core = FRA\nadd_core = CAS\n");
    }

    // --- block insert / delete ------------------------------------------

    #[test]
    fn insert_heir_block_multiline() {
        let src = b"government = monarchy\n1444.1.1 = {\n\tmonarch = {\n\t\tname = \"A\"\n\t}\n}\n";
        let out = apply(
            src,
            &Edit::InsertStatement {
                block_path: vec!["1444.1.1".into()],
                statement: "heir = {\n\tname = \"B\"\n\tclaim = 90\n}".into(),
            },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        // Heir block indented one level, nested content two levels.
        assert!(text.contains("\their = {\n\t\tname = \"B\"\n\t\tclaim = 90\n\t}\n}"));
        assert!(text.contains("\t\tname = \"A\"\n"));
    }

    // --- date-ordered dated-block insert (Sprint 12.3) -------------------

    const ORDERED_DATES: &[u8] =
        b"owner = FRA\n1450.1.1 = { unrest = 5 }\n1460.1.1 = { unrest = 6 }\n";

    #[test]
    fn insert_dated_block_lands_between_neighbors_byte_surgical() {
        let out = apply(
            ORDERED_DATES,
            &Edit::InsertDatedBlock {
                date: "1455.1.1".into(),
                statement: "1455.1.1 = { religion = reformed }".into(),
            },
        )
        .unwrap();
        assert_eq!(
            out,
            b"owner = FRA\n1450.1.1 = { unrest = 5 }\n1455.1.1 = { religion = reformed }\n1460.1.1 = { unrest = 6 }\n"
        );
        // Everything before the 1460 line is byte-identical (pure insert).
        let s = find(ORDERED_DATES, b"1460.1.1");
        let ls = line_start(ORDERED_DATES, s);
        assert_eq!(&out[..ls], &ORDERED_DATES[..ls]);
    }

    #[test]
    fn insert_dated_block_before_all_and_after_all() {
        // Earlier than every existing block → before the first (1450) block.
        let early = apply(
            ORDERED_DATES,
            &Edit::InsertDatedBlock {
                date: "1440.1.1".into(),
                statement: "1440.1.1 = { owner = ENG }".into(),
            },
        )
        .unwrap();
        assert_eq!(
            early,
            b"owner = FRA\n1440.1.1 = { owner = ENG }\n1450.1.1 = { unrest = 5 }\n1460.1.1 = { unrest = 6 }\n"
        );
        // Later than every existing block → appended at EOF.
        let late = apply(
            ORDERED_DATES,
            &Edit::InsertDatedBlock {
                date: "1470.1.1".into(),
                statement: "1470.1.1 = { owner = CAS }".into(),
            },
        )
        .unwrap();
        assert_eq!(
            late,
            b"owner = FRA\n1450.1.1 = { unrest = 5 }\n1460.1.1 = { unrest = 6 }\n1470.1.1 = { owner = CAS }\n"
        );
    }

    #[test]
    fn merge_into_existing_exact_date_block_unchanged() {
        // Merging into an existing exact-date block stays a plain InsertStatement
        // into the ["Y.M.D"] path — byte-surgical, only that block grows.
        let out = apply(
            ORDERED_DATES,
            &Edit::InsertStatement {
                block_path: vec!["1450.1.1".into()],
                statement: "religion = protestant".into(),
            },
        )
        .unwrap();
        assert_eq!(
            out,
            b"owner = FRA\n1450.1.1 = { unrest = 5 religion = protestant }\n1460.1.1 = { unrest = 6 }\n"
        );
    }

    #[test]
    fn insert_dated_block_at_top_level() {
        let src = b"government = monarchy\n";
        let out = apply(
            src,
            &Edit::InsertStatement {
                block_path: vec![],
                statement: "1450.1.1 = { unrest = 5 }".into(),
            },
        )
        .unwrap();
        assert_eq!(out, b"government = monarchy\n1450.1.1 = { unrest = 5 }\n");
    }

    #[test]
    fn delete_dated_block_leaves_rest_intact() {
        let out = apply(
            SAMPLE,
            &Edit::RemoveStatement {
                block_path: vec![],
                key: "1445.1.1".into(),
                value: None,
            },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(!text.contains("Too Late"));
        // Surrounding blocks byte-identical.
        assert!(text.contains("\t\tname = \"Charles VII\" # le Victorieux\n"));
        assert!(text.contains("1461.7.22 = {\n\tmonarch = { name = \"Louis XI\" }\n}\n"));
    }

    #[test]
    fn insert_nested_block_into_named_group() {
        // Religion-into-group shape (Sprint 5).
        let src =
            b"christian = {\n\tcatholic = {\n\t\tcolor = { 1 1 0 }\n\t}\n}\n";
        let out = apply(
            src,
            &Edit::InsertStatement {
                block_path: vec!["christian".into()],
                statement: "orthodox = {\n\tcolor = { 0 1 0 }\n}".into(),
            },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("\torthodox = {\n\t\tcolor = { 0 1 0 }\n\t}\n}"));
        assert!(text.contains("\tcatholic = {\n\t\tcolor = { 1 1 0 }\n\t}\n"));
    }

    // --- occurrence-indexed dated blocks (Sprint 2.3) --------------------

    const DUP_DATES: &[u8] = b"owner = TUR\n\
1481.6.1 = { unrest = 6 }\n\
1482.7.26 = { unrest = 0 }\n\
1481.6.1 = { unrest = 9 base_tax = 5 }\n";

    #[test]
    fn set_scalar_addresses_second_duplicate_date() {
        // Edit the unrest inside the SECOND 1481.6.1 block; the first is untouched.
        let out = apply(
            DUP_DATES,
            &Edit::SetScalar {
                path: vec!["1481.6.1#1".into(), "unrest".into()],
                value: "3".into(),
                quoted: false,
            },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("1481.6.1 = { unrest = 6 }\n"), "first block intact");
        assert!(text.contains("1481.6.1 = { unrest = 3 base_tax = 5 }\n"), "second edited");
    }

    #[test]
    fn set_scalar_occurrence_zero_is_first_match() {
        // `#0` must behave exactly like an un-suffixed segment (first match).
        let a = apply(
            DUP_DATES,
            &Edit::SetScalar {
                path: vec!["1481.6.1#0".into(), "unrest".into()],
                value: "1".into(),
                quoted: false,
            },
        )
        .unwrap();
        let text = String::from_utf8(a).unwrap();
        assert!(text.contains("1481.6.1 = { unrest = 1 }\n"), "first block edited");
        assert!(text.contains("1481.6.1 = { unrest = 9 base_tax = 5 }\n"), "second intact");
    }

    #[test]
    fn remove_statement_inside_second_duplicate_date() {
        // Delete base_tax from the second 1481.6.1 block only.
        let out = apply(
            DUP_DATES,
            &Edit::RemoveStatement {
                block_path: vec!["1481.6.1#1".into()],
                key: "base_tax".into(),
                value: None,
            },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("1481.6.1 = { unrest = 6 }\n"), "first block intact");
        assert!(text.contains("1481.6.1 = { unrest = 9 }\n"), "second lost base_tax");
    }

    #[test]
    fn remove_statement_deletes_second_same_key_sibling_group() {
        // 14B gap fix: two same-key sibling GROUPS (value-disambiguation can't
        // reach them — their value is a `{ … }` block). `key#n` selects the nth,
        // so `OR#1` removes the SECOND OR and leaves the first byte-for-byte.
        let src = b"potential = {\n\tOR = { a = 1 }\n\tOR = { b = 2 }\n\ttag = FRA\n}\n";
        let out = apply(
            src,
            &Edit::RemoveStatement {
                block_path: vec!["potential".into()],
                key: "OR#1".into(),
                value: None,
            },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("OR = { a = 1 }"), "first OR intact");
        assert!(!text.contains("b = 2"), "second OR removed");
        assert!(text.contains("tag = FRA"), "sibling leaf intact");
        // `OR#0` (or bare `OR`) still removes the first.
        let first = apply(
            src,
            &Edit::RemoveStatement {
                block_path: vec!["potential".into()],
                key: "OR#0".into(),
                value: None,
            },
        )
        .unwrap();
        let ftext = String::from_utf8(first).unwrap();
        assert!(!ftext.contains("a = 1"), "first OR removed by #0");
        assert!(ftext.contains("OR = { b = 2 }"), "second OR intact");
    }

    #[test]
    fn occurrence_beyond_range_errors() {
        assert!(apply(
            DUP_DATES,
            &Edit::SetScalar {
                path: vec!["1481.6.1#2".into(), "unrest".into()],
                value: "0".into(),
                quoted: false,
            },
        )
        .is_err());
    }

    // --- id-list splice --------------------------------------------------

    const CLIMATE: &[u8] = b"tropical = {\n\t746 747 748 # west indies\n\t1097\n}\narctic = {\n\t100\n}\n";

    #[test]
    fn add_id_to_multiline_list() {
        let out = apply(
            CLIMATE,
            &Edit::AddId {
                list_path: vec!["tropical".into()],
                id: "999".into(),
            },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("\t1097\n\t999\n}"));
        // Other list and comment untouched.
        assert!(text.contains("746 747 748 # west indies"));
        assert!(text.contains("arctic = {\n\t100\n}"));
    }

    #[test]
    fn remove_first_id_on_line() {
        let out = apply(
            CLIMATE,
            &Edit::RemoveId {
                list_path: vec!["tropical".into()],
                id: "746".into(),
            },
        )
        .unwrap();
        assert_eq!(
            out,
            b"tropical = {\n\t747 748 # west indies\n\t1097\n}\narctic = {\n\t100\n}\n"
        );
    }

    #[test]
    fn remove_middle_id_on_line() {
        let out = apply(
            CLIMATE,
            &Edit::RemoveId {
                list_path: vec!["tropical".into()],
                id: "747".into(),
            },
        )
        .unwrap();
        assert_eq!(
            out,
            b"tropical = {\n\t746 748 # west indies\n\t1097\n}\narctic = {\n\t100\n}\n"
        );
    }

    #[test]
    fn remove_last_id_before_comment() {
        // 748 is followed by a comment, so it is not "line end"; the id and its
        // preceding space go, the comment stays.
        let out = apply(
            CLIMATE,
            &Edit::RemoveId {
                list_path: vec!["tropical".into()],
                id: "748".into(),
            },
        )
        .unwrap();
        assert_eq!(
            out,
            b"tropical = {\n\t746 747 # west indies\n\t1097\n}\narctic = {\n\t100\n}\n"
        );
    }

    #[test]
    fn remove_sole_id_on_line_drops_the_line() {
        let out = apply(
            CLIMATE,
            &Edit::RemoveId {
                list_path: vec!["tropical".into()],
                id: "1097".into(),
            },
        )
        .unwrap();
        assert_eq!(
            out,
            b"tropical = {\n\t746 747 748 # west indies\n}\narctic = {\n\t100\n}\n"
        );
    }

    #[test]
    fn id_in_comment_is_not_removed() {
        // The "747" in the comment must not match; only the real token is gone.
        let src = b"tropical = {\n\t# keep 747 in this note\n\t747 748\n}\n";
        let out = apply(
            src,
            &Edit::RemoveId {
                list_path: vec!["tropical".into()],
                id: "747".into(),
            },
        )
        .unwrap();
        assert_eq!(out, b"tropical = {\n\t# keep 747 in this note\n\t748\n}\n");
    }

    #[test]
    fn add_and_remove_id_inline_list() {
        let src = b"members = { 1 2 3 }\n";
        let added = apply(
            src,
            &Edit::AddId {
                list_path: vec!["members".into()],
                id: "4".into(),
            },
        )
        .unwrap();
        assert_eq!(added, b"members = { 1 2 3 4 }\n");
        let removed = apply(
            &added,
            &Edit::RemoveId {
                list_path: vec!["members".into()],
                id: "1".into(),
            },
        )
        .unwrap();
        assert_eq!(removed, b"members = { 2 3 4 }\n");
    }

    #[test]
    fn id_list_crlf_and_high_bytes_roundtrip() {
        // CRLF endings, a comment with a Windows-1252 high byte (0xE9).
        let src = b"tropical = {\r\n\t10 20 30 # caf\xE9\r\n}\r\n";
        let out = apply(
            src,
            &Edit::AddId {
                list_path: vec!["tropical".into()],
                id: "40".into(),
            },
        )
        .unwrap();
        assert_eq!(out, b"tropical = {\r\n\t10 20 30 # caf\xE9\r\n\t40\r\n}\r\n");
    }

    // --- append / scaffold ----------------------------------------------

    #[test]
    fn append_adds_separating_newline() {
        let src = b"FRA = { color = { 1 1 1 } }"; // no trailing newline
        let out = apply(
            src,
            &Edit::Append {
                text: "ENG = { color = { 2 2 2 } }".into(),
            },
        )
        .unwrap();
        assert_eq!(
            out,
            b"FRA = { color = { 1 1 1 } }\nENG = { color = { 2 2 2 } }\n"
        );
    }

    #[test]
    fn create_file_encodes_latin1() {
        let out = apply(
            b"",
            &Edit::CreateFile {
                text: "name = \"Caf\u{e9}\"\n".into(),
            },
        )
        .unwrap();
        assert_eq!(out, b"name = \"Caf\xE9\"\n");
    }

    // --- Vfs copy-on-write driver ---------------------------------------

    fn setup_vfs(name: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let root = std::env::temp_dir().join(format!("eu_toolkit_modwriter_test_{name}"));
        let base = root.join("base");
        let project = root.join("project");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(base.join("map")).unwrap();
        std::fs::create_dir_all(base.join("history/countries")).unwrap();
        // provinces.bmp so Vfs::new accepts the base as an install.
        std::fs::write(base.join("map/provinces.bmp"), b"x").unwrap();
        (base, project)
    }

    #[test]
    fn apply_file_edits_is_copy_on_write() {
        let (base, project) = setup_vfs("cow");
        let rel = "history/countries/FRA - France.txt";
        let original = b"government = monarchy\ncapital = 183\n".to_vec();
        std::fs::write(base.join(rel), &original).unwrap();

        let vfs = Vfs::new(base.to_str().unwrap(), None).unwrap();
        let written = apply_file_edits(
            &vfs,
            &project,
            rel,
            &[Edit::SetScalar {
                path: vec!["government".into()],
                value: "republic".into(),
                quoted: false,
            }],
        )
        .unwrap();
        assert_eq!(written, rel);

        // Written into the project, edited.
        let saved = std::fs::read(project.join(rel)).unwrap();
        assert_eq!(saved, b"government = republic\ncapital = 183\n");
        // Base file untouched.
        assert_eq!(std::fs::read(base.join(rel)).unwrap(), original);
    }

    #[test]
    fn apply_file_edits_resolves_mod_layer() {
        // With a mod layer present, edits source from the mod's own copy.
        let (base, project) = setup_vfs("modlayer");
        let rel = "history/countries/FRA - France.txt";
        std::fs::write(base.join(rel), b"government = monarchy\n").unwrap();
        // Mod already shadows the base file with its own content.
        std::fs::create_dir_all(project.join("history/countries")).unwrap();
        std::fs::write(project.join(rel), b"government = republic\ncapital = 1\n").unwrap();
        std::fs::write(
            project.join("descriptor.mod"),
            b"name=\"m\"\n",
        )
        .unwrap();

        let vfs = Vfs::new(base.to_str().unwrap(), Some(project.to_str().unwrap())).unwrap();
        apply_file_edits(
            &vfs,
            &project,
            rel,
            &[Edit::SetScalar {
                path: vec!["government".into()],
                value: "theocracy".into(),
                quoted: false,
            }],
        )
        .unwrap();
        // Sourced from the mod copy (had "republic"), now "theocracy".
        let saved = std::fs::read_to_string(project.join(rel)).unwrap();
        assert!(saved.starts_with("government = theocracy\n"));
        assert!(saved.contains("capital = 1")); // mod-only line preserved
    }

    #[test]
    fn create_file_via_driver_for_missing_source() {
        let (base, project) = setup_vfs("createfile");
        let vfs = Vfs::new(base.to_str().unwrap(), None).unwrap();
        let rel = "common/country_tags/zz_new.txt";
        apply_file_edits(
            &vfs,
            &project,
            rel,
            &[Edit::CreateFile {
                text: "ZZZ = \"countries/Newland.txt\"\n".into(),
            }],
        )
        .unwrap();
        let saved = std::fs::read_to_string(project.join(rel)).unwrap();
        assert_eq!(saved, "ZZZ = \"countries/Newland.txt\"\n");
    }

    #[test]
    fn rename_ruler_via_toolkit_still_available() {
        // The original operation is unchanged and coexists with the toolkit.
        let out = rename_ruler(SAMPLE, "Karl").unwrap();
        assert!(String::from_utf8(out).unwrap().contains("\"Karl\""));
    }

    // --- spans API (Sprint 14.1) ----------------------------------------

    const TRIGGER: &[u8] = br#"potential = {
	NOT = { has_country_flag = formed_france }
	OR = {
		tag = FRA
		culture_group = french
	}
	183 = { is_state = yes }
}"#;

    #[test]
    fn block_span_slices_the_braces_inclusive_block() {
        let (s, e) = block_span(TRIGGER, &["potential".into()]).unwrap();
        let slice = &TRIGGER[s..e];
        assert!(slice.starts_with(b"{"));
        assert!(slice.ends_with(b"}"));
        // The nested OR block is reachable and slices to itself.
        let (os, oe) = block_span(TRIGGER, &["potential".into(), "OR".into()]).unwrap();
        let or = String::from_utf8_lossy(&TRIGGER[os..oe]);
        assert!(or.contains("tag = FRA"));
        assert!(or.contains("culture_group = french"));
        assert!(!or.contains("has_country_flag"));
    }

    #[test]
    fn block_children_reports_keys_kinds_and_occurrence() {
        let kids = block_children(TRIGGER, &["potential".into()]).unwrap();
        assert_eq!(kids.len(), 3);
        assert_eq!(kids[0].key.as_deref(), Some("NOT"));
        assert!(kids[0].is_block);
        assert_eq!(kids[1].key.as_deref(), Some("OR"));
        assert_eq!(kids[2].key.as_deref(), Some("183")); // province-id scope
        // Scalar leaf children inside OR.
        let inner = block_children(TRIGGER, &["potential".into(), "OR".into()]).unwrap();
        assert_eq!(inner.len(), 2);
        assert_eq!(inner[0].key.as_deref(), Some("tag"));
        assert!(!inner[0].is_block);
        let (vs, ve) = inner[0].value_span;
        assert_eq!(&TRIGGER[vs..ve], b"FRA");
    }

    #[test]
    fn block_children_occurrence_matches_locate_addressing() {
        // Two same-key sibling blocks: occurrence 0 and 1, addressable via `#n`.
        let src = b"root = {\n\tOR = { a = 1 }\n\tOR = { b = 2 }\n}";
        let kids = block_children(src, &["root".into()]).unwrap();
        assert_eq!(kids[0].occurrence, 0);
        assert_eq!(kids[1].occurrence, 1);
        // The `#1`-addressed span is the SECOND OR block.
        let (s, e) = block_span(src, &["root".into(), "OR#1".into()]).unwrap();
        assert!(String::from_utf8_lossy(&src[s..e]).contains("b = 2"));
    }
}
