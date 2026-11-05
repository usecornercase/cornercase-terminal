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
            here: ThisComputer { icon: "▭".into(), version: "0.12.225".into() },
            machines: vec![
                Info {
                    name: "vps".into(),
                    icon: "☁".into(),
                    destination: "miguel@vps.example.com".into(),
                    folder: "~/code".into(),
                    state: State::Linked,
                    version: Some("0.12.225".into()),
                    server_version: Some("0.12.224".into()),
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
