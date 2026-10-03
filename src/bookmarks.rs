use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub struct Bookmark {
    pub title: String,
    pub url: String,
}

fn store_path() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("cheetah").join("bookmarks.json"))
}

pub fn load() -> Vec<Bookmark> {
    store_path()
        .and_then(|path| fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save(bookmarks: &[Bookmark]) {
    let Some(path) = store_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_string_pretty(bookmarks) {
        let _ = fs::write(path, text);
    }
}

#[cfg(target_os = "macos")]
fn app_data_dir() -> Option<PathBuf> {
    dirs::data_dir()
}

#[cfg(target_os = "windows")]
fn app_data_dir() -> Option<PathBuf> {
    dirs::data_local_dir()
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn app_data_dir() -> Option<PathBuf> {
    dirs::config_dir()
}

fn browser_roots() -> Vec<(&'static str, PathBuf)> {
    let Some(base) = app_data_dir() else {
        return Vec::new();
    };
    let windows = cfg!(target_os = "windows");
    let macos = cfg!(target_os = "macos");
    let pick = |mac: &str, win: &str, linux: &str| base.join(if macos { mac } else if windows { win } else { linux });
    vec![
        ("Chrome", pick("Google/Chrome", "Google/Chrome/User Data", "google-chrome")),
        ("Edge", pick("Microsoft Edge", "Microsoft/Edge/User Data", "microsoft-edge")),
        ("Brave", pick("BraveSoftware/Brave-Browser", "BraveSoftware/Brave-Browser/User Data", "BraveSoftware/Brave-Browser")),
        ("Chromium", pick("Chromium", "Chromium/User Data", "chromium")),
        ("Vivaldi", pick("Vivaldi", "Vivaldi/User Data", "vivaldi")),
    ]
}

fn collect(node: &Value, out: &mut Vec<Bookmark>) {
    match node["type"].as_str() {
        Some("url") => {
            let url = node["url"].as_str().unwrap_or_default();
            if url.starts_with("http://") || url.starts_with("https://") {
                let title = node["name"].as_str().filter(|n| !n.is_empty()).unwrap_or(url);
                out.push(Bookmark { title: title.to_string(), url: url.to_string() });
            }
        }
        Some("folder") => {
            for child in node["children"].as_array().into_iter().flatten() {
                collect(child, out);
            }
        }
        _ => {}
    }
}

fn read_profile(file: &Path) -> Vec<Bookmark> {
    let Ok(text) = fs::read_to_string(file) else {
        return Vec::new();
    };
    let Ok(json) = serde_json::from_str::<Value>(&text) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    collect(&json["roots"]["bookmark_bar"], &mut out);
    out
}

pub struct ImportResult {
    pub browsers: Vec<&'static str>,
    pub added: usize,
}

pub fn import_all(existing: &mut Vec<Bookmark>) -> ImportResult {
    let mut result = ImportResult { browsers: Vec::new(), added: 0 };
    for (name, root) in browser_roots() {
        let Ok(entries) = fs::read_dir(&root) else {
            continue;
        };
        let mut found = false;
        for entry in entries.flatten() {
            let dir_name = entry.file_name().to_string_lossy().to_string();
            if dir_name != "Default" && !dir_name.starts_with("Profile ") {
                continue;
            }
            for bookmark in read_profile(&entry.path().join("Bookmarks")) {
                found = true;
                if !existing.iter().any(|b| b.url == bookmark.url) {
                    existing.push(bookmark);
                    result.added += 1;
                }
            }
        }
        if found {
            result.browsers.push(name);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collects_nested_http_bookmarks_only() {
        let json: Value = serde_json::from_str(
            r#"{"type":"folder","children":[
                {"type":"url","name":"A","url":"https://a.com"},
                {"type":"url","name":"JS","url":"javascript:void(0)"},
                {"type":"folder","children":[{"type":"url","name":"","url":"http://b.com"}]}
            ]}"#,
        )
        .unwrap();
        let mut out = Vec::new();
        collect(&json, &mut out);
        assert_eq!(out.len(), 2);
        assert_eq!(out[1].title, "http://b.com");
    }
}
