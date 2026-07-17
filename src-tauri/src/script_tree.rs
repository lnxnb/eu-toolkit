//! Sprint 14.1 — a typed tree over any Paradox/Clausewitz trigger/effect block,
//! plus the curated known-trigger / known-effect registries and the raw-editor
//! parse-validate command.
//!
//! ## Typed tree
//! Missions, decisions and events (Sprints 15–17) all edit trigger/effect
//! script. Rather than a second parser, the tree is built from the [`mod_writer`]
//! spans API (which reuses the byte-offset tokenizer): every node carries its
//! byte-surgical **path** (with `#n` occurrence ordinals on block ancestors), so
//! the frontend edits a leaf via `SetScalar { path }` and a block-valued leaf via
//! `SetBlock { path }` without ever re-parsing.
//!
//! Nodes are one of:
//!   * **group** — a scope changer (`FRA = { … }`, a province id `183 = { … }`,
//!     `ROOT`/`THIS`, an `any_/all_/every_/random_` quantifier) or a logical
//!     combinator (`AND`/`OR`/`NOT`/`NAND`/`NOR`/`hidden_trigger`/
//!     `custom_trigger_tooltip`/`if`/`limit`/`calc_true_if`). Groups recurse.
//!   * **leaf** — a `key = value` condition/effect. The value is typed
//!     (bool/number/tag/scope/quoted string) or, for `key = { … }` conditions
//!     (`num_of_owned_provinces_with`, `define_ruler`, `country_event`), a raw
//!     `block` value preserved verbatim.
//!
//! Anything unmodeled round-trips: an unedited tree emits no changes, and every
//! node keeps its `raw` statement text for the raw/tree toggle.

use crate::mod_writer::{self, ChildSpan};
use crate::vfs::Vfs;

// ---------------------------------------------------------------------------
// Tree model
// ---------------------------------------------------------------------------

/// A leaf's typed value.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TypedValue {
    /// `bool` | `number` | `tag` | `scope` | `string` | `block`.
    pub kind: String,
    /// The value text as written (quotes stripped for `string`; braces-inclusive
    /// for a `block` value).
    pub text: String,
}

/// One node of the typed script tree.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeNode {
    /// `group` | `leaf`.
    pub node_type: String,
    /// Statement key; `None` for a bare list element or an anonymous `{ … }`.
    pub key: Option<String>,
    /// Byte-surgical path from the addressed root block to this node. Block
    /// ancestors carry `#n` occurrence suffixes; a scalar leaf's final segment is
    /// the bare key (so `SetScalar { path }` applies directly). Empty when the
    /// node is a non-addressable bare/anonymous element.
    pub path: Vec<String>,
    /// Group classification (empty for leaves): `and`/`or`/`not`/`nand`/`nor`/
    /// `scope`/`quantifier`/`hidden`/`tooltip`/`control`/`calc`/`anonymous`.
    pub group_kind: String,
    /// Typed value (leaves only).
    pub value: Option<TypedValue>,
    /// Child nodes (groups only).
    pub children: Vec<TreeNode>,
    /// Raw statement text (raw/tree toggle + preserve-unknown editing).
    pub raw: String,
}

/// Classifies a keyed block as a recognized group construct, or returns `""`
/// when the block is a block-valued *leaf* (a condition/effect with a `{ … }`
/// argument, e.g. `num_of_owned_provinces_with`, `define_ruler`, `country_event`).
pub fn classify_group(key: &str) -> &'static str {
    let up = key.to_ascii_uppercase();
    match up.as_str() {
        "AND" => return "and",
        "OR" => return "or",
        "NOT" => return "not",
        "NAND" => return "nand",
        "NOR" => return "nor",
        "HIDDEN_TRIGGER" => return "hidden",
        "CUSTOM_TRIGGER_TOOLTIP" => return "tooltip",
        "IF" | "ELSE_IF" | "ELSE" | "WHILE" => return "control",
        "LIMIT" => return "limit",
        "CALC_TRUE_IF" => return "calc",
        "ROOT" | "THIS" | "FROM" | "PREV" | "PREV_PREV" | "PREV_PREV_PREV" | "FROM_FROM"
        | "OWNER" | "CONTROLLER" | "EMPEROR" | "CAPITAL_SCOPE" | "COLONIAL_PARENT" => {
            return "scope"
        }
        _ => {}
    }
    let lower = key.to_ascii_lowercase();
    if lower.starts_with("any_")
        || lower.starts_with("all_")
        || lower.starts_with("every_")
        || lower.starts_with("random_")
    {
        "quantifier"
    } else if key.len() == 3 && key.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()) {
        "scope" // country tag scope
    } else if !key.is_empty() && key.chars().all(|c| c.is_ascii_digit()) {
        "scope" // province-id scope
    } else {
        "" // block-valued leaf (or an unmodeled region/area scope → raw)
    }
}

/// Types a scalar value's raw text (quotes included for quoted strings).
fn type_value(raw: &str) -> TypedValue {
    let s = raw.trim();
    if s.starts_with('"') {
        return TypedValue {
            kind: "string".into(),
            text: s.trim_matches('"').to_string(),
        };
    }
    let lower = s.to_ascii_lowercase();
    let kind = if lower == "yes" || lower == "no" {
        "bool"
    } else if s.parse::<f64>().is_ok() {
        "number"
    } else if matches!(
        s,
        "ROOT" | "THIS" | "FROM" | "PREV" | "PREV_PREV" | "FROM_FROM" | "OWNER" | "CONTROLLER"
            | "EMPEROR"
    ) {
        "scope"
    } else if s.len() == 3 && s.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()) {
        "tag"
    } else {
        "string"
    };
    TypedValue {
        kind: kind.into(),
        text: s.to_string(),
    }
}

fn segment(key: &str, occurrence: usize) -> String {
    if occurrence > 0 {
        format!("{key}#{occurrence}")
    } else {
        key.to_string()
    }
}

/// Builds the direct child nodes of the block addressed by `root_path`.
pub fn build_nodes(src: &[u8], root_path: &[String]) -> Vec<TreeNode> {
    let Some(children) = mod_writer::block_children(src, root_path) else {
        return Vec::new();
    };
    children
        .iter()
        .map(|c| build_node(src, root_path, c))
        .collect()
}

fn slice(src: &[u8], span: (usize, usize)) -> String {
    String::from_utf8_lossy(&src[span.0..span.1]).into_owned()
}

/// Builds the child nodes of a group whose braces-inclusive `value_span` is
/// already known, by lexing ONLY that span's interior. Recursing through
/// [`build_nodes`] instead (which re-resolves `path` from the top of `src`)
/// made tree building O(nodes × file size) — a 15s stall on vanilla's
/// scripted_triggers file. Node output is identical: `path` still carries the
/// full absolute path (threaded down), and raw/value text slices are the same
/// bytes either way.
fn build_nodes_in_block(src: &[u8], value_span: (usize, usize), path: &[String]) -> Vec<TreeNode> {
    let (s, e) = value_span;
    // Defensive: expect `{ … }`. Fall back to the path-resolving walk if the
    // span isn't brace-shaped (shouldn't happen for an `is_block` child).
    if e <= s + 1 || src.get(s) != Some(&b'{') || src.get(e - 1) != Some(&b'}') {
        return build_nodes(src, path);
    }
    let interior = &src[s + 1..e - 1];
    let Some(children) = mod_writer::block_children(interior, &[]) else {
        return Vec::new();
    };
    children
        .iter()
        .map(|c| build_node(interior, path, c))
        .collect()
}

fn build_node(src: &[u8], parent_path: &[String], c: &ChildSpan) -> TreeNode {
    let raw = slice(src, c.stmt_span);
    match &c.key {
        None => {
            // Bare list element or anonymous block — not path-addressable.
            if c.is_block {
                TreeNode {
                    node_type: "group".into(),
                    key: None,
                    path: parent_path.to_vec(),
                    group_kind: "anonymous".into(),
                    value: None,
                    children: Vec::new(),
                    raw,
                }
            } else {
                TreeNode {
                    node_type: "leaf".into(),
                    key: None,
                    path: parent_path.to_vec(),
                    group_kind: String::new(),
                    value: Some(type_value(&slice(src, c.value_span))),
                    children: Vec::new(),
                    raw,
                }
            }
        }
        Some(key) => {
            if c.is_block {
                let kind = classify_group(key);
                let mut path = parent_path.to_vec();
                path.push(segment(key, c.occurrence));
                if kind.is_empty() {
                    // Block-valued leaf: preserve the `{ … }` argument verbatim.
                    TreeNode {
                        node_type: "leaf".into(),
                        key: Some(key.clone()),
                        path,
                        group_kind: String::new(),
                        value: Some(TypedValue {
                            kind: "block".into(),
                            text: slice(src, c.value_span),
                        }),
                        children: Vec::new(),
                        raw,
                    }
                } else {
                    let children = build_nodes_in_block(src, c.value_span, &path);
                    TreeNode {
                        node_type: "group".into(),
                        key: Some(key.clone()),
                        path,
                        group_kind: kind.into(),
                        value: None,
                        children,
                        raw,
                    }
                }
            } else {
                // Scalar leaf: final path segment is the bare key for SetScalar.
                let mut path = parent_path.to_vec();
                path.push(key.clone());
                TreeNode {
                    node_type: "leaf".into(),
                    key: Some(key.clone()),
                    path,
                    group_kind: String::new(),
                    value: Some(type_value(&slice(src, c.value_span))),
                    children: Vec::new(),
                    raw,
                }
            }
        }
    }
}

/// The child trees of EVERY top-level `key = { … }` block in `src`, in file
/// order, from ONE lex of the file. For loaders that want many/all definitions
/// (scripted triggers): calling [`build_nodes`] per key re-tokenizes the whole
/// file per definition — O(defs × file size), a multi-second stall on vanilla's
/// scripted_triggers. Non-block and keyless top-level statements are skipped.
pub fn build_top_level_trees(src: &[u8]) -> Vec<(String, Vec<TreeNode>)> {
    let Some(children) = mod_writer::block_children(src, &[]) else {
        return Vec::new();
    };
    children
        .iter()
        .filter_map(|c| {
            let key = c.key.clone()?;
            if !c.is_block {
                return None;
            }
            let path = vec![key.clone()];
            Some((key, build_nodes_in_block(src, c.value_span, &path)))
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Command: parse_script_block
// ---------------------------------------------------------------------------

/// The typed tree of one script block plus its raw text slice (raw/tree toggle).
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptBlock {
    /// Direct children of the addressed block.
    pub nodes: Vec<TreeNode>,
    /// Braces-inclusive raw text of the addressed block (for the raw editor).
    pub raw: String,
    /// The block's braces-inclusive byte span in the source file.
    pub span: (usize, usize),
}

/// Parses the block at `path` inside `file` into a typed tree + its raw slice.
#[tauri::command(async)]
pub fn parse_script_block(
    install_path: String,
    mod_path: Option<String>,
    file: String,
    path: Vec<String>,
) -> Result<ScriptBlock, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    let bytes = vfs.read(&file)?;
    build_script_block(&bytes, &path)
}

/// Like [`parse_script_block`], but folds the frontend's PENDING edit queue onto
/// the file first (via [`crate::edits::preview_file`]) so the tree reflects unsaved
/// state. `parse_script_block` reads the saved file; while a decision/event/mission
/// editor has queued edits (a toggled `major`, an edited leaf, a freshly-scaffolded
/// decision whose file isn't written yet), re-parsing must see them — this command
/// applies the edits targeting `file`, in queue order, to an in-memory copy before
/// building the typed tree. Mirrors how `apply_queue` folds per file.
#[tauri::command(async)]
pub fn parse_script_block_with_edits(
    install_path: String,
    mod_path: Option<String>,
    file: String,
    path: Vec<String>,
    edits: Vec<crate::edits::TypedEdit>,
) -> Result<ScriptBlock, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    let bytes = crate::edits::preview_file(&vfs, &file, &edits)?;
    build_script_block(&bytes, &path)
}

/// Pure builder (no I/O) — the testable core of [`parse_script_block`].
pub fn build_script_block(bytes: &[u8], path: &[String]) -> Result<ScriptBlock, String> {
    let span = mod_writer::block_span(bytes, path)
        .ok_or_else(|| format!("No block found at path {path:?}"))?;
    Ok(ScriptBlock {
        nodes: build_nodes(bytes, path),
        raw: String::from_utf8_lossy(&bytes[span.0..span.1]).into_owned(),
        span,
    })
}

// ---------------------------------------------------------------------------
// Command: validate_script_text (raw-editor blur validation)
// ---------------------------------------------------------------------------

/// Result of validating a raw script fragment (the raw/tree toggle blocks the
/// switch back to the tree while `valid == false`).
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptValidation {
    pub valid: bool,
    pub error: Option<String>,
}

/// Validates a raw script fragment (balanced braces/quotes, comment-aware). A
/// fragment need not be a single block — the frontend hands the raw contents of
/// one edited block, which balance-checks on their own.
#[tauri::command(async)]
pub fn validate_script_text(text: String) -> ScriptValidation {
    match validate_fragment(text.as_bytes()) {
        Ok(()) => ScriptValidation {
            valid: true,
            error: None,
        },
        Err(e) => ScriptValidation {
            valid: false,
            error: Some(e),
        },
    }
}

fn validate_fragment(src: &[u8]) -> Result<(), String> {
    let mut depth = 0i32;
    let mut line = 1usize;
    let mut i = 0;
    while i < src.len() {
        match src[i] {
            b'#' => {
                while i < src.len() && src[i] != b'\n' {
                    i += 1;
                }
            }
            b'"' => {
                i += 1;
                while i < src.len() && src[i] != b'"' {
                    if src[i] == b'\n' {
                        return Err(format!("Unterminated string on line {line}"));
                    }
                    i += 1;
                }
                if i >= src.len() {
                    return Err("Unterminated string at end of text".into());
                }
                i += 1; // closing quote
            }
            b'{' => {
                depth += 1;
                i += 1;
            }
            b'}' => {
                depth -= 1;
                if depth < 0 {
                    return Err(format!("Unmatched '}}' on line {line}"));
                }
                i += 1;
            }
            b'\n' => {
                line += 1;
                i += 1;
            }
            _ => i += 1,
        }
    }
    if depth != 0 {
        return Err(format!(
            "{depth} unclosed '{{' block{}",
            if depth == 1 { "" } else { "s" }
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Known-key registries (14.1) — curated static tables, modeled on the
// `KNOWN_MODIFIERS` pattern (registry.rs). Unknown keys still work as raw text.
// ---------------------------------------------------------------------------

/// How a known trigger/effect key's argument is entered/displayed.
#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ArgKind {
    /// `yes` / `no`.
    Bool,
    /// A number (numeric triggers compare `>=`).
    Number,
    /// A 3-char country tag or a scope reference (`ROOT`/`THIS`/`FROM`/`PREV`).
    Tag,
    /// A bare identifier / quoted string (culture, religion, flag, government…).
    String,
    /// A `{ … }` block argument.
    Block,
    /// A numeric comparison (dev/tech/etc., compared `>=`).
    Comparison,
}

/// One curated known trigger or effect key.
#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KnownKey {
    pub key: &'static str,
    pub arg_kind: ArgKind,
    pub display_name: &'static str,
}

use ArgKind::{Block, Bool, Comparison, Number, String as Str, Tag};

/// The common triggers vanilla decisions/events/missions actually use most.
/// Not exhaustive by design — unknown keys fall back to raw editing.
static KNOWN_TRIGGERS: &[KnownKey] = &[
    // Identity
    KnownKey { key: "tag", arg_kind: Tag, display_name: "Is country" },
    KnownKey { key: "exists", arg_kind: Tag, display_name: "Country exists" },
    KnownKey { key: "was_tag", arg_kind: Tag, display_name: "Was country" },
    KnownKey { key: "has_country_flag", arg_kind: Str, display_name: "Has country flag" },
    KnownKey { key: "has_global_flag", arg_kind: Str, display_name: "Has global flag" },
    KnownKey { key: "has_ruler_flag", arg_kind: Str, display_name: "Has ruler flag" },
    // Religion / culture
    KnownKey { key: "religion", arg_kind: Str, display_name: "Religion is" },
    KnownKey { key: "religion_group", arg_kind: Str, display_name: "Religion group is" },
    KnownKey { key: "secondary_religion", arg_kind: Str, display_name: "Secondary religion is" },
    KnownKey { key: "dominant_religion", arg_kind: Str, display_name: "Dominant religion is" },
    KnownKey { key: "culture", arg_kind: Str, display_name: "Culture is" },
    KnownKey { key: "primary_culture", arg_kind: Str, display_name: "Primary culture is" },
    KnownKey { key: "culture_group", arg_kind: Str, display_name: "Culture group is" },
    // Government / tech
    KnownKey { key: "government", arg_kind: Str, display_name: "Government is" },
    KnownKey { key: "government_rank", arg_kind: Number, display_name: "Government rank" },
    KnownKey { key: "technology_group", arg_kind: Str, display_name: "Tech group is" },
    KnownKey { key: "has_reform", arg_kind: Str, display_name: "Has government reform" },
    KnownKey { key: "is_emperor", arg_kind: Bool, display_name: "Is HRE emperor" },
    KnownKey { key: "is_elector", arg_kind: Bool, display_name: "Is HRE elector" },
    // Provinces / development
    KnownKey { key: "owns", arg_kind: Number, display_name: "Owns province" },
    KnownKey { key: "owns_core_province", arg_kind: Number, display_name: "Owns core province" },
    KnownKey { key: "owns_or_non_sovereign_subject_of", arg_kind: Number, display_name: "Owns/subject holds" },
    KnownKey { key: "controls", arg_kind: Number, display_name: "Controls province" },
    KnownKey { key: "capital", arg_kind: Number, display_name: "Capital is" },
    KnownKey { key: "num_of_cities", arg_kind: Comparison, display_name: "Number of provinces" },
    KnownKey { key: "num_of_ports", arg_kind: Comparison, display_name: "Number of ports" },
    KnownKey { key: "total_development", arg_kind: Comparison, display_name: "Total development" },
    KnownKey { key: "development", arg_kind: Comparison, display_name: "Development" },
    KnownKey { key: "adm", arg_kind: Comparison, display_name: "Administrative power" },
    KnownKey { key: "dip", arg_kind: Comparison, display_name: "Diplomatic power" },
    KnownKey { key: "mil", arg_kind: Comparison, display_name: "Military power" },
    KnownKey { key: "is_year", arg_kind: Number, display_name: "Is year" },
    KnownKey { key: "is_month", arg_kind: Number, display_name: "Is month" },
    // Diplomacy / subjects / war
    KnownKey { key: "is_subject", arg_kind: Bool, display_name: "Is a subject" },
    KnownKey { key: "is_subject_of", arg_kind: Tag, display_name: "Is subject of" },
    KnownKey { key: "is_subject_of_type", arg_kind: Str, display_name: "Is subject of type" },
    KnownKey { key: "overlord_of", arg_kind: Tag, display_name: "Is overlord of" },
    KnownKey { key: "is_at_war", arg_kind: Bool, display_name: "Is at war" },
    KnownKey { key: "war_with", arg_kind: Tag, display_name: "At war with" },
    KnownKey { key: "alliance_with", arg_kind: Tag, display_name: "Allied with" },
    KnownKey { key: "is_rival", arg_kind: Tag, display_name: "Rival of" },
    KnownKey { key: "is_neighbor_of", arg_kind: Tag, display_name: "Neighbours" },
    KnownKey { key: "is_free_or_tributary_trigger", arg_kind: Bool, display_name: "Free or tributary" },
    // Ruler / dynasty
    KnownKey { key: "dynasty", arg_kind: Str, display_name: "Ruling dynasty is" },
    KnownKey { key: "has_regency", arg_kind: Bool, display_name: "Has a regency" },
    KnownKey { key: "ruler_age", arg_kind: Comparison, display_name: "Ruler age" },
    KnownKey { key: "is_lesser_in_union", arg_kind: Bool, display_name: "Junior union partner" },
    // Missions / decisions / progress
    KnownKey { key: "mission_completed", arg_kind: Str, display_name: "Mission completed" },
    KnownKey { key: "has_country_modifier", arg_kind: Str, display_name: "Has country modifier" },
    KnownKey { key: "has_dlc", arg_kind: Str, display_name: "Has DLC" },
    KnownKey { key: "ai", arg_kind: Bool, display_name: "Is AI-controlled" },
    KnownKey { key: "normal_or_historical_nations", arg_kind: Bool, display_name: "Normal/historical nations" },
    // Economy / stats
    KnownKey { key: "treasury", arg_kind: Comparison, display_name: "Treasury" },
    KnownKey { key: "stability", arg_kind: Comparison, display_name: "Stability" },
    KnownKey { key: "prestige", arg_kind: Comparison, display_name: "Prestige" },
    KnownKey { key: "legitimacy", arg_kind: Comparison, display_name: "Legitimacy" },
    KnownKey { key: "manpower", arg_kind: Comparison, display_name: "Manpower (k)" },
    KnownKey { key: "army_size", arg_kind: Comparison, display_name: "Army size" },
];

/// The common effects vanilla decisions/events/missions actually use most.
static KNOWN_EFFECTS: &[KnownKey] = &[
    // Powers / resources
    KnownKey { key: "add_prestige", arg_kind: Number, display_name: "Add prestige" },
    KnownKey { key: "add_stability", arg_kind: Number, display_name: "Add stability" },
    KnownKey { key: "add_stability_or_adm_power", arg_kind: Number, display_name: "Add stability or ADM" },
    KnownKey { key: "add_treasury", arg_kind: Number, display_name: "Add ducats" },
    KnownKey { key: "add_adm_power", arg_kind: Number, display_name: "Add ADM power" },
    KnownKey { key: "add_dip_power", arg_kind: Number, display_name: "Add DIP power" },
    KnownKey { key: "add_mil_power", arg_kind: Number, display_name: "Add MIL power" },
    KnownKey { key: "add_legitimacy", arg_kind: Number, display_name: "Add legitimacy" },
    KnownKey { key: "add_republican_tradition", arg_kind: Number, display_name: "Add republican tradition" },
    KnownKey { key: "add_army_tradition", arg_kind: Number, display_name: "Add army tradition" },
    KnownKey { key: "add_navy_tradition", arg_kind: Number, display_name: "Add navy tradition" },
    KnownKey { key: "add_war_exhaustion", arg_kind: Number, display_name: "Add war exhaustion" },
    KnownKey { key: "add_mercantilism", arg_kind: Number, display_name: "Add mercantilism" },
    KnownKey { key: "add_manpower", arg_kind: Number, display_name: "Add manpower (k)" },
    KnownKey { key: "add_absolutism", arg_kind: Number, display_name: "Add absolutism" },
    KnownKey { key: "add_adm_tech", arg_kind: Number, display_name: "Add ADM tech" },
    KnownKey { key: "change_adm", arg_kind: Number, display_name: "Change ruler ADM" },
    // Provinces / development
    KnownKey { key: "add_core", arg_kind: Tag, display_name: "Add core" },
    KnownKey { key: "remove_core", arg_kind: Tag, display_name: "Remove core" },
    KnownKey { key: "add_permanent_claim", arg_kind: Tag, display_name: "Add permanent claim" },
    KnownKey { key: "add_claim", arg_kind: Tag, display_name: "Add claim" },
    KnownKey { key: "cede_province", arg_kind: Tag, display_name: "Cede province to" },
    KnownKey { key: "add_base_tax", arg_kind: Number, display_name: "Add base tax" },
    KnownKey { key: "add_base_production", arg_kind: Number, display_name: "Add base production" },
    KnownKey { key: "add_base_manpower", arg_kind: Number, display_name: "Add base manpower" },
    KnownKey { key: "transfer_development", arg_kind: Block, display_name: "Transfer development" },
    // Country identity
    KnownKey { key: "change_tag", arg_kind: Tag, display_name: "Change tag to" },
    KnownKey { key: "change_religion", arg_kind: Str, display_name: "Change religion" },
    KnownKey { key: "change_primary_culture", arg_kind: Str, display_name: "Change primary culture" },
    KnownKey { key: "add_accepted_culture", arg_kind: Str, display_name: "Add accepted culture" },
    KnownKey { key: "change_government", arg_kind: Str, display_name: "Change government" },
    KnownKey { key: "add_government_reform", arg_kind: Str, display_name: "Add government reform" },
    KnownKey { key: "set_government_rank", arg_kind: Number, display_name: "Set government rank" },
    KnownKey { key: "define_ruler", arg_kind: Block, display_name: "Define ruler" },
    KnownKey { key: "define_heir", arg_kind: Block, display_name: "Define heir" },
    KnownKey { key: "define_consort", arg_kind: Block, display_name: "Define consort" },
    // Flags / modifiers / events
    KnownKey { key: "set_country_flag", arg_kind: Str, display_name: "Set country flag" },
    KnownKey { key: "clr_country_flag", arg_kind: Str, display_name: "Clear country flag" },
    KnownKey { key: "set_global_flag", arg_kind: Str, display_name: "Set global flag" },
    KnownKey { key: "add_country_modifier", arg_kind: Block, display_name: "Add country modifier" },
    KnownKey { key: "remove_country_modifier", arg_kind: Str, display_name: "Remove country modifier" },
    KnownKey { key: "country_event", arg_kind: Block, display_name: "Trigger country event" },
    KnownKey { key: "province_event", arg_kind: Block, display_name: "Trigger province event" },
    KnownKey { key: "complete_mission", arg_kind: Str, display_name: "Complete mission" },
    // Diplomacy / war
    KnownKey { key: "add_opinion", arg_kind: Block, display_name: "Add opinion" },
    KnownKey { key: "create_alliance", arg_kind: Tag, display_name: "Create alliance" },
    KnownKey { key: "declare_war", arg_kind: Tag, display_name: "Declare war" },
    KnownKey { key: "release", arg_kind: Tag, display_name: "Release nation" },
    KnownKey { key: "vassalize", arg_kind: Tag, display_name: "Vassalize" },
    KnownKey { key: "inherit", arg_kind: Tag, display_name: "Inherit" },
];

/// The curated known-trigger list.
#[cfg(test)]
pub fn known_triggers() -> &'static [KnownKey] {
    KNOWN_TRIGGERS
}

/// The curated known-effect list.
#[cfg(test)]
pub fn known_effects() -> &'static [KnownKey] {
    KNOWN_EFFECTS
}

#[tauri::command(async)]
pub fn get_known_triggers() -> Vec<KnownKey> {
    KNOWN_TRIGGERS.to_vec()
}

#[tauri::command(async)]
pub fn get_known_effects() -> Vec<KnownKey> {
    KNOWN_EFFECTS.to_vec()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mod_writer::{apply, Edit};

    // FrenchNation-shaped decision: nested OR/AND/NOT, a province-id scope, a
    // block-valued leaf, and effect statements.
    const DECISION: &[u8] = br#"country_decisions = {
	form_france = {
		major = yes
		potential = {
			NOT = { has_country_flag = formed_france_flag }
			OR = {
				culture_group = french
				primary_culture = cosmopolitan_french
			}
			is_year = 1500
		}
		allow = {
			is_subject = no
			num_of_owned_provinces_with = {
				value = 10
				culture_group = french
				is_core = ROOT
			}
			owns_core_province = 183
		}
		effect = {
			change_tag = FRA
			add_prestige = 25
			183 = { add_permanent_claim = FRA }
		}
	}
}"#;

    fn potential_path() -> Vec<String> {
        vec![
            "country_decisions".into(),
            "form_france".into(),
            "potential".into(),
        ]
    }

    #[test]
    fn parses_nested_logical_and_scope_shapes() {
        let block = build_script_block(DECISION, &potential_path()).unwrap();
        assert_eq!(block.nodes.len(), 3);
        // NOT group with a single leaf child.
        let not = &block.nodes[0];
        assert_eq!(not.node_type, "group");
        assert_eq!(not.group_kind, "not");
        assert_eq!(not.children.len(), 1);
        assert_eq!(not.children[0].key.as_deref(), Some("has_country_flag"));
        // OR group with two culture leaves.
        let or = &block.nodes[1];
        assert_eq!(or.group_kind, "or");
        assert_eq!(or.children.len(), 2);
        assert_eq!(or.children[0].value.as_ref().unwrap().text, "french");
        // is_year leaf typed as a number.
        let year = &block.nodes[2];
        assert_eq!(year.node_type, "leaf");
        assert_eq!(year.value.as_ref().unwrap().kind, "number");
    }

    #[test]
    fn block_valued_leaf_and_province_scope() {
        let allow = vec![
            "country_decisions".into(),
            "form_france".into(),
            "allow".into(),
        ];
        let block = build_script_block(DECISION, &allow).unwrap();
        // num_of_owned_provinces_with is a block-valued LEAF, preserved raw.
        let nowp = block
            .nodes
            .iter()
            .find(|n| n.key.as_deref() == Some("num_of_owned_provinces_with"))
            .unwrap();
        assert_eq!(nowp.node_type, "leaf");
        let v = nowp.value.as_ref().unwrap();
        assert_eq!(v.kind, "block");
        assert!(v.text.contains("value = 10"));
        assert!(v.text.contains("is_core = ROOT"));

        // Province-id scope in the effect is a GROUP that recurses.
        let effect = vec![
            "country_decisions".into(),
            "form_france".into(),
            "effect".into(),
        ];
        let eff = build_script_block(DECISION, &effect).unwrap();
        let prov = eff.nodes.iter().find(|n| n.key.as_deref() == Some("183")).unwrap();
        assert_eq!(prov.group_kind, "scope");
        assert_eq!(prov.children[0].key.as_deref(), Some("add_permanent_claim"));
        assert_eq!(prov.children[0].value.as_ref().unwrap().kind, "tag");
    }

    #[test]
    fn unedited_tree_emits_no_changes_and_leaf_edit_is_byte_surgical() {
        // Building the tree does not touch the file (round-trip is free).
        let block = build_script_block(DECISION, &potential_path()).unwrap();
        // Edit the is_year leaf via its emitted path; only that value changes.
        let year = block.nodes.iter().find(|n| n.key.as_deref() == Some("is_year")).unwrap();
        let out = apply(
            DECISION,
            &Edit::SetScalar {
                path: year.path.clone(),
                value: "1450".into(),
                quoted: false,
            },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("is_year = 1450"));
        // Everything else intact.
        assert!(text.contains("has_country_flag = formed_france_flag"));
        assert!(text.contains("owns_core_province = 183"));
        assert!(text.contains("183 = { add_permanent_claim = FRA }"));
    }

    #[test]
    fn nested_leaf_edit_uses_occurrence_qualified_path() {
        // The OR's second leaf (primary_culture) is edited through its path,
        // which threads the group ancestors' occurrence-qualified segments.
        let block = build_script_block(DECISION, &potential_path()).unwrap();
        let or = block.nodes.iter().find(|n| n.group_kind == "or").unwrap();
        let leaf = &or.children[1];
        assert_eq!(leaf.key.as_deref(), Some("primary_culture"));
        let out = apply(
            DECISION,
            &Edit::SetScalar {
                path: leaf.path.clone(),
                value: "old_frankish".into(),
                quoted: false,
            },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("primary_culture = old_frankish"));
        assert!(text.contains("culture_group = french"));
    }

    #[test]
    fn validate_accepts_good_and_rejects_bad_fragment() {
        assert!(validate_script_text("tag = FRA\nOR = { a = 1 b = 2 }".into()).valid);
        // Comment/quote-aware: a brace in a comment or string is not counted.
        assert!(validate_script_text("name = \"a { b\" # trailing }".into()).valid);
        let bad = validate_script_text("OR = { a = 1 ".into());
        assert!(!bad.valid);
        assert!(bad.error.unwrap().contains("unclosed"));
        let extra = validate_script_text("a = 1 } }".into());
        assert!(!extra.valid);
    }

    #[test]
    fn known_registries_cover_common_keys() {
        let trig: Vec<&str> = known_triggers().iter().map(|k| k.key).collect();
        for k in ["tag", "religion", "owns_core_province", "is_year", "num_of_cities", "war_with"] {
            assert!(trig.contains(&k), "missing trigger {k}");
        }
        let eff: Vec<&str> = known_effects().iter().map(|k| k.key).collect();
        for k in ["add_prestige", "add_core", "set_country_flag", "country_event", "change_tag"] {
            assert!(eff.contains(&k), "missing effect {k}");
        }
        // No duplicate keys within each table.
        let mut t = trig.clone();
        t.sort_unstable();
        let n = t.len();
        t.dedup();
        assert_eq!(n, t.len(), "duplicate trigger keys");
    }

    #[test]
    fn build_script_block_missing_path_errors() {
        assert!(build_script_block(DECISION, &["nope".into()]).is_err());
    }

    #[test]
    fn with_edits_reflects_pending_queue_before_parsing() {
        // parse_script_block_with_edits previews the pending edits, so a queued
        // leaf change shows in the freshly-built tree even though nothing is saved.
        use crate::edits::{preview_file, TypedEdit};
        use crate::vfs::Vfs;
        let root = std::env::temp_dir().join("eu_toolkit_script_with_edits_test");
        let _ = std::fs::remove_dir_all(&root);
        let base = root.join("base");
        std::fs::create_dir_all(base.join("map")).unwrap();
        std::fs::write(base.join("map/provinces.bmp"), b"x").unwrap();
        std::fs::create_dir_all(base.join("decisions")).unwrap();
        let rel = "decisions/Demo.txt";
        std::fs::write(
            base.join(rel),
            b"country_decisions = {\n\td = {\n\t\tpotential = { is_year = 1500 }\n\t}\n}\n",
        )
        .unwrap();
        let vfs = Vfs::new(base.to_str().unwrap(), None).unwrap();

        let path: Vec<String> =
            vec!["country_decisions".into(), "d".into(), "potential".into()];
        let edits = vec![TypedEdit::SetScalar {
            file: rel.into(),
            path: {
                let mut p = path.clone();
                p.push("is_year".into());
                p
            },
            value: "1600".into(),
            quoted: false,
        }];
        let previewed = preview_file(&vfs, rel, &edits).unwrap();
        let block = build_script_block(&previewed, &path).unwrap();
        let year = block.nodes.iter().find(|n| n.key.as_deref() == Some("is_year")).unwrap();
        assert_eq!(year.value.as_ref().unwrap().text, "1600");
    }
}
