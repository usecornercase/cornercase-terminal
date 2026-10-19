use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::agents::Known;

fn listed(links: &[&str], name: &str) -> bool {
    links.iter().any(|link| match link.strip_suffix('*') {
        Some(prefix) => name.starts_with(prefix),
        None => *link == name,
    })
}

pub(crate) fn shared(kind: Known, default: &Path) -> Vec<String> {
    let Some(home) = kind.account() else { return Vec::new() };
    let Ok(entries) = fs::read_dir(home.config(default)) else { return Vec::new() };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| listed(home.links, name))
        .collect();
    names.sort();
    names
}

fn folders(kind: Known, default: &Path, account: &Path) -> Option<(PathBuf, PathBuf)> {
    let home = kind.account()?;
    let (from, to) = (home.config(default), home.config(account));
    (from != to).then_some((from, to))
}

pub(crate) fn ensure(kind: Known, default: &Path, account: &Path) -> io::Result<usize> {
    let Some((from, to)) = folders(kind, default, account) else { return Ok(0) };
    let mut made = 0;
    for name in shared(kind, default) {
        let link = to.join(&name);
        match fs::symlink_metadata(&link) {
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                std::os::unix::fs::symlink(from.join(&name), &link)?;
                made += 1;
            }
            Err(e) => return Err(e),
        }
    }
    Ok(made)
}

pub(crate) fn status(kind: Known, default: &Path, account: &Path) -> Vec<String> {
    let Some((_, to)) = folders(kind, default, account) else { return Vec::new() };
    shared(kind, default)
        .into_iter()
        .filter(|name| fs::symlink_metadata(to.join(name)).is_ok_and(|meta| !meta.file_type().is_symlink()))
        .collect()
}
