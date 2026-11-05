use clap::Parser;
use rstest::rstest;

use super::*;
use crate::cli::Cli;
use crate::cli::args::Command;
use crate::machines::State;
use crate::time::Stamp;

const READ_AT: &str = "2026-10-04T11:40:00.000Z";

fn now() -> SystemTime {
    Stamp::parse("2026-10-04T11:46:00.000Z").expect("a time")
}

fn read(name: &str, read_at: Option<&str>, error: Option<&str>) -> MachineRead {
    MachineRead {
        name: name.into(),
        state: if read_at.is_some() { State::Linked } else { State::Unreachable },
        read_at: read_at.map(str::to_string),
        error: error.map(str::to_string),
        ..MachineRead::default()
    }
}

fn view() -> View {
    View { since: "2026-10-01T00:00:00.000Z".into(), ..View::default() }
}

mod the_machine_flag {
    use super::*;

    #[rstest]
    #[case::a_name(&["stats", "--machine", "vps"], Some("vps"))]
    #[case::every_machine_as_json(&["stats", "--machine", "all", "--json"], Some("all"))]
    #[case::none(&["stats", "--share"], None)]
    fn is_read_as_given(#[case] args: &[&str], #[case] machine: Option<&str>) {
        let cli = Cli::try_parse_from(std::iter::once("cornercase").chain(args.iter().copied())).expect("parse");
        match cli.command {
            Some(Command::Stats(args)) => assert_eq!(args.machine.as_deref(), machine),
            other => panic!("not stats: {other:?}"),
        }
    }

    #[test]
    fn needs_a_name() {
        assert!(Cli::try_parse_from(["cornercase", "stats", "--machine", ""]).is_err());
    }
}

mod the_text_of_machines {
    use super::*;

    #[test]
    fn heads_one_machine_with_when_it_was_read() {
        let stats =
            MachineStats { view: view(), machine: Some(read("vps", Some(READ_AT), None)), ..MachineStats::default() };

        assert_eq!(machine_text(&stats, false, now()), format!("● vps  read 6 min ago\n\n{}", view().text()));
    }

    #[test]
    fn lists_what_was_summed_after_the_sum() {
        let machines = vec![read("vps", Some(READ_AT), None), read("pi-4", None, Some("that is this computer"))];
        let stats = MachineStats { view: view(), machines, ..MachineStats::default() };

        assert_eq!(
            machine_text(&stats, false, now()),
            format!(
                "{}\n\nmost at once: the largest peak of one machine, not a sum\nsummed from:\n▭ this computer\n● vps   read 6 min ago\n○ pi-4  that is this computer · not summed",
                view().text()
            )
        );
    }

    #[test]
    fn of_a_machine_never_read_says_so() {
        let stats = MachineStats { view: view(), machines: vec![read("vps", None, None)], ..MachineStats::default() };

        assert!(machine_text(&stats, false, now()).ends_with("○ vps  not read yet · not summed"));
    }

    #[test]
    fn is_only_the_link_when_shared() {
        let stats =
            MachineStats { view: view(), machine: Some(read("vps", Some(READ_AT), None)), ..MachineStats::default() };

        assert_eq!(machine_text(&stats, true, now()), view().share_link());
    }
}
