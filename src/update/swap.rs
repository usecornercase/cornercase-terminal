use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const OLD: &str = "old";
const SPARES: u32 = 100;

struct Names {
    stem: String,
    extension: String,
}

impl Names {
    fn of(exe: &Path) -> Option<Self> {
        let stem = exe.file_stem()?.to_str()?.to_string();
        let extension = match exe.extension() {
            Some(extension) => format!(".{}", extension.to_str()?),
            None => String::new(),
        };
        Some(Self { stem, extension })
    }

    fn old(&self, n: u32) -> String {
        let Self { stem, extension } = self;
        if n == 0 { format!("{stem}.{OLD}{extension}") } else { format!("{stem}.{OLD}.{n}{extension}") }
    }

    fn is_old(&self, name: &str) -> bool {
        let Some(middle) = name
            .strip_prefix(self.stem.as_str())
            .and_then(|rest| rest.strip_prefix('.'))
            .and_then(|rest| rest.strip_prefix(OLD))
            .and_then(|rest| rest.strip_suffix(self.extension.as_str()))
        else {
            return false;
        };
        middle.is_empty()
            || middle.strip_prefix('.').is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
    }
}

pub(crate) fn swap(new: &Path, exe: &Path) -> io::Result<()> {
    if cfg!(windows) { swap_aside(new, exe) } else { fs::rename(new, exe) }
}

fn swap_aside(new: &Path, exe: &Path) -> io::Result<()> {
    let old = set_aside(exe)?;
    fs::rename(new, exe).inspect_err(|_| {
        let _ = fs::rename(&old, exe);
    })
}

fn set_aside(exe: &Path) -> io::Result<PathBuf> {
    remove_old(exe);
    let names = Names::of(exe).ok_or_else(|| unnamed(exe))?;
    let old = (0..SPARES).map(|n| exe.with_file_name(names.old(n))).find(|path| !path.exists()).ok_or_else(|| {
        io::Error::new(io::ErrorKind::AlreadyExists, format!("{SPARES} old copies of `{}` still run", exe.display()))
    })?;
    fs::rename(exe, &old)?;
    Ok(old)
}

fn unnamed(exe: &Path) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, format!("`{}` has no file name", exe.display()))
}

pub(crate) fn remove_old(exe: &Path) {
    let (Some(names), Some(dir)) = (Names::of(exe), exe.parent()) else { return };
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        if entry.file_name().to_str().is_some_and(|name| names.is_old(name)) {
            let _ = fs::remove_file(entry.path());
        }
    }
}

#[cfg(test)]
pub(crate) mod tests;
