use rstest::rstest;
use serde_json::json;

use super::*;

fn machine(name: &str, destination: &str) -> Machine {
    Machine { name: name.into(), icon: "☁".into(), destination: destination.into(), folder: "~/code".into() }
}

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
