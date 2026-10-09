use std::cmp::Ordering;
use std::fs;
use std::io;
use std::io::Read as _;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{self, AtomicU64};
use std::time::SystemTime;

use ignore::WalkBuilder;

use crate::graphics::Picture;
use crate::graphics::decode;
use crate::syntax::{self, Segments};

pub const MAX_ENTRIES: usize = 10_000;
pub const MAX_BYTES: u64 = 8 << 20;
pub const MAX_IMAGE_BYTES: u64 = 32 << 20;
const BINARY_PROBE: usize = 8000;
const SNIFF_BYTES: u64 = 64;

static PICTURES: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub dir: bool,
}

fn order(a: &Entry, b: &Entry) -> Ordering {
    b.dir.cmp(&a.dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())).then_with(|| a.name.cmp(&b.name))
}

pub fn list(root: &Path, folder: &str) -> io::Result<Vec<Entry>> {
    let dir = if folder.is_empty() { root.to_path_buf() } else { root.join(folder) };
    if !dir.is_dir() {
        return Err(io::Error::from(io::ErrorKind::NotFound));
    }
    let walk =
        WalkBuilder::new(&dir).max_depth(Some(1)).hidden(false).filter_entry(|e| e.file_name() != ".git").build();
    let mut entries: Vec<Entry> = walk
        .filter_map(Result::ok)
        .filter(|item| item.depth() == 1)
        .take(MAX_ENTRIES)
        .map(|item| Entry { name: item.file_name().to_string_lossy().into_owned(), dir: item.path().is_dir() })
        .collect();
    entries.sort_by(order);
    Ok(entries)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Stamp {
    modified: Option<SystemTime>,
    len: u64,
    found: bool,
}

const GONE: Stamp = Stamp { modified: None, len: 0, found: false };

#[derive(Debug, Clone, PartialEq)]
pub enum Body {
    Text { lines: Vec<String>, styles: Option<Vec<Segments>> },
    Image(Arc<Picture>),
    Unreadable { format: &'static str, bytes: u64, reason: String },
    Binary,
    TooLarge(u64),
    Missing,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Content {
    pub stamp: Stamp,
    pub language: Option<&'static str>,
    pub body: Body,
}

impl Content {
    pub fn is_image(&self) -> bool {
        matches!(self.body, Body::Image(_) | Body::Unreadable { .. })
    }

    pub fn lines(&self) -> &[String] {
        match &self.body {
            Body::Text { lines, .. } => lines,
            _ => &[],
        }
    }

    pub fn styles(&self) -> Option<&[Segments]> {
        match &self.body {
            Body::Text { styles, .. } => styles.as_deref(),
            _ => None,
        }
    }
}

pub struct Read {
    pub content: Content,
    pub source: Option<String>,
}

pub fn read(path: &Path, previous: Option<Stamp>) -> Option<Read> {
    let meta = fs::metadata(path).ok();
    let stamp =
        meta.as_ref().map_or(GONE, |meta| Stamp { modified: meta.modified().ok(), len: meta.len(), found: true });
    if previous == Some(stamp) {
        return None;
    }
    let content = |language, body| Content { stamp, language, body };
    let plain = |body| Some(Read { content: content(None, body), source: None });
    let Some(meta) = meta else { return plain(Body::Missing) };
    if !meta.is_file() {
        return plain(Body::Binary);
    }
    let limit = if stamp.len > MAX_BYTES && sniff(path).is_some() { MAX_IMAGE_BYTES } else { MAX_BYTES };
    if stamp.len > limit {
        return plain(Body::TooLarge(stamp.len));
    }
    let Ok(bytes) = head(path, limit + 1) else { return plain(Body::Missing) };
    let size = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    if size > limit {
        return plain(Body::TooLarge(size));
    }
    if let Some(format) = decode::sniff(&bytes) {
        return plain(image(format, &bytes));
    }
    if size > MAX_BYTES {
        return plain(Body::TooLarge(size));
    }
    if bytes[..bytes.len().min(BINARY_PROBE)].contains(&0) {
        return plain(Body::Binary);
    }
    let source = String::from_utf8_lossy(&bytes).into_owned();
    let lines = syntax::lines(&source).map(|(_, line)| syntax::expand(line)).collect();
    let first = syntax::lines(&source).next().map_or("", |(_, line)| line);
    let language = syntax::language(path, first);
    Some(Read { content: content(language, Body::Text { lines, styles: None }), source: Some(source) })
}

fn head(path: &Path, limit: u64) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    fs::File::open(path)?.take(limit).read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn sniff(path: &Path) -> Option<&'static str> {
    decode::sniff(&head(path, SNIFF_BYTES).ok()?)
}

fn image(format: &'static str, bytes: &[u8]) -> Body {
    match decode::decode(bytes, PICTURES.fetch_add(1, atomic::Ordering::Relaxed)) {
        Ok(picture) => Body::Image(Arc::new(picture)),
        Err(reason) => Body::Unreadable { format, bytes: u64::try_from(bytes.len()).unwrap_or(u64::MAX), reason },
    }
}

pub fn highlighted(read: Read) -> Option<Content> {
    let (Some(language), Some(source)) = (read.content.language, read.source.as_deref()) else { return None };
    let styles = syntax::highlight(source, language)?;
    let Body::Text { lines, .. } = read.content.body else { return None };
    Some(Content { body: Body::Text { lines, styles: Some(styles) }, ..read.content })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::{TempDir, git_repo};

    fn names(entries: &[Entry]) -> Vec<(&str, bool)> {
        entries.iter().map(|e| (e.name.as_str(), e.dir)).collect()
    }

    #[test]
    fn lists_folders_first_then_files_by_name() {
        let dir = TempDir::new();
        fs::create_dir(dir.path().join("src")).expect("mkdir");
        fs::write(dir.path().join("b.rs"), "").expect("write");
        fs::write(dir.path().join("A.md"), "").expect("write");
        fs::write(dir.path().join(".env"), "").expect("write");
        let entries = list(dir.path(), "").expect("listed");
        assert_eq!(names(&entries), [("src", true), (".env", false), ("A.md", false), ("b.rs", false)]);
    }

    #[test]
    fn leaves_out_what_git_ignores_and_the_git_folder() {
        let dir = git_repo(&[(".gitignore", "target/\n*.log\n"), ("src/main.rs", "")]);
        fs::create_dir_all(dir.path().join("target/debug")).expect("mkdir");
        fs::write(dir.path().join("src/run.log"), "").expect("write");
        let root: Vec<String> = list(dir.path(), "").expect("listed").into_iter().map(|e| e.name).collect();
        assert!(!root.contains(&"target".to_string()) && !root.contains(&".git".to_string()), "{root:?}");
        assert_eq!(names(&list(dir.path(), "src").expect("listed")), [("main.rs", false)]);
    }

    #[test]
    fn a_missing_folder_is_an_error() {
        assert!(list(TempDir::new().path(), "gone").is_err());
    }

    #[test]
    fn reads_lines_and_the_language() {
        let dir = TempDir::new();
        let path = dir.path().join("main.rs");
        fs::write(&path, "fn main() {\n\tprintln!();\n}\n").expect("write");
        let read = read(&path, None).expect("read");
        assert_eq!(read.content.lines(), ["fn main() {", "    println!();", "}"]);
        assert_eq!(read.content.language, Some("rust"));
        let styled = highlighted(read).expect("highlighted");
        assert_eq!(styled.styles().map(<[Segments]>::len), Some(3));
    }

    #[test]
    fn an_unchanged_file_is_not_read_again() {
        let dir = TempDir::new();
        let path = dir.path().join("a.txt");
        fs::write(&path, "one\n").expect("write");
        let stamp = read(&path, None).expect("read").content.stamp;
        assert!(read(&path, Some(stamp)).is_none());
        fs::write(&path, "one\ntwo\n").expect("write");
        assert_eq!(read(&path, Some(stamp)).map(|r| r.content.lines().len()), Some(2));
    }

    #[test]
    fn binary_and_missing_files_have_no_lines() {
        let dir = TempDir::new();
        let path = dir.path().join("logo.png");
        fs::write(&path, [0x89, b'P', 0, 1]).expect("write");
        let read = read(&path, None).expect("read");
        assert_eq!(read.content.body, Body::Binary);
        fs::remove_file(&path).expect("remove");
        let gone = super::read(&path, Some(read.content.stamp)).expect("read again");
        assert_eq!(gone.content.body, Body::Missing);
        assert!(super::read(&path, Some(gone.content.stamp)).is_none());
    }

    #[test]
    fn a_device_behind_a_link_is_not_read() {
        let dir = TempDir::new();
        let path = dir.path().join("null");
        std::os::unix::fs::symlink("/dev/null", &path).expect("link");
        assert_eq!(read(&path, None).expect("read").content.body, Body::Binary);
    }

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut out = io::Cursor::new(Vec::new());
        image::RgbaImage::new(width, height).write_to(&mut out, image::ImageFormat::Png).expect("encode a png");
        out.into_inner()
    }

    fn picture(content: &Content) -> &Picture {
        match &content.body {
            Body::Image(picture) => picture,
            body => panic!("not an image: {body:?}"),
        }
    }

    #[test]
    fn an_image_is_decoded_again_each_time_it_changes() {
        let dir = TempDir::new();
        let path = dir.path().join("logo");
        fs::write(&path, png(4, 3)).expect("write");
        let first = read(&path, None).expect("read").content;
        assert_eq!((picture(&first).width, picture(&first).height), (4, 3));
        assert!(first.lines().is_empty() && first.language.is_none());

        fs::write(&path, png(6, 2)).expect("write again");
        let second = read(&path, Some(first.stamp)).expect("read again").content;

        assert_eq!((picture(&second).width, picture(&second).height), (6, 2));
        assert_ne!(picture(&first).id, picture(&second).id);
    }

    #[test]
    fn a_broken_image_says_why() {
        let dir = TempDir::new();
        let path = dir.path().join("broken.png");
        let mut bytes = png(4, 3);
        bytes.truncate(40);
        fs::write(&path, &bytes).expect("write");

        let body = read(&path, None).expect("read").content.body;

        assert!(matches!(body, Body::Unreadable { bytes: 40, ref reason, .. } if !reason.is_empty()), "{body:?}");
    }

    #[test]
    fn an_image_may_be_larger_than_a_text_file() {
        let dir = TempDir::new();
        let (photo, text) = (dir.path().join("photo.png"), dir.path().join("big.txt"));
        let mut bytes = png(2, 2);
        bytes.resize(usize::try_from(MAX_BYTES).expect("fits") + 1024, 0);
        fs::write(&photo, &bytes).expect("write the image");
        fs::write(&text, vec![b'a'; bytes.len()]).expect("write the text");

        assert!(matches!(read(&photo, None).expect("read").content.body, Body::Image(_)));
        assert!(matches!(read(&text, None).expect("read").content.body, Body::TooLarge(_)));
    }

    #[test]
    fn an_svg_stays_text() {
        let dir = TempDir::new();
        let path = dir.path().join("icon.svg");
        fs::write(&path, "<svg xmlns=\"http://www.w3.org/2000/svg\"/>\n").expect("write");

        assert_eq!(read(&path, None).expect("read").content.lines().len(), 1);
    }
}
