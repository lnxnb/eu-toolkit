//! Sprint 14.4 — GFX sprite index + on-demand DDS/TGA→PNG serving.
//!
//! Parses `interface/*.gfx` `spriteTypes` through the [`Vfs`] into a
//! name → texturefile listing (mission icons, event pictures, …), and serves any
//! referenced texture as PNG. Unlike the Phase-0.7 atlas pipeline (which only
//! needed the uncompressed 32-bpp strips), individual mission icons and event
//! pictures are frequently **block-compressed**: vanilla event pictures are
//! DXT1 (BC1), Anbennar mission icons mix uncompressed / DXT1 / DXT3 / DXT5 (and
//! a stray DX10). So this module carries a compact BC1/BC2/BC3 decoder in
//! addition to the uncompressed path; BC7/other DX10 formats degrade honestly to
//! a clear "unsupported format" error rather than garbage pixels.

use std::collections::HashMap;

use image::{ExtendedColorType, ImageEncoder};

use crate::vfs::Vfs;

// ---------------------------------------------------------------------------
// Sprite index
// ---------------------------------------------------------------------------

/// One `spriteType` entry: its `name` and the (normalized) `texturefile` path.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Sprite {
    pub name: String,
    /// Game-relative texture path (`gfx//…` collapsed to `gfx/…`).
    pub texturefile: String,
}

/// Normalizes a `texturefile` path: backslashes → `/`, and the doubled `//`
/// separators the game writes (`gfx//interface//…`) collapsed to single `/`.
fn normalize_texture(path: &str) -> String {
    let unified = path.replace('\\', "/");
    let mut out = String::with_capacity(unified.len());
    let mut prev_slash = false;
    for c in unified.chars() {
        if c == '/' {
            if !prev_slash {
                out.push('/');
            }
            prev_slash = true;
        } else {
            out.push(c);
            prev_slash = false;
        }
    }
    out
}

/// Parses every `interface/*.gfx` `spriteTypes` block through the Vfs into a
/// name → texturefile map (later definitions / mod files win), preserving
/// first-seen order for a stable picker listing.
pub fn sprite_index(vfs: &Vfs) -> Vec<Sprite> {
    let mut order: Vec<String> = Vec::new();
    let mut index: HashMap<String, String> = HashMap::new();
    for (fname, path) in vfs.list_dir("interface") {
        if !fname.to_lowercase().ends_with(".gfx") {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let block = crate::paradox::parse(&String::from_utf8_lossy(&bytes));
        for (key, sprite_types) in block.key_blocks() {
            if key != "spriteTypes" {
                continue;
            }
            // Every child block (spriteType / frameAnimatedSpriteType / …) that
            // carries a name + texturefile is a sprite.
            for (_kind, sb) in sprite_types.key_blocks() {
                let Some(name) = sb.get_scalar("name") else {
                    continue;
                };
                let Some(tex) = sb
                    .get_scalar("texturefile")
                    .or_else(|| sb.get_scalar("textureFile"))
                else {
                    continue;
                };
                let name = name.to_string();
                if !index.contains_key(&name) {
                    order.push(name.clone());
                }
                index.insert(name, normalize_texture(tex));
            }
        }
    }
    order
        .into_iter()
        .map(|name| {
            let texturefile = index.remove(&name).unwrap_or_default();
            Sprite { name, texturefile }
        })
        .collect()
}

/// Serves the sprite index, optionally filtered to names starting with `prefix`
/// (case-insensitive), e.g. `mission_` or `GFX_mission_`.
#[tauri::command(async)]
pub fn get_sprite_index(
    install_path: String,
    mod_path: Option<String>,
    prefix_filter: Option<String>,
) -> Result<Vec<Sprite>, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    let mut sprites = sprite_index(&vfs);
    if let Some(prefix) = prefix_filter.filter(|p| !p.is_empty()) {
        let p = prefix.to_lowercase();
        sprites.retain(|s| s.name.to_lowercase().starts_with(&p));
    }
    Ok(sprites)
}

// ---------------------------------------------------------------------------
// DDS / TGA decoding
// ---------------------------------------------------------------------------

/// A decoded surface as tightly-packed RGBA8.
#[derive(Debug)]
pub struct Surface {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Which compressed layout a DDS uses (or uncompressed).
enum DdsFormat {
    Uncompressed,
    Bc1,
    Bc2,
    Bc3,
}

/// Expands a 5-6-5 packed color to 8-bit RGB.
fn rgb565(c: u16) -> [u8; 3] {
    let r = ((c >> 11) & 0x1f) as u32;
    let g = ((c >> 5) & 0x3f) as u32;
    let b = (c & 0x1f) as u32;
    [
        ((r * 255 + 15) / 31) as u8,
        ((g * 255 + 31) / 63) as u8,
        ((b * 255 + 15) / 31) as u8,
    ]
}

/// Decodes one 8-byte BC color sub-block into 16 RGBA pixels. `punchthrough`
/// (BC1 only) enables the 3-color + 1-bit-alpha mode when `c0 <= c1`; BC2/BC3
/// color blocks always use the opaque 4-color mode.
fn decode_color_block(block: &[u8], punchthrough: bool, out: &mut [[u8; 4]; 16]) {
    let c0 = u16::from_le_bytes([block[0], block[1]]);
    let c1 = u16::from_le_bytes([block[2], block[3]]);
    let e0 = rgb565(c0);
    let e1 = rgb565(c1);
    let mut pal = [[0u8; 4]; 4];
    pal[0] = [e0[0], e0[1], e0[2], 255];
    pal[1] = [e1[0], e1[1], e1[2], 255];
    if c0 > c1 || !punchthrough {
        for i in 0..3 {
            pal[2][i] = ((2 * e0[i] as u32 + e1[i] as u32) / 3) as u8;
            pal[3][i] = ((e0[i] as u32 + 2 * e1[i] as u32) / 3) as u8;
        }
        pal[2][3] = 255;
        pal[3][3] = 255;
    } else {
        for i in 0..3 {
            pal[2][i] = ((e0[i] as u32 + e1[i] as u32) / 2) as u8;
        }
        pal[2][3] = 255;
        pal[3] = [0, 0, 0, 0];
    }
    let idx = u32::from_le_bytes([block[4], block[5], block[6], block[7]]);
    for (p, px) in out.iter_mut().enumerate() {
        *px = pal[((idx >> (p * 2)) & 0x3) as usize];
    }
}

/// Decodes the 8-byte BC3/DXT5 alpha sub-block into 16 alpha values.
fn decode_bc3_alpha(block: &[u8], out: &mut [u8; 16]) {
    let a0 = block[0];
    let a1 = block[1];
    let mut alpha = [0u8; 8];
    alpha[0] = a0;
    alpha[1] = a1;
    if a0 > a1 {
        for i in 1..7 {
            alpha[i + 1] = (((7 - i as u32) * a0 as u32 + i as u32 * a1 as u32) / 7) as u8;
        }
    } else {
        for i in 1..5 {
            alpha[i + 1] = (((5 - i as u32) * a0 as u32 + i as u32 * a1 as u32) / 5) as u8;
        }
        alpha[6] = 0;
        alpha[7] = 255;
    }
    let bits = u64::from_le_bytes([
        block[2], block[3], block[4], block[5], block[6], block[7], 0, 0,
    ]);
    for (p, a) in out.iter_mut().enumerate() {
        *a = alpha[((bits >> (p * 3)) & 0x7) as usize];
    }
}

/// Decodes a block-compressed surface (BC1/BC2/BC3) starting at `data`.
fn decode_bc(data: &[u8], width: u32, height: u32, fmt: &DdsFormat) -> Result<Vec<u8>, String> {
    let block_bytes = match fmt {
        DdsFormat::Bc1 => 8,
        DdsFormat::Bc2 | DdsFormat::Bc3 => 16,
        DdsFormat::Uncompressed => unreachable!(),
    };
    let bw = width.div_ceil(4);
    let bh = height.div_ceil(4);
    let needed = (bw * bh) as usize * block_bytes;
    if data.len() < needed {
        return Err("truncated compressed DDS data".into());
    }
    let mut rgba = vec![0u8; (width * height * 4) as usize];
    let mut colors = [[0u8; 4]; 16];
    let mut alphas = [255u8; 16];
    for by in 0..bh {
        for bx in 0..bw {
            let off = ((by * bw + bx) as usize) * block_bytes;
            let blk = &data[off..off + block_bytes];
            match fmt {
                DdsFormat::Bc1 => {
                    decode_color_block(blk, true, &mut colors);
                    for (i, a) in alphas.iter_mut().enumerate() {
                        *a = colors[i][3];
                    }
                }
                DdsFormat::Bc2 => {
                    decode_color_block(&blk[8..16], false, &mut colors);
                    let abits = u64::from_le_bytes(blk[0..8].try_into().unwrap());
                    for (p, a) in alphas.iter_mut().enumerate() {
                        let nib = ((abits >> (p * 4)) & 0xf) as u8;
                        *a = nib * 17;
                    }
                }
                DdsFormat::Bc3 => {
                    decode_color_block(&blk[8..16], false, &mut colors);
                    decode_bc3_alpha(&blk[0..8], &mut alphas);
                }
                DdsFormat::Uncompressed => unreachable!(),
            }
            for py in 0..4 {
                for px in 0..4 {
                    let x = bx * 4 + px;
                    let y = by * 4 + py;
                    if x >= width || y >= height {
                        continue;
                    }
                    let p = (py * 4 + px) as usize;
                    let dst = ((y * width + x) * 4) as usize;
                    rgba[dst] = colors[p][0];
                    rgba[dst + 1] = colors[p][1];
                    rgba[dst + 2] = colors[p][2];
                    rgba[dst + 3] = alphas[p];
                }
            }
        }
    }
    Ok(rgba)
}

/// Decodes a DDS (uncompressed 32-bpp, or BC1/BC2/BC3, incl. a DX10 header) to
/// RGBA8. BC7/other DX10 formats return a clear "unsupported" error.
pub fn decode_dds(bytes: &[u8]) -> Result<Surface, String> {
    if bytes.len() < 128 || &bytes[0..4] != b"DDS " {
        return Err("not a DDS file (bad magic)".into());
    }
    let rd = |o: usize| u32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
    if rd(4) != 124 {
        return Err("unexpected DDS header size".into());
    }
    let height = rd(12);
    let width = rd(16);
    const DDPF_FOURCC: u32 = 0x4;
    let pf_flags = rd(80);
    let four_cc = &bytes[84..88];
    let rgb_bit_count = rd(88);

    let mut data_off = 128usize;
    let fmt = if pf_flags & DDPF_FOURCC != 0 {
        match four_cc {
            b"DXT1" => DdsFormat::Bc1,
            b"DXT2" | b"DXT3" => DdsFormat::Bc2,
            b"DXT4" | b"DXT5" => DdsFormat::Bc3,
            b"DX10" => {
                if bytes.len() < 148 {
                    return Err("truncated DX10 DDS header".into());
                }
                let dxgi = u32::from_le_bytes(bytes[128..132].try_into().unwrap());
                data_off = 148;
                match dxgi {
                    70..=72 => DdsFormat::Bc1,
                    73..=75 => DdsFormat::Bc2,
                    76..=78 => DdsFormat::Bc3,
                    _ => {
                        return Err(format!(
                            "unsupported DX10 DDS format (DXGI {dxgi}); BC7/other not decoded"
                        ))
                    }
                }
            }
            other => {
                return Err(format!(
                    "unsupported DDS fourCC {:?}",
                    String::from_utf8_lossy(other)
                ))
            }
        }
    } else {
        if rgb_bit_count != 32 {
            return Err(format!(
                "unsupported uncompressed DDS bit depth {rgb_bit_count} (need 32)"
            ));
        }
        DdsFormat::Uncompressed
    };

    let data = &bytes[data_off..];
    let rgba = match fmt {
        DdsFormat::Uncompressed => {
            let n = (width * height) as usize;
            if data.len() < n * 4 {
                return Err("truncated DDS pixel data".into());
            }
            let r_mask = rd(92);
            let g_mask = rd(96);
            let b_mask = rd(100);
            let a_mask = rd(104);
            let shift = |mask: u32| if mask == 0 { 0 } else { mask.trailing_zeros() };
            let (rs, gs, bs, as_) = (shift(r_mask), shift(g_mask), shift(b_mask), shift(a_mask));
            let mut out = vec![0u8; n * 4];
            for i in 0..n {
                let px = u32::from_le_bytes(data[i * 4..i * 4 + 4].try_into().unwrap());
                out[i * 4] = ((px & r_mask) >> rs) as u8;
                out[i * 4 + 1] = ((px & g_mask) >> gs) as u8;
                out[i * 4 + 2] = ((px & b_mask) >> bs) as u8;
                out[i * 4 + 3] = if a_mask == 0 {
                    255
                } else {
                    ((px & a_mask) >> as_) as u8
                };
            }
            out
        }
        _ => decode_bc(data, width, height, &fmt)?,
    };
    Ok(Surface {
        width,
        height,
        rgba,
    })
}

/// Decodes a sprite texture (`.dds` or `.tga`) to a [`Surface`].
fn decode_texture(rel: &str, bytes: &[u8]) -> Result<Surface, String> {
    let lower = rel.to_lowercase();
    if lower.ends_with(".dds") {
        decode_dds(bytes)
    } else if lower.ends_with(".tga") {
        // TGA has no magic bytes, so the format is set explicitly.
        let img = image::load_from_memory_with_format(bytes, image::ImageFormat::Tga)
            .map_err(|e| format!("Failed to decode TGA {rel}: {e}"))?
            .to_rgba8();
        let (w, h) = img.dimensions();
        Ok(Surface {
            width: w,
            height: h,
            rgba: img.into_raw(),
        })
    } else {
        Err(format!("unsupported texture format: {rel}"))
    }
}

fn encode_png_rgba(surface: &Surface) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    image::codecs::png::PngEncoder::new(&mut out)
        .write_image(
            &surface.rgba,
            surface.width,
            surface.height,
            ExtendedColorType::Rgba8,
        )
        .map_err(|e| format!("PNG encode failed: {e}"))?;
    Ok(out)
}

/// Serves one sprite (looked up by `name`) as PNG bytes. Resolves the sprite's
/// `texturefile` through the Vfs (mod overrides base) and decodes it.
#[tauri::command(async)]
pub fn get_sprite(
    install_path: String,
    mod_path: Option<String>,
    name: String,
) -> Result<tauri::ipc::Response, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    let sprite = sprite_index(&vfs)
        .into_iter()
        .find(|s| s.name == name)
        .ok_or_else(|| format!("Sprite not found: {name}"))?;
    let bytes = vfs.read(&sprite.texturefile)?;
    let surface = decode_texture(&sprite.texturefile, &bytes)?;
    let png = encode_png_rgba(&surface)?;
    Ok(tauri::ipc::Response::new(png))
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

    fn real_vfs() -> Option<Vfs> {
        Path::new(INSTALL)
            .join("map/provinces.bmp")
            .is_file()
            .then(|| Vfs::new(INSTALL, None).unwrap())
    }

    #[test]
    fn normalizes_double_slash_texture_paths() {
        assert_eq!(
            normalize_texture("gfx//interface//missions//x.dds"),
            "gfx/interface/missions/x.dds"
        );
        assert_eq!(normalize_texture("gfx\\flags\\FRA.tga"), "gfx/flags/FRA.tga");
    }

    /// A synthetic single-block DXT1 (BC1) DDS: 4×4, two colors, index pattern
    /// selecting c0 for the top row and c1 for the rest.
    fn synthetic_dxt1() -> Vec<u8> {
        let mut out = vec![0u8; 128];
        out[0..4].copy_from_slice(b"DDS ");
        let put = |o: usize, v: u32, b: &mut Vec<u8>| b[o..o + 4].copy_from_slice(&v.to_le_bytes());
        put(4, 124, &mut out);
        put(12, 4, &mut out); // height
        put(16, 4, &mut out); // width
        put(80, 0x4, &mut out); // DDPF_FOURCC
        out[84..88].copy_from_slice(b"DXT1");
        // Color block: c0 = pure red (565), c1 = pure blue (565), c0 > c1.
        let red: u16 = 0xF800;
        let blue: u16 = 0x001F;
        let mut blk = Vec::new();
        blk.extend_from_slice(&red.to_le_bytes());
        blk.extend_from_slice(&blue.to_le_bytes());
        // 16 two-bit indices: row0 = c0 (index 0), rows1-3 = c1 (index 1).
        // pixel p index = row*4+col; bits little-endian by pixel.
        let mut idx: u32 = 0;
        for p in 0..16u32 {
            let row = p / 4;
            let ci = if row == 0 { 0u32 } else { 1u32 };
            idx |= ci << (p * 2);
        }
        blk.extend_from_slice(&idx.to_le_bytes());
        out.extend_from_slice(&blk);
        out
    }

    #[test]
    fn bc1_decodes_to_expected_pixels() {
        let s = decode_dds(&synthetic_dxt1()).unwrap();
        assert_eq!((s.width, s.height), (4, 4));
        // Top-left pixel = c0 (red).
        assert_eq!(&s.rgba[0..3], &[255, 0, 0]);
        // Bottom-right pixel (row 3) = c1 (blue).
        let last = ((3 * 4 + 3) * 4) as usize;
        assert_eq!(&s.rgba[last..last + 3], &[0, 0, 255]);
        // Not a single flat color.
        let flat = s.rgba.chunks(4).all(|px| px[0..3] == s.rgba[0..3]);
        assert!(!flat, "BC1 decode is not all one color");
    }

    #[test]
    fn rejects_dx10_bc7() {
        let mut out = vec![0u8; 148];
        out[0..4].copy_from_slice(b"DDS ");
        let put = |o: usize, v: u32, b: &mut Vec<u8>| b[o..o + 4].copy_from_slice(&v.to_le_bytes());
        put(4, 124, &mut out);
        put(12, 4, &mut out);
        put(16, 4, &mut out);
        put(80, 0x4, &mut out);
        out[84..88].copy_from_slice(b"DX10");
        put(128, 98, &mut out); // DXGI_FORMAT_BC7_UNORM
        let err = decode_dds(&out).unwrap_err();
        assert!(err.contains("BC7") || err.contains("unsupported"));
    }

    #[test]
    fn vanilla_sprite_index_resolves_mission_icon() {
        let Some(vfs) = real_vfs() else { return };
        let sprites = sprite_index(&vfs);
        assert!(!sprites.is_empty());
        // Mission icon names use the mission_* / GFX prefix convention.
        let mission = sprites.iter().find(|s| s.name.starts_with("mission_"));
        assert!(mission.is_some(), "expected some mission_* sprites");
        // Its texturefile resolves through the Vfs and decodes to plausible pixels.
        let s = mission.unwrap();
        assert!(s.texturefile.ends_with(".dds"));
        let bytes = vfs.read(&s.texturefile).unwrap();
        let surface = decode_dds(&bytes).unwrap();
        assert!(surface.width > 0 && surface.height > 0);
        assert_eq!(surface.rgba.len(), (surface.width * surface.height * 4) as usize);
    }

    #[test]
    fn vanilla_event_picture_bc1_decodes_non_garbage() {
        let Some(vfs) = real_vfs() else { return };
        let sprites = sprite_index(&vfs);
        // Event pictures are DXT1 (BC1). Find one and decode it.
        let ep = sprites.iter().find(|s| s.name.ends_with("_eventPicture"));
        let Some(ep) = ep else { return };
        let bytes = vfs.read(&ep.texturefile).unwrap();
        // Only assert on a compressed one (some event pics are uncompressed).
        let surface = decode_dds(&bytes).unwrap();
        assert!(surface.width >= 64 && surface.height >= 64);
        // Not all one color (a real picture has variation).
        let first = &surface.rgba[0..3];
        let varied = surface.rgba.chunks(4).any(|px| px[0..3] != *first);
        assert!(varied, "decoded event picture is a flat color (garbage)");
    }

    #[test]
    fn anbennar_mission_icons_resolve_through_replace_path() {
        if !Path::new(INSTALL).join("map/provinces.bmp").is_file() || !Path::new(ANBENNAR).is_dir()
        {
            return;
        }
        let vfs = Vfs::new(INSTALL, Some(ANBENNAR)).unwrap();
        let sprites = sprite_index(&vfs);
        assert!(!sprites.is_empty());
        // Anbennar ships its own missionicons_*.gfx additively; at least one of
        // its custom mission sprites must resolve + decode (mix of BC1/3/5).
        let mut decoded_any = false;
        for s in sprites.iter().filter(|s| s.name.starts_with("mission_")).take(40) {
            if let Ok(bytes) = vfs.read(&s.texturefile) {
                if let Ok(surface) = decode_texture(&s.texturefile, &bytes) {
                    assert!(surface.width > 0 && surface.height > 0);
                    decoded_any = true;
                }
            }
        }
        assert!(decoded_any, "no Anbennar mission icon decoded");
    }
}
