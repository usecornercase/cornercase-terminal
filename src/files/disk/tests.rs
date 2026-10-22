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
    assert_eq!(read.lines(), ["fn main() {", "    println!();", "}"]);
    assert_eq!(read.language, Some("rust"));
    let styled = highlighted(read).expect("highlighted");
    assert_eq!(styled.styles().map(<[Segments]>::len), Some(3));
    assert_eq!(styled.text(None).as_deref(), Some("fn main() {\n\tprintln!();\n}\n"));
}

#[test]
fn the_text_is_the_file_as_it_is_on_disk() {
    let dir = TempDir::new();
    let path = dir.path().join("Makefile");
    let source = "all:\r\n\tcargo build\r\n\ntest:\n\tcargo test\n";
    fs::write(&path, source).expect("write");
    let content = read(&path, None).expect("read");
    assert_eq!(content.text(None).as_deref(), Some(source));
    assert_eq!(content.text(Some((2, 2))).as_deref(), Some("\tcargo build"));
    assert_eq!(content.text(Some((4, 9))).as_deref(), Some("test:\n\tcargo test"));
    assert_eq!(content.text(Some((9, 9))).as_deref(), Some(""));
}

#[test]
fn an_unchanged_file_is_not_read_again() {
    let dir = TempDir::new();
    let path = dir.path().join("a.txt");
    fs::write(&path, "one\n").expect("write");
    let stamp = read(&path, None).expect("read").stamp;
    assert!(read(&path, Some(stamp)).is_none());
    fs::write(&path, "one\ntwo\n").expect("write");
    assert_eq!(read(&path, Some(stamp)).map(|r| r.lines().len()), Some(2));
}

#[test]
fn binary_and_missing_files_have_no_lines() {
    let dir = TempDir::new();
    let path = dir.path().join("logo.png");
    fs::write(&path, [0x89, b'P', 0, 1]).expect("write");
    let read = read(&path, None).expect("read");
    assert_eq!(read.body, Body::Binary);
    assert_eq!(read.text(None), None);
    fs::remove_file(&path).expect("remove");
    let gone = super::read(&path, Some(read.stamp)).expect("read again");
    assert_eq!(gone.body, Body::Missing);
    assert!(super::read(&path, Some(gone.stamp)).is_none());
}

#[test]
fn a_device_behind_a_link_is_not_read() {
    let dir = TempDir::new();
    let path = dir.path().join("null");
    std::os::unix::fs::symlink("/dev/null", &path).expect("link");
    assert_eq!(read(&path, None).expect("read").body, Body::Binary);
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
    let first = read(&path, None).expect("read");
    assert_eq!((picture(&first).width, picture(&first).height), (4, 3));
    assert!(first.lines().is_empty() && first.language.is_none());

    fs::write(&path, png(6, 2)).expect("write again");
    let second = read(&path, Some(first.stamp)).expect("read again");

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

    let body = read(&path, None).expect("read").body;

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

    assert!(matches!(read(&photo, None).expect("read").body, Body::Image(_)));
    assert!(matches!(read(&text, None).expect("read").body, Body::TooLarge(_)));
}

#[test]
fn an_svg_stays_text() {
    let dir = TempDir::new();
    let path = dir.path().join("icon.svg");
    fs::write(&path, "<svg xmlns=\"http://www.w3.org/2000/svg\"/>\n").expect("write");

    assert_eq!(read(&path, None).expect("read").lines().len(), 1);
}

mod lines_of_the_source {
    use super::*;
    use crate::text::stretch::{Point, Stretch};

    fn content(source: &str) -> Content {
        let dir = TempDir::new();
        let path = dir.path().join("a.txt");
        fs::write(&path, source).expect("write");
        read(&path, None).expect("read")
    }

    #[test]
    fn each_line_is_the_files_own_text_without_its_break() {
        let content = content("all:\r\n\tcargo build\r\n\ntest:\n\tcargo test");
        let lines: Vec<_> = (1..=5).map(|n| content.source_line(n)).collect();
        assert_eq!(lines, [Some("all:"), Some("\tcargo build"), Some(""), Some("test:"), Some("\tcargo test")]);
        assert_eq!((content.source_line(0), content.source_line(6)), (None, None));
    }

    #[test]
    fn an_empty_file_has_no_lines() {
        assert_eq!(content("").source_line(1), None);
    }

    #[test]
    fn a_stretch_is_cut_from_the_source_with_tabs_kept() {
        let content = content("fn main() {\n\trun();\n}\n");
        let stretch = Stretch { from: Point { line: 1, offset: 3 }, to: Point { line: 2, offset: 2 } };
        assert_eq!(content.cut(stretch).as_deref(), Some("main() {\n\tr"));
        assert_eq!(content.cut(Stretch::within(2, 1..4)).as_deref(), Some("run"));
    }

    #[test]
    fn a_stretch_past_the_end_is_clamped_to_the_text() {
        let content = content("one\ntwo\n");
        let past = Stretch { from: Point { line: 2, offset: 1 }, to: Point { line: 9, offset: 3 } };
        let clamped = content.clamped(past).expect("clamped");
        assert_eq!((clamped.to.line, clamped.to.offset), (2, 3));
        assert_eq!(content.cut(clamped).as_deref(), Some("wo"));
        assert_eq!(content.clamped(Stretch::within(5, 0..1)), None);
    }
}
