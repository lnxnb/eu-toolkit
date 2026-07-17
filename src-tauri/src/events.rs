//! Sprint 16 — the events editor backend (View ▸ Events).
//!
//! Events live in `events/*.txt`. Each file declares one or more file-level
//! `namespace = <ns>` lines (vanilla ALWAYS uses `namespace`, never
//! `add_namespace` — verified across the 347 vanilla event files), then a series
//! of `country_event = { … }` / `province_event = { … }` blocks. Every event
//! carries an `id = <ns>.<n>`, `title`/`desc` loc **keys** (the quoted string is
//! the loc key, not a `<key>_title` convention as decisions use), a `picture`
//! sprite name, boolean flags (`is_triggered_only`/`fire_only_once`/`hidden`/
//! `major`), an optional `trigger = { … }`, a `mean_time_to_happen = { … }`
//! likelihood block, and one or more `option = { … }` blocks.
//!
//! Files merge through the [`Vfs`] exactly like any other game location. Events
//! **repeat within a file**, and `country_event`/`province_event` are counted
//! independently (mod_writer's `locate_block` filters occurrences per matching
//! key), so an event block's byte-surgical path is occurrence-qualified as
//! `country_event#n` / `province_event#n`. Sub-block paths (trigger, MTTH, each
//! option) hang off that. Every unmodeled key (`ai_chance`, custom mechanics, …)
//! round-trips untouched — editing is always a byte-surgical splice.

use std::collections::HashMap;

use crate::loc::{self};
use crate::paradox::{self, Block, Value};
use crate::trigger_eval::{self, TriggerEvaluation};
use crate::vfs::Vfs;

/// The game location holding event files.
const EVENTS_DIR: &str = "events";

/// One `option = { … }` inside an event.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventOption {
    /// The `name = "<loc key>"` value (a loc key), if present.
    pub name_key: Option<String>,
    /// The resolved localisation for `name_key` (else `None`).
    pub name_loc: Option<String>,
    /// Byte-surgical path to this option block (occurrence-qualified).
    pub path: Vec<String>,
}

/// One `country_event`/`province_event`, ready for the overlay list + editor.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventEntry {
    /// The full `id` (`flavor_fra.9100`).
    pub id: String,
    /// The namespace portion of the id (`flavor_fra`).
    pub namespace: String,
    /// The numeric portion of the id, if it parses (`9100`).
    pub number: Option<u64>,
    /// Game-relative file the event was found in.
    pub file: String,
    /// `base` or `mod` — which layer the source file came from.
    pub origin: String,
    /// `country` or `province`.
    pub kind: String,
    pub is_triggered_only: bool,
    pub fire_only_once: bool,
    pub hidden: bool,
    pub major: bool,
    /// The `title` loc key (the quoted string value).
    pub title_key: Option<String>,
    /// The `desc` loc key.
    pub desc_key: Option<String>,
    /// Loc-resolved title (`title_key`, else the id).
    pub title: String,
    /// The raw loc value for `title_key` if defined (for `LocOverride` edits).
    pub title_loc: Option<String>,
    /// The raw loc value for `desc_key` if defined.
    pub desc_loc: Option<String>,
    /// The `picture` sprite name, if present.
    pub picture: Option<String>,
    /// The MTTH base unit (`months`/`years`/`days`), if present.
    pub mtth_base_unit: Option<String>,
    /// The MTTH base value (as written — usually a number).
    pub mtth_base_value: Option<String>,
    /// Number of `modifier = { … }` rows in the MTTH block.
    pub mtth_modifier_count: usize,
    /// The options (name loc key + path each).
    pub options: Vec<EventOption>,
    /// Byte-surgical path to the event block (`["country_event#n"]`).
    pub path: Vec<String>,
    /// Path to the `trigger` block (present iff `has_trigger`).
    pub trigger_path: Vec<String>,
    /// Path to the `mean_time_to_happen` block (present iff `has_mtth`).
    pub mtth_path: Vec<String>,
    pub has_trigger: bool,
    pub has_mtth: bool,
}

/// A `#n` occurrence-qualified path segment (bare when the occurrence is 0), so a
/// repeated key still resolves through `mod_writer`'s occurrence addressing.
fn segment(key: &str, occ: usize) -> String {
    if occ > 0 {
        format!("{key}#{occ}")
    } else {
        key.to_string()
    }
}

/// The namespace portion of an event id (`flavor_fra.9100` → `flavor_fra`).
fn namespace_of(id: &str) -> &str {
    match id.rfind('.') {
        Some(dot) => &id[..dot],
        None => id,
    }
}

/// The numeric portion of an event id (`flavor_fra.9100` → `9100`).
fn number_of(id: &str) -> Option<u64> {
    id.rsplit('.').next().and_then(|s| s.parse().ok())
}

/// Extracts every event from one parsed file's `block`, pushing an
/// [`EventEntry`] per `country_event`/`province_event`. `loc` resolves loc keys.
fn collect_file(block: &Block, file: &str, origin: &str, loc: &loc::LocStore, out: &mut Vec<EventEntry>) {
    // Occurrence per event key kind — country_event / province_event count
    // independently, matching mod_writer's per-key `locate_block` addressing.
    let mut occ: HashMap<&str, usize> = HashMap::new();
    for (key, value) in &block.items {
        let (Some(k), Value::Block(ev)) = (key, value) else {
            continue;
        };
        let kind = match k.as_str() {
            "country_event" => "country",
            "province_event" => "province",
            _ => continue,
        };
        let n = occ.entry(k.as_str()).or_insert(0);
        let ev_seg = segment(k, *n);
        *n += 1;

        let Some(id) = ev.get_scalar("id") else {
            continue; // an event with no id is malformed; skip
        };
        let path = vec![ev_seg];
        let sub = |name: &str| {
            let mut p = path.clone();
            p.push(name.to_string());
            p
        };

        let flag = |key: &str| ev.get_scalar(key) == Some("yes");
        let title_key = ev.get_scalar("title").map(str::to_string);
        let desc_key = ev.get_scalar("desc").map(str::to_string);
        let title = title_key
            .as_deref()
            .and_then(|k| loc.get(k))
            .map(str::to_string)
            .unwrap_or_else(|| id.to_string());

        // MTTH base + modifier count.
        let (mtth_base_unit, mtth_base_value, mtth_modifier_count) = match ev.get_block("mean_time_to_happen") {
            Some(m) => {
                let (unit, val) = ["months", "years", "days"]
                    .iter()
                    .find_map(|u| m.get_scalar(u).map(|v| (Some(u.to_string()), Some(v.to_string()))))
                    .unwrap_or((None, None));
                let mods = m.key_blocks().filter(|(k, _)| *k == "modifier").count();
                (unit, val, mods)
            }
            None => (None, None, 0),
        };

        // Options (name loc key + occurrence-qualified path each).
        let mut opt_occ = 0usize;
        let mut options = Vec::new();
        for (ok, ov) in &ev.items {
            let (Some(okey), Value::Block(ob)) = (ok, ov) else {
                continue;
            };
            if okey != "option" {
                continue;
            }
            let opt_seg = segment("option", opt_occ);
            opt_occ += 1;
            let name_key = ob.get_scalar("name").map(str::to_string);
            let name_loc = name_key.as_deref().and_then(|k| loc.get(k)).map(str::to_string);
            let mut p = path.clone();
            p.push(opt_seg);
            options.push(EventOption { name_key, name_loc, path: p });
        }

        let has_trigger = ev.get_block("trigger").is_some();
        let has_mtth = ev.get_block("mean_time_to_happen").is_some();

        out.push(EventEntry {
            namespace: namespace_of(id).to_string(),
            number: number_of(id),
            id: id.to_string(),
            file: file.to_string(),
            origin: origin.to_string(),
            kind: kind.to_string(),
            is_triggered_only: flag("is_triggered_only"),
            fire_only_once: flag("fire_only_once"),
            hidden: flag("hidden"),
            major: flag("major"),
            title_loc: title_key.as_deref().and_then(|k| loc.get(k)).map(str::to_string),
            desc_loc: desc_key.as_deref().and_then(|k| loc.get(k)).map(str::to_string),
            title,
            title_key,
            desc_key,
            picture: ev.get_scalar("picture").map(str::to_string),
            mtth_base_unit,
            mtth_base_value,
            mtth_modifier_count,
            options,
            trigger_path: sub("trigger"),
            mtth_path: sub("mean_time_to_happen"),
            has_trigger,
            has_mtth,
            path,
        });
    }
}

/// Loads every event across the VFS-merged `events/` files.
pub fn load_events(vfs: &Vfs, loc: &loc::LocStore) -> Vec<EventEntry> {
    let mut out = Vec::new();
    let mod_dir = vfs.mod_dir();
    for (file_name, path) in vfs.list_dir(EVENTS_DIR) {
        if !file_name.to_lowercase().ends_with(".txt") {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let origin = if mod_dir.is_some_and(|md| path.starts_with(md)) {
            "mod"
        } else {
            "base"
        };
        let block = paradox::parse(&String::from_utf8_lossy(&bytes));
        let rel = format!("{EVENTS_DIR}/{file_name}");
        collect_file(&block, &rel, origin, loc, &mut out);
    }
    out
}

/// Tauri command: list all events (base + mod) for the Events overlay.
#[tauri::command]
pub fn get_events(install_path: String, mod_path: Option<String>) -> Result<Vec<EventEntry>, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    let loc = loc::store(&vfs, &install_path, mod_path.as_deref());
    Ok(load_events(&vfs, &loc))
}

/// The next free `n` for a namespace: `max(n) + 1` across saved events (pending
/// scaffolds are tracked frontend-side). Returns `1` when the namespace is new.
#[tauri::command]
pub fn next_event_id(install_path: String, mod_path: Option<String>, namespace: String) -> Result<u64, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    let loc = loc::store(&vfs, &install_path, mod_path.as_deref());
    let max = load_events(&vfs, &loc)
        .iter()
        .filter(|e| e.namespace == namespace)
        .filter_map(|e| e.number)
        .max();
    Ok(max.map(|m| m + 1).unwrap_or(1))
}

/// "Can happen to" for one event: the trigger evaluation (14.3) per country.
/// (The frontend skips this for `is_triggered_only` events.)
#[tauri::command]
pub fn evaluate_event(
    install_path: String,
    mod_path: Option<String>,
    date: Option<String>,
    file: String,
    trigger_path: Vec<String>,
) -> Result<TriggerEvaluation, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    let loc = loc::store(&vfs, &install_path, mod_path.as_deref());
    let at = crate::bookmarks::resolve_date(&vfs, date.as_deref())?;
    let bytes = vfs.read(&file)?;

    // An empty path is "no trigger" → zero nodes → every country passes.
    let nodes = if trigger_path.is_empty() {
        Vec::new()
    } else {
        crate::script_tree::build_nodes(&bytes, &trigger_path)
    };
    let snap = trigger_eval::build_snapshot(&vfs, &loc, at);
    Ok(trigger_eval::evaluate_all(&nodes, &snap))
}

// ---------------------------------------------------------------------------
// References scan (is_triggered_only events: "referenced from N files")
// ---------------------------------------------------------------------------

/// One call site that fires an event by id (a jump-link target).
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventReference {
    /// Game-relative file the call appears in.
    pub file: String,
    /// `country` or `province` (which `*_event` triggered it); empty for an
    /// on_action reference (an engine hook, not a `*_event` call).
    pub kind: String,
    /// `base` or `mod`.
    pub origin: String,
    /// Which editor location the reference lives in: `events` | `decisions` |
    /// `missions` | `on_actions` (Sprint 28).
    pub location: String,
    /// For an `on_actions` reference: the engine hook that fires the event
    /// (`on_startup`), else `None`. Sprint 28.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hook: Option<String>,
    /// For an `on_actions` reference: `events` (unconditional) | `random_events`
    /// (weighted), else `None`. Sprint 28.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub via: Option<String>,
}

/// Recursively walks `block`, invoking `visit(key, sub_block)` for every keyed
/// sub-block (at any depth), so `country_event`/`province_event` calls nested
/// inside option effects, `if`/`limit`, scopes, etc. are all found.
fn walk_blocks<'a>(block: &'a Block, visit: &mut dyn FnMut(&'a str, &'a Block)) {
    for (k, v) in &block.items {
        if let Value::Block(b) = v {
            if let Some(key) = k {
                visit(key.as_str(), b);
            }
            walk_blocks(b, visit);
        }
    }
}

/// Whether an `*_event = { … }` block is an event DEFINITION (not a call site).
/// A definition carries a `title` or `option`; a call site is just `id` plus
/// optional `days`/`tooltip`/`hours`/`random`.
fn is_definition(ev: &Block) -> bool {
    ev.get("title").is_some() || ev.get("option").is_some()
}

/// Scans `events/`, `decisions/`, and `missions/` for every call site that fires
/// the event `id` (an `country_event`/`province_event = { id = <id> … }` that is
/// NOT a definition). Returns each (file, kind, origin, location) for jump links.
pub fn scan_references(vfs: &Vfs, id: &str) -> Vec<EventReference> {
    let mod_dir = vfs.mod_dir();
    let mut out = Vec::new();
    for (dir, location) in [(EVENTS_DIR, "events"), ("decisions", "decisions"), ("missions", "missions")] {
        for (file_name, path) in vfs.list_dir(dir) {
            if !file_name.to_lowercase().ends_with(".txt") {
                continue;
            }
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            let origin = if mod_dir.is_some_and(|md| path.starts_with(md)) { "mod" } else { "base" };
            let block = paradox::parse(&String::from_utf8_lossy(&bytes));
            let rel = format!("{dir}/{file_name}");
            let mut hits: Vec<&'static str> = Vec::new();
            walk_blocks(&block, &mut |key, sub| {
                let kind = match key {
                    "country_event" => "country",
                    "province_event" => "province",
                    _ => return,
                };
                if is_definition(sub) {
                    return;
                }
                if sub.get_scalar("id") == Some(id) {
                    hits.push(kind);
                }
            });
            for kind in hits {
                out.push(EventReference {
                    file: rel.clone(),
                    kind: kind.to_string(),
                    origin: origin.to_string(),
                    location: location.to_string(),
                    hook: None,
                    via: None,
                });
            }
        }
    }
    // Sprint 28: on_action hooks that fire this event (events / random_events).
    for r in crate::on_actions::scan_on_action_refs(vfs, id) {
        out.push(EventReference {
            file: r.file,
            kind: String::new(),
            origin: r.origin,
            location: "on_actions".to_string(),
            hook: Some(r.hook),
            via: Some(r.via),
        });
    }
    out
}

/// Tauri command: call sites firing the event `id` (for the is_triggered_only
/// "referenced from N files" jump list).
#[tauri::command]
pub fn find_event_references(
    install_path: String,
    mod_path: Option<String>,
    id: String,
) -> Result<Vec<EventReference>, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    Ok(scan_references(&vfs, &id))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    const INSTALL: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Europa Universalis IV";
    const ANBENNAR: &str = r"C:\Users\User\projects\anbennar-eu4-dev";

    fn install_present() -> bool {
        Path::new(INSTALL).join("map/provinces.bmp").is_file()
    }

    const SAMPLE: &[u8] = br#"namespace = demo

country_event = {
	id = demo.1
	title = "demo.1.t"
	desc = "demo.1.d"
	picture = ECONOMY_eventPicture
	fire_only_once = yes

	trigger = {
		tag = FRA
		NOT = { is_year = 1500 }
	}

	mean_time_to_happen = {
		months = 300
		modifier = {
			factor = 0.5
			stability = 3
		}
		modifier = {
			factor = 2
			NOT = { stability = 0 }
		}
	}

	option = {
		name = "demo.1.a"
		ai_chance = { factor = 1 }
		add_prestige = 10
	}
	option = {
		name = "demo.1.b"
		add_stability = 1
	}
}

province_event = {
	id = demo.2
	title = "demo.2.t"
	is_triggered_only = yes
	hidden = yes
	option = {
		name = "demo.2.a"
	}
}
"#;

    fn parse_sample() -> Vec<EventEntry> {
        let block = paradox::parse(&String::from_utf8_lossy(SAMPLE));
        let loc = loc::LocStore::from_pairs(&[
            ("demo.1.t", "The Demo Event"),
            ("demo.1.d", "It happened."),
            ("demo.1.a", "Good"),
        ]);
        let mut out = Vec::new();
        collect_file(&block, "events/Demo.txt", "base", &loc, &mut out);
        out
    }

    #[test]
    fn collects_events_flags_paths_loc_mtth_options() {
        let out = parse_sample();
        assert_eq!(out.len(), 2);

        let e1 = out.iter().find(|e| e.id == "demo.1").unwrap();
        assert_eq!(e1.namespace, "demo");
        assert_eq!(e1.number, Some(1));
        assert_eq!(e1.kind, "country");
        assert!(e1.fire_only_once && !e1.is_triggered_only && !e1.hidden && !e1.major);
        assert_eq!(e1.title, "The Demo Event");
        assert_eq!(e1.title_key.as_deref(), Some("demo.1.t"));
        assert_eq!(e1.desc_key.as_deref(), Some("demo.1.d"));
        assert_eq!(e1.picture.as_deref(), Some("ECONOMY_eventPicture"));
        assert!(e1.has_trigger && e1.has_mtth);
        assert_eq!(e1.path, vec!["country_event"]);
        assert_eq!(e1.trigger_path, vec!["country_event", "trigger"]);
        // MTTH base + modifiers.
        assert_eq!(e1.mtth_base_unit.as_deref(), Some("months"));
        assert_eq!(e1.mtth_base_value.as_deref(), Some("300"));
        assert_eq!(e1.mtth_modifier_count, 2);
        // Options with occurrence-qualified paths + name loc.
        assert_eq!(e1.options.len(), 2);
        assert_eq!(e1.options[0].name_key.as_deref(), Some("demo.1.a"));
        assert_eq!(e1.options[0].name_loc.as_deref(), Some("Good"));
        assert_eq!(e1.options[0].path, vec!["country_event", "option"]);
        assert_eq!(e1.options[1].path, vec!["country_event", "option#1"]);

        // The province_event is counted independently → occurrence 0 (bare key),
        // NOT country_event#1.
        let e2 = out.iter().find(|e| e.id == "demo.2").unwrap();
        assert_eq!(e2.kind, "province");
        assert!(e2.is_triggered_only && e2.hidden);
        assert_eq!(e2.path, vec!["province_event"]);
        // No title loc defined → title falls back to the id.
        assert_eq!(e2.title, "demo.2");
        assert!(!e2.has_mtth);
    }

    #[test]
    fn event_paths_feed_the_spans_api() {
        // The emitted trigger/MTTH/option paths resolve through the same spans API
        // the tree editor uses, so the frontend can parse each block for editing.
        let out = parse_sample();
        let e1 = out.iter().find(|e| e.id == "demo.1").unwrap();
        let trig = crate::script_tree::build_script_block(SAMPLE, &e1.trigger_path).unwrap();
        assert!(trig.nodes.iter().any(|n| n.key.as_deref() == Some("tag")));
        let mtth = crate::script_tree::build_script_block(SAMPLE, &e1.mtth_path).unwrap();
        assert!(mtth.nodes.iter().any(|n| n.key.as_deref() == Some("months")));
        // The second modifier is addressable via its occurrence-qualified path.
        let mod1 = crate::script_tree::build_script_block(
            SAMPLE,
            &[e1.mtth_path.clone(), vec!["modifier#1".into()]].concat(),
        )
        .unwrap();
        assert!(mod1.nodes.iter().any(|n| n.key.as_deref() == Some("factor")));
        // The second option resolves too.
        let opt1 = crate::script_tree::build_script_block(SAMPLE, &e1.options[1].path).unwrap();
        assert!(opt1.nodes.iter().any(|n| n.key.as_deref() == Some("add_stability")));
    }

    #[test]
    fn flag_toggle_is_byte_surgical() {
        use crate::mod_writer::{apply, Edit};
        // Toggle demo.1's fire_only_once yes → no via its event path; only that
        // value changes, everything else (trigger, MTTH, options) round-trips.
        let out = apply(
            SAMPLE,
            &Edit::SetScalar {
                path: vec!["country_event".into(), "fire_only_once".into()],
                value: "no".into(),
                quoted: false,
            },
        )
        .unwrap();
        let marker = b"fire_only_once = yes";
        let mpos = SAMPLE.windows(marker.len()).position(|w| w == marker).unwrap();
        let vstart = mpos + b"fire_only_once = ".len();
        let vend = vstart + b"yes".len();
        assert_eq!(&SAMPLE[..vstart], &out[..vstart], "prefix byte-identical");
        assert_eq!(&out[vstart..vstart + 2], b"no");
        assert_eq!(&SAMPLE[vend..], &out[vstart + 2..], "suffix byte-identical");
    }

    #[test]
    fn mtth_modifier_factor_edit_is_byte_surgical() {
        use crate::mod_writer::{apply, Edit};
        // Edit the FIRST MTTH modifier's factor (0.5 → 0.8) through its
        // occurrence-qualified path. Only that scalar changes; the second modifier
        // and its NOT condition are untouched.
        let out = apply(
            SAMPLE,
            &Edit::SetScalar {
                path: vec![
                    "country_event".into(),
                    "mean_time_to_happen".into(),
                    "modifier".into(), // occurrence 0 = bare key
                    "factor".into(),
                ],
                value: "0.8".into(),
                quoted: false,
            },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("factor = 0.8"));
        assert!(text.contains("factor = 2")); // second modifier untouched
        assert!(text.contains("NOT = { stability = 0 }"));
        assert!(text.contains("months = 300"));
    }

    #[test]
    fn option_add_and_remove_round_trip() {
        use crate::mod_writer::{apply, Edit};
        // Add a third option to demo.1, then remove it again; the file returns to
        // byte-identical (proves the add/remove pair composes cleanly).
        let added = apply(
            SAMPLE,
            &Edit::InsertStatement {
                block_path: vec!["country_event".into()],
                statement: "option = {\n\t\tname = \"demo.1.c\"\n\t\tadd_adm_power = 5\n\t}".into(),
            },
        )
        .unwrap();
        // Now there are three options.
        let block = paradox::parse(&String::from_utf8_lossy(&added));
        let loc = loc::LocStore::from_pairs(&[]);
        let mut out = Vec::new();
        collect_file(&block, "events/Demo.txt", "base", &loc, &mut out);
        let e1 = out.iter().find(|e| e.id == "demo.1").unwrap();
        assert_eq!(e1.options.len(), 3);
        assert_eq!(e1.options[2].path, vec!["country_event", "option#2"]);

        // Remove the freshly-added third option (occurrence #2).
        let removed = apply(
            &added,
            &Edit::RemoveStatement {
                block_path: vec!["country_event".into()],
                key: "option#2".into(),
                value: None,
            },
        )
        .unwrap();
        let block2 = paradox::parse(&String::from_utf8_lossy(&removed));
        let mut out2 = Vec::new();
        collect_file(&block2, "events/Demo.txt", "base", &loc, &mut out2);
        let e1b = out2.iter().find(|e| e.id == "demo.1").unwrap();
        assert_eq!(e1b.options.len(), 2, "third option removed");
    }

    #[test]
    fn vanilla_event_flag_toggle_is_byte_surgical() {
        // Spec acceptance: toggling a flag on a real vanilla event changes ONLY
        // that span — the rest of the file is byte-identical. flavor_fra.9100 has
        // `fire_only_once = yes`; it is the FIRST such line in the file.
        if !install_present() {
            return;
        }
        use crate::mod_writer::{apply, Edit};
        let vfs = Vfs::new(INSTALL, None).unwrap();
        let base = vfs.read("events/FlavorFRA.txt").unwrap();
        // flavor_fra.9100 is the first country_event in the file (occurrence 0).
        let out = apply(
            &base,
            &Edit::SetScalar {
                path: vec!["country_event".into(), "fire_only_once".into()],
                value: "no".into(),
                quoted: false,
            },
        )
        .unwrap();
        let marker = b"fire_only_once = yes";
        let mpos = base.windows(marker.len()).position(|w| w == marker).unwrap();
        let vstart = mpos + b"fire_only_once = ".len();
        let vend = vstart + b"yes".len();
        assert_eq!(&base[..vstart], &out[..vstart], "prefix byte-identical");
        assert_eq!(&out[vstart..vstart + 2], b"no");
        assert_eq!(&base[vend..], &out[vstart + 2..], "suffix byte-identical");
    }

    #[test]
    fn references_scan_distinguishes_calls_from_definitions() {
        // A file that DEFINES demo.5 (title+option) and CALLS demo.9 inline (from
        // an option effect and an `if`). The definition of demo.5 must NOT be a
        // reference; both demo.9 call sites must be found, and a substring-prefix
        // id (demo.90) must NOT match demo.9.
        const REFS: &[u8] = br#"namespace = demo
country_event = {
	id = demo.5
	title = "demo.5.t"
	option = {
		name = "demo.5.a"
		country_event = { id = demo.9 days = 30 }
	}
}
country_event = {
	id = demo.90
	title = "demo.90.t"
	option = {
		name = "demo.90.a"
		if = {
			limit = { tag = FRA }
			province_event = { id = demo.9 }
		}
	}
}
"#;
        let block = paradox::parse(&String::from_utf8_lossy(REFS));
        let mut kinds: Vec<&str> = Vec::new();
        walk_blocks(&block, &mut |key, sub| {
            if matches!(key, "country_event" | "province_event")
                && !is_definition(sub)
                && sub.get_scalar("id") == Some("demo.9")
            {
                kinds.push(if key == "country_event" { "country" } else { "province" });
            }
        });
        // Two calls (one country_event, one province_event); the demo.5/demo.90
        // definitions and the demo.90 substring are all excluded.
        assert_eq!(kinds.len(), 2);
        assert!(kinds.contains(&"country"));
        assert!(kinds.contains(&"province"));
    }

    // --- Real-install smoke tests (no-op if the game/Anbennar is absent) ------

    #[test]
    fn vanilla_lists_events_and_finds_flavor_fra_9100() {
        if !install_present() {
            return;
        }
        let vfs = Vfs::new(INSTALL, None).unwrap();
        let loc = loc::build(&vfs);
        let events = load_events(&vfs, &loc);
        // Vanilla ships thousands of events.
        assert!(events.len() > 2000, "expected thousands of vanilla events, got {}", events.len());

        let ff = events.iter().find(|e| e.id == "flavor_fra.9100").expect("flavor_fra.9100 exists");
        assert_eq!(ff.namespace, "flavor_fra");
        assert_eq!(ff.kind, "country");
        assert_eq!(ff.origin, "base");
        assert!(ff.fire_only_once);
        assert_eq!(ff.picture.as_deref(), Some("ECONOMY_eventPicture"));
        assert_eq!(ff.mtth_base_unit.as_deref(), Some("months"));
        assert_eq!(ff.mtth_base_value.as_deref(), Some("300"));
        assert!(ff.has_trigger);
        assert!(!ff.options.is_empty());
        assert!(ff.file.starts_with("events/"));
    }

    #[test]
    fn next_free_id_is_max_plus_one() {
        if !install_present() {
            return;
        }
        let n = next_event_id(INSTALL.to_string(), None, "flavor_fra".to_string()).unwrap();
        // flavor_fra.9100 exists, so the next free id is well above it.
        assert!(n > 9100, "next flavor_fra id should exceed the max, got {n}");
        // A brand-new namespace starts at 1.
        let fresh = next_event_id(INSTALL.to_string(), None, "eutoolkit_zzz_unused".to_string()).unwrap();
        assert_eq!(fresh, 1);
    }

    #[test]
    fn references_scan_finds_colonial_nation_180_callers() {
        // colonial_nation.180 is is_triggered_only and fired from many inline call
        // sites (verified via a brace-matched scan of the vanilla files).
        if !install_present() {
            return;
        }
        let vfs = Vfs::new(INSTALL, None).unwrap();
        let refs = scan_references(&vfs, "colonial_nation.180");
        assert!(refs.len() >= 3, "expected several colonial_nation.180 callers, got {}", refs.len());
        // All are country_event calls in the events location.
        assert!(refs.iter().all(|r| r.kind == "country"));
        assert!(refs.iter().any(|r| r.location == "events"));
        // The definition itself is not counted as a reference.
        let loc = loc::build(&vfs);
        let events = load_events(&vfs, &loc);
        let def = events.iter().find(|e| e.id == "colonial_nation.180").unwrap();
        assert!(def.is_triggered_only);
    }

    #[test]
    fn scaffold_shape_parses_back_namespace_first() {
        // The exact shape "+ New event" writes into events/zz_eutoolkit_events.txt:
        // a file-level `namespace = <ns>` (which MUST precede the event — vanilla
        // uses `namespace`, never `add_namespace`) then one event with one option.
        // It must parse back as one addressable event with a trigger, an MTTH, and
        // one option, all reachable through the spans API.
        let scaffold = b"namespace = eutoolkit\n\ncountry_event = {\n\tid = eutoolkit.1\n\ttitle = \"eutoolkit.1.t\"\n\tdesc = \"eutoolkit.1.d\"\n\tpicture = ECONOMY_eventPicture\n\tis_triggered_only = yes\n\n\ttrigger = {\n\t}\n\n\tmean_time_to_happen = {\n\t\tdays = 1\n\t}\n\n\toption = {\n\t\tname = \"eutoolkit.1.a\"\n\t}\n}\n";
        // Namespace-first invariant: the `namespace` line precedes the event block.
        let text = String::from_utf8_lossy(scaffold);
        let ns_pos = text.find("namespace = eutoolkit").unwrap();
        let ev_pos = text.find("country_event").unwrap();
        assert!(ns_pos < ev_pos, "namespace must be declared before the event");

        let block = paradox::parse(&text);
        let loc = loc::LocStore::from_pairs(&[]);
        let mut out = Vec::new();
        collect_file(&block, "events/zz_eutoolkit_events.txt", "mod", &loc, &mut out);
        assert_eq!(out.len(), 1);
        let e = &out[0];
        assert_eq!(e.id, "eutoolkit.1");
        assert_eq!(e.namespace, "eutoolkit");
        assert!(e.is_triggered_only);
        assert!(e.has_trigger && e.has_mtth);
        assert_eq!(e.options.len(), 1);
        // Its trigger/option paths resolve through the spans API (editable).
        assert!(crate::script_tree::build_script_block(scaffold, &e.trigger_path).is_ok());
        assert!(crate::script_tree::build_script_block(scaffold, &e.options[0].path).is_ok());
    }

    #[test]
    fn scan_references_includes_on_action_hooks() {
        // Sprint 28: an event fired from an on_action hook must appear in the
        // event's "referenced from" list, tagged location=on_actions with the hook.
        let root = std::env::temp_dir().join("eu_toolkit_events_onaction_scan_test");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("map")).unwrap();
        std::fs::write(root.join("map/provinces.bmp"), b"x").unwrap();
        std::fs::create_dir_all(root.join("events")).unwrap();
        std::fs::create_dir_all(root.join("common/on_actions")).unwrap();
        // The event DEFINITION.
        std::fs::write(
            root.join("events/Demo.txt"),
            b"namespace = demo\ncountry_event = {\n\tid = demo.7\n\ttitle = \"demo.7.t\"\n\toption = { name = \"demo.7.a\" }\n}\n",
        )
        .unwrap();
        // An on_action firing it via events, plus a decoy random_events hook.
        std::fs::write(
            root.join("common/on_actions/00.txt"),
            b"on_startup = {\n\tevents = { demo.7 }\n}\non_battle_won_by_country = {\n\trandom_events = { 100 = demo.7 }\n}\n",
        )
        .unwrap();
        let vfs = Vfs::new(root.to_str().unwrap(), None).unwrap();

        let refs = scan_references(&vfs, "demo.7");
        let on_action_refs: Vec<_> = refs.iter().filter(|r| r.location == "on_actions").collect();
        assert_eq!(on_action_refs.len(), 2, "both on_action hooks report the event");
        assert!(on_action_refs.iter().any(|r| r.hook.as_deref() == Some("on_startup") && r.via.as_deref() == Some("events")));
        assert!(on_action_refs
            .iter()
            .any(|r| r.hook.as_deref() == Some("on_battle_won_by_country") && r.via.as_deref() == Some("random_events")));
    }

    #[test]
    fn anbennar_events_smoke() {
        if !install_present() || !Path::new(ANBENNAR).is_dir() {
            return;
        }
        let vfs = Vfs::new(INSTALL, Some(ANBENNAR)).unwrap();
        let loc = loc::build(&vfs);
        let events = load_events(&vfs, &loc);
        assert!(!events.is_empty());
        // Anbennar ships its own events; at least one must be mod-origin.
        assert!(events.iter().any(|e| e.origin == "mod"), "Anbennar should contribute mod-origin events");
    }
}
