use rstest::rstest;
use serde_json::json;

use super::*;

mod a_name {
    use super::*;

    #[rstest]
    #[case::word("vps")]
    #[case::symbols("pi.lan_2-b")]
    #[case::longest("abcdefghijabcdefghij")]
    fn is_letters_digits_and_dots_dashes_or_underscores(#[case] name: &str) {
        assert_eq!(valid_name(name), Ok(()));
    }

    #[rstest]
    #[case::empty("", "the name is required")]
    #[case::a_space("my vps", "use letters, digits, `.`, `_` and `-`, at most 20 characters")]
    #[case::a_slash("a/b", "use letters, digits, `.`, `_` and `-`, at most 20 characters")]
    #[case::not_ascii("café", "use letters, digits, `.`, `_` and `-`, at most 20 characters")]
    #[case::over_twenty("abcdefghijabcdefghijk", "use letters, digits, `.`, `_` and `-`, at most 20 characters")]
    fn is_refused_otherwise(#[case] name: &str, #[case] why: &str) {
        assert_eq!(valid_name(name), Err(why));
    }

    #[rstest]
    #[case::same("vps")]
    #[case::other_case("VPS")]
    fn already_used_ignoring_case_is_a_conflict(#[case] name: &str) {
        let others = [machine("pi", "pi.lan"), machine("vps", "vps.example.com")];
        assert_eq!(name_taken(&others, name), Some("another machine is already called `vps`".into()));
    }

    #[test]
    fn of_no_other_machine_is_free() {
        assert_eq!(name_taken(&[machine("pi", "pi.lan")], "vps"), None);
    }
}

mod a_destination {
    use super::*;

    #[rstest]
    #[case::host("vps.example.com")]
    #[case::user_and_host("miguel@vps.example.com")]
    #[case::ssh_config_alias("pi")]
    #[case::longest(&"h".repeat(255))]
    fn is_a_host_maybe_with_its_user(#[case] destination: &str) {
        assert_eq!(valid_destination(destination), Ok(()));
    }

    #[rstest]
    #[case::empty("", "the destination is required")]
    #[case::an_option("-oProxyCommand=sh", "a destination cannot start with `-`")]
    #[case::a_space("vps example", "a destination cannot hold spaces, control or invisible characters")]
    #[case::a_tab("vps\texample", "a destination cannot hold spaces, control or invisible characters")]
    #[case::a_control("vps\u{1b}[31m", "a destination cannot hold spaces, control or invisible characters")]
    #[case::invisible("vps\u{200b}", "a destination cannot hold spaces, control or invisible characters")]
    #[case::over_255_bytes(&"é".repeat(128), "a destination is at most 255 bytes")]
    fn is_refused_otherwise(#[case] destination: &str, #[case] why: &str) {
        assert_eq!(valid_destination(destination), Err(why));
    }

    #[test]
    fn already_used_by_another_machine_is_a_conflict() {
        let others = [machine("vps", "miguel@vps.example.com")];
        assert_eq!(
            destination_taken(&others, "miguel@vps.example.com"),
            Some("the machine `vps` already uses this destination".into())
        );
        assert_eq!(destination_taken(&others, "root@vps.example.com"), None);
    }
}

mod a_folder {
    use super::*;

    #[rstest]
    #[case::home("~")]
    #[case::under_home("~/code")]
    #[case::absolute("/srv/code")]
    fn is_absolute_or_under_home(#[case] folder: &str) {
        assert_eq!(valid_folder(folder), Ok(()));
    }

    #[rstest]
    #[case::empty("", "the folder is required")]
    #[case::relative("code", "use an absolute path or one starting with ~/")]
    #[case::another_user_s_home("~root/code", "use an absolute path or one starting with ~/")]
    #[case::a_control("~/code\n", "a folder cannot hold control or invisible characters")]
    fn is_refused_otherwise(#[case] folder: &str, #[case] why: &str) {
        assert_eq!(valid_folder(folder), Err(why));
    }
}

mod an_icon {
    use super::*;

    #[test]
    fn is_one_of_six_glyphs_each_with_its_name() {
        let names: Vec<(char, &str)> = ICONS.to_vec();
        assert_eq!(
            names,
            [('☁', "server"), ('▭', "laptop"), ('▣', "desktop"), ('◫', "mini"), ('⌂', "home server"), ('◆', "other")]
        );
    }

    #[test]
    fn known_is_shown_as_written() {
        for (glyph, _) in ICONS {
            assert_eq!(known_icon(&glyph.to_string()), glyph);
        }
    }

    #[rstest]
    #[case::unknown_glyph("★")]
    #[case::empty("")]
    #[case::two_glyphs("☁▭")]
    #[case::a_name("laptop")]
    fn unknown_is_shown_as_a_server(#[case] icon: &str) {
        assert_eq!(known_icon(icon), '☁');
    }

    #[rstest]
    #[case::its_glyph("◫", Some('◫'))]
    #[case::its_name("home server", Some('⌂'))]
    #[case::its_name_in_capitals("Laptop", Some('▭'))]
    #[case::nothing_known("castle", None)]
    fn is_named_by_its_glyph_or_its_name(#[case] text: &str, #[case] icon: Option<char>) {
        assert_eq!(icon_named(text), icon);
    }
}

mod an_entry_of_the_file {
    use super::*;

    #[test]
    fn reads_its_four_keys() {
        let entry = json!({"name": "vps", "icon": "☁", "destination": "vps.example.com", "folder": "~/code"});
        assert_eq!(serde_json::from_value::<Machine>(entry).expect("an entry"), machine("vps", "vps.example.com"));
    }

    #[rstest]
    #[case::missing_keys(json!({"name": "vps"}), Machine { name: "vps".into(), ..Machine::default() })]
    #[case::other_types(json!({"name": 5, "icon": ["☁"], "destination": null, "folder": true}), Machine::default())]
    #[case::not_an_object(json!("vps"), Machine::default())]
    fn that_is_broken_still_reads_with_what_it_has(#[case] entry: Value, #[case] expected: Machine) {
        assert_eq!(serde_json::from_value::<Machine>(entry).expect("an entry"), expected);
    }

    #[test]
    fn writes_the_icon_as_it_was_written() {
        let odd = Machine { icon: "★".into(), ..machine("vps", "vps.example.com") };
        assert_eq!(serde_json::to_value(&odd).expect("json")["icon"], json!("★"));
    }

    #[test]
    fn that_is_valid_has_no_problem() {
        let machines = [machine("vps", "vps.example.com"), machine("pi", "pi.lan")];
        assert_eq!(problems(&machines), [None, None]);
    }

    #[test]
    fn that_is_invalid_has_its_first_problem() {
        let machines = [
            Machine { name: "my vps".into(), ..machine("vps", "vps.example.com") },
            Machine { destination: String::new(), ..machine("pi", "pi.lan") },
            Machine { folder: "code".into(), ..machine("mini", "mini.lan") },
        ];
        assert_eq!(
            problems(&machines),
            [
                Some("use letters, digits, `.`, `_` and `-`, at most 20 characters".into()),
                Some("the destination is required".into()),
                Some("use an absolute path or one starting with ~/".into()),
            ]
        );
    }

    #[test]
    fn that_repeats_an_earlier_one_is_the_one_with_the_problem() {
        let machines =
            [machine("vps", "vps.example.com"), machine("VPS", "other.example.com"), machine("pi", "vps.example.com")];
        assert_eq!(
            problems(&machines),
            [
                None,
                Some("another machine is already called `vps`".into()),
                Some("the machine `vps` already uses this destination".into()),
            ]
        );
    }
}

mod info {
    use super::*;

    #[test]
    fn is_the_row_of_the_machines_answer() {
        let info = Info {
            name: "vps".into(),
            icon: "☁".into(),
            destination: "miguel@vps.example.com".into(),
            folder: "~/code".into(),
            state: State::Linked,
            version: Some("0.12.225".into()),
            server_version: Some("0.12.225".into()),
            seen: Some("2026-10-29T10:00:00.000Z".into()),
            ..Info::default()
        };
        let expected = json!({
            "name": "vps", "icon": "☁", "destination": "miguel@vps.example.com", "folder": "~/code",
            "state": "linked", "version": "0.12.225", "server_version": "0.12.225",
            "seen": "2026-10-29T10:00:00.000Z", "error": null, "updating": false, "problem": null,
        });
        assert_eq!(serde_json::to_value(&info).expect("json"), expected);
        assert_eq!(serde_json::from_value::<Info>(expected).expect("an info"), info);
    }

    #[rstest]
    #[case::connecting(State::Connecting, "connecting")]
    #[case::linked(State::Linked, "linked")]
    #[case::unreachable(State::Unreachable, "unreachable")]
    #[case::older(State::Older, "older")]
    fn names_its_state_in_lower_case(#[case] state: State, #[case] id: &str) {
        assert_eq!(serde_json::to_value(state).expect("json"), json!(id));
    }

    #[test]
    fn starts_connecting() {
        assert_eq!(Info::default().state, State::Connecting);
    }
}

mod a_status {
    use super::*;

    const NOW: i64 = 1_793_000_000;

    fn at(secs: i64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(u64::try_from(secs).expect("after 1970"))
    }

    fn note(status: &Status) -> String {
        Info::of(&machine("vps", "vps.lan"), Some(status), None, None).note(at(NOW))
    }

    fn linked(release: &str, running: &str) -> Status {
        let mut status = Status::default();
        status.linked(release.into(), running.into(), NOW - 60);
        status
    }

    #[test]
    fn starts_connecting() {
        assert_eq!(note(&Status::default()), "connecting…");
    }

    #[test]
    fn linked_shows_the_cornercase_there() {
        assert_eq!(note(&linked("0.12.225", "0.12.225")), "linked · 0.12.225");
    }

    #[test]
    fn linked_to_an_older_running_server_says_until_when() {
        assert_eq!(
            note(&linked("0.12.225", "0.12.224")),
            "linked · 0.12.225 · its server runs 0.12.224 until it restarts there"
        );
    }

    #[rstest]
    #[case::seconds(8, "unreachable · 8 s")]
    #[case::minutes(3 * 60 + 5, "unreachable · 3 min")]
    #[case::hours(2 * 3_600, "unreachable · 2 h")]
    #[case::a_day(86_400, "unreachable · 1 day")]
    #[case::days(4 * 86_400 + 7, "unreachable · 4 days")]
    fn unreachable_counts_from_the_last_moment_it_was_up(#[case] ago: i64, #[case] expected: &str) {
        let mut status = linked("0.12.225", "0.12.225");
        status.down(Down::Unreachable("Connection reset".into()), NOW - ago);
        assert_eq!(note(&status), expected);
    }

    #[test]
    fn unreachable_since_this_server_started_was_not_seen_yet() {
        let mut status = Status::default();
        status.down(Down::Unreachable("Connection refused".into()), NOW);
        assert_eq!(note(&status), "unreachable · not seen yet");
    }

    #[test]
    fn a_second_failure_keeps_when_it_was_last_up() {
        let mut status = linked("0.12.225", "0.12.225");
        status.down(Down::Unreachable("Connection reset".into()), NOW - 30);
        status.down(Down::Unreachable("Connection refused".into()), NOW - 10);
        assert_eq!((status.seen, note(&status).as_str()), (Some(NOW - 30), "unreachable · 30 s"));
    }

    #[rstest]
    #[case::with_its_release(Some("0.0.1"), "older · 0.0.1")]
    #[case::from_before_the_proxy(None, "older")]
    fn older_names_the_release_there(#[case] there: Option<&str>, #[case] expected: &str) {
        let mut status = Status::default();
        status.down(Down::Older(there.map(String::from)), NOW);
        assert_eq!((status.state, note(&status).as_str()), (State::Older, expected));
    }

    #[test]
    fn updating_says_so_whatever_the_state() {
        let mut status = Status::default();
        status.down(Down::Older(Some("0.0.1".into())), NOW);
        status.updating = true;
        assert_eq!(note(&status), "updating…");
    }

    #[test]
    fn linked_again_forgets_why_it_was_down() {
        let mut status = Status::default();
        status.down(Down::Missing, NOW);
        status.linked("0.12.225".into(), "0.12.225".into(), NOW);
        assert_eq!((status.state, status.down), (State::Linked, None));
    }

    #[test]
    fn linked_while_it_updates_keeps_updating() {
        let mut status = Status::default();
        status.down(Down::Older(Some("0.0.1".into())), NOW);
        status.updating = true;
        status.linked("0.12.225".into(), "0.12.225".into(), NOW);
        assert_eq!((status.state, status.updating), (State::Linked, true));
    }
}

mod a_row {
    use super::*;

    #[test]
    fn of_an_entry_with_a_problem_says_it_and_has_no_link() {
        let info = Info::of(&machine("my vps", "vps.lan"), None, Some("use letters".into()), None);
        assert_eq!(
            (info.state, info.mark(), info.note(SystemTime::now())),
            (State::Unreachable, '○', "use letters".into())
        );
    }

    #[test]
    fn of_a_machine_not_linked_yet_is_connecting() {
        let info = Info::of(&machine("vps", "vps.lan"), None, None, None);
        assert_eq!((info.state, info.mark()), (State::Connecting, '◌'));
    }

    #[test]
    fn shows_an_unknown_icon_as_a_server_and_its_last_moment_up_as_a_time() {
        let mut status = Status::default();
        status.linked("0.12.225".into(), "0.12.225".into(), 1_793_000_000);
        let odd = Machine { icon: "★".into(), ..machine("vps", "vps.lan") };
        let info = Info::of(&odd, Some(&status), None, None);
        assert_eq!((info.icon.as_str(), info.seen.as_deref()), ("☁", Some("2026-10-26T07:33:20.000Z")));
    }

    #[rstest]
    #[case::connecting(State::Connecting, '◌')]
    #[case::linked(State::Linked, '●')]
    #[case::unreachable(State::Unreachable, '○')]
    #[case::older(State::Older, '◐')]
    fn marks_its_state(#[case] state: State, #[case] mark: char) {
        assert_eq!(state.mark(), mark);
    }
}

mod a_logged_error {
    use super::*;

    #[rstest]
    #[case::the_host(
        "ssh: Could not resolve hostname vps.example.com: Name or service not known",
        "ssh: Could not resolve hostname vps: Name or service not known"
    )]
    #[case::the_whole_destination(
        "miguel@vps.example.com: Permission denied (publickey).",
        "vps: Permission denied (publickey)."
    )]
    #[case::neither("Connection refused", "Connection refused")]
    #[case::the_host_at_the_end_of_a_sentence("cannot reach vps.example.com.", "cannot reach vps.")]
    #[case::a_longer_host_that_contains_it(
        "ssh: connect to host db.vps.example.com.au",
        "ssh: connect to host db.vps.example.com.au"
    )]
    fn names_the_machine_instead_of_its_destination(#[case] said: &str, #[case] logged: &str) {
        assert_eq!(hide_destination(said, "vps", "miguel@vps.example.com"), logged);
    }

    #[rstest]
    #[case::inside_a_word("Connection refused", "Connection refused")]
    #[case::as_a_word(
        "ssh: connect to host c port 22: Connection refused",
        "ssh: connect to host vps port 22: Connection refused"
    )]
    fn names_the_machine_only_where_a_short_host_is_a_word_of_its_own(#[case] said: &str, #[case] logged: &str) {
        assert_eq!(hide_destination(said, "vps", "me@c"), logged);
    }
}
