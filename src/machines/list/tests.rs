use rstest::rstest;

use super::*;
use crate::machines::machine;
use crate::test_util::TempDir;

fn here_in(dir: &TempDir) -> Here {
    Here { config: dir.path().join("config.json"), legacy: dir.path().join("machines.json") }
}

fn text(file: &Path) -> String {
    std::fs::read_to_string(file).expect("the file")
}

fn remote(destination: &str, command: Option<&str>) -> Remote {
    Remote { destination: destination.into(), command: command.map(Into::into), option: "--command" }
}

fn names(list: &[Machine]) -> Vec<&str> {
    list.iter().map(|machine| machine.name.as_str()).collect()
}

mod naming {
    use super::*;

    #[rstest]
    #[case::a_host("devbox", "devbox")]
    #[case::an_address("me@10.0.0.5", "10.0.0.5")]
    #[case::a_url("ssh://me@gpu-box:2222", "gpu-box")]
    #[case::a_long_host("abcdefghijabcdefghijabcdefghij", "abcdefghijabcdefghij")]
    #[case::the_reserved_word("ALL", "machine")]
    #[case::nothing_left("me@", "machine")]
    #[case::an_ipv6_address("me@[fe80::1]:22", "fe80--1")]
    fn comes_from_the_host(#[case] destination: &str, #[case] name: &str) {
        assert_eq!(name_for(destination, &[]), name);
    }

    #[test]
    fn is_not_one_that_is_taken_whatever_its_case() {
        assert_eq!(name_for("devbox", &[machine("DevBox", "a")]), "devbox-2");
    }

    #[test]
    fn counts_up_until_one_is_free() {
        let taken = [machine("devbox", "a"), machine("devbox-2", "b")];

        assert_eq!(name_for("devbox", &taken), "devbox-3");
    }

    #[test]
    fn is_cut_so_that_its_suffix_fits() {
        let long = "abcdefghijabcdefghij";

        let name = name_for(long, &[machine(long, "a")]);

        assert_eq!(name, "abcdefghijabcdefgh-2");
        assert_eq!(name.len(), 20);
    }
}

mod reading {
    use super::*;

    #[test]
    fn a_missing_file_knows_no_machine() {
        let dir = TempDir::new();

        assert_eq!(read(&here_in(&dir)), []);
        assert!(!here_in(&dir).config.exists());
    }

    #[test]
    fn a_broken_file_knows_no_machine() {
        let dir = TempDir::new();
        std::fs::write(&here_in(&dir).config, "{").expect("broken");

        assert_eq!(read(&here_in(&dir)), []);
    }

    #[test]
    fn entries_come_in_the_order_of_the_file_whatever_else_it_holds() {
        let dir = TempDir::new();
        let written = r#"{"fetch_minutes": "bad", "machines": [
            {"name":"b","icon":"☁","destination":"b.lan","folder":"~"},
            {"name":"a","icon":"☁","destination":"a.lan","folder":"~","shown":false}]}"#;
        std::fs::write(&here_in(&dir).config, written).expect("written");

        let list = read(&here_in(&dir));

        assert_eq!(names(&list), ["b", "a"]);
        assert!(list[0].shown && !list[1].shown);
    }
}

mod changing {
    use super::*;

    #[test]
    fn writers_at_once_lose_none() {
        let dir = TempDir::new();
        let here = here_in(&dir);
        let writers = 8;

        std::thread::scope(|scope| {
            for i in 0..writers {
                let here = &here;
                scope.spawn(move || {
                    change(here, |list| {
                        std::thread::sleep(std::time::Duration::from_millis(15));
                        list.push(machine(&format!("m{i}"), &format!("m{i}.lan")));
                    })
                    .expect("written");
                });
            }
        });

        assert_eq!(read(&here).len(), writers);
    }

    #[test]
    fn a_migration_that_cannot_finish_stops_the_change() {
        let dir = TempDir::new();
        let here = here_in(&dir);
        std::fs::write(&here.legacy, "[nope").expect("written");

        assert!(forget(&here, "vps.lan").is_err());
        assert!(change(&here, |list| list.push(machine("vps", "vps.lan"))).is_err());

        assert!(!here.config.exists());
    }

    #[test]
    fn a_missing_file_gets_only_the_machines() {
        let dir = TempDir::new();

        change(&here_in(&dir), |list| list.push(machine("vps", "vps.lan"))).expect("written");

        let written: Value = serde_json::from_str(&text(&here_in(&dir).config)).expect("json");
        assert_eq!(written.as_object().map(serde_json::Map::len), Some(1));
        assert_eq!(written["machines"][0]["name"], "vps");
    }

    #[test]
    fn a_broken_file_is_refused_and_left_as_it_is() {
        let dir = TempDir::new();
        std::fs::write(&here_in(&dir).config, "{ nope").expect("broken");

        assert!(change(&here_in(&dir), |list| list.push(machine("vps", "vps.lan"))).is_err());

        assert_eq!(text(&here_in(&dir).config), "{ nope");
    }

    #[test]
    fn an_unchanged_list_writes_nothing() {
        let dir = TempDir::new();
        let by_hand = r#"{"machines":[{"name":"vps","icon":"☁","destination":"vps.lan","folder":"~"}]}"#;
        std::fs::write(&here_in(&dir).config, by_hand).expect("written");

        change(&here_in(&dir), |_| ()).expect("fine");

        assert_eq!(text(&here_in(&dir).config), by_hand);
    }
}

mod reaching {
    use super::*;

    #[test]
    fn a_command_with_a_control_character_leaves_the_list_as_it_is() {
        let dir = TempDir::new();
        let here = here_in(&dir);
        change(&here, |list| list.push(machine("vps", "vps.lan"))).expect("written");

        reached(&here, &remote("vps.lan", Some("a\tb"))).expect("fine");

        assert_eq!(read(&here), [machine("vps", "vps.lan")]);
    }

    #[test]
    fn a_new_machine_is_not_shown_and_gets_a_name_and_the_home_folder() {
        let dir = TempDir::new();

        reached(&here_in(&dir), &remote("me@devbox:22", Some("/opt/cc"))).expect("written");

        let list = read(&here_in(&dir));
        assert_eq!(names(&list), ["devbox"]);
        assert!(!list[0].shown && !list[0].unreached);
        assert_eq!((list[0].folder.as_str(), list[0].command.as_deref()), ("~", Some("/opt/cc")));
    }

    #[test]
    fn a_known_one_takes_the_last_command_and_loses_its_mark() {
        let dir = TempDir::new();
        remembered(&here_in(&dir), "devbox".into(), Some("/opt/cc".into())).expect("written");

        reached(&here_in(&dir), &remote("devbox", None)).expect("written");

        let list = read(&here_in(&dir));
        assert_eq!((list.len(), list[0].command.clone(), list[0].unreached), (1, None, false));
    }

    #[test]
    fn the_same_link_again_writes_nothing() {
        let dir = TempDir::new();
        reached(&here_in(&dir), &remote("devbox", None)).expect("written");
        let by_hand = text(&here_in(&dir).config).replace(',', ", ");
        std::fs::write(&here_in(&dir).config, &by_hand).expect("rewritten");

        reached(&here_in(&dir), &remote("devbox", None)).expect("fine");

        assert_eq!(text(&here_in(&dir).config), by_hand);
    }

    #[test]
    fn a_destination_ssh_would_take_for_an_option_writes_nothing() {
        let dir = TempDir::new();

        reached(&here_in(&dir), &remote("-oProxyCommand=sh", None)).expect("fine");

        assert!(!here_in(&dir).config.exists());
    }
}

mod remembering {
    use super::*;

    #[test]
    fn an_invisible_destination_or_a_command_with_a_tab_is_refused_and_nothing_is_written() {
        let dir = TempDir::new();

        assert!(remembered(&here_in(&dir), "box\u{200b}".into(), None).is_err());
        assert!(remembered(&here_in(&dir), "box".into(), Some("a\tb".into())).is_err());

        assert!(!here_in(&dir).config.exists());
    }

    #[test]
    fn a_new_machine_waits_for_a_window() {
        let dir = TempDir::new();

        let kept = remembered(&here_in(&dir), "newbox".into(), Some("/opt/cc".into())).expect("written");

        assert!(kept.unreached && !kept.shown);
        assert_eq!(read(&here_in(&dir)), [kept]);
    }

    #[test]
    fn a_known_one_keeps_its_mark_and_takes_the_last_command() {
        let dir = TempDir::new();
        remembered(&here_in(&dir), "newbox".into(), Some("/opt/cc".into())).expect("written");

        let kept = remembered(&here_in(&dir), "newbox".into(), None).expect("written");

        assert!(kept.unreached);
        assert_eq!(kept.command, None);
    }

    #[test]
    fn a_machine_a_window_reached_is_not_marked() {
        let dir = TempDir::new();
        reached(&here_in(&dir), &remote("devbox", None)).expect("written");

        let kept = remembered(&here_in(&dir), "devbox".into(), Some("/opt/cc".into())).expect("written");

        assert!(!kept.unreached);
    }

    #[test]
    fn forgetting_removes_it_and_an_unknown_one_creates_no_file() {
        let dir = TempDir::new();
        assert!(!forget(&here_in(&dir), "devbox").expect("nothing to write"));
        assert!(!here_in(&dir).config.exists());
        remembered(&here_in(&dir), "devbox".into(), None).expect("written");

        assert!(forget(&here_in(&dir), "devbox").expect("written"));

        assert_eq!(read(&here_in(&dir)), []);
    }
}

mod migrating {
    use super::*;

    const LEGACY: &str =
        r#"[{"destination":"me@devbox","command":"/opt/cc","unreached":true},{"destination":"gpu-box"}]"#;

    #[test]
    fn the_machines_a_window_reached_join_the_list_not_shown_and_the_file_goes() {
        let dir = TempDir::new();
        let here = here_in(&dir);
        std::fs::write(&here.legacy, LEGACY).expect("written");

        let list = read(&here);

        assert_eq!(names(&list), ["devbox", "gpu-box"]);
        assert!(list.iter().all(|machine| !machine.shown && machine.folder == "~"));
        assert_eq!((list[0].command.as_deref(), list[0].unreached), (Some("/opt/cc"), true));
        assert!(!here.legacy.exists());
    }

    #[test]
    fn a_known_destination_takes_a_command_only_when_it_had_none() {
        let dir = TempDir::new();
        let here = here_in(&dir);
        let own = Machine { command: Some("mine".into()), ..machine("a", "a.lan") };
        change(&here, |list| list.extend([own, machine("b", "b.lan")])).expect("written");
        let old = r#"[{"destination":"a.lan","command":"theirs"},{"destination":"b.lan","command":"theirs"}]"#;
        std::fs::write(&here.legacy, old).expect("written");

        let list = read(&here);

        assert_eq!(list.len(), 2);
        assert_eq!((list[0].command.as_deref(), list[1].command.as_deref()), (Some("mine"), Some("theirs")));
    }

    #[test]
    fn a_broken_config_keeps_the_old_file_and_is_not_written() {
        let dir = TempDir::new();
        let here = here_in(&dir);
        std::fs::write(&here.legacy, LEGACY).expect("written");
        std::fs::write(&here.config, "{ nope").expect("broken");

        assert!(migrate(&here).is_err());

        assert_eq!(text(&here.legacy), LEGACY);
        assert_eq!(text(&here.config), "{ nope");
    }

    #[test]
    fn broken_and_option_like_entries_are_skipped() {
        let dir = TempDir::new();
        let here = here_in(&dir);
        let old = r#"[42,{"destination":["x"]},{"command":"/opt/cc"},{"destination":"-oProxyCommand=sh"},{"destination":"devbox"}]"#;
        std::fs::write(&here.legacy, old).expect("written");

        assert_eq!(names(&read(&here)), ["devbox"]);
    }

    #[test]
    fn an_old_file_that_is_not_an_array_stays_byte_for_byte() {
        let dir = TempDir::new();
        let here = here_in(&dir);
        std::fs::write(&here.legacy, r#"[{"destination":"devbox"},"#).expect("written");

        assert!(migrate(&here).is_err());

        assert_eq!(text(&here.legacy), r#"[{"destination":"devbox"},"#);
        assert!(!here.config.exists());
    }

    #[test]
    fn without_the_old_file_nothing_is_written() {
        let dir = TempDir::new();

        migrate(&here_in(&dir)).expect("fine");

        assert!(!here_in(&dir).config.exists());
    }
}
