//! Sprint 4 — create-country scaffold generation.
//!
//! One backend command, [`prepare_country_scaffold`], turns a capital province +
//! a name/adjective into **everything a new country needs to exist**, returned as
//! data. Nothing is written here: the frontend queues the returned edits as one
//! composite (undo) operation and they land on Save through
//! [`crate::edits::apply_queue`] exactly like every other edit.
//!
//! The returned [`ScaffoldEdit`] list is wire-identical to the frontend's
//! `TypedEdit` union (same internally-tagged camelCase JSON as
//! [`crate::edits::TypedEdit`]) so the frontend pushes them almost verbatim; the
//! `tests` here prove that by serializing the payload and re-applying it through
//! the real queue applier.
//!
//! What a country needs (all produced as one composite):
//!  - a unique 3-letter **tag** (avoiding base+mod tags and reserved patterns),
//!  - **tag registration** appended to a project-owned country_tags file,
//!  - a **common/countries file** (color, graphical_culture, name pools),
//!  - a **history/countries file** (government/tech/religion/culture/capital +
//!    a starting ruler),
//!  - **capital province edits** (owner/controller/add_core = the new tag),
//!  - **localisation** (name + adjective, UTF-8 BOM via the loc override path),
//!  - a generated **flag** TGA.

use std::collections::HashSet;

use crate::game_data;
use crate::paradox::{self, Block, Value};
use crate::vfs::Vfs;

// ---------------------------------------------------------------------------
// Wire types
// ---------------------------------------------------------------------------

/// One scaffold edit, serialized to the **exact** JSON shape the frontend's
/// `TypedEdit` union uses (and that [`crate::edits::TypedEdit`] deserializes):
/// internally tagged on `kind`, camelCase variant + field names. The frontend
/// pushes these near-verbatim into its pending-edit queue.
///
/// Only the variants create-country needs are modeled; the full union lives in
/// `edits.rs`.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ScaffoldEdit {
    /// Replace an existing scalar (e.g. the capital's `owner` when it's already
    /// owned). Last path element is the key.
    SetScalar {
        file: String,
        path: Vec<String>,
        value: String,
        quoted: bool,
    },
    /// Insert a pre-formatted statement into a block (capital cores / a first
    /// owner on an uncolonized province).
    InsertStatement {
        file: String,
        block_path: Vec<String>,
        statement: String,
    },
    /// Append raw text to the end of a file (the tag mapping line).
    AppendText { file: String, text: String },
    /// Create a brand-new game file wholesale (country + history files).
    CreateFile { file: String, text: String },
    /// Localisation override -> project loc file (UTF-8 BOM).
    LocOverride { key: String, value: String },
    /// Raw bytes for a binary asset (the generated flag TGA).
    BinaryAsset { file: String, bytes: Vec<u8> },
}

/// The full create-country payload: the ready-to-queue edit list plus the
/// display metadata the panel needs to auto-select and repaint the new country.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CountryScaffold {
    /// The generated, currently-unused 3-letter tag.
    pub tag: String,
    /// The map color (also baked into the country file and the flag).
    pub color: [u8; 3],
    /// The capital province id.
    pub capital_id: u32,
    /// The country name (as entered; goes to loc `TAG:`).
    pub name: String,
    /// The adjective (as entered; goes to loc `TAG_ADJ:`).
    pub adjective: String,
    /// Game-relative path of the created common/countries file.
    pub country_file: String,
    /// Game-relative path of the created history/countries file.
    pub history_file: String,
    /// Game-relative path of the created flag.
    pub flag_file: String,
    /// The composite, in queue order. Push verbatim as one undo unit.
    pub edits: Vec<ScaffoldEdit>,
}

// ---------------------------------------------------------------------------
// Tag generation
// ---------------------------------------------------------------------------

/// The project-owned tag registration file (append target). `zz_` prefix so it
/// collates last, mirroring the loc override convention.
pub const TAG_FILE: &str = "common/country_tags/zz_eutoolkit_countries.txt";

/// Tags the game reserves for fixed special countries. The runtime-generated
/// dynamic tags (colonial nations, client states, trade leagues, custom
/// nations, …) all match the `[A-Z][0-9][0-9]` pattern — see
/// [`is_reserved_pattern`] — and are handled separately.
const RESERVED_TAGS: &[&str] = &["REB", "PIR", "NAT", "AUX"];

/// True for the `[A-Z][0-9][0-9]` shape the game reserves for its dynamically
/// created countries. Our generated candidates are always three letters, so this
/// never fires on them; it exists so a candidate can never collide with the
/// reserved dynamic space even in principle.
fn is_reserved_pattern(tag: &str) -> bool {
    let b = tag.as_bytes();
    b.len() == 3
        && b[0].is_ascii_uppercase()
        && b[1].is_ascii_digit()
        && b[2].is_ascii_digit()
}

/// Every country tag defined in base+mod (the keys of every
/// `common/country_tags/*.txt`, uppercased). With a mod loaded the merged Vfs
/// listing already includes the mod's tags (e.g. Anbennar's hundreds), so a
/// generated tag is guaranteed distinct from them for free.
pub fn used_tags(vfs: &Vfs) -> HashSet<String> {
    let mut set = HashSet::new();
    for (name, path) in vfs.list_dir("common/country_tags") {
        if !name.to_lowercase().ends_with(".txt") {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let block = paradox::parse(&String::from_utf8_lossy(&bytes));
        for (k, v) in &block.items {
            if let (Some(k), Value::Scalar(_)) = (k, v) {
                set.insert(k.to_uppercase());
            }
        }
    }
    set
}

/// Whether `tag` is a legal, unused, non-reserved 3-uppercase-letter tag.
fn tag_available(tag: &str, used: &HashSet<String>) -> bool {
    tag.len() == 3
        && tag.bytes().all(|b| b.is_ascii_uppercase())
        && !RESERVED_TAGS.contains(&tag)
        && !is_reserved_pattern(tag)
        && !used.contains(tag)
}

/// Uppercase ASCII letters of a string, in order.
fn letters_of(s: &str) -> Vec<char> {
    s.chars()
        .filter(|c| c.is_ascii_alphabetic())
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

const VOWELS: &[char] = &['A', 'E', 'I', 'O', 'U'];

/// Name-derived tag candidates, tried in order before the systematic fallback:
/// first three letters, word initials, first-letter + consonants, and the
/// leading consonants. Only well-formed 3-letter candidates are emitted.
fn name_candidates(name: &str) -> Vec<String> {
    let letters = letters_of(name);
    let words: Vec<Vec<char>> = name
        .split_whitespace()
        .map(letters_of)
        .filter(|w| !w.is_empty())
        .collect();
    let mut out: Vec<String> = Vec::new();
    let mut push = |c: Vec<char>| {
        if c.len() == 3 {
            out.push(c.into_iter().collect());
        }
    };

    // 1. First three letters ("Newland" -> NEW).
    if letters.len() >= 3 {
        push(letters[..3].to_vec());
    }
    // 2. Word initials ("United Provinces of X" -> UPO/UPX); pad a 2-word name
    //    with the second word's next letter.
    if words.len() >= 3 {
        push(words.iter().take(3).map(|w| w[0]).collect());
    } else if words.len() == 2 {
        let mut c = vec![words[0][0], words[1][0]];
        c.push(*words[1].get(1).unwrap_or(&words[0].get(1).copied().unwrap_or(words[0][0])));
        push(c);
    }
    // 3. First letter + the next two consonants ("Kingdom" -> KNG).
    if let Some(&first) = letters.first() {
        let mut c = vec![first];
        c.extend(
            letters
                .iter()
                .skip(1)
                .filter(|ch| !VOWELS.contains(ch))
                .take(2),
        );
        push(c);
    }
    // 4. Leading consonants ("Newland" -> NWL).
    let cons: Vec<char> = letters.iter().copied().filter(|c| !VOWELS.contains(c)).collect();
    if cons.len() >= 3 {
        push(cons[..3].to_vec());
    }
    out
}

/// Picks an unused, non-reserved 3-letter tag: name-derived candidates first,
/// then a deterministic `AAA…ZZZ` sweep. `None` only if all 17,576 three-letter
/// combinations are taken (never in practice).
pub fn generate_tag(name: &str, used: &HashSet<String>) -> Option<String> {
    for cand in name_candidates(name) {
        if tag_available(&cand, used) {
            return Some(cand);
        }
    }
    for a in b'A'..=b'Z' {
        for b in b'A'..=b'Z' {
            for c in b'A'..=b'Z' {
                let t = String::from_utf8(vec![a, b, c]).unwrap();
                if tag_available(&t, used) {
                    return Some(t);
                }
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Color
// ---------------------------------------------------------------------------

fn dist2(a: [u8; 3], b: [u8; 3]) -> i64 {
    (0..3)
        .map(|i| {
            let d = a[i] as i64 - b[i] as i64;
            d * d
        })
        .sum()
}

/// A color visually distinct from every existing country color: of many
/// `hash_color`-derived candidates (seeded by `seed` for stability + variety),
/// the one whose nearest existing color is farthest away (max-min distance).
fn distinct_color(existing: &[[u8; 3]], seed: &str) -> [u8; 3] {
    let mut best = crate::map_renderer::hash_color(seed);
    let mut best_d = i64::MIN;
    for i in 0..256 {
        let c = crate::map_renderer::hash_color(&format!("{seed}#{i}"));
        let d = existing
            .iter()
            .map(|e| dist2(c, *e))
            .min()
            .unwrap_or(i64::MAX);
        if d > best_d {
            best_d = d;
            best = c;
        }
    }
    best
}

// ---------------------------------------------------------------------------
// Flag
// ---------------------------------------------------------------------------

/// A 128x128 uncompressed 24-bit TGA (image type 2 — same format vanilla flags
/// and [`game_data::convert_flag`] use): a horizontal two-tone split of the
/// country color (upper two-thirds full, lower third darkened) so the game and
/// the panel both render something recognizable.
fn generate_flag_tga(color: [u8; 3]) -> Result<Vec<u8>, String> {
    use image::codecs::tga::TgaEncoder;
    use image::{ExtendedColorType, ImageEncoder};

    const W: u32 = 128;
    const H: u32 = 128;
    let darker = [
        (color[0] as u16 * 6 / 10) as u8,
        (color[1] as u16 * 6 / 10) as u8,
        (color[2] as u16 * 6 / 10) as u8,
    ];
    let split = H * 2 / 3;
    let mut raw = Vec::with_capacity((W * H * 3) as usize);
    for y in 0..H {
        let band = if y < split { color } else { darker };
        for _ in 0..W {
            raw.extend_from_slice(&band);
        }
    }
    let mut tga = Vec::new();
    TgaEncoder::new(&mut tga)
        .disable_rle()
        .write_image(&raw, W, H, ExtendedColorType::Rgb8)
        .map_err(|e| format!("Failed to encode flag TGA: {e}"))?;
    Ok(tga)
}

// ---------------------------------------------------------------------------
// Inheritance / defaults
// ---------------------------------------------------------------------------

/// The vanilla tier-1 default government reform for a government type. Every
/// government must carry a reform; these four are the universal defaults present
/// in the base game (and therefore in any total conversion built on it), so they
/// load with zero manual fixes regardless of the inherited government.
fn default_reform(government: &str) -> Option<&'static str> {
    match government {
        "monarchy" => Some("feudalism_reform"),
        "republic" => Some("oligarchy_reform"),
        "theocracy" => Some("leading_clergy_reform"),
        "tribal" => Some("tribal_despotism"),
        _ => None,
    }
}

/// Technology group fallback when there is no owner to inherit from, keyed off
/// the capital's religion (the stated default is western — see SPRINT 4.2).
fn default_tech_group(religion: Option<&str>) -> &'static str {
    match religion {
        Some("orthodox") => "eastern",
        Some("sunni") | Some("shiite") | Some("ibadi") => "muslim",
        _ => "western",
    }
}

/// What the new country inherits for its government block.
struct GovProfile {
    government: String,
    reform: String,
    tech_group: String,
    /// Only when the owner explicitly overrides it (rare); else the game derives
    /// unit type from the tech group.
    unit_type: Option<String>,
}

/// Derives the government profile from the capital's current owner's history
/// file, falling back to monarchy + a religion-derived tech group. Only known
/// vanilla government types are inherited (so [`default_reform`] always resolves
/// to a valid reform); anything exotic falls back to monarchy.
fn gov_profile(vfs: &Vfs, owner: Option<&str>, religion: Option<&str>) -> GovProfile {
    let owner_block = owner
        .and_then(|tag| game_data::country_history_file(vfs, tag))
        .map(|(_, bytes)| paradox::parse(&String::from_utf8_lossy(&bytes)));

    let owner_gov = owner_block
        .as_ref()
        .and_then(|b| b.get_scalar("government"))
        .filter(|g| default_reform(g).is_some())
        .map(str::to_string);
    let government = owner_gov.unwrap_or_else(|| "monarchy".to_string());
    let reform = default_reform(&government).unwrap_or("feudalism_reform").to_string();

    let tech_group = owner_block
        .as_ref()
        .and_then(|b| b.get_scalar("technology_group"))
        .map(str::to_string)
        .unwrap_or_else(|| default_tech_group(religion).to_string());

    let unit_type = owner_block
        .as_ref()
        .and_then(|b| b.get_scalar("unit_type"))
        .map(str::to_string);

    GovProfile {
        government,
        reform,
        tech_group,
        unit_type,
    }
}

// ---------------------------------------------------------------------------
// Name pools
// ---------------------------------------------------------------------------

const FALLBACK_MALE: &[&str] = &[
    "John", "William", "Henry", "Charles", "Robert", "Edward", "Philip", "Louis", "Frederick",
    "George", "Albert", "Otto",
];
const FALLBACK_DYNASTY: &[&str] = &[
    "von Hohenberg",
    "de la Marche",
    "Aldenburg",
    "Marsend",
    "Reinhardt",
    "Falkenrath",
];

/// Male names + dynasty names for the new country, drawn from the capital
/// culture's pools (culture-level, falling back to the group's) where available,
/// with generic-but-valid fallbacks so ruler generation always works.
struct NamePools {
    graphical_culture: String,
    male: Vec<String>,
    dynasties: Vec<String>,
}

fn name_pools(vfs: &Vfs, culture: Option<&str>) -> NamePools {
    // culture_details resolves the culture's group graphical_culture and the
    // culture/group name pools; if the capital has no (or an unknown) culture we
    // fall back to generic pools + westerngfx.
    let details = culture.and_then(|key| {
        let loc = crate::loc::build(vfs);
        game_data::culture_details(vfs, &loc, key).ok()
    });

    let (graphical_culture, mut male, mut dynasties) = match details {
        Some(d) => {
            let male = if !d.male_names.is_empty() {
                d.male_names
            } else {
                d.group_male_names
            };
            let dynasties = if !d.dynasty_names.is_empty() {
                d.dynasty_names
            } else {
                d.group_dynasty_names
            };
            (
                d.group_graphical_culture.unwrap_or_else(|| "westerngfx".to_string()),
                male,
                dynasties,
            )
        }
        None => ("westerngfx".to_string(), Vec::new(), Vec::new()),
    };

    if male.is_empty() {
        male = FALLBACK_MALE.iter().map(|s| s.to_string()).collect();
    }
    if dynasties.is_empty() {
        dynasties = FALLBACK_DYNASTY.iter().map(|s| s.to_string()).collect();
    }
    NamePools {
        graphical_culture,
        male,
        dynasties,
    }
}

/// Deterministic index into a pool from a seed string (so tests are stable and
/// re-running a create yields the same ruler).
fn seeded_index(seed: &str, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    let mut h: u32 = 2166136261;
    for b in seed.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(16777619);
    }
    (h as usize) % len
}

// ---------------------------------------------------------------------------
// Scaffold text
// ---------------------------------------------------------------------------

/// A filename-safe stem for the country/history files (ASCII alnum, spaces,
/// `-`/`_`), falling back to the tag when nothing usable remains. The country
/// file stem must match the path in the tag mapping, so it is computed once and
/// reused for both.
fn safe_stem(name: &str, tag: &str) -> String {
    let filtered: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '-' | '_'))
        .collect();
    let collapsed = filtered.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = collapsed.trim();
    if trimmed.is_empty() {
        tag.to_string()
    } else {
        trimmed.to_string()
    }
}

/// The `common/countries/<stem>.txt` body: graphical_culture, color, revolutionary
/// colors, and starter name pools (monarch_names with weights, leader/ship/army/
/// fleet names). Mirrors vanilla structure (e.g. Ulm.txt).
fn country_file_text(color: [u8; 3], pools: &NamePools, adjective: &str) -> String {
    let [r, g, b] = color;
    let mut s = String::new();
    s.push_str(&format!("graphical_culture = {}\n\n", pools.graphical_culture));
    s.push_str(&format!("color = {{ {r} {g} {b} }}\n"));
    s.push_str(&format!("revolutionary_colors = {{ {r} {g} {b} }}\n\n"));

    // monarch_names: up to 12 male names as regnal entries with descending
    // weights. `"Name #0" = weight` — #0 is the regnal-number placeholder, the
    // integer is the relative draw weight.
    s.push_str("monarch_names = {\n");
    let count = pools.male.len().min(12).max(1);
    for (i, name) in pools.male.iter().take(count).enumerate() {
        let weight = 100i32.saturating_sub(i as i32 * 8).max(10);
        s.push_str(&format!("\t\"{name} #0\" = {weight}\n"));
    }
    s.push_str("}\n\n");

    // leader_names from dynasty pool; ship/army/fleet names from generic pools.
    s.push_str("leader_names = {\n");
    for name in pools.dynasties.iter().take(12) {
        // Multi-word (e.g. "von X") entries must be quoted.
        if name.contains(' ') {
            s.push_str(&format!("\t\"{name}\"\n"));
        } else {
            s.push_str(&format!("\t{name}\n"));
        }
    }
    s.push_str("}\n\n");

    s.push_str("ship_names = {\n");
    for name in pools.male.iter().take(10) {
        if name.contains(' ') {
            s.push_str(&format!("\t\"{name}\"\n"));
        } else {
            s.push_str(&format!("\t{name}\n"));
        }
    }
    s.push_str("}\n\n");

    s.push_str("army_names = {\n\t\"Army of $PROVINCE$\"\n}\n\n");
    s.push_str(&format!("fleet_names = {{\n\t\"{adjective} Fleet\"\n}}\n"));
    s
}

/// The `history/countries/<TAG> - <stem>.txt` body: government block, religion +
/// primary culture, capital, and a starting ruler dated block at the 1444 start.
#[allow(clippy::too_many_arguments)]
fn history_file_text(
    gov: &GovProfile,
    religion: &str,
    culture: &str,
    capital_id: u32,
    ruler_name: &str,
    dynasty: &str,
) -> String {
    let mut s = String::new();
    s.push_str(&format!("government = {}\n", gov.government));
    s.push_str(&format!("add_government_reform = {}\n", gov.reform));
    s.push_str("government_rank = 1\n");
    s.push_str(&format!("technology_group = {}\n", gov.tech_group));
    if let Some(unit) = &gov.unit_type {
        s.push_str(&format!("unit_type = {unit}\n"));
    }
    s.push_str(&format!("religion = {religion}\n"));
    s.push_str(&format!("primary_culture = {culture}\n"));
    s.push_str(&format!("capital = {capital_id}\n\n"));

    // Starting ruler: a dated block at the 1444 start date (<= 1444.11.11 is
    // read as the initial ruler, matching how vanilla dated monarch blocks work).
    s.push_str("1444.11.11 = {\n");
    s.push_str("\tmonarch = {\n");
    s.push_str(&format!("\t\tname = \"{ruler_name}\"\n"));
    s.push_str(&format!("\t\tdynasty = \"{dynasty}\"\n"));
    s.push_str("\t\tbirth_date = 1410.1.1\n");
    s.push_str("\t\tadm = 3\n");
    s.push_str("\t\tdip = 3\n");
    s.push_str("\t\tmil = 3\n");
    s.push_str("\t}\n");
    s.push_str("}\n");
    s
}

// ---------------------------------------------------------------------------
// Command
// ---------------------------------------------------------------------------

/// Builds the full create-country composite for `capital_id`. Reads through the
/// Vfs (base + optional mod); writes nothing — the returned [`CountryScaffold`]
/// is queued and applied by the frontend on Save.
///
/// Errors if the capital is not a land province (water/wasteland) or does not
/// exist in the map definitions.
///
/// `exclude_tags` names tags that are unavailable beyond those on disk — the
/// frontend passes tags already claimed by pending (unsaved) country creates so
/// that scaffolding several countries before a single Save never collides.
#[tauri::command(async)]
pub fn prepare_country_scaffold(
    install_path: String,
    mod_path: Option<String>,
    capital_id: u32,
    name: String,
    adjective: String,
    exclude_tags: Option<Vec<String>>,
) -> Result<CountryScaffold, String> {
    let vfs = Vfs::new(&install_path, mod_path.as_deref())?;
    let exclude: HashSet<String> = exclude_tags
        .unwrap_or_default()
        .into_iter()
        .map(|t| t.to_uppercase())
        .collect();
    build_scaffold(&vfs, capital_id, &name, &adjective, &exclude)
}

/// The scaffold builder, split out so tests can drive it with a synthetic Vfs.
/// `exclude_tags` (uppercased) are treated as already-used, on top of the
/// base+mod tags on disk — so pending, unsaved creates can't be re-issued.
pub fn build_scaffold(
    vfs: &Vfs,
    capital_id: u32,
    name: &str,
    adjective: &str,
    exclude_tags: &HashSet<String>,
) -> Result<CountryScaffold, String> {
    // --- Locate + validate the capital province. -------------------------
    let provinces = game_data::province_political(vfs);
    let capital = provinces
        .iter()
        .find(|p| p.id == capital_id)
        .ok_or_else(|| format!("Capital province {capital_id} not found in map definitions"))?;
    if capital.water {
        return Err("Capital must be a land province (a water province was selected)".to_string());
    }
    if capital.wasteland {
        return Err("Capital must be a land province (an impassable wasteland was selected)".to_string());
    }

    // Capital's culture/religion/owner: from its history file (may not exist for
    // an uncolonized, fileless province — then None, and we fall back).
    let capital_block: Option<Block> = vfs
        .resolve(&capital.file)
        .and_then(|p| std::fs::read(&p).ok())
        .map(|b| paradox::parse(&String::from_utf8_lossy(&b)));
    let cap_religion = capital_block
        .as_ref()
        .and_then(|b| b.get_scalar("religion"))
        .map(str::to_string);
    let cap_culture = capital_block
        .as_ref()
        .and_then(|b| b.get_scalar("culture"))
        .map(str::to_string);

    let religion = cap_religion.clone().unwrap_or_else(|| "catholic".to_string());

    // --- Tag. -------------------------------------------------------------
    // On-disk tags plus any pending-create tags the caller wants excluded.
    let mut used = used_tags(vfs);
    used.extend(exclude_tags.iter().cloned());
    let tag = generate_tag(name, &used)
        .ok_or_else(|| "No unused 3-letter tag is available".to_string())?;

    // --- Color, distinct from all existing country colors. ---------------
    let existing_colors: Vec<[u8; 3]> = game_data::country_colors(vfs).into_values().collect();
    let color = distinct_color(&existing_colors, &tag);

    // --- Inheritance + name pools. ---------------------------------------
    let gov = gov_profile(vfs, capital.owner.as_deref(), cap_religion.as_deref());
    let pools = name_pools(vfs, cap_culture.as_deref());
    // primary_culture must reference a real culture; if the capital had none, we
    // still need a value — reuse the capital's culture if present, else a
    // generic vanilla culture that carries name pools.
    let primary_culture = cap_culture.clone().unwrap_or_else(|| "saxon".to_string());
    let ruler_name = pools.male[seeded_index(&tag, pools.male.len())].clone();
    let dynasty = pools.dynasties[seeded_index(&format!("{tag}d"), pools.dynasties.len())].clone();

    // --- File paths. ------------------------------------------------------
    let stem = safe_stem(name, &tag);
    let country_file = format!("common/countries/{stem}.txt");
    let history_file = format!("history/countries/{tag} - {stem}.txt");
    let flag_file = format!("gfx/flags/{tag}.tga");

    // --- Build the composite. --------------------------------------------
    let mut edits: Vec<ScaffoldEdit> = Vec::new();

    // 1. Tag registration (appended to the project-owned file). Vanilla format:
    //    `TAG = "countries/<stem>.txt"` (path relative to common/, forward slash).
    edits.push(ScaffoldEdit::AppendText {
        file: TAG_FILE.to_string(),
        text: format!("{tag} = \"countries/{stem}.txt\"\n"),
    });

    // 2. Country file.
    edits.push(ScaffoldEdit::CreateFile {
        file: country_file.clone(),
        text: country_file_text(color, &pools, adjective),
    });

    // 3. History file.
    edits.push(ScaffoldEdit::CreateFile {
        file: history_file.clone(),
        text: history_file_text(&gov, &religion, &primary_culture, capital_id, &ruler_name, &dynasty),
    });

    // 4. Capital province: owner/controller = TAG (replace if present, else
    //    insert), and a core. Same shapes as the 1.4 add-province tool.
    match capital.owner {
        Some(_) => edits.push(ScaffoldEdit::SetScalar {
            file: capital.file.clone(),
            path: vec!["owner".to_string()],
            value: tag.clone(),
            quoted: false,
        }),
        None => edits.push(ScaffoldEdit::InsertStatement {
            file: capital.file.clone(),
            block_path: vec![],
            statement: format!("owner = {tag}"),
        }),
    }
    match capital.controller {
        Some(_) => edits.push(ScaffoldEdit::SetScalar {
            file: capital.file.clone(),
            path: vec!["controller".to_string()],
            value: tag.clone(),
            quoted: false,
        }),
        None => edits.push(ScaffoldEdit::InsertStatement {
            file: capital.file.clone(),
            block_path: vec![],
            statement: format!("controller = {tag}"),
        }),
    }
    edits.push(ScaffoldEdit::InsertStatement {
        file: capital.file.clone(),
        block_path: vec![],
        statement: format!("add_core = {tag}"),
    });

    // 5. Localisation (name + adjective). UTF-8 BOM is handled by the loc writer.
    edits.push(ScaffoldEdit::LocOverride {
        key: tag.clone(),
        value: name.to_string(),
    });
    edits.push(ScaffoldEdit::LocOverride {
        key: format!("{tag}_ADJ"),
        value: adjective.to_string(),
    });

    // 6. Flag.
    edits.push(ScaffoldEdit::BinaryAsset {
        file: flag_file.clone(),
        bytes: generate_flag_tga(color)?,
    });

    Ok(CountryScaffold {
        tag,
        color,
        capital_id,
        name: name.to_string(),
        adjective: adjective.to_string(),
        country_file,
        history_file,
        flag_file,
        edits,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edits::{self, TypedEdit};
    use std::path::{Path, PathBuf};

    const INSTALL: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Europa Universalis IV";
    const ANBENNAR: &str = r"C:\Users\User\projects\anbennar-eu4-dev";

    // --- Tag generation (pure, no Vfs) -----------------------------------

    #[test]
    fn reserved_patterns_and_tags_skipped() {
        let used = HashSet::new();
        assert!(is_reserved_pattern("C01"));
        assert!(is_reserved_pattern("Z99"));
        assert!(!is_reserved_pattern("ABC"));
        assert!(!tag_available("REB", &used));
        assert!(!tag_available("PIR", &used));
        assert!(!tag_available("NAT", &used));
        assert!(!tag_available("AUX", &used));
        assert!(!tag_available("C01", &used)); // dynamic pattern
        assert!(!tag_available("ab", &used)); // wrong length
        assert!(!tag_available("A1B", &used)); // not all letters
        assert!(tag_available("ABC", &used));
    }

    #[test]
    fn generate_tag_prefers_name() {
        let used = HashSet::new();
        assert_eq!(generate_tag("Newland", &used).as_deref(), Some("NEW"));
    }

    #[test]
    fn generate_tag_falls_through_candidates() {
        // NEW taken -> word-initial candidate can't form (one word) -> first
        // letter + consonants "NWL" -> leading consonants "NWL" (same). Ensure a
        // usable candidate or the sweep yields *some* valid, unused tag.
        let mut used = HashSet::new();
        used.insert("NEW".to_string());
        let t = generate_tag("Newland", &used).unwrap();
        assert!(tag_available(&t, &used));
        assert_ne!(t, "NEW");
    }

    #[test]
    fn generate_tag_exhaustion_finds_the_last_free() {
        // Fill every AAA..ZZZ except one; a digitless name has no candidates, so
        // the systematic sweep must land on the single free tag.
        let mut used = HashSet::new();
        for a in b'A'..=b'Z' {
            for b in b'A'..=b'Z' {
                for c in b'A'..=b'Z' {
                    used.insert(String::from_utf8(vec![a, b, c]).unwrap());
                }
            }
        }
        used.remove("QZX");
        assert_eq!(generate_tag("12345", &used).as_deref(), Some("QZX"));
    }

    #[test]
    fn distinct_color_avoids_existing() {
        // With black + white present, the picked color is meaningfully far from
        // both (max-min distance beats a trivially-adjacent pick).
        let existing = vec![[0, 0, 0], [255, 255, 255]];
        let c = distinct_color(&existing, "ABC");
        let nearest = existing.iter().map(|e| dist2(c, *e)).min().unwrap();
        assert!(nearest > 2000, "picked color too close to an existing one: {c:?} d={nearest}");
    }

    #[test]
    fn flag_tga_has_expected_header_and_dims() {
        let tga = generate_flag_tga([12, 34, 56]).unwrap();
        // 18-byte TGA header: [2]=image type 2 (uncompressed truecolor),
        // [12..14]=width LE, [14..16]=height LE, [16]=pixel depth.
        assert!(tga.len() > 18 + 128 * 128 * 3 - 1);
        assert_eq!(tga[2], 2, "image type must be uncompressed truecolor");
        let w = u16::from_le_bytes([tga[12], tga[13]]);
        let h = u16::from_le_bytes([tga[14], tga[15]]);
        assert_eq!((w, h), (128, 128));
        assert_eq!(tga[16], 24, "must be 24-bit");
    }

    #[test]
    fn safe_stem_sanitizes() {
        assert_eq!(safe_stem("New Country", "ABC"), "New Country");
        assert_eq!(safe_stem("  Wei\u{00df}rus\u{00df}?? ", "ABC"), "Weirus"); // ß dropped, spaces collapse
        assert_eq!(safe_stem("!!!", "ABC"), "ABC"); // nothing usable -> tag
    }

    // --- Synthetic install round-trips -----------------------------------

    /// Builds a minimal synthetic base install with one owned capital province
    /// (owner FRA), FRA + culture + tag files. Returns (base_dir, vfs).
    fn synthetic(name: &str, capital_owned: bool) -> (PathBuf, Vfs) {
        let root = std::env::temp_dir().join(format!("eu_toolkit_ccreate_test_{name}"));
        let _ = std::fs::remove_dir_all(&root);
        let w = |rel: &str, bytes: &[u8]| {
            let p = root.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, bytes).unwrap();
        };
        w("map/provinces.bmp", b"x");
        // Two provinces: 10 (the capital, land), 99 (a sea).
        w(
            "map/definition.csv",
            b"province;red;green;blue;name;x\n10;10;20;30;Testburg;x\n99;0;0;255;Ocean;x\n",
        );
        w("map/default.map", b"sea_starts = { 99 }\nlakes = { }\n");
        w("map/climate.txt", b"tropical = { 10 }\nimpassable = { }\n");

        if capital_owned {
            w(
                "history/provinces/10 - Testburg.txt",
                b"owner = FRA\ncontroller = FRA\nadd_core = FRA\nculture = swabian\nreligion = catholic\ntrade_goods = cloth\nbase_tax = 3\n",
            );
        } else {
            // Uncolonized: no owner/controller, but culture/religion present (as
            // vanilla uncolonized land carries, for colonization).
            w(
                "history/provinces/10 - Testburg.txt",
                b"culture = swabian\nreligion = catholic\ntrade_goods = cloth\nbase_tax = 3\n",
            );
        }

        w(
            "common/country_tags/00_countries.txt",
            b"REB = \"countries/Rebels.txt\"\nFRA = \"countries/France.txt\"\nNEW = \"countries/Taken.txt\"\n",
        );
        w("common/countries/France.txt", b"color = { 20 20 200 }\ngraphical_culture = westerngfx\n");
        w(
            "history/countries/FRA - France.txt",
            b"government = republic\ngovernment_rank = 2\ntechnology_group = western\nreligion = catholic\nprimary_culture = cosmopolitan_french\ncapital = 183\n",
        );
        // A culture group with graphical_culture + swabian culture pools.
        w(
            "common/cultures/00_cultures.txt",
            b"germanic = {\n\tgraphical_culture = westerngfx\n\tmale_names = { Gunther Rolf }\n\tswabian = {\n\t\tmale_names = { Friedrich Ludwig Ruprecht }\n\t\tdynasty_names = { \"von Hohenberg\" Habsburg }\n\t}\n}\n",
        );

        let vfs = Vfs::new(root.to_str().unwrap(), None).unwrap();
        (root, vfs)
    }

    /// Applies the scaffold's edits through the *real* queue applier, proving the
    /// payload is wire-identical to `TypedEdit` and lands correctly.
    fn apply(scaffold: &CountryScaffold, project: &Path, vfs: &Vfs) -> Vec<String> {
        let json = serde_json::to_string(&scaffold.edits).unwrap();
        let typed: Vec<TypedEdit> = serde_json::from_str(&json).unwrap();
        edits::apply_queue(vfs, project, &typed).unwrap()
    }

    #[test]
    fn scaffold_on_owned_capital_round_trips() {
        let (root, vfs) = synthetic("owned", true);
        let project = root.join("project");
        let scaffold = build_scaffold(&vfs, 10, "Newland", "Newlandish", &HashSet::new()).unwrap();

        // NEW is taken in the tag file -> a different, valid, unused tag.
        assert!(tag_available(&scaffold.tag, &used_tags(&vfs)));
        assert_ne!(scaffold.tag, "NEW");
        let tag = scaffold.tag.clone();

        let written = apply(&scaffold, &project, &vfs);

        // Tag registration file exists and maps the tag.
        let tags = std::fs::read_to_string(project.join(TAG_FILE)).unwrap();
        assert!(tags.contains(&format!("{tag} = \"countries/Newland.txt\"")));

        // Country file: parses back, has color/graphical_culture/monarch_names.
        let cty_bytes = std::fs::read(project.join(&scaffold.country_file)).unwrap();
        let cty = paradox::parse(&String::from_utf8_lossy(&cty_bytes));
        assert!(cty.get_block("color").is_some());
        assert_eq!(cty.get_scalar("graphical_culture"), Some("westerngfx"));
        assert!(cty.get_block("monarch_names").is_some());

        // History file: parses back, required keys + a monarch with name+dynasty.
        let hist_bytes = std::fs::read(project.join(&scaffold.history_file)).unwrap();
        let hist = paradox::parse(&String::from_utf8_lossy(&hist_bytes));
        assert!(hist.get_scalar("government").is_some());
        assert!(hist.get_scalar("technology_group").is_some());
        assert_eq!(hist.get_scalar("religion"), Some("catholic"));
        assert_eq!(hist.get_scalar("primary_culture"), Some("swabian"));
        assert_eq!(hist.get_scalar("capital"), Some("10"));
        let ruler = hist
            .get_block("1444.11.11")
            .and_then(|b| b.get_block("monarch"))
            .expect("a starting monarch block");
        assert!(ruler.get_scalar("name").is_some());
        assert!(ruler.get_scalar("dynasty").is_some());

        // Capital province: owner/controller/core now the new tag; the old FRA
        // core survives (carve-out semantics of the add tool).
        let prov = std::fs::read_to_string(project.join("history/provinces/10 - Testburg.txt")).unwrap();
        assert!(prov.contains(&format!("owner = {tag}")));
        assert!(prov.contains(&format!("controller = {tag}")));
        assert!(prov.contains(&format!("add_core = {tag}")));
        assert!(prov.contains("add_core = FRA"));
        assert!(prov.contains("culture = swabian"));

        // Flag TGA: correct header + dims.
        let tga = std::fs::read(project.join(&scaffold.flag_file)).unwrap();
        assert_eq!(tga[2], 2);
        assert_eq!(u16::from_le_bytes([tga[12], tga[13]]), 128);
        assert_eq!(u16::from_le_bytes([tga[14], tga[15]]), 128);

        // Loc: name + adjective present, file has a UTF-8 BOM.
        let loc_bytes = std::fs::read(project.join(crate::loc::OVERRIDE_REL)).unwrap();
        assert_eq!(&loc_bytes[..3], &[0xEF, 0xBB, 0xBF], "loc must be UTF-8 BOM");
        let loc_text = String::from_utf8_lossy(&loc_bytes);
        assert!(loc_text.contains(&format!(" {tag}:0 \"Newland\"")));
        assert!(loc_text.contains(&format!(" {tag}_ADJ:0 \"Newlandish\"")));

        assert!(written.contains(&crate::loc::OVERRIDE_REL.to_string()));
    }

    #[test]
    fn exclude_tags_avoid_pending_creates() {
        // Scaffolding several countries before a single Save: each must claim a
        // tag distinct from the ones already pending in the queue.
        let (_root, vfs) = synthetic("exclude", true);
        let first = build_scaffold(&vfs, 10, "Newland", "Newlandish", &HashSet::new()).unwrap();

        // Second create excludes the first's (still-pending) tag → a new one.
        let mut exclude = HashSet::new();
        exclude.insert(first.tag.clone());
        let second = build_scaffold(&vfs, 10, "Newland", "Newlandish", &exclude).unwrap();
        assert_ne!(second.tag, first.tag, "pending tag must be excluded");
        assert!(tag_available(&second.tag, &used_tags(&vfs)));

        // Third excludes both prior pending tags → distinct from each.
        exclude.insert(second.tag.clone());
        let third = build_scaffold(&vfs, 10, "Newland", "Newlandish", &exclude).unwrap();
        assert_ne!(third.tag, first.tag);
        assert_ne!(third.tag, second.tag);
    }

    #[test]
    fn inheritance_from_owner_government_and_tech() {
        // FRA is a republic on western tech -> the new country inherits republic
        // (+ its default reform oligarchy_reform) and western tech.
        let (root, vfs) = synthetic("inherit", true);
        let project = root.join("project");
        let scaffold = build_scaffold(&vfs, 10, "Testia", "Testian", &HashSet::new()).unwrap();
        apply(&scaffold, &project, &vfs);
        let hist = std::fs::read_to_string(project.join(&scaffold.history_file)).unwrap();
        assert!(hist.contains("government = republic"));
        assert!(hist.contains("add_government_reform = oligarchy_reform"));
        assert!(hist.contains("technology_group = western"));
        // Government rank always starts at 1 (small new country), not inherited 2.
        assert!(hist.contains("government_rank = 1"));
    }

    #[test]
    fn scaffold_on_uncolonized_capital_uses_defaults() {
        let (root, vfs) = synthetic("uncolonized", false);
        let project = root.join("project");
        let scaffold = build_scaffold(&vfs, 10, "Frontier", "Frontierish", &HashSet::new()).unwrap();
        apply(&scaffold, &project, &vfs);

        // No owner to inherit from -> monarchy + western defaults.
        let hist = std::fs::read_to_string(project.join(&scaffold.history_file)).unwrap();
        assert!(hist.contains("government = monarchy"));
        assert!(hist.contains("add_government_reform = feudalism_reform"));
        assert!(hist.contains("technology_group = western"));

        // Capital had no owner/controller -> keys inserted (not replaced).
        let prov = std::fs::read_to_string(project.join("history/provinces/10 - Testburg.txt")).unwrap();
        assert!(prov.contains(&format!("owner = {}", scaffold.tag)));
        assert!(prov.contains(&format!("controller = {}", scaffold.tag)));
        assert!(prov.contains(&format!("add_core = {}", scaffold.tag)));
        // Original data preserved.
        assert!(prov.contains("culture = swabian"));
        assert!(prov.contains("base_tax = 3"));
    }

    #[test]
    fn water_capital_is_rejected() {
        let (_root, vfs) = synthetic("water", true);
        let err = build_scaffold(&vfs, 99, "Sealand", "Sea", &HashSet::new()).unwrap_err();
        assert!(err.to_lowercase().contains("water"), "got: {err}");
    }

    #[test]
    fn missing_capital_is_rejected() {
        let (_root, vfs) = synthetic("missing", true);
        assert!(build_scaffold(&vfs, 4242, "Nowhere", "N", &HashSet::new()).unwrap_err().contains("not found"));
    }

    // --- Real-install structural parity ----------------------------------

    #[test]
    fn structural_parity_against_vanilla_ulm() {
        if !Path::new(INSTALL).join("map/provinces.bmp").is_file() {
            return; // game absent: no-op
        }
        let vfs = Vfs::new(INSTALL, None).unwrap();
        // Ulm's capital is province 1872. Create a country there and compare the
        // generated files' key sets against Ulm's own (the "minimal country" bar).
        let scaffold = build_scaffold(&vfs, 1872, "Testland", "Testish", &HashSet::new()).unwrap();

        // Tag is genuinely unused in vanilla.
        assert!(tag_available(&scaffold.tag, &used_tags(&vfs)));

        let cty_edit = scaffold
            .edits
            .iter()
            .find_map(|e| match e {
                ScaffoldEdit::CreateFile { file, text } if file.contains("common/countries/") => {
                    Some(text.clone())
                }
                _ => None,
            })
            .unwrap();
        let cty = paradox::parse(&cty_edit);
        // Vanilla country files carry these; ours must too.
        for key in ["graphical_culture", "color", "monarch_names"] {
            assert!(
                cty.get(key).is_some(),
                "generated country file missing `{key}` that vanilla Ulm has"
            );
        }

        let hist_edit = scaffold
            .edits
            .iter()
            .find_map(|e| match e {
                ScaffoldEdit::CreateFile { file, text } if file.contains("history/countries/") => {
                    Some(text.clone())
                }
                _ => None,
            })
            .unwrap();
        let hist = paradox::parse(&hist_edit);
        // The required key set every vanilla country history carries.
        for key in [
            "government",
            "add_government_reform",
            "government_rank",
            "technology_group",
            "religion",
            "primary_culture",
            "capital",
        ] {
            assert!(hist.get_scalar(key).is_some(), "generated history missing `{key}`");
        }
        // Inherited from Ulm's owner-at-start? Ulm(1872) is owned by ULM itself
        // (a free city republic); we at least inherit a valid religion/culture
        // from the province and a monarch exists.
        let ruler = hist
            .get_block("1444.11.11")
            .and_then(|b| b.get_block("monarch"))
            .expect("starting monarch");
        assert!(ruler.get_scalar("name").is_some());
        assert!(ruler.get_scalar("dynasty").is_some());
        assert!(ruler.get_scalar("adm").is_some());
    }

    #[test]
    fn real_install_tag_is_unique() {
        if !Path::new(INSTALL).join("map/provinces.bmp").is_file() {
            return;
        }
        let vfs = Vfs::new(INSTALL, None).unwrap();
        let used = used_tags(&vfs);
        // A representative sample of real vanilla tags must be in the used set.
        assert!(used.contains("FRA"));
        assert!(used.contains("SWE"));
        let tag = generate_tag("Newland", &used).unwrap();
        assert!(!used.contains(&tag));
    }

    #[test]
    fn anbennar_tags_are_avoided() {
        if !Path::new(INSTALL).join("map/provinces.bmp").is_file() || !Path::new(ANBENNAR).is_dir() {
            return; // game or Anbennar absent: no-op
        }
        let vfs = Vfs::new(INSTALL, Some(ANBENNAR)).unwrap();
        let used = used_tags(&vfs);
        // Anbennar's custom tags come through the merged Vfs listing for free.
        assert!(used.contains("A01"), "Anbennar tag A01 should be collected as used");
        assert!(used.len() > 1000, "Anbennar loaded -> hundreds of tags");
        // A freshly generated tag never collides with any of them.
        let tag = generate_tag("Newland", &used).unwrap();
        assert!(!used.contains(&tag));
        assert!(tag_available(&tag, &used));
    }
}
