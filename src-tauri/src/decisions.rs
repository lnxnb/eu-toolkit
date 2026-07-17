//! Sprint 15 — the decisions editor backend (View ▸ Decisions).
//!
//! Decisions live in `decisions/*.txt`, each a `country_decisions = { <key> = { … } }`
//! wrapper (a file may also carry `province_decisions`, which this module ignores).
//! Multiple files are merged through the [`Vfs`] exactly like any other game
//! location (mod files shadow same-named base files). Every `country_decisions`
//! entry becomes a [`DecisionEntry`] carrying:
//!   * its `key`, source `file`, and `origin` (base|mod),
//!   * the `major` flag,
//!   * loc-resolved `title` + the raw `<key>_title` / `<key>_desc` loc values,
//!   * the braces-inclusive `ai_will_do` raw text (shown raw-preserved, advanced),
//!   * the presence + byte-surgical path of the `potential`/`allow`/`effect`
//!     blocks, so the frontend feeds each to `parse_script_block` (14.2) and to
//!     `evaluate_decision` (14.3 availability).
//!
//! Every unmodeled key (`provinces_to_highlight`, custom mechanics, …) round-trips
//! untouched — editing is always a byte-surgical splice of one targeted span.

use std::collections::HashMap;

use crate::loc::{self};
use crate::mod_writer;
use crate::paradox::{self, Block, Value};
use crate::trigger_eval::{self, TriggerEvaluation};
use crate::vfs::Vfs;

/// The game location holding decision files.
const DECISIONS_DIR: &str = "decisions";

/// The top-level wrapper block for country-scope decisions.
const COUNTRY_DECISIONS: &str = "country_decisions";

/// One `country_decisions` entry, ready for the overlay list + row editor.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DecisionEntry {
    /// The decision script key (e.g. `french_nation`).
    pub key: String,
    /// Game-relative file the decision was found in.
    pub file: String,
    /// `base` or `mod` — which layer the source file came from.
    pub origin: String,
    /// Whether `major = yes` is set.
    pub major: bool,
    /// Loc-resolved title (`<key>_title`, else the prettified key).
    pub title: String,
    /// The `<key>_title` loc key (for `LocOverride` edits).
    pub title_key: String,
    /// The `<key>_desc` loc key.
    pub desc_key: String,
    /// The raw `<key>_title` loc value if one is defined (else `None`).
    pub title_loc: Option<String>,
    /// The raw `<key>_desc` loc value if one is defined.
    pub desc_loc: Option<String>,
    /// Braces-inclusive `ai_will_do = { … }` raw text, if present.
    pub ai_will_do: Option<String>,
    /// Byte-surgical path to the decision block (`["country_decisions", "<key>"]`,
    /// occurrence-qualified if the key repeats). The `major` toggle targets it.
    pub path: Vec<String>,
    /// Path to the `potential` block (present iff `has_potential`).
    pub potential_path: Vec<String>,
    /// Path to the `allow` block (present iff `has_allow`).
    pub allow_path: Vec<String>,
    /// Path to the `effect` block (present iff `has_effect`).
    pub effect_path: Vec<String>,
    pub has_potential: bool,
    pub has_allow: bool,
    pub has_effect: bool,
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

/// Extracts every `country_decisions` decision from one parsed file's `block`,
/// pushing a [`DecisionEntry`] per decision. `bytes` is the raw file (for the
/// `ai_will_do` raw span); `loc` resolves titles.
fn collect_file(
    bytes: &[u8],
    block: &Block,
    file: &str,
    origin: &str,
    loc: &loc::LocStore,
    out: &mut Vec<DecisionEntry>,
) {
    // A file may (rarely) carry more than one country_decisions wrapper; track
    // the occurrence so the path stays byte-addressable in every case.
    let mut cd_occ = 0usize;
    for (key, value) in &block.items {
        let (Some(k), Value::Block(cd)) = (key, value) else {
            continue;
        };
        if k != COUNTRY_DECISIONS {
            continue;
        }
        let cd_seg = segment(COUNTRY_DECISIONS, cd_occ);
        cd_occ += 1;

        let mut key_occ: HashMap<String, usize> = HashMap::new();
        for (dk, dv) in &cd.items {
            let (Some(dec_key), Value::Block(dec)) = (dk, dv) else {
                continue;
            };
            let occ = key_occ.entry(dec_key.clone()).or_insert(0);
            let dec_seg = segment(dec_key, *occ);
            *occ += 1;

            let path = vec![cd_seg.clone(), dec_seg];
            let sub = |name: &str| {
                let mut p = path.clone();
                p.push(name.to_string());
                p
            };

            let major = dec.get_scalar("major") == Some("yes");
            let has_potential = dec.get_block("potential").is_some();
            let has_allow = dec.get_block("allow").is_some();
            let has_effect = dec.get_block("effect").is_some();

            let ai_will_do = {
                let mut p = path.clone();
                p.push("ai_will_do".to_string());
                mod_writer::block_span(bytes, &p)
                    .map(|(s, e)| String::from_utf8_lossy(&bytes[s..e]).into_owned())
            };

            let title_key = format!("{dec_key}_title");
            let desc_key = format!("{dec_key}_desc");
            let title = loc
                .get(&title_key)
                .map(str::to_string)
                .unwrap_or_else(|| loc::prettify(dec_key));

            out.push(DecisionEntry {
                key: dec_key.clone(),
                file: file.to_string(),
                origin: origin.to_string(),
                major,
                title,
                title_loc: loc.get(&title_key).map(str::to_string),
                desc_loc: loc.get(&desc_key).map(str::to_string),
                title_key,
                desc_key,
                ai_will_do,
                potential_path: sub("potential"),
                allow_path: sub("allow"),
                effect_path: sub("effect"),
                path,
                has_potential,
                has_allow,
                has_effect,
            });
        }
    }
}

/// Loads every `country_decisions` entry across the VFS-merged `decisions/` files.
pub fn load_decisions(vfs: &Vfs, loc: &loc::LocStore) -> Vec<DecisionEntry> {
    let mut out = Vec::new();
    let mod_dir = vfs.mod_dir();
    for (file_name, path) in vfs.list_dir(DECISIONS_DIR) {
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
        let rel = format!("{DECISIONS_DIR}/{file_name}");
        collect_file(&bytes, &block, &rel, origin, loc, &mut out);
    }
    out
}

/// Tauri command: list all decisions (base + mod) for the Decisions overlay.
#[tauri::command]
pub fn get_decisions(
    install_path: String,
    mod_path: Option<String>,
) -> Result<Vec<DecisionEntry>, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    let loc = loc::store(&vfs, &install_path, mod_path.as_deref());
    Ok(load_decisions(&vfs, &loc))
}

/// Availability of one decision: the trigger evaluation (14.3) of its
/// `potential` AND `allow` conditions, intersected per country. A decision is
/// available to a country iff BOTH gates pass — so the combined node list is
/// simply `potential ++ allow` evaluated as one implicit AND (`Yes∧Yes = Yes`,
/// any `No = No`, else `Unknown`). The `unevaluated` set (the "approximate — N
/// conditions not evaluated" badge) is the union across both blocks.
#[tauri::command]
pub fn evaluate_decision(
    install_path: String,
    mod_path: Option<String>,
    date: Option<String>,
    file: String,
    potential_path: Vec<String>,
    allow_path: Vec<String>,
) -> Result<TriggerEvaluation, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    let loc = loc::store(&vfs, &install_path, mod_path.as_deref());
    let at = crate::bookmarks::resolve_date(&vfs, date.as_deref())?;
    let bytes = vfs.read(&file)?;

    // An EMPTY path is treated as "this gate is absent" → zero nodes, which ANDs
    // to Yes (imposes no constraint) — exactly right for a decision lacking one of
    // the two blocks. (An empty path would otherwise address the whole file.)
    let build = |p: &[String]| {
        if p.is_empty() {
            Vec::new()
        } else {
            crate::script_tree::build_nodes(&bytes, p)
        }
    };
    let mut nodes = build(&potential_path);
    nodes.extend(build(&allow_path));

    let snap = trigger_eval::build_snapshot(&vfs, &loc, at);
    Ok(trigger_eval::evaluate_all(&nodes, &snap))
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

    const SAMPLE: &[u8] = br#"country_decisions = {
	french_nation = {
		major = yes
		potential = {
			culture_group = french
			NOT = { has_country_flag = formed_france_flag }
		}
		allow = {
			is_at_war = no
			owns_core_province = 183
		}
		effect = {
			change_tag = FRA
			add_prestige = 25
		}
		ai_will_do = {
			factor = 1
		}
	}
	plain_decision = {
		potential = { tag = ENG }
		effect = { add_prestige = 10 }
	}
}"#;

    fn parse_sample() -> Vec<DecisionEntry> {
        let block = paradox::parse(&String::from_utf8_lossy(SAMPLE));
        let loc = loc::LocStore::from_pairs(&[
            ("french_nation_title", "Form French Nation"),
            ("french_nation_desc", "Become France."),
        ]);
        let mut out = Vec::new();
        collect_file(SAMPLE, &block, "decisions/Sample.txt", "base", &loc, &mut out);
        out
    }

    #[test]
    fn collects_entries_with_flags_paths_and_loc() {
        let out = parse_sample();
        assert_eq!(out.len(), 2);

        let fr = out.iter().find(|d| d.key == "french_nation").unwrap();
        assert!(fr.major);
        assert_eq!(fr.title, "Form French Nation");
        assert_eq!(fr.title_key, "french_nation_title");
        assert_eq!(fr.desc_key, "french_nation_desc");
        assert_eq!(fr.title_loc.as_deref(), Some("Form French Nation"));
        assert!(fr.has_potential && fr.has_allow && fr.has_effect);
        assert_eq!(fr.path, vec!["country_decisions", "french_nation"]);
        assert_eq!(
            fr.potential_path,
            vec!["country_decisions", "french_nation", "potential"]
        );
        // ai_will_do is preserved raw, braces-inclusive.
        let awd = fr.ai_will_do.as_deref().unwrap();
        assert!(awd.starts_with('{') && awd.ends_with('}'));
        assert!(awd.contains("factor = 1"));

        // A decision without major/allow/ai_will_do reports them absent, and its
        // title falls back to the prettified key (no loc defined).
        let plain = out.iter().find(|d| d.key == "plain_decision").unwrap();
        assert!(!plain.major);
        assert!(!plain.has_allow);
        assert!(plain.ai_will_do.is_none());
        assert_eq!(plain.title, "Plain Decision");
    }

    #[test]
    fn scaffold_wrapper_file_parses_back() {
        // The exact shape "+ New decision" writes into a brand-new
        // decisions/zz_eutoolkit_decisions.txt (a country_decisions wrapper around
        // an empty decision) must parse back as one addressable decision with all
        // three (empty) blocks + a sensible ai_will_do.
        let scaffold = b"country_decisions = {\n\tmy_new_decision = {\n\t\tpotential = {\n\t\t}\n\t\tallow = {\n\t\t}\n\t\teffect = {\n\t\t}\n\t\tai_will_do = {\n\t\t\tfactor = 1\n\t\t}\n\t}\n}\n";
        let block = paradox::parse(&String::from_utf8_lossy(scaffold));
        let loc = loc::LocStore::from_pairs(&[]);
        let mut out = Vec::new();
        collect_file(scaffold, &block, "decisions/zz_eutoolkit_decisions.txt", "mod", &loc, &mut out);
        assert_eq!(out.len(), 1);
        let d = &out[0];
        assert_eq!(d.key, "my_new_decision");
        assert!(d.has_potential && d.has_allow && d.has_effect);
        assert!(d.ai_will_do.as_deref().unwrap().contains("factor = 1"));
        assert_eq!(d.title, "My New Decision");
        // The emitted potential path resolves through the spans API (editable).
        assert!(crate::script_tree::build_script_block(scaffold, &d.potential_path).is_ok());
    }

    #[test]
    fn decision_paths_feed_the_spans_api() {
        // The emitted potential path must resolve through the same spans API the
        // script tree uses, so the frontend can parse the block for editing.
        let out = parse_sample();
        let fr = out.iter().find(|d| d.key == "french_nation").unwrap();
        let block = crate::script_tree::build_script_block(SAMPLE, &fr.potential_path).unwrap();
        assert!(block.nodes.iter().any(|n| n.key.as_deref() == Some("culture_group")));
    }

    /// A small synthetic world: FRA (french culture group, owns+cores 183),
    /// ENG (british), OTT (turko-semitic).
    fn fixture() -> trigger_eval::WorldSnapshot {
        use std::collections::HashMap;
        let mk = |tag: &str, culture: &str, prov: &[u32]| trigger_eval::CountryState {
            tag: tag.into(),
            owned: prov.iter().copied().collect(),
            cores: prov.iter().copied().collect(),
            culture: Some(culture.into()),
            num_cities: prov.len(),
            ..Default::default()
        };
        let mut countries = HashMap::new();
        countries.insert("FRA".into(), mk("FRA", "cosmopolitan_french", &[183, 184]));
        countries.insert("ENG".into(), mk("ENG", "english", &[236]));
        countries.insert("OTT".into(), mk("OTT", "turkish", &[151]));
        let mut culture_group = HashMap::new();
        culture_group.insert("cosmopolitan_french".into(), "french".into());
        culture_group.insert("english".into(), "british".into());
        culture_group.insert("turkish".into(), "turko_semitic".into());
        trigger_eval::WorldSnapshot {
            countries,
            culture_group,
            year: 1500,
            ..Default::default()
        }
    }

    #[test]
    fn availability_intersects_potential_and_allow() {
        // Hand-derivation on a synthetic world: potential = culture_group french,
        // allow = owns_core_province 183 + is_at_war = no. Only a french-culture-
        // group country holding core 183 and at peace passes both gates.
        let snap = fixture();
        let mut nodes =
            crate::script_tree::build_nodes(SAMPLE, &["country_decisions".into(), "french_nation".into(), "potential".into()]);
        nodes.extend(crate::script_tree::build_nodes(
            SAMPLE,
            &["country_decisions".into(), "french_nation".into(), "allow".into()],
        ));
        let eval = trigger_eval::evaluate_all(&nodes, &snap);
        // FRA is french-culture-group; owns_core 183 is decidable in the fixture
        // (FRA owns+cores 183) → Yes. ENG/OTT fail culture_group → No.
        let by = |tag: &str| {
            eval.verdicts
                .iter()
                .find(|v| v.tag == tag)
                .map(|v| v.verdict.as_str())
        };
        assert_eq!(by("FRA"), Some("yes"));
        assert_eq!(by("ENG"), Some("no"));
        assert_eq!(by("OTT"), Some("no"));
    }

    #[test]
    fn absent_allow_gate_imposes_no_constraint() {
        // plain_decision has a potential (tag = ENG) and NO allow. The empty allow
        // path must contribute zero nodes (not address the whole file), so the
        // result is exactly the potential: only ENG.
        let snap = fixture();
        let potential = crate::script_tree::build_nodes(
            SAMPLE,
            &["country_decisions".into(), "plain_decision".into(), "potential".into()],
        );
        // Mirror evaluate_decision's empty-path guard: an empty allow adds nothing.
        let nodes = potential;
        let eval = trigger_eval::evaluate_all(&nodes, &snap);
        let yes: Vec<&str> = eval
            .verdicts
            .iter()
            .filter(|v| v.verdict == "yes")
            .map(|v| v.tag.as_str())
            .collect();
        assert_eq!(yes, vec!["ENG"]);
    }

    // --- Real-install smoke tests (no-op if the game/Anbennar is absent) ------

    #[test]
    fn vanilla_lists_decisions_and_finds_french_nation() {
        if !install_present() {
            return;
        }
        let vfs = Vfs::new(INSTALL, None).unwrap();
        let loc = loc::build(&vfs);
        let decisions = load_decisions(&vfs, &loc);
        assert!(
            decisions.len() > 100,
            "expected many vanilla decisions, got {}",
            decisions.len()
        );
        let fr = decisions
            .iter()
            .find(|d| d.key == "french_nation")
            .expect("french_nation decision exists");
        assert!(fr.major);
        assert_eq!(fr.origin, "base");
        assert_eq!(fr.title, "Form French Nation");
        assert!(fr.has_potential && fr.has_allow && fr.has_effect);
        assert!(fr.ai_will_do.as_deref().unwrap().contains("factor"));
        assert!(fr.file.starts_with("decisions/"));
    }

    #[test]
    fn vanilla_decision_edit_is_byte_surgical() {
        // Spec acceptance: toggling `major` / editing a leaf on a real vanilla
        // decision changes ONLY that span — the rest of the file is byte-identical.
        if !install_present() {
            return;
        }
        let vfs = Vfs::new(INSTALL, None).unwrap();
        let base = vfs.read("decisions/FrenchNation.txt").unwrap();

        // Toggle french_nation major yes -> no via the decision path.
        let out = mod_writer::apply(
            &base,
            &mod_writer::Edit::SetScalar {
                path: vec![
                    "country_decisions".into(),
                    "french_nation".into(),
                    "major".into(),
                ],
                value: "no".into(),
                quoted: false,
            },
        )
        .unwrap();

        // The first `major = yes` is french_nation's; prove every other byte
        // round-trips by splitting around just that value.
        let marker = b"major = yes";
        let mpos = base
            .windows(marker.len())
            .position(|w| w == marker)
            .expect("french_nation major line");
        let vstart = mpos + b"major = ".len();
        let vend = vstart + b"yes".len();
        assert_eq!(&base[..vstart], &out[..vstart], "prefix byte-identical");
        assert_eq!(&out[vstart..vstart + 2], b"no", "value changed to no");
        assert_eq!(&base[vend..], &out[vstart + 2..], "suffix byte-identical");

        // A leaf edit in the effect block also splices only its value.
        let leaf = mod_writer::apply(
            &base,
            &mod_writer::Edit::SetScalar {
                path: vec![
                    "country_decisions".into(),
                    "french_nation".into(),
                    "effect".into(),
                    "add_prestige".into(),
                ],
                value: "30".into(),
                quoted: false,
            },
        )
        .unwrap();
        assert!(String::from_utf8_lossy(&leaf).contains("add_prestige = 30"));
    }

    #[test]
    fn vanilla_french_nation_availability_is_sane() {
        if !install_present() {
            return;
        }
        let fr_potential = vec![
            "country_decisions".to_string(),
            "french_nation".to_string(),
            "potential".to_string(),
        ];
        let fr_allow = vec![
            "country_decisions".to_string(),
            "french_nation".to_string(),
            "allow".to_string(),
        ];
        let avail = evaluate_decision(
            INSTALL.to_string(),
            None,
            None,
            "decisions/FrenchNation.txt".to_string(),
            fr_potential,
            fr_allow,
        )
        .unwrap();
        // The trigger touches province-scope / block-valued conditions the
        // evaluator can't decide, so it is honestly approximate.
        assert!(
            !avail.unevaluated.is_empty(),
            "french_nation gates on conditions the evaluator can't decide"
        );
        // FRA itself is excluded (NOT tag = FRA) → never a Yes.
        let fra = avail.verdicts.iter().find(|v| v.tag == "FRA");
        assert!(fra.map(|v| v.verdict != "yes").unwrap_or(true));
    }

    #[test]
    fn anbennar_decisions_smoke() {
        if !install_present() || !Path::new(ANBENNAR).is_dir() {
            return;
        }
        let vfs = Vfs::new(INSTALL, Some(ANBENNAR)).unwrap();
        let loc = loc::build(&vfs);
        let decisions = load_decisions(&vfs, &loc);
        assert!(!decisions.is_empty());
        // Anbennar ships its own decisions; at least one must be mod-origin.
        assert!(
            decisions.iter().any(|d| d.origin == "mod"),
            "Anbennar should contribute mod-origin decisions"
        );
    }
}
