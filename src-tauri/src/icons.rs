//! Phase 0.7 — icon/atlas pipeline: sprite strips served as PNG + key→index maps.
//!
//! The game ships its trade-good and religion icons as horizontal sprite strips
//! (`gfx/interface/resources.dds`, `gfx/interface/icon_religion.dds`) and its
//! development-component icons as individual files
//! (`gfx/interface/development_button_base_{tax,production,manpower}.dds`). All of
//! these are **uncompressed 32-bit BGRA** DDS textures (fourCC = 0, standard
//! A8R8G8B8 masks) — no block-compression decoder is needed, so we decode them in
//! pure Rust here (the `image` crate build has no `dds` feature; see Cargo.toml).
//!
//! ## Wire shape (one command, `get_icon_atlas(kind)`)
//! `[u32 header_len LE][header JSON (UTF-8)][PNG bytes]`, matching the layout used
//! by `get_mode_data`. The header is:
//! ```json
//! { "kind": "...", "frameW": 64, "frameH": 64, "count": 32,
//!   "index": { "grain": 0, "wine": 1, ... } }
//! ```
//! * `count` — number of frames in the PNG strip (frames are laid out left→right
//!   in a single row, each `frameW × frameH`).
//! * `index` — the **key → frame index** map. A trade good / religion / dev
//!   component's icon is frame `index[key]` of the strip. The frontend overlay
//!   slices frame `i` from the strip at `x = i * frameW`.
//!
//! The PNG is RGBA (alpha matters — icons are transparent outside their glyph),
//! so it is encoded here directly rather than through `map_renderer::encode_png`
//! (which is RGB-only).
//!
//! ## Key → index construction (definition-order proof)
//! * **Trade goods** (`kind = "trade_goods"`): the strip is *positionally indexed
//!   by the trade-good definition order* (AGENTS.md gotcha) — good `i` in
//!   `common/tradegoods` uses frame `i`. So `index` is the top-level `key_blocks`
//!   of `common/tradegoods` enumerated in file order: grain→0, wine→1, …
//! * **Religions** (`kind = "religions"`): each religion block carries an explicit
//!   `icon = N` field (1-based) that *is* the strip frame. We map religion key →
//!   `N - 1`. This is the game's own indexing and is authoritative — it is NOT
//!   plain definition order (e.g. `catholic` = icon 1 → frame 0, but later groups
//!   interleave icon numbers). Any group child without an `icon` scalar
//!   (`flag_emblem_index_range`, `religious_schools`, …) is skipped.
//! * **Development** (`kind = "development"`): the three component icons are
//!   stitched into a synthetic 3-frame strip; keys are the province-history dev
//!   keys `base_tax`→0, `base_production`→1, `base_manpower`→2.
//!
//! All reads go through the [`Vfs`] so a mod may override any strip; the index is
//! rebuilt from the mod-overlaid `common/` data too.
//!
//! ## Known limitation — block-compressed mod strips
//! A total conversion may ship a **block-compressed** strip (Anbennar replaces
//! `resources.dds` with a DX10/BC7 texture). We decode only uncompressed 32-bpp
//! DDS in pure Rust; adding a BCn/DX10 decompressor is deliberately out of scope
//! for 0.7. When the mod's strip is compressed we fall back to the **base game's**
//! strip PNG while still using the mod-aware key→index map, so vanilla-derived
//! icons render correctly and any mod-added frames beyond the base strip simply
//! read out of range (the documented positional-indexing gotcha). A real BCn
//! decoder is a future task.

use image::{ExtendedColorType, ImageEncoder};

use crate::gfx;
use crate::paradox::Block;
use crate::vfs::Vfs;

/// A decoded uncompressed DDS surface as tightly-packed RGBA8.
struct DdsImage {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

/// Decodes an **uncompressed 32-bpp** DDS (the only kind EU4 uses for these
/// strips). Returns an error for block-compressed (DXT/BCn) textures rather than
/// silently producing garbage — callers document the requirement.
fn decode_dds(bytes: &[u8]) -> Result<DdsImage, String> {
    if bytes.len() < 128 || &bytes[0..4] != b"DDS " {
        return Err("not a DDS file (bad magic)".into());
    }
    let rd = |o: usize| u32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
    if rd(4) != 124 {
        return Err("unexpected DDS header size".into());
    }
    let height = rd(12);
    let width = rd(16);

    // Pixel format sub-header starts at offset 76.
    const DDPF_FOURCC: u32 = 0x4;
    let pf_flags = rd(80);
    let four_cc = rd(84);
    let rgb_bit_count = rd(88);
    let r_mask = rd(92);
    let g_mask = rd(96);
    let b_mask = rd(100);
    let a_mask = rd(104);

    if four_cc != 0 || (pf_flags & DDPF_FOURCC) != 0 {
        let tag = four_cc.to_le_bytes();
        return Err(format!(
            "compressed DDS unsupported (fourCC {:?}); need an uncompressed strip",
            String::from_utf8_lossy(&tag)
        ));
    }
    if rgb_bit_count != 32 {
        return Err(format!("unsupported DDS bit depth {rgb_bit_count} (need 32)"));
    }

    let n = (width as usize) * (height as usize);
    let data = &bytes[128..];
    if data.len() < n * 4 {
        return Err("truncated DDS pixel data".into());
    }

    // Extract each channel by its bitmask (handles BGRA and any 8-bit-channel
    // 32-bpp layout the game might use).
    let shift = |mask: u32| if mask == 0 { 0 } else { mask.trailing_zeros() };
    let (rs, gs, bs, as_) = (shift(r_mask), shift(g_mask), shift(b_mask), shift(a_mask));
    let mut rgba = vec![0u8; n * 4];
    for i in 0..n {
        let px = u32::from_le_bytes(data[i * 4..i * 4 + 4].try_into().unwrap());
        rgba[i * 4] = ((px & r_mask) >> rs) as u8;
        rgba[i * 4 + 1] = ((px & g_mask) >> gs) as u8;
        rgba[i * 4 + 2] = ((px & b_mask) >> bs) as u8;
        rgba[i * 4 + 3] = if a_mask == 0 {
            255
        } else {
            ((px & a_mask) >> as_) as u8
        };
    }
    Ok(DdsImage {
        width,
        height,
        rgba,
    })
}

/// Encodes tightly-packed RGBA8 to PNG (alpha preserved).
fn encode_png_rgba(rgba: &[u8], width: u32, height: u32) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    image::codecs::png::PngEncoder::new(&mut out)
        .write_image(rgba, width, height, ExtendedColorType::Rgba8)
        .map_err(|e| format!("PNG encode failed: {e}"))?;
    Ok(out)
}

/// Reads and decodes an uncompressed DDS strip at `rel`, mod layer first. If the
/// mod-resolved strip is block-compressed (unsupported), falls back to the base
/// game's strip via `fallback` so the atlas still renders vanilla-derived icons.
fn read_strip(vfs: &Vfs, fallback: Option<&Vfs>, rel: &str) -> Result<DdsImage, String> {
    let bytes = vfs.read(rel)?;
    match decode_dds(&bytes) {
        Ok(img) => Ok(img),
        Err(e) => match fallback {
            Some(base) => {
                let base_bytes = base.read(rel)?;
                decode_dds(&base_bytes)
            }
            None => Err(e),
        },
    }
}

/// The built atlas: a horizontal strip of `count` `frame_w × frame_h` frames plus
/// the key → frame-index map, in stable (definition/icon) order.
pub struct IconAtlas {
    pub kind: String,
    pub frame_w: u32,
    pub frame_h: u32,
    pub count: u32,
    /// key → frame index, in stable order.
    pub index: Vec<(String, u32)>,
    /// RGBA PNG of the whole strip.
    pub png: Vec<u8>,
}

impl IconAtlas {
    /// `[u32 header_len LE][header JSON][PNG bytes]` (see module docs).
    pub fn to_wire(&self) -> Vec<u8> {
        #[derive(serde::Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Header {
            kind: String,
            frame_w: u32,
            frame_h: u32,
            count: u32,
            index: serde_json::Map<String, serde_json::Value>,
        }
        let mut index = serde_json::Map::new();
        for (k, v) in &self.index {
            index.insert(k.clone(), (*v).into());
        }
        let header = Header {
            kind: self.kind.clone(),
            frame_w: self.frame_w,
            frame_h: self.frame_h,
            count: self.count,
            index,
        };
        let json = serde_json::to_vec(&header).unwrap_or_default();
        let mut out = Vec::with_capacity(4 + json.len() + self.png.len());
        out.extend_from_slice(&(json.len() as u32).to_le_bytes());
        out.extend_from_slice(&json);
        out.extend_from_slice(&self.png);
        out
    }
}

/// Trade goods in definition order: `common/tradegoods` top-level `key = { … }`
/// blocks. Position = strip frame index (the documented positional-indexing rule).
pub fn trade_good_order(vfs: &Vfs) -> Vec<String> {
    let mut merged = Block::default();
    for (name, path) in vfs.list_dir("common/tradegoods") {
        if !name.to_lowercase().ends_with(".txt") {
            continue;
        }
        if let Ok(bytes) = std::fs::read(&path) {
            let b = crate::paradox::parse(&String::from_utf8_lossy(&bytes));
            merged.items.extend(b.items);
        }
    }
    merged.key_blocks().map(|(k, _)| k.to_string()).collect()
}

/// Religion key → strip frame index, from each religion block's explicit
/// `icon = N` field (1-based → frame `N - 1`). Group children without an `icon`
/// scalar are not religions and are skipped.
pub fn religion_icon_index(vfs: &Vfs) -> Vec<(String, u32)> {
    let mut merged = Block::default();
    for (name, path) in vfs.list_dir("common/religions") {
        if !name.to_lowercase().ends_with(".txt") {
            continue;
        }
        if let Ok(bytes) = std::fs::read(&path) {
            let b = crate::paradox::parse(&String::from_utf8_lossy(&bytes));
            merged.items.extend(b.items);
        }
    }
    let mut out = Vec::new();
    for (_group, group_block) in merged.key_blocks() {
        for (rel, block) in group_block.key_blocks() {
            if let Some(icon) = block.get_scalar("icon").and_then(|s| s.parse::<u32>().ok()) {
                // `icon` is 1-based in the files; frame index is 0-based.
                let frame = icon.saturating_sub(1);
                out.push((rel.to_string(), frame));
            }
        }
    }
    out
}

/// Slices a decoded strip into a validated single-row atlas. Frames are assumed
/// square (`frame_w = frame_h = height`), matching the EU4 strip convention;
/// `count = width / frame_w`.
fn strip_from_dds(kind: &str, img: DdsImage, index: Vec<(String, u32)>) -> Result<IconAtlas, String> {
    if img.height == 0 || img.width == 0 {
        return Err(format!("{kind} strip has zero dimensions"));
    }
    let frame = img.height;
    let count = img.width / frame;
    if count == 0 {
        return Err(format!(
            "{kind} strip too narrow ({}×{}) for a {frame}px frame",
            img.width, img.height
        ));
    }
    let png = encode_png_rgba(&img.rgba, img.width, img.height)?;
    Ok(IconAtlas {
        kind: kind.to_string(),
        frame_w: frame,
        frame_h: frame,
        count,
        index,
        png,
    })
}

/// Blits `src` (RGBA) centered into cell `cell` of a `count`-wide RGBA strip
/// whose frames are `frame × frame`.
fn blit_centered(dst: &mut [u8], strip_w: u32, frame: u32, cell: u32, src: &DdsImage) {
    let ox = cell * frame + (frame.saturating_sub(src.width)) / 2;
    let oy = (frame.saturating_sub(src.height)) / 2;
    for y in 0..src.height.min(frame) {
        for x in 0..src.width.min(frame) {
            let sp = ((y * src.width + x) * 4) as usize;
            let dx = ox + x;
            let dy = oy + y;
            let dp = ((dy * strip_w + dx) * 4) as usize;
            dst[dp..dp + 4].copy_from_slice(&src.rgba[sp..sp + 4]);
        }
    }
}

/// Stitches a set of individual game DDS icons into one synthetic single-row
/// strip: `sources` is `(key, game-relative dds path)` in the desired frame
/// order. Each source may differ in size, so it is centered in a common square
/// frame (max of all source dimensions). The returned `index` maps each key to
/// its frame position (definition order). Shared by the development stat strip and
/// the S3.3 trade-details strip (CoT tiers + trade-modifier badge).
fn stitched_atlas(
    vfs: &Vfs,
    fallback: Option<&Vfs>,
    kind: &str,
    sources: &[(&str, &str)],
) -> Result<IconAtlas, String> {
    let mut imgs = Vec::new();
    for (_key, rel) in sources {
        imgs.push(read_strip(vfs, fallback, rel)?);
    }
    let frame = imgs
        .iter()
        .flat_map(|i| [i.width, i.height])
        .max()
        .unwrap_or(1);
    let count = imgs.len() as u32;
    let strip_w = frame * count;
    let mut rgba = vec![0u8; (strip_w * frame * 4) as usize];
    for (i, img) in imgs.iter().enumerate() {
        blit_centered(&mut rgba, strip_w, frame, i as u32, img);
    }
    let png = encode_png_rgba(&rgba, strip_w, frame)?;
    Ok(IconAtlas {
        kind: kind.to_string(),
        frame_w: frame,
        frame_h: frame,
        count,
        index: sources
            .iter()
            .enumerate()
            .map(|(i, (k, _))| (k.to_string(), i as u32))
            .collect(),
        png,
    })
}

/// Builds the synthetic development strip (tax / production / manpower).
fn development_atlas(vfs: &Vfs, fallback: Option<&Vfs>) -> Result<IconAtlas, String> {
    stitched_atlas(
        vfs,
        fallback,
        "development",
        &[
            ("base_tax", "gfx/interface/development_button_base_tax.dds"),
            (
                "base_production",
                "gfx/interface/development_button_base_production.dds",
            ),
            (
                "base_manpower",
                "gfx/interface/development_button_base_manpower.dds",
            ),
        ],
    )
}

/// Builds the synthetic trade-details strip (S3.3): the six center-of-trade tier
/// icons (inland 1–3 at frames 0–2, coastal 1–3 at frames 3–5) plus a trade-power
/// badge (frame 6) for provinces carrying a trade-relevant permanent modifier. All
/// are individual uncompressed 32-bpp DDS in `gfx/interface/cot_icons/` (+ the
/// shared `icon_trade_power.dds`), so [`stitched_atlas`] decodes them directly.
/// Frontend frame math: inland tier `t` → `t-1`; coastal tier `t` → `2 + t`.
fn trade_details_atlas(vfs: &Vfs, fallback: Option<&Vfs>) -> Result<IconAtlas, String> {
    stitched_atlas(
        vfs,
        fallback,
        "trade_details",
        &[
            ("cot_inland_1", "gfx/interface/cot_icons/cot_inland_1.dds"),
            ("cot_inland_2", "gfx/interface/cot_icons/cot_inland_2.dds"),
            ("cot_inland_3", "gfx/interface/cot_icons/cot_inland_3.dds"),
            ("cot_coastal_1", "gfx/interface/cot_icons/cot_coastal_1.dds"),
            ("cot_coastal_2", "gfx/interface/cot_icons/cot_coastal_2.dds"),
            ("cot_coastal_3", "gfx/interface/cot_icons/cot_coastal_3.dds"),
            ("trade_modifier", "gfx/interface/icon_trade_power.dds"),
        ],
    )
}

/// Builds the icon atlas for `kind` ∈ `trade_goods` | `religions` | `development`.
///
/// `fallback` is a base-only [`Vfs`] used to recover the strip PNG when a mod
/// ships a block-compressed strip (see the module's known-limitation note); pass
/// `None` when there is no mod layer.
pub fn icon_atlas(vfs: &Vfs, fallback: Option<&Vfs>, kind: &str) -> Result<IconAtlas, String> {
    match kind {
        "trade_goods" => {
            let img = read_strip(vfs, fallback, "gfx/interface/resources.dds")?;
            let index = trade_good_order(vfs)
                .into_iter()
                .enumerate()
                .map(|(i, k)| (k, i as u32))
                .collect();
            strip_from_dds("trade_goods", img, index)
        }
        "religions" => {
            let img = read_strip(vfs, fallback, "gfx/interface/icon_religion.dds")?;
            let index = religion_icon_index(vfs);
            strip_from_dds("religions", img, index)
        }
        "development" => development_atlas(vfs, fallback),
        "trade_details" => trade_details_atlas(vfs, fallback),
        other => Err(format!("Unknown icon atlas kind: {other}")),
    }
}

// ---------------------------------------------------------------------------
// Trade-good icon-strip extension (Sprint 7.4)
// ---------------------------------------------------------------------------
//
// A newly created trade good lands at the END of definition order, so the game
// reads its icon from a frame *past* the vanilla resources strip. A mod ships
// its own `gfx/interface/resources.dds` that fully overrides the base one (Vfs
// shadow semantics in-app; the game reads the mod file). We generate that
// override: the current strip's frames verbatim, plus one colored placeholder
// tile per new good at its definition index.
//
// **Multi-create chaining.** Each `prepare_trade_good_scaffold` emits ONE
// `binaryAsset` write of the *whole* strip. Since a binary write replaces the
// file wholesale and the last write to a file wins in `apply_queue`, the strip
// this call generates must be a **superset** covering every not-yet-saved good
// created before it. So the frontend passes the prior pending goods' colors and
// this call draws frames for all of them plus the new one — the last create's
// (largest) strip wins on save. Across saves it chains automatically: the next
// session reads the already-extended strip through the Vfs as its base.

/// Encodes tightly-packed RGBA8 as an **uncompressed 32-bpp BGRA** DDS — the
/// exact layout [`decode_dds`] reads (fourCC 0, A8R8G8B8 masks). 128-byte header
/// + `B,G,R,A` pixels. Round-trips through `decode_dds`.
pub fn encode_dds_bgra(rgba: &[u8], width: u32, height: u32) -> Result<Vec<u8>, String> {
    let n = (width as usize) * (height as usize);
    if rgba.len() < n * 4 {
        return Err("encode_dds_bgra: pixel buffer too small".into());
    }
    let mut out = vec![0u8; 128];
    out[0..4].copy_from_slice(b"DDS ");
    let put = |o: usize, v: u32, buf: &mut [u8]| buf[o..o + 4].copy_from_slice(&v.to_le_bytes());
    put(4, 124, &mut out); // header size
    put(8, 0x0000_100f, &mut out); // CAPS|HEIGHT|WIDTH|PITCH|PIXELFORMAT
    put(12, height, &mut out);
    put(16, width, &mut out);
    put(20, width * 4, &mut out); // pitch
    put(76, 32, &mut out); // pixel-format struct size
    put(80, 0x41, &mut out); // DDPF_ALPHAPIXELS | DDPF_RGB
    put(84, 0, &mut out); // fourCC = 0 (uncompressed)
    put(88, 32, &mut out); // 32 bpp
    put(92, 0x00ff_0000, &mut out); // R mask
    put(96, 0x0000_ff00, &mut out); // G mask
    put(100, 0x0000_00ff, &mut out); // B mask
    put(104, 0xff00_0000, &mut out); // A mask
    put(108, 0x1000, &mut out); // DDSCAPS_TEXTURE
    out.reserve(n * 4);
    for i in 0..n {
        let p = i * 4;
        out.extend_from_slice(&[rgba[p + 2], rgba[p + 1], rgba[p], rgba[p + 3]]); // B G R A
    }
    Ok(out)
}

/// Draws a distinct colored placeholder tile into cell `cell` of an RGBA strip
/// (`frame × frame` cells): solid fill of `color`, a darker border, and a
/// lighter diagonal — enough to read as "a good with no real art yet".
fn draw_placeholder(dst: &mut [u8], strip_w: u32, frame: u32, cell: u32, color: [u8; 3]) {
    let scale = |c: u8, f: f32| ((c as f32 * f).round().clamp(0.0, 255.0)) as u8;
    let border = [scale(color[0], 0.45), scale(color[1], 0.45), scale(color[2], 0.45)];
    let diag = [
        scale(color[0], 1.0).max(180).min(255),
        scale(color[1], 1.0).max(180).min(255),
        scale(color[2], 1.0).max(180).min(255),
    ];
    let b = (frame / 12).max(2); // border thickness
    let ox = cell * frame;
    for y in 0..frame {
        for x in 0..frame {
            let on_border = x < b || y < b || x >= frame - b || y >= frame - b;
            // A 2px-wide anti-diagonal band.
            let on_diag = ((x + y) as i64 - (frame as i64 - 1)).abs() <= 1;
            let px = if on_border {
                [border[0], border[1], border[2], 255]
            } else if on_diag {
                [diag[0], diag[1], diag[2], 255]
            } else {
                [color[0], color[1], color[2], 255]
            };
            let dp = (((y) * strip_w + (ox + x)) * 4) as usize;
            dst[dp..dp + 4].copy_from_slice(&px);
        }
    }
}

/// Builds an extended `resources.dds` (uncompressed BGRA) covering the current
/// strip plus a placeholder tile for each `(definition_index, rgb)` in
/// `placements`. Frame size and existing frames come from the Vfs-resolved strip
/// (base fallback if a mod ships a block-compressed one, per the module note).
pub fn extended_resources_strip(
    vfs: &Vfs,
    fallback: Option<&Vfs>,
    placements: &[(u32, [u8; 3])],
) -> Result<Vec<u8>, String> {
    let base = read_strip(vfs, fallback, "gfx/interface/resources.dds")?;
    let frame = base.height;
    if frame == 0 {
        return Err("resources strip has zero height".into());
    }
    let base_count = base.width / frame;
    let max_idx = placements.iter().map(|(i, _)| *i).max().unwrap_or(0);
    let target_count = base_count.max(max_idx + 1);
    let strip_w = target_count * frame;
    let mut rgba = vec![0u8; (strip_w * frame * 4) as usize];
    // Copy the existing strip's pixels row-by-row into the left region; frames
    // beyond it start fully transparent.
    let copy_w = (base.width.min(strip_w) * 4) as usize;
    for y in 0..frame {
        let src_row = (y * base.width * 4) as usize;
        let dst_row = (y * strip_w * 4) as usize;
        rgba[dst_row..dst_row + copy_w].copy_from_slice(&base.rgba[src_row..src_row + copy_w]);
    }
    for (idx, color) in placements {
        draw_placeholder(&mut rgba, strip_w, frame, *idx, *color);
    }
    encode_dds_bgra(&rgba, strip_w, frame)
}

// ---------------------------------------------------------------------------
// Custom icon-art import (S2.5)
// ---------------------------------------------------------------------------
//
// Replace one trade-good or religion icon with a user-picked image. The picked
// file is decoded, resized to the strip's square tile, and spliced into the
// good/religion's POSITIONAL frame index of the project's copy of the strip (the
// AGENTS.md gotcha: strips are indexed by definition/`icon` order). The result is
// a whole re-encoded strip returned as a pending `BinaryAsset` — it rides the
// save queue like every other edit; the base install is never written here.
//
// ## Format decisions (documented per the spec)
// * **Reading the user's file** — PNG/JPG/BMP via the `image` crate (sniffed),
//   TGA via the crate with an explicit format (TGA has no magic bytes), DDS via
//   the Sprint-14 [`crate::gfx`] decoder (uncompressed 32-bpp + BC1/2/3; BC7 /
//   other DX10 → clear error).
// * **Reading the target strip** — the same `gfx` decoder, so a mod's
//   block-compressed strip (Anbennar ships BC1/2/3 icon strips) still decodes.
//   A BC7/DX10 strip that `gfx` can't decode surfaces a clear error rather than
//   corrupting the file.
// * **Writing the strip back** — in the SOURCE strip's container: a `.tga` strip
//   re-encodes as TGA (`image` crate), any `.dds` strip re-encodes as
//   **uncompressed** 32-bpp BGRA via [`encode_dds_bgra`]. There is no BCn
//   encoder, so a BC-compressed source is written UNCOMPRESSED — the game reads
//   uncompressed DDS fine (vanilla ships many). Both toolkit target strips
//   (`resources.dds`, `icon_religion.dds`) are `.dds`, so output is DDS in
//   practice; the TGA branch covers a future `.tga` strip and is unit-tested.
// * **Resize** — plain (non-aspect-preserving) resize to the tile size; tiles are
//   tiny (64px) so letterboxing buys nothing.
//
// ## Preview + chaining
// The command also returns the spliced tile as a standalone PNG so the panel can
// show the new art immediately (the lighter of the two preview paths — no
// pending-aware atlas re-fetch). Multiple imports before a save chain the way the
// trade-good scaffold's strip does: the frontend passes the most recent pending
// strip bytes as `pending_strip`, and this splice composes on top of it, so the
// last (superset) `BinaryAsset` wins on save.

/// Game-relative strip path for an importable icon `kind`.
fn import_strip_rel(kind: &str) -> Result<&'static str, String> {
    match kind {
        "trade_goods" => Ok("gfx/interface/resources.dds"),
        "religions" => Ok("gfx/interface/icon_religion.dds"),
        other => Err(format!("import_icon: unsupported kind '{other}'")),
    }
}

/// Decodes a user-picked image file (PNG/JPG/BMP/TGA/DDS) to tightly-packed
/// RGBA8 + dimensions.
fn decode_import_source(path: &str) -> Result<(Vec<u8>, u32, u32), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("Failed to read {path}: {e}"))?;
    let lower = path.to_lowercase();
    if lower.ends_with(".dds") {
        let s = gfx::decode_dds(&bytes)?;
        Ok((s.rgba, s.width, s.height))
    } else if lower.ends_with(".tga") {
        // TGA has no magic bytes — set the format explicitly.
        let img = image::load_from_memory_with_format(&bytes, image::ImageFormat::Tga)
            .map_err(|e| format!("Failed to decode TGA {path}: {e}"))?
            .to_rgba8();
        let (w, h) = img.dimensions();
        Ok((img.into_raw(), w, h))
    } else {
        let img = image::load_from_memory(&bytes)
            .map_err(|e| format!("Failed to decode image {path}: {e}"))?
            .to_rgba8();
        let (w, h) = img.dimensions();
        Ok((img.into_raw(), w, h))
    }
}

/// Plain (non-aspect-preserving) resize of RGBA8 to `tw × th`.
fn resize_rgba(rgba: &[u8], w: u32, h: u32, tw: u32, th: u32) -> Result<Vec<u8>, String> {
    let img: image::RgbaImage = image::ImageBuffer::from_raw(w, h, rgba.to_vec())
        .ok_or("resize: source buffer size mismatch")?;
    let resized = image::imageops::resize(&img, tw, th, image::imageops::FilterType::Triangle);
    Ok(resized.into_raw())
}

/// Encodes tightly-packed RGBA8 as an uncompressed 32-bpp TGA (used only when the
/// target strip is itself a `.tga`).
fn encode_tga_rgba(rgba: &[u8], width: u32, height: u32) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    image::codecs::tga::TgaEncoder::new(&mut out)
        .write_image(rgba, width, height, ExtendedColorType::Rgba8)
        .map_err(|e| format!("TGA encode failed: {e}"))?;
    Ok(out)
}

/// The result of splicing one tile into a strip: the whole re-encoded strip, the
/// single spliced tile as a PNG (preview), and the tile size (= strip height).
struct SplicedStrip {
    strip: Vec<u8>,
    tile_png: Vec<u8>,
    tile: u32,
}

/// Core splice: decode `strip_bytes` (TGA if `is_tga`, else DDS via `gfx`), resize
/// `src` to the strip's square tile size (= strip height), overwrite cell `frame`
/// (widening the strip with transparent cells if `frame` is past the current
/// count), and re-encode in the strip's container. Untouched tiles are copied
/// verbatim from the decoded strip, so they are byte-identical after a decode of
/// the output.
fn splice_into_strip(
    strip_bytes: &[u8],
    is_tga: bool,
    frame: u32,
    src_rgba: &[u8],
    src_w: u32,
    src_h: u32,
) -> Result<SplicedStrip, String> {
    // Decode the target strip to RGBA.
    let (base_rgba, base_w, base_h) = if is_tga {
        let img = image::load_from_memory_with_format(strip_bytes, image::ImageFormat::Tga)
            .map_err(|e| format!("Failed to decode TGA strip: {e}"))?
            .to_rgba8();
        let (w, h) = img.dimensions();
        (img.into_raw(), w, h)
    } else {
        let s = gfx::decode_dds(strip_bytes)?;
        (s.rgba, s.width, s.height)
    };
    if base_w == 0 || base_h == 0 {
        return Err("strip has zero dimensions".into());
    }
    // Frames are square, tile = strip height (matches the EU4 strip convention).
    let tile = base_h;
    let resized = resize_rgba(src_rgba, src_w, src_h, tile, tile)?;

    let base_count = base_w / tile;
    let target_count = base_count.max(frame + 1);
    let out_w = target_count * tile;
    let mut out = vec![0u8; (out_w * tile * 4) as usize];
    // Copy the existing strip rows into the left region; any widened cells stay
    // fully transparent.
    let copy_w = (base_w.min(out_w) * 4) as usize;
    for y in 0..tile {
        let s = (y * base_w * 4) as usize;
        let d = (y * out_w * 4) as usize;
        out[d..d + copy_w].copy_from_slice(&base_rgba[s..s + copy_w]);
    }
    // Overwrite the target cell with the resized tile.
    let ox = frame * tile;
    for y in 0..tile {
        for x in 0..tile {
            let sp = ((y * tile + x) * 4) as usize;
            let dp = ((y * out_w + (ox + x)) * 4) as usize;
            out[dp..dp + 4].copy_from_slice(&resized[sp..sp + 4]);
        }
    }

    let strip = if is_tga {
        encode_tga_rgba(&out, out_w, tile)?
    } else {
        encode_dds_bgra(&out, out_w, tile)?
    };
    let tile_png = encode_png_rgba(&resized, tile, tile)?;
    Ok(SplicedStrip {
        strip,
        tile_png,
        tile,
    })
}

/// The import result handed back to the frontend.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportedIcon {
    /// Project-relative strip path the `BinaryAsset` targets.
    pub file: String,
    /// The whole re-encoded strip (queue it as a `BinaryAsset`).
    pub strip: Vec<u8>,
    /// The spliced tile as a standalone PNG (immediate panel preview).
    pub tile_png: Vec<u8>,
    pub frame_w: u32,
    pub frame_h: u32,
    /// Echoes the target frame index.
    pub frame: u32,
}

/// Imports a user-picked image as the icon at positional `frame` of the
/// `trade_goods`/`religions` strip. `pending_strip`, when given, is the most
/// recent not-yet-saved strip bytes (from an earlier import or a create-good
/// scaffold) so multiple pre-save imports chain (see the module note). Returns
/// the spliced strip (queue as a `BinaryAsset`) + a preview tile PNG. The base
/// install is never written.
#[tauri::command]
pub fn import_icon(
    install_path: String,
    mod_path: Option<String>,
    kind: String,
    frame: u32,
    source_path: String,
    pending_strip: Option<Vec<u8>>,
) -> Result<ImportedIcon, String> {
    let rel = import_strip_rel(&kind)?;
    let (src_rgba, src_w, src_h) = decode_import_source(&source_path)?;
    // The strip we splice into: a pending (always uncompressed-DDS) strip chains;
    // otherwise the Vfs-resolved strip (mod shadows base).
    let (strip_bytes, is_tga) = match pending_strip {
        Some(b) => (b, false),
        None => {
            let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
            let bytes = vfs.read(rel)?;
            (bytes, rel.to_lowercase().ends_with(".tga"))
        }
    };
    let spliced = splice_into_strip(&strip_bytes, is_tga, frame, &src_rgba, src_w, src_h)
        .map_err(|e| format!("Can't import into the {kind} strip: {e}"))?;
    Ok(ImportedIcon {
        file: rel.to_string(),
        strip: spliced.strip,
        tile_png: spliced.tile_png,
        frame_w: spliced.tile,
        frame_h: spliced.tile,
        frame,
    })
}

/// Serves a sprite strip as `[u32 header_len][header JSON][PNG]` (see module
/// docs). `kind` is `trade_goods` | `religions` | `development`.
#[tauri::command]
pub fn get_icon_atlas(
    install_path: String,
    mod_path: Option<String>,
    kind: String,
) -> Result<tauri::ipc::Response, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    // A base-only Vfs recovers the strip PNG if a mod ships a compressed one.
    let base = mod_path
        .as_deref()
        .map(|_| Vfs::new(&install_path, None))
        .transpose()?;
    let atlas = icon_atlas(&vfs, base.as_ref(), &kind)?;
    Ok(tauri::ipc::Response::new(atlas.to_wire()))
}

#[cfg(test)]
mod tests {
    use super::*;
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

    /// Builds a minimal uncompressed 32-bpp BGRA DDS in memory, mirroring the
    /// game's masks (R=0x00ff0000, G=0x0000ff00, B=0x000000ff, A=0xff000000).
    fn synthetic_dds(width: u32, height: u32, fill: [u8; 4]) -> Vec<u8> {
        let mut out = vec![0u8; 128];
        out[0..4].copy_from_slice(b"DDS ");
        let put = |o: usize, v: u32, buf: &mut Vec<u8>| {
            buf[o..o + 4].copy_from_slice(&v.to_le_bytes());
        };
        put(4, 124, &mut out); // header size
        put(8, 0x100f, &mut out); // flags
        put(12, height, &mut out);
        put(16, width, &mut out);
        put(80, 0x41, &mut out); // DDPF_ALPHAPIXELS | DDPF_RGB
        put(84, 0, &mut out); // fourCC = 0 (uncompressed)
        put(88, 32, &mut out); // 32 bpp
        put(92, 0x00ff0000, &mut out); // R
        put(96, 0x0000ff00, &mut out); // G
        put(100, 0x000000ff, &mut out); // B
        put(104, 0xff000000, &mut out); // A
        // Pixels: file byte order is B, G, R, A (BGRA).
        let [r, g, b, a] = fill;
        for _ in 0..(width * height) {
            out.extend_from_slice(&[b, g, r, a]);
        }
        out
    }

    #[test]
    fn decodes_uncompressed_bgra() {
        let dds = synthetic_dds(2, 1, [10, 20, 30, 40]);
        let img = decode_dds(&dds).unwrap();
        assert_eq!((img.width, img.height), (2, 1));
        // BGRA in file -> RGBA out.
        assert_eq!(&img.rgba[0..4], &[10, 20, 30, 40]);
        assert_eq!(&img.rgba[4..8], &[10, 20, 30, 40]);
    }

    #[test]
    fn rejects_compressed_dds() {
        let mut dds = synthetic_dds(4, 4, [0, 0, 0, 0]);
        // Set fourCC to "DXT5".
        dds[84..88].copy_from_slice(b"DXT5");
        dds[80..84].copy_from_slice(&0x4u32.to_le_bytes()); // DDPF_FOURCC
        assert!(decode_dds(&dds).is_err());
    }

    #[test]
    fn strip_frame_math_is_square_single_row() {
        // A 4-frame strip: width 256, height 64 -> 64px square frames, count 4.
        let img = DdsImage {
            width: 256,
            height: 64,
            rgba: vec![0u8; (256 * 64 * 4) as usize],
        };
        let atlas = strip_from_dds(
            "trade_goods",
            img,
            vec![("grain".into(), 0), ("wine".into(), 1)],
        )
        .unwrap();
        assert_eq!(atlas.frame_w, 64);
        assert_eq!(atlas.frame_h, 64);
        assert_eq!(atlas.count, 4);
        // PNG round-trips at the strip dimensions.
        let decoded =
            image::load_from_memory_with_format(&atlas.png, image::ImageFormat::Png).unwrap();
        assert_eq!(decoded.width(), 256);
        assert_eq!(decoded.height(), 64);
    }

    #[test]
    fn wire_round_trips_header_and_png() {
        let img = DdsImage {
            width: 128,
            height: 64,
            rgba: vec![0u8; (128 * 64 * 4) as usize],
        };
        let atlas =
            strip_from_dds("trade_goods", img, vec![("grain".into(), 0), ("wine".into(), 1)])
                .unwrap();
        let wire = atlas.to_wire();
        let header_len = u32::from_le_bytes(wire[..4].try_into().unwrap()) as usize;
        let header: serde_json::Value =
            serde_json::from_slice(&wire[4..4 + header_len]).unwrap();
        assert_eq!(header["kind"], "trade_goods");
        assert_eq!(header["frameW"], 64);
        assert_eq!(header["count"], 2); // 128/64 = 2 frames
        assert_eq!(header["index"]["grain"], 0);
        assert_eq!(header["index"]["wine"], 1);
        // Trailing bytes are a valid PNG.
        let png = &wire[4 + header_len..];
        assert!(image::load_from_memory_with_format(png, image::ImageFormat::Png).is_ok());
    }

    #[test]
    fn real_trade_goods_atlas() {
        let Some(vfs) = real_install() else { return };
        let atlas = icon_atlas(&vfs, None, "trade_goods").unwrap();
        // Vanilla resources.dds is 2048×64 -> 32 frames.
        assert_eq!(atlas.frame_w, 64);
        assert_eq!(atlas.frame_h, 64);
        assert_eq!(atlas.count, 32);
        // The index is the definition order; one entry per trade good.
        let order = trade_good_order(&vfs);
        assert_eq!(atlas.index.len(), order.len());
        assert_eq!(order.first().map(String::as_str), Some("grain"));
        // "grain" is the first-defined good -> frame 0.
        let grain = atlas.index.iter().find(|(k, _)| k == "grain").unwrap();
        assert_eq!(grain.1, 0);
        // "unknown" (the no-good sentinel) is defined last in vanilla.
        assert!(atlas.index.iter().any(|(k, _)| k == "unknown"));
        // PNG decodes at the strip dimensions.
        let img =
            image::load_from_memory_with_format(&atlas.png, image::ImageFormat::Png).unwrap();
        assert_eq!(img.width(), 2048);
        assert_eq!(img.height(), 64);
    }

    #[test]
    fn real_religion_atlas() {
        let Some(vfs) = real_install() else { return };
        let atlas = icon_atlas(&vfs, None, "religions").unwrap();
        // icon_religion.dds is 1856×64 -> 29 frames.
        assert_eq!(atlas.frame_h, 64);
        assert_eq!(atlas.count, 29);
        // catholic has `icon = 1` -> frame 0.
        let cat = atlas
            .index
            .iter()
            .find(|(k, _)| k == "catholic")
            .expect("catholic religion");
        assert_eq!(cat.1, 0);
        // protestant has `icon = 2` -> frame 1.
        let prot = atlas.index.iter().find(|(k, _)| k == "protestant").unwrap();
        assert_eq!(prot.1, 1);
        // Non-religion group children (flag_emblem_index_range) are not indexed.
        assert!(!atlas.index.iter().any(|(k, _)| k == "flag_emblem_index_range"));
        // Every mapped frame is within the strip.
        assert!(atlas.index.iter().all(|(_, f)| *f < atlas.count));
    }

    #[test]
    fn real_development_atlas() {
        let Some(vfs) = real_install() else { return };
        let atlas = icon_atlas(&vfs, None, "development").unwrap();
        assert_eq!(atlas.count, 3);
        assert_eq!(atlas.index[0], ("base_tax".to_string(), 0));
        assert_eq!(atlas.index[1], ("base_production".to_string(), 1));
        assert_eq!(atlas.index[2], ("base_manpower".to_string(), 2));
        // Strip is 3 square frames wide.
        let img =
            image::load_from_memory_with_format(&atlas.png, image::ImageFormat::Png).unwrap();
        assert_eq!(img.width(), atlas.frame_w * 3);
        assert_eq!(img.height(), atlas.frame_h);
    }

    #[test]
    fn real_trade_details_atlas() {
        let Some(vfs) = real_install() else { return };
        let atlas = icon_atlas(&vfs, None, "trade_details").unwrap();
        // Six CoT tier icons + one trade-power badge = 7 square frames.
        assert_eq!(atlas.count, 7);
        assert_eq!(atlas.frame_w, atlas.frame_h);
        // Frame layout is stable: inland 1–3, coastal 1–3, then the badge.
        let idx = |k: &str| atlas.index.iter().find(|(n, _)| n == k).map(|(_, f)| *f);
        assert_eq!(idx("cot_inland_1"), Some(0));
        assert_eq!(idx("cot_inland_3"), Some(2));
        assert_eq!(idx("cot_coastal_1"), Some(3));
        assert_eq!(idx("cot_coastal_3"), Some(5));
        assert_eq!(idx("trade_modifier"), Some(6));
        // Every mapped frame is within the strip.
        assert!(atlas.index.iter().all(|(_, f)| *f < atlas.count));
        // PNG decodes at the strip dimensions.
        let img =
            image::load_from_memory_with_format(&atlas.png, image::ImageFormat::Png).unwrap();
        assert_eq!(img.width(), atlas.frame_w * 7);
        assert_eq!(img.height(), atlas.frame_h);
    }

    #[test]
    fn dds_encoder_round_trips_through_decoder() {
        // A 3-wide, 1-tall RGBA image with distinct channel values must survive
        // encode -> decode losslessly (proves BGRA byte order + masks).
        let rgba = vec![
            10, 20, 30, 40, // px0
            50, 60, 70, 255, // px1
            200, 100, 5, 128, // px2
        ];
        let dds = encode_dds_bgra(&rgba, 3, 1).unwrap();
        let img = decode_dds(&dds).unwrap();
        assert_eq!((img.width, img.height), (3, 1));
        assert_eq!(img.rgba, rgba);
    }

    #[test]
    fn extended_strip_appends_one_frame_with_placeholder() {
        // Base install strip has 32 frames; extending at index 32 yields a strip
        // with 33 frames whose final frame is the (opaque) placeholder tile.
        let Some(vfs) = real_install() else { return };
        let new_color = [200u8, 40, 90];
        let bytes = extended_resources_strip(&vfs, None, &[(32, new_color)]).unwrap();
        let img = decode_dds(&bytes).unwrap();
        assert_eq!(img.height, 64);
        assert_eq!(img.width / 64, 33, "one frame appended");
        // The new frame's interior center pixel is fully opaque (placeholder
        // drew there; the vanilla strip's frame 32 would not exist).
        let cx = 32 * 64 + 32; // center-x of frame 32
        let cy = 32u32;
        let p = ((cy * img.width + cx) * 4) as usize;
        assert_eq!(img.rgba[p + 3], 255, "placeholder is opaque");
        // Frame 0 (grain) is copied verbatim from the base strip: some pixel in
        // it must be non-transparent.
        let any_opaque_frame0 = (0..64).any(|y| {
            (0..64).any(|x| {
                let q = ((y * img.width + x) * 4) as usize;
                img.rgba[q + 3] > 0
            })
        });
        assert!(any_opaque_frame0, "base frames preserved");
    }

    #[test]
    fn extended_strip_covers_multiple_pending_goods() {
        // Two new goods (indices 32, 33) -> a 34-frame strip, both placeholders
        // opaque. Mirrors the multi-create superset the last save writes.
        let Some(vfs) = real_install() else { return };
        let bytes =
            extended_resources_strip(&vfs, None, &[(32, [10, 200, 30]), (33, [220, 10, 40])])
                .unwrap();
        let img = decode_dds(&bytes).unwrap();
        assert_eq!(img.width / 64, 34);
        for cell in [32u32, 33] {
            let p = ((32 * img.width + (cell * 64 + 32)) * 4) as usize;
            assert_eq!(img.rgba[p + 3], 255);
        }
    }

    #[test]
    fn synthetic_mod_strip_resolves_and_extends() {
        // A synthetic mod that ships an *uncompressed* extended strip plus an
        // extra trade good: the atlas count follows the mod strip, and a further
        // extension chains on top of it.
        let root = std::env::temp_dir().join("eu_toolkit_icons_test_synthmod");
        let base = root.join("base");
        let md = root.join("mod");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(base.join("map")).unwrap();
        std::fs::write(base.join("map/provinces.bmp"), b"x").unwrap();
        // Base: a 2-frame (128x64) strip + 2 goods.
        std::fs::create_dir_all(base.join("gfx/interface")).unwrap();
        std::fs::write(
            base.join("gfx/interface/resources.dds"),
            synthetic_dds(128, 64, [10, 20, 30, 255]),
        )
        .unwrap();
        std::fs::create_dir_all(base.join("common/tradegoods")).unwrap();
        std::fs::write(
            base.join("common/tradegoods/00_tradegoods.txt"),
            b"grain = { color = { 0.9 0.9 0.5 } }\nwine = { color = { 0.3 0.1 0.2 } }\n",
        )
        .unwrap();
        // Mod: a 3-frame (192x64) strip and a third good appended.
        std::fs::create_dir_all(md.join("gfx/interface")).unwrap();
        std::fs::write(
            md.join("gfx/interface/resources.dds"),
            synthetic_dds(192, 64, [90, 80, 70, 255]),
        )
        .unwrap();
        std::fs::create_dir_all(md.join("common/tradegoods")).unwrap();
        std::fs::write(
            md.join("common/tradegoods/zz_new.txt"),
            b"spark = { color = { 0.1 0.8 0.3 } }\n",
        )
        .unwrap();
        // The .mod descriptor is optional for Vfs::new; point straight at the dir.
        let vfs = Vfs::new(base.to_str().unwrap(), Some(md.to_str().unwrap())).unwrap();

        // Atlas reflects the mod strip (3 frames) and the mod-added good.
        let atlas = icon_atlas(&vfs, None, "trade_goods").unwrap();
        assert_eq!(atlas.count, 3);
        assert_eq!(trade_good_order(&vfs), vec!["grain", "wine", "spark"]);
        assert_eq!(atlas.index.iter().find(|(k, _)| k == "spark").unwrap().1, 2);

        // Extending places the next good at index 3 -> a 4-frame strip.
        let ext = extended_resources_strip(&vfs, None, &[(3, [255, 0, 0])]).unwrap();
        let img = decode_dds(&ext).unwrap();
        assert_eq!(img.width / 64, 4);
    }

    // --- S2.5 custom icon-art import ----------------------------------------

    /// An uncompressed BGRA DDS strip whose cells are painted with distinct
    /// (opaque) fills, so a splice can be shown to leave the OTHER cells intact.
    fn strip_dds_distinct(tile: u32, fills: &[[u8; 4]]) -> Vec<u8> {
        let count = fills.len() as u32;
        let w = tile * count;
        let h = tile;
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        for (cell, c) in fills.iter().enumerate() {
            let ox = cell as u32 * tile;
            for y in 0..tile {
                for x in 0..tile {
                    let dp = ((y * w + (ox + x)) * 4) as usize;
                    rgba[dp..dp + 4].copy_from_slice(c);
                }
            }
        }
        encode_dds_bgra(&rgba, w, h).unwrap()
    }

    /// A solid `tile`-sized RGBA source.
    fn solid_rgba(w: u32, h: u32, c: [u8; 4]) -> Vec<u8> {
        let mut v = vec![0u8; (w * h * 4) as usize];
        for px in v.chunks_mut(4) {
            px.copy_from_slice(&c);
        }
        v
    }

    /// Reads cell `cell`'s center pixel of a decoded strip.
    fn cell_center(img: &gfx::Surface, tile: u32, cell: u32) -> [u8; 4] {
        let cx = cell * tile + tile / 2;
        let cy = tile / 2;
        let p = ((cy * img.width + cx) * 4) as usize;
        [img.rgba[p], img.rgba[p + 1], img.rgba[p + 2], img.rgba[p + 3]]
    }

    #[test]
    fn splice_dds_replaces_only_target_tile() {
        // 4-cell strip, distinct fills; splice a solid tile into cell 2 and prove
        // cells 0/1/3 survive byte-for-byte after decode.
        let tile = 8u32;
        let fills = [
            [10, 20, 30, 255],
            [40, 50, 60, 255],
            [70, 80, 90, 255],
            [100, 110, 120, 255],
        ];
        let strip = strip_dds_distinct(tile, &fills);
        let src = solid_rgba(16, 16, [200, 5, 150, 255]);
        let out = splice_into_strip(&strip, false, 2, &src, 16, 16).unwrap();
        assert_eq!(out.tile, tile);

        let before = gfx::decode_dds(&strip).unwrap();
        let after = gfx::decode_dds(&out.strip).unwrap();
        assert_eq!((after.width, after.height), (before.width, before.height));
        // Cell 2 is the new art.
        assert_eq!(cell_center(&after, tile, 2), [200, 5, 150, 255]);
        // Every other cell is byte-identical to the original decode.
        for cell in [0u32, 1, 3] {
            assert_eq!(
                cell_center(&after, tile, cell),
                cell_center(&before, tile, cell),
                "cell {cell} changed"
            );
        }
        // The whole non-cell-2 pixel region round-trips byte-for-byte.
        for cell in [0u32, 1, 3] {
            for y in 0..tile {
                for x in 0..tile {
                    let ox = cell * tile + x;
                    let p = ((y * after.width + ox) * 4) as usize;
                    assert_eq!(&after.rgba[p..p + 4], &before.rgba[p..p + 4]);
                }
            }
        }
    }

    #[test]
    fn splice_tga_strip_round_trips() {
        // Same guarantee for a TGA-container strip: write back as TGA, decode, and
        // confirm the target cell changed while the others survived.
        let tile = 8u32;
        let count = 3u32;
        let w = tile * count;
        // Distinct per-cell RGBA source strip.
        let fills = [[11, 22, 33, 255], [44, 55, 66, 255], [77, 88, 99, 255]];
        let mut rgba = vec![0u8; (w * tile * 4) as usize];
        for (cell, c) in fills.iter().enumerate() {
            let ox = cell as u32 * tile;
            for y in 0..tile {
                for x in 0..tile {
                    let dp = ((y * w + (ox + x)) * 4) as usize;
                    rgba[dp..dp + 4].copy_from_slice(c);
                }
            }
        }
        let tga = encode_tga_rgba(&rgba, w, tile).unwrap();
        let src = solid_rgba(10, 10, [1, 2, 253, 255]);
        let out = splice_into_strip(&tga, true, 1, &src, 10, 10).unwrap();

        let after = image::load_from_memory_with_format(&out.strip, image::ImageFormat::Tga)
            .unwrap()
            .to_rgba8();
        assert_eq!(after.dimensions(), (w, tile));
        let center = |cell: u32| -> [u8; 4] {
            after.get_pixel(cell * tile + tile / 2, tile / 2).0
        };
        assert_eq!(center(1), [1, 2, 253, 255]); // new art
        assert_eq!(center(0), [11, 22, 33, 255]); // preserved
        assert_eq!(center(2), [77, 88, 99, 255]); // preserved
    }

    /// A minimal DXT1 (BC1) DDS strip: `count` 4×4 blocks in one row, block `k`
    /// filled with a distinct solid color (c0 == c1 so all indices resolve to it).
    fn strip_dxt1(colors565: &[u16]) -> Vec<u8> {
        let count = colors565.len() as u32;
        let mut out = vec![0u8; 128];
        out[0..4].copy_from_slice(b"DDS ");
        let put = |o: usize, v: u32, b: &mut Vec<u8>| b[o..o + 4].copy_from_slice(&v.to_le_bytes());
        put(4, 124, &mut out);
        put(12, 4, &mut out); // height
        put(16, count * 4, &mut out); // width
        put(80, 0x4, &mut out); // DDPF_FOURCC
        out[84..88].copy_from_slice(b"DXT1");
        for &c in colors565 {
            // c0 == c1 -> the 4-color opaque mode, palette[0..1] both == c.
            out.extend_from_slice(&c.to_le_bytes());
            out.extend_from_slice(&c.to_le_bytes());
            out.extend_from_slice(&0u32.to_le_bytes()); // all indices -> palette[0]
        }
        out
    }

    #[test]
    fn splice_bc1_source_writes_uncompressed_dds() {
        // A block-compressed (BC1) strip: we can decode it but not BC-encode, so
        // the spliced strip must come back as an UNCOMPRESSED DDS while the
        // untouched frame keeps its (decoded) color.
        let red: u16 = 0xF800;
        let green: u16 = 0x07E0;
        let strip = strip_dxt1(&[red, green]); // 2 frames, 4px tiles
        let before = gfx::decode_dds(&strip).unwrap();
        let src = solid_rgba(4, 4, [12, 34, 56, 255]);
        let out = splice_into_strip(&strip, false, 1, &src, 4, 4).unwrap();

        // Output is an uncompressed DDS (fourCC 0), not BC.
        assert_eq!(&out.strip[0..4], b"DDS ");
        assert_eq!(u32::from_le_bytes(out.strip[84..88].try_into().unwrap()), 0);
        let after = gfx::decode_dds(&out.strip).unwrap();
        assert_eq!((after.width, after.height), (before.width, before.height));
        // Frame 1 is the new art; frame 0 keeps the BC-decoded red.
        assert_eq!(cell_center(&after, 4, 1), [12, 34, 56, 255]);
        assert_eq!(cell_center(&after, 4, 0), cell_center(&before, 4, 0));
    }

    #[test]
    fn splice_resizes_odd_input_to_tile() {
        // A 37×53 input is resized to the strip's tile size (16) and the preview
        // PNG comes back at tile dimensions.
        let tile = 16u32;
        let strip = strip_dds_distinct(tile, &[[9, 9, 9, 255], [8, 8, 8, 255]]);
        let src = solid_rgba(37, 53, [123, 45, 67, 255]);
        let out = splice_into_strip(&strip, false, 0, &src, 37, 53).unwrap();
        assert_eq!(out.tile, tile);
        let png =
            image::load_from_memory_with_format(&out.tile_png, image::ImageFormat::Png).unwrap();
        assert_eq!((png.width(), png.height()), (tile, tile));
        // The spliced cell is (approximately) the solid source color.
        let after = gfx::decode_dds(&out.strip).unwrap();
        let c = cell_center(&after, tile, 0);
        assert!((c[0] as i32 - 123).abs() <= 2 && (c[1] as i32 - 45).abs() <= 2);
    }

    #[test]
    fn splice_widens_strip_for_out_of_range_frame() {
        // Splicing at a frame past the current count widens the strip (the pending
        // create-good case where the new frame is at the end).
        let tile = 8u32;
        let strip = strip_dds_distinct(tile, &[[1, 2, 3, 255], [4, 5, 6, 255]]);
        let src = solid_rgba(8, 8, [9, 8, 7, 255]);
        let out = splice_into_strip(&strip, false, 4, &src, 8, 8).unwrap();
        let after = gfx::decode_dds(&out.strip).unwrap();
        assert_eq!(after.width / tile, 5, "strip widened to hold frame 4");
        assert_eq!(cell_center(&after, tile, 4), [9, 8, 7, 255]);
        // Frame 0/1 preserved; gap frames 2/3 are transparent.
        assert_eq!(cell_center(&after, tile, 0), [1, 2, 3, 255]);
        assert_eq!(cell_center(&after, tile, 2)[3], 0);
    }

    /// Writes a solid RGBA PNG to a temp file and returns its path.
    fn write_temp_png(name: &str, w: u32, h: u32, c: [u8; 4]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join("eu_toolkit_import_icon_src");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        let png = encode_png_rgba(&solid_rgba(w, h, c), w, h).unwrap();
        std::fs::write(&path, png).unwrap();
        path
    }

    #[test]
    fn import_icon_command_round_trips_through_project() {
        // End-to-end over the command: a synthetic base install with an
        // uncompressed resources strip, import a PNG, apply the BinaryAsset via the
        // edit queue, and re-read the project strip.
        use crate::edits::{apply_queue, TypedEdit};
        let root = std::env::temp_dir().join("eu_toolkit_icons_test_import_cmd");
        let base = root.join("base");
        let project = root.join("project");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(base.join("map")).unwrap();
        std::fs::write(base.join("map/provinces.bmp"), b"x").unwrap();
        std::fs::create_dir_all(base.join("gfx/interface")).unwrap();
        // 3-frame, 16px strip.
        let strip = strip_dds_distinct(16, &[[10, 0, 0, 255], [0, 10, 0, 255], [0, 0, 10, 255]]);
        std::fs::write(base.join("gfx/interface/resources.dds"), &strip).unwrap();

        let src = write_temp_png("goods.png", 32, 32, [222, 111, 33, 255]);
        let res = import_icon(
            base.to_str().unwrap().to_string(),
            None,
            "trade_goods".into(),
            1,
            src.to_str().unwrap().to_string(),
            None,
        )
        .unwrap();
        assert_eq!(res.file, "gfx/interface/resources.dds");
        assert_eq!((res.frame_w, res.frame_h), (16, 16));

        let vfs = Vfs::new(base.to_str().unwrap(), None).unwrap();
        let edits = vec![TypedEdit::BinaryAsset {
            file: res.file.clone(),
            bytes: res.strip.clone(),
        }];
        apply_queue(&vfs, &project, &edits).unwrap();

        let saved = std::fs::read(project.join("gfx/interface/resources.dds")).unwrap();
        let after = gfx::decode_dds(&saved).unwrap();
        assert_eq!(cell_center(&after, 16, 1), [222, 111, 33, 255]);
        // Base install strip untouched.
        assert_eq!(std::fs::read(base.join("gfx/interface/resources.dds")).unwrap(), strip);
    }

    #[test]
    fn import_icon_chains_on_pending_strip() {
        // Two imports before a save: the second passes the first's strip as
        // `pending_strip`, and the result carries BOTH new tiles.
        let root = std::env::temp_dir().join("eu_toolkit_icons_test_import_chain");
        let base = root.join("base");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(base.join("map")).unwrap();
        std::fs::write(base.join("map/provinces.bmp"), b"x").unwrap();
        std::fs::create_dir_all(base.join("gfx/interface")).unwrap();
        let strip = strip_dds_distinct(16, &[[10, 0, 0, 255], [0, 10, 0, 255], [0, 0, 10, 255]]);
        std::fs::write(base.join("gfx/interface/resources.dds"), &strip).unwrap();

        let a = write_temp_png("a.png", 16, 16, [1, 2, 3, 255]);
        let b = write_temp_png("b.png", 16, 16, [250, 240, 230, 255]);
        let first = import_icon(
            base.to_str().unwrap().to_string(),
            None,
            "trade_goods".into(),
            0,
            a.to_str().unwrap().to_string(),
            None,
        )
        .unwrap();
        let second = import_icon(
            base.to_str().unwrap().to_string(),
            None,
            "trade_goods".into(),
            2,
            b.to_str().unwrap().to_string(),
            Some(first.strip.clone()),
        )
        .unwrap();
        let after = gfx::decode_dds(&second.strip).unwrap();
        assert_eq!(cell_center(&after, 16, 0), [1, 2, 3, 255]);
        assert_eq!(cell_center(&after, 16, 2), [250, 240, 230, 255]);
    }

    #[test]
    fn import_icon_rejects_unknown_kind() {
        let src = write_temp_png("k.png", 8, 8, [0, 0, 0, 255]);
        let err = import_icon(
            "nonexistent".into(),
            None,
            "banana".into(),
            0,
            src.to_str().unwrap().to_string(),
            None,
        );
        assert!(err.is_err());
    }

    #[test]
    fn anbennar_import_smoke() {
        // Anbennar replaces resources.dds with a BC7/DX10 strip our decoder can't
        // rewrite -> a trade-good import surfaces a clear error. Its religion strip
        // (BC1/2/3 or uncompressed) splices while preserving its (wider) frames.
        if !Path::new(INSTALL).join("map/provinces.bmp").is_file() || !Path::new(ANBENNAR).is_dir() {
            return;
        }
        let src = write_temp_png("anb.png", 40, 40, [200, 30, 90, 255]);
        let goods = import_icon(
            INSTALL.into(),
            Some(ANBENNAR.into()),
            "trade_goods".into(),
            5,
            src.to_str().unwrap().to_string(),
            None,
        );
        assert!(goods.is_err(), "BC7/DX10 goods strip must error clearly");

        // Religion strip: if it decodes, the splice preserves its frame count.
        if let Ok(res) = import_icon(
            INSTALL.into(),
            Some(ANBENNAR.into()),
            "religions".into(),
            3,
            src.to_str().unwrap().to_string(),
            None,
        ) {
            let vfs = Vfs::new(INSTALL, Some(ANBENNAR)).unwrap();
            let orig = gfx::decode_dds(&vfs.read("gfx/interface/icon_religion.dds").unwrap()).unwrap();
            let after = gfx::decode_dds(&res.strip).unwrap();
            assert_eq!((after.width, after.height), (orig.width, orig.height));
        }
    }

    #[test]
    fn anbennar_atlas_smoke() {
        if !Path::new(INSTALL).join("map/provinces.bmp").is_file() || !Path::new(ANBENNAR).is_dir()
        {
            return;
        }
        let vfs = Vfs::new(INSTALL, Some(ANBENNAR)).unwrap();
        let base = Vfs::new(INSTALL, None).unwrap();

        // The index is rebuilt from Anbennar's mod-overlaid common/ data.
        let goods_order = trade_good_order(&vfs);
        assert!(!goods_order.is_empty());
        assert!(!religion_icon_index(&vfs).is_empty());

        // Anbennar replaces resources.dds with a block-compressed (DX10) strip
        // our pure decoder can't read; without a base fallback that surfaces as
        // an error, and with one it recovers the vanilla strip PNG.
        assert!(icon_atlas(&vfs, None, "trade_goods").is_err());
        let goods = icon_atlas(&vfs, Some(&base), "trade_goods").unwrap();
        assert!(!goods.index.is_empty());
        assert_eq!(goods.count, 32); // vanilla strip via fallback
        // Mod-added goods keep their (definition-order) index even when the
        // strip has fewer frames — the documented positional gotcha.
        assert_eq!(goods.index.len(), goods_order.len());

        // Religion icons: Anbennar's strip may or may not be compressed; the
        // fallback path builds regardless.
        let rel = icon_atlas(&vfs, Some(&base), "religions").unwrap();
        assert!(!rel.index.is_empty());
    }
}
