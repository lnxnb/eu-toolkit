//! Sprint 10 — areas & regions geography network: the area→region→superregion
//! hierarchy parsed into one payload for the Areas and Regions map modes, plus
//! the scaffold text helpers for creating a new area/region. The frontend
//! (thin selection panels + membership brush) rides on top of the data + edit
//! recipes documented here.
//!
//! # Ground-truth file formats
//!
//! * `map/area.txt` — an area is a named top-level block whose bare ids are its
//!   provinces. A leading `color = { r g b }` block is optional (skipped by
//!   `bare_ids`):
//!   ```text
//!   mecklenburg_area = {
//!       1758 1759
//!   }
//!   american_east_area = {
//!       color = { 12 158 66 }
//!       978 979 980
//!   }
//!   ```
//! * `map/region.txt` — a region is a named block with an `areas = { <names> }`
//!   list, optionally a `monsoon = { ... }` date block (preserved untouched,
//!   read-only), and possibly other unmodeled keys:
//!   ```text
//!   scandinavia_region = {
//!       areas = { skane_area ... }
//!       monsoon = { ... }
//!   }
//!   ```
//! * `map/superregion.txt` — a superregion is a named block whose bare scalars
//!   are region names: `europe_superregion = { scandinavia_region ... }`.
//!
//! All three are single canonical map files (EU4 does not split them); the Vfs
//! resolves the mod's shadow when present, and copy-on-write edits target the
//! fixed game-relative paths below.
//!
//! # Membership granularity + edit recipes (frontend emits these `TypedEdit`s)
//!
//! * **Areas mode** edits at PROVINCE granularity. A province belongs to exactly
//!   one area (its bare id in that area's block). list_path is `[<area>]`.
//!   * add:   `AddId    { file: AREA_FILE, list_path: [area], id: prov }`
//!   * remove:`RemoveId { file: AREA_FILE, list_path: [area], id: prov }`
//!   * steal: `ListMove { from: AREA_FILE/[old], to: AREA_FILE/[new], id: prov }`
//! * **Regions mode** edits at AREA granularity. An area belongs to one region
//!   (its name in that region's `areas` list). list_path is `[<region>,"areas"]`.
//!   * steal an area: `ListMove` with id = area name, both paths `[region,"areas"]`.
//! * **Superregion membership** (a region's parent) edits at REGION granularity:
//!   the region name is a bare scalar in the superregion block. list_path is
//!   `[<superregion>]`; move a region between superregions with `ListMove`.
//! * **Create area**: `AppendText { file: AREA_FILE, text: scaffold_area(...) }`
//!   (or `CreateFile` when the project has no area.txt) + a `LocOverride`.
//! * **Create region**: `AppendText { file: REGION_FILE, text: scaffold_region }`
//!   + `LocOverride` (+ optional `AddId` into a superregion).
//! * **Delete area**: `RemoveStatement { file: AREA_FILE, block_path: [], key:
//!   area }` (whole block; provinces become area-less) preceded by a `RemoveId`
//!   out of its region's `areas` list if it has one.
//! * **Delete region**: `RemoveStatement { file: REGION_FILE, block_path: [],
//!   key: region }` preceded by a `RemoveId` out of its superregion if any.
//!
//! No new mod_writer edit kinds are needed — bare-token `AddId`/`RemoveId`/
//! `ListMove` already splice these string lists (names are bare scalars just
//! like numeric ids). The only backend additions are the two scaffold helpers
//! (single source of truth for formatting) and the network payload below.

use std::collections::HashMap;

use crate::loc::LocStore;
use crate::map_renderer::hash_color;
use crate::paradox::{self, Block, Value};
use crate::vfs::Vfs;

pub const AREA_FILE: &str = "map/area.txt";
pub const REGION_FILE: &str = "map/region.txt";
pub const SUPERREGION_FILE: &str = "map/superregion.txt";
/// `map/continent.txt` — same bare-id-list shape as `map/area.txt` (S3.1). Its
/// blocks are the continents, plus the engine's `island_check_provinces` helper
/// list and the (usually empty) `new_world` RNW continent.
pub const CONTINENT_FILE: &str = "map/continent.txt";

/// Modeled top-level keys inside a region block; anything else is `raw_extra`.
const REGION_KNOWN_KEYS: &[&str] = &["areas", "monsoon"];

// ---------------------------------------------------------------------------
// Payload types (serialize snake_case; see module header for the JSON contract).
// ---------------------------------------------------------------------------

#[derive(serde::Serialize, Clone, Debug)]
pub struct GeoNetwork {
    pub areas: Vec<GeoArea>,
    pub regions: Vec<GeoRegion>,
    pub superregions: Vec<GeoSuperregion>,
    /// Fixed game-relative files the three membership levels live in.
    pub area_file: String,
    pub region_file: String,
    pub superregion_file: String,
}

#[derive(serde::Serialize, Clone, Debug)]
pub struct GeoArea {
    pub key: String,
    pub name: String,
    /// Explicit `color = { r g b }` in area.txt, if any (else `None`).
    pub color: Option<[u8; 3]>,
    /// Toolkit hash color — what the map render/highlight actually uses.
    pub hash_color: [u8; 3],
    pub provinces: Vec<u32>,
    /// Parent region key (rollup), or `None` if the area is in no region.
    pub region: Option<String>,
    pub source_file: String,
}

#[derive(serde::Serialize, Clone, Debug)]
pub struct GeoRegion {
    pub key: String,
    pub name: String,
    pub hash_color: [u8; 3],
    /// Member area keys, in file order.
    pub areas: Vec<String>,
    /// Parent superregion key, or `None`.
    pub superregion: Option<String>,
    /// A `monsoon = { ... }` block is present (== `!monsoon.is_empty()`; kept for
    /// the panel badge / existing callers).
    pub has_monsoon: bool,
    /// The region's `monsoon = { start end }` blocks in file order (S2.6 editable).
    /// A region may carry several — occurrence order is the edit-address order.
    pub monsoon: Vec<MonsoonRange>,
    /// Unmodeled top-level keys (preserve-unknown; shown read-only).
    pub raw_extra: Vec<String>,
    pub source_file: String,
}

/// One `monsoon = { <start> <end> }` block. Both dates are stored verbatim as the
/// game writes them — `YY.MM.DD` with a `00` year (the season is year-agnostic).
#[derive(serde::Serialize, Clone, Debug)]
pub struct MonsoonRange {
    /// First bare date in the block, e.g. `"00.06.01"`.
    pub start: String,
    /// Second bare date in the block, e.g. `"00.09.30"`.
    pub end: String,
}

#[derive(serde::Serialize, Clone, Debug)]
pub struct GeoSuperregion {
    pub key: String,
    pub name: String,
    /// Member region keys, in file order.
    pub regions: Vec<String>,
    pub source_file: String,
}

// ---------------------------------------------------------------------------
// Parse.
// ---------------------------------------------------------------------------

fn parse_rel(vfs: &Vfs, rel: &str) -> Option<Block> {
    vfs.read(rel)
        .ok()
        .map(|b| paradox::parse(&String::from_utf8_lossy(&b)))
}

/// Builds the full [`GeoNetwork`] (areas + regions + superregions + rollups).
pub fn load_network(vfs: &Vfs, loc: &LocStore) -> GeoNetwork {
    // --- superregion -> its regions, and each region's parent superregion ---
    let super_block = parse_rel(vfs, SUPERREGION_FILE).unwrap_or_default();
    let mut superregions: Vec<GeoSuperregion> = Vec::new();
    let mut region_parent: HashMap<String, String> = HashMap::new();
    for (name, b) in super_block.key_blocks() {
        let regions: Vec<String> = b.bare_scalars().map(str::to_string).collect();
        for r in &regions {
            region_parent
                .entry(r.clone())
                .or_insert_with(|| name.to_string());
        }
        superregions.push(GeoSuperregion {
            key: name.to_string(),
            name: loc.resolve(name),
            regions,
            source_file: SUPERREGION_FILE.to_string(),
        });
    }

    // --- region -> its areas, and each area's parent region ---
    let region_block = parse_rel(vfs, REGION_FILE).unwrap_or_default();
    let mut regions: Vec<GeoRegion> = Vec::new();
    let mut area_parent: HashMap<String, String> = HashMap::new();
    for (name, b) in region_block.key_blocks() {
        let areas: Vec<String> = b
            .get_block("areas")
            .map(|a| a.bare_scalars().map(str::to_string).collect())
            .unwrap_or_default();
        for a in &areas {
            area_parent
                .entry(a.clone())
                .or_insert_with(|| name.to_string());
        }
        // Every `monsoon = { start end }` block, in file order. A region may have
        // more than one (e.g. a two-season monsoon); occurrence index == edit key.
        let monsoon: Vec<MonsoonRange> = b
            .items
            .iter()
            .filter_map(|(k, v)| match (k.as_deref(), v) {
                (Some("monsoon"), Value::Block(mb)) => {
                    let mut dates = mb.bare_scalars();
                    let start = dates.next().unwrap_or("").to_string();
                    let end = dates.next().unwrap_or("").to_string();
                    Some(MonsoonRange { start, end })
                }
                _ => None,
            })
            .collect();
        let has_monsoon = !monsoon.is_empty();
        let raw_extra: Vec<String> = b
            .items
            .iter()
            .filter_map(|(k, _)| k.as_deref())
            .filter(|k| !REGION_KNOWN_KEYS.contains(k))
            .map(str::to_string)
            .collect::<Vec<_>>();
        // De-dup raw_extra while preserving order.
        let mut seen = std::collections::HashSet::new();
        let raw_extra = raw_extra
            .into_iter()
            .filter(|k| seen.insert(k.clone()))
            .collect();
        regions.push(GeoRegion {
            key: name.to_string(),
            name: loc.resolve(name),
            hash_color: hash_color(name),
            areas,
            superregion: region_parent.get(name).cloned(),
            has_monsoon,
            monsoon,
            raw_extra,
            source_file: REGION_FILE.to_string(),
        });
    }

    // --- areas + provinces + explicit color ---
    let area_block = parse_rel(vfs, AREA_FILE).unwrap_or_default();
    let mut areas: Vec<GeoArea> = Vec::new();
    for (name, b) in area_block.key_blocks() {
        let color = b.get_block("color").and_then(paradox::color_from_block);
        areas.push(GeoArea {
            key: name.to_string(),
            name: loc.resolve(name),
            color,
            hash_color: hash_color(name),
            provinces: b.bare_ids(),
            region: area_parent.get(name).cloned(),
            source_file: AREA_FILE.to_string(),
        });
    }

    GeoNetwork {
        areas,
        regions,
        superregions,
        area_file: AREA_FILE.to_string(),
        region_file: REGION_FILE.to_string(),
        superregion_file: SUPERREGION_FILE.to_string(),
    }
}

/// The whole area/region/superregion tree in one payload. Registered in lib.rs.
#[allow(dead_code)]
#[tauri::command(async)]
pub fn get_geo_network(
    install_path: String,
    mod_path: Option<String>,
) -> Result<GeoNetwork, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    let loc = crate::loc::store(&vfs, &install_path, mod_path.as_deref());
    Ok(load_network(&vfs, &loc))
}

// ---------------------------------------------------------------------------
// Scaffolds (single source of truth for area/region text; unit-tested).
// ---------------------------------------------------------------------------

/// A brand-new area block with its starting province(s). Authored at column 0
/// (append at top level, or `InsertStatement` re-indents). Matches vanilla tab
/// style and parses back as an area.
pub fn scaffold_area(key: &str, provinces: &[u32]) -> String {
    let ids = provinces
        .iter()
        .map(|id| id.to_string())
        .collect::<Vec<_>>()
        .join(" ");
    format!("{key} = {{\n\t{ids}\n}}")
}

/// A brand-new region block with its first member area. Authored at column 0.
pub fn scaffold_region(key: &str, first_area: &str) -> String {
    format!("{key} = {{\n\tareas = {{\n\t\t{first_area}\n\t}}\n}}")
}

/// A brand-new superregion block with its first member region (S3.1). A
/// superregion's members are bare region-name scalars, so this mirrors
/// [`scaffold_area`] with a name instead of ids. Authored at column 0.
pub fn scaffold_superregion(key: &str, first_region: &str) -> String {
    format!("{key} = {{\n\t{first_region}\n}}")
}

/// A brand-new continent block (S3.1). `map/continent.txt` is the same bare-id
/// list as `map/area.txt`, so this reuses [`scaffold_area`]; passing an empty
/// `provinces` slice yields an empty block that a following `AddId` fills (the
/// list-creation-when-absent pattern used by the province-panel create flow).
pub fn scaffold_continent(key: &str, provinces: &[u32]) -> String {
    scaffold_area(key, provinces)
}

/// Command wrapper around [`scaffold_area`].
#[allow(dead_code)]
#[tauri::command(async)]
pub fn scaffold_area_block(key: String, provinces: Vec<u32>) -> String {
    scaffold_area(&key, &provinces)
}

/// Command wrapper around [`scaffold_region`].
#[allow(dead_code)]
#[tauri::command(async)]
pub fn scaffold_region_block(key: String, first_area: String) -> String {
    scaffold_region(&key, &first_area)
}

/// Command wrapper around [`scaffold_superregion`] (S3.1).
#[allow(dead_code)]
#[tauri::command(async)]
pub fn scaffold_superregion_block(key: String, first_region: String) -> String {
    scaffold_superregion(&key, &first_region)
}

/// Command wrapper around [`scaffold_continent`] (S3.1).
#[allow(dead_code)]
#[tauri::command(async)]
pub fn scaffold_continent_block(key: String, provinces: Vec<u32>) -> String {
    scaffold_continent(&key, &provinces)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mod_writer::{apply, Edit};
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

    /// A synthetic install with the three map files; returns (root, Vfs). One
    /// dir per test — parallel tests must not share a temp dir.
    fn synthetic(name: &str, files: &[(&str, &str)]) -> (std::path::PathBuf, Vfs) {
        let root = std::env::temp_dir().join(format!("eu_toolkit_geo_test_{name}"));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("map")).unwrap();
        std::fs::write(root.join("map/provinces.bmp"), b"x").unwrap();
        for (rel, contents) in files {
            let path = root.join(rel);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(path, contents).unwrap();
        }
        let vfs = Vfs::new(root.to_str().unwrap(), None).unwrap();
        (root, vfs)
    }

    const AREA_SRC: &str = "a_area = {\n\tcolor = { 10 20 30 }\n\t1 2 3\n}\nb_area = {\n\t4 5\n}\nlone_area = {\n\t6\n}\n";
    const REGION_SRC: &str = "x_region = {\n\tareas = {\n\t\ta_area\n\t\tb_area\n\t}\n\tmonsoon = {\n\t\t2888 2904\n\t}\n}\ny_region = {\n\tareas = {\n\t\tlone_area\n\t}\n}\n";
    const SUPER_SRC: &str = "big_super = {\n\tx_region\n}\n";

    fn tree() -> (std::path::PathBuf, Vfs) {
        synthetic(
            "tree",
            &[
                ("map/area.txt", AREA_SRC),
                ("map/region.txt", REGION_SRC),
                ("map/superregion.txt", SUPER_SRC),
            ],
        )
    }

    #[test]
    fn parses_hierarchy_with_rollups() {
        let (_root, vfs) = tree();
        let loc = crate::loc::build(&vfs);
        let net = load_network(&vfs, &loc);

        assert_eq!(net.areas.len(), 3);
        let a = net.areas.iter().find(|a| a.key == "a_area").unwrap();
        assert_eq!(a.provinces, vec![1, 2, 3]);
        assert_eq!(a.color, Some([10, 20, 30]));
        assert_eq!(a.region.as_deref(), Some("x_region"));
        let b = net.areas.iter().find(|a| a.key == "b_area").unwrap();
        assert_eq!(b.color, None);
        assert_eq!(b.region.as_deref(), Some("x_region"));
        let lone = net.areas.iter().find(|a| a.key == "lone_area").unwrap();
        assert_eq!(lone.region.as_deref(), Some("y_region"));

        let x = net.regions.iter().find(|r| r.key == "x_region").unwrap();
        assert_eq!(x.areas, vec!["a_area", "b_area"]);
        assert_eq!(x.superregion.as_deref(), Some("big_super"));
        assert!(x.has_monsoon);
        let y = net.regions.iter().find(|r| r.key == "y_region").unwrap();
        assert!(!y.has_monsoon);
        assert_eq!(y.superregion, None); // y_region in no superregion

        assert_eq!(net.superregions.len(), 1);
        assert_eq!(net.superregions[0].regions, vec!["x_region"]);
    }

    // --- membership move round-trips (byte-surgical) ---------------------

    #[test]
    fn province_moves_between_areas_only_touched_lists_change() {
        // Move province 4 from b_area to a_area. The color block, lone_area,
        // and the region/superregion files must be byte-identical.
        let out = apply(
            AREA_SRC.as_bytes(),
            &Edit::RemoveId {
                list_path: vec!["b_area".into()],
                id: "4".into(),
            },
        )
        .unwrap();
        let out = apply(
            &out,
            &Edit::AddId {
                list_path: vec!["a_area".into()],
                id: "4".into(),
            },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("1 2 3\n\t4"), "4 added to a_area");
        assert!(text.contains("b_area = {\n\t5\n}"), "4 removed from b_area");
        assert!(text.contains("color = { 10 20 30 }"), "color block intact");
        assert!(text.contains("lone_area = {\n\t6\n}"), "lone_area intact");
    }

    #[test]
    fn area_moves_between_regions_monsoon_intact() {
        // Move b_area from x_region to y_region. x_region's monsoon block must
        // survive byte-for-byte.
        let out = apply(
            REGION_SRC.as_bytes(),
            &Edit::RemoveId {
                list_path: vec!["x_region".into(), "areas".into()],
                id: "b_area".into(),
            },
        )
        .unwrap();
        let out = apply(
            &out,
            &Edit::AddId {
                list_path: vec!["y_region".into(), "areas".into()],
                id: "b_area".into(),
            },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("monsoon = {\n\t\t2888 2904\n\t}"), "monsoon intact");
        assert!(
            text.contains("lone_area\n\t\tb_area") || text.contains("lone_area\n\t\t b_area"),
            "b_area added to y_region: {text}"
        );
        // x_region's areas list now only has a_area.
        let x = text.split("x_region").nth(1).unwrap();
        let x_areas = x.split("areas = {").nth(1).unwrap().split('}').next().unwrap();
        assert!(x_areas.contains("a_area"));
        assert!(!x_areas.contains("b_area"));
    }

    #[test]
    fn region_moves_between_superregions() {
        let src = "s1 = {\n\tr_a\n\tr_b\n}\ns2 = {\n\tr_c\n}\n";
        let out = apply(
            src.as_bytes(),
            &Edit::RemoveId {
                list_path: vec!["s1".into()],
                id: "r_b".into(),
            },
        )
        .unwrap();
        let out = apply(
            &out,
            &Edit::AddId {
                list_path: vec!["s2".into()],
                id: "r_b".into(),
            },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("s1 = {\n\tr_a\n}"));
        assert!(text.contains("r_c\n\tr_b"));
    }

    // --- create / delete round-trips -------------------------------------

    #[test]
    fn create_area_then_delete_is_identity() {
        let scaffold = scaffold_area("new_area", &[42, 43]);
        let appended = apply(
            AREA_SRC.as_bytes(),
            &Edit::Append {
                text: scaffold.clone(),
            },
        )
        .unwrap();
        let text = String::from_utf8(appended.clone()).unwrap();
        assert!(text.contains("new_area = {\n\t42 43\n}"));
        // Parses back as an area.
        let block = paradox::parse(&text);
        assert_eq!(
            block.get_block("new_area").unwrap().bare_ids(),
            vec![42, 43]
        );
        // Delete the top-level block.
        let deleted = apply(
            &appended,
            &Edit::RemoveStatement {
                block_path: vec![],
                key: "new_area".into(),
                value: None,
            },
        )
        .unwrap();
        assert_eq!(deleted, AREA_SRC.as_bytes(), "create then delete is identity");
    }

    #[test]
    fn create_region_then_delete_is_identity() {
        let scaffold = scaffold_region("new_region", "lone_area");
        let appended = apply(
            REGION_SRC.as_bytes(),
            &Edit::Append {
                text: scaffold,
            },
        )
        .unwrap();
        let text = String::from_utf8(appended.clone()).unwrap();
        let block = paradox::parse(&text);
        let nr = block.get_block("new_region").unwrap();
        assert_eq!(
            nr.get_block("areas").unwrap().bare_scalars().collect::<Vec<_>>(),
            vec!["lone_area"]
        );
        let deleted = apply(
            &appended,
            &Edit::RemoveStatement {
                block_path: vec![],
                key: "new_region".into(),
                value: None,
            },
        )
        .unwrap();
        assert_eq!(deleted, REGION_SRC.as_bytes(), "create then delete is identity");
    }

    // --- superregion create / delete round-trips (S3.1) ------------------

    #[test]
    fn create_superregion_then_delete_is_identity() {
        // Append a new superregion with x_region as its first member, then delete
        // the whole block. The original superregion.txt bytes must be restored.
        let scaffold = scaffold_superregion("new_super", "x_region");
        assert_eq!(scaffold, "new_super = {\n\tx_region\n}");
        let appended = apply(
            SUPER_SRC.as_bytes(),
            &Edit::Append { text: scaffold },
        )
        .unwrap();
        let text = String::from_utf8(appended.clone()).unwrap();
        // Parses back as a superregion holding x_region.
        let block = paradox::parse(&text);
        assert_eq!(
            block
                .get_block("new_super")
                .unwrap()
                .bare_scalars()
                .collect::<Vec<_>>(),
            vec!["x_region"]
        );
        // The pre-existing big_super block is untouched.
        assert!(text.contains("big_super = {\n\tx_region\n}"));
        let deleted = apply(
            &appended,
            &Edit::RemoveStatement {
                block_path: vec![],
                key: "new_super".into(),
                value: None,
            },
        )
        .unwrap();
        assert_eq!(deleted, SUPER_SRC.as_bytes(), "create then delete is identity");
    }

    #[test]
    fn delete_superregion_leaves_regions_unassigned() {
        // Deleting a superregion block removes it; the region rollup then reports
        // its former members as having no superregion. Own temp dir (not tree()'s)
        // so it can't race the shared fixture in parallel runs.
        let (_root, vfs) = synthetic(
            "delete_super",
            &[
                ("map/area.txt", AREA_SRC),
                ("map/region.txt", REGION_SRC),
                ("map/superregion.txt", SUPER_SRC),
            ],
        );
        let out = apply(
            SUPER_SRC.as_bytes(),
            &Edit::RemoveStatement {
                block_path: vec![],
                key: "big_super".into(),
                value: None,
            },
        )
        .unwrap();
        assert_eq!(String::from_utf8(out).unwrap(), "");
        // Sanity: before deletion x_region rolled up to big_super.
        let loc = crate::loc::build(&vfs);
        let net = load_network(&vfs, &loc);
        assert_eq!(
            net.regions
                .iter()
                .find(|r| r.key == "x_region")
                .unwrap()
                .superregion
                .as_deref(),
            Some("big_super")
        );
    }

    // --- continent create / delete / move round-trips (S3.1) -------------

    // continent.txt is the same bare-id list as area.txt, with comments and a
    // trailing helper block that must round-trip byte-for-byte.
    const CONT_SRC: &str = "europe = {\n\t# Scandinavia\n\t1 2 3\n}\nasia = {\n\t100 101\n}\nisland_check_provinces = {\n\t1 # helper\n}\nnew_world = {\n}\n";

    #[test]
    fn create_continent_empty_then_add_id_then_delete_is_identity() {
        // The province-panel flow: append an empty continent block, then AddId the
        // starting province into it (list-creation-when-absent). Then delete.
        let scaffold = scaffold_continent("new_continent", &[]);
        assert_eq!(scaffold, "new_continent = {\n\t\n}");
        let appended = apply(CONT_SRC.as_bytes(), &Edit::Append { text: scaffold }).unwrap();
        let with_id = apply(
            &appended,
            &Edit::AddId {
                list_path: vec!["new_continent".into()],
                id: "500".into(),
            },
        )
        .unwrap();
        let text = String::from_utf8(with_id.clone()).unwrap();
        let block = paradox::parse(&text);
        assert_eq!(
            block.get_block("new_continent").unwrap().bare_ids(),
            vec![500]
        );
        // Existing continents + the comment + helper block untouched.
        assert!(text.contains("europe = {\n\t# Scandinavia\n\t1 2 3\n}"));
        assert!(text.contains("island_check_provinces = {\n\t1 # helper\n}"));
        assert!(text.contains("new_world = {\n}"));
        // Remove the id, then the block — restores the original bytes.
        let no_id = apply(
            &with_id,
            &Edit::RemoveId {
                list_path: vec!["new_continent".into()],
                id: "500".into(),
            },
        )
        .unwrap();
        let deleted = apply(
            &no_id,
            &Edit::RemoveStatement {
                block_path: vec![],
                key: "new_continent".into(),
                value: None,
            },
        )
        .unwrap();
        assert_eq!(deleted, CONT_SRC.as_bytes(), "create then delete is identity");
    }

    #[test]
    fn move_province_between_continents_only_touched_lists_change() {
        // Steal province 100 from asia into europe. The comment, the helper block,
        // and new_world must all round-trip byte-for-byte.
        let out = apply(
            CONT_SRC.as_bytes(),
            &Edit::RemoveId {
                list_path: vec!["asia".into()],
                id: "100".into(),
            },
        )
        .unwrap();
        let out = apply(
            &out,
            &Edit::AddId {
                list_path: vec!["europe".into()],
                id: "100".into(),
            },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("1 2 3\n\t100"), "100 joined europe: {text}");
        assert!(text.contains("asia = {\n\t101\n}"), "100 left asia: {text}");
        assert!(text.contains("# Scandinavia"), "comment preserved");
        assert!(text.contains("island_check_provinces = {\n\t1 # helper\n}"));
        assert!(text.contains("new_world = {\n}"));
    }

    // --- monsoon parse + edit/add/remove round-trips (S2.6) --------------

    // A region carrying TWO monsoon blocks (two-season monsoon), like vanilla's
    // east/south africa. Occurrence addressing must reach the second one.
    const MONSOON_SRC: &str = "m_region = {\n\tareas = {\n\t\tm_area\n\t}\n\tmonsoon = {\n\t\t00.06.01\n\t\t00.09.30\n\t}\n\tmonsoon = {\n\t\t00.11.01\n\t\t00.12.30\n\t}\n}\n";

    #[test]
    fn parses_multiple_monsoon_ranges_in_order() {
        let (_root, vfs) = synthetic("monsoon_parse", &[("map/region.txt", MONSOON_SRC)]);
        let loc = crate::loc::build(&vfs);
        let net = load_network(&vfs, &loc);
        let m = net.regions.iter().find(|r| r.key == "m_region").unwrap();
        assert!(m.has_monsoon);
        assert_eq!(m.monsoon.len(), 2);
        assert_eq!(m.monsoon[0].start, "00.06.01");
        assert_eq!(m.monsoon[0].end, "00.09.30");
        assert_eq!(m.monsoon[1].start, "00.11.01");
        assert_eq!(m.monsoon[1].end, "00.12.30");
    }

    #[test]
    fn edit_second_monsoon_leaves_first_byte_identical() {
        // SetBlock on `monsoon#1` rewrites only the second block's `{ … }` span.
        let out = apply(
            MONSOON_SRC.as_bytes(),
            &Edit::SetBlock {
                path: vec!["m_region".into(), "monsoon#1".into()],
                value: "00.10.01 00.12.15".into(),
            },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        // First monsoon untouched (multi-line form preserved verbatim).
        assert!(text.contains("monsoon = {\n\t\t00.06.01\n\t\t00.09.30\n\t}"), "first intact: {text}");
        // Second rewritten to the canonical inline form.
        assert!(text.contains("monsoon = { 00.10.01 00.12.15 }"), "second edited: {text}");
        // areas block + region shell survive.
        assert!(text.contains("areas = {\n\t\tm_area\n\t}"));
    }

    #[test]
    fn add_monsoon_then_remove_is_identity() {
        // Add a third monsoon (InsertStatement, authored at col 0, re-indented)…
        let stmt = "monsoon = {\n\t00.01.01\n\t00.04.30\n}";
        let added = apply(
            MONSOON_SRC.as_bytes(),
            &Edit::InsertStatement {
                block_path: vec!["m_region".into()],
                statement: stmt.into(),
            },
        )
        .unwrap();
        let text = String::from_utf8(added.clone()).unwrap();
        let net_block = paradox::parse(&text);
        let count = net_block
            .get_block("m_region")
            .unwrap()
            .items
            .iter()
            .filter(|(k, _)| k.as_deref() == Some("monsoon"))
            .count();
        assert_eq!(count, 3, "third monsoon added: {text}");
        assert!(text.contains("monsoon = {\n\t\t00.01.01\n\t\t00.04.30\n\t}"), "re-indented: {text}");
        // …removing the third (occurrence #2) restores the original bytes.
        let removed = apply(
            &added,
            &Edit::RemoveStatement {
                block_path: vec!["m_region".into()],
                key: "monsoon#2".into(),
                value: None,
            },
        )
        .unwrap();
        assert_eq!(removed, MONSOON_SRC.as_bytes(), "add then remove is identity");
    }

    #[test]
    fn remove_first_monsoon_keeps_second_byte_identical() {
        let out = apply(
            MONSOON_SRC.as_bytes(),
            &Edit::RemoveStatement {
                block_path: vec!["m_region".into()],
                key: "monsoon#0".into(),
                value: None,
            },
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(!text.contains("00.06.01"), "first removed: {text}");
        assert!(text.contains("monsoon = {\n\t\t00.11.01\n\t\t00.12.30\n\t}"), "second intact: {text}");
    }

    // --- real install + Anbennar smoke -----------------------------------

    #[test]
    fn parses_vanilla_geography() {
        let Some(vfs) = real_install() else { return };
        let loc = crate::loc::store(&vfs, INSTALL, None);
        let net = load_network(&vfs, &loc);
        assert!(net.areas.len() > 700, "vanilla has ~800 areas: {}", net.areas.len());
        assert!(net.regions.len() > 70, "vanilla has ~90 regions: {}", net.regions.len());
        assert!(!net.superregions.is_empty());
        // Every non-empty area rolls up to a region for the vast majority.
        let with_region = net.areas.iter().filter(|a| a.region.is_some()).count();
        assert!(with_region > net.areas.len() / 2, "most areas have a region");
        // Some region carries a monsoon block in vanilla.
        assert!(
            net.regions.iter().any(|r| r.has_monsoon),
            "expected at least one monsoon region"
        );
    }

    #[test]
    fn anbennar_geography_loads() {
        if !Path::new(INSTALL).join("map/provinces.bmp").is_file() || !Path::new(ANBENNAR).is_dir()
        {
            return;
        }
        let vfs = Vfs::new(INSTALL, Some(ANBENNAR)).unwrap();
        let loc = crate::loc::store(&vfs, INSTALL, Some(ANBENNAR));
        let net = load_network(&vfs, &loc);
        println!(
            "[geography:anbennar] {} areas, {} regions, {} superregions",
            net.areas.len(),
            net.regions.len(),
            net.superregions.len()
        );
        assert!(!net.areas.is_empty(), "anbennar should have areas");
        assert!(!net.regions.is_empty(), "anbennar should have regions");
    }
}
