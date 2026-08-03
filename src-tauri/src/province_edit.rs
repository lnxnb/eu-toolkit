//! Province Colors map-mode structural editing: add / expand / dissolve
//! provinces by **rewriting `map/provinces.bmp` pixels**.
//!
//! This is the first *binary raster* write in the toolkit. Every other edit is
//! byte-surgical text (Clausewitz) or line-surgical CSV; a province's identity,
//! though, is a region of same-colored pixels in `provinces.bmp`, so adding,
//! expanding, or removing a province fundamentally means **reassigning pixels**.
//! A BMP write can't be surgical — it's a whole-file re-encode — so this module
//! keeps the *edit* semantic and color-space, and only materializes the 34 MB
//! bitmap once, at save time, inside [`apply_ops`].
//!
//! ## Edit model ([`BmpOp`], color-space)
//!
//! The frontend never ships the whole bitmap. It ships small, deterministic ops
//! that [`apply_ops`] replays against the base bitmap decoded through the Vfs:
//!
//! - [`BmpOp::Paint`] — a brush stroke: the listed pixels (top-down `y*w + x`
//!   flat indices, matching the province-id buffer and `province_colors` PNG the
//!   frontend already hit-tests against) become one RGB color. This is how a new
//!   province is *carved* (paint a freshly-allocated color) and how an existing
//!   province is *expanded* (paint its own color over a neighbor).
//! - [`BmpOp::Dissolve`] — remove a province by distributing every pixel of its
//!   color among one or more **target** colors, each dissolved pixel going to the
//!   geodesically nearest target via a multi-source BFS over the dissolved
//!   region. One target = a plain merge; several = the "divide it between the
//!   neighbours" split.
//!
//! Ops apply in order on the evolving pixel buffer, so a Paint that carves a new
//! province and a later Dissolve compose exactly like the text edit queue.
//!
//! ## BMP format
//!
//! Decoding reuses the `image` crate (it already reads EU4's 108-byte
//! BITMAPV4HEADER `provinces.bmp`). Encoding is hand-rolled to a plain 24-bit,
//! bottom-up, BI_RGB BMP with a 54-byte header — the format every province-map
//! modding tool emits and the engine loads — so the output is predictable rather
//! than at the mercy of the encoder's header choices. (Whether a given EU4 build
//! accepts the re-encoded file is the one thing only a real launch confirms.)

use std::collections::VecDeque;

/// One color-space operation on the province bitmap. Mirrored 1:1 by the
/// frontend; serialized internally-tagged on `op` with camelCase field names
/// (`{ "op": "paint", "pixels": [...], "color": [r,g,b] }`).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "op", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum BmpOp {
    /// Set every listed pixel (top-down flat index `y*width + x`) to `color`.
    /// Out-of-range indices are ignored (a stale brush over a resized map can't
    /// corrupt neighbouring rows).
    Paint { pixels: Vec<u32>, color: [u8; 3] },
    /// Reassign every pixel currently equal to `from` among the `into` colors,
    /// each dissolved pixel taking the nearest target's color (multi-source BFS
    /// through the dissolved region, 4-connected with horizontal map wrap). A
    /// dissolved region that no target borders falls back to `into[0]` so the
    /// province's color never partially survives.
    Dissolve { from: [u8; 3], into: Vec<[u8; 3]> },
}

/// Decoded pixel buffer + dimensions. `rgb` is top-down (row 0 = top), 3 bytes
/// per pixel — the same orientation `image`'s `to_rgb8` yields and the renderer's
/// id buffer uses.
struct Bitmap {
    width: usize,
    height: usize,
    rgb: Vec<u8>,
}

fn decode(base: &[u8]) -> Result<Bitmap, String> {
    let img = image::load_from_memory(base)
        .map_err(|e| format!("Failed to decode provinces.bmp: {e}"))?
        .to_rgb8();
    let (w, h) = img.dimensions();
    Ok(Bitmap {
        width: w as usize,
        height: h as usize,
        rgb: img.into_raw(),
    })
}

/// Applies `ops` to the province bitmap `base` (raw BMP bytes) and re-encodes to
/// a 24-bit bottom-up BMP. Used by the edit queue at save time and by the
/// pending-preview path.
pub fn apply_ops(base: &[u8], ops: &[BmpOp]) -> Result<Vec<u8>, String> {
    let mut bmp = decode(base)?;
    for op in ops {
        match op {
            BmpOp::Paint { pixels, color } => paint(&mut bmp, pixels, *color),
            BmpOp::Dissolve { from, into } => dissolve(&mut bmp, *from, into)?,
        }
    }
    Ok(encode_bmp24(&bmp.rgb, bmp.width, bmp.height))
}

fn color_at(bmp: &Bitmap, idx: usize) -> [u8; 3] {
    let o = idx * 3;
    [bmp.rgb[o], bmp.rgb[o + 1], bmp.rgb[o + 2]]
}

fn set_color(bmp: &mut Bitmap, idx: usize, c: [u8; 3]) {
    let o = idx * 3;
    bmp.rgb[o] = c[0];
    bmp.rgb[o + 1] = c[1];
    bmp.rgb[o + 2] = c[2];
}

fn paint(bmp: &mut Bitmap, pixels: &[u32], color: [u8; 3]) {
    let n = bmp.width * bmp.height;
    for &p in pixels {
        let idx = p as usize;
        if idx < n {
            set_color(bmp, idx, color);
        }
    }
}

/// 4-connected neighbours of `idx`, horizontal wrap honored (the map wraps at the
/// antimeridian), vertical clamped.
fn neighbors(bmp: &Bitmap, idx: usize, out: &mut [usize; 4]) -> usize {
    let w = bmp.width;
    let x = idx % w;
    let y = idx / w;
    let mut n = 0;
    // left / right with wrap
    out[n] = if x == 0 { idx + w - 1 } else { idx - 1 };
    n += 1;
    out[n] = if x + 1 == w { idx + 1 - w } else { idx + 1 };
    n += 1;
    if y > 0 {
        out[n] = idx - w;
        n += 1;
    }
    if y + 1 < bmp.height {
        out[n] = idx + w;
        n += 1;
    }
    n
}

fn dissolve(bmp: &mut Bitmap, from: [u8; 3], into: &[[u8; 3]]) -> Result<(), String> {
    if into.is_empty() {
        return Err("dissolve needs at least one target province".to_string());
    }
    let n = bmp.width * bmp.height;
    // Collect the dissolved region once (cheap relative to the whole map).
    let from_pixels: Vec<usize> = (0..n).filter(|&i| color_at(bmp, i) == from).collect();
    if from_pixels.is_empty() {
        return Ok(());
    }
    let target_set: Vec<[u8; 3]> = into.to_vec();
    let is_target = |c: [u8; 3]| target_set.contains(&c);

    // Multi-source BFS: seed each from-pixel that borders a target with that
    // target's color, then flood the assignment through the from-region. The
    // buffer is NOT mutated during traversal (assignments live in `assigned`),
    // so region topology stays stable and each pixel resolves to its nearest
    // target by geodesic distance.
    let mut assigned: std::collections::HashMap<usize, [u8; 3]> = std::collections::HashMap::new();
    let mut q: VecDeque<usize> = VecDeque::new();
    let mut buf = [0usize; 4];
    for &p in &from_pixels {
        let cnt = neighbors(bmp, p, &mut buf);
        for &nb in &buf[..cnt] {
            let c = color_at(bmp, nb);
            if is_target(c) {
                assigned.entry(p).or_insert(c);
            }
        }
        if assigned.contains_key(&p) {
            q.push_back(p);
        }
    }
    while let Some(p) = q.pop_front() {
        let c = assigned[&p];
        let cnt = neighbors(bmp, p, &mut buf);
        for &nb in &buf[..cnt] {
            if color_at(bmp, nb) == from && !assigned.contains_key(&nb) {
                assigned.insert(nb, c);
                q.push_back(nb);
            }
        }
    }

    // Apply, with a fallback for any pixel no target could reach (a fully
    // enclosed region whose selected targets don't border it).
    let fallback = target_set[0];
    for &p in &from_pixels {
        set_color(bmp, p, *assigned.get(&p).unwrap_or(&fallback));
    }
    Ok(())
}

/// Encodes a top-down RGB buffer to a plain 24-bit, bottom-up, BI_RGB BMP
/// (54-byte BITMAPFILEHEADER + BITMAPINFOHEADER). Rows are padded to a 4-byte
/// boundary and stored bottom-up in BGR order.
fn encode_bmp24(rgb: &[u8], width: usize, height: usize) -> Vec<u8> {
    let row_raw = width * 3;
    let pad = (4 - (row_raw % 4)) % 4;
    let stride = row_raw + pad;
    let pixel_bytes = stride * height;
    let file_size = 54 + pixel_bytes;

    let mut out = Vec::with_capacity(file_size);
    // BITMAPFILEHEADER (14 bytes)
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&(file_size as u32).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // reserved
    out.extend_from_slice(&54u32.to_le_bytes()); // pixel data offset
    // BITMAPINFOHEADER (40 bytes)
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(width as i32).to_le_bytes());
    out.extend_from_slice(&(height as i32).to_le_bytes()); // positive => bottom-up
    out.extend_from_slice(&1u16.to_le_bytes()); // planes
    out.extend_from_slice(&24u16.to_le_bytes()); // bits per pixel
    out.extend_from_slice(&0u32.to_le_bytes()); // BI_RGB
    out.extend_from_slice(&(pixel_bytes as u32).to_le_bytes());
    out.extend_from_slice(&2835i32.to_le_bytes()); // x pixels-per-meter (~72dpi)
    out.extend_from_slice(&2835i32.to_le_bytes()); // y pixels-per-meter
    out.extend_from_slice(&0u32.to_le_bytes()); // colors used
    out.extend_from_slice(&0u32.to_le_bytes()); // colors important

    let padding = [0u8; 3];
    for y in (0..height).rev() {
        let row = &rgb[y * row_raw..y * row_raw + row_raw];
        for px in row.chunks_exact(3) {
            out.push(px[2]); // B
            out.push(px[1]); // G
            out.push(px[0]); // R
        }
        out.extend_from_slice(&padding[..pad]);
    }
    out
}

// ---------------------------------------------------------------------------
// Add-province scaffold (the text cascade that accompanies the Paint op)
// ---------------------------------------------------------------------------

use crate::edits::TypedEdit;
use crate::vfs::Vfs;

/// Everything a freshly-carved province needs, returned to the frontend as one
/// undo composite. `edits` is wire-ready `TypedEdit`s (the Paint op plus the
/// definition.csv / default.map / area.txt / history / loc cascade).
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvinceScaffold {
    pub id: u32,
    pub color: [u8; 3],
    pub name: String,
    pub area: String,
    pub edits: Vec<TypedEdit>,
}

/// Allocates a province id + a distinct RGB color and builds the add-province
/// composite: paint the carved `pixels` with the new color, register the
/// definition.csv row, bump `default.map` `max_provinces` if needed, join the
/// source province's `area`, create a minimal history file, and localise
/// `PROV<id>`. The new province inherits the source province's culture/religion/
/// trade_goods so it loads as a sensible uncolonized tile.
///
/// `positions.txt` / `continent.txt` are deliberately NOT emitted here: a
/// province loads without them (it just has no city graphic and no continent),
/// and both are known follow-ups.
/// Tauri command wrapper: builds a session Vfs and delegates to
/// [`prepare_add_province`].
#[tauri::command(async)]
pub fn add_province_scaffold(
    install_path: String,
    mod_path: Option<String>,
    pixels: Vec<u32>,
    name: String,
    source_id: u32,
) -> Result<ProvinceScaffold, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    prepare_add_province(&vfs, pixels, &name, source_id)
}

pub fn prepare_add_province(
    vfs: &Vfs,
    pixels: Vec<u32>,
    name: &str,
    source_id: u32,
) -> Result<ProvinceScaffold, String> {
    if pixels.is_empty() {
        return Err("No pixels were carved for the new province".to_string());
    }
    let name = name.trim();
    if name.is_empty() {
        return Err("The new province needs a name".to_string());
    }

    // --- id + color, distinct from every existing province. --------------
    let defs = crate::game_data::province_definitions(vfs);
    if defs.is_empty() {
        return Err("map/definition.csv has no province definitions".to_string());
    }
    let new_id = defs.iter().map(|(id, _, _)| *id).max().unwrap_or(0) + 1;
    let used: std::collections::HashSet<[u8; 3]> = defs.iter().map(|(_, c, _)| *c).collect();
    let color = distinct_color(&used, new_id);

    // --- Area: the source province's area (a province with no area errors on
    //     load, so the carve must inherit one). ----------------------------
    let area = crate::game_data::areas(vfs)
        .into_iter()
        .find(|(_, ids)| ids.contains(&source_id))
        .map(|(name, _)| name)
        .ok_or_else(|| {
            format!("Source province {source_id} is not in any area; carve from a province that belongs to an area")
        })?;

    // --- Inherit culture/religion/trade_goods from the source province. --
    let source_state = crate::game_data::province_history_at(vfs, crate::date::DEFAULT_START)
        .into_iter()
        .find(|(id, _)| *id == source_id)
        .map(|(_, s)| s);

    let stem = safe_stem(name);
    let history_file = format!("history/provinces/{new_id} - {stem}.txt");
    let [r, g, b] = color;

    let mut edits: Vec<TypedEdit> = Vec::new();

    // 1. Paint the carved pixels with the new color.
    edits.push(TypedEdit::ProvinceBmp {
        file: "map/provinces.bmp".to_string(),
        ops: vec![BmpOp::Paint {
            pixels,
            color,
        }],
    });

    // 2. definition.csv row (`id;r;g;b;name;x`), newline-safe against a file that
    //    may not end in a newline.
    let raw = vfs.read("map/definition.csv").unwrap_or_default();
    let lead = if raw.last().is_some_and(|&c| c != b'\n') { "\n" } else { "" };
    edits.push(TypedEdit::AppendText {
        file: "map/definition.csv".to_string(),
        text: format!("{lead}{new_id};{r};{g};{b};{stem};x\n"),
    });

    // 3. default.map: bump max_provinces to id+1 if the current ceiling is lower.
    if let Some(max) = default_map_max(vfs) {
        if new_id + 1 > max {
            edits.push(TypedEdit::SetScalar {
                file: "map/default.map".to_string(),
                path: vec!["max_provinces".to_string()],
                value: (new_id + 1).to_string(),
                quoted: false,
            });
        }
    }

    // 4. Area membership (bare-id list under the area block; same shape as the
    //    geography province->area move).
    edits.push(TypedEdit::AddId {
        file: "map/area.txt".to_string(),
        list_path: vec![area.clone()],
        id: new_id.to_string(),
    });

    // 5. Minimal history file inheriting the source's culture/religion/goods.
    edits.push(TypedEdit::CreateFile {
        file: history_file,
        text: history_text(name, source_state.as_ref()),
    });

    // 6. Localise PROV<id>.
    edits.push(TypedEdit::LocOverride {
        key: format!("PROV{new_id}"),
        value: name.to_string(),
    });

    Ok(ProvinceScaffold {
        id: new_id,
        color,
        name: name.to_string(),
        area,
        edits,
    })
}

/// A stable RGB color not already used by any province. Walks a fixed additive
/// sequence over the 24-bit space seeded by the new id, so the choice is
/// deterministic (tests) and collision-free against `used`.
fn distinct_color(used: &std::collections::HashSet<[u8; 3]>, seed: u32) -> [u8; 3] {
    let mut v = seed.wrapping_mul(2_654_435_761).wrapping_add(1) & 0xFF_FFFF;
    for _ in 0..0x100_0000u32 {
        let c = [(v >> 16) as u8, (v >> 8) as u8, v as u8];
        if !used.contains(&c) {
            return c;
        }
        v = v.wrapping_add(2_654_435_761) & 0xFF_FFFF;
    }
    [seed as u8, (seed >> 8) as u8, (seed >> 16) as u8]
}

/// A filesystem/CSV-safe stem for the province name (ASCII letters/digits/space/
/// hyphen; collapses others to nothing). Latin-1 names round-trip fine in
/// definition.csv, but keeping the stem ASCII keeps the created filename portable.
fn safe_stem(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == ' ' || c == '-' { c } else { ' ' })
        .collect();
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if s.is_empty() { "Province".to_string() } else { s }
}

fn default_map_max(vfs: &Vfs) -> Option<u32> {
    let bytes = vfs.read("map/default.map").ok()?;
    let block = crate::paradox::parse(&String::from_utf8_lossy(&bytes));
    block.get_scalar("max_provinces").and_then(|s| s.trim().parse().ok())
}

fn history_text(name: &str, source: Option<&crate::game_data::ProvinceState>) -> String {
    let mut out = format!("# {name} - added by EU Toolkit\n");
    if let Some(s) = source {
        if let Some(c) = &s.culture {
            out.push_str(&format!("culture = {c}\n"));
        }
        if let Some(r) = &s.religion {
            out.push_str(&format!("religion = {r}\n"));
        }
        if let Some(g) = &s.trade_goods {
            out.push_str(&format!("trade_goods = {g}\n"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Decodes BMP bytes back to a top-down RGB buffer for assertions.
    fn decode_rgb(bytes: &[u8]) -> (usize, usize, Vec<u8>) {
        let b = decode(bytes).unwrap();
        (b.width, b.height, b.rgb)
    }

    /// Builds a small BMP from a top-down RGB buffer via the module's encoder.
    fn make_bmp(width: usize, height: usize, rgb: &[u8]) -> Vec<u8> {
        encode_bmp24(rgb, width, height)
    }

    #[test]
    fn encode_decode_round_trips_with_padding() {
        // 3px-wide rows => 9 raw bytes => padded to 12; exercises row padding.
        let w = 3;
        let h = 2;
        let rgb: Vec<u8> = (0..(w * h * 3) as u8).collect();
        let bmp = make_bmp(w, h, &rgb);
        let (dw, dh, back) = decode_rgb(&bmp);
        assert_eq!((dw, dh), (w, h));
        assert_eq!(back, rgb, "pixels survive encode->decode byte-for-byte");
    }

    #[test]
    fn apply_no_ops_is_pixel_identity() {
        let w = 4;
        let h = 4;
        let rgb: Vec<u8> = (0..(w * h * 3)).map(|i| (i % 256) as u8).collect();
        let bmp = make_bmp(w, h, &rgb);
        let out = apply_ops(&bmp, &[]).unwrap();
        let (_, _, back) = decode_rgb(&out);
        assert_eq!(back, rgb);
    }

    #[test]
    fn paint_sets_listed_pixels_only() {
        let w = 2;
        let h = 2;
        // Four distinct colors.
        let rgb = vec![
            10, 10, 10, // idx 0
            20, 20, 20, // idx 1
            30, 30, 30, // idx 2
            40, 40, 40, // idx 3
        ];
        let bmp = make_bmp(w, h, &rgb);
        let out = apply_ops(
            &bmp,
            &[BmpOp::Paint {
                pixels: vec![1, 2],
                color: [99, 88, 77],
            }],
        )
        .unwrap();
        let (_, _, back) = decode_rgb(&out);
        assert_eq!(&back[0..3], &[10, 10, 10]); // untouched
        assert_eq!(&back[3..6], &[99, 88, 77]); // painted
        assert_eq!(&back[6..9], &[99, 88, 77]); // painted
        assert_eq!(&back[9..12], &[40, 40, 40]); // untouched
    }

    #[test]
    fn paint_ignores_out_of_range_indices() {
        let bmp = make_bmp(2, 1, &[1, 1, 1, 2, 2, 2]);
        // idx 5 is well past the 2-pixel buffer; must be a no-op, not a panic.
        let out = apply_ops(
            &bmp,
            &[BmpOp::Paint {
                pixels: vec![0, 5],
                color: [7, 7, 7],
            }],
        )
        .unwrap();
        let (_, _, back) = decode_rgb(&out);
        assert_eq!(&back[0..3], &[7, 7, 7]);
        assert_eq!(&back[3..6], &[2, 2, 2]);
    }

    #[test]
    fn dissolve_divides_region_between_two_neighbors() {
        // Row of 4: [A, X, X, B]. Dissolving X among {A,B} sends the left X to A
        // (borders A) and the right X to B (borders B).
        let a = [1u8, 0, 0];
        let x = [9u8, 9, 9];
        let b = [0u8, 0, 1];
        let rgb = vec![
            a[0], a[1], a[2], //
            x[0], x[1], x[2], //
            x[0], x[1], x[2], //
            b[0], b[1], b[2],
        ];
        let bmp = make_bmp(4, 1, &rgb);
        let out = apply_ops(
            &bmp,
            &[BmpOp::Dissolve {
                from: x,
                into: vec![a, b],
            }],
        )
        .unwrap();
        let (_, _, back) = decode_rgb(&out);
        assert_eq!(&back[0..3], &a);
        assert_eq!(&back[3..6], &a, "left X nearest to A");
        assert_eq!(&back[6..9], &b, "right X nearest to B");
        assert_eq!(&back[9..12], &b);
        // The dissolved color is gone entirely.
        assert!(!back.chunks_exact(3).any(|c| c == x));
    }

    #[test]
    fn dissolve_single_target_is_a_plain_merge() {
        let a = [1u8, 2, 3];
        let x = [9u8, 9, 9];
        let rgb = vec![a[0], a[1], a[2], x[0], x[1], x[2], x[0], x[1], x[2]];
        let bmp = make_bmp(3, 1, &rgb);
        let out = apply_ops(
            &bmp,
            &[BmpOp::Dissolve {
                from: x,
                into: vec![a],
            }],
        )
        .unwrap();
        let (_, _, back) = decode_rgb(&out);
        assert!(back.chunks_exact(3).all(|c| c == a));
    }

    #[test]
    fn dissolve_enclosed_region_falls_back_to_first_target() {
        // X borders no target (surrounded only by a non-target C). It must still
        // be fully cleared to into[0] rather than partially surviving.
        let x = [9u8, 9, 9];
        let c = [5u8, 5, 5];
        let t = [1u8, 1, 1];
        let rgb = vec![c[0], c[1], c[2], x[0], x[1], x[2], c[0], c[1], c[2]];
        let bmp = make_bmp(3, 1, &rgb);
        let out = apply_ops(
            &bmp,
            &[BmpOp::Dissolve {
                from: x,
                into: vec![t],
            }],
        )
        .unwrap();
        let (_, _, back) = decode_rgb(&out);
        assert_eq!(&back[3..6], &t, "unreachable region falls back to into[0]");
        assert!(!back.chunks_exact(3).any(|col| col == x));
    }

    fn write(base: &std::path::Path, rel: &str, bytes: &[u8]) {
        let p = base.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, bytes).unwrap();
    }

    #[test]
    fn add_province_scaffold_builds_and_applies() {
        use std::io::Cursor;
        let root = std::env::temp_dir().join("eu_toolkit_province_edit_scaffold");
        let base = root.join("base");
        let project = root.join("project");
        let _ = std::fs::remove_dir_all(&root);

        // A 2x2 base bitmap; two defined provinces (colors match the pixels).
        let mut img = image::RgbImage::new(2, 2);
        img.put_pixel(0, 0, image::Rgb([1, 1, 1])); // prov 1
        img.put_pixel(1, 0, image::Rgb([2, 2, 2])); // prov 2
        img.put_pixel(0, 1, image::Rgb([1, 1, 1]));
        img.put_pixel(1, 1, image::Rgb([2, 2, 2]));
        let mut bmp = Vec::new();
        img.write_to(&mut Cursor::new(&mut bmp), image::ImageFormat::Bmp).unwrap();
        write(&base, "map/provinces.bmp", &bmp);
        // definition.csv WITHOUT a trailing newline (the real file's shape).
        write(&base, "map/definition.csv", b"province;red;green;blue;x;x\n1;1;1;1;Alpha;x\n2;2;2;2;Beta;x");
        write(&base, "map/default.map", b"max_provinces = 3\nsea_starts = { }\nlakes = { }\n");
        write(&base, "map/area.txt", b"test_area = {\n\t1 2\n}\n");
        write(&base, "history/provinces/1 - Alpha.txt", b"culture = swedish\nreligion = catholic\ntrade_goods = grain\n");

        let vfs = Vfs::new(base.to_str().unwrap(), None).unwrap();
        // Carve pixel (1,1) [top-down idx 3] out of province 2 into a new province.
        let scaffold = prepare_add_province(&vfs, vec![3], "New Land", 1).unwrap();

        assert_eq!(scaffold.id, 3, "id = max defined + 1");
        assert_eq!(scaffold.area, "test_area");
        assert_ne!(scaffold.color, [1, 1, 1]);
        assert_ne!(scaffold.color, [2, 2, 2]);

        // The composite applies cleanly through the real queue.
        crate::edits::apply_queue(&vfs, &project, &scaffold.edits).unwrap();

        // definition.csv gained a well-formed row on its own line.
        let defs = String::from_utf8(std::fs::read(project.join("map/definition.csv")).unwrap()).unwrap();
        let [r, g, b] = scaffold.color;
        assert!(defs.contains(&format!("\n3;{r};{g};{b};New Land;x\n")), "row appended: {defs:?}");
        assert!(defs.contains("2;2;2;2;Beta;x"), "existing rows intact");

        // default.map ceiling bumped to id+1.
        let dm = String::from_utf8(std::fs::read(project.join("map/default.map")).unwrap()).unwrap();
        assert!(dm.contains("max_provinces = 4"), "max bumped: {dm}");

        // Area gained the new id.
        let area = String::from_utf8(std::fs::read(project.join("map/area.txt")).unwrap()).unwrap();
        assert!(area.contains('3'), "new id joined the area: {area}");

        // History file created, inheriting the source's culture/religion/goods.
        let hist = String::from_utf8(std::fs::read(project.join("history/provinces/3 - New Land.txt")).unwrap()).unwrap();
        assert!(hist.contains("culture = swedish"));
        assert!(hist.contains("religion = catholic"));
        assert!(hist.contains("trade_goods = grain"));

        // The bitmap repainted the carved pixel to the new color; neighbours intact.
        let out = image::load_from_memory(&std::fs::read(project.join("map/provinces.bmp")).unwrap())
            .unwrap()
            .to_rgb8();
        assert_eq!(out.get_pixel(1, 1).0, scaffold.color);
        assert_eq!(out.get_pixel(0, 0).0, [1, 1, 1]);
        assert_eq!(out.get_pixel(1, 0).0, [2, 2, 2]);

        // Loc override for PROV3.
        let loc = String::from_utf8(std::fs::read(project.join(crate::loc::OVERRIDE_REL)).unwrap()).unwrap();
        assert!(loc.contains("PROV3"));
    }

    #[test]
    fn ops_compose_in_order() {
        // Carve a new color into pixel 1, then dissolve it back into pixel 0's
        // color — the second op sees the first op's result.
        let a = [2u8, 2, 2];
        let rgb = vec![a[0], a[1], a[2], 8, 8, 8];
        let bmp = make_bmp(2, 1, &rgb);
        let new = [50u8, 60, 70];
        let out = apply_ops(
            &bmp,
            &[
                BmpOp::Paint {
                    pixels: vec![1],
                    color: new,
                },
                BmpOp::Dissolve {
                    from: new,
                    into: vec![a],
                },
            ],
        )
        .unwrap();
        let (_, _, back) = decode_rgb(&out);
        assert!(back.chunks_exact(3).all(|c| c == a));
    }
}
