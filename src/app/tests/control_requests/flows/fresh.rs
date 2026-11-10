use super::*;
use crate::test_util::{clone_of, commit, git as git_in, head};

struct Origin {
    _dir: TempDir,
    behind: String,
    ahead: String,
}

fn origin_ahead_of(team: &Team) -> Origin {
    let (dir, origin) = clone_of(team.repo.path());
    git_in(team.repo.path(), &["remote", "add", "origin", &origin.display().to_string()]);
    let behind = head(team.repo.path());
    let ahead = commit(&origin, "news", "x");
    Origin { _dir: dir, behind, ahead }
}

fn started_with(team: &mut Team, fresh: Option<bool>) -> String {
    let run = wire::FlowRun { name: "Pair".into(), prompt: "Fix the login form".into(), fresh, ..Default::default() };
    ask(&mut team.app, None, Command::FlowRun(run));
    let info: RunInfo = match answered(&mut team.app, &team.rx, "the flow's tabs are open") {
        Response::Ok(value) => serde_json::from_value(value).expect("the run"),
        Response::Error(message) => panic!("the flow did not start: {message}"),
    };
    let workspace = team.app.projects[0].workspaces.iter().find(|w| w.id == info.workspace).expect("the workspace");
    head(&workspace.path)
}

#[test]
fn a_flow_run_with_fresh_branches_from_the_newest_commit_of_origin() {
    let mut team = Team::new();
    let origin = origin_ahead_of(&team);

    let started = started_with(&mut team, Some(true));

    assert_eq!(started, origin.ahead);
    assert_eq!(head(team.repo.path()), origin.behind);
}

#[test]
fn a_flow_run_without_the_field_branches_from_head() {
    let mut team = Team::new();
    let origin = origin_ahead_of(&team);

    assert_eq!(started_with(&mut team, None), origin.behind);
}

#[test]
fn a_flow_run_follows_the_setting() {
    let mut team = Team::configured(FAKE, |config, _| config.fresh_branches = true);
    let origin = origin_ahead_of(&team);

    assert_eq!(started_with(&mut team, None), origin.ahead);
}

#[test]
fn no_fresh_overrides_the_setting_for_a_flow_run() {
    let mut team = Team::configured(FAKE, |config, _| config.fresh_branches = true);
    let origin = origin_ahead_of(&team);

    assert_eq!(started_with(&mut team, Some(false)), origin.behind);
}
