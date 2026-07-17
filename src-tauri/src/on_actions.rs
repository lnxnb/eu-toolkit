//! Sprint 28 — on_actions editor (View ▸ On Actions).
//!
//! `common/on_actions/*.txt` wire game events to engine hooks. Each file holds a
//! flat series of `<hook_name> = { <effect body> }` blocks (`on_startup`,
//! `on_religion_change`, `on_battle_won`, …). A hook body is effect-shaped and,
//! besides direct effects + scripted-effect calls, may carry:
//!   * `events = { <id> <id> … }` — a bare list of event ids fired unconditionally.
//!   * `random_events = { <weight> = <id> … }` — weighted event ids (`0` = nothing).
//!
//! The same hook may appear across several files (the game concatenates them); we
//! surface each occurrence separately, keyed by its own file + occurrence path so
//! byte-surgical editing stays unambiguous. The effect body is edited through the
//! existing 14.2 `parse_script_block` machinery at path `[hook]`; this module adds
//! the hook list + the typed events / random_events rows the editor renders with
//! event-id pickers.
//!
//! It also FEEDS Sprint 16's "referenced from" scan: [`crate::events::scan_references`]
//! now also reports on_action hooks that fire an event (see `scan_on_action_refs`).

use crate::paradox::{self, Block, Value};
use crate::vfs::Vfs;

/// The game location holding on_action files.
pub const ON_ACTIONS_DIR: &str = "common/on_actions";

/// Toolkit-owned project file new hooks scaffold into.
pub const PROJECT_FILE: &str = "common/on_actions/zz_eutoolkit_on_actions.txt";

/// One weighted entry of a `random_events` block (`<weight> = <id>`).
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeightedEvent {
    /// The weight (as written — usually an integer; `0` is "nothing happens").
    pub weight: String,
    /// The event id (or `0` for the nothing slot).
    pub id: String,
}

/// One engine-hook block, ready for the overlay list + editor.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OnActionHook {
    /// The hook name (`on_startup`).
    pub hook: String,
    /// Game-relative file the hook was found in.
    pub file: String,
    /// `base` | `mod`.
    pub origin: String,
    /// Byte-surgical path to the hook block (occurrence-qualified: `on_startup#n`).
    pub path: Vec<String>,
    /// Path to the `events` bare-id list (present iff `has_events`).
    pub events_path: Vec<String>,
    /// Path to the `random_events` block (present iff `has_random_events`).
    pub random_events_path: Vec<String>,
    /// Unconditional event ids in `events = { … }`.
    pub events: Vec<String>,
    /// Weighted ids in `random_events = { … }`.
    pub random_events: Vec<WeightedEvent>,
    pub has_events: bool,
    pub has_random_events: bool,
    /// Count of the remaining (non-events) statements in the body — a size hint.
    pub effect_count: usize,
}

/// A `#n` occurrence-qualified path segment (bare when the occurrence is 0).
fn segment(key: &str, occ: usize) -> String {
    if occ > 0 {
        format!("{key}#{occ}")
    } else {
        key.to_string()
    }
}

/// The bare event ids of an `events = { … }` list block.
fn events_of(hook: &Block) -> Vec<String> {
    match hook.get_block("events") {
        None => Vec::new(),
        Some(b) => b
            .items
            .iter()
            .filter_map(|(k, v)| match (k, v) {
                // Bare list elements are keyless scalars.
                (None, Value::Scalar(s)) => Some(s.clone()),
                _ => None,
            })
            .collect(),
    }
}

/// The `<weight> = <id>` entries of a `random_events = { … }` block.
fn random_events_of(hook: &Block) -> Vec<WeightedEvent> {
    match hook.get_block("random_events") {
        None => Vec::new(),
        Some(b) => b
            .items
            .iter()
            .filter_map(|(k, v)| match (k, v) {
                (Some(w), Value::Scalar(id)) => Some(WeightedEvent {
                    weight: w.clone(),
                    id: id.clone(),
                }),
                _ => None,
            })
            .collect(),
    }
}

/// Collects every hook from one parsed file into `out`.
fn collect_file(block: &Block, file: &str, origin: &str, out: &mut Vec<OnActionHook>) {
    let mut occ: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for (key, value) in &block.items {
        let (Some(k), Value::Block(hook)) = (key, value) else {
            continue;
        };
        let n = occ.entry(k.as_str()).or_insert(0);
        let seg = segment(k, *n);
        *n += 1;

        let path = vec![seg];
        let sub = |name: &str| {
            let mut p = path.clone();
            p.push(name.to_string());
            p
        };
        let events = events_of(hook);
        let random_events = random_events_of(hook);
        let has_events = hook.get_block("events").is_some();
        let has_random_events = hook.get_block("random_events").is_some();
        let effect_count = hook
            .items
            .iter()
            .filter(|(k, _)| !matches!(k.as_deref(), Some("events") | Some("random_events")))
            .count();

        out.push(OnActionHook {
            hook: k.clone(),
            file: file.to_string(),
            origin: origin.to_string(),
            events_path: sub("events"),
            random_events_path: sub("random_events"),
            events,
            random_events,
            has_events,
            has_random_events,
            effect_count,
            path,
        });
    }
}

/// Loads every on_action hook across the VFS-merged `common/on_actions/` files.
pub fn load_on_actions(vfs: &Vfs) -> Vec<OnActionHook> {
    let mod_dir = vfs.mod_dir();
    let mut out = Vec::new();
    for (file_name, path) in vfs.list_dir(ON_ACTIONS_DIR) {
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
        let block = paradox::parse(&String::from_utf8_lossy(&bytes));
        let rel = format!("{ON_ACTIONS_DIR}/{file_name}");
        collect_file(&block, &rel, origin, &mut out);
    }
    out
}

/// Tauri command: list all on_action hooks (base + mod) for the overlay.
#[tauri::command]
pub fn get_on_actions(
    install_path: String,
    mod_path: Option<String>,
) -> Result<Vec<OnActionHook>, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    Ok(load_on_actions(&vfs))
}

/// A create scaffold: the file to write and the statement to insert.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OnActionScaffold {
    pub file: String,
    pub statement: String,
}

/// Tauri command: scaffold a new (or override) on_action hook block. Returns the
/// toolkit project file + a `<hook> = { events = { } }` statement the frontend
/// queues (InsertStatement into an existing toolkit file, else CreateFile).
#[tauri::command]
pub fn scaffold_on_action(hook: String) -> Result<OnActionScaffold, String> {
    let hook = hook.trim();
    if hook.is_empty() || !hook.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err("Hook must be a bare identifier (letters, digits, underscore).".into());
    }
    Ok(OnActionScaffold {
        file: PROJECT_FILE.to_string(),
        statement: format!("{hook} = {{\n\tevents = {{\n\t}}\n}}"),
    })
}

// ---------------------------------------------------------------------------
// References scan (feeds events::scan_references — an event's "referenced from"
// now also reports the on_action hooks that fire it).
// ---------------------------------------------------------------------------

/// One on_action hook that fires an event `id` (a jump-link target).
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OnActionRef {
    /// Game-relative on_action file the firing hook lives in.
    pub file: String,
    /// `base` | `mod`.
    pub origin: String,
    /// The hook name that fires the event.
    pub hook: String,
    /// `events` (unconditional) | `random_events` (weighted).
    pub via: String,
}

/// Scans `common/on_actions/` for every hook that fires the event `id` via its
/// `events` or `random_events` list. Used by [`crate::events::scan_references`].
pub fn scan_on_action_refs(vfs: &Vfs, id: &str) -> Vec<OnActionRef> {
    let mut out = Vec::new();
    for hook in load_on_actions(vfs) {
        if hook.events.iter().any(|e| e == id) {
            out.push(OnActionRef {
                file: hook.file.clone(),
                origin: hook.origin.clone(),
                hook: hook.hook.clone(),
                via: "events".to_string(),
            });
        }
        if hook.random_events.iter().any(|w| w.id == id) {
            out.push(OnActionRef {
                file: hook.file.clone(),
                origin: hook.origin.clone(),
                hook: hook.hook.clone(),
                via: "random_events".to_string(),
            });
        }
    }
    out
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

    const SAMPLE: &[u8] = br#"# country
on_startup = {
	save_global_event_target_as = Emperor
	events = {
		muslim_school_events.20 #Pick School
		flavor_mng.42
	}
	some_scripted_effect = yes
}

on_battle_won_by_country = {
	random_events = {
		90 = 0
		10 = friendship_events.1
	}
}

on_startup = {
	events = { extra.1 }
}
"#;

    fn parse_sample() -> Vec<OnActionHook> {
        let block = paradox::parse(&String::from_utf8_lossy(SAMPLE));
        let mut out = Vec::new();
        collect_file(&block, "common/on_actions/00.txt", "base", &mut out);
        out
    }

    #[test]
    fn collects_hooks_events_and_random_events() {
        let hooks = parse_sample();
        assert_eq!(hooks.len(), 3);

        let startup = &hooks[0];
        assert_eq!(startup.hook, "on_startup");
        assert_eq!(startup.path, vec!["on_startup"]);
        assert!(startup.has_events);
        assert_eq!(startup.events, vec!["muslim_school_events.20", "flavor_mng.42"]);
        // `save_global_event_target_as` + `some_scripted_effect` = 2 remaining stmts.
        assert_eq!(startup.effect_count, 2);
        assert_eq!(startup.events_path, vec!["on_startup", "events"]);

        let battle = &hooks[1];
        assert_eq!(battle.hook, "on_battle_won_by_country");
        assert!(battle.has_random_events);
        assert_eq!(battle.random_events.len(), 2);
        assert_eq!(battle.random_events[0].weight, "90");
        assert_eq!(battle.random_events[0].id, "0");
        assert_eq!(battle.random_events[1].id, "friendship_events.1");

        // The second on_startup is occurrence-qualified.
        let startup2 = &hooks[2];
        assert_eq!(startup2.path, vec!["on_startup#1"]);
        assert_eq!(startup2.events, vec!["extra.1"]);
    }

    #[test]
    fn hook_body_and_lists_resolve_through_spans_api() {
        let hooks = parse_sample();
        let startup = &hooks[0];
        // The hook body parses through the tree editor's spans API.
        let body = crate::script_tree::build_script_block(SAMPLE, &startup.path).unwrap();
        assert!(body.nodes.iter().any(|n| n.key.as_deref() == Some("events")));
        // The events list is an id-list AddId/RemoveId target.
        use crate::mod_writer::{apply, Edit};
        let added = apply(
            SAMPLE,
            &Edit::AddId {
                list_path: startup.events_path.clone(),
                id: "new_event.5".into(),
            },
        )
        .unwrap();
        assert!(String::from_utf8_lossy(&added).contains("new_event.5"));
        // The second on_startup#1 events list resolves independently.
        let b2 = crate::script_tree::build_script_block(SAMPLE, &hooks[2].path).unwrap();
        assert!(b2.nodes.iter().any(|n| n.key.as_deref() == Some("events")));
    }

    #[test]
    fn add_remove_event_round_trip() {
        use crate::mod_writer::{apply, Edit};
        let hooks = parse_sample();
        let ep = hooks[0].events_path.clone();
        let added = apply(SAMPLE, &Edit::AddId { list_path: ep.clone(), id: "zz.1".into() }).unwrap();
        let removed = apply(&added, &Edit::RemoveId { list_path: ep, id: "zz.1".into() }).unwrap();
        assert_eq!(removed, SAMPLE, "add then remove returns byte-identical");
    }

    #[test]
    fn scan_refs_finds_firing_hooks() {
        // A synthetic Vfs so scan_on_action_refs runs end-to-end.
        let root = std::env::temp_dir().join("eu_toolkit_onaction_refs_test");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("map")).unwrap();
        std::fs::write(root.join("map/provinces.bmp"), b"x").unwrap();
        std::fs::create_dir_all(root.join(ON_ACTIONS_DIR)).unwrap();
        std::fs::write(root.join(format!("{ON_ACTIONS_DIR}/00.txt")), SAMPLE).unwrap();
        let vfs = Vfs::new(root.to_str().unwrap(), None).unwrap();

        let refs = scan_on_action_refs(&vfs, "flavor_mng.42");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].hook, "on_startup");
        assert_eq!(refs[0].via, "events");

        let refs = scan_on_action_refs(&vfs, "friendship_events.1");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].via, "random_events");

        assert!(scan_on_action_refs(&vfs, "nonexistent.999").is_empty());
    }

    #[test]
    fn scaffold_parses_back() {
        let sc = scaffold_on_action("on_startup".into()).unwrap();
        assert_eq!(sc.file, PROJECT_FILE);
        let b = paradox::parse(&sc.statement);
        assert!(b.get_block("on_startup").is_some());
        assert!(scaffold_on_action("bad hook!".into()).is_err());
    }

    #[test]
    fn vanilla_on_actions_smoke() {
        if !install_present() {
            return;
        }
        let vfs = Vfs::new(INSTALL, None).unwrap();
        let hooks = load_on_actions(&vfs);
        // Vanilla's 00_on_actions.txt declares 200+ hooks.
        assert!(hooks.len() > 100, "expected many hooks, got {}", hooks.len());
        let startup = hooks.iter().find(|h| h.hook == "on_startup").unwrap();
        assert!(startup.has_events);
        assert!(!startup.events.is_empty());
        // Every hook body parses through the spans API.
        for h in hooks.iter().take(60) {
            let bytes = vfs.read(&h.file).unwrap();
            assert!(
                crate::script_tree::build_script_block(&bytes, &h.path).is_ok(),
                "{} body must parse",
                h.hook
            );
        }
    }

    #[test]
    fn anbennar_on_actions_smoke() {
        if !install_present() || !Path::new(ANBENNAR).is_dir() {
            return;
        }
        let vfs = Vfs::new(INSTALL, Some(ANBENNAR)).unwrap();
        let hooks = load_on_actions(&vfs);
        assert!(!hooks.is_empty());
    }
}
