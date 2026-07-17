//! Sprint 28 — scripted triggers / scripted effects browser + name registry.
//!
//! `common/scripted_triggers/*.txt` and `common/scripted_effects/*.txt` each hold
//! a flat series of `<name> = { <body> }` definitions. The body is trigger-shaped
//! (scripted_triggers) or effect-shaped (scripted_effects) and may contain
//! `$PARAMETER$` meta-script tokens pasted in at each call site.
//!
//! ## Why this matters beyond a browser
//! Every 14.2 condition/effect tree anywhere in the app renders an unmodeled key
//! as a raw row. A great many of those "unknown" keys are actually calls to a
//! scripted trigger/effect (`has_mil_advisor = yes`, `add_loot_from_province_effect
//! = yes`). This module is the registry the frontend uses to resolve such a name
//! to its DEFINITION so the tree renders it as a jump-LINK instead. Mod-defined
//! scripted names resolve too, because we scan through the [`Vfs`] (base + mod).
//!
//! The body itself is edited through the existing `parse_script_block` machinery
//! at path `[name]` — no second editor. This module only enumerates definitions
//! (name, kind, file, origin, path, `$PARAM$` tokens) + a create scaffold.

use crate::vfs::Vfs;

/// Directory + kind pairs scanned for definitions.
const DIRS: [(&str, &str); 2] = [
    ("common/scripted_triggers", "trigger"),
    ("common/scripted_effects", "effect"),
];

/// Toolkit-owned project files new definitions scaffold into.
pub const TRIGGERS_FILE: &str = "common/scripted_triggers/zz_eutoolkit_scripted_triggers.txt";
pub const EFFECTS_FILE: &str = "common/scripted_effects/zz_eutoolkit_scripted_effects.txt";

/// One scripted trigger or scripted effect definition.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptedDef {
    /// The definition name (`has_mil_advisor`), also the call-site key.
    pub name: String,
    /// `trigger` | `effect`.
    pub kind: String,
    /// Game-relative file the definition was found in.
    pub file: String,
    /// `base` | `mod`.
    pub origin: String,
    /// Byte-surgical path to the definition body (`[name]`) for the 14.2 editor.
    pub path: Vec<String>,
    /// Distinct `$PARAM$` meta-script tokens referenced in the body (raw display).
    pub params: Vec<String>,
    /// Line count of the body (a cheap size hint for the list).
    pub line_count: usize,
}

/// Scans `text` for distinct `$WORD$` meta-script tokens (returned without the
/// surrounding `$`, first-seen order preserved).
fn scan_params(text: &str) -> Vec<String> {
    let bytes = text.as_bytes();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' {
            let start = i + 1;
            let mut j = start;
            while j < bytes.len()
                && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_')
            {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b'$' && j > start {
                let tok = &text[start..j];
                if !out.iter().any(|t| t == tok) {
                    out.push(tok.to_string());
                }
                i = j + 1;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// Collects every scripted trigger/effect definition across the Vfs-merged dirs.
pub fn load_scripted(vfs: &Vfs) -> Vec<ScriptedDef> {
    let mod_dir = vfs.mod_dir();
    let mut out = Vec::new();
    for (dir, kind) in DIRS {
        for (file_name, path) in vfs.list_dir(dir) {
            if !file_name.to_lowercase().ends_with(".txt") {
                continue;
            }
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            let origin = if mod_dir.is_some_and(|m| path.starts_with(m)) {
                "mod"
            } else {
                "base"
            };
            let rel = format!("{dir}/{file_name}");
            // Top-level `<name> = { … }` blocks are the definitions.
            for c in crate::mod_writer::block_children(&bytes, &[]).unwrap_or_default() {
                let (Some(name), true) = (&c.key, c.is_block) else {
                    continue;
                };
                if c.occurrence > 0 {
                    // A duplicate name in one file: the earlier one wins in game;
                    // list only the first occurrence (path stays bare `[name]`).
                    continue;
                }
                let body = String::from_utf8_lossy(&bytes[c.value_span.0..c.value_span.1]);
                out.push(ScriptedDef {
                    name: name.clone(),
                    kind: kind.to_string(),
                    file: rel.clone(),
                    origin: origin.to_string(),
                    path: vec![name.clone()],
                    params: scan_params(&body),
                    line_count: body.lines().count().max(1),
                });
            }
        }
    }
    out
}

/// Tauri command: every scripted trigger + scripted effect (base + mod). The
/// frontend uses this both for the browser list AND to build the name→definition
/// map that resolves references in every 14.2 tree into jump links.
#[tauri::command(async)]
pub fn get_scripted_definitions(
    install_path: String,
    mod_path: Option<String>,
) -> Result<Vec<ScriptedDef>, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    Ok(load_scripted(&vfs))
}

/// A create scaffold: the file to write and the statement to insert.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptedScaffold {
    pub file: String,
    pub statement: String,
}

/// Tauri command: scaffold a new scripted trigger/effect. Returns the toolkit
/// project file + the `<name> = { }` statement the frontend queues (InsertStatement
/// into an existing toolkit file, else CreateFile). `kind` = `trigger`|`effect`.
#[tauri::command(async)]
pub fn scaffold_scripted(kind: String, name: String) -> Result<ScriptedScaffold, String> {
    let name = name.trim();
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err("Name must be a bare identifier (letters, digits, underscore).".into());
    }
    let file = match kind.as_str() {
        "trigger" => TRIGGERS_FILE,
        "effect" => EFFECTS_FILE,
        _ => return Err(format!("Unknown scripted kind: {kind}")),
    };
    let body = if kind == "trigger" {
        "\talways = yes\n"
    } else {
        "\tadd_prestige = 0\n"
    };
    Ok(ScriptedScaffold {
        file: file.to_string(),
        statement: format!("{name} = {{\n{body}}}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    const INSTALL: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Europa Universalis IV";
    const ANBENNAR: &str = r"C:\Users\User\projects\anbennar-eu4-dev";

    fn install_present() -> bool {
        Path::new(INSTALL).join("map/provinces.bmp").is_file()
    }

    fn synthetic(name: &str, files: &[(&str, &str)]) -> (std::path::PathBuf, Vfs) {
        let root = std::env::temp_dir().join(format!("eu_toolkit_scripted_test_{name}"));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("map")).unwrap();
        std::fs::write(root.join("map/provinces.bmp"), b"x").unwrap();
        for (rel, contents) in files {
            let p = root.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, contents).unwrap();
        }
        let vfs = Vfs::new(root.to_str().unwrap(), None).unwrap();
        (root, vfs)
    }

    #[test]
    fn scans_params_distinct_in_order() {
        let p = scan_params("has_country_modifier = $ARG1$_x\n$ARG2$ = { $ARG1$ }\nprice = $10 dollars");
        assert_eq!(p, vec!["ARG1", "ARG2"]);
    }

    #[test]
    fn loads_trigger_and_effect_defs_with_params() {
        let (_root, vfs) = synthetic(
            "load",
            &[
                (
                    "common/scripted_triggers/00_t.txt",
                    "# comment\nhas_mil_advisor = {\n\tOR = { advisor = army_organiser }\n}\nmeta_t = {\n\thas_country_modifier = $ARG1$\n}\n",
                ),
                (
                    "common/scripted_effects/00_e.txt",
                    "add_loot_from_province_effect = {\n\tadd_prestige = 1\n}\n",
                ),
            ],
        );
        let defs = load_scripted(&vfs);
        assert_eq!(defs.len(), 3);
        let t = defs.iter().find(|d| d.name == "has_mil_advisor").unwrap();
        assert_eq!(t.kind, "trigger");
        assert_eq!(t.path, vec!["has_mil_advisor"]);
        assert!(t.params.is_empty());
        let meta = defs.iter().find(|d| d.name == "meta_t").unwrap();
        assert_eq!(meta.params, vec!["ARG1"]);
        let e = defs.iter().find(|d| d.name == "add_loot_from_province_effect").unwrap();
        assert_eq!(e.kind, "effect");
    }

    #[test]
    fn def_body_parses_through_spans_api() {
        // The emitted `[name]` path resolves through the same spans API the 14.2
        // tree editor uses, so the browser can parse each body for editing.
        let (_root, vfs) = synthetic(
            "spans",
            &[(
                "common/scripted_triggers/00_t.txt",
                "has_mil_advisor = {\n\tOR = { advisor = army_organiser }\n}\n",
            )],
        );
        let defs = load_scripted(&vfs);
        let d = &defs[0];
        let bytes = vfs.read(&d.file).unwrap();
        let block = crate::script_tree::build_script_block(&bytes, &d.path).unwrap();
        assert!(block.nodes.iter().any(|n| n.group_kind == "or"));
    }

    #[test]
    fn scaffold_parses_back() {
        let sc = scaffold_scripted("trigger".into(), "my_new_trigger".into()).unwrap();
        assert_eq!(sc.file, TRIGGERS_FILE);
        let b = crate::paradox::parse(&sc.statement);
        assert!(b.get_block("my_new_trigger").is_some());
        // effect variant.
        let sc = scaffold_scripted("effect".into(), "my_new_effect".into()).unwrap();
        assert_eq!(sc.file, EFFECTS_FILE);
        // bad name rejected.
        assert!(scaffold_scripted("trigger".into(), "bad name!".into()).is_err());
        assert!(scaffold_scripted("nope".into(), "x".into()).is_err());
    }

    #[test]
    fn tree_reference_resolves_to_definition_incl_mod_defined() {
        // The link-resolution contract: a 14.2 tree that references a scripted
        // trigger by name resolves that leaf's key to its definition — for a
        // base-defined AND a mod-defined name (the frontend's resolveScripted).
        let root = std::env::temp_dir().join("eu_toolkit_scripted_link_test");
        let _ = std::fs::remove_dir_all(&root);
        let base = root.join("base");
        let m = root.join("mod");
        std::fs::create_dir_all(base.join("map")).unwrap();
        std::fs::write(base.join("map/provinces.bmp"), b"x").unwrap();
        std::fs::create_dir_all(base.join("common/scripted_triggers")).unwrap();
        std::fs::write(
            base.join("common/scripted_triggers/00_t.txt"),
            "base_scripted_trigger = {\n\talways = yes\n}\n",
        )
        .unwrap();
        std::fs::create_dir_all(m.join("common/scripted_triggers")).unwrap();
        std::fs::write(
            m.join("common/scripted_triggers/zz_mod.txt"),
            "mod_scripted_trigger = {\n\ttag = FRA\n}\n",
        )
        .unwrap();
        std::fs::write(m.join("descriptor.mod"), "name=\"m\"\n").unwrap();
        let vfs = Vfs::new(base.to_str().unwrap(), Some(m.to_str().unwrap())).unwrap();

        // Build the registry map the frontend consumes.
        let defs = load_scripted(&vfs);
        let by_name: std::collections::HashMap<&str, &ScriptedDef> =
            defs.iter().map(|d| (d.name.as_str(), d)).collect();

        // A decision whose potential references both scripted names + a plain key.
        let decision = br#"country_decisions = {
	my_decision = {
		potential = {
			base_scripted_trigger = yes
			mod_scripted_trigger = yes
			is_year = 1500
		}
	}
}"#;
        let nodes = crate::script_tree::build_nodes(
            decision,
            &["country_decisions".into(), "my_decision".into(), "potential".into()],
        );
        // Each leaf key: a scripted name resolves to a jump target; a plain key does not.
        let base_ref = nodes.iter().find(|n| n.key.as_deref() == Some("base_scripted_trigger")).unwrap();
        let resolved = by_name.get(base_ref.key.as_deref().unwrap()).unwrap();
        assert_eq!(resolved.kind, "trigger");
        assert_eq!(resolved.origin, "base");
        assert_eq!(resolved.path, vec!["base_scripted_trigger"]);

        let mod_ref = nodes.iter().find(|n| n.key.as_deref() == Some("mod_scripted_trigger")).unwrap();
        let resolved = by_name.get(mod_ref.key.as_deref().unwrap()).unwrap();
        assert_eq!(resolved.origin, "mod", "mod-defined scripted name resolves");

        // A plain (non-scripted) key does NOT resolve to a definition.
        assert!(by_name.get("is_year").is_none());
    }

    #[test]
    fn vanilla_scripted_smoke() {
        if !install_present() {
            return;
        }
        let vfs = Vfs::new(INSTALL, None).unwrap();
        let defs = load_scripted(&vfs);
        // Vanilla ships hundreds of scripted triggers + effects.
        assert!(defs.len() > 300, "expected many scripted defs, got {}", defs.len());
        assert!(defs.iter().any(|d| d.name == "has_mil_advisor" && d.kind == "trigger"));
        assert!(defs.iter().any(|d| d.kind == "effect"));
        // Every def body parses through the spans API.
        for d in defs.iter().take(50) {
            let bytes = vfs.read(&d.file).unwrap();
            assert!(
                crate::script_tree::build_script_block(&bytes, &d.path).is_ok(),
                "{} body must parse",
                d.name
            );
        }
    }

    #[test]
    fn anbennar_scripted_smoke() {
        if !install_present() || !Path::new(ANBENNAR).is_dir() {
            return;
        }
        let vfs = Vfs::new(INSTALL, Some(ANBENNAR)).unwrap();
        let defs = load_scripted(&vfs);
        assert!(!defs.is_empty());
        // Anbennar ships massive scripted content; at least one mod-origin def.
        assert!(defs.iter().any(|d| d.origin == "mod"), "Anbennar contributes mod scripted defs");
        // One mod-defined def resolves + its body parses (link-resolution target).
        let mod_def = defs.iter().find(|d| d.origin == "mod").unwrap();
        let bytes = vfs.read(&mod_def.file).unwrap();
        assert!(crate::script_tree::build_script_block(&bytes, &mod_def.path).is_ok());
    }
}
