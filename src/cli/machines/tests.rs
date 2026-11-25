use std::time::{Duration, UNIX_EPOCH};

use clap::Parser;
use rstest::rstest;

use super::*;
use crate::cli::Cli;
use crate::cli::args::{self, Control};
use crate::control::machines::ThisComputer;
use crate::machines::{Info, State};

fn sent(args: &[&str]) -> (Machines, bool) {
    let cli = Cli::try_parse_from(std::iter::once("cornercase").chain(args.iter().copied())).expect("parse");
    match cli.command {
        Some(args::Command::Control(Control::Machines(args))) => {
            let (request, print) = request(&args);
            (request, print.json)
        }
        other => panic!("not machines: {other:?}"),
    }
}

mod the_arguments {
    use super::*;

    #[test]
    fn list_by_default() {
        assert_eq!(sent(&["machines", "--json"]), (Machines::List, true));
    }

    #[test]
    fn add_only_a_new_machine() {
        let set = MachineSet {
            name: "vps".into(),
            icon: Some("laptop".into()),
            destination: Some("me@vps.lan".into()),
            create: Some(true),
            ..MachineSet::default()
        };
        assert_eq!(sent(&["machines", "add", "vps", "me@vps.lan", "--icon", "laptop"]), (Machines::Set(set), false));
    }

    #[test]
    fn add_takes_a_command_and_can_leave_the_machine_not_shown() {
        let set = MachineSet {
            name: "vps".into(),
            destination: Some("me@vps.lan".into()),
            command: Some("~/bin/cc".into()),
            shown: Some(false),
            create: Some(true),
            ..MachineSet::default()
        };

        let line = ["machines", "add", "vps", "me@vps.lan", "--command", "~/bin/cc", "--hidden"];

        assert_eq!(sent(&line), (Machines::Set(set), false));
    }

    #[test]
    fn edit_takes_a_command_and_an_empty_one_clears_it() {
        let set = MachineSet {
            name: "vps".into(),
            command: Some(String::new()),
            create: Some(false),
            ..MachineSet::default()
        };

        assert_eq!(sent(&["machines", "edit", "vps", "--command", ""]), (Machines::Set(set), false));
    }

    #[rstest]
    #[case::a_command(&["machines", "add", "vps", "x.lan", "--command", "cc"], true)]
    #[case::hidden(&["machines", "add", "vps", "x.lan", "--hidden"], true)]
    #[case::an_edited_command(&["machines", "edit", "vps", "--command", ""], true)]
    #[case::neither(&["machines", "add", "vps", "x.lan"], false)]
    #[case::a_list(&["machines"], false)]
    fn only_what_an_older_server_would_drop_needs_the_feature(#[case] line: &[&str], #[case] needs: bool) {
        assert_eq!(needs_shown(&sent(line).0), needs);
    }

    #[rstest]
    #[case::hide_one(&["machines", "hide", "vps"], Machines::Hide(Some("vps".into())))]
    #[case::hide_all(&["machines", "hide", "all"], Machines::Hide(None))]
    #[case::hide_all_in_capitals(&["machines", "hide", "ALL"], Machines::Hide(None))]
    #[case::show_one(&["machines", "show", "vps"], Machines::Show(Some("vps".into())))]
    #[case::show_all(&["machines", "show", "all"], Machines::Show(None))]
    fn hide_and_show_take_a_name_or_all(#[case] line: &[&str], #[case] request: Machines) {
        assert_eq!(sent(line).0, request);
    }

    #[rstest]
    #[case::hide(&["cornercase", "machines", "hide"])]
    #[case::show(&["cornercase", "machines", "show"])]
    fn hide_and_show_need_a_target(#[case] line: &[&str]) {
        assert!(Cli::try_parse_from(line.iter().copied()).is_err());
    }

    #[rstest]
    #[case::hide(&["machines", "hide", "vps"], true)]
    #[case::show(&["machines", "show", "all"], true)]
    #[case::retry(&["machines", "retry"], false)]
    #[case::hidden_add(&["machines", "add", "vps", "x.lan", "--hidden"], false)]
    #[case::a_list(&["machines"], false)]
    fn only_hide_and_show_need_machine_hide(#[case] line: &[&str], #[case] needs: bool) {
        assert_eq!(needs_hide(&sent(line).0), needs);
    }

    #[test]
    fn edit_only_an_existing_machine_and_what_was_given() {
        let set = MachineSet {
            name: "vps".into(),
            new_name: Some("box".into()),
            folder: Some("/srv".into()),
            create: Some(false),
            ..MachineSet::default()
        };
        assert_eq!(
            sent(&["machines", "edit", "vps", "--name", "box", "--folder", "/srv", "--json"]),
            (Machines::Set(set), true)
        );
    }

    #[rstest]
    #[case::remove(&["machines", "remove", "vps"], Machines::Remove("vps".into()))]
    #[case::update(&["machines", "update", "vps"], Machines::Update("vps".into()))]
    #[case::retry_one(&["machines", "retry", "vps"], Machines::Retry(Some("vps".into())))]
    #[case::retry_every_machine(&["machines", "retry"], Machines::Retry(None))]
    fn name_the_machine(#[case] args: &[&str], #[case] request: Machines) {
        assert_eq!(sent(args).0, request);
    }

    #[test]
    fn add_needs_a_destination() {
        assert!(Cli::try_parse_from(["cornercase", "machines", "add", "vps"]).is_err());
    }
}

mod the_text {
    use super::*;

    #[test]
    fn lists_this_computer_then_each_machine_with_its_state_and_why() {
        let list = MachineList {
            here: ThisComputer { icon: "▭".into(), version: "0.12.225".into(), memory: None },
            machines: vec![
                Info {
                    name: "vps".into(),
                    icon: "☁".into(),
                    destination: "miguel@vps.example.com".into(),
                    folder: "~/code".into(),
                    state: State::Linked,
                    version: Some("0.12.225".into()),
                    server_version: Some("0.12.224".into()),
                    shown: true,
                    ..Info::default()
                },
                Info {
                    name: "pi".into(),
                    icon: "◫".into(),
                    destination: "pi.lan".into(),
                    folder: "/srv".into(),
                    state: State::Unreachable,
                    seen: Some("2026-10-29T10:00:00.000Z".into()),
                    error: Some("ssh: connect to host pi.lan port 22: Connection refused".into()),
                    shown: true,
                    ..Info::default()
                },
            ],
        };
        let now = UNIX_EPOCH + Duration::from_secs(1_793_268_000 + 8);

        assert_eq!(
            list_text(&list, now),
            "▭ this computer  0.12.225\n\
             ● ☁ vps  miguel@vps.example.com  ~/code  linked · 0.12.225 · its server runs 0.12.224 until it restarts there\n\
             ○ ◫ pi   pi.lan                  /srv  unreachable · 8 s\n    \
             ssh: connect to host pi.lan port 22: Connection refused\n"
        );
    }
}
