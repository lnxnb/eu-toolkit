use std::fs;
use std::path::Path;

#[derive(Debug, serde::Serialize)]
pub struct ModInfo {
    pub path: String,
    pub name: String,
    pub version: Option<String>,
    pub supported_version: Option<String>,
    pub tags: Vec<String>,
    pub replace_paths: Vec<String>,
    pub has_descriptor: bool,
    pub directories: Vec<String>,
    pub files: Vec<String>,
}

pub fn open_mod(path: &str) -> Result<ModInfo, String> {
    let root = Path::new(path);
    if !root.is_dir() {
        return Err(format!("Not a directory: {path}"));
    }

    let folder_name = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string());

    let descriptor = find_descriptor(root);
    let has_descriptor = descriptor.is_some();

    let mut info = ModInfo {
        path: path.to_string(),
        name: folder_name,
        version: None,
        supported_version: None,
        tags: Vec::new(),
        replace_paths: Vec::new(),
        has_descriptor,
        directories: Vec::new(),
        files: Vec::new(),
    };

    if let Some(text) = descriptor {
        apply_descriptor(&mut info, &text);
    }

    let entries = fs::read_dir(root).map_err(|e| format!("Failed to read directory: {e}"))?;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        match entry.file_type() {
            Ok(t) if t.is_dir() => info.directories.push(name),
            Ok(_) => info.files.push(name),
            Err(_) => {}
        }
    }
    info.directories.sort_unstable_by_key(|s| s.to_lowercase());
    info.files.sort_unstable_by_key(|s| s.to_lowercase());

    Ok(info)
}

/// EU4 mods keep their metadata in `descriptor.mod`; some distributions only
/// ship a differently-named `*.mod` file, so fall back to the first one found.
fn find_descriptor(root: &Path) -> Option<String> {
    let canonical = root.join("descriptor.mod");
    if canonical.is_file() {
        return fs::read_to_string(&canonical).ok();
    }
    let entries = fs::read_dir(root).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() && path.extension().is_some_and(|e| e == "mod") {
            if let Ok(text) = fs::read_to_string(&path) {
                return Some(text);
            }
        }
    }
    None
}

fn apply_descriptor(info: &mut ModInfo, text: &str) {
    for (key, value) in parse_descriptor(text) {
        match key.as_str() {
            "name" => {
                if let Value::Scalar(s) = value {
                    info.name = s;
                }
            }
            "version" => {
                if let Value::Scalar(s) = value {
                    info.version = Some(s);
                }
            }
            "supported_version" => {
                if let Value::Scalar(s) = value {
                    info.supported_version = Some(s);
                }
            }
            "tags" => {
                if let Value::List(items) = value {
                    info.tags = items;
                }
            }
            "replace_path" => {
                if let Value::Scalar(s) = value {
                    info.replace_paths.push(s);
                }
            }
            _ => {}
        }
    }
}

enum Value {
    Scalar(String),
    List(Vec<String>),
}

/// Minimal parser for the Paradox descriptor format: `key="value"` pairs and
/// `key={ "a" "b" }` lists, where lists may span multiple lines.
fn parse_descriptor(text: &str) -> Vec<(String, Value)> {
    let mut pairs = Vec::new();
    let mut chars = text.chars().peekable();

    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
            continue;
        }
        if c == '#' {
            while chars.next_if(|&c| c != '\n').is_some() {}
            continue;
        }

        let mut key = String::new();
        while let Some(&c) = chars.peek() {
            if c.is_alphanumeric() || c == '_' {
                key.push(c);
                chars.next();
            } else {
                break;
            }
        }
        if key.is_empty() {
            chars.next();
            continue;
        }

        while chars.next_if(|&c| c.is_whitespace()).is_some() {}
        if chars.next_if_eq(&'=').is_none() {
            continue;
        }
        while chars.next_if(|&c| c.is_whitespace()).is_some() {}

        match chars.peek() {
            Some('"') => {
                chars.next();
                pairs.push((key, Value::Scalar(read_quoted(&mut chars))));
            }
            Some('{') => {
                chars.next();
                let mut items = Vec::new();
                while let Some(&c) = chars.peek() {
                    match c {
                        '}' => {
                            chars.next();
                            break;
                        }
                        '"' => {
                            chars.next();
                            items.push(read_quoted(&mut chars));
                        }
                        _ => {
                            chars.next();
                        }
                    }
                }
                pairs.push((key, Value::List(items)));
            }
            _ => {
                let mut value = String::new();
                while let Some(&c) = chars.peek() {
                    if c.is_whitespace() {
                        break;
                    }
                    value.push(c);
                    chars.next();
                }
                pairs.push((key, Value::Scalar(value)));
            }
        }
    }

    pairs
}

fn read_quoted(chars: &mut std::iter::Peekable<std::str::Chars>) -> String {
    let mut s = String::new();
    for c in chars.by_ref() {
        if c == '"' {
            break;
        }
        s.push(c);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_typical_descriptor() {
        let text = r#"
name="My Great Mod"
version="1.2"
tags={
	"Gameplay"
	"National Ideas"
}
supported_version="1.37.*"
replace_path="history/wars"
# a comment
path="mod/my_great_mod"
"#;
        let mut info = ModInfo {
            path: String::new(),
            name: "fallback".into(),
            version: None,
            supported_version: None,
            tags: Vec::new(),
            replace_paths: Vec::new(),
            has_descriptor: true,
            directories: Vec::new(),
            files: Vec::new(),
        };
        apply_descriptor(&mut info, text);
        assert_eq!(info.name, "My Great Mod");
        assert_eq!(info.version.as_deref(), Some("1.2"));
        assert_eq!(info.supported_version.as_deref(), Some("1.37.*"));
        assert_eq!(info.tags, vec!["Gameplay", "National Ideas"]);
        assert_eq!(info.replace_paths, vec!["history/wars"]);
    }
}
