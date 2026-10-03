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
    pub blocked: Vec<&'static str>,
    pub added: usize,
}

pub fn import_all(existing: &mut Vec<Bookmark>) -> ImportResult {
    let mut result = ImportResult { browsers: Vec::new(), blocked: Vec::new(), added: 0 };
    let mut merge = |name: &'static str, found: Vec<Bookmark>, result: &mut ImportResult| {
        if found.is_empty() {
            return;
        }
        result.browsers.push(name);
        for bookmark in found {
            if !existing.iter().any(|b| b.url == bookmark.url) {
                existing.push(bookmark);
                result.added += 1;
            }
        }
    };
    merge("Firefox", read_firefox(), &mut result);
    match read_safari() {
        SafariOutcome::Found(list) => merge("Safari", list, &mut result),
        SafariOutcome::Blocked => result.blocked.push("Safari"),
        SafariOutcome::Missing => {}
    }
    for (name, root) in browser_roots() {
        let Ok(entries) = fs::read_dir(&root) else {
            continue;
        };
        let mut found = Vec::new();
        for entry in entries.flatten() {
            let dir_name = entry.file_name().to_string_lossy().to_string();
            if dir_name != "Default" && !dir_name.starts_with("Profile ") {
                continue;
            }
            found.extend(read_profile(&entry.path().join("Bookmarks")));
        }
        merge(name, found, &mut result);
    }
    result
}

fn firefox_profiles_dir() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    if cfg!(target_os = "macos") {
        Some(home.join("Library/Application Support/Firefox/Profiles"))
    } else if cfg!(target_os = "windows") {
        dirs::data_dir().map(|d| d.join("Mozilla/Firefox/Profiles"))
    } else {
        Some(home.join(".mozilla/firefox"))
    }
}

fn lz4_block_decode(input: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut i = 0;
    let read_length = |i: &mut usize, base: usize| -> Option<usize> {
        let mut length = base;
        if base == 15 {
            loop {
                let byte = *input.get(*i)?;
                *i += 1;
                length += byte as usize;
                if byte != 255 {
                    break;
                }
            }
        }
        Some(length)
    };
    while i < input.len() {
        let token = input[i];
        i += 1;
        let literals = read_length(&mut i, (token >> 4) as usize)?;
        out.extend_from_slice(input.get(i..i + literals)?);
        i += literals;
        if i >= input.len() {
            break;
        }
        let offset = u16::from_le_bytes([*input.get(i)?, *input.get(i + 1)?]) as usize;
        i += 2;
        let match_len = read_length(&mut i, (token & 15) as usize)? + 4;
        if offset == 0 || offset > out.len() {
            return None;
        }
        for _ in 0..match_len {
            out.push(out[out.len() - offset]);
        }
    }
    Some(out)
}

fn collect_firefox(node: &Value, out: &mut Vec<Bookmark>) {
    if let Some(uri) = node["uri"].as_str()
        && (uri.starts_with("http://") || uri.starts_with("https://"))
    {
        let title = node["title"].as_str().filter(|t| !t.is_empty()).unwrap_or(uri);
        out.push(Bookmark { title: title.to_string(), url: uri.to_string() });
    }
    for child in node["children"].as_array().into_iter().flatten() {
        collect_firefox(child, out);
    }
}

fn read_firefox() -> Vec<Bookmark> {
    let mut out = Vec::new();
    let Some(profiles) = firefox_profiles_dir().and_then(|dir| fs::read_dir(dir).ok()) else {
        return out;
    };
    for profile in profiles.flatten() {
        let Ok(files) = fs::read_dir(profile.path().join("bookmarkbackups")) else {
            continue;
        };
        let Some(latest) = files.flatten().map(|f| f.path()).filter(|p| p.extension().is_some_and(|e| e == "jsonlz4")).max()
        else {
            continue;
        };
        let Ok(data) = fs::read(latest) else {
            continue;
        };
        let Some(json) = data
            .strip_prefix(b"mozLz40\0")
            .and_then(|rest| rest.get(4..))
            .and_then(lz4_block_decode)
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        else {
            continue;
        };
        let toolbar = json["children"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|n| n["guid"].as_str() == Some("toolbar_____"));
        if let Some(toolbar) = toolbar {
            collect_firefox(toolbar, &mut out);
        }
    }
    out
}

enum SafariOutcome {
    Found(Vec<Bookmark>),
    Blocked,
    Missing,
}

fn collect_safari(node: &Value, out: &mut Vec<Bookmark>) {
    if let Some(url) = node["URLString"].as_str()
        && (url.starts_with("http://") || url.starts_with("https://"))
    {
        let title = node["URIDictionary"]["title"].as_str().filter(|t| !t.is_empty()).unwrap_or(url);
        out.push(Bookmark { title: title.to_string(), url: url.to_string() });
    }
    for child in node["Children"].as_array().into_iter().flatten() {
        collect_safari(child, out);
    }
}

#[cfg(target_os = "macos")]
fn read_safari() -> SafariOutcome {
    let Some(path) = dirs::home_dir().map(|h| h.join("Library/Safari/Bookmarks.plist")) else {
        return SafariOutcome::Missing;
    };
    let converted = std::process::Command::new("plutil")
        .args(["-convert", "json", "-o", "-"])
        .arg(&path)
        .output();
    let Ok(output) = converted else {
        return SafariOutcome::Missing;
    };
    let Ok(json) = serde_json::from_slice::<Value>(&output.stdout) else {
        return if path.exists() { SafariOutcome::Blocked } else { SafariOutcome::Missing };
    };
    let mut out = Vec::new();
    let bar = json["Children"].as_array().into_iter().flatten().find(|n| n["Title"].as_str() == Some("BookmarksBar"));
    if let Some(bar) = bar {
        collect_safari(bar, &mut out);
    }
    SafariOutcome::Found(out)
}

#[cfg(not(target_os = "macos"))]
fn read_safari() -> SafariOutcome {
    SafariOutcome::Missing
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

    #[test]
    fn decodes_lz4_blocks_with_matches() {
        let compressed = [0x32, b'a', b'b', b'c', 3, 0];
        assert_eq!(lz4_block_decode(&compressed).unwrap(), b"abcabcabc");
        assert!(lz4_block_decode(&[0x12, b'a', 9, 0]).is_none());
    }

    #[test]
    fn collects_firefox_and_safari_nodes() {
        let firefox: Value = serde_json::from_str(
            r#"{"children":[{"uri":"https://f.com","title":"F"},{"children":[{"uri":"place:x"}]}]}"#,
        )
        .unwrap();
        let mut out = Vec::new();
        collect_firefox(&firefox, &mut out);
        assert_eq!(out.len(), 1);

        let safari: Value = serde_json::from_str(
            r#"{"Children":[{"URLString":"https://s.com","URIDictionary":{"title":"S"}}]}"#,
        )
        .unwrap();
        let mut out = Vec::new();
        collect_safari(&safari, &mut out);
        assert_eq!(out[0].title, "S");
    }
}
