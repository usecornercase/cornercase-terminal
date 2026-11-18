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
    #[case::upper("Campus", false)]
    #[case::space("a b", false)]
    #[case::forty("a".repeat(40).leak(), true)]
    #[case::forty_one("a".repeat(41).leak(), false)]
    fn names(#[case] name: &str, #[case] fine: bool) {
        assert_eq!(name_refusal(name).is_none(), fine);
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
    fn leaves_short_values_alone() {
        assert_eq!(redact("abc abcde", &[entry("pin", "abc"), entry("five", "abcde")]), "abc abcde");
    }

    #[test]
    fn covers_the_lines_that_carry_text() {
        let kept = Kept::Open(vec![entry("pw", MARKER)]);
        let text = format!("uses {MARKER}");
        for kind in [Kind::Step, Kind::Bot, Kind::Error, Kind::Started, Kind::Finished, Kind::Waiting] {
            assert_eq!(kept.redacted(Line::new(1, 1, kind, text.clone())).text, "uses [secret pw]");
        }
        assert_eq!(kept.redacted(Line::new(1, 1, Kind::You, text.clone())).text, text);
        let mut with_body = Line::new(1, 1, Kind::Finished, "done");
        with_body.body = Some(format!("{MARKER}!"));
        assert_eq!(kept.redacted(with_body).body.as_deref(), Some("[secret pw]!"));
    }
}

#[test]
fn nothing_shows_a_value_or_a_key_in_debug() {
    let entry = entry("pw", MARKER);
    let kept = Kept::Open(vec![entry.clone()]);
    let shown = format!("{entry:?} {kept:?} {:?} {:?}", key(), KeyState::Ready(key()));
    assert!(!shown.contains(MARKER) && !shown.contains('7'));
}
