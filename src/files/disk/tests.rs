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
