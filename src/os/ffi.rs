use std::borrow::Cow;
use std::path::{Path, PathBuf};

pub(crate) use imp::{LIST_SEPARATOR, plain, same, to_bytes, to_os, to_path};

pub(crate) fn join(dir: &Path, relative: &str) -> PathBuf {
    plain(&dir.join(to_path(relative.as_bytes()))).into_owned()
}

pub(crate) fn text(path: &Path) -> Cow<'_, str> {
    match to_bytes(path) {
        Cow::Borrowed(bytes) => String::from_utf8_lossy(bytes),
        Cow::Owned(bytes) => Cow::Owned(String::from_utf8_lossy(&bytes).into_owned()),
    }
}

#[cfg(unix)]
mod imp {
    use std::borrow::Cow;
    use std::ffi::{OsStr, OsString};
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    use std::path::{Path, PathBuf};

    pub(crate) const LIST_SEPARATOR: u8 = b':';

    pub(crate) fn to_path(bytes: &[u8]) -> PathBuf {
        PathBuf::from(OsStr::from_bytes(bytes))
    }

    pub(crate) fn to_bytes(path: &Path) -> Cow<'_, [u8]> {
        Cow::Borrowed(path.as_os_str().as_bytes())
    }

    pub(crate) fn to_os(bytes: Vec<u8>) -> OsString {
        OsString::from_vec(bytes)
    }

    pub(crate) fn plain(path: &Path) -> Cow<'_, Path> {
        Cow::Borrowed(path)
    }

    pub(crate) fn same(a: &Path, b: &Path) -> bool {
        a == b
    }
}

#[cfg(not(unix))]
mod imp {
    use std::borrow::Cow;
    use std::ffi::OsString;
    use std::path::{Component, Path, PathBuf, Prefix};

    pub(crate) const LIST_SEPARATOR: u8 = b';';

    pub(crate) fn to_path(bytes: &[u8]) -> PathBuf {
        PathBuf::from(String::from_utf8_lossy(bytes).replace('/', "\\"))
    }

    pub(crate) fn to_bytes(path: &Path) -> Cow<'_, [u8]> {
        Cow::Owned(plain(path).to_string_lossy().replace('\\', "/").into_bytes())
    }

    pub(crate) fn to_os(bytes: Vec<u8>) -> OsString {
        String::from_utf8(bytes).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned()).into()
    }

    pub(crate) fn plain(path: &Path) -> Cow<'_, Path> {
        let mut components = path.components();
        let Some(Component::Prefix(prefix)) = components.next() else { return Cow::Borrowed(path) };
        let head = match prefix.kind() {
            Prefix::VerbatimDisk(drive) => format!("{}:", char::from(drive)),
            Prefix::VerbatimUNC(server, share) => {
                format!(r"\\{}\{}", server.to_string_lossy(), share.to_string_lossy())
            }
            _ => return Cow::Borrowed(path),
        };
        let mut plain = PathBuf::from(head);
        plain.extend(components);
        Cow::Owned(plain)
    }

    pub(crate) fn same(a: &Path, b: &Path) -> bool {
        a == b || comparable(a) == comparable(b)
    }

    fn comparable(path: &Path) -> PathBuf {
        let native = to_path(path.to_string_lossy().as_bytes());
        let plain = plain(&native);
        let mut components = plain.components();
        match components.next() {
            Some(Component::Prefix(prefix)) if matches!(prefix.kind(), Prefix::Disk(_)) => {
                let drive = prefix.as_os_str().to_string_lossy().to_ascii_uppercase();
                let mut upper = PathBuf::from(drive);
                upper.extend(components);
                upper
            }
            _ => plain.into_owned(),
        }
    }
}

#[cfg(test)]
mod tests;
