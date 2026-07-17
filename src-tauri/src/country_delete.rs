//! Sprint S2.1 — country deletion.
//!
//! The inverse of [`crate::country_create`]. Two backend commands:
//!   * [`get_country_blast_radius`] — everything the confirm dialog must show
//!     BEFORE anything happens: how many provinces the country owns, which
//!     diplomacy relations reference it (with the orphaned-subject/overlord ones
//!     called out), which wars reference it (active vs. historical, for the
//!     13.4 jump-link warning), tag references left in other provinces
//!     (`add_core` / `tribal_owner`), whether the tag is toolkit-created, and
//!     which associated files are toolkit-written.
//!   * [`prepare_country_deletion`] — the deletion composite: a flat, ordered
//!     [`crate::edits::TypedEdit`] list the frontend queues as ONE undo unit and
//!     applies on Save through [`crate::edits::apply_queue`], exactly like the
//!     create-country scaffold.
//!
//! ## What deletion does (all as pending edits, one composite)
//!   * **Owned provinces** (top-level `owner == tag`) → uncolonized (drop
//!     owner/controller and the tag's own core, keep culture/religion/goods —
//!     the same shape as the 1.4 remove-province tool), OR transferred to a
//!     chosen target (owner/controller ← target; the deleted tag's core is
//!     dropped; **no** target core is added — a transfer of ownership doesn't
//!     grant a core, matching the historical `owner`-without-`add_core` state).
//!   * **Diplomacy relations** referencing the tag are removed (byte-surgical
//!     `RemoveStatement` by occurrence, emitted high-index-first per file so the
//!     occurrence addressing of earlier removals doesn't shift later ones).
//!   * **Wars** are NOT auto-removed (13.4): the dialog shows a jump-link warning
//!     and the `wars` validation domain flags the dangling participant after the
//!     save. Deleting the country simply leaves the war referencing a gone tag.
//!   * **Tag registration** is removed from whichever `common/country_tags` file
//!     holds it — the toolkit's own `zz_eutoolkit_countries.txt` for a
//!     toolkit-created tag, or (copy-on-write) the base file for a base tag. The
//!     base install is never touched.
//!   * **Toolkit-written files** (country file, history file, flag, and the
//!     `TAG`/`TAG_ADJ` loc overrides) are deleted only when the tag is
//!     toolkit-created; a base country's files simply become unreferenced.
//!
//! A pending-created (unsaved) country isn't handled here at all — the frontend
//! deletes it by dropping its create composite from the edit queue.

use crate::diplomacy;
use crate::edits::TypedEdit;
use crate::game_data;
use crate::paradox::{self, Value};
use crate::vfs::Vfs;
use crate::wars;

// ---------------------------------------------------------------------------
// Wire types
// ---------------------------------------------------------------------------

/// One diplomacy relation the deletion will remove, from the deleted tag's point
/// of view — enough for the dialog to render a row + jump to the partner.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlastRelation {
    /// `alliance` | `guarantee` | `warning` | `royal_marriage` | `dependency`.
    pub relation_type: String,
    /// Subject type for dependencies (`vassal`/`union`/…), else null.
    pub subject_type: Option<String>,
    /// The role the DELETED tag plays: `overlord` (its subjects orphan),
    /// `subject` (its overlord loses a subject), or the relation type otherwise.
    pub role: String,
    /// The OTHER country in the relation — the jump-link target.
    pub partner: Option<String>,
    /// Active at the query date (drives the "orphaned relation" emphasis).
    pub active: bool,
}

/// One war the deleted tag participates in — a jump-link warning row (13.4).
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlastWar {
    pub file: String,
    pub name: Option<String>,
    pub active: bool,
    /// The tag's side in the war (`attacker`/`defender`), if resolvable.
    pub side: Option<String>,
    /// A belligerent on the opposite side — the jump-link target for the row.
    pub enemy: Option<String>,
}

/// The full blast radius of deleting `tag`, as of the query date.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CountryBlastRadius {
    pub tag: String,
    /// The tag is registered in the toolkit's own `zz_eutoolkit_countries.txt`
    /// (so its country/history/flag/loc are toolkit-written and get deleted).
    pub is_toolkit_created: bool,
    /// The `common/country_tags` file that registers the tag (removal target).
    pub tag_file: Option<String>,
    /// Province ids the tag owns at the top level (become uncolonized/transfer).
    pub owned_provinces: Vec<u32>,
    /// Diplomacy relations referencing the tag (removed on delete).
    pub relations: Vec<BlastRelation>,
    /// Wars active at the query date referencing the tag (jump-link warning).
    pub active_wars: Vec<BlastWar>,
    /// Wars referencing the tag that are inactive at the query date.
    pub historical_wars: Vec<BlastWar>,
    /// Provinces NOT owned by the tag that still hold a core for it
    /// (`add_core = TAG`) — left dangling, surfaced for awareness.
    pub core_references: Vec<u32>,
    /// Provinces with `tribal_owner = TAG` — left dangling, surfaced.
    pub tribal_owner_references: Vec<u32>,
    /// The country's `common/countries` file, if resolvable.
    pub country_file: Option<String>,
    /// The country's `history/countries` file, if resolvable.
    pub history_file: Option<String>,
    /// Game-relative flag path (`gfx/flags/TAG.tga`).
    pub flag_file: String,
    /// The files this delete will remove from the project (empty for a base tag).
    pub toolkit_files: Vec<String>,
}

// ---------------------------------------------------------------------------
// Tag registration lookup
// ---------------------------------------------------------------------------

/// Where a tag is registered: the `common/country_tags` file holding it and the
/// `common/countries` file it points at. `None` when the tag isn't registered.
struct Registration {
    tag_file: String,
    country_file: Option<String>,
}

/// Scans `common/country_tags` (Vfs-merged, so mod files shadow base) for the one
/// file that declares `tag` as a top-level key, returning that file and the
/// `common/`-relative country file the mapping points at.
fn find_registration(vfs: &Vfs, tag: &str) -> Option<Registration> {
    let want = tag.to_uppercase();
    for (name, path) in vfs.list_dir("common/country_tags") {
        if !name.to_lowercase().ends_with(".txt") {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let block = paradox::parse(&String::from_utf8_lossy(&bytes));
        for (k, v) in &block.items {
            let (Some(k), Value::Scalar(rel)) = (k, v) else {
                continue;
            };
            if k.to_uppercase() == want {
                return Some(Registration {
                    tag_file: format!("common/country_tags/{name}"),
                    country_file: Some(format!("common/{rel}")),
                });
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Blast radius
// ---------------------------------------------------------------------------

/// Owned-province edit facts for one province: its file plus which top-level keys
/// are present (so the composite emits set-vs-insert / remove precisely).
struct OwnedProvince {
    id: u32,
    file: String,
    controller_present: bool,
    has_core: bool,
}

/// Provinces the tag owns at the top level, plus core/tribal references from the
/// rest of the map. One pass over the bulk political payload + a targeted scan
/// for `tribal_owner` (which the political payload doesn't model).
fn scan_provinces(
    vfs: &Vfs,
    tag: &str,
) -> (Vec<OwnedProvince>, Vec<u32>, Vec<u32>) {
    let mut owned = Vec::new();
    let mut core_refs = Vec::new();
    for p in game_data::province_political(vfs) {
        let owns = p.owner.as_deref() == Some(tag);
        let has_core = p.cores.iter().any(|c| c == tag);
        if owns {
            owned.push(OwnedProvince {
                id: p.id,
                file: p.file.clone(),
                controller_present: p.controller.is_some(),
                has_core,
            });
        } else if has_core {
            core_refs.push(p.id);
        }
    }
    owned.sort_unstable_by_key(|o| o.id);
    core_refs.sort_unstable();

    // Targeted `tribal_owner = TAG` scan (top-level only).
    let mut tribal_refs = Vec::new();
    for (name, path) in vfs.list_dir("history/provinces") {
        if !name.to_lowercase().ends_with(".txt") {
            continue;
        }
        let digits: String = name.chars().take_while(|c| c.is_ascii_digit()).collect();
        let Ok(id) = digits.parse::<u32>() else {
            continue;
        };
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let block = paradox::parse(&String::from_utf8_lossy(&bytes));
        if block.get_scalar("tribal_owner") == Some(tag) {
            tribal_refs.push(id);
        }
    }
    tribal_refs.sort_unstable();

    (owned, core_refs, tribal_refs)
}

/// Diplomacy relations referencing `tag`, mapped to display/jump rows.
fn blast_relations(vfs: &Vfs, tag: &str, at: crate::date::Date) -> Vec<BlastRelation> {
    diplomacy::all_relations_at(vfs, at)
        .into_iter()
        .filter(|r| r.first.as_deref() == Some(tag) || r.second.as_deref() == Some(tag))
        .map(|r| {
            let is_first = r.first.as_deref() == Some(tag);
            let partner = if is_first { r.second.clone() } else { r.first.clone() };
            let role = if r.relation_type == "dependency" {
                if is_first { "overlord" } else { "subject" }.to_string()
            } else {
                r.relation_type.clone()
            };
            BlastRelation {
                relation_type: r.relation_type,
                subject_type: r.subject_type,
                role,
                partner,
                active: r.active_at_start,
            }
        })
        .collect()
}

/// Wars referencing `tag`, split into active-at-date and historical.
fn blast_wars(vfs: &Vfs, tag: &str, at: crate::date::Date) -> (Vec<BlastWar>, Vec<BlastWar>) {
    let mut active = Vec::new();
    let mut historical = Vec::new();
    for w in wars::all_wars_at(vfs, at) {
        if !w.participants.iter().any(|p| p.tag == tag) {
            continue;
        }
        let side = w
            .participants
            .iter()
            .find(|p| p.tag == tag)
            .map(|p| p.side.clone());
        let enemy = side.as_deref().and_then(|s| {
            let other = if s == "attacker" { "defender" } else { "attacker" };
            w.participants
                .iter()
                .find(|p| p.side == other)
                .map(|p| p.tag.clone())
        });
        let bw = BlastWar {
            file: w.file.clone(),
            name: w.name.clone(),
            active: w.active_at_date,
            side,
            enemy,
        };
        if w.active_at_date {
            active.push(bw);
        } else {
            historical.push(bw);
        }
    }
    (active, historical)
}

/// Builds the blast radius for deleting `tag` at `at`.
pub fn blast_radius(vfs: &Vfs, tag: &str, at: crate::date::Date) -> CountryBlastRadius {
    let reg = find_registration(vfs, tag);
    let tag_file = reg.as_ref().map(|r| r.tag_file.clone());
    let is_toolkit_created = tag_file.as_deref() == Some(crate::country_create::TAG_FILE);
    let country_file = reg.and_then(|r| r.country_file);
    let history_file =
        game_data::country_history_file(vfs, tag).map(|(name, _)| format!("history/countries/{name}"));
    let flag_file = format!("gfx/flags/{tag}.tga");

    let (owned, core_references, tribal_owner_references) = scan_provinces(vfs, tag);
    let owned_provinces: Vec<u32> = owned.iter().map(|o| o.id).collect();
    let relations = blast_relations(vfs, tag, at);
    let (active_wars, historical_wars) = blast_wars(vfs, tag, at);

    // The files a toolkit-created delete will remove.
    let mut toolkit_files = Vec::new();
    if is_toolkit_created {
        if let Some(f) = &country_file {
            toolkit_files.push(f.clone());
        }
        if let Some(f) = &history_file {
            toolkit_files.push(f.clone());
        }
        toolkit_files.push(flag_file.clone());
    }

    CountryBlastRadius {
        tag: tag.to_string(),
        is_toolkit_created,
        tag_file,
        owned_provinces,
        relations,
        active_wars,
        historical_wars,
        core_references,
        tribal_owner_references,
        country_file,
        history_file,
        flag_file,
        toolkit_files,
    }
}

// ---------------------------------------------------------------------------
// Deletion composite
// ---------------------------------------------------------------------------

/// Builds the ordered deletion edit list for `tag` at `at`. `transfer_to` names
/// the country the owned provinces go to (owner/controller ← target, deleted
/// tag's core dropped, no target core added); `None` uncolonizes them.
pub fn build_deletion(
    vfs: &Vfs,
    tag: &str,
    at: crate::date::Date,
    transfer_to: Option<&str>,
) -> Result<Vec<TypedEdit>, String> {
    let mut edits: Vec<TypedEdit> = Vec::new();

    let (owned, _core_refs, _tribal_refs) = scan_provinces(vfs, tag);

    // 1. Owned provinces → uncolonized or transferred.
    for o in &owned {
        match transfer_to {
            None => {
                // owner is always present here (owner == tag).
                edits.push(TypedEdit::RemoveStatement {
                    file: o.file.clone(),
                    block_path: vec![],
                    key: "owner".into(),
                    value: None,
                });
                if o.controller_present {
                    edits.push(TypedEdit::RemoveStatement {
                        file: o.file.clone(),
                        block_path: vec![],
                        key: "controller".into(),
                        value: None,
                    });
                }
                if o.has_core {
                    edits.push(TypedEdit::RemoveStatement {
                        file: o.file.clone(),
                        block_path: vec![],
                        key: "add_core".into(),
                        value: Some(tag.to_string()),
                    });
                }
            }
            Some(target) => {
                edits.push(TypedEdit::SetScalar {
                    file: o.file.clone(),
                    path: vec!["owner".into()],
                    value: target.to_string(),
                    quoted: false,
                });
                if o.controller_present {
                    edits.push(TypedEdit::SetScalar {
                        file: o.file.clone(),
                        path: vec!["controller".into()],
                        value: target.to_string(),
                        quoted: false,
                    });
                } else {
                    edits.push(TypedEdit::InsertStatement {
                        file: o.file.clone(),
                        block_path: vec![],
                        statement: format!("controller = {target}"),
                    });
                }
                // Drop the deleted tag's core; do NOT add a target core.
                if o.has_core {
                    edits.push(TypedEdit::RemoveStatement {
                        file: o.file.clone(),
                        block_path: vec![],
                        key: "add_core".into(),
                        value: Some(tag.to_string()),
                    });
                }
            }
        }
    }

    // 2. Diplomacy relations referencing the tag. Occurrence addressing shifts
    //    when an earlier same-key block in a file is removed, so remove
    //    high-index-first per file (indices are per-key, but a global
    //    descending sort within a file keeps every key's higher index first).
    let mut rels: Vec<diplomacy::Relation> = diplomacy::all_relations_at(vfs, at)
        .into_iter()
        .filter(|r| r.first.as_deref() == Some(tag) || r.second.as_deref() == Some(tag))
        .collect();
    rels.sort_by(|a, b| {
        a.file
            .cmp(&b.file)
            .then_with(|| b.block_index.cmp(&a.block_index))
    });
    for r in &rels {
        edits.push(TypedEdit::RemoveStatement {
            file: r.file.clone(),
            block_path: vec![],
            key: format!("{}#{}", r.block_key, r.block_index),
            value: None,
        });
    }

    // 3. Tag registration removal (toolkit or, copy-on-write, base). Never the
    //    base install — apply_queue writes into the project only.
    let reg = find_registration(vfs, tag);
    let is_toolkit_created =
        reg.as_ref().map(|r| r.tag_file.as_str()) == Some(crate::country_create::TAG_FILE);
    if let Some(reg) = &reg {
        edits.push(TypedEdit::RemoveStatement {
            file: reg.tag_file.clone(),
            block_path: vec![],
            key: tag.to_string(),
            value: None,
        });
    }

    // 4. Toolkit-written files + loc overrides (only for toolkit-created tags).
    if is_toolkit_created {
        if let Some(f) = reg.and_then(|r| r.country_file) {
            edits.push(TypedEdit::DeleteFile { file: f });
        }
        if let Some((name, _)) = game_data::country_history_file(vfs, tag) {
            edits.push(TypedEdit::DeleteFile {
                file: format!("history/countries/{name}"),
            });
        }
        edits.push(TypedEdit::DeleteFile {
            file: format!("gfx/flags/{tag}.tga"),
        });
        edits.push(TypedEdit::LocRemove { key: tag.to_string() });
        edits.push(TypedEdit::LocRemove {
            key: format!("{tag}_ADJ"),
        });
    }

    Ok(edits)
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// The blast radius of deleting `tag`, for the confirm dialog.
#[tauri::command]
pub fn get_country_blast_radius(
    install_path: String,
    mod_path: Option<String>,
    tag: String,
    date: Option<String>,
) -> Result<CountryBlastRadius, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    let at = crate::bookmarks::resolve_date(&vfs, date.as_deref())?;
    Ok(blast_radius(&vfs, &tag, at))
}

/// The deletion composite (queue verbatim as one undo unit). `transfer_to` moves
/// owned provinces to that country; omit it to uncolonize them.
#[tauri::command]
pub fn prepare_country_deletion(
    install_path: String,
    mod_path: Option<String>,
    tag: String,
    date: Option<String>,
    transfer_to: Option<String>,
) -> Result<Vec<TypedEdit>, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    let at = crate::bookmarks::resolve_date(&vfs, date.as_deref())?;
    build_deletion(&vfs, &tag, at, transfer_to.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::country_create::{build_scaffold, TAG_FILE};
    use crate::date::DEFAULT_START;
    use crate::edits::apply_queue;
    use std::collections::HashSet;
    use std::path::{Path, PathBuf};

    const INSTALL: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Europa Universalis IV";
    const ANBENNAR: &str = r"C:\Users\User\projects\anbennar-eu4-dev";

    /// A synthetic base install: two land provinces owned by FRA (10, 11), one
    /// uncolonized land province that holds a stale FRA core (12), FRA registered
    /// in a BASE tag file, an FRA country + history file, and an FRA-SCO alliance.
    fn synthetic(name: &str) -> (PathBuf, Vfs) {
        let root = std::env::temp_dir().join(format!("eu_toolkit_cdel_test_{name}"));
        let _ = std::fs::remove_dir_all(&root);
        let w = |rel: &str, bytes: &[u8]| {
            let p = root.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, bytes).unwrap();
        };
        w("map/provinces.bmp", b"x");
        w(
            "map/definition.csv",
            b"province;red;green;blue;name;x\n10;10;20;30;Aaa;x\n11;11;21;31;Bbb;x\n12;12;22;32;Ccc;x\n99;0;0;255;Sea;x\n",
        );
        w("map/default.map", b"sea_starts = { 99 }\nlakes = { }\n");
        w("map/climate.txt", b"tropical = { 10 }\nimpassable = { }\n");
        w(
            "history/provinces/10 - Aaa.txt",
            b"owner = FRA\ncontroller = FRA\nadd_core = FRA\nculture = swabian\nreligion = catholic\ntrade_goods = cloth\nbase_tax = 3\n",
        );
        // 11 is occupied by ENG (controller differs) and cored by both.
        w(
            "history/provinces/11 - Bbb.txt",
            b"owner = FRA\ncontroller = ENG\nadd_core = FRA\nadd_core = ENG\nculture = swabian\nreligion = catholic\ntrade_goods = grain\n",
        );
        // 12 is uncolonized but still carries a stale FRA core + a tribal owner.
        w(
            "history/provinces/12 - Ccc.txt",
            b"add_core = FRA\ntribal_owner = FRA\nculture = swabian\nreligion = catholic\n",
        );
        w(
            "common/country_tags/00_countries.txt",
            b"REB = \"countries/Rebels.txt\"\nFRA = \"countries/France.txt\"\nENG = \"countries/England.txt\"\nSCO = \"countries/Scotland.txt\"\n",
        );
        w("common/countries/France.txt", b"color = { 20 20 200 }\ngraphical_culture = westerngfx\n");
        w("common/countries/England.txt", b"color = { 200 20 20 }\n");
        w("common/countries/Scotland.txt", b"color = { 20 120 40 }\n");
        w(
            "history/countries/FRA - France.txt",
            b"government = monarchy\ntechnology_group = western\nreligion = catholic\nprimary_culture = swabian\ncapital = 10\n",
        );
        w(
            "history/diplomacy/west.txt",
            b"alliance = { first = FRA second = SCO start_date = 1428.1.1 end_date = 1560.1.1 }\nvassal = { first = FRA second = ENG start_date = 1444.1.1 end_date = 9999.1.1 }\n",
        );
        w(
            "history/wars/W.txt",
            b"name = \"Test War\"\n1444.1.1 = { add_attacker = FRA add_defender = ENG }\n",
        );

        let vfs = Vfs::new(root.to_str().unwrap(), None).unwrap();
        (root, vfs)
    }

    #[test]
    fn blast_radius_counts_correct() {
        let (_root, vfs) = synthetic("blast");
        let b = blast_radius(&vfs, "FRA", DEFAULT_START);
        // Owns 10 and 11 (top-level owner = FRA); 12 is uncolonized.
        assert_eq!(b.owned_provinces, vec![10, 11]);
        // Base tag → not toolkit-created; tag file is the base file.
        assert!(!b.is_toolkit_created);
        assert_eq!(b.tag_file.as_deref(), Some("common/country_tags/00_countries.txt"));
        assert!(b.toolkit_files.is_empty(), "base tag deletes no files");
        // Relations: FRA-SCO alliance + FRA-ENG vassal (FRA is overlord).
        assert_eq!(b.relations.len(), 2);
        assert!(b.relations.iter().any(|r| r.relation_type == "alliance" && r.partner.as_deref() == Some("SCO")));
        let vassal = b.relations.iter().find(|r| r.relation_type == "dependency").unwrap();
        assert_eq!(vassal.role, "overlord");
        assert_eq!(vassal.partner.as_deref(), Some("ENG"));
        // War: active at 1444.11.11 (both sides joined 1444.1.1, never left).
        assert_eq!(b.active_wars.len(), 1);
        assert_eq!(b.active_wars[0].name.as_deref(), Some("Test War"));
        assert_eq!(b.active_wars[0].side.as_deref(), Some("attacker"));
        assert_eq!(b.active_wars[0].enemy.as_deref(), Some("ENG"));
        // Stale FRA core on 12 (not owned by FRA) + tribal owner on 12.
        assert_eq!(b.core_references, vec![12]);
        assert_eq!(b.tribal_owner_references, vec![12]);
    }

    #[test]
    fn delete_base_country_writes_only_expected_files_and_never_touches_base() {
        let (root, vfs) = synthetic("base_delete");
        let project = root.join("project");
        let edits = build_deletion(&vfs, "FRA", DEFAULT_START, None).unwrap();

        // A base delete must carry NO file deletions and NO loc removals.
        assert!(
            !edits
                .iter()
                .any(|e| matches!(e, TypedEdit::DeleteFile { .. } | TypedEdit::LocRemove { .. })),
            "base country delete must not delete files or loc"
        );

        // Snapshot the base install to prove it is never written.
        let snapshot = |rel: &str| std::fs::read(root.join(rel)).unwrap();
        let base_tags = snapshot("common/country_tags/00_countries.txt");
        let base_prov10 = snapshot("history/provinces/10 - Aaa.txt");
        let base_diplo = snapshot("history/diplomacy/west.txt");

        let written = apply_queue(&vfs, &project, &edits).unwrap();

        // Province 10 → uncolonized: owner/controller/FRA core gone; rest intact.
        let p10 = std::fs::read_to_string(project.join("history/provinces/10 - Aaa.txt")).unwrap();
        assert!(!p10.contains("owner = FRA"));
        assert!(!p10.contains("controller = FRA"));
        assert!(!p10.contains("add_core = FRA"));
        assert!(p10.contains("culture = swabian"));
        assert!(p10.contains("religion = catholic"));
        assert!(p10.contains("trade_goods = cloth"));
        // Province 11 → uncolonized: FRA core gone, ENG core survives; controller
        // (ENG occupier) removed too.
        let p11 = std::fs::read_to_string(project.join("history/provinces/11 - Bbb.txt")).unwrap();
        assert!(!p11.contains("owner = FRA"));
        assert!(!p11.contains("add_core = FRA"));
        assert!(p11.contains("add_core = ENG"), "other owner's core untouched");

        // Tag registration line removed; ENG/SCO survive.
        let tags = std::fs::read_to_string(project.join(TAG_FILE_BASE)).unwrap();
        assert!(!tags.contains("FRA ="), "FRA registration removed: {tags}");
        assert!(tags.contains("ENG ="));
        assert!(tags.contains("SCO ="));

        // Both diplomacy relations referencing FRA removed.
        let diplo = std::fs::read_to_string(project.join("history/diplomacy/west.txt")).unwrap();
        assert!(!diplo.contains("FRA"), "no FRA relation remains: {diplo}");

        // The base install is byte-for-byte unchanged.
        assert_eq!(snapshot("common/country_tags/00_countries.txt"), base_tags);
        assert_eq!(snapshot("history/provinces/10 - Aaa.txt"), base_prov10);
        assert_eq!(snapshot("history/diplomacy/west.txt"), base_diplo);

        // Sanity: the war file was never scheduled for deletion.
        assert!(!written.iter().any(|w| w.contains("history/wars")));
    }

    const TAG_FILE_BASE: &str = "common/country_tags/00_countries.txt";

    #[test]
    fn delete_base_country_transfer_moves_provinces_to_target() {
        let (root, vfs) = synthetic("base_transfer");
        let project = root.join("project");
        let edits = build_deletion(&vfs, "FRA", DEFAULT_START, Some("SCO")).unwrap();
        apply_queue(&vfs, &project, &edits).unwrap();

        // Province 10 owner/controller → SCO; FRA core dropped; NO SCO core added.
        let p10 = std::fs::read_to_string(project.join("history/provinces/10 - Aaa.txt")).unwrap();
        assert!(p10.contains("owner = SCO"));
        assert!(p10.contains("controller = SCO"));
        assert!(!p10.contains("add_core = FRA"));
        assert!(!p10.contains("add_core = SCO"), "transfer adds no target core");
        assert!(p10.contains("culture = swabian"));
    }

    #[test]
    fn delete_toolkit_country_round_trips_to_no_trace() {
        // Scaffold a NEW country onto an uncolonized capital, SAVE it, then delete
        // it. Nothing referencing the tag may remain in the project, and the base
        // install is never touched.
        let root = std::env::temp_dir().join("eu_toolkit_cdel_test_roundtrip");
        let _ = std::fs::remove_dir_all(&root);
        let base = root.join("base");
        let project = root.join("project");
        let w = |rel: &str, bytes: &[u8]| {
            let p = base.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, bytes).unwrap();
        };
        w("map/provinces.bmp", b"x");
        w(
            "map/definition.csv",
            b"province;red;green;blue;name;x\n10;10;20;30;Testburg;x\n99;0;0;255;Ocean;x\n",
        );
        w("map/default.map", b"sea_starts = { 99 }\nlakes = { }\n");
        w("map/climate.txt", b"tropical = { 10 }\nimpassable = { }\n");
        w(
            "history/provinces/10 - Testburg.txt",
            b"culture = swabian\nreligion = catholic\ntrade_goods = cloth\nbase_tax = 3\n",
        );
        w("common/country_tags/00_countries.txt", b"FRA = \"countries/France.txt\"\n");
        w("common/countries/France.txt", b"color = { 20 20 200 }\n");
        w(
            "common/cultures/00_cultures.txt",
            b"germanic = {\n\tgraphical_culture = westerngfx\n\tswabian = { male_names = { Fritz } dynasty_names = { Habsburg } }\n}\n",
        );

        let base_vfs = Vfs::new(base.to_str().unwrap(), None).unwrap();
        let scaffold = build_scaffold(&base_vfs, 10, "Newland", "Newlandish", &HashSet::new()).unwrap();
        let tag = scaffold.tag.clone();
        // Save the scaffold into the project (scaffold edits are wire-identical to
        // TypedEdit — same JSON round-trip the country_create tests use).
        let scaffold_typed: Vec<TypedEdit> =
            serde_json::from_str(&serde_json::to_string(&scaffold.edits).unwrap()).unwrap();
        apply_queue(&base_vfs, &project, &scaffold_typed).unwrap();

        // Reopen with the project as a mod so the delete resolves its own copies.
        let vfs = Vfs::new(base.to_str().unwrap(), Some(project.to_str().unwrap())).unwrap();
        let del = build_deletion(&vfs, &tag, DEFAULT_START, None).unwrap();
        // Toolkit tag → deletes country/history/flag files + loc overrides.
        assert!(del.iter().any(|e| matches!(e, TypedEdit::DeleteFile { .. })));
        assert!(del.iter().any(|e| matches!(e, TypedEdit::LocRemove { .. })));
        apply_queue(&vfs, &project, &del).unwrap();

        // No trace of the tag anywhere in the project.
        assert!(!project.join(&scaffold.country_file).exists(), "country file gone");
        assert!(!project.join(&scaffold.history_file).exists(), "history file gone");
        assert!(!project.join(&scaffold.flag_file).exists(), "flag gone");
        let tags = std::fs::read_to_string(project.join(TAG_FILE)).unwrap();
        assert!(!tags.contains(&tag), "tag registration gone: {tags}");
        let loc = std::fs::read_to_string(project.join(crate::loc::OVERRIDE_REL)).unwrap();
        assert!(!loc.contains(&format!("{tag}:")), "loc overrides gone: {loc}");
        // Capital reverted to uncolonized (no owner referencing the tag).
        let prov = std::fs::read_to_string(project.join("history/provinces/10 - Testburg.txt")).unwrap();
        assert!(!prov.contains(&format!("owner = {tag}")), "capital uncolonized: {prov}");
        assert!(prov.contains("culture = swabian"), "province data preserved");

        // Base install is untouched throughout.
        assert!(!base.join("common/countries").join(format!("{tag}.txt")).exists());
        let base_tags = std::fs::read_to_string(base.join("common/country_tags/00_countries.txt")).unwrap();
        assert!(!base_tags.contains(&tag));
    }

    #[test]
    fn anbennar_blast_radius_smoke() {
        if !Path::new(INSTALL).join("map/provinces.bmp").is_file() || !Path::new(ANBENNAR).is_dir() {
            return; // game or Anbennar absent: no-op
        }
        let vfs = Vfs::new(INSTALL, Some(ANBENNAR)).unwrap();
        // A01 is a real Anbennar custom tag (see country_create tests).
        let b = blast_radius(&vfs, "A01", DEFAULT_START);
        assert_eq!(b.tag, "A01");
        // It must resolve a tag registration file and a history file.
        assert!(b.tag_file.is_some(), "A01 should be registered");
        // Building the composite must not panic and yields some edits (A01 owns
        // land + a tag registration removal at minimum).
        let edits = build_deletion(&vfs, "A01", DEFAULT_START, None).unwrap();
        assert!(!edits.is_empty());
    }
}
