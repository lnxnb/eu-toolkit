//! Sprint 8 — trade-node network: full node/route parsing, editing support, and
//! graph validation for the Trade Nodes map mode. The frontend (overlay, panel,
//! route editor) rides on top of the data + edit recipes documented here.
//!
//! # Ground-truth file format (`common/tradenodes/*.txt`)
//!
//! A node is a named top-level block. In vanilla every node lives in the single
//! `common/tradenodes/00_tradenodes.txt` (80 nodes, no duplicate top-level
//! keys), but a mod may add its own files or `replace_path` the folder; nodes
//! are addressed **by key** because keys are unique top-level keys (verified —
//! plain `[<key>]` mod_writer paths suffice for byte-surgical edits).
//!
//! ```text
//! african_great_lakes={
//!     location=4064                 # collection province (scalar)
//!     color={ 57 168 220 }          # optional; integer 0-255 rgb
//!     inland=yes                    # optional flag
//!     outgoing={                    # 0..n of these, in file order
//!         name="zanzibar"           # QUOTED target node key
//!         path={ 1273 1202 }        # sea provinces the goods traverse
//!         control={ 3351.000000 607.000000 3388.000000 610.000000 ... }
//!     }
//!     outgoing={ name="kongo" ... }
//!     members={ 1273 1649 ... 4064 ... }   # provinces belonging to the node
//! }
//! ```
//!
//! * **`end=yes`** marks a terminal node (genua/venice/english_channel in
//!   vanilla). Vanilla end nodes carry NO outgoing routes; field order is not
//!   fixed (`end=yes` may sit after `members`).
//! * **`outgoing`** repeats; the parser preserves file order and each route's
//!   position is its `index` (nth outgoing) → the mod_writer occurrence path
//!   segment `"outgoing#<index>"` addresses it byte-surgically.
//! * **`name`** is always double-quoted and equals another node's key.
//!
//! ## Control-point coordinate space (VERIFIED — bottom-left origin, map pixels)
//!
//! `control` holds pairs of floats `x y x y …` (2..n pairs, most `.000000`,
//! occasionally fractional e.g. `3319.139893`). They are **map-pixel coordinates
//! with the origin at the BOTTOM-LEFT and y increasing UPWARD** — the standard
//! Clausewitz map convention, identical to `map/positions.txt`.
//!
//! Evidence (vanilla, map is 5632×2048):
//! * `african_great_lakes` `location=4064`; positions.txt gives 4064 the point
//!   `(3308, 653)`. Its two routes' FIRST control points are `(3351, 607)` and
//!   `(3299, 628)` — clustered on the node's own location, so `control` shares
//!   positions.txt's coordinate space.
//! * `zambezi` `location=1191` (Mozambique, ~18°S) sits at y=388; the equatorial
//!   `african_great_lakes` sits at y=653. The souther province has the SMALLER y,
//!   so y increases northward ⇒ bottom-left origin.
//!
//! **Frontend transform.** The province-id pixel buffer and the canvas use a
//! TOP-LEFT origin (row 0 = top). [`TradeNetwork`] therefore reports each control
//! point already converted to top-left as `control`, and also the untouched
//! file-space value as `control_file`. The transform (both directions):
//!
//! ```text
//! x_top  = x_file                       x_file = x_top
//! y_top  = map_height - y_file           y_file = map_height - y_top
//! ```
//!
//! `map_height` is included in the payload. When the route editor drags a handle
//! (top-left space) it converts back with `y_file = map_height - y_top` before
//! writing.
//!
//! # Payload — [`TradeNetwork`] (command `get_trade_network`)
//!
//! One call returns the whole graph. Fields serialize snake_case:
//! * `map_width`, `map_height` — for the top-left⇄file transform above.
//! * `nodes: [TradeNode]` where `TradeNode` =
//!   `{ key, name (localized), color: [r,g,b]|null, location: u32|null,
//!      inland: bool, end: bool, members: [u32], source_file,
//!      outgoing: [Outgoing], incoming: [Incoming], raw_extra: [string] }`.
//!   * `source_file` — game-relative path of the file the node is defined in
//!     (its final/overriding definition), so the frontend emits edits against
//!     the right file.
//!   * `raw_extra` — names of unmodeled top-level keys inside the node block,
//!     surfaced read-only (preserve-unknown; byte-surgical edits keep them).
//! * `Outgoing` = `{ index, target, path: [u32], control: [[x,y]] (top-left),
//!    control_file: [[x,y]] (file-space) }`.
//! * `Incoming` = `{ from (source node key), outgoing_index }` — the reverse
//!   index is built backend-side so the panel's "incoming routes" list is free.
//!
//! # Edit recipes (frontend generates these `TypedEdit`s — see `edits.rs`)
//!
//! Every node lives under a unique top-level key, so `block_path`/`list_path`
//! start with `[<node_key>]`. All membership files may be one or several; a
//! province belongs to at most one node, so membership moves span two files in
//! general — use the `MoveId` typed edit (from_file/from_path → to_file/to_path)
//! which already handles the two-file case.
//!
//! * **Membership add**:  `AddId    { file, list_path: [node,"members"], id }`
//! * **Membership remove**: `RemoveId { file, list_path: [node,"members"], id }`
//! * **Membership steal** (paint over another node): `MoveId {
//!     from_file, from_path: [old_node,"members"], to_file,
//!     to_path: [new_node,"members"], id }` — removes from old, adds to new in
//!     one edit even across files.
//! * **Set location**: `SetScalar { file, path: [node,"location"], value, quoted:false }`
//! * **inland/end toggle on**:  `InsertStatement { file, block_path:[node],
//!     statement:"inland=yes" }` (or `"end=yes"`)
//! * **inland/end toggle off**: `RemoveStatement { file, block_path:[node],
//!     key:"inland", value:None }` (or `"end"`)
//! * **Color set**:  `SetBlock { file, path:[node,"color"], value:"r g b" }`
//!   (if the node has no `color` yet, `InsertStatement` a `color={ r g b }`).
//! * **Node create**: `AppendText { file, text: scaffold_node(...) }` into the
//!   project's tradenodes file (or `CreateFile` if none exists). See
//!   [`scaffold_node`] / command `scaffold_trade_node`.
//! * **Node delete**: `RemoveStatement { file, block_path:[], key:node, value:None }`
//!   (top-level key removal); members become node-less (legal).
//! * **Route add**: `InsertStatement { file, block_path:[node],
//!     statement: scaffold_route(target, path, control_file) }`. Geometry from
//!     command `derive_route_geometry`. See [`scaffold_route`] / command
//!     `scaffold_trade_route`.
//! * **Route delete**: `RemoveStatement { file, block_path:[node],
//!     key:"outgoing#<index>", value:None }` (occurrence-indexed).
//! * **Route reshape — control**: `SetBlock { file,
//!     path:[node,"outgoing#<index>","control"], value:"x y x y …" }` (file-space
//!     floats).
//! * **Route reshape — path**: `SetBlock { file,
//!     path:[node,"outgoing#<index>","path"], value:"id id id" }`.
//! * **Reverse direction**: delete the outgoing block from A (RemoveStatement
//!     `outgoing#n`) and InsertStatement a `scaffold_route` into B with the
//!     reversed control points and path.
//!
//! No new mod_writer edit kinds are needed — the existing toolkit expresses
//! every recipe. The only backend additions are the two scaffold text helpers
//! (single source of truth for formatting) and the two geometry/derivation
//! commands below.
//!
//! # `derive_route_geometry` (command)
//!
//! `derive_route_geometry(install, mod, from_node, to_node)` → `DerivedRoute {
//! control (top-left), control_file (file-space), path: [u32] }`. It computes
//! each node's `location` province **centroid** from the province-id pixel
//! buffer (`map_renderer::province_id_buffer` — robust, always on-map; cheaper
//! than reading hand-placed `map/positions.txt` and avoids its multi-slot
//! ambiguity), samples a straight line between them for a first-draft curve, and
//! collects the WATER provinces the line crosses as the draft `path`. Endpoints'
//! own location provinces are excluded from the path. Deliberately tolerant — a
//! first draft the user reshapes.

use std::collections::HashMap;

use crate::loc::LocStore;
use crate::paradox::{self, Block, Value};
use crate::vfs::Vfs;

/// The modeled top-level keys of a node block; everything else is `raw_extra`.
const KNOWN_KEYS: &[&str] = &["location", "color", "inland", "end", "outgoing", "members"];

// ---------------------------------------------------------------------------
// Payload types (serialize snake_case; see module header for the JSON contract).
// ---------------------------------------------------------------------------

#[derive(serde::Serialize, Clone, Debug)]
pub struct TradeNetwork {
    pub map_width: u32,
    pub map_height: u32,
    pub nodes: Vec<TradeNode>,
}

#[derive(serde::Serialize, Clone, Debug)]
pub struct TradeNode {
    pub key: String,
    pub name: String,
    pub color: Option<[u8; 3]>,
    pub location: Option<u32>,
    pub inland: bool,
    pub end: bool,
    pub members: Vec<u32>,
    pub source_file: String,
    pub outgoing: Vec<Outgoing>,
    pub incoming: Vec<Incoming>,
    pub raw_extra: Vec<String>,
}

#[derive(serde::Serialize, Clone, Debug)]
pub struct Outgoing {
    /// nth outgoing block in this node (0-based) → path segment `outgoing#<index>`.
    pub index: usize,
    pub target: String,
    pub path: Vec<u32>,
    /// Control points in TOP-LEFT origin map pixels (for canvas rendering).
    pub control: Vec<[f64; 2]>,
    /// Control points exactly as stored (BOTTOM-LEFT origin) for edit round-trips.
    pub control_file: Vec<[f64; 2]>,
}

#[derive(serde::Serialize, Clone, Debug)]
pub struct Incoming {
    /// Source node key of the route pointing at this node.
    pub from: String,
    /// Index of the route within the source node's `outgoing` list.
    pub outgoing_index: usize,
}

/// First-draft geometry for a new route (command `derive_route_geometry`).
#[derive(serde::Serialize, Clone, Debug)]
pub struct DerivedRoute {
    /// Straight-line control points in TOP-LEFT origin (for rendering).
    pub control: Vec<[f64; 2]>,
    /// Same points in FILE (bottom-left) space — feed straight to `scaffold_route`.
    pub control_file: Vec<[f64; 2]>,
    /// Water provinces the line crosses (draft `path`, endpoints excluded).
    pub path: Vec<u32>,
}

// ---------------------------------------------------------------------------
// Raw parse (shared by the payload and the validation graph).
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct RawOutgoing {
    target: String,
    path: Vec<u32>,
    /// File-space (bottom-left) control points.
    control: Vec<[f64; 2]>,
}

#[derive(Clone, Debug)]
struct RawNode {
    key: String,
    source_file: String,
    location: Option<u32>,
    color: Option<[u8; 3]>,
    inland: bool,
    end: bool,
    members: Vec<u32>,
    outgoing: Vec<RawOutgoing>,
    raw_extra: Vec<String>,
}

/// Parses a `control={ x y x y … }` block into file-space `[x, y]` pairs.
fn parse_control(block: &Block) -> Vec<[f64; 2]> {
    let floats: Vec<f64> = block.bare_scalars().filter_map(|s| s.parse().ok()).collect();
    floats.chunks_exact(2).map(|c| [c[0], c[1]]).collect()
}

/// Extracts a [`RawNode`] from a top-level `key = { … }` block, or `None` if the
/// block isn't a node (no `location` and no `members`).
fn raw_node_from_block(key: &str, source_file: &str, block: &Block) -> Option<RawNode> {
    let has_members = block.get_block("members").is_some();
    let location = block.get_scalar("location").and_then(|s| s.parse::<u32>().ok());
    if location.is_none() && !has_members {
        return None;
    }

    let color = block.get_block("color").and_then(paradox::color_from_block);
    let inland = block.get_scalar("inland") == Some("yes");
    let end = block.get_scalar("end") == Some("yes");
    let members = block
        .get_block("members")
        .map(|b| b.bare_ids())
        .unwrap_or_default();

    // Every `outgoing = { … }` in file order.
    let mut outgoing = Vec::new();
    for (k, v) in &block.items {
        if k.as_deref() != Some("outgoing") {
            continue;
        }
        let Value::Block(ob) = v else { continue };
        let target = ob.get_scalar("name").unwrap_or_default().to_string();
        let path = ob.get_block("path").map(|b| b.bare_ids()).unwrap_or_default();
        let control = ob.get_block("control").map(parse_control).unwrap_or_default();
        outgoing.push(RawOutgoing {
            target,
            path,
            control,
        });
    }

    // Unmodeled top-level keys (preserve-unknown; shown read-only).
    let mut raw_extra: Vec<String> = Vec::new();
    for (k, _) in &block.items {
        if let Some(name) = k {
            if !KNOWN_KEYS.contains(&name.as_str()) && !raw_extra.contains(name) {
                raw_extra.push(name.clone());
            }
        }
    }

    Some(RawNode {
        key: key.to_string(),
        source_file: source_file.to_string(),
        location,
        color,
        inland,
        end,
        members,
        outgoing,
        raw_extra,
    })
}

/// Parses every trade node across `common/tradenodes/*.txt` in game load order.
/// A node key defined more than once keeps its LAST (overriding) definition —
/// mirroring the game — while retaining first-seen ordering for stable output.
fn parse_raw_nodes(vfs: &Vfs) -> Vec<RawNode> {
    let mut order: Vec<String> = Vec::new();
    let mut by_key: HashMap<String, RawNode> = HashMap::new();

    for (name, path) in vfs.list_dir("common/tradenodes") {
        if !name.to_lowercase().ends_with(".txt") {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let source_file = format!("common/tradenodes/{name}");
        let root = paradox::parse(&String::from_utf8_lossy(&bytes));
        for (key, block) in root.key_blocks() {
            if let Some(node) = raw_node_from_block(key, &source_file, block) {
                if !by_key.contains_key(key) {
                    order.push(key.to_string());
                }
                by_key.insert(key.to_string(), node);
            }
        }
    }

    order.into_iter().filter_map(|k| by_key.remove(&k)).collect()
}

// ---------------------------------------------------------------------------
// Full network payload.
// ---------------------------------------------------------------------------

/// Reads `map/provinces.bmp`'s 24-bit BMP header for the map dimensions without
/// decoding the whole ~34 MB image.
fn map_dimensions(vfs: &Vfs) -> Result<(u32, u32), String> {
    use std::io::Read;
    let path = vfs
        .resolve("map/provinces.bmp")
        .ok_or("map/provinces.bmp not found")?;
    let mut f = std::fs::File::open(&path).map_err(|e| format!("open provinces.bmp: {e}"))?;
    let mut hdr = [0u8; 26];
    f.read_exact(&mut hdr)
        .map_err(|e| format!("read provinces.bmp header: {e}"))?;
    if &hdr[0..2] != b"BM" {
        return Err("provinces.bmp is not a BMP".into());
    }
    let w = i32::from_le_bytes(hdr[18..22].try_into().unwrap());
    let h = i32::from_le_bytes(hdr[22..26].try_into().unwrap());
    Ok((w.unsigned_abs(), h.unsigned_abs()))
}

/// Builds the full [`TradeNetwork`] payload (nodes + routes + reverse index).
pub fn load_network(vfs: &Vfs, loc: &LocStore, map_width: u32, map_height: u32) -> TradeNetwork {
    let raw = parse_raw_nodes(vfs);

    // Index for the reverse (incoming) lookup.
    let mut index_of: HashMap<String, usize> = HashMap::new();
    for (i, n) in raw.iter().enumerate() {
        index_of.insert(n.key.clone(), i);
    }

    let h = map_height as f64;
    let mut nodes: Vec<TradeNode> = raw
        .iter()
        .map(|n| {
            let outgoing = n
                .outgoing
                .iter()
                .enumerate()
                .map(|(index, o)| Outgoing {
                    index,
                    target: o.target.clone(),
                    path: o.path.clone(),
                    control: o.control.iter().map(|[x, y]| [*x, h - *y]).collect(),
                    control_file: o.control.clone(),
                })
                .collect();
            TradeNode {
                key: n.key.clone(),
                name: loc.resolve(&n.key),
                color: n.color,
                location: n.location,
                inland: n.inland,
                end: n.end,
                members: n.members.clone(),
                source_file: n.source_file.clone(),
                outgoing,
                incoming: Vec::new(),
                raw_extra: n.raw_extra.clone(),
            }
        })
        .collect();

    // Reverse index: append an Incoming to each route's target node.
    for i in 0..raw.len() {
        for (j, o) in raw[i].outgoing.iter().enumerate() {
            if let Some(&t) = index_of.get(&o.target) {
                nodes[t].incoming.push(Incoming {
                    from: raw[i].key.clone(),
                    outgoing_index: j,
                });
            }
        }
    }

    TradeNetwork {
        map_width,
        map_height,
        nodes,
    }
}

/// The whole trade graph in one payload. Registered by the orchestrator.
#[allow(dead_code)]
#[tauri::command(async)]
pub fn get_trade_network(
    install_path: String,
    mod_path: Option<String>,
) -> Result<TradeNetwork, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    let loc = crate::loc::store(&vfs, &install_path, mod_path.as_deref());
    let (w, h) = map_dimensions(&vfs)?;
    Ok(load_network(&vfs, &loc, w, h))
}

// ---------------------------------------------------------------------------
// Validation graph (consumed by validation::check_trade_nodes).
// ---------------------------------------------------------------------------

/// A route as the graph checker sees it.
#[derive(Clone, Debug)]
pub struct GraphRoute {
    pub target: String,
    pub path: Vec<u32>,
}

/// A node reduced to what the graph checks need.
#[derive(Clone, Debug)]
pub struct GraphNode {
    pub key: String,
    pub location: Option<u32>,
    pub end: bool,
    pub members: Vec<u32>,
    pub routes: Vec<GraphRoute>,
}

/// The trade graph for validation (no map-pixel transform needed).
pub fn node_graph(vfs: &Vfs) -> Vec<GraphNode> {
    parse_raw_nodes(vfs)
        .into_iter()
        .map(|n| GraphNode {
            key: n.key,
            location: n.location,
            end: n.end,
            members: n.members,
            routes: n
                .outgoing
                .into_iter()
                .map(|o| GraphRoute {
                    target: o.target,
                    path: o.path,
                })
                .collect(),
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Scaffolds (single source of truth for node/route text; unit-tested).
// ---------------------------------------------------------------------------

/// A brand-new node block: `location`, distinct `color`, `members={ location }`,
/// no routes. Authored at column 0 (append at top level, or `InsertStatement`
/// re-indents). Matches vanilla tab style; parses back as a node.
pub fn scaffold_node(key: &str, location: u32, color: [u8; 3]) -> String {
    format!(
        "{key}={{\n\tlocation={location}\n\tcolor={{ {r} {g} {b} }}\n\tmembers={{\n\t\t{location}\n\t}}\n}}",
        r = color[0],
        g = color[1],
        b = color[2],
    )
}

/// A new `outgoing` route block. `control` is FILE-space (bottom-left) — the
/// caller converts from top-left with `y_file = map_height - y_top` (or takes
/// `DerivedRoute.control_file` verbatim). Authored at column 0; `InsertStatement`
/// into `[<node>]` re-indents it into the node.
pub fn scaffold_route(target: &str, path: &[u32], control: &[[f64; 2]]) -> String {
    let path_str = path
        .iter()
        .map(|id| id.to_string())
        .collect::<Vec<_>>()
        .join(" ");
    let ctrl_str = control
        .iter()
        .map(|[x, y]| format!("{x:.6} {y:.6}"))
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        "outgoing={{\n\tname=\"{target}\"\n\tpath={{\n\t\t{path_str}\n\t}}\n\tcontrol={{\n\t\t{ctrl_str}\n\t}}\n}}"
    )
}

/// Command wrapper around [`scaffold_node`] so the frontend obtains correctly
/// formatted text for an `AppendText`/`CreateFile` edit.
#[allow(dead_code)]
#[tauri::command(async)]
pub fn scaffold_trade_node(key: String, location: u32, color: [u8; 3]) -> String {
    scaffold_node(&key, location, color)
}

/// Command wrapper around [`scaffold_route`] (`control` is file-space).
#[allow(dead_code)]
#[tauri::command(async)]
pub fn scaffold_trade_route(target: String, path: Vec<u32>, control: Vec<[f64; 2]>) -> String {
    scaffold_route(&target, &path, &control)
}

// ---------------------------------------------------------------------------
// Route geometry derivation.
// ---------------------------------------------------------------------------

/// Water province ids from `map/default.map` (`sea_starts` + `lakes`).
fn water_ids(vfs: &Vfs) -> std::collections::HashSet<u32> {
    let mut water = std::collections::HashSet::new();
    if let Ok(bytes) = vfs.read("map/default.map") {
        let block = paradox::parse(&String::from_utf8_lossy(&bytes));
        for key in ["sea_starts", "lakes"] {
            if let Some(list) = block.get_block(key) {
                water.extend(list.bare_ids());
            }
        }
    }
    water
}

/// Decoded province-id pixel buffer: (width, height, row-major ids, top-left).
struct IdBuffer {
    width: usize,
    height: usize,
    ids: Vec<u32>,
}

impl IdBuffer {
    fn at(&self, x: usize, y: usize) -> u32 {
        if x < self.width && y < self.height {
            self.ids[y * self.width + x]
        } else {
            0
        }
    }
}

/// Reuses `map_renderer::province_id_buffer` ([w][h][u16 id per pixel]) and
/// widens ids back to u32 for centroid/hit lookups.
fn id_buffer(vfs: &Vfs) -> Result<IdBuffer, String> {
    let raw = crate::map_renderer::province_id_buffer(vfs)?;
    if raw.len() < 8 {
        return Err("province id buffer too small".into());
    }
    let width = u32::from_le_bytes(raw[0..4].try_into().unwrap()) as usize;
    let height = u32::from_le_bytes(raw[4..8].try_into().unwrap()) as usize;
    let ids: Vec<u32> = raw[8..]
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]) as u32)
        .collect();
    Ok(IdBuffer { width, height, ids })
}

/// Centroid (top-left pixel coords) of every requested province id, in one pass.
fn centroids(buf: &IdBuffer, wanted: &[u32]) -> HashMap<u32, (f64, f64)> {
    let mut acc: HashMap<u32, (f64, f64, u64)> = HashMap::new();
    for &id in wanted {
        acc.insert(id, (0.0, 0.0, 0));
    }
    for (i, &id) in buf.ids.iter().enumerate() {
        if let Some(e) = acc.get_mut(&id) {
            e.0 += (i % buf.width) as f64;
            e.1 += (i / buf.width) as f64;
            e.2 += 1;
        }
    }
    acc.into_iter()
        .filter(|(_, (_, _, n))| *n > 0)
        .map(|(id, (sx, sy, n))| (id, (sx / n as f64, sy / n as f64)))
        .collect()
}

/// Signed horizontal delta from `ax` to `bx` the SHORT way around a world of
/// width `w` — the map wraps at the antimeridian, so a span longer than w/2
/// is measured the other way round (|result| ≤ w/2).
pub fn wrap_dx(ax: f64, bx: f64, w: f64) -> f64 {
    let dx = bx - ax;
    dx - (dx / w).round() * w
}

/// First-draft geometry between two nodes' location provinces: a straight
/// line the short way around the world (wrap-aware).
pub fn derive_geometry(
    vfs: &Vfs,
    from_node: &str,
    to_node: &str,
) -> Result<DerivedRoute, String> {
    let nodes = parse_raw_nodes(vfs);
    let find = |key: &str| {
        nodes
            .iter()
            .find(|n| n.key == key)
            .ok_or_else(|| format!("unknown trade node: {key}"))
    };
    let from = find(from_node)?;
    let to = find(to_node)?;
    let from_id = from
        .location
        .ok_or_else(|| format!("node {from_node} has no location"))?;
    let to_id = to
        .location
        .ok_or_else(|| format!("node {to_node} has no location"))?;

    let buf = id_buffer(vfs)?;
    let cents = centroids(&buf, &[from_id, to_id]);
    let a = *cents
        .get(&from_id)
        .ok_or_else(|| format!("location province {from_id} not found on map"))?;
    let b = *cents
        .get(&to_id)
        .ok_or_else(|| format!("location province {to_id} not found on map"))?;

    let h = buf.height as f64;
    let w = buf.width as f64;

    // World wrap: go the SHORT way around. If the straight horizontal span
    // exceeds half the map width (e.g. Asia ↔ the American west coast), derive
    // in unwrapped space — the endpoint shifted by ±w — and wrap every emitted
    // coordinate back onto the map, matching the vanilla convention (control
    // points stay in-bounds; the wrap is implied by the jump between them).
    let bx = a.0 + wrap_dx(a.0, b.0, w);

    // Draft control curve: 5 evenly-spaced points including both endpoints.
    let steps = 4usize;
    let mut control = Vec::with_capacity(steps + 1);
    for k in 0..=steps {
        let t = k as f64 / steps as f64;
        control.push([(a.0 + (bx - a.0) * t).rem_euclid(w), a.1 + (b.1 - a.1) * t]);
    }
    let control_file: Vec<[f64; 2]> = control.iter().map(|[x, y]| [*x, h - *y]).collect();

    // Draft path: unique WATER provinces the line crosses, in crossing order,
    // excluding the two endpoint location provinces and the none-sentinel (0).
    let water = water_ids(vfs);
    let dist = ((bx - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
    let samples = (dist.ceil() as usize).max(1) * 2;
    let mut path: Vec<u32> = Vec::new();
    for s in 0..=samples {
        let t = s as f64 / samples as f64;
        let x = (a.0 + (bx - a.0) * t).rem_euclid(w).round() as usize % buf.width;
        let y = (a.1 + (b.1 - a.1) * t).round() as usize;
        let id = buf.at(x, y);
        if id != 0 && id != from_id && id != to_id && water.contains(&id) && !path.contains(&id) {
            path.push(id);
        }
    }

    Ok(DerivedRoute {
        control,
        control_file,
        path,
    })
}

/// Auto-derived first-draft geometry for a new route. Registered by the orchestrator.
#[allow(dead_code)]
#[tauri::command(async)]
pub fn derive_route_geometry(
    install_path: String,
    mod_path: Option<String>,
    from_node: String,
    to_node: String,
) -> Result<DerivedRoute, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    derive_geometry(&vfs, &from_node, &to_node)
}

// ---------------------------------------------------------------------------
// Re-derive a route's PATH from its current (edited) control curve.
//
// `derive_geometry` above ignores hand-edited control points and re-draws a
// straight node-to-node line. When the user has reshaped a route's curve, the
// "Re-derive path" button instead wants the water provinces UNDER the actual
// curve. These helpers mirror the frontend spline (`tradenet.ts`
// unwrapControl/samplePolyline/catmull) so the derived path matches exactly
// what the editor draws, and the wrap machinery (`wrap_dx`/unwrap) keeps an
// antimeridian-crossing route going the short way.
// ---------------------------------------------------------------------------

/// Catmull-Rom point at `t` through `p1`→`p2` with neighbors `p0`/`p3`. Matches
/// frontend `tradenet.ts` `catmull` coefficient-for-coefficient.
fn catmull(p0: [f64; 2], p1: [f64; 2], p2: [f64; 2], p3: [f64; 2], t: f64) -> [f64; 2] {
    let t2 = t * t;
    let t3 = t2 * t;
    let f = |a: f64, b: f64, c: f64, d: f64| {
        0.5 * (2.0 * b
            + (-a + c) * t
            + (2.0 * a - 5.0 * b + 4.0 * c - d) * t2
            + (-a + 3.0 * b - 3.0 * c + d) * t3)
    };
    [
        f(p0[0], p1[0], p2[0], p3[0]),
        f(p0[1], p1[1], p2[1], p3[1]),
    ]
}

/// Unwraps a control sequence: each point after the first is shifted by the
/// multiple of `w` (map width) that puts it nearest its predecessor, so an
/// implied antimeridian crossing becomes a continuous line (x may leave
/// `[0, w)`). Mirrors frontend `unwrapControl`.
fn unwrap_control(points: &[[f64; 2]], w: f64) -> Vec<[f64; 2]> {
    if points.is_empty() || w <= 0.0 {
        return points.to_vec();
    }
    let mut out = vec![points[0]];
    for i in 1..points.len() {
        let px = out[i - 1][0];
        // wrap_dx(px, points[i][0], w) is the short-way delta; predecessor + it
        // is the nearest unwrapped x. Equivalent to the frontend's round form.
        let x = px + wrap_dx(px, points[i][0], w);
        out.push([x, points[i][1]]);
    }
    out
}

/// Samples a Catmull-Rom curve through `points` with `seg` samples per span.
/// With ≤2 points it is the straight segment. Mirrors frontend `samplePolyline`
/// (endpoints duplicated so the curve passes through first/last control point).
fn sample_polyline(points: &[[f64; 2]], seg: usize) -> Vec<[f64; 2]> {
    if points.len() <= 2 {
        return points.to_vec();
    }
    let n = points.len();
    let mut out = Vec::with_capacity((n - 1) * seg + 1);
    for i in 0..n - 1 {
        let p0 = points[if i == 0 { 0 } else { i - 1 }];
        let p1 = points[i];
        let p2 = points[i + 1];
        let p3 = points[if i + 2 < n { i + 2 } else { n - 1 }];
        for s in 0..seg {
            let t = s as f64 / seg as f64;
            out.push(catmull(p0, p1, p2, p3, t));
        }
    }
    out.push(points[n - 1]);
    out
}

/// Collects the WATER provinces under a route's control curve (top-left origin
/// map pixels) against a pre-decoded id buffer + water set. Unwraps the control
/// points, samples the spline the same way the editor renders it, then walks
/// each sampled sub-segment at ~1px resolution so no crossed province is
/// skipped. Endpoints' own location provinces (`from_id`/`to_id`) and the
/// none-sentinel are excluded; order is first-cross.
fn path_under_control_buf(
    buf: &IdBuffer,
    water: &std::collections::HashSet<u32>,
    control_top_left: &[[f64; 2]],
    from_id: Option<u32>,
    to_id: Option<u32>,
) -> Vec<u32> {
    if control_top_left.len() < 2 {
        return Vec::new();
    }
    let w = buf.width as f64;

    // Spline in continuous (unwrapped) space, matching the editor.
    let un = unwrap_control(control_top_left, w);
    let curve = sample_polyline(&un, 16);

    let mut path: Vec<u32> = Vec::new();
    let sample = |x: f64, y: f64, path: &mut Vec<u32>| {
        if y < 0.0 {
            return;
        }
        let xi = (x.rem_euclid(w).round() as usize) % buf.width;
        let yi = y.round() as usize;
        let id = buf.at(xi, yi);
        if id != 0
            && Some(id) != from_id
            && Some(id) != to_id
            && water.contains(&id)
            && !path.contains(&id)
        {
            path.push(id);
        }
    };

    // Walk every sub-segment of the sampled curve at ~1px steps.
    for seg in curve.windows(2) {
        let a = seg[0];
        let b = seg[1];
        let dist = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
        let steps = (dist.ceil() as usize).max(1);
        for s in 0..steps {
            let t = s as f64 / steps as f64;
            sample(a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, &mut path);
        }
    }
    if let Some(last) = curve.last() {
        sample(last[0], last[1], &mut path);
    }

    path
}

/// Water provinces under a route's control curve; decodes the id buffer + water
/// set, then delegates to [`path_under_control_buf`].
pub fn path_under_control(
    vfs: &Vfs,
    control_top_left: &[[f64; 2]],
    from_id: Option<u32>,
    to_id: Option<u32>,
) -> Result<Vec<u32>, String> {
    if control_top_left.len() < 2 {
        return Ok(Vec::new());
    }
    let buf = id_buffer(vfs)?;
    let water = water_ids(vfs);
    Ok(path_under_control_buf(
        &buf,
        &water,
        control_top_left,
        from_id,
        to_id,
    ))
}

/// Re-derives a route's `path` from its CURRENT (edited) control curve rather
/// than a straight node-to-node line. `control_file` is FILE-space (bottom-left)
/// — exactly the route's stored/edited control — and is flipped to top-left
/// internally. `from_node`/`to_node`, when given, exclude the endpoints' own
/// location provinces from the path. Registered by the orchestrator.
#[allow(dead_code)]
#[tauri::command(async)]
pub fn derive_route_path(
    install_path: String,
    mod_path: Option<String>,
    control_file: Vec<[f64; 2]>,
    from_node: Option<String>,
    to_node: Option<String>,
) -> Result<Vec<u32>, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    let (_w, h) = map_dimensions(&vfs)?;
    let hf = h as f64;
    // File (bottom-left) → top-left: y_top = height - y_file.
    let control_top_left: Vec<[f64; 2]> =
        control_file.iter().map(|[x, y]| [*x, hf - *y]).collect();

    // Resolve endpoint location provinces for exclusion (best-effort, one parse).
    let nodes = parse_raw_nodes(&vfs);
    let loc_of = |key: &Option<String>| -> Option<u32> {
        let key = key.as_deref()?;
        nodes.iter().find(|n| n.key == key).and_then(|n| n.location)
    };
    let from_id = loc_of(&from_node);
    let to_id = loc_of(&to_node);

    path_under_control(&vfs, &control_top_left, from_id, to_id)
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

    // --- vanilla parse ---------------------------------------------------

    #[test]
    fn parses_vanilla_network() {
        let Some(vfs) = real_install() else { return };
        let loc = crate::loc::store(&vfs, INSTALL, None);
        let (w, h) = map_dimensions(&vfs).unwrap();
        assert_eq!((w, h), (5632, 2048), "vanilla map dimensions");
        let net = load_network(&vfs, &loc, w, h);
        assert_eq!(net.nodes.len(), 80, "vanilla node count matches file");

        let node = |k: &str| net.nodes.iter().find(|n| n.key == k).unwrap();

        // Venice / Genoa: end nodes, colored, no outgoing.
        let venice = node("venice");
        assert_eq!(venice.location, Some(1308));
        assert_eq!(venice.color, Some([54, 167, 156]));
        assert!(venice.end);
        assert!(venice.outgoing.is_empty());
        assert!(venice.members.contains(&1308));

        let genua = node("genua");
        assert_eq!(genua.location, Some(1298));
        assert_eq!(genua.color, Some([218, 215, 56]));
        assert!(genua.end);

        // African Great Lakes: inland, two routes, control-point convention.
        let agl = node("african_great_lakes");
        assert_eq!(agl.location, Some(4064));
        assert_eq!(agl.color, Some([57, 168, 220]));
        assert!(agl.inland);
        assert!(!agl.end);
        assert_eq!(agl.outgoing.len(), 2);

        let zanzibar = agl
            .outgoing
            .iter()
            .find(|o| o.target == "zanzibar")
            .expect("route to zanzibar");
        assert_eq!(zanzibar.index, 0);
        assert_eq!(zanzibar.path, vec![1273, 1202]);
        // File-space first control point (bottom-left origin) verbatim.
        assert_eq!(zanzibar.control_file[0], [3351.0, 607.0]);
        // Top-left transform: y_top = height - y_file.
        assert_eq!(zanzibar.control[0], [3351.0, 2048.0 - 607.0]);

        // Reverse index populated (something steers into venice).
        assert!(
            !venice.incoming.is_empty(),
            "venice should have incoming routes"
        );
    }

    // --- scaffolds -------------------------------------------------------

    #[test]
    fn scaffold_node_parses_back() {
        let text = scaffold_node("my_node", 1234, [128, 64, 200]);
        let block = paradox::parse(&text);
        let node = block.get_block("my_node").expect("node block");
        assert_eq!(node.get_scalar("location"), Some("1234"));
        assert_eq!(node.get_block("members").unwrap().bare_ids(), vec![1234]);
        assert_eq!(
            paradox::color_from_block(node.get_block("color").unwrap()),
            Some([128, 64, 200])
        );
        // And it registers as a node.
        let raw = raw_node_from_block("my_node", "x", node).unwrap();
        assert_eq!(raw.location, Some(1234));
        assert!(!raw.end && !raw.inland);
    }

    #[test]
    fn scaffold_route_inserts_and_parses() {
        let node = b"n1={\n\tlocation=1\n\tmembers={\n\t\t1\n\t}\n}\n";
        let route = scaffold_route("n2", &[10, 11, 12], &[[1.5, 2.5], [3.0, 4.0]]);
        let out = apply(
            node,
            &Edit::InsertStatement {
                block_path: vec!["n1".into()],
                statement: route,
            },
        )
        .unwrap();
        let block = paradox::parse(&String::from_utf8_lossy(&out));
        let n1 = block.get_block("n1").unwrap();
        let og = n1.get_block("outgoing").expect("outgoing inserted");
        assert_eq!(og.get_scalar("name"), Some("n2"));
        assert_eq!(og.get_block("path").unwrap().bare_ids(), vec![10, 11, 12]);
        assert_eq!(parse_control(og.get_block("control").unwrap()), vec![[1.5, 2.5], [3.0, 4.0]]);
        // Original node data intact.
        assert_eq!(n1.get_scalar("location"), Some("1"));
    }

    // --- edit recipe round-trips (byte-surgical) -------------------------

    // A two-node fixture in the vanilla file style.
    const FIXTURE: &[u8] = b"alpha={\n\tlocation=100\n\tcolor={ 10 20 30 }\n\tinland=yes\n\toutgoing={\n\t\tname=\"beta\"\n\t\tpath={\n\t\t\t500 501\n\t\t}\n\t\tcontrol={\n\t\t\t1.000000 2.000000 3.000000 4.000000\n\t\t}\n\t}\n\tmembers={\n\t\t100 101 102\n\t}\n}\nbeta={\n\tlocation=200\n\tcolor={ 40 50 60 }\n\tend=yes\n\tmembers={\n\t\t200 201\n\t}\n}\n";

    /// Asserts everything outside `[start, before_end)` round-trips byte-identically.
    fn assert_outside(before: &[u8], after: &[u8], start: usize, before_end: usize) {
        assert_eq!(&before[..start], &after[..start], "prefix changed");
        let delta = after.len() as isize - before.len() as isize;
        let after_end = (before_end as isize + delta) as usize;
        assert_eq!(&before[before_end..], &after[after_end..], "suffix changed");
    }

    fn find(hay: &[u8], needle: &[u8]) -> usize {
        hay.windows(needle.len())
            .position(|w| w == needle)
            .unwrap_or_else(|| panic!("not found: {}", String::from_utf8_lossy(needle)))
    }

    #[test]
    fn color_set_roundtrip() {
        let out = apply(
            FIXTURE,
            &Edit::SetBlock {
                path: vec!["alpha".into(), "color".into()],
                value: "99 88 77".into(),
            },
        )
        .unwrap();
        let text = String::from_utf8_lossy(&out);
        assert!(text.contains("color={ 99 88 77 }"));
        let s = find(FIXTURE, b"{ 10 20 30 }");
        assert_outside(FIXTURE, &out, s, s + b"{ 10 20 30 }".len());
    }

    #[test]
    fn member_add_and_remove_roundtrip() {
        let added = apply(
            FIXTURE,
            &Edit::AddId {
                list_path: vec!["alpha".into(), "members".into()],
                id: "103".into(),
            },
        )
        .unwrap();
        assert!(String::from_utf8_lossy(&added).contains("100 101 102\n\t\t103"));
        // Round-trips back to the original when removed again.
        let removed = apply(
            &added,
            &Edit::RemoveId {
                list_path: vec!["alpha".into(), "members".into()],
                id: "103".into(),
            },
        )
        .unwrap();
        assert_eq!(removed, FIXTURE, "add then remove is identity");
    }

    #[test]
    fn route_control_reshape_roundtrip() {
        let out = apply(
            FIXTURE,
            &Edit::SetBlock {
                path: vec!["alpha".into(), "outgoing#0".into(), "control".into()],
                value: "5.000000 6.000000".into(),
            },
        )
        .unwrap();
        let text = String::from_utf8_lossy(&out);
        assert!(text.contains("control={ 5.000000 6.000000 }"));
        // Path and name of the same route untouched.
        assert!(text.contains("name=\"beta\""));
        assert!(text.contains("500 501"));
    }

    #[test]
    fn route_path_reshape_roundtrip() {
        let out = apply(
            FIXTURE,
            &Edit::SetBlock {
                path: vec!["alpha".into(), "outgoing#0".into(), "path".into()],
                value: "500 501 502".into(),
            },
        )
        .unwrap();
        assert!(String::from_utf8_lossy(&out).contains("path={ 500 501 502 }"));
    }

    #[test]
    fn route_add_and_delete_roundtrip() {
        let route = scaffold_route("beta", &[600], &[[7.0, 8.0]]);
        let added = apply(
            FIXTURE,
            &Edit::InsertStatement {
                block_path: vec!["alpha".into()],
                statement: route,
            },
        )
        .unwrap();
        let text = String::from_utf8_lossy(&added);
        // alpha now has two outgoing routes.
        let n = text.matches("outgoing={").count();
        assert_eq!(n, 2, "second route inserted");
        // Delete the newly-added one (occurrence #1).
        let deleted = apply(
            &added,
            &Edit::RemoveStatement {
                block_path: vec!["alpha".into()],
                key: "outgoing#1".into(),
                value: None,
            },
        )
        .unwrap();
        assert_eq!(deleted, FIXTURE, "add then delete route is identity");
    }

    #[test]
    fn node_scaffold_append_and_delete_roundtrip() {
        let node = scaffold_node("gamma", 300, [1, 2, 3]);
        let appended = apply(FIXTURE, &Edit::Append { text: node }).unwrap();
        let text = String::from_utf8_lossy(&appended);
        assert!(text.contains("gamma={"));
        assert!(text.contains("location=300"));
        // Delete the top-level node key.
        let deleted = apply(
            &appended,
            &Edit::RemoveStatement {
                block_path: vec![],
                key: "gamma".into(),
                value: None,
            },
        )
        .unwrap();
        assert_eq!(deleted, FIXTURE, "scaffold then delete node is identity");
    }

    #[test]
    fn reverse_direction_roundtrip() {
        // Delete alpha->beta, add beta->alpha with reversed control/path.
        let stripped = apply(
            FIXTURE,
            &Edit::RemoveStatement {
                block_path: vec!["alpha".into()],
                key: "outgoing#0".into(),
                value: None,
            },
        )
        .unwrap();
        assert!(!String::from_utf8_lossy(&stripped).contains("name=\"beta\""));
        let rev = scaffold_route("alpha", &[501, 500], &[[3.0, 4.0], [1.0, 2.0]]);
        let out = apply(
            &stripped,
            &Edit::InsertStatement {
                block_path: vec!["beta".into()],
                statement: rev,
            },
        )
        .unwrap();
        let block = paradox::parse(&String::from_utf8_lossy(&out));
        let beta = block.get_block("beta").unwrap();
        let og = beta.get_block("outgoing").unwrap();
        assert_eq!(og.get_scalar("name"), Some("alpha"));
        assert_eq!(og.get_block("path").unwrap().bare_ids(), vec![501, 500]);
    }

    // --- inland/end toggles ---------------------------------------------

    #[test]
    fn end_toggle_on_and_off_roundtrip() {
        // beta already has end=yes; toggling it off then on returns to origin.
        let off = apply(
            FIXTURE,
            &Edit::RemoveStatement {
                block_path: vec!["beta".into()],
                key: "end".into(),
                value: None,
            },
        )
        .unwrap();
        assert!(!String::from_utf8_lossy(&off).contains("end=yes"));
        let on = apply(
            &off,
            &Edit::InsertStatement {
                block_path: vec!["beta".into()],
                statement: "end=yes".into(),
            },
        )
        .unwrap();
        assert!(String::from_utf8_lossy(&on).contains("end=yes"));
    }

    // --- derive_route_geometry sanity -----------------------------------

    #[test]
    fn derive_geometry_between_two_nodes() {
        let Some(vfs) = real_install() else { return };
        // Two nearby coastal end nodes.
        let d = derive_geometry(&vfs, "genua", "venice").unwrap();
        assert_eq!(d.control.len(), 5, "5 draft control points");
        assert_eq!(d.control_file.len(), 5);
        // Endpoints of the draft curve differ (a real line was drawn).
        assert_ne!(d.control.first(), d.control.last());
        // control_file is the bottom-left mirror of control.
        let (_w, h) = map_dimensions(&vfs).unwrap();
        for (top, file) in d.control.iter().zip(&d.control_file) {
            assert!((top[0] - file[0]).abs() < 1e-6);
            assert!((top[1] - (h as f64 - file[1])).abs() < 1e-6);
        }
        println!("[tradenodes] genua->venice draft path ({} provs): {:?}", d.path.len(), d.path);
    }

    #[test]
    fn wrap_dx_short_way() {
        // No wrap needed: plain delta.
        assert_eq!(wrap_dx(100.0, 300.0, 1000.0), 200.0);
        assert_eq!(wrap_dx(300.0, 100.0, 1000.0), -200.0);
        // Wrap: the short way crosses the antimeridian.
        assert_eq!(wrap_dx(50.0, 950.0, 1000.0), -100.0);
        assert_eq!(wrap_dx(950.0, 50.0, 1000.0), 100.0);
        // Exactly half the world away: either direction, magnitude w/2.
        assert_eq!(wrap_dx(0.0, 500.0, 1000.0).abs(), 500.0);
    }

    #[test]
    fn derive_geometry_wraps_around_the_world() {
        let Some(vfs) = real_install() else { return };
        let loc = crate::loc::store(&vfs, INSTALL, None);
        let (w, h) = map_dimensions(&vfs).unwrap();
        let net = load_network(&vfs, &loc, w, h);
        let wf = w as f64;
        // Vanilla ships routes that wrap the antimeridian, encoded as a
        // control-x jump larger than half the map width — find one so the test
        // runs against real data.
        let wrapping = net.nodes.iter().find_map(|n| {
            n.outgoing.iter().find_map(|o| {
                o.control
                    .windows(2)
                    .any(|p| (p[1][0] - p[0][0]).abs() > wf / 2.0)
                    .then(|| (n.key.clone(), o.target.clone()))
            })
        });
        let Some((from, to)) = wrapping else {
            panic!("vanilla should contain at least one wrapping trade route");
        };
        let d = derive_geometry(&vfs, &from, &to).unwrap();
        // Every draft point stays on the map...
        for p in &d.control {
            assert!(p[0] >= 0.0 && p[0] < wf, "control x {} out of [0,{wf})", p[0]);
        }
        // ...and the draft goes the SHORT way: total wrap-aware x-travel stays
        // under half the map (the pre-fix literal lerp would exceed it here).
        let travel: f64 = d
            .control
            .windows(2)
            .map(|p| wrap_dx(p[0][0], p[1][0], wf).abs())
            .sum();
        assert!(
            travel < wf / 2.0,
            "draft {from}->{to} should wrap the short way (x-travel {travel} vs map width {wf})"
        );
        println!(
            "[tradenodes] wrap draft {from}->{to}: x-travel {travel:.0}px, {} path provs",
            d.path.len()
        );
    }

    // --- re-derive path from the edited control curve --------------------

    #[test]
    fn rederive_straight_control_matches_geometry() {
        let Some(vfs) = real_install() else { return };
        // Straight node-to-node draft (the old behavior).
        let straight = derive_geometry(&vfs, "genua", "venice").unwrap();
        // Feeding that straight draft's own (top-left) control back through the
        // curve-based re-derivation reproduces the same water provinces — the
        // new path collector reduces to the straight line for straight control.
        let raw = parse_raw_nodes(&vfs);
        let loc = |k: &str| raw.iter().find(|n| n.key == k).unwrap().location;
        let path =
            path_under_control(&vfs, &straight.control, loc("genua"), loc("venice")).unwrap();
        let got: std::collections::HashSet<u32> = path.into_iter().collect();
        let want: std::collections::HashSet<u32> = straight.path.iter().copied().collect();
        assert_eq!(got, want, "straight control reproduces the straight-line derive");
    }

    #[test]
    fn rederive_reshaped_control_changes_path() {
        let Some(vfs) = real_install() else { return };
        let loc_store = crate::loc::store(&vfs, INSTALL, None);
        let (w, h) = map_dimensions(&vfs).unwrap();
        let net = load_network(&vfs, &loc_store, w, h);
        // Decode the id buffer + water set ONCE, then reuse across candidates.
        let buf = id_buffer(&vfs).unwrap();
        let water = water_ids(&vfs);
        let loc = |k: &str| net.nodes.iter().find(|n| n.key == k).and_then(|n| n.location);

        // For at least one hand-authored (curved) vanilla route, the water
        // provinces UNDER the curve differ from the straight line between the
        // route's own endpoints — proving the edited control points drive the
        // derivation rather than a straight node-to-node line.
        let mut found: Option<(String, String, usize, usize)> = None;
        for node in &net.nodes {
            for o in &node.outgoing {
                if o.control.len() < 3 {
                    continue;
                }
                let ends = [o.control[0], *o.control.last().unwrap()];
                let from = loc(&node.key);
                let to = loc(&o.target);
                let straight = path_under_control_buf(&buf, &water, &ends, from, to);
                let curved = path_under_control_buf(&buf, &water, &o.control, from, to);
                if straight != curved {
                    found = Some((node.key.clone(), o.target.clone(), straight.len(), curved.len()));
                    break;
                }
            }
            if found.is_some() {
                break;
            }
        }
        let (from, to, sn, cn) = found.expect(
            "a curved vanilla route should derive a different path than its straight line",
        );
        println!("[tradenodes] reshape {from}->{to}: straight {sn} provs vs curved {cn} provs");
    }

    #[test]
    fn rederive_wrapping_route_stays_short() {
        let Some(vfs) = real_install() else { return };
        let loc_store = crate::loc::store(&vfs, INSTALL, None);
        let (w, h) = map_dimensions(&vfs).unwrap();
        let net = load_network(&vfs, &loc_store, w, h);
        let wf = w as f64;
        // A vanilla route that wraps the antimeridian (a control-x jump wider
        // than half the map).
        let wrapping = net
            .nodes
            .iter()
            .find_map(|n| {
                n.outgoing.iter().find_map(|o| {
                    o.control
                        .windows(2)
                        .any(|p| (p[1][0] - p[0][0]).abs() > wf / 2.0)
                        .then(|| (n.key.clone(), o.clone()))
                })
            })
            .expect("a wrapping vanilla route");
        let (from, o) = wrapping;
        let loc = |k: &str| net.nodes.iter().find(|n| n.key == k).and_then(|n| n.location);
        let path =
            path_under_control(&vfs, &o.control, loc(&from), loc(&o.target)).unwrap();
        assert!(!path.is_empty(), "wrapping route should derive a non-empty path");
        // Short way: unwrapping the control keeps total horizontal extent under
        // the map width — a long-way trace across the whole map would exceed it.
        let un = unwrap_control(&o.control, wf);
        let travel: f64 = un.windows(2).map(|p| (p[1][0] - p[0][0]).abs()).sum();
        assert!(
            travel < wf,
            "wrapping control should unwrap the short way (x-travel {travel} vs map width {wf})"
        );
        println!(
            "[tradenodes] wrap re-derive {from}->{}: {} path provs, unwrapped x-travel {travel:.0}px",
            o.target,
            path.len()
        );
    }

    #[test]
    fn spline_helpers_match_frontend() {
        // Straight (≤2 points) is the segment verbatim.
        assert_eq!(
            sample_polyline(&[[0.0, 0.0], [10.0, 0.0]], 4),
            vec![[0.0, 0.0], [10.0, 0.0]]
        );
        // Catmull-Rom passes through its control points (t=0 → p1).
        let pts = [[0.0, 0.0], [10.0, 0.0], [20.0, 10.0], [30.0, 10.0]];
        let s = sample_polyline(&pts, 8);
        assert_eq!(s.first(), Some(&[0.0, 0.0]));
        assert_eq!(s.last(), Some(&[30.0, 10.0]));
        // unwrap_control shifts a wrapped x to sit next to its predecessor.
        let un = unwrap_control(&[[10.0, 0.0], [990.0, 0.0]], 1000.0);
        assert_eq!(un[1][0], -10.0, "990 unwraps to -10 next to 10 (short way)");
    }

    // --- Anbennar smoke --------------------------------------------------

    #[test]
    fn anbennar_network_parses() {
        if !Path::new(INSTALL).join("map/provinces.bmp").is_file() || !Path::new(ANBENNAR).is_dir()
        {
            return;
        }
        let vfs = Vfs::new(INSTALL, Some(ANBENNAR)).unwrap();
        let loc = crate::loc::store(&vfs, INSTALL, Some(ANBENNAR));
        let (w, h) = map_dimensions(&vfs).unwrap();
        let net = load_network(&vfs, &loc, w, h);
        let routes: usize = net.nodes.iter().map(|n| n.outgoing.len()).sum();
        let ends = net.nodes.iter().filter(|n| n.end).count();
        println!(
            "[tradenodes:anbennar] {} nodes, {} routes, {} end nodes ({}x{})",
            net.nodes.len(),
            routes,
            ends,
            w,
            h
        );
        assert!(net.nodes.len() > 0, "anbennar should have trade nodes");
    }
}
