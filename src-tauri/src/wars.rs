//! Sprint 13 — `history/wars` parsing, active-at-date folding, and validation.
//!
//! ## File format (verified against vanilla + Anbennar)
//! Each `history/wars/*.txt` is a single war:
//! ```text
//! name = "Crusade of Varna"
//! war_goal = { type = superiority_crusade casus_belli = cb_crusade }
//! 1443.1.1 = { add_attacker = HUN add_attacker = SER ... add_defender = TUR }
//! 1444.8.1 = { rem_attacker = KAR }
//! 1444.11.10 = { battle = { ... } }
//! 1444.11.11 = { rem_attacker = HUN ... rem_defender = TUR }
//! ```
//! `war_goal.type` names a `common/wargoal_types` entry; `war_goal.casus_belli`
//! names a `common/cb_types` entry. The optional target is `province = <id>`
//! (take_* goals) or `tag = <TAG>` (defend_* goals); superiority goals carry
//! neither. Participants are folded from the dated `add_/rem_attacker/defender`
//! blocks. `battle = { ... }` blocks we never model — they are preserved
//! byte-exactly on write (edits are byte-surgical per `mod_writer`), and only a
//! read-only count is surfaced.
//!
//! ## Active-at-date boundary semantics (decided deliberately, Sprint 13.5)
//! A participant is *active at `at`* iff it **joined ≤ at** and **has not left by
//! then**, where leaving *on* `at` counts as already gone (`leave_date > at`).
//! A war is active iff any participant is active. This matches EU4's own start:
//! `CrusadeOfVarna` has every participant `rem_*` on exactly 1444.11.11, so the
//! Crusade of Varna is NOT active at the 1444.11.11 grand-campaign start (it is
//! active at 1444.11.10 and every earlier date back to its 1443.1.1 join). The
//! spec's 13.5 "Ottoman–Hungarian war active at 1444" claim is wrong against the
//! files: `FirstOttomanHungarianWar` starts 1453.1.1, so it is inactive at 1444
//! and active at any 1453+ date. The tests encode this honestly.
//!
//! ## Replace-path
//! Anbennar `replace_path`s `history/wars` wholesale; iterating via
//! `Vfs::list_dir` yields the mod's files only in that case (matching
//! `diplomacy::all_relations`).
//!
//! ## Editing (byte-surgical; the frontend generates the TypedEdits)
//!   * rename a war       → `SetScalar { path: ["name"], value, quoted: true }`
//!   * change a war goal  → `SetScalar`/`InsertStatement`/`RemoveStatement` on
//!                          the `war_goal` block's `type`/`casus_belli`/target.
//!   * add a participant  → `InsertStatement` `add_attacker = TAG` into the join
//!                          date's block (merge if present), else an
//!                          `InsertDatedBlock` creating it. War files may carry
//!                          duplicate same-date blocks legitimately; the toolkit's
//!                          `key#n` occurrence addressing reaches any of them.
//!   * leave / remove     → `InsertStatement` `rem_attacker = TAG` into the
//!                          leave date's block (or a new dated block).
//!   * new war            → `CreateFile` at `history/wars/zz_eutoolkit_<slug>.txt`.
//!   * delete war         → toolkit-created files: `DeleteFile` (project-only).
//!     Base wars: EU4 has no "hide this war" override mechanism, so the only
//!     lever is a project file of the same name shadowing the base file through
//!     the Vfs. An empty (or comment-only) shadow parses to "no war" on our side
//!     (see [`all_wars_at`] — nameless, participantless files are dropped), so
//!     the war vanishes from the toolkit. Whether the *game* tolerates an empty
//!     `history/wars` file is unverified here (no in-game test available); the
//!     frontend confirms base-war deletion and documents the caveat.

use std::collections::{HashMap, HashSet};

use crate::date::{self, Date};
#[cfg(test)]
use crate::date::DEFAULT_START;
use crate::loc::LocStore;
use crate::paradox::{self, Block, Value};
use crate::validation::{JumpTarget, Severity, ValidationIssue};
use crate::vfs::Vfs;

/// An unmodeled `key = value` carried through untouched (preserve-unknown).
#[derive(Debug, Clone, serde::Serialize)]
pub struct RawKv {
    pub key: String,
    pub value: String,
}

/// A war's `war_goal` block: the modeled fields plus preserved extras.
#[derive(Debug, Clone, serde::Serialize)]
pub struct WarGoal {
    /// `type` — a `common/wargoal_types` key (`take_claim`, `superiority_crusade`…).
    pub goal_type: Option<String>,
    /// `casus_belli` — a `common/cb_types` key.
    pub casus_belli: Option<String>,
    /// Target province (take_* goals).
    pub province: Option<u32>,
    /// Target tag (defend_* goals).
    pub tag: Option<String>,
    /// Any other scalar in the block (preserve-unknown).
    pub raw_extra: Vec<RawKv>,
}

/// One folded war participant.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Participant {
    pub tag: String,
    /// `attacker` | `defender`.
    pub side: String,
    /// Date the tag was first `add_*`ed (its join), if any.
    pub join_date: Option<String>,
    /// Date the tag was `rem_*`ed (its leave); `None` if it never left.
    pub leave_date: Option<String>,
}

/// One war (one `history/wars` file).
#[derive(Debug, Clone, serde::Serialize)]
pub struct War {
    /// Game-relative file path.
    pub file: String,
    /// Literal war name (the quoted string, unquoted).
    pub name: Option<String>,
    pub war_goal: Option<WarGoal>,
    /// Participants in first-appearance order.
    pub participants: Vec<Participant>,
    /// Count of `battle = { ... }` blocks (read-only; never edited/dropped).
    pub battle_count: usize,
    /// Whether the war is a real, ongoing war at the query date: BOTH sides have
    /// at least one active participant (see module docs). Vanilla's Crusade of
    /// Varna leaves MOL as an attacker who is never `rem_`'d, so a pure "any
    /// participant still in" rule wrongly keeps it active at 1444.11.11 — the
    /// both-sides rule is what makes the 1444 start correctly war-free.
    pub active_at_date: bool,
}

impl Participant {
    /// Active at `at`: joined ≤ at and not left by then (leave-on-`at` = left).
    fn active_at(&self, at: Date) -> bool {
        let joined = self
            .join_date
            .as_deref()
            .and_then(date::parse_date)
            .is_some_and(|j| j <= at);
        let not_left = match self.leave_date.as_deref().and_then(date::parse_date) {
            Some(l) => l > at,
            None => true,
        };
        joined && not_left
    }
}

/// A war is active at `at` iff both sides have an active participant.
fn war_active_at(participants: &[Participant], at: Date) -> bool {
    let side_active = |side: &str| {
        participants
            .iter()
            .any(|p| p.side == side && p.active_at(at))
    };
    side_active("attacker") && side_active("defender")
}

/// Parses one war file's block into a [`War`], computing `active_at_date` at `at`.
fn war_from_block(file: &str, block: &Block, at: Date) -> War {
    let name = block.get_scalar("name").map(str::to_string);
    let war_goal = block.get_block("war_goal").map(war_goal_from_block);

    // Fold participants from the dated blocks in file order (vanilla war files
    // are written chronologically, and file order is what the join-after-leave
    // validation keys on). Per (tag, side): join = the FIRST `add_*` date, leave
    // = the LAST `rem_*` date. This is deliberately shallow — a tag that rejoins
    // after leaving collapses to one span (Sprint 13 doesn't model re-entry).
    let mut index: HashMap<(String, String), usize> = HashMap::new();
    let mut folded: Vec<Participant> = Vec::new();
    let mut battle_count = 0usize;

    for (k, v) in &block.items {
        let (Some(k), Value::Block(b)) = (k, v) else {
            continue;
        };
        let Some(d) = date::parse_date(k) else { continue };
        let date_str = date::format_date(d);
        for (ek, ev) in &b.items {
            let (Some(ek), Value::Scalar(tag)) = (ek, ev) else {
                // Count battles (block-valued `battle` entries).
                if let (Some(ek), Value::Block(_)) = (ek, ev) {
                    if ek == "battle" {
                        battle_count += 1;
                    }
                }
                continue;
            };
            let (side, joining) = match ek.as_str() {
                "add_attacker" => ("attacker", true),
                "add_defender" => ("defender", true),
                "rem_attacker" => ("attacker", false),
                "rem_defender" => ("defender", false),
                _ => continue,
            };
            let key = (tag.clone(), side.to_string());
            let i = *index.entry(key).or_insert_with(|| {
                folded.push(Participant {
                    tag: tag.clone(),
                    side: side.to_string(),
                    join_date: None,
                    leave_date: None,
                });
                folded.len() - 1
            });
            if joining {
                if folded[i].join_date.is_none() {
                    folded[i].join_date = Some(date_str.clone());
                }
            } else {
                folded[i].leave_date = Some(date_str.clone());
            }
        }
    }

    let active_at_date = war_active_at(&folded, at);

    War {
        file: file.to_string(),
        name,
        war_goal,
        participants: folded,
        battle_count,
        active_at_date,
    }
}

/// Turns a `war_goal = { ... }` block into a [`WarGoal`], preserving unknown keys.
fn war_goal_from_block(block: &Block) -> WarGoal {
    let modeled = ["type", "casus_belli", "province", "tag"];
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
    WarGoal {
        goal_type: block.get_scalar("type").map(str::to_string),
        casus_belli: block.get_scalar("casus_belli").map(str::to_string),
        province: block.get_scalar("province").and_then(|s| s.parse().ok()),
        tag: block.get_scalar("tag").map(str::to_string),
        raw_extra,
    }
}

/// Every war under `history/wars`, folded at the effective start date.
/// Pre-Sprint-12 signature; used by tests.
#[cfg(test)]
pub fn all_wars(vfs: &Vfs) -> Vec<War> {
    all_wars_at(vfs, DEFAULT_START)
}

/// Every war under `history/wars` (replace_path aware), with `active_at_date`
/// evaluated at `at`. A file that parses to no name AND no participants is
/// dropped: that is exactly the empty/comment-only shadow a base-war deletion
/// leaves, so a deleted base war disappears from the list.
pub fn all_wars_at(vfs: &Vfs, at: Date) -> Vec<War> {
    let mut out = Vec::new();
    for (name, path) in vfs.list_dir("history/wars") {
        if !name.to_lowercase().ends_with(".txt") {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let block = paradox::parse(&String::from_utf8_lossy(&bytes));
        let rel = format!("history/wars/{name}");
        let war = war_from_block(&rel, &block, at);
        if war.name.is_none() && war.participants.is_empty() {
            continue; // empty shadow / not a real war
        }
        out.push(war);
    }
    out
}

/// All wars, optionally filtered to those involving `tag` (as any participant),
/// each carrying its `active_at_date` flag as of `date`.
// Registered by the orchestrator in lib.rs; unused until then.
#[allow(dead_code)]
#[tauri::command]
pub fn get_wars(
    install_path: String,
    mod_path: Option<String>,
    date: Option<String>,
    tag: Option<String>,
) -> Result<Vec<War>, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    let at = crate::bookmarks::resolve_date(&vfs, date.as_deref())?;
    let mut wars = all_wars_at(&vfs, at);
    if let Some(tag) = tag {
        wars.retain(|w| w.participants.iter().any(|p| p.tag == tag));
    }
    Ok(wars)
}

// --- Validation domain: "wars" (Sprint 13.4) -----------------------------
//
// Whole-game checks over every war, at the selected date:
//   * a war active at `at` with zero active attackers or zero active defenders  → Error
//   * a participant whose join date is after its leave date                     → Error
//   * a war goal province/tag that does not exist                               → Error
//   * a province occupied (controller ≠ owner) where the controller is not at
//     war with the owner at `at` — vanilla ships a few; rebel-held (REB) skipped → Warning
// Jump targets reuse Country/Province/File (no new variant needed).

/// The `wars` validation domain evaluated at `at`.
pub fn check_wars(vfs: &Vfs, loc: &LocStore, at: Date) -> Vec<ValidationIssue> {
    let wars = all_wars_at(vfs, at);
    let mut issues = Vec::new();

    let name = |tag: &str| {
        let n = loc.resolve_or(tag, tag);
        if n == tag {
            tag.to_string()
        } else {
            format!("{n} ({tag})")
        }
    };
    let war_label = |w: &War| {
        w.name
            .clone()
            .unwrap_or_else(|| w.file.rsplit('/').next().unwrap_or(&w.file).to_string())
    };

    let provinces = all_province_ids(vfs);
    let tags = all_country_tags(vfs);

    for w in &wars {
        // 1. Structurally one-sided war: a whole side is never defined. Because
        //    `active_at_date` already requires BOTH sides to have an active
        //    participant, the literal "zero of a side while active" is vacuous;
        //    the real defect a war can carry is a side with no participants at
        //    all (e.g. add_attacker but never any add_defender). Vanilla defines
        //    both sides for every war, so this stays error-free there.
        let attackers = w.participants.iter().filter(|p| p.side == "attacker").count();
        let defenders = w.participants.iter().filter(|p| p.side == "defender").count();
        if attackers == 0 || defenders == 0 {
            issues.push(ValidationIssue {
                severity: Severity::Error,
                message: format!(
                    "War \"{}\" has {} attacker(s) and {} defender(s) — a side is undefined",
                    war_label(w),
                    attackers,
                    defenders
                ),
                jump: Some(JumpTarget::File(w.file.clone())),
            });
        }

        // 2. Participant join after leave.
        for p in &w.participants {
            if let (Some(j), Some(l)) = (
                p.join_date.as_deref().and_then(date::parse_date),
                p.leave_date.as_deref().and_then(date::parse_date),
            ) {
                if j > l {
                    issues.push(ValidationIssue {
                        severity: Severity::Error,
                        message: format!(
                            "War \"{}\": {} joins ({}) after it leaves ({})",
                            war_label(w),
                            name(&p.tag),
                            p.join_date.as_deref().unwrap_or("?"),
                            p.leave_date.as_deref().unwrap_or("?"),
                        ),
                        jump: Some(JumpTarget::Country(p.tag.clone())),
                    });
                }
            }
        }

        // 3. War goal target that doesn't exist.
        if let Some(goal) = &w.war_goal {
            if let Some(prov) = goal.province {
                if !provinces.is_empty() && !provinces.contains(&prov) {
                    issues.push(ValidationIssue {
                        severity: Severity::Error,
                        message: format!(
                            "War \"{}\" war goal targets province {prov}, which does not exist",
                            war_label(w)
                        ),
                        jump: Some(JumpTarget::Province(prov)),
                    });
                }
            }
            if let Some(t) = &goal.tag {
                if !tags.is_empty() && !tags.contains(t) {
                    issues.push(ValidationIssue {
                        severity: Severity::Error,
                        message: format!(
                            "War \"{}\" war goal targets tag {}, which does not exist",
                            war_label(w),
                            name(t)
                        ),
                        jump: Some(JumpTarget::Country(t.clone())),
                    });
                }
            }
        }

        // 3b. A participant tag that no longer exists — the dangling reference a
        //     country deletion leaves behind (Sprint 13.4 / S2.1). A Warning, not
        //     an Error: the game silently drops an unknown war participant, but the
        //     war is now malformed and the modder should know. Special/dynamic tags
        //     (REB/PIR/… and the [A-Z][0-9][0-9] runtime space) are never expected
        //     in the on-disk tag universe, so they don't warn.
        if !tags.is_empty() {
            let mut warned: HashSet<&str> = HashSet::new();
            for p in &w.participants {
                if is_special_tag(&p.tag) || tags.contains(&p.tag) {
                    continue;
                }
                if !warned.insert(p.tag.as_str()) {
                    continue;
                }
                issues.push(ValidationIssue {
                    severity: Severity::Warning,
                    message: format!(
                        "War \"{}\" references participant {}, which no longer exists",
                        war_label(w),
                        name(&p.tag)
                    ),
                    jump: Some(JumpTarget::Country(p.tag.clone())),
                });
            }
        }
    }

    // 4. Occupation without a war between owner and controller (warning). Build
    // the active-side sets once, then scan occupied provinces.
    let mut belligerents: Vec<(HashSet<String>, HashSet<String>)> = Vec::new();
    for w in &wars {
        if !w.active_at_date {
            continue;
        }
        let mut atk = HashSet::new();
        let mut def = HashSet::new();
        for p in &w.participants {
            if !p.active_at(at) {
                continue;
            }
            if p.side == "attacker" {
                atk.insert(p.tag.clone());
            } else {
                def.insert(p.tag.clone());
            }
        }
        belligerents.push((atk, def));
    }
    let at_war = |a: &str, b: &str| {
        belligerents.iter().any(|(atk, def)| {
            (atk.contains(a) && def.contains(b)) || (atk.contains(b) && def.contains(a))
        })
    };

    let mut occupied: Vec<(u32, String, String)> = Vec::new();
    for (id, state) in crate::game_data::province_history_at(vfs, at) {
        let (Some(owner), Some(controller)) = (state.owner, state.controller) else {
            continue;
        };
        // Rebel-held provinces (REB) are not a war participant; skip.
        if owner == controller || controller == "REB" {
            continue;
        }
        if !at_war(&owner, &controller) {
            occupied.push((id, owner, controller));
        }
    }
    occupied.sort_unstable_by_key(|(id, _, _)| *id);
    for (id, owner, controller) in occupied {
        issues.push(ValidationIssue {
            severity: Severity::Warning,
            message: format!(
                "Province {id} is occupied by {} but {} is not at war with its owner {} at {}",
                name(&controller),
                name(&controller),
                name(&owner),
                date::format_date(at),
            ),
            jump: Some(JumpTarget::Province(id)),
        });
    }

    issues
}

/// Tags that never appear in the on-disk country-tag universe: the fixed special
/// countries (REB/PIR/NAT/AUX) and the `[A-Z][0-9][0-9]` runtime-dynamic space
/// (colonial nations, client states, …). A war referencing one of these is not a
/// dangling reference, so participant-existence validation skips them.
fn is_special_tag(tag: &str) -> bool {
    if matches!(tag, "REB" | "PIR" | "NAT" | "AUX") {
        return true;
    }
    let b = tag.as_bytes();
    b.len() == 3 && b[0].is_ascii_uppercase() && b[1].is_ascii_digit() && b[2].is_ascii_digit()
}

/// Every province id from definition.csv (the universe war goals must target).
fn all_province_ids(vfs: &Vfs) -> HashSet<u32> {
    let mut out = HashSet::new();
    if let Ok(bytes) = vfs.read("map/definition.csv") {
        for line in String::from_utf8_lossy(&bytes).lines() {
            if let Some(Ok(id)) = line.split(';').next().map(|s| s.trim().parse::<u32>()) {
                out.insert(id);
            }
        }
    }
    out
}

/// Every country tag (the tag universe defend_* goals must target). Uses the
/// same country-color set the political render keys on, so mod tags are included.
fn all_country_tags(vfs: &Vfs) -> HashSet<String> {
    crate::game_data::country_colors(vfs).into_keys().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    const INSTALL: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Europa Universalis IV";
    const ANBENNAR: &str = r"C:\Users\User\projects\anbennar-eu4-dev";

    /// A synthetic install with the given files. One dir per test.
    fn synthetic(name: &str, files: &[(&str, &str)]) -> (PathBuf, Vfs) {
        let root = std::env::temp_dir().join(format!("eu_toolkit_wars_test_{name}"));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("map")).unwrap();
        std::fs::create_dir_all(root.join("history/wars")).unwrap();
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

    const VARNA: &str = "\
name = \"Crusade of Varna\"
war_goal = {
\ttype = superiority_crusade
\tcasus_belli = cb_crusade
}
1443.1.1 = {
\tadd_attacker = HUN
\tadd_attacker = POL
\tadd_defender = TUR
}
1444.11.10 = {
\tbattle = {
\t\tname = \"Varna\"
\t\tlocation = 150
\t}
}
1444.11.11 = {
\trem_attacker = HUN
\trem_attacker = POL
\trem_defender = TUR
}
";

    const OTTO_HUN: &str = "\
name = \"First Ottoman-Hungarian War\"
war_goal = {
\ttype = take_claim
\tcasus_belli = cb_conquest
\tprovince = 156
}
1453.1.1 = {
\tadd_attacker = TUR
\tadd_defender = HUN
}
";

    fn war<'a>(wars: &'a [War], name: &str) -> &'a War {
        wars.iter()
            .find(|w| w.name.as_deref() == Some(name))
            .unwrap_or_else(|| panic!("no war named {name}"))
    }

    #[test]
    fn parses_war_goal_and_participant_fold() {
        let (_r, vfs) = synthetic("fold", &[("history/wars/Varna.txt", VARNA)]);
        let wars = all_wars(&vfs);
        let v = war(&wars, "Crusade of Varna");
        // War goal: superiority goal → no target province/tag, cb preserved.
        let goal = v.war_goal.as_ref().unwrap();
        assert_eq!(goal.goal_type.as_deref(), Some("superiority_crusade"));
        assert_eq!(goal.casus_belli.as_deref(), Some("cb_crusade"));
        assert_eq!(goal.province, None);
        assert_eq!(goal.tag, None);
        // Participants folded with join+leave dates matching the dated blocks.
        assert_eq!(v.participants.len(), 3);
        let hun = v.participants.iter().find(|p| p.tag == "HUN").unwrap();
        assert_eq!(hun.side, "attacker");
        assert_eq!(hun.join_date.as_deref(), Some("1443.1.1"));
        assert_eq!(hun.leave_date.as_deref(), Some("1444.11.11"));
        let tur = v.participants.iter().find(|p| p.tag == "TUR").unwrap();
        assert_eq!(tur.side, "defender");
        assert_eq!(tur.leave_date.as_deref(), Some("1444.11.11"));
        // One battle, counted not modeled.
        assert_eq!(v.battle_count, 1);
    }

    #[test]
    fn war_goal_province_target_parsed() {
        let (_r, vfs) = synthetic("goaltarget", &[("history/wars/OH.txt", OTTO_HUN)]);
        let wars = all_wars(&vfs);
        let goal = war(&wars, "First Ottoman-Hungarian War")
            .war_goal
            .as_ref()
            .unwrap();
        assert_eq!(goal.goal_type.as_deref(), Some("take_claim"));
        assert_eq!(goal.province, Some(156));
    }

    #[test]
    fn varna_boundary_semantics() {
        // Leave-on-date counts as LEFT: Varna is active at 1444.11.10 and every
        // earlier date, but NOT at the 1444.11.11 grand-campaign start.
        let (_r, vfs) = synthetic("boundary", &[("history/wars/Varna.txt", VARNA)]);
        assert!(
            !all_wars_at(&vfs, (1444, 11, 11))[0].active_at_date,
            "Varna must be inactive at 1444.11.11 (all participants rem on that date)"
        );
        assert!(
            all_wars_at(&vfs, (1444, 11, 10))[0].active_at_date,
            "Varna must be active at 1444.11.10 (day before Varna battle end)"
        );
        assert!(
            all_wars_at(&vfs, (1443, 6, 1))[0].active_at_date,
            "Varna must be active mid-1443"
        );
        // Before it starts: inactive.
        assert!(!all_wars_at(&vfs, (1442, 1, 1))[0].active_at_date);
    }

    #[test]
    fn active_at_date_differs_1444_vs_1453() {
        // Both wars present. At 1444: neither active (Varna just ended; OH not
        // started). At 1453: OH active, Varna long over.
        let (_r, vfs) = synthetic(
            "twowars",
            &[
                ("history/wars/Varna.txt", VARNA),
                ("history/wars/OH.txt", OTTO_HUN),
            ],
        );
        let at_1444 = all_wars_at(&vfs, DEFAULT_START);
        assert!(!war(&at_1444, "Crusade of Varna").active_at_date);
        assert!(!war(&at_1444, "First Ottoman-Hungarian War").active_at_date);
        let at_1453 = all_wars_at(&vfs, (1453, 6, 1));
        assert!(!war(&at_1453, "Crusade of Varna").active_at_date);
        assert!(war(&at_1453, "First Ottoman-Hungarian War").active_at_date);
    }

    #[test]
    fn get_wars_filters_by_tag() {
        let (_r, vfs) = synthetic(
            "tagfilter",
            &[
                ("history/wars/Varna.txt", VARNA),
                ("history/wars/OH.txt", OTTO_HUN),
            ],
        );
        // TUR is in both; HUN in both; POL only in Varna.
        let all = all_wars_at(&vfs, DEFAULT_START);
        let only_pol: Vec<_> = all
            .iter()
            .filter(|w| w.participants.iter().any(|p| p.tag == "POL"))
            .collect();
        assert_eq!(only_pol.len(), 1);
        assert_eq!(only_pol[0].name.as_deref(), Some("Crusade of Varna"));
    }

    #[test]
    fn empty_or_comment_only_file_is_not_a_war() {
        // A base-war-deletion shadow: an empty (or comment-only) file yields no
        // war in the list, so the war disappears.
        let (_r, vfs) = synthetic(
            "emptyshadow",
            &[
                ("history/wars/Empty.txt", ""),
                ("history/wars/CommentOnly.txt", "# war removed by EU Toolkit\n"),
                ("history/wars/Varna.txt", VARNA),
            ],
        );
        let wars = all_wars(&vfs);
        assert_eq!(wars.len(), 1, "only the real war survives: {wars:?}");
        assert_eq!(wars[0].name.as_deref(), Some("Crusade of Varna"));
    }

    #[test]
    fn preserves_unknown_war_goal_keys() {
        let (_r, vfs) = synthetic(
            "rawextra",
            &[(
                "history/wars/Custom.txt",
                "name = \"Custom\"\nwar_goal = { type = superiority casus_belli = cb_x custom_key = 42 }\n1400.1.1 = { add_attacker = AAA add_defender = BBB }\n",
            )],
        );
        let wars = all_wars(&vfs);
        let goal = war(&wars, "Custom").war_goal.as_ref().unwrap();
        assert!(goal
            .raw_extra
            .iter()
            .any(|kv| kv.key == "custom_key" && kv.value == "42"));
    }

    // --- round-trip edits via the byte-surgical toolkit ----------------------

    use crate::mod_writer::{apply, Edit};

    #[test]
    fn rename_war_is_byte_surgical() {
        let out = apply(
            VARNA.as_bytes(),
            &Edit::SetScalar {
                path: vec!["name".into()],
                value: "Renamed War".into(),
                quoted: true,
            },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("name = \"Renamed War\""));
        // War goal, participants, and the battle block are untouched.
        assert!(text.contains("type = superiority_crusade"));
        assert!(text.contains("add_attacker = HUN"));
        assert!(text.contains("name = \"Varna\""));
    }

    #[test]
    fn add_and_remove_participant_round_trip() {
        // Add SCO into the existing 1443.1.1 block, then a rem into the 1444.11.11
        // block. Battles + every unmodeled byte survive.
        let added = apply(
            VARNA.as_bytes(),
            &Edit::InsertStatement {
                block_path: vec!["1443.1.1".into()],
                statement: "add_attacker = SCO".into(),
            },
        )
        .unwrap();
        let out = apply(
            &added,
            &Edit::InsertStatement {
                block_path: vec!["1444.11.11".into()],
                statement: "rem_attacker = SCO".into(),
            },
        )
        .unwrap();
        // Re-parse: SCO now a folded participant with join+leave.
        let block = paradox::parse(&String::from_utf8_lossy(&out));
        let w = war_from_block("history/wars/Varna.txt", &block, DEFAULT_START);
        let sco = w.participants.iter().find(|p| p.tag == "SCO").unwrap();
        assert_eq!(sco.join_date.as_deref(), Some("1443.1.1"));
        assert_eq!(sco.leave_date.as_deref(), Some("1444.11.11"));
        // Battle preserved, count unchanged.
        assert_eq!(w.battle_count, 1);
        assert!(String::from_utf8_lossy(&out).contains("name = \"Varna\""));
    }

    #[test]
    fn change_war_goal_type_round_trip() {
        let out = apply(
            OTTO_HUN.as_bytes(),
            &Edit::SetScalar {
                path: vec!["war_goal".into(), "type".into()],
                value: "take_capital".into(),
                quoted: false,
            },
        )
        .unwrap();
        let block = paradox::parse(&String::from_utf8_lossy(&out));
        let w = war_from_block("f.txt", &block, DEFAULT_START);
        assert_eq!(w.war_goal.unwrap().goal_type.as_deref(), Some("take_capital"));
    }

    #[test]
    fn scaffold_new_war_parses_back() {
        // The shape the +New war flow scaffolds: name + war_goal + an initial
        // dated block. It must parse back to a well-formed war.
        let scaffold = "name = \"My New War\"\nwar_goal = {\n\ttype = take_claim\n\tcasus_belli = cb_conquest\n\tprovince = 1\n}\n1500.1.1 = {\n\tadd_attacker = FRA\n\tadd_defender = ENG\n}\n";
        let out = apply(b"", &Edit::CreateFile { text: scaffold.into() }).unwrap();
        let block = paradox::parse(&String::from_utf8_lossy(&out));
        let w = war_from_block("history/wars/zz_eutoolkit_my_new_war.txt", &block, (1500, 6, 1));
        assert_eq!(w.name.as_deref(), Some("My New War"));
        assert_eq!(w.participants.len(), 2);
        assert!(w.active_at_date);
        assert_eq!(w.war_goal.unwrap().province, Some(1));
    }

    // --- validation ----------------------------------------------------------

    fn loc_for(vfs: &Vfs) -> LocStore {
        crate::loc::build(vfs)
    }

    fn defs_and_tags() -> (&'static str, &'static str) {
        // definition.csv with provinces 1,2,3; two country color files give tags.
        (
            "province;red;green;blue;name;x\n1;1;1;1;A;x\n2;2;2;2;B;x\n3;3;3;3;C;x\n",
            "",
        )
    }

    #[test]
    fn validation_flags_missing_defender_side() {
        // A war active at 1500 with attackers but no defenders.
        let (_r, vfs) = synthetic(
            "onesided",
            &[(
                "history/wars/One.txt",
                "name = \"One-sided\"\n1500.1.1 = { add_attacker = FRA }\n",
            )],
        );
        // The war starts 1500, so evaluate there (it's inactive at 1444).
        let issues = check_wars(&vfs, &loc_for(&vfs), (1500, 6, 1));
        assert!(
            issues
                .iter()
                .any(|i| i.severity == Severity::Error && i.message.contains("0 defender")),
            "expected missing-defender error: {issues:?}"
        );
    }

    #[test]
    fn validation_flags_join_after_leave() {
        let (_r, vfs) = synthetic(
            "joinleave",
            &[(
                "history/wars/Bad.txt",
                "name = \"Bad\"\n1500.1.1 = { add_attacker = FRA add_defender = ENG }\n1400.1.1 = { rem_attacker = FRA }\n",
            )],
        );
        // FRA joins 1500 but leaves 1400 → join after leave.
        let issues = check_wars(&vfs, &loc_for(&vfs), (1600, 1, 1));
        assert!(
            issues
                .iter()
                .any(|i| i.severity == Severity::Error && i.message.contains("after it leaves")),
            "expected join-after-leave error: {issues:?}"
        );
    }

    #[test]
    fn validation_warns_dangling_participant_tag() {
        // FRA and ENG exist (country color files); GONE does not. A war with GONE
        // as a participant warns (never errors), with a country jump target. REB
        // and a dynamic C01 tag are skipped.
        let (_r, vfs) = synthetic(
            "dangling",
            &[
                ("map/definition.csv", "province;red;green;blue;name;x\n1;1;1;1;A;x\n"),
                ("common/countries/France.txt", "color = { 1 1 1 }\n"),
                ("common/countries/England.txt", "color = { 2 2 2 }\n"),
                ("common/country_tags/00.txt", "FRA = \"countries/France.txt\"\nENG = \"countries/England.txt\"\n"),
                (
                    "history/wars/W.txt",
                    "name = \"W\"\n1500.1.1 = { add_attacker = FRA add_attacker = GONE add_defender = ENG add_defender = REB add_defender = C01 }\n",
                ),
            ],
        );
        let issues = check_wars(&vfs, &loc_for(&vfs), (1500, 6, 1));
        let dangling: Vec<_> = issues
            .iter()
            .filter(|i| i.message.contains("no longer exists"))
            .collect();
        assert_eq!(dangling.len(), 1, "only GONE dangles: {issues:?}");
        assert_eq!(dangling[0].severity, Severity::Warning);
        assert_eq!(dangling[0].jump, Some(JumpTarget::Country("GONE".to_string())));
        // Never an error for a dangling participant.
        assert!(!issues.iter().any(|i| i.severity == Severity::Error));
    }

    #[test]
    fn validation_flags_missing_goal_province() {
        let (defs, _) = defs_and_tags();
        let (_r, vfs) = synthetic(
            "badprov",
            &[
                ("map/definition.csv", defs),
                (
                    "history/wars/W.txt",
                    "name = \"W\"\nwar_goal = { type = take_claim province = 9999 }\n1500.1.1 = { add_attacker = FRA add_defender = ENG }\n",
                ),
            ],
        );
        let issues = check_wars(&vfs, &loc_for(&vfs), (1500, 6, 1));
        assert!(
            issues
                .iter()
                .any(|i| i.severity == Severity::Error && i.message.contains("province 9999")),
            "expected missing-province error: {issues:?}"
        );
    }

    #[test]
    fn validation_occupation_without_war_is_warning() {
        // Province 1 owned by AAA, controlled by BBB, and there is NO war between
        // them → warning (never error).
        let (defs, _) = defs_and_tags();
        let (_r, vfs) = synthetic(
            "occupy",
            &[
                ("map/definition.csv", defs),
                (
                    "history/provinces/1 - A.txt",
                    "owner = AAA\ncontroller = BBB\n",
                ),
            ],
        );
        let issues = check_wars(&vfs, &loc_for(&vfs), DEFAULT_START);
        let w = issues
            .iter()
            .find(|i| i.jump == Some(JumpTarget::Province(1)));
        assert!(w.is_some(), "expected occupation warning: {issues:?}");
        assert_eq!(w.unwrap().severity, Severity::Warning);
        // Never an error.
        assert!(!issues.iter().any(|i| i.severity == Severity::Error));
    }

    #[test]
    fn validation_rebel_occupation_not_warned() {
        let (defs, _) = defs_and_tags();
        let (_r, vfs) = synthetic(
            "rebel",
            &[
                ("map/definition.csv", defs),
                (
                    "history/provinces/1 - A.txt",
                    "owner = AAA\ncontroller = REB\n",
                ),
            ],
        );
        let issues = check_wars(&vfs, &loc_for(&vfs), DEFAULT_START);
        assert!(
            !issues
                .iter()
                .any(|i| i.jump == Some(JumpTarget::Province(1))),
            "rebel-held provinces must not warn: {issues:?}"
        );
    }

    #[test]
    fn validation_occupation_with_active_war_is_clean() {
        // AAA occupies BBB's province 1, and AAA is at war with BBB at the date
        // → no occupation warning.
        let (defs, _) = defs_and_tags();
        let (_r, vfs) = synthetic(
            "occupy_at_war",
            &[
                ("map/definition.csv", defs),
                (
                    "history/provinces/1 - A.txt",
                    "owner = BBB\ncontroller = AAA\n",
                ),
                (
                    "history/wars/W.txt",
                    "name = \"W\"\n1440.1.1 = { add_attacker = AAA add_defender = BBB }\n",
                ),
            ],
        );
        let issues = check_wars(&vfs, &loc_for(&vfs), DEFAULT_START);
        assert!(
            !issues
                .iter()
                .any(|i| i.jump == Some(JumpTarget::Province(1))),
            "an active war between owner+controller means no warning: {issues:?}"
        );
    }

    // --- real install / Anbennar (no-op when absent) -------------------------

    fn real_vfs() -> Option<Vfs> {
        Path::new(INSTALL)
            .join("map/provinces.bmp")
            .is_file()
            .then(|| Vfs::new(INSTALL, None).unwrap())
    }

    #[test]
    fn real_varna_and_ottoman_hungarian() {
        let Some(vfs) = real_vfs() else { return };
        // Crusade of Varna: inactive at the 1444.11.11 start, active the day before.
        let start = all_wars_at(&vfs, DEFAULT_START);
        let varna = start
            .iter()
            .find(|w| w.name.as_deref() == Some("Crusade of Varna"))
            .expect("Crusade of Varna present in vanilla");
        assert!(!varna.active_at_date, "Varna inactive at 1444.11.11");
        assert!(varna.battle_count >= 1, "Varna records the Varna battle");
        let day_before = all_wars_at(&vfs, (1444, 11, 10));
        assert!(
            day_before
                .iter()
                .find(|w| w.name.as_deref() == Some("Crusade of Varna"))
                .unwrap()
                .active_at_date,
            "Varna active at 1444.11.10"
        );
        // First Ottoman-Hungarian War: inactive at 1444, active 1453+.
        let oh_name = "First Ottoman-Hungarian War";
        assert!(!start
            .iter()
            .find(|w| w.name.as_deref() == Some(oh_name))
            .expect("OH war present")
            .active_at_date);
        let at_1453 = all_wars_at(&vfs, (1453, 6, 1));
        assert!(at_1453
            .iter()
            .find(|w| w.name.as_deref() == Some(oh_name))
            .unwrap()
            .active_at_date);
    }

    #[test]
    fn real_wars_validation_no_errors() {
        let Some(vfs) = real_vfs() else { return };
        let loc = crate::loc::store(&vfs, INSTALL, None);
        for at in [DEFAULT_START, (1453, 6, 1), (1500, 1, 1)] {
            let issues = check_wars(&vfs, &loc, at);
            let errs = issues.iter().filter(|i| i.severity == Severity::Error).count();
            let warns = issues.iter().filter(|i| i.severity == Severity::Warning).count();
            println!(
                "[validation:wars @ {}] {} issues ({errs} error, {warns} warning)",
                date::format_date(at),
                issues.len()
            );
            for i in issues.iter().filter(|i| i.severity == Severity::Error) {
                println!("    ERROR: {}", i.message);
            }
            assert_eq!(errs, 0, "vanilla wars produced errors at {at:?}");
        }
    }

    #[test]
    fn anbennar_wars_smoke() {
        if !Path::new(INSTALL).join("map/provinces.bmp").is_file() || !Path::new(ANBENNAR).is_dir() {
            return;
        }
        let vfs = Vfs::new(INSTALL, Some(ANBENNAR)).unwrap();
        // Anbennar replace_paths history/wars; we must read its custom wars.
        let wars = all_wars_at(&vfs, (1444, 11, 1));
        assert!(!wars.is_empty(), "expected Anbennar wars");
        // Graytide: a superiority_monster goal with no target, custom Z-tags.
        let gray = wars
            .iter()
            .find(|w| w.name.as_deref() == Some("Graytide"))
            .expect("Graytide war present in Anbennar");
        let goal = gray.war_goal.as_ref().unwrap();
        assert_eq!(goal.goal_type.as_deref(), Some("superiority_monster"));
        assert_eq!(goal.province, None);
        assert_eq!(goal.tag, None);
        assert!(gray.participants.iter().any(|p| p.tag == "Z18"));
        // Validation runs without producing Rust panics / errors on a TC.
        let loc = crate::loc::store(&vfs, INSTALL, Some(ANBENNAR));
        let issues = check_wars(&vfs, &loc, (1444, 11, 1));
        let errs = issues.iter().filter(|i| i.severity == Severity::Error).count();
        println!("[validation:wars:anbennar] {} issues ({errs} error)", issues.len());
    }
}
