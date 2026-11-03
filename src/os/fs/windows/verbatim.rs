use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf, Prefix};

const VERBATIM: &str = r"\\?\";
const MAX_PATH: usize = 260;
const MAX_NAME: usize = 255;
const FORBIDDEN: &str = r#"<>:"/\|?*"#;
const DEVICES: [&str; 6] = ["CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$"];
const PORTS: [&str; 2] = ["COM", "LPT"];

pub(crate) fn simplified(path: PathBuf) -> PathBuf {
    let plain = plain(&path).map(PathBuf::from);
    plain.unwrap_or(path)
}

fn plain(path: &Path) -> Option<&str> {
    let rest = path.to_str()?.strip_prefix(VERBATIM)?;
    let mut components = path.components();
    let drive = matches!(
        components.next(),
        Some(Component::Prefix(prefix)) if matches!(prefix.kind(), Prefix::VerbatimDisk(_))
    );
    let rooted = components.next() == Some(Component::RootDir);
    let names = components.all(|component| matches!(component, Component::Normal(name) if usable(name)));
    (drive && rooted && names && rest.encode_utf16().count() < MAX_PATH).then_some(rest)
}

fn usable(name: &OsStr) -> bool {
    name.to_str().is_some_and(|name| {
        name.encode_utf16().count() <= MAX_NAME
            && !name.chars().any(|c| c < ' ' || FORBIDDEN.contains(c))
            && !name.ends_with(['.', ' '])
            && !reserved(name)
    })
}

fn reserved(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or_default().trim_end_matches(' ');
    DEVICES.iter().any(|device| stem.eq_ignore_ascii_case(device)) || PORTS.iter().any(|port| numbered(stem, port))
}

fn numbered(stem: &str, port: &str) -> bool {
    let Some((head, number)) = stem.split_at_checked(port.len()) else { return false };
    let mut digits = number.chars();
    head.eq_ignore_ascii_case(port)
        && matches!((digits.next(), digits.next()), (Some('0'..='9' | '¹' | '²' | '³'), None))
}

#[cfg(test)]
mod tests;
