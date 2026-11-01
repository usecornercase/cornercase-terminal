use std::time::SystemTime;

use serde_json::{Value, json};

use super::*;
use crate::agents::Known;
use crate::agents::stats::Counted;
use crate::app::machine_usage::{CONNECTING, NOT_READ};
use crate::app::machines::OLDER;
use crate::app::tests::usage_modal::ANSWER;
use crate::machines::Status;
use crate::test_util::write_executable;

const CLIENT: u64 = 7;
const LATER: Duration = Duration::from_secs(5);
const EMAIL: &str = "ana@example.com";
const REFUSED: &str = "ssh: connect to host vps.lan port 22: Connection refused";
const HIDDEN: &str = "ssh: connect to host vps port 22: Connection refused";

type Replied = std::result::Result<Value, (String, String)>;

impl Rig {
    fn asks(&mut self, command: &str, args: &Value) -> Replied {
        self.sends(command, args);
        self.reply()
    }

    fn sends(&mut self, command: &str, args: &Value) {
        let text = json!({"contract": 1, "command": command, "args": args}).to_string();
        self.app.request(CLIENT, &text, Some(AREA), Instant::now());
    }

    fn reply(&mut self) -> Replied {
        let (at, clock) = (self.start + LATER, CLOCK + 5);
        let (app, rx) = (&mut self.app, &self.rx);
        let mut answer = None;
        wait_until("the request is answered", || {
            drain(app, rx);
            app.refresh_at(at, clock);
            answer = app.take_answers().pop().map(|(_, text)| text);
            answer.is_some()
        });
        replied(&answer.expect("an answer"))
    }

    fn down(&mut self, down: Down) {
        let link = self.links()[0];
        self.app.machine_event(MachineEvent::Link(Report { link, event: Event::Down { down, failed: 1 } }));
    }

    fn with_claude(&mut self, script: &str) {
        let path = self.dir.path().join("claude");
        write_executable(&path, &format!("#!/bin/sh\n{script}\n"));
        let commands = &mut self.app.config.agent_commands;
        commands.insert(Known::Claude.name().into(), path.display().to_string());
        commands.insert(Known::Codex.name().into(), "/nonexistent/codex".into());
    }
}

fn prompts(answer: &Value) -> &Value {
    &answer["windows"]["total"]["prompts"]
}

fn plans_of(identity: &str) -> Plans {
    let agent = PlanInfo { agent: "claude".into(), identity: Some(identity.into()), ..PlanInfo::default() };
    Plans { agents: vec![agent], ..Plans::default() }
}

mod one_machine {
    use super::*;

    #[test]
    fn is_asked_only_when_the_request_names_it() {
        let mut rig = linked(answer(&counted()));

        let here = rig.asks("stats", &json!({})).expect("this computer's stats");

        assert_eq!(prompts(&here), 0);
        assert!(here.get("machine").is_none() && here.get("machines").is_none(), "{here}");
        assert_eq!(rig.seen("ask"), 0);
    }

    #[test]
    fn that_is_linked_answers_its_stats_and_names_itself() {
        let mut rig = linked(answer(&counted()));

        let there = rig.asks("stats", &json!({"machine": "VPS"})).expect("the machine's stats");

        assert_eq!(prompts(&there), 5);
        let machine = &there["machine"];
        assert_eq!(
            (&machine["name"], &machine["state"], &machine["error"]),
            (&json!("vps"), &json!("linked"), &json!(null))
        );
        assert!(machine["read_at"].is_string() && machine["seen"].is_string(), "{machine}");
        assert_eq!(rig.seen("asked stats"), 1);
    }

    #[test]
    fn that_is_down_answers_its_last_numbers_and_why_without_its_destination() {
        let mut rig = linked(answer(&counted()));
        assert_eq!(rig.stats_read(), Some(5));
        rig.down(Down::Unreachable(REFUSED.into()));

        let there = rig.asks("stats", &json!({"machine": "vps"})).expect("the cached stats");

        assert_eq!(prompts(&there), 5);
        assert_eq!((&there["machine"]["state"], &there["machine"]["error"]), (&json!("unreachable"), &json!(HIDDEN)));
        assert!(there["machine"]["read_at"].is_string());
        assert_eq!(rig.seen("ask"), 1);
    }

    #[rstest]
    #[case::stats("stats")]
    #[case::usage("usage")]
    fn that_is_down_without_numbers_fails_without_its_destination(#[case] command: &str) {
        let mut rig = Rig::new(vec![machine("vps", "vps.lan")], There::Missing);
        rig.until("the link reports", |app| app.machines.held()[0].status.state == State::Unreachable);
        let down = Some(Down::Unreachable(REFUSED.into()));
        rig.app.machines.set_status(0, Status { state: State::Unreachable, down, ..Status::default() });

        let (code, message) = rig.asks(command, &json!({"machine": "vps"})).expect_err("no numbers");

        assert_eq!(code, "failed");
        assert!(message.starts_with(&format!("no {command} from `vps`: ")), "{message}");
        assert!(!message.contains("vps.lan"), "{message}");
    }

    #[rstest]
    #[case::stats("stats")]
    #[case::usage("usage")]
    fn that_is_not_listed_is_not_found(#[case] command: &str) {
        let mut rig = linked(answer(&counted()));

        let refused = rig.asks(command, &json!({"machine": "pi"}));

        let message = "no machine is called `pi`; `cornercase machines` lists them";
        assert_eq!(refused, Err(("not-found".into(), message.into())));
    }

    #[test]
    fn removed_while_it_was_asked_fails() {
        let (mut rig, open) = gated(answer(&counted()));
        rig.sends("stats", &json!({"machine": "vps"}));
        assert_eq!(rig.app.take_answers(), Vec::<(u64, String)>::new());

        rig.edit(Vec::new(), Duration::from_secs(3));
        open.send(()).expect("let the machine answer");

        assert_eq!(rig.reply(), Err(("failed".into(), GONE.into())));
    }

    #[test]
    fn a_cached_usage_request_does_not_ask_it() {
        let mut rig = linked(answer(&plans_of("remote")));

        let refused = rig.asks("usage", &json!({"machine": "vps", "cached": true}));

        assert_eq!(refused, Err(("failed".into(), format!("no usage from `vps`: {NOT_READ}"))));
        assert_eq!(rig.seen("ask"), 0);
    }

    #[test]
    fn answers_its_usage_and_names_itself() {
        let mut rig = linked(answer(&plans_of("remote")));

        let there = rig.asks("usage", &json!({"machine": "vps"})).expect("the machine's usage");

        assert_eq!(there["agents"][0]["identity"], "remote");
        assert_eq!(there["machine"]["name"], "vps");
        assert!(there.get("machines").is_none(), "{there}");
    }
}

mod every_machine {
    use super::*;

    #[test]
    fn sums_the_stats_of_this_computer_and_each_machine_and_lists_them() {
        let mut rig = linked(answer(&counted()));
        let now = SystemTime::now();
        rig.app.agents.stats.counted(Known::Claude, Counted { prompts: vec![now, now], tokens: Vec::new() });

        let all = rig.asks("stats", &json!({"machine": "all"})).expect("the sum");

        assert_eq!(prompts(&all), 7);
        assert_eq!(all["windows"]["total"]["by_agent"]["claude"]["prompts"], 2);
        let machines = all["machines"].as_array().expect("the machines");
        assert_eq!((machines.len(), &machines[0]["name"]), (1, &json!("vps")));
        assert!(machines[0]["read_at"].is_string());
        assert!(all.get("machine").is_none());
    }

    #[test]
    fn counts_a_machine_listed_twice_once() {
        let mut rig = linked(answer(&counted()));
        rig.edit(vec![machine("vps", "vps.lan"), machine("pi", "vps.lan")], Duration::from_secs(3));

        let all = rig.asks("stats", &json!({"machine": "all"})).expect("the sum");

        assert_eq!(prompts(&all), 5);
        let machines = all["machines"].as_array().expect("the machines");
        assert_eq!((machines.len(), &machines[0]["name"]), (1, &json!("vps")));
    }

    #[rstest]
    #[case::still_connecting(Status::default(), CONNECTING)]
    #[case::older(Status { state: State::Older, down: Some(Down::Older(None)), ..Status::default() }, OLDER)]
    #[case::linked_but_never_asked(Status { state: State::Linked, ..Status::default() }, NOT_READ)]
    fn says_why_a_machine_never_read_has_no_numbers(#[case] status: Status, #[case] why: &str) {
        let mut rig = linked(answer(&plans_of("remote")));
        rig.app.machines.set_status(0, status);

        let all = rig.asks("usage", &json!({"machine": "all", "cached": true})).expect("every usage");

        let there = &all["machines"][0];
        assert_eq!((&there["error"], &there["read_at"], &there["agents"]), (&json!(why), &json!(null), &json!([])));
        assert_eq!(rig.seen("ask"), 0);
    }

    #[test]
    fn moves_the_heatmap_of_a_copy_read_days_ago_to_today() {
        let mut view = counted();
        view.blocks[0].agent_seconds = 60.0;
        let mut rig = linked(answer(&view));
        assert_eq!(rig.stats_read(), Some(5));
        rig.down(Down::Unreachable(REFUSED.into()));
        let held = rig.app.machines.named("vps").expect("the machine");
        let (_, read_at) = held.cache.stats.read.as_mut().expect("its copy");
        *read_at -= Duration::from_hours(72);

        let all = rig.asks("stats", &json!({"machine": "all"})).expect("the sum");

        assert_eq!(
            (&all["blocks"][0]["agent_seconds"], &all["blocks"][3]["agent_seconds"]),
            (&json!(0.0), &json!(60.0))
        );
        assert_eq!(prompts(&all), 5);
    }

    #[test]
    fn never_sums_a_machine_that_is_this_computer() {
        let mut rig = linked(answer(&counted()));
        assert_eq!(rig.stats_read(), Some(5));
        rig.down(Down::ThisComputer);

        let all = rig.asks("stats", &json!({"machine": "ALL"})).expect("the sum");

        assert_eq!(prompts(&all), 0);
        let machine = &all["machines"][0];
        assert_eq!((&machine["read_at"], &machine["error"]), (&json!(null), &json!("that is this computer")));
    }

    #[test]
    fn lists_the_usage_of_this_computer_then_each_machine_with_identities_but_no_email() {
        let mut rig = linked(answer(&plans_of("remote")));
        let account = json!({"email": EMAIL, "organization": "Acme", "apiProvider": "firstParty"});
        let response = json!({"subtype": "success", "request_id": "initialize", "response": {"account": account}});
        let initialize = json!({"type": "control_response", "response": response});
        rig.with_claude(&format!("read -r a\nread -r b\necho '{initialize}'\necho '{ANSWER}'"));

        let all = rig.asks("usage", &json!({"machine": "all"})).expect("every usage");

        let here = all["agents"].as_array().expect("this computer's agents");
        let claude = here.iter().find(|plan| plan["agent"] == "claude").expect("claude");
        assert_eq!(claude["plan"], "max");
        assert!(claude["identity"].as_str().is_some_and(|identity| identity.len() > 8), "{claude}");
        assert_eq!(
            (&all["machines"][0]["name"], &all["machines"][0]["agents"][0]["identity"]),
            (&json!("vps"), &json!("remote"))
        );
        let text = all.to_string();
        assert!(!text.contains(EMAIL) && !text.contains("Acme"), "{text}");
    }
}
