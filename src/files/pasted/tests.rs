use std::os::unix::fs::PermissionsExt;
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;

use super::*;
use crate::test_util::TempDir;

const PNG: &[u8] = b"\x89PNG\r\n\x1a\n rest";
const JPEG: &[u8] = &[0xff, 0xd8, 0xff, 0xe0, 1, 2];

mod names {
    use super::*;

    #[rstest::rstest]
    #[case::plain("shot.png", PNG, "shot.png")]
    #[case::spaces("My Shot (1).png", PNG, "My-Shot-1.png")]
    #[case::parent_folders("../../etc/passwd", b"x", "passwd")]
    #[case::absolute("/Users/me/shot.png", PNG, "shot.png")]
    #[case::hidden(".bashrc", b"x", "bashrc")]
    #[case::nameless_png("", PNG, "pasted.png")]
    #[case::extensionless_jpeg("photo", JPEG, "photo.jpg")]
    #[case::extensionless_text("notes", b"hello", "notes")]
    #[case::extension_is_kept("shot.jpg", PNG, "shot.jpg")]
    #[case::macos_screenshot(
        "Screenshot 2026-10-27 at 10.03.12\u{202f}AM.png",
        PNG,
        "Screenshot-2026-10-27-at-10.03.12-AM.png"
    )]
    fn are_cleaned_so_the_path_needs_no_quoting(#[case] asked: &str, #[case] head: &[u8], #[case] expected: &str) {
        assert_eq!(file_name(asked, head), expected);
    }

    #[test]
    fn a_long_name_is_cut_and_keeps_its_extension() {
        let name = file_name(&format!("{}.png", "a".repeat(300)), PNG);
        assert!(name.len() <= MAX_NAME, "{name}");
        assert_eq!(Path::new(&name).extension().and_then(|e| e.to_str()), Some("png"));
    }
}

fn encoded(bytes: &[u8]) -> String {
    STANDARD.encode(bytes)
}

mod storing {
    use super::*;

    #[test]
    fn the_bytes_land_in_a_private_folder_of_their_own() {
        let temp = TempDir::new();
        let dir = temp.path().join("pasted");
        let stored = store(&dir, "shot.png", &encoded(PNG), &[], SystemTime::now()).expect("stored");
        assert_eq!(fs::read(&stored.path).expect("file"), PNG);
        assert_eq!(stored.bytes, PNG.len() as u64);
        assert_eq!(stored.path, stored.dir.join("shot.png"));
        let mode = |path: &Path| fs::metadata(path).expect("metadata").permissions().mode() & 0o777;
        assert_eq!((mode(&dir), mode(&stored.dir), mode(&stored.path)), (0o700, 0o700, 0o600));
        assert!(stored.dir.file_name().is_some_and(|name| name.to_string_lossy().ends_with('Z')));
    }

    #[test]
    fn the_same_name_twice_gives_two_folders() {
        let temp = TempDir::new();
        let (a, b) = (
            store(temp.path(), "a.txt", &encoded(b"1"), &[], SystemTime::now()).expect("first"),
            store(temp.path(), "a.txt", &encoded(b"2"), &[], SystemTime::now()).expect("second"),
        );
        assert_ne!(a.dir, b.dir);
        assert_eq!((fs::read(a.path).expect("a"), fs::read(b.path).expect("b")), (b"1".to_vec(), b"2".to_vec()));
    }

    #[test]
    fn line_breaks_and_missing_padding_still_decode() {
        let temp = TempDir::new();
        let text = encoded(b"hello world!!");
        let broken = format!("{}\n{}\r\n", &text[..6], text[6..].trim_end_matches('='));
        let stored = store(temp.path(), "n.txt", &broken, &[], SystemTime::now()).expect("stored");
        assert_eq!(fs::read(stored.path).expect("file"), b"hello world!!");
    }

    #[test]
    fn what_is_not_base64_is_refused() {
        let temp = TempDir::new();
        let refused = store(temp.path(), "n.txt", "%%%", &[], SystemTime::now());
        assert!(matches!(refused, Err(Problem::Unfit(message)) if message.contains("not base64")));
        assert_eq!(fs::read_dir(temp.path()).expect("dir").count(), 0);
    }

    #[test]
    fn a_file_over_the_cap_is_refused() {
        let temp = TempDir::new();
        let refused = store_within(
            &Place { dir: temp.path(), keep: &[], cap: 1 << 20 },
            "n.txt",
            &encoded(&vec![0; 2 << 20]),
            SystemTime::now(),
        );
        assert_eq!(refused, Err(Problem::Unfit("the file is 2 MiB, over the 1 MiB `paste-file` takes".into())));
    }
}

mod sweeping {
    use super::*;

    fn folders(dir: &Path) -> Vec<PathBuf> {
        let mut found: Vec<PathBuf> = fs::read_dir(dir).expect("dir").flatten().map(|e| e.path()).collect();
        found.sort();
        found
    }

    fn two(temp: &TempDir) -> (PathBuf, PathBuf) {
        let (old, live) = (temp.path().join("old"), temp.path().join("live"));
        for dir in [&old, &live] {
            fs::create_dir(dir).expect("folder");
            fs::write(dir.join("f"), b"x").expect("file");
        }
        (old, live)
    }

    #[test]
    fn what_is_older_than_the_cutoff_goes_except_what_is_kept() {
        let temp = TempDir::new();
        let (_, live) = two(&temp);
        sweep(temp.path(), std::slice::from_ref(&live), SystemTime::now() + Duration::from_secs(3600));
        assert_eq!(folders(temp.path()), vec![live]);
    }

    #[test]
    fn nothing_goes_when_all_is_newer_than_the_cutoff() {
        let temp = TempDir::new();
        two(&temp);
        sweep(temp.path(), &[], SystemTime::UNIX_EPOCH);
        assert_eq!(folders(temp.path()).len(), 2);
    }

    #[test]
    fn a_missing_folder_is_fine() {
        let temp = TempDir::new();
        sweep(&temp.path().join("nope"), &[], SystemTime::now());
        remove(&[temp.path().join("nope")]);
    }

    #[test]
    fn removing_takes_a_folder_with_its_file() {
        let temp = TempDir::new();
        let (old, live) = two(&temp);
        remove(&[old]);
        assert_eq!(folders(temp.path()), vec![live]);
    }
}
