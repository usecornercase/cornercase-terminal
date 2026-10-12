use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::{Error, Result};

pub const EXTENSION: &str = "code-workspace";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workspace {
    pub name: String,
    pub folders: Vec<Folder>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Folder {
    pub path: PathBuf,
    pub name: Option<String>,
}

#[derive(Deserialize)]
struct File {
    #[serde(default)]
    folders: Vec<Entry>,
}

#[derive(Deserialize)]
struct Entry {
    path: Option<String>,
    uri: Option<String>,
    name: Option<String>,
}

pub fn is_workspace(path: &Path) -> bool {
    path.extension().is_some_and(|e| e == EXTENSION)
}

pub fn read(file: &Path) -> Result<Workspace> {
    let invalid = |reason: String| Error::CodeWorkspace { path: file.to_path_buf(), reason };
    let text = std::fs::read_to_string(file).map_err(|e| invalid(e.to_string()))?;
    let text = text.trim_start_matches('\u{feff}');
    let parsed: File = serde_json::from_str(&strip_jsonc(text)).map_err(|e| invalid(e.to_string()))?;
    let base = file.parent().unwrap_or(Path::new("/"));
    let name = file.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let folders = parsed
        .folders
        .into_iter()
        .filter_map(|entry| {
            let path = match (entry.path, entry.uri) {
                (Some(path), _) => base.join(path),
                (None, Some(uri)) => PathBuf::from(file_uri(&uri)?),
                (None, None) => return None,
            };
            Some(Folder { path, name: entry.name.filter(|n| !n.trim().is_empty()) })
        })
        .collect();
    Ok(Workspace { name, folders })
}

fn file_uri(uri: &str) -> Option<String> {
    let path = uri.strip_prefix("file://")?;
    let path = path.strip_prefix("localhost").unwrap_or(path);
    if !path.starts_with('/') {
        return None;
    }
    let bytes = path.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = (bytes[i] == b'%').then(|| path.get(i + 1..i + 3)).flatten();
        let (byte, width) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()).map_or((bytes[i], 1), |b| (b, 3));
        out.push(byte);
        i += width;
    }
    String::from_utf8(out).ok()
}

fn strip_jsonc(text: &str) -> String {
    without_trailing_commas(&without_comments(text))
}

fn without_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut in_string = false;
    while let Some(c) = chars.next() {
        if in_string {
            out.push(c);
            match c {
                '\\' => out.extend(chars.next()),
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match (c, chars.peek()) {
            ('"', _) => {
                in_string = true;
                out.push(c);
            }
            ('/', Some('/')) => while chars.next_if(|&c| c != '\n').is_some() {},
            ('/', Some('*')) => {
                chars.next();
                let mut last = ' ';
                for c in chars.by_ref() {
                    if last == '*' && c == '/' {
                        break;
                    }
                    last = c;
                }
                out.push(' ');
            }
            _ => out.push(c),
        }
    }
    out
}

fn without_trailing_commas(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut in_string = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        i += 1;
        if in_string {
            out.push(c);
            match c {
                '\\' if i < chars.len() => {
                    out.push(chars[i]);
                    i += 1;
                }
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }
        if c == '"' {
            in_string = true;
        }
        let closes = chars[i..].iter().find(|c| !c.is_whitespace()).is_some_and(|&c| c == '}' || c == ']');
        if c != ',' || !closes {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests;
