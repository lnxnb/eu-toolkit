mod adjacencies;
mod blank;
mod bookmarks;
mod color_pools;
mod colonial;
mod country_create;
mod country_delete;
mod date;
mod db;
mod decisions;
mod defines;
mod diplomacy;
mod dynasties;
mod edits;
mod empires;
mod estates;
mod events;
mod export;
mod game_data;
mod geography;
mod gfx;
mod government_names;
mod great_projects;
mod group_create;
mod icons;
mod installations;
mod launch;
mod loc;
mod map_renderer;
mod mechanics;
mod mercenary_companies;
mod missions;
mod mod_loader;
mod mod_writer;
mod on_actions;
mod paradox;
mod project_diff;
mod province_details;
mod province_names;
mod rebels;
mod recents;
mod registry;
mod script_tree;
mod scripted;
mod search;
mod technology;
mod trade_details;
mod tradegoods;
mod tradenodes;
mod trigger_eval;
mod validation;
mod vfs;
mod wars;
mod workshop;

use std::path::{Path, PathBuf};

use installations::Installation;
use mod_loader::ModInfo;
use vfs::Vfs;

const INSTALLATION_KEY: &str = "installation_path";

fn open_vfs(install_path: &str, mod_path: &Option<String>) -> Result<Vfs, String> {
    Vfs::new(install_path, mod_path.as_deref())
}

/// Opens an EU4 mod folder: validates it, parses its descriptor, and lists its contents.
#[tauri::command]
fn open_mod(path: String) -> Result<ModInfo, String> {
    mod_loader::open_mod(&path)
}

/// Scans Steam libraries (and common locations) for EU4 installations.
#[tauri::command]
fn detect_installations() -> Vec<Installation> {
    installations::detect()
}

/// Returns the remembered installation path, or None if unset or no longer valid.
#[tauri::command]
fn get_saved_installation(app: tauri::AppHandle) -> Result<Option<String>, String> {
    Ok(db::get_setting(&app, INSTALLATION_KEY)?
        .filter(|p| installations::is_valid_installation(Path::new(p))))
}

#[tauri::command]
fn save_installation(app: tauri::AppHandle, path: String) -> Result<(), String> {
    if !installations::is_valid_installation(Path::new(&path)) {
        return Err(format!(
            "{path} does not look like an EU4 installation (map\\provinces.bmp not found). \
             Note: the Documents\\Paradox Interactive folder is not the game installation."
        ));
    }
    db::set_setting(&app, INSTALLATION_KEY, &path)
}

#[tauri::command]
fn clear_saved_installation(app: tauri::AppHandle) -> Result<(), String> {
    db::delete_setting(&app, INSTALLATION_KEY)
}

/// Checks that a folder is usable as a mod project; returns its display name.
#[tauri::command]
fn validate_project(path: String) -> Result<String, String> {
    let dir = PathBuf::from(&path);
    if !vfs::is_mod_project(&dir) {
        return Err(format!(
            "{path} does not look like an EU4 mod project (no .mod descriptor or game folders like common/, map/, history/)."
        ));
    }
    let name = vfs::read_descriptor(&dir)
        .map(|text| paradox::parse(&text))
        .and_then(|b| b.get_scalar("name").map(str::to_string))
        .or_else(|| {
            dir.file_name()
                .map(|n| n.to_string_lossy().into_owned())
        })
        .unwrap_or(path);
    Ok(name)
}

// --- Recent projects (Sprint 18.1) ---------------------------------------
// Stored as one JSON array under the `recent_projects` settings key. Recording
// happens on every successful open (fired from the frontend's session funnel);
// the list is validated against the live filesystem on read.

fn load_recents(app: &tauri::AppHandle) -> Result<Vec<recents::RecentProject>, String> {
    Ok(recents::parse(db::get_setting(app, recents::RECENTS_KEY)?))
}

fn save_recents(app: &tauri::AppHandle, list: &[recents::RecentProject]) -> Result<(), String> {
    db::set_setting(app, recents::RECENTS_KEY, &recents::serialize(list)?)
}

/// Records a just-opened session (dedupes by path, bumps its timestamp). The
/// display name is derived server-side (descriptor `name` / folder name, or
/// "Base game @ <install>") so every open path records consistently.
#[tauri::command]
fn record_recent_project(
    app: tauri::AppHandle,
    install_path: String,
    project_path: Option<String>,
) -> Result<(), String> {
    let entry = recents::RecentProject {
        display_name: recents::display_name(&project_path, &install_path),
        project_path,
        install_path,
        last_opened: recents::now_millis(),
        pinned: false,
        missing: false,
    };
    let list = recents::upsert(load_recents(&app)?, entry);
    save_recents(&app, &list)
}

/// The recent-projects list, most-recent-first (pinned first), each row's
/// `missing` flag freshly computed from the filesystem.
#[tauri::command]
fn list_recent_projects(app: tauri::AppHandle) -> Result<Vec<recents::RecentProject>, String> {
    let mut list = load_recents(&app)?;
    recents::annotate_missing(&mut list);
    Ok(list)
}

#[tauri::command]
fn remove_recent_project(
    app: tauri::AppHandle,
    install_path: String,
    project_path: Option<String>,
) -> Result<(), String> {
    let list = recents::remove(load_recents(&app)?, &project_path, &install_path);
    save_recents(&app, &list)
}

#[tauri::command]
fn set_recent_project_pinned(
    app: tauri::AppHandle,
    install_path: String,
    project_path: Option<String>,
    pinned: bool,
) -> Result<(), String> {
    let list = recents::set_pinned(load_recents(&app)?, &project_path, &install_path, pinned);
    save_recents(&app, &list)
}

// --- Steam Workshop detection & forking (Sprint 18.2 / 18.4) --------------

/// True if `path` sits under a Steam workshop item folder for EU4. Used on open
/// to warn (not block) that Steam overwrites the folder on updates.
#[tauri::command]
fn is_workshop_path(path: String) -> bool {
    workshop::is_workshop_path(Path::new(&path))
}

/// True if this session's install is a Steam install with a provisioned EU4
/// workshop folder (gates File ▸ Fork from Steam…).
#[tauri::command]
fn is_steam_backed_install(install_path: String) -> bool {
    workshop::is_steam_backed(Path::new(&install_path))
}

/// Subscribed EU4 workshop mods for this install's Steam library.
#[tauri::command]
fn list_workshop_mods(install_path: String) -> Vec<workshop::WorkshopMod> {
    workshop::list_workshop_mods(Path::new(&install_path))
}

/// The user's `Documents\Paradox Interactive\Europa Universalis IV\mod` folder,
/// where forks (and launcher pointers) live.
fn user_mod_folder(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    use tauri::Manager;
    let documents = app
        .path()
        .document_dir()
        .map_err(|e| format!("Failed to resolve Documents folder: {e}"))?;
    Ok(documents
        .join("Paradox Interactive")
        .join("Europa Universalis IV")
        .join("mod"))
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ForkPlan {
    /// Default fork name ("<name> (Fork)").
    name: String,
    /// Default non-colliding folder slug under the user mod folder.
    slug: String,
    /// Copy sizes for the skip-junk (default) and full-copy settings.
    size_skip: u64,
    size_full: u64,
    /// Free bytes on the destination drive (0 if unknown).
    free_bytes: u64,
}

/// Pre-copy fork defaults for `source_path`: suggested name, a collision-free
/// slug, payload sizes (skip vs full), and destination free space.
#[tauri::command]
fn prepare_fork(app: tauri::AppHandle, source_path: String) -> Result<ForkPlan, String> {
    let src = PathBuf::from(&source_path);
    if !src.is_dir() {
        return Err(format!("{source_path} is not a folder."));
    }
    let orig = vfs::read_descriptor(&src)
        .map(|text| paradox::parse(&text))
        .and_then(|b| b.get_scalar("name").map(str::to_string))
        .filter(|s| !s.trim().is_empty())
        .or_else(|| src.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "Mod".to_string());

    let mod_root = user_mod_folder(&app)?;
    let free_bytes = workshop::available_space(&mod_root).unwrap_or(0);

    Ok(ForkPlan {
        name: format!("{orig} (Fork)"),
        slug: workshop::unique_slug(&mod_root, &orig),
        size_skip: workshop::dir_size(&src, false),
        size_full: workshop::dir_size(&src, true),
        free_bytes,
    })
}

/// Requests cancellation of the in-flight fork (the copy loop stops at the next
/// file and the partial destination is removed).
#[tauri::command]
fn cancel_fork() {
    workshop::request_cancel();
}

/// Kicks off a fork of `source_path` into `<user mod folder>\<slug>` on a worker
/// thread. Returns after synchronous preflight (collision + free-space); copy
/// progress and completion arrive as `fork-progress` / `fork-finished` events.
#[tauri::command]
fn start_fork(
    app: tauri::AppHandle,
    install_path: String,
    source_path: String,
    name: String,
    slug: String,
    full_copy: bool,
) -> Result<(), String> {
    let src = PathBuf::from(&source_path);
    if !src.is_dir() {
        return Err(format!("{source_path} is not a folder."));
    }
    let slug = slug.trim();
    if slug.is_empty() {
        return Err("Please choose a folder name for the fork.".to_string());
    }
    let mod_root = user_mod_folder(&app)?;
    let dst = mod_root.join(slug);
    if dst.exists() {
        return Err(format!(
            "A folder named \"{slug}\" already exists in your mod folder. Choose another name."
        ));
    }
    let total = workshop::dir_size(&src, full_copy);
    if let Some(free) = workshop::available_space(&mod_root) {
        if !workshop::has_enough_space(total, free) {
            return Err(format!(
                "Not enough free space: the fork needs about {} MiB but the drive has {} MiB free.",
                total / (1024 * 1024),
                free / (1024 * 1024)
            ));
        }
    }
    if !workshop::begin() {
        return Err("A fork is already in progress.".to_string());
    }

    let install = PathBuf::from(&install_path);
    std::thread::spawn(move || {
        use tauri::Emitter;
        let cancel = workshop::cancel_flag();
        let _ = app.emit(
            "fork-progress",
            ForkProgress {
                copied_bytes: 0,
                total_bytes: total,
                current_file: String::new(),
            },
        );

        let app_for_progress = app.clone();
        let mut last = std::time::Instant::now();
        let outcome = workshop::fork_into(
            &src,
            &dst,
            full_copy,
            cancel,
            total,
            |copied, total, file| {
                // Throttle to ~12 events/sec so the IPC channel isn't flooded.
                if last.elapsed().as_millis() >= 80 || copied >= total {
                    last = std::time::Instant::now();
                    let _ = app_for_progress.emit(
                        "fork-progress",
                        ForkProgress {
                            copied_bytes: copied,
                            total_bytes: total,
                            current_file: file
                                .file_name()
                                .map(|n| n.to_string_lossy().into_owned())
                                .unwrap_or_default(),
                        },
                    );
                }
            },
        );

        let finished = match outcome {
            Ok(workshop::ForkStatus::Canceled) => ForkFinished {
                error: None,
                canceled: true,
                path: None,
                name: None,
            },
            Err(e) => ForkFinished {
                error: Some(e),
                canceled: false,
                path: None,
                name: None,
            },
            Ok(workshop::ForkStatus::Completed) => {
                // Rewrite the fork's descriptor (name + path) and register the
                // launcher pointer, mirroring export_to_game. Any failure here
                // cleans up the destination so no half-registered fork remains.
                let result = finalize_fork(&app, &install, &dst, &name);
                match result {
                    Ok(()) => ForkFinished {
                        error: None,
                        canceled: false,
                        path: Some(dst.to_string_lossy().replace('/', "\\")),
                        name: Some(name.clone()),
                    },
                    Err(e) => {
                        let _ = std::fs::remove_dir_all(&dst);
                        ForkFinished {
                            error: Some(e),
                            canceled: false,
                            path: None,
                            name: None,
                        }
                    }
                }
            }
        };

        let _ = app.emit("fork-finished", finished);
        workshop::end();
    });

    Ok(())
}

/// After a successful copy: rewrite descriptor.mod's name/path in the fork and
/// write the launcher-side pointer file into the user's Documents.
fn finalize_fork(
    app: &tauri::AppHandle,
    install: &Path,
    dst: &Path,
    name: &str,
) -> Result<(), String> {
    let path_fwd = dst.to_string_lossy().replace('\\', "/");
    let descriptor = dst.join("descriptor.mod");
    let existing = std::fs::read(&descriptor).ok();
    let rewritten = match existing {
        Some(bytes) => workshop::rewrite_descriptor(&bytes, name, &path_fwd),
        None => workshop::rewrite_descriptor(b"", name, &path_fwd),
    };
    std::fs::write(&descriptor, rewritten)
        .map_err(|e| format!("Failed to write descriptor.mod: {e}"))?;

    use tauri::Manager;
    let documents = app
        .path()
        .document_dir()
        .map_err(|e| format!("Failed to resolve Documents folder: {e}"))?;
    export::write_game_pointer(&documents, install, dst)?;
    Ok(())
}

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ForkProgress {
    copied_bytes: u64,
    total_bytes: u64,
    current_file: String,
}

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ForkFinished {
    /// Present on failure (human-readable message).
    error: Option<String>,
    canceled: bool,
    /// The fork folder (backslashes) on success — the frontend opens it.
    path: Option<String>,
    name: Option<String>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct MapMode {
    id: &'static str,
    label: &'static str,
    /// Sprint 11.3: view-only raster modes (terrain/heightmap/province_colors)
    /// get a "View Only" badge and offer no tools/panel. Additive field —
    /// existing `id`/`label` consumers are unaffected.
    view_only: bool,
}

#[tauri::command]
fn list_map_modes() -> Vec<MapMode> {
    map_renderer::MAP_MODES
        .iter()
        .map(|&(id, label)| MapMode {
            id,
            label,
            view_only: map_renderer::VIEW_ONLY_MODES.contains(&id),
        })
        .collect()
}

/// Per-province effective terrain (override vs. auto) plus the terrain-category
/// catalog (colors + gameplay modifiers) for the Simple Terrain mode (11.2).
#[tauri::command]
fn get_effective_terrain(
    install_path: String,
    mod_path: Option<String>,
) -> Result<game_data::EffectiveTerrainPayload, String> {
    let vfs = open_vfs(&install_path, &mod_path)?;
    let loc = loc::store(&vfs, &install_path, mod_path.as_deref());
    game_data::effective_terrain_payload(&vfs, &loc)
}

/// The two independent slots of map/climate.txt (climate zone + winter severity)
/// plus which list blocks exist, for the Climate mode's two-slot paint selector
/// (Sprint 11.1). Both slots share one file; painting one never touches the other.
#[tauri::command]
fn get_climate(
    install_path: String,
    mod_path: Option<String>,
) -> Result<game_data::ClimatePayload, String> {
    let vfs = open_vfs(&install_path, &mod_path)?;
    Ok(game_data::climate_payload(&vfs))
}

/// Adjacencies payload for the Provinces map mode (Sprint 25): the parsed
/// `map/adjacencies.csv` rows (index = file order) plus the water province id
/// set (for strait type-derivation / through-suggestion frontend heuristics).
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AdjacenciesPayload {
    rows: Vec<adjacencies::AdjRow>,
    water_ids: Vec<u32>,
}

/// Reads `map/adjacencies.csv` through the Vfs (mod shadows/replaces base) and
/// returns the parsed rows + water ids. Static (no date threading).
#[tauri::command]
fn get_adjacencies(
    install_path: String,
    mod_path: Option<String>,
) -> Result<AdjacenciesPayload, String> {
    let vfs = open_vfs(&install_path, &mod_path)?;
    let bytes = vfs.read("map/adjacencies.csv").unwrap_or_default();
    let rows = adjacencies::parse_rows(&bytes);
    let mut water_ids: Vec<u32> = map_renderer::water_ids(&vfs)?.into_iter().collect();
    water_ids.sort_unstable();
    Ok(AdjacenciesPayload { rows, water_ids })
}

/// Validates a folded adjacency row list (Sprint 25): sea straits whose
/// `through` isn't water (error); sea-strait endpoints that aren't coastal, and
/// duplicate From/To pairs either direction (warnings).
#[tauri::command]
fn validate_adjacencies(
    install_path: String,
    mod_path: Option<String>,
    rows: Vec<adjacencies::AdjRow>,
) -> Result<Vec<adjacencies::AdjIssue>, String> {
    let vfs = open_vfs(&install_path, &mod_path)?;
    let water = map_renderer::water_ids(&vfs)?;
    let coastal = map_renderer::coastal_land_ids(&vfs)?;
    Ok(adjacencies::validate(&rows, &water, &coastal))
}

/// Renders a map mode to a PNG; returned as raw bytes (ArrayBuffer in JS).
#[tauri::command]
fn render_map_mode(
    install_path: String,
    mod_path: Option<String>,
    mode: String,
    date: Option<String>,
) -> Result<tauri::ipc::Response, String> {
    let vfs = open_vfs(&install_path, &mod_path)?;
    let at = bookmarks::resolve_date(&vfs, date.as_deref())?;
    let png = map_renderer::render_map_mode_at(&vfs, &mode, at)?;
    Ok(tauri::ipc::Response::new(png))
}

/// Province id per pixel: [u32 width][u32 height][u16 id]* little-endian.
#[tauri::command]
fn get_province_ids(
    install_path: String,
    mod_path: Option<String>,
) -> Result<tauri::ipc::Response, String> {
    let vfs = open_vfs(&install_path, &mod_path)?;
    let buf = map_renderer::province_id_buffer(&vfs)?;
    Ok(tauri::ipc::Response::new(buf))
}

/// Unified per-mode selection/hover data. Returns a binary payload:
/// `[u32 header_len][header JSON][u16 value per province id]` (little-endian).
/// The JSON header carries `kind` ("categorical" | "gradient" | "raster"),
/// `groups` (categorical), `maxId`, and `valueScale` (gradient). See
/// `game_data::ModeData`.
#[tauri::command]
fn get_mode_data(
    app: tauri::AppHandle,
    install_path: String,
    mod_path: Option<String>,
    mode: String,
    date: Option<String>,
) -> Result<tauri::ipc::Response, String> {
    let vfs = open_vfs(&install_path, &mod_path)?;
    let loc = loc::store(&vfs, &install_path, mod_path.as_deref());
    let at = bookmarks::resolve_date(&vfs, date.as_deref())?;
    // Culture mode honors the toolkit's own per-culture display-color overrides
    // (stored in the settings DB, never in the mod). Other modes ignore them.
    let overrides = if mode == "culture" {
        load_culture_overrides(&app, &mod_path)?
    } else {
        std::collections::HashMap::new()
    };
    let data = game_data::mode_data_with_overrides_at(&vfs, &loc, &mode, &overrides, at)?;
    Ok(tauri::ipc::Response::new(data.to_wire()))
}

/// Settings-DB key scoping the selected view/edit date to a project (or the base
/// game), Sprint 12.2. Toolkit-only session state keyed per (mod|base).
fn selected_date_key(mod_path: &Option<String>) -> String {
    format!(
        "selected_date:{}",
        mod_path.as_deref().unwrap_or("__base__")
    )
}

/// The persisted selected date for this session's scope, or None if never set
/// (the frontend then falls back to the effective start bookmark date).
#[tauri::command]
fn get_selected_date(
    app: tauri::AppHandle,
    mod_path: Option<String>,
) -> Result<Option<String>, String> {
    db::get_setting(&app, &selected_date_key(&mod_path))
}

/// Persists (or clears, when `date` is None) the selected date for this scope.
#[tauri::command]
fn set_selected_date(
    app: tauri::AppHandle,
    mod_path: Option<String>,
    date: Option<String>,
) -> Result<(), String> {
    let key = selected_date_key(&mod_path);
    match date {
        Some(d) => db::set_setting(&app, &key, &d),
        None => db::delete_setting(&app, &key),
    }
}

/// A boolean View-menu toggle persisted in the settings DB, keyed globally
/// (display preference, not per-project). Used by the S3.3 trade-details overlay
/// toggle; reusable for future view toggles. `None` = never set (caller defaults).
#[tauri::command]
fn get_view_toggle(app: tauri::AppHandle, key: String) -> Result<Option<bool>, String> {
    Ok(db::get_setting(&app, &format!("view_toggle:{key}"))?.map(|v| v == "1"))
}

/// Persists a boolean View-menu toggle (see [`get_view_toggle`]).
#[tauri::command]
fn set_view_toggle(app: tauri::AppHandle, key: String, value: bool) -> Result<(), String> {
    db::set_setting(&app, &format!("view_toggle:{key}"), if value { "1" } else { "0" })
}

/// Settings-DB key prefix scoping culture display-color overrides to a project
/// (or the base game). Cultures have no color in the game files (Sprint 6.1); an
/// override is toolkit-only state keyed per (mod|base, culture key).
fn culture_override_scope(mod_path: &Option<String>) -> String {
    mod_path.as_deref().unwrap_or("__base__").to_string()
}

fn culture_override_prefix(mod_path: &Option<String>) -> String {
    format!("culture_color:{}:", culture_override_scope(mod_path))
}

fn culture_override_key(mod_path: &Option<String>, culture: &str) -> String {
    format!("{}{}", culture_override_prefix(mod_path), culture)
}

/// All culture display-color overrides for this session's scope: culture key -> rgb.
fn load_culture_overrides(
    app: &tauri::AppHandle,
    mod_path: &Option<String>,
) -> Result<std::collections::HashMap<String, [u8; 3]>, String> {
    let prefix = culture_override_prefix(mod_path);
    let mut out = std::collections::HashMap::new();
    for (key, value) in db::get_settings_prefix(app, &prefix)? {
        // Culture keys contain no ':'; the culture is the tail after the prefix.
        let Some(culture) = key.strip_prefix(&prefix) else {
            continue;
        };
        if let Some(rgb) = parse_rgb(&value) {
            out.insert(culture.to_string(), rgb);
        }
    }
    Ok(out)
}

fn parse_rgb(s: &str) -> Option<[u8; 3]> {
    let p: Vec<u8> = s.split_whitespace().filter_map(|t| t.parse().ok()).collect();
    if p.len() == 3 {
        Some([p[0], p[1], p[2]])
    } else {
        None
    }
}

/// Per-province political + eligibility payload for the political-mode brush
/// tools (Sprint 1.4). Loaded once per session; ~5k small records.
#[tauri::command]
fn get_province_political(
    install_path: String,
    mod_path: Option<String>,
    date: Option<String>,
) -> Result<Vec<game_data::ProvincePolitical>, String> {
    let vfs = open_vfs(&install_path, &mod_path)?;
    let at = bookmarks::resolve_date(&vfs, date.as_deref())?;
    Ok(game_data::province_political_at(&vfs, at))
}

#[tauri::command]
fn get_country_details(
    install_path: String,
    mod_path: Option<String>,
    tag: String,
    date: Option<String>,
) -> Result<game_data::CountryDetails, String> {
    let vfs = open_vfs(&install_path, &mod_path)?;
    let loc = loc::store(&vfs, &install_path, mod_path.as_deref());
    let at = bookmarks::resolve_date(&vfs, date.as_deref())?;
    game_data::country_details_at(&vfs, &loc, &tag, at)
}

/// Religions grouped by religion group (localized), for the panel dropdown.
#[tauri::command]
fn list_religions(
    install_path: String,
    mod_path: Option<String>,
) -> Result<Vec<game_data::GroupedEntry>, String> {
    let vfs = open_vfs(&install_path, &mod_path)?;
    let loc = loc::store(&vfs, &install_path, mod_path.as_deref());
    Ok(game_data::religion_list(&vfs, &loc))
}

/// Full details of one religion (its block inside a group in common/religions).
#[tauri::command]
fn get_religion_details(
    install_path: String,
    mod_path: Option<String>,
    key: String,
) -> Result<game_data::ReligionDetails, String> {
    let vfs = open_vfs(&install_path, &mod_path)?;
    let loc = loc::store(&vfs, &install_path, mod_path.as_deref());
    game_data::religion_details(&vfs, &loc, &key)
}

/// All religion groups (localized), for the create flow and move-to-group.
#[tauri::command]
fn list_religion_groups(
    install_path: String,
    mod_path: Option<String>,
) -> Result<Vec<game_data::ReligionGroupEntry>, String> {
    let vfs = open_vfs(&install_path, &mod_path)?;
    let loc = loc::store(&vfs, &install_path, mod_path.as_deref());
    Ok(game_data::religion_group_list(&vfs, &loc))
}

/// Cultures grouped by culture group (localized), for the panel dropdown.
#[tauri::command]
fn list_cultures(
    install_path: String,
    mod_path: Option<String>,
) -> Result<Vec<game_data::GroupedEntry>, String> {
    let vfs = open_vfs(&install_path, &mod_path)?;
    let loc = loc::store(&vfs, &install_path, mod_path.as_deref());
    Ok(game_data::culture_list(&vfs, &loc))
}

/// Full details of one culture (its block inside a group in common/cultures).
#[tauri::command]
fn get_culture_details(
    install_path: String,
    mod_path: Option<String>,
    key: String,
) -> Result<game_data::CultureDetails, String> {
    let vfs = open_vfs(&install_path, &mod_path)?;
    let loc = loc::store(&vfs, &install_path, mod_path.as_deref());
    game_data::culture_details(&vfs, &loc, &key)
}

/// All culture groups (localized), for the create flow and move-to-group.
#[tauri::command]
fn list_culture_groups(
    install_path: String,
    mod_path: Option<String>,
) -> Result<Vec<game_data::CultureGroupEntry>, String> {
    let vfs = open_vfs(&install_path, &mod_path)?;
    let loc = loc::store(&vfs, &install_path, mod_path.as_deref());
    Ok(game_data::culture_group_list(&vfs, &loc))
}

/// The dynamic province-rename list for one key (culture / culture group / TAG),
/// from `common/province_names/<key>.txt` through the Vfs (Sprint 24).
#[tauri::command]
fn get_province_names(
    install_path: String,
    mod_path: Option<String>,
    key: String,
) -> Result<province_names::ProvinceNamesFile, String> {
    let vfs = open_vfs(&install_path, &mod_path)?;
    Ok(province_names::province_names_for(&vfs, &key))
}

/// Reverse view for the province panel: every culture/group/tag that renames the
/// given province, with the name it assigns (Sprint 24).
#[tauri::command]
fn get_province_name_assignments(
    install_path: String,
    mod_path: Option<String>,
    id: u32,
) -> Result<Vec<province_names::ProvinceNameAssignment>, String> {
    let vfs = open_vfs(&install_path, &mod_path)?;
    let loc = loc::store(&vfs, &install_path, mod_path.as_deref());
    Ok(province_names::assignments_for(&vfs, &loc, id))
}

/// This session's per-culture display-color overrides: culture key -> [r,g,b].
/// Toolkit-only state (settings DB), applied to the culture map mode.
#[tauri::command]
fn list_culture_color_overrides(
    app: tauri::AppHandle,
    mod_path: Option<String>,
) -> Result<std::collections::HashMap<String, [u8; 3]>, String> {
    load_culture_overrides(&app, &mod_path)
}

/// One culture's display-color override, or None if unset.
#[tauri::command]
fn get_culture_color_override(
    app: tauri::AppHandle,
    mod_path: Option<String>,
    key: String,
) -> Result<Option<[u8; 3]>, String> {
    let dbkey = culture_override_key(&mod_path, &key);
    Ok(db::get_setting(&app, &dbkey)?.and_then(|v| parse_rgb(&v)))
}

/// Pins a display color for `key` (Sprint 6.1). Applied immediately to the
/// culture map mode; never written into the mod files.
#[tauri::command]
fn set_culture_color_override(
    app: tauri::AppHandle,
    mod_path: Option<String>,
    key: String,
    r: u8,
    g: u8,
    b: u8,
) -> Result<(), String> {
    let dbkey = culture_override_key(&mod_path, &key);
    db::set_setting(&app, &dbkey, &format!("{r} {g} {b}"))
}

/// Clears `key`'s display-color override, reverting to the toolkit hash color.
#[tauri::command]
fn clear_culture_color_override(
    app: tauri::AppHandle,
    mod_path: Option<String>,
    key: String,
) -> Result<(), String> {
    let dbkey = culture_override_key(&mod_path, &key);
    db::delete_setting(&app, &dbkey)
}

/// Pickable idea groups (the 8-idea groups with a category), for the country
/// panel's historical-idea-groups picker.
#[tauri::command]
fn list_idea_groups(
    install_path: String,
    mod_path: Option<String>,
) -> Result<Vec<game_data::IdeaGroupEntry>, String> {
    let vfs = open_vfs(&install_path, &mod_path)?;
    let loc = loc::store(&vfs, &install_path, mod_path.as_deref());
    Ok(game_data::idea_group_list(&vfs, &loc))
}

/// All unit keys (common/units file stems), for the historical-units picker.
#[tauri::command]
fn list_units(install_path: String, mod_path: Option<String>) -> Result<Vec<String>, String> {
    let vfs = open_vfs(&install_path, &mod_path)?;
    Ok(game_data::unit_list(&vfs))
}

/// Every country tag with localized name + map color, for tag pickers.
#[tauri::command]
fn list_countries(
    install_path: String,
    mod_path: Option<String>,
) -> Result<Vec<game_data::CountryBrief>, String> {
    let vfs = open_vfs(&install_path, &mod_path)?;
    let loc = loc::store(&vfs, &install_path, mod_path.as_deref());
    Ok(game_data::country_list(&vfs, &loc))
}

/// Converts a user-picked image file into a 128x128 flag. Returns the TGA bytes
/// (for the `gfx/flags/TAG.tga` binary-asset edit) plus a PNG preview (for the
/// panel to show the pending flag before save). Both cross IPC as byte arrays.
#[derive(serde::Serialize)]
struct FlagConversion {
    tga: Vec<u8>,
    preview: Vec<u8>,
}

#[tauri::command]
fn convert_flag(path: String) -> Result<FlagConversion, String> {
    let (tga, preview) = game_data::convert_flag(&path)?;
    Ok(FlagConversion { tga, preview })
}

/// The country's flag as PNG bytes.
#[tauri::command]
fn get_country_flag(
    install_path: String,
    mod_path: Option<String>,
    tag: String,
) -> Result<tauri::ipc::Response, String> {
    let vfs = open_vfs(&install_path, &mod_path)?;
    let png = game_data::country_flag_png(&vfs, &tag)?;
    Ok(tauri::ipc::Response::new(png))
}

/// Applies the session's typed pending-edit queue to a mod project folder
/// (copy-on-write from the resolved source files) and returns the written
/// game-relative paths. `target_dir` names a new project folder when the
/// session has no mod yet. `edits` is the frontend queue flattened in order.
#[tauri::command]
fn save_project(
    install_path: String,
    mod_path: Option<String>,
    target_dir: Option<String>,
    edits: Vec<edits::TypedEdit>,
) -> Result<Vec<String>, String> {
    let project = mod_path
        .clone()
        .or(target_dir)
        .ok_or("No project folder selected")?;
    let project = PathBuf::from(project);
    std::fs::create_dir_all(&project)
        .map_err(|e| format!("Failed to create project folder: {e}"))?;

    // A fresh project needs a descriptor to be a valid mod.
    if vfs::read_descriptor(&project).is_none() {
        let name = project
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "My Mod".to_string());
        let mut descriptor = format!("name=\"{name}\"\n");
        if let Some(v) = export::detect_game_version(Path::new(&install_path)) {
            descriptor.push_str(&format!("supported_version=\"{v}\"\n"));
        }
        std::fs::write(project.join("descriptor.mod"), descriptor)
            .map_err(|e| format!("Failed to write descriptor.mod: {e}"))?;
    }

    // Resolve sources through the session's own layering (an existing
    // project edits its own copies; a base session reads vanilla).
    let vfs = open_vfs(&install_path, &mod_path)?;
    edits::apply_queue(&vfs, &project, &edits)
}

/// Scaffolds a blank-world mod project (SPRINT2 18.3) into `target_dir` over the
/// base install and returns the new project's name. The scaffold keeps the base
/// map and all definitions but empties the world via `replace_path` on the
/// world-populating folders, shipping only the engine-required special tags —
/// see `blank.rs` for the replace_path + tag rationale and the Anbennar/vanilla
/// ground-truth equivalence.
#[tauri::command]
fn scaffold_blank_project(
    install_path: String,
    target_dir: String,
) -> Result<String, String> {
    blank::scaffold_blank(Path::new(&install_path), Path::new(&target_dir))
}

/// Registers the project with the game launcher (pointer .mod file in the
/// user's Documents); overwrites any previous registration. Returns the
/// project name.
#[tauri::command]
fn export_to_game(
    app: tauri::AppHandle,
    install_path: String,
    mod_path: String,
) -> Result<String, String> {
    use tauri::Manager;
    let documents = app
        .path()
        .document_dir()
        .map_err(|e| format!("Failed to resolve Documents folder: {e}"))?;
    export::write_game_pointer(&documents, Path::new(&install_path), Path::new(&mod_path))
}

/// Export & Launch (Sprint 30.5): registers the project, writes it into
/// `dlc_load.json`'s `enabled_mods`, then boots `eu4.exe` directly with the mod
/// active — bypassing the Paradox launcher (see `launch.rs` for the mechanism
/// rationale). Refuses if EU4 is already running. `dry_run` stops short of
/// spawning the game (the write side still runs, so the plan is testable and the
/// UI can preview it). The frontend guards unsaved edits before calling this.
#[tauri::command]
fn export_and_launch(
    app: tauri::AppHandle,
    install_path: String,
    mod_path: String,
    dry_run: bool,
) -> Result<launch::LaunchPlan, String> {
    use tauri::Manager;
    // Refuse when the game is already open (a live launch would race the running
    // instance's file locks / user session). Skipped on dry runs.
    if !dry_run && launch::game_running() {
        return Err(
            "Europa Universalis IV is already running. Close it before launching with the mod."
                .to_string(),
        );
    }
    let documents = app
        .path()
        .document_dir()
        .map_err(|e| format!("Failed to resolve Documents folder: {e}"))?;
    let install = Path::new(&install_path);
    let mut plan = launch::prepare_launch(&documents, install, Path::new(&mod_path), dry_run)?;
    if dry_run {
        return Ok(plan);
    }
    let exe = install.join("eu4.exe");
    if !exe.is_file() {
        return Err(format!("eu4.exe not found in {}", install.display()));
    }
    std::process::Command::new(&exe)
        .current_dir(install)
        .spawn()
        .map_err(|e| format!("Failed to launch eu4.exe: {e}"))?;
    plan.launched = true;
    Ok(plan)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            open_mod,
            detect_installations,
            get_saved_installation,
            save_installation,
            clear_saved_installation,
            validate_project,
            record_recent_project,
            list_recent_projects,
            remove_recent_project,
            set_recent_project_pinned,
            is_workshop_path,
            is_steam_backed_install,
            list_workshop_mods,
            prepare_fork,
            start_fork,
            cancel_fork,
            list_map_modes,
            get_effective_terrain,
            get_climate,
            get_adjacencies,
            validate_adjacencies,
            render_map_mode,
            get_province_ids,
            get_mode_data,
            get_province_political,
            get_country_details,
            get_country_flag,
            list_religions,
            get_religion_details,
            list_religion_groups,
            list_cultures,
            get_culture_details,
            list_culture_groups,
            get_province_names,
            get_province_name_assignments,
            list_culture_color_overrides,
            get_culture_color_override,
            set_culture_color_override,
            clear_culture_color_override,
            list_countries,
            list_idea_groups,
            list_units,
            convert_flag,
            save_project,
            scaffold_blank_project,
            export_to_game,
            registry::get_registry,
            registry::get_known_modifiers,
            script_tree::parse_script_block,
            script_tree::parse_script_block_with_edits,
            script_tree::validate_script_text,
            script_tree::get_known_triggers,
            script_tree::get_known_effects,
            trigger_eval::evaluate_trigger,
            decisions::get_decisions,
            decisions::evaluate_decision,
            events::get_events,
            events::evaluate_event,
            events::next_event_id,
            events::find_event_references,
            missions::get_mission_series,
            missions::evaluate_series_potential,
            missions::mission_link_creates_cycle,
            gfx::get_sprite_index,
            gfx::get_sprite,
            icons::get_icon_atlas,
            icons::import_icon,
            validation::validate,
            validation::validate_all,
            dynasties::scan_dynasties,
            province_details::get_province_details,
            province_details::get_geo_options,
            diplomacy::get_diplomacy,
            wars::get_wars,
            trade_details::get_trade_details,
            tradenodes::get_trade_network,
            tradenodes::derive_route_geometry,
            tradenodes::derive_route_path,
            tradenodes::scaffold_trade_node,
            tradenodes::scaffold_trade_route,
            geography::get_geo_network,
            colonial::get_colonial_data,
            colonial::scaffold_colonial_block,
            government_names::get_government_names,
            government_names::scaffold_government_name,
            government_names::preview_government_name,
            geography::scaffold_area_block,
            geography::scaffold_region_block,
            geography::scaffold_superregion_block,
            geography::scaffold_continent_block,
            country_create::prepare_country_scaffold,
            group_create::prepare_religion_group_scaffold,
            group_create::prepare_culture_group_scaffold,
            country_delete::get_country_blast_radius,
            country_delete::prepare_country_deletion,
            tradegoods::get_trade_goods,
            tradegoods::rebalance_chances,
            tradegoods::prepare_trade_good_scaffold,
            bookmarks::get_bookmarks,
            bookmarks::scaffold_bookmark,
            defines::get_defines_dates,
            loc::get_calendar_loc,
            get_selected_date,
            set_selected_date,
            get_view_toggle,
            set_view_toggle,
            estates::get_estates,
            estates::scaffold_estate_object,
            estates::get_privilege_holders,
            estates::get_country_estates,
            rebels::get_rebels,
            rebels::scaffold_rebel_faction,
            rebels::get_rebel_provinces,
            great_projects::get_province_monuments,
            great_projects::list_monuments,
            great_projects::scaffold_great_project_cmd,
            mercenary_companies::get_province_mercenaries,
            mercenary_companies::scaffold_mercenary_company,
            technology::get_technologies,
            technology::get_units,
            technology::scaffold_unit_file,
            color_pools::get_color_pools,
            mechanics::get_mechanic_families,
            mechanics::get_mechanic_family,
            mechanics::get_mechanics,
            mechanics::scaffold_mechanic,
            mechanics::find_mechanic_event_refs,
            empires::get_emperor_timeline,
            empires::get_hre_electors,
            empires::get_hre_members,
            empires::scaffold_imperial_reform_chain,
            scripted::get_scripted_definitions,
            scripted::scaffold_scripted,
            on_actions::get_on_actions,
            on_actions::scaffold_on_action,
            loc::search_loc,
            loc::missing_loc_report,
            defines::get_defines,
            search::search_project,
            search::read_project_file,
            project_diff::get_project_changes,
            project_diff::get_file_diff,
            export_and_launch
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Copy-on-write save into a fresh project dir, sourcing from a fake base.
    #[test]
    fn save_project_copy_on_write() {
        let root = std::env::temp_dir().join("eu_toolkit_save_test");
        let base = root.join("base");
        let project = root.join("project");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(base.join("map")).unwrap();
        std::fs::create_dir_all(base.join("history/countries")).unwrap();
        std::fs::write(base.join("map/provinces.bmp"), b"x").unwrap();
        std::fs::write(
            base.join("history/countries/FRA - France.txt"),
            b"government = monarchy\n1422.10.21 = {\n\tmonarch = {\n\t\tname = \"Charles VII\"\n\t}\n}\n",
        )
        .unwrap();

        let written = save_project(
            base.to_string_lossy().into_owned(),
            None,
            Some(project.to_string_lossy().into_owned()),
            vec![edits::TypedEdit::RenameRuler {
                tag: "FRA".into(),
                name: "Charles the Renamed".into(),
            }],
        )
        .unwrap();

        assert_eq!(written, vec!["history/countries/FRA - France.txt"]);
        assert!(project.join("descriptor.mod").is_file());
        let saved =
            std::fs::read_to_string(project.join("history/countries/FRA - France.txt")).unwrap();
        assert!(saved.contains("\"Charles the Renamed\""));
        assert!(saved.contains("government = monarchy"));
        // Base game untouched.
        let original =
            std::fs::read_to_string(base.join("history/countries/FRA - France.txt")).unwrap();
        assert!(original.contains("\"Charles VII\""));

        // Second save into the now-existing project resolves the project's
        // own copy (mod_path set) and edits on top of it.
        save_project(
            base.to_string_lossy().into_owned(),
            Some(project.to_string_lossy().into_owned()),
            None,
            vec![edits::TypedEdit::RenameRuler {
                tag: "FRA".into(),
                name: "Charles III".into(),
            }],
        )
        .unwrap();
        let saved =
            std::fs::read_to_string(project.join("history/countries/FRA - France.txt")).unwrap();
        assert!(saved.contains("\"Charles III\""));
    }
}
