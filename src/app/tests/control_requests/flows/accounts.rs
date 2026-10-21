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
    run_choosing(team, Some(profile), BTreeMap::new())
}

fn run_choosing(team: &mut Team, profile: Option<&str>, accounts: BTreeMap<String, String>) -> Response {
    run_by(team, None, profile, accounts)
}

fn run_by(team: &mut Team, caller: Option<u64>, profile: Option<&str>, accounts: BTreeMap<String, String>) -> Response {
    let run = wire::FlowRun {
        name: "Pair".into(),
        prompt: "Fix the login form".into(),
        profile: profile.map(Into::into),
        accounts,
        ..wire::FlowRun::default()
    };
    ask(&mut team.app, caller, Command::FlowRun(run));
    answered(&mut team.app, &team.rx, "the flow's tabs are open")
}

fn every_role_on(team: &mut Team, response: Response, home: &Path) -> RunInfo {
    let run: RunInfo = match response {
        Response::Ok(value) => serde_json::from_value(value).expect("the run"),
        Response::Error(message) => panic!("the flow did not start: {message}"),
    };
    let log = team.log.path().join("order");
    let accounted = |log: &Path| {
        std::fs::read_to_string(log).unwrap_or_default().lines().filter(|l| l.starts_with("account ")).count() >= 3
    };
    until_logged(&mut team.app, &team.rx, "every role says its account", || accounted(&log));
    let lines: Vec<String> = team.order().into_iter().filter(|line| line.starts_with("account ")).collect();
    assert_eq!(lines, vec![format!("account {}", home.display()); 3]);
    let name = home.file_name().and_then(|name| name.to_str());
    for member in &run.members {
        assert_eq!(pane(&team.app, member.pane).accounts().name(Known::Claude), name, "{}", member.role);
    }
    run
}

#[test]
fn every_role_of_a_run_gets_the_accounts_of_its_profile() {
    let (mut team, work) = team();
    assert_eq!(team.app.config.profile, "");

    let response = run_with(&mut team, "Work");
    let run = every_role_on(&mut team, response, &work);
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

#[test]
fn an_account_named_for_a_run_reaches_every_role_over_the_profile() {
    let (mut team, _) = team();
    let home = accounts::add(&team.config.path().join("accounts"), Known::Claude, "home", None).expect("the account");
    let chosen = BTreeMap::from([("claude".to_string(), "home".to_string())]);

    let response = run_choosing(&mut team, Some("Work"), chosen);
    every_role_on(&mut team, response, &home);
}

#[test]
fn a_run_started_from_a_pane_takes_the_selection_not_the_pane_s_accounts() {
    let (mut team, work) = team();
    accounts::add(&team.config.path().join("accounts"), Known::Claude, "home", None).expect("the account");
    let opened = ok(&mut team.app, "new-tab", &json!({"accounts": {"claude": "home"}}));
    let caller = serde_json::from_value::<Done>(opened).expect("done").ids.pane;
    ok(&mut team.app, "account-use", &json!({"provider": "claude", "account": "work"}));

    let response = run_by(&mut team, caller, None, BTreeMap::new());

    every_role_on(&mut team, response, &work);
}

#[test]
fn a_run_with_an_unknown_account_opens_nothing() {
    let (mut team, _) = team();
    let tabs = team.app.projects[0].workspaces.iter().map(|w| w.tabs.len()).sum::<usize>();
    let args = json!({"name": "Pair", "prompt": "Fix it", "accounts": {"claude": "gone"}});

    let refused = refused(&mut team.app, "flow-run", &args);

    assert_eq!(refused.code, "not-found", "{}", refused.message);
    assert_eq!(team.app.projects[0].workspaces.iter().map(|w| w.tabs.len()).sum::<usize>(), tabs);
}
