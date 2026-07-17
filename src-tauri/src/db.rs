use rusqlite::Connection;
use tauri::Manager;

fn connection(app: &tauri::AppHandle) -> Result<Connection, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to resolve app data directory: {e}"))?;
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Failed to create app data directory: {e}"))?;
    let conn = Connection::open(dir.join("eu_toolkit.db"))
        .map_err(|e| format!("Failed to open database: {e}"))?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );",
    )
    .map_err(|e| format!("Failed to initialize database: {e}"))?;
    Ok(conn)
}

pub fn get_setting(app: &tauri::AppHandle, key: &str) -> Result<Option<String>, String> {
    let conn = connection(app)?;
    conn.query_row(
        "SELECT value FROM settings WHERE key = ?1",
        [key],
        |row| row.get(0),
    )
    .map(Some)
    .or_else(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        e => Err(format!("Failed to read setting {key}: {e}")),
    })
}

pub fn set_setting(app: &tauri::AppHandle, key: &str, value: &str) -> Result<(), String> {
    let conn = connection(app)?;
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [key, value],
    )
    .map_err(|e| format!("Failed to save setting {key}: {e}"))?;
    Ok(())
}

pub fn delete_setting(app: &tauri::AppHandle, key: &str) -> Result<(), String> {
    let conn = connection(app)?;
    conn.execute("DELETE FROM settings WHERE key = ?1", [key])
        .map_err(|e| format!("Failed to delete setting {key}: {e}"))?;
    Ok(())
}

/// All (key, value) pairs whose key begins with `prefix`. Implemented as a range
/// scan (`prefix <= key < prefix++`) so wildcard metacharacters in the prefix
/// (SQL `LIKE` `%`/`_`) can never over-match — mod paths contain `_` freely.
/// Used for the per-(scope, culture) display-color overrides (Sprint 6.1).
pub fn get_settings_prefix(
    app: &tauri::AppHandle,
    prefix: &str,
) -> Result<Vec<(String, String)>, String> {
    let conn = connection(app)?;
    // Upper bound: the prefix with its last byte incremented — the smallest key
    // that is strictly greater than every key starting with `prefix`.
    let mut upper = prefix.as_bytes().to_vec();
    while let Some(last) = upper.last_mut() {
        if *last < 0xff {
            *last += 1;
            break;
        }
        upper.pop();
    }
    let upper = String::from_utf8_lossy(&upper).into_owned();
    let mut stmt = conn
        .prepare("SELECT key, value FROM settings WHERE key >= ?1 AND key < ?2")
        .map_err(|e| format!("Failed to prepare prefix query: {e}"))?;
    let rows = stmt
        .query_map([prefix, upper.as_str()], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| format!("Failed to query settings by prefix: {e}"))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| format!("Failed to read setting row: {e}"))?);
    }
    Ok(out)
}
