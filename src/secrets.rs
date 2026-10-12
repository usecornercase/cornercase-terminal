use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

const FILE: &str = "secrets.json";

pub fn path(config_path: &Path) -> PathBuf {
    config_path.with_file_name(FILE)
}

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
    crate::state::write(path, secrets, true)
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;
    use crate::test_util::TempDir;

    fn file(tmp: &TempDir) -> PathBuf {
        path(&tmp.path().join("config.json"))
    }

    #[test]
    fn lives_next_to_the_config() {
        assert_eq!(path(Path::new("/c/cornercase/config.json")), PathBuf::from("/c/cornercase/secrets.json"));
    }

    #[test]
    fn a_written_secret_reads_back() {
        let tmp = TempDir::new();
        write(&file(&tmp), "shortcut_token", " t0k ").expect("write");
        assert_eq!(read(&file(&tmp), "shortcut_token").as_deref(), Some("t0k"));
    }

    #[test]
    fn only_the_owner_can_read_the_file() {
        let tmp = TempDir::new();
        write(&file(&tmp), "k", "v").expect("write");
        let mode = std::fs::metadata(file(&tmp)).expect("metadata").permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn writing_one_keeps_the_others() {
        let tmp = TempDir::new();
        write(&file(&tmp), "a", "1").expect("write");
        write(&file(&tmp), "b", "2").expect("write");
        assert_eq!((read(&file(&tmp), "a"), read(&file(&tmp), "b")), (Some("1".into()), Some("2".into())));
    }

    #[test]
    fn removing_the_last_one_deletes_the_file() {
        let tmp = TempDir::new();
        write(&file(&tmp), "a", "1").expect("write");
        remove(&file(&tmp), "a").expect("remove");
        assert!(!file(&tmp).exists());
    }

    #[test]
    fn a_missing_file_has_no_secrets() {
        let tmp = TempDir::new();
        assert_eq!(read(&file(&tmp), "a"), None);
    }
}
