use super::*;
use crate::app::machine_links::checks::{AgentThere, Checked, Wanted};

const EVERY: Wanted = Wanted { accounts: true, sources: true, flows: true };

const ACCOUNTS: Wanted = Wanted { accounts: true, sources: false, flows: false };

const PROJECT: u64 = 1;

fn there(command: &str) -> Value {
    match command {
        "account-list" => json!({
            "providers": [{ "provider": "claude", "accounts": [{ "name": "work" }], "active": "work",
                            "system_login": { "state": "signed-out" } }],
            "agents": [{ "agent": "claude", "command": "claude", "installed": true }],
        }),
        "issue-sources" => json!({
            "project": PROJECT,
            "sources": [{ "source": "linear", "name": "Linear", "state": "missing", "missing": ["token"] }],
        }),
        "flow-list" => {
            json!({ "flows": [{ "name": "Pair", "origin": "user", "flow": { "name": "Pair" }, "default": true }] })
        }
        _ => json!({}),
    }
}

fn read(app: &App) -> bool {
    app.accounts_there("vps") != Checked::Waiting
        && app.tracker_there("vps", PROJECT, "linear") != Checked::Waiting
        && app.flows_there("vps", PROJECT) != Checked::Waiting
}

impl Remote {
    fn checks(&mut self) {
        self.rig.app.check_machine("vps", Some(PROJECT), EVERY);
        let (app, rx) = (&mut self.rig.app, &self.rig.rx);
        wait_until("the machine answers the checks", || {
            drain(app, rx);
            read(app)
        });
    }
}

mod the_checks {
    use super::*;

    #[test]
    fn ask_the_machine_its_accounts_trackers_and_flows_with_its_boot() {
        let mut remote = seeing(Reply::Each(there));

        remote.checks();

        let mut asked: Vec<(String, Value)> = remote
            .asked()
            .into_iter()
            .inspect(|asked| assert_eq!(asked["boot"], json!(BOOT)))
            .map(|asked| (asked["command"].as_str().unwrap_or_default().to_string(), asked["args"].clone()))
            .collect();
        asked.sort_by(|a, b| a.0.cmp(&b.0));
        let project = json!({ "project": PROJECT });
        assert_eq!(
            asked,
            [
                ("account-list".to_string(), json!({ "check": true })),
                ("flow-list".to_string(), project.clone()),
                ("issue-sources".to_string(), project),
            ]
        );
    }

    #[test]
    fn read_as_the_dialogs_show_them() {
        let mut remote = seeing(Reply::Each(there));

        remote.checks();

        let app = &remote.rig.app;
        let Checked::Read(Some(accounts)) = app.accounts_there("vps") else { panic!("no accounts") };
        assert_eq!(accounts.pick("claude"), Some("work"));
        let Checked::Read(for_flows) = app.flow_accounts_there("vps") else { panic!("no accounts for flows") };
        let kinds: Vec<&str> = for_flows.iter().map(|agent| agent.kind.as_str()).collect();
        assert_eq!(kinds, ["claude"]);
        assert_eq!(
            (app.agent_there("vps", "claude", ""), app.agent_there("vps", "claude", "work")),
            (AgentThere::SignedOut, AgentThere::Ready)
        );
        assert_eq!(
            app.tracker_there("vps", PROJECT, "linear"),
            Checked::Read(Some("vps has no Linear token for api".into()))
        );
        let Checked::Read(flows) = app.flows_there("vps", PROJECT) else { panic!("no flows") };
        assert_eq!(flows.default.as_deref(), Some("Pair"));
    }

    #[test]
    fn are_asked_once_while_they_are_kept() {
        let mut remote = seeing(Reply::Each(there));

        remote.checks();
        remote.checks();

        assert_eq!(remote.rig.seen("ask"), 3);
    }

    #[test]
    fn are_asked_again_when_the_dialog_opens_again() {
        let mut remote = seeing(Reply::Each(there));
        remote.checks();

        remote.rig.app.recheck_machine("vps", Some(PROJECT));
        remote.rig.app.check_machine("vps", Some(PROJECT), ACCOUNTS);
        let (app, rx) = (&mut remote.rig.app, &remote.rig.rx);
        wait_until("the machine answers again", || {
            drain(app, rx);
            app.accounts_there("vps") != Checked::Waiting
        });

        assert_eq!(remote.rig.seen("ask"), 4);
    }

    #[test]
    fn drop_an_answer_to_the_question_before_they_were_asked_again() {
        let mut remote = seeing(Reply::Each(there));
        remote.rig.app.check_machine("vps", None, ACCOUNTS);
        let earlier = remote.held_relay();

        remote.rig.app.recheck_machine("vps", None);
        remote.rig.app.check_machine("vps", None, ACCOUNTS);
        remote.rig.app.handle_event(earlier, AREA).expect("handle event");

        assert_eq!(remote.rig.app.accounts_there("vps"), Checked::Waiting);
        remote.rig.until("the machine answers again", |app| app.accounts_there("vps") != Checked::Waiting);
        assert_eq!(remote.rig.seen("ask"), 2);
    }

    #[test]
    fn are_forgotten_when_the_machine_restarts() {
        let mut remote = seeing(Reply::Each(there));
        remote.checks();

        remote.restarted();

        assert!(!read(&remote.rig.app));
        assert_eq!(remote.rig.app.agent_there("vps", "claude", ""), AgentThere::Unknown);
        remote.checks();
        assert_eq!(remote.rig.seen("ask"), 6);
    }

    #[test]
    fn fail_without_the_destination() {
        let mut remote = seeing(Reply::Down);

        remote.checks();

        let Checked::Failed(why) = remote.rig.app.accounts_there("vps") else { panic!("no failure") };
        assert!(why.contains("vps") && !why.contains("vps.lan"), "{why}");
    }

    #[test]
    fn fail_at_once_on_a_machine_that_is_not_linked() {
        let mut remote = seeing(Reply::Each(there));
        remote.falls();

        remote.rig.app.check_machine("vps", Some(PROJECT), EVERY);

        assert_eq!(remote.rig.app.flows_there("vps", PROJECT), Checked::Failed("vps unreachable".into()));
        assert_eq!(remote.rig.seen("ask"), 0);
    }
}
