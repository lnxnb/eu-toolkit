//! Export & Launch (Sprint 30.5): register the mod with the game and boot EU4
//! with it active, bypassing the Paradox launcher.
//!
//! ── Mechanism (investigated on this machine, 2026-07-17) ──────────────────────
//! Three activation paths exist:
//!   1. The Paradox launcher's `launcher-v2.sqlite` (knex-migrated `playsets` /
//!      `playsets_mods` tables) — the launcher's own active playset.
//!   2. `dlc_load.json` `enabled_mods` in the user data folder — the file the
//!      launcher writes for the *game* to read on boot.
//!   3. `steam://rungameid/236850` — launches through the launcher (path 1).
//!
//! We chose **path 2 + a direct `eu4.exe` spawn**:
//!   • Writing the sqlite DB is version-fragile: its schema is under knex
//!     migrations and a bad write can corrupt the user's launcher state. We must
//!     never risk that.
//!   • `eu4.exe`, launched directly, reads `dlc_load.json`'s `enabled_mods`
//!     (that file is the launcher→game contract). Setting it to exactly our
//!     pointer guarantees the mod is active regardless of which playset the user
//!     last selected in the launcher — no launcher round-trip, no playset
//!     ambiguity.
//!   • `disabled_dlcs` is preserved untouched; only `enabled_mods` is rewritten.
//! Evidence: `dlc_load.json` = `{"enabled_mods":[],"disabled_dlcs":[]}`,
//! `eu4.exe` present in the install root, launcher DB tables include the
//! migration-tracked `playsets`/`playsets_mods`.

use std::path::{Path, PathBuf};

use crate::export;

/// The planned (and, unless `dry_run`, executed) launch.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchPlan {
    /// Project display name.
    pub name: String,
    /// The `enabled_mods` entry written (`mod/<stem>.mod`).
    pub enabled_mod: String,
    /// Absolute `dlc_load.json` path that was written.
    pub dlc_load_path: String,
    /// Absolute `eu4.exe` path to be spawned.
    pub exe: String,
    /// True when the caller asked to stop short of spawning the game.
    pub dry_run: bool,
    /// True when the game process was actually spawned.
    pub launched: bool,
}

fn user_data_dir(documents: &Path) -> PathBuf {
    documents
        .join("Paradox Interactive")
        .join("Europa Universalis IV")
}

/// Rewrites `dlc_load.json`'s `enabled_mods` to exactly `[entry]`, preserving
/// `disabled_dlcs` and any other keys already present. Creates the file if
/// absent. Returns the file path written.
pub fn write_dlc_load(documents: &Path, entry: &str) -> Result<PathBuf, String> {
    let dir = user_data_dir(documents);
    std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create {}: {e}", dir.display()))?;
    let path = dir.join("dlc_load.json");

    // Parse the existing file (tolerant of absence / malformed content).
    let mut json: serde_json::Value = std::fs::read_to_string(&path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_else(|| serde_json::json!({ "enabled_mods": [], "disabled_dlcs": [] }));
    if !json.is_object() {
        json = serde_json::json!({ "enabled_mods": [], "disabled_dlcs": [] });
    }
    let obj = json.as_object_mut().unwrap();
    obj.insert(
        "enabled_mods".to_string(),
        serde_json::Value::Array(vec![serde_json::Value::String(entry.to_string())]),
    );
    obj.entry("disabled_dlcs".to_string())
        .or_insert_with(|| serde_json::Value::Array(Vec::new()));

    let text = serde_json::to_string(&json).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| format!("Failed to write dlc_load.json: {e}"))?;
    Ok(path)
}

/// Registers the pointer, writes `dlc_load.json`, and assembles the launch plan
/// — everything up to (but not including) spawning the game. Pure w.r.t. the
/// process table, so it is unit-testable with temp dirs (Sprint 30.6).
pub fn prepare_launch(
    documents: &Path,
    install: &Path,
    mod_dir: &Path,
    dry_run: bool,
) -> Result<LaunchPlan, String> {
    // 1. Ensure the launcher pointer .mod exists (idempotent overwrite).
    let name = export::write_game_pointer(documents, install, mod_dir)?;
    // 2. Compute + write the enabled_mods entry into dlc_load.json.
    let entry = export::enabled_mods_entry(mod_dir)?;
    let dlc_path = write_dlc_load(documents, &entry)?;

    Ok(LaunchPlan {
        name,
        enabled_mod: entry,
        dlc_load_path: dlc_path.to_string_lossy().into_owned(),
        exe: install.join("eu4.exe").to_string_lossy().into_owned(),
        dry_run,
        launched: false,
    })
}

/// True if an `eu4.exe` process is currently running (Windows `tasklist`). Used
/// to refuse a launch while the game is already open. Best-effort: on any
/// failure to query, returns false (don't block on a broken probe).
#[cfg(windows)]
pub fn game_running() -> bool {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    std::process::Command::new("tasklist")
        .args(["/FI", "IMAGENAME eq eu4.exe", "/NH"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_lowercase().contains("eu4.exe"))
        .unwrap_or(false)
}

#[cfg(not(windows))]
pub fn game_running() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("eu_toolkit_launch_test_{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn prepare_writes_pointer_and_dlc_load() {
        let root = temp("prepare");
        let docs = root.join("docs");
        let install = root.join("game");
        let mod_dir = root.join("mymod");
        std::fs::create_dir_all(&docs).unwrap();
        std::fs::create_dir_all(&install).unwrap();
        std::fs::create_dir_all(&mod_dir).unwrap();
        std::fs::write(
            install.join("launcher-settings.json"),
            r#"{ "rawVersion": "v1.37.5.0" }"#,
        )
        .unwrap();
        std::fs::write(mod_dir.join("descriptor.mod"), "name=\"My Mod\"\n").unwrap();

        let plan = prepare_launch(&docs, &install, &mod_dir, true).unwrap();
        assert_eq!(plan.name, "My Mod");
        assert_eq!(plan.enabled_mod, "mod/My Mod.mod");
        assert!(plan.dry_run && !plan.launched);
        assert!(plan.exe.ends_with("eu4.exe"));

        // Pointer .mod written.
        let pointer = docs.join("Paradox Interactive/Europa Universalis IV/mod/My Mod.mod");
        assert!(pointer.is_file());

        // dlc_load.json has our entry and a disabled_dlcs key.
        let dlc = std::fs::read_to_string(&plan.dlc_load_path).unwrap();
        let v: serde_json::Value = serde_json::from_str(&dlc).unwrap();
        assert_eq!(v["enabled_mods"][0], "mod/My Mod.mod");
        assert!(v["disabled_dlcs"].is_array());
    }

    #[test]
    fn write_dlc_load_preserves_disabled_dlcs() {
        let root = temp("preserve");
        let docs = root.join("docs");
        let dir = user_data_dir(&docs);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("dlc_load.json"),
            r#"{"enabled_mods":["mod/old.mod"],"disabled_dlcs":["dlc/foo"]}"#,
        )
        .unwrap();

        write_dlc_load(&docs, "mod/new.mod").unwrap();
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("dlc_load.json")).unwrap())
                .unwrap();
        // enabled_mods replaced with exactly our entry; disabled_dlcs preserved.
        assert_eq!(v["enabled_mods"].as_array().unwrap().len(), 1);
        assert_eq!(v["enabled_mods"][0], "mod/new.mod");
        assert_eq!(v["disabled_dlcs"][0], "dlc/foo");
    }

    #[test]
    fn write_dlc_load_creates_when_absent() {
        let root = temp("absent");
        let docs = root.join("docs");
        let path = write_dlc_load(&docs, "mod/x.mod").unwrap();
        assert!(path.is_file());
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(v["enabled_mods"][0], "mod/x.mod");
        assert!(v["disabled_dlcs"].is_array());
    }
}
