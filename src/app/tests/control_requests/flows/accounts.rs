use std::collections::BTreeMap;
use std::path::PathBuf;

use super::*;
use crate::accounts;
use crate::agents::Known;

const ACCOUNTED: &str = r#"#!/bin/sh
log="LOG"
printf 'account %s\n' "${CLAUDE_CONFIG_DIR:-system}" >> "$log/order"
printf 'agent ready> '
stty -icanon 2>/dev/null
IFS= read -r line
printf 'got %s\n' "$line" >> "$log/order"
while [ -d "$log" ]; do sleep 0.1; done
"#;

fn team() -> (Team, PathBuf) {
    let mut team = Team::configured(ACCOUNTED, |config, _| {
        let chosen = BTreeMap::from([("claude".to_string(), "work".to_string())]);
        config.profiles.insert("Work".into(), chosen);
        config.profiles.insert("Home".into(), BTreeMap::new());
    });
    let work = accounts::add(&team.config.path().join("accounts"), Known::Claude, "work", None).expect("the account");
    team.app.set_accounts_dir(&team.config.path().join("accounts"));
    (team, work)
}

fn run_with(team: &mut Team, profile: &str) -> Response {
    let run = wire::FlowRun {
        name: "Pair".into(),
        prompt: "Fix the login form".into(),
        profile: Some(profile.into()),
        ..wire::FlowRun::default()
    };
    ask(&mut team.app, None, Command::FlowRun(run));
    answered(&mut team.app, &team.rx, "the flow's tabs are open")
}

#[test]
fn every_role_of_a_run_gets_the_accounts_of_its_profile() {
    let (mut team, work) = team();
    assert_eq!(team.app.config.profile, "");

    let run: RunInfo = match run_with(&mut team, "Work") {
        Response::Ok(value) => serde_json::from_value(value).expect("the run"),
        Response::Error(message) => panic!("the flow did not start: {message}"),
    };

    let log = team.log.path().join("order");
    let accounted = |log: &Path| {
        std::fs::read_to_string(log).unwrap_or_default().lines().filter(|l| l.starts_with("account ")).count() >= 3
    };
    until_logged(&mut team.app, &team.rx, "every role says its account", || accounted(&log));
    let lines: Vec<String> = team.order().into_iter().filter(|line| line.starts_with("account ")).collect();
    assert_eq!(lines, vec![format!("account {}", work.display()); 3]);
    for member in &run.members {
        assert_eq!(pane(&team.app, member.pane).accounts().name(Known::Claude), Some("work"), "{}", member.role);
    }
    assert_eq!(team.app.config.profile, "", "the run leaves the active profile alone");

    ok(&mut team.app, "profile-use", &json!({"name": "Home"}));

    for member in &run.members {
        assert_eq!(pane(&team.app, member.pane).accounts().name(Known::Claude), Some("work"), "{}", member.role);
    }
}

#[test]
fn a_run_with_an_unknown_profile_opens_nothing() {
    let (mut team, _) = team();
    let tabs = team.app.projects[0].workspaces.iter().map(|w| w.tabs.len()).sum::<usize>();

    let refused = refused(&mut team.app, "flow-run", &json!({"name": "Pair", "prompt": "Fix it", "profile": "Gone"}));

    assert_eq!(refused.code, "not-found", "{}", refused.message);
    assert_eq!(team.app.projects[0].workspaces.iter().map(|w| w.tabs.len()).sum::<usize>(), tabs);
}
