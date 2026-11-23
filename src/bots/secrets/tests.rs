use super::*;
use crate::test_util::TempDir;

const MARKER: &str = "hunter2-marker-value-that-is-long";

fn key() -> Key {
    Key([7; KEY_LEN])
}

fn entry(name: &str, value: &str) -> Entry {
    Entry { name: name.to_string(), value: Secret(value.to_string()), saved: 100 }
}

mod the_file {
    use super::*;

    #[test]
    fn opens_to_what_was_sealed() {
        let entries = vec![entry("pw", MARKER), entry("token", "abc")];
        let sealed = seal(&key(), "campus", &entries).expect("seal");
        assert_eq!(open(&key(), "campus", &sealed), Some(entries));
    }

    #[test]
    fn never_holds_the_value_in_plain_text() {
        let sealed = seal(&key(), "campus", &[entry("pw", MARKER)]).expect("seal");
        assert!(!sealed.windows(MARKER.len()).any(|window| window == MARKER.as_bytes()));
    }

    #[test]
    fn differs_on_every_write() {
        let entries = [entry("pw", MARKER)];
        assert_ne!(seal(&key(), "a", &entries).expect("one"), seal(&key(), "a", &entries).expect("two"));
    }

    #[test]
    fn does_not_open_as_another_bot() {
        let sealed = seal(&key(), "a", &[entry("pw", MARKER)]).expect("seal");
        assert_eq!(open(&key(), "b", &sealed), None);
    }

    #[test]
    fn does_not_open_with_another_key_or_damaged() {
        let sealed = seal(&key(), "a", &[entry("pw", MARKER)]).expect("seal");
        assert_eq!(open(&Key([8; KEY_LEN]), "a", &sealed), None);
        let mut flipped = sealed.clone();
        *flipped.last_mut().expect("byte") ^= 1;
        assert_eq!(open(&key(), "a", &flipped), None);
        assert_eq!(open(&key(), "a", &sealed[..sealed.len() - 3]), None);
        assert_eq!(open(&key(), "a", b"short"), None);
    }
}

mod on_disk {
    use super::*;

    #[test]
    fn saves_loads_and_removes_the_file_with_the_last_secret() {
        let dir = TempDir::new();
        let state = KeyState::Ready(key());
        assert_eq!(load(dir.path(), "a", &state), Kept::Open(Vec::new()));
        save(dir.path(), "a", &key(), &[entry("pw", MARKER)]).expect("save");
        assert_eq!(load(dir.path(), "a", &state).count(), 1);
        save(dir.path(), "a", &key(), &[]).expect("empty");
        assert!(!dir.path().join(folder::SECRETS_FILE).exists());
    }

    #[test]
    fn a_file_that_does_not_open_is_unreadable() {
        let dir = TempDir::new();
        std::fs::write(dir.path().join(folder::SECRETS_FILE), b"garbage").expect("write");
        assert_eq!(load(dir.path(), "a", &KeyState::Ready(key())), Kept::Unreadable);
        save(dir.path(), "a", &key(), &[entry("pw", MARKER)]).expect("save");
        assert_eq!(load(dir.path(), "a", &KeyState::Missing), Kept::Unreadable);
        assert_eq!(load(dir.path(), "a", &KeyState::Broken), Kept::Unreadable);
    }

    #[test]
    fn a_directory_is_unreadable() {
        let dir = TempDir::new();
        std::fs::create_dir(dir.path().join(folder::SECRETS_FILE)).expect("folder");
        assert_eq!(load(dir.path(), "a", &KeyState::Ready(key())), Kept::Unreadable);
    }

    #[cfg(unix)]
    #[test]
    fn a_fifo_is_unreadable_without_waiting() {
        let dir = TempDir::new();
        let path = dir.path().join(folder::SECRETS_FILE);
        assert!(std::process::Command::new("mkfifo").arg(&path).status().expect("mkfifo").success());
        assert_eq!(load(dir.path(), "a", &KeyState::Ready(key())), Kept::Unreadable);
    }

    #[test]
    fn a_key_is_made_only_where_there_is_none() {
        let dir = TempDir::new();
        let path = dir.path().join("k.key");
        std::fs::write(&path, b"somebody else's").expect("write");
        let error = make_key(&path).expect_err("refused");
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read(&path).expect("kept"), b"somebody else's");
        let leftovers = std::fs::read_dir(dir.path()).expect("list").count();
        assert_eq!(leftovers, 1, "no temporary file stays");
    }

    #[test]
    fn the_key_is_missing_ready_or_broken() {
        let dir = TempDir::new();
        let path = dir.path().join("k.key");
        assert_eq!(read_key(&path), KeyState::Missing);
        let made = make_key(&path).expect("make");
        assert_eq!(read_key(&path), KeyState::Ready(made));
        std::fs::write(&path, b"short").expect("write");
        assert_eq!(read_key(&path), KeyState::Broken);
    }
}

mod the_rules {
    use super::*;

    #[rstest::rstest]
    #[case::fine("campus-password_2", true)]
    #[case::empty("", false)]
    #[case::upper("GITHUB_TOKEN", true)]
    #[case::mixed("Campus-Password", true)]
    #[case::accented("contraseña", false)]
    #[case::space("a b", false)]
    #[case::forty("a".repeat(40).leak(), true)]
    #[case::forty_one("a".repeat(41).leak(), false)]
    fn names(#[case] name: &str, #[case] fine: bool) {
        assert_eq!(name_refusal(name).is_none(), fine);
    }

    #[test]
    fn a_name_is_the_same_however_its_letters_are_cased() {
        let kept = Kept::Open(vec![entry("GITHUB_TOKEN", "first")]);
        assert_eq!(kept.find("github_token").map(|found| found.name.as_str()), Some("GITHUB_TOKEN"));
        let replaced = kept.with("Github_Token", "second", 2);
        assert_eq!(replaced.len(), 1);
        assert_eq!((replaced[0].name.as_str(), replaced[0].value.0.as_str()), ("Github_Token", "second"));
        assert_eq!(kept.without("github_token"), Vec::new());
    }

    #[test]
    fn values_are_refused_with_their_size() {
        assert_eq!(value_refusal(""), Some(String::from("the value is empty")));
        assert_eq!(value_refusal(&"a".repeat(8192)), None);
        let refused = value_refusal(&"a".repeat(8193)).expect("refused");
        assert!(refused.contains("8193") && refused.contains("8,192"));
    }
}

mod redacting {
    use super::*;

    #[test]
    fn replaces_every_occurrence_with_the_name() {
        let entries = [entry("pw", MARKER)];
        assert_eq!(redact(&format!("curl -u me:{MARKER} {MARKER}"), &entries), "curl -u me:[secret pw] [secret pw]");
    }

    #[test]
    fn takes_the_longest_value_first() {
        let entries = [entry("short", "abcdef"), entry("long", "abcdefghij")];
        assert_eq!(redact("x abcdefghij y abcdef", &entries), "x [secret long] y [secret short]");
    }

    #[test]
    fn never_scans_what_it_wrote_so_names_that_hold_values_do_not_multiply() {
        let entries = [entry("bbbbbb", "aaaaaa"), entry("aaaaaa", "bbbbbb")];
        assert_eq!(redact("aaaaaa bbbbbb", &entries), "[secret bbbbbb] [secret aaaaaa]");
        let many: Vec<Entry> = (0..13).map(|n| entry(&"x".repeat(30), &format!("value{n:02}"))).collect();
        let text = many.iter().map(|e| e.value.0.clone()).collect::<Vec<_>>().join(" ");
        assert!(redact(&text, &many).len() < 1000);
    }

    #[test]
    fn a_big_answer_full_of_matches_is_not_quadratic() {
        let text = "secret".repeat(200_000);
        let started = std::time::Instant::now();
        let out = redact(&text, &[entry("pw", "secret")]);
        assert_eq!(out.len(), "[secret pw]".len() * 200_000);
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
    }

    #[test]
    fn leaves_short_values_alone() {
        assert_eq!(redact("abc abcde", &[entry("pin", "abc"), entry("five", "abcde")]), "abc abcde");
    }

    #[test]
    fn covers_the_lines_that_carry_text() {
        let kept = Kept::Open(vec![entry("pw", MARKER)]);
        let text = format!("uses {MARKER}");
        for kind in [Kind::Step, Kind::Bot, Kind::Error, Kind::Started, Kind::Finished, Kind::Waiting] {
            assert_eq!(kept.redacted(&[], Line::new(1, 1, kind, text.clone())).text, "uses [secret pw]");
        }
        assert_eq!(kept.redacted(&[], Line::new(1, 1, Kind::You, text.clone())).text, text);
        let mut with_body = Line::new(1, 1, Kind::Finished, "done");
        with_body.body = Some(format!("{MARKER}!"));
        assert_eq!(kept.redacted(&[], with_body).body.as_deref(), Some("[secret pw]!"));
    }
}

#[test]
fn nothing_shows_a_value_or_a_key_in_debug() {
    let entry = entry("pw", MARKER);
    let kept = Kept::Open(vec![entry.clone()]);
    let shown = format!("{entry:?} {kept:?} {:?} {:?}", key(), KeyState::Ready(key()));
    assert!(!shown.contains(MARKER) && !shown.contains('7'));
}

#[cfg(unix)]
#[test]
fn a_filesystem_that_refuses_hard_links_still_gets_a_key() {
    use std::os::unix::fs::PermissionsExt as _;
    let dir = TempDir::new();
    let path = dir.path().join("k.key");
    create_in_place(&path, &[5u8; KEY_LEN]).expect("created in place");
    assert_eq!(std::fs::read(&path).expect("key"), [5u8; KEY_LEN]);
    assert_eq!(std::fs::metadata(&path).expect("meta").permissions().mode() & 0o777, 0o600);
    assert_eq!(create_in_place(&path, &[6u8; KEY_LEN]).expect_err("kept").kind(), io::ErrorKind::AlreadyExists);
    assert_eq!(std::fs::read(&path).expect("key"), [5u8; KEY_LEN]);
}
