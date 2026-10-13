use std::collections::BTreeMap;
use std::io;
use std::path::Path;

fn load(path: &Path) -> BTreeMap<String, String> {
    std::fs::read_to_string(path).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default()
}

pub fn read(path: &Path, key: &str) -> Option<String> {
    load(path).remove(key).map(|value| value.trim().to_string()).filter(|value| !value.is_empty())
}

pub fn write(path: &Path, key: &str, value: &str) -> io::Result<()> {
    let mut secrets = load(path);
    secrets.insert(key.into(), value.trim().into());
    store(path, &secrets)
}

pub fn remove(path: &Path, key: &str) -> io::Result<()> {
    let mut secrets = load(path);
    if secrets.remove(key).is_none() {
        return Ok(());
    }
    if secrets.is_empty() {
        return std::fs::remove_file(path)
            .or_else(|e| if e.kind() == io::ErrorKind::NotFound { Ok(()) } else { Err(e) });
    }
    store(path, &secrets)
}

fn store(path: &Path, secrets: &BTreeMap<String, String>) -> io::Result<()> {
    crate::persist::write(path, secrets, true)
}

#[cfg(test)]
mod tests;
