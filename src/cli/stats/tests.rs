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

fn parsed(args: &[&str]) -> StatsArgs {
    let cli = Cli::try_parse_from(std::iter::once("cornercase").chain(args.iter().copied())).expect("parse");
    match cli.command {
        Some(Command::Stats(args)) => args,
        other => panic!("not stats: {other:?}"),
    }
}

mod the_machine_flag {
    use super::*;

    #[rstest]
    #[case::a_name(&["stats", "--machine", "vps"], Some("vps"))]
    #[case::every_machine_as_json(&["stats", "--machine", "all", "--json"], Some("all"))]
    #[case::none(&["stats", "--share"], None)]
    fn is_read_as_given(#[case] args: &[&str], #[case] machine: Option<&str>) {
        assert_eq!(parsed(args).machine.as_deref(), machine);
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

mod the_cached_flag {
    use std::thread;

    use serde_json::{Value, json};

    use super::*;
    use crate::cli::line;
    use crate::client;
    use crate::control::Response;
    use crate::control::contract::describe;
    use crate::machines::link::fake::{ended, request, respond};
    use crate::os::socket::UnixStream;

    const LACKS: &str = "cannot read a machine's last stats without asking it";

    fn contract_without(feature: &str) -> Value {
        let mut descriptor = describe("boot", &[]);
        descriptor.features.retain(|offered| offered != feature);
        serde_json::to_value(descriptor).expect("json")
    }

    fn stats_answer() -> Value {
        serde_json::to_value(view()).expect("json")
    }

    fn served(args: &[&str], replies: Vec<Value>) -> (Result<bool>, Vec<Value>) {
        let args = parsed(args);
        let (ours, mut theirs) = UnixStream::pair().expect("a socket pair");
        let server = thread::spawn(move || {
            let mut asked = Vec::new();
            for reply in replies {
                asked.push(request(&mut theirs));
                respond(&mut theirs, &Response::Ok(reply));
            }
            assert!(ended(&mut theirs), "more was asked than the test expects");
            asked
        });
        client::share_socket(ours);
        let (done, _) = line::run(None, || run(&args));
        client::unshare();
        (done, server.join().expect("the fake server"))
    }

    fn sent(asked: &Value) -> (&Value, &Value) {
        (&asked["command"], &asked["args"])
    }

    #[test]
    fn is_read_with_or_without_a_machine() {
        assert!(parsed(&["stats", "--cached"]).cached);
        assert!(parsed(&["stats", "--cached", "--machine", "all", "--json"]).cached);
        assert!(parsed(&["stats", "--cached", "--machine", "vps", "--share"]).cached);
        assert!(!parsed(&["stats", "--machine", "vps"]).cached);
    }

    #[test]
    fn goes_with_the_machine_to_a_server_that_offers_it() {
        let replies = vec![contract_without("none"), stats_answer()];

        let (done, asked) = served(&["stats", "--cached", "--machine", "all", "--json"], replies);

        assert!(done.expect("an answer"));
        assert_eq!(sent(&asked[1]), (&json!("stats"), &json!({"cached": true, "machine": "all"})));
    }

    #[test]
    fn is_refused_before_a_server_without_it_can_wait_for_the_machine() {
        let (done, asked) = served(&["stats", "--cached", "--machine", "vps"], vec![contract_without("stats-cached")]);

        let refused = done.expect_err("refused").to_string();
        assert!(refused.contains(LACKS), "{refused}");
        assert_eq!(asked.iter().map(|asked| &asked["command"]).collect::<Vec<_>>(), [&json!("contract")]);
    }

    #[test]
    fn is_not_needed_of_a_server_to_ask_a_machine_afresh() {
        let replies = vec![contract_without("stats-cached"), stats_answer()];

        let (done, asked) = served(&["stats", "--machine", "vps", "--json"], replies);

        assert!(done.expect("an answer"));
        assert_eq!(sent(&asked[1]), (&json!("stats"), &json!({"machine": "vps"})));
    }

    #[test]
    fn changes_nothing_without_a_machine_and_needs_no_feature() {
        let (done, asked) = served(&["stats", "--cached"], vec![stats_answer()]);

        assert!(done.expect("an answer"));
        assert_eq!(sent(&asked[0]), (&json!("stats"), &json!({})));
    }
}
