use super::*;
use crate::agents::AgentState;
use crate::config::ProjectSettings;
use crate::flows::Worktree;
use crate::git;
use crate::git::worktree::{checkout_path, git as run_git};
use crate::test_util::git as git_in;

const AT: &str = r#"#!/bin/sh
log="LOG"
printf 'start in %s\n' "$(pwd -P)" >> "$log/order"
printf 'Do you trust the files in this folder?\n❯ 1. Yes, proceed\n  2. No, exit\n'
read answer
printf '\033[2J\033[Hagent ready> '
stty -icanon 2>/dev/null
IFS= read -r line
printf 'got %s\n' "$line" >> "$log/order"
while [ -d "$log" ]; do sleep 0.1; done
"#;

fn duel() -> Flow {
    let apart = |name: &str| Role { worktree: Worktree::Own, ..role(name, None, true, "Compete.") };
    Flow {
        name: "Duel".into(),
        orchestrator: role(ORCHESTRATOR, None, false, "Be brief."),
        roles: vec![apart("coder A"), apart("coder B"), role("judge", None, false, "Judge.")],
        stages: vec![
            Stage { roles: vec!["coder A".into(), "coder B".into()], back: None },
            Stage { roles: vec!["judge".into()], back: None },
        ],
        ..Flow::default()
    }
}

fn team(setup: &str, teardown: &str) -> Team {
    let (setup, teardown) = (setup.to_string(), teardown.to_string());
    let team = Team::configured(AT, move |config, repo| {
        let settings = ProjectSettings { setup, teardown };
        config.projects.insert(repo.display().to_string(), settings);
    });
    let flows = paths::flows(&team.config.path().join("config.json"));
    store::save(&flows, &duel(), None).expect("save the duel");
    team
}

fn start(team: &mut Team, branch: Option<&str>) -> Response {
    team.run("Duel", branch)
}

fn started(team: &mut Team) -> RunInfo {
    match start(team, None) {
        Response::Ok(value) => serde_json::from_value(value).expect("the run"),
        Response::Error(message) => panic!("the flow did not start: {message}"),
    }
}

fn seat<'a>(run: &'a RunInfo, role: &str) -> &'a crate::control::MemberInfo {
    run.members.iter().find(|m| m.role == role).expect("the role")
}

fn tree_of(team: &Team, role: &str) -> PathBuf {
    let run = team.app.runs.first().expect("a run");
    let (p, w) = team.app.projects.workspace_index(run.workspace).expect("the workspace");
    let trees = &team.app.projects[p].workspaces[w].role_worktrees;
    trees.iter().find(|tree| tree.role == role).map(|tree| tree.path.clone()).expect("the role's worktree")
}

#[test]
fn an_own_role_starts_in_a_worktree_of_its_own_on_a_branch_made_from_the_runs() {
    let mut team = team("", "");
    let run = started(&mut team);

    let apart = seat(&run, "coder A");
    let path = apart.worktree.clone().expect("a worktree");
    assert_eq!(apart.branch.as_deref(), Some("fix-the-login-form--coder-a"));
    assert_eq!(git::branch(&path).as_deref(), Some("fix-the-login-form--coder-a"));
    assert!(seat(&run, "judge").worktree.is_none() && seat(&run, ORCHESTRATOR).branch.is_none());
    let (p, w) = team.app.projects.workspace_index(run.workspace).expect("the run's workspace");
    let project = &team.app.projects[p];
    assert_eq!(project.workspaces[w].role_worktrees.len(), 2);
    assert!(project.workspaces.iter().any(|ws| ws.path == path && ws.worktree), "the role has a row");
    let run_folder = project.workspaces[w].path.display().to_string();
    let order = team.until_got(4);
    let mut started: Vec<&String> = order.iter().filter(|l| l.starts_with("start in ")).collect();
    started.sort();
    let folders: Vec<String> = started.iter().map(|l| l.trim_start_matches("start in ").to_string()).collect();
    assert_eq!(folders.iter().filter(|f| **f == run_folder).count(), 2, "{folders:#?}");
    assert!(folders.contains(&path.display().to_string()), "{folders:#?}");
}

#[test]
fn the_runs_setup_runs_in_every_worktree_with_its_own_branch_before_the_agents_start() {
    let mut team = team("echo \"$CORNERCASE_BRANCH\" > .setup-ran", "");
    let run = started(&mut team);

    team.until_got(4);

    let ran = |path: &Path| std::fs::read_to_string(path.join(".setup-ran")).unwrap_or_default();
    assert_eq!(ran(&tree_of(&team, "coder A")), "fix-the-login-form--coder-a\n");
    assert_eq!(ran(&tree_of(&team, "coder B")), "fix-the-login-form--coder-b\n");
    let (p, w) = team.app.projects.workspace_index(run.workspace).expect("the workspace");
    assert_eq!(ran(&team.app.projects[p].workspaces[w].path), "fix-the-login-form\n");
}

#[test]
fn the_orchestrator_is_told_each_folder_and_branch() {
    let mut team = team("", "");
    started(&mut team);

    let order = team.until_got(4);

    let brief = &order[position(&order, "got You are the orchestrator")];
    let path = tree_of(&team, "coder A");
    assert!(brief.contains("coder A (pane "), "{brief}");
    assert!(brief.contains(&format!("in its own worktree {} on branch fix-the-login-form--coder-a", path.display())));
    assert!(brief.contains("## Worktrees"), "{brief}");
}

fn close_removing(team: &mut Team, workspace: u64, force: bool) -> Response {
    let close = wire::Close { item: Item::Workspace(workspace), remove_worktree: true, force };
    ask(&mut team.app, None, Command::Close(close));
    answered(&mut team.app, &team.rx, "the removal answers")
}

#[test]
fn removing_the_runs_workspace_removes_its_role_worktrees_runs_each_teardown_and_keeps_the_branches() {
    let marker = TempDir::new();
    let teardown = format!("echo \"$CORNERCASE_BRANCH\" >> '{}/gone'", marker.path().display());
    let mut team = team("", &teardown);
    let run = started(&mut team);
    let trees = [tree_of(&team, "coder A"), tree_of(&team, "coder B")];

    let answer = close_removing(&mut team, run.workspace, false);

    done(answer);
    assert!(trees.iter().all(|tree| !tree.exists()));
    let gone = std::fs::read_to_string(marker.path().join("gone")).expect("the teardowns ran");
    let mut branches: Vec<&str> = gone.lines().collect();
    branches.sort_unstable();
    assert_eq!(branches, ["fix-the-login-form", "fix-the-login-form--coder-a", "fix-the-login-form--coder-b"]);
    assert!(git::has_branch(team.repo.path(), "fix-the-login-form--coder-a"));
    refreshing(&mut team.app, &team.rx, "the rows go", |a| a.projects[0].workspaces.len() == 1);
}

#[test]
fn an_uncommitted_change_in_a_role_worktree_stops_the_removal_until_it_is_forced() {
    let mut team = team("", "");
    let run = started(&mut team);
    let tree = tree_of(&team, "coder A");
    std::fs::write(tree.join("wip"), "x").expect("write");

    let refused = error(close_removing(&mut team, run.workspace, false));

    assert!(refused.contains("has changes that are not committed"), "{refused}");
    assert!(tree.exists());
    done(close_removing(&mut team, run.workspace, true));
    assert!(!tree.exists());
}

#[test]
fn a_role_worktree_cannot_be_removed_while_its_run_lives() {
    let mut team = team("", "");
    let run = started(&mut team);
    let tree = tree_of(&team, "coder B");
    let row = team.app.projects[0].workspaces.iter().find(|w| w.path == tree).map(|w| w.id).expect("its row");
    assert_ne!(row, run.workspace);

    let refused = error(close_removing(&mut team, row, false));

    assert!(refused.contains("belongs to `coder B` in the flow running in"), "{refused}");
    assert!(tree.exists());
}

#[test]
fn a_run_comes_back_with_its_roles_in_their_worktrees_and_removal_still_finds_them() {
    let mut team = team("", "");
    let run = started(&mut team);
    team.until_got(4);
    let trees = [tree_of(&team, "coder A"), tree_of(&team, "coder B")];
    let saved = team.app.state();
    let (mut again, _rx) = new_app(team.config.path().join("config.json"));

    assert!(again.restore(&saved, AREA));

    let workspace =
        again.projects.iter().flat_map(|p| &p.workspaces).find(|w| w.id != 0 && !w.role_worktrees.is_empty());
    let workspace = workspace.expect("the run's workspace");
    let remembered: Vec<&PathBuf> = workspace.role_worktrees.iter().map(|tree| &tree.path).collect();
    assert_eq!(remembered, trees.iter().collect::<Vec<_>>());
    let cwds: Vec<Option<PathBuf>> = workspace.tabs.iter().filter_map(|t| t.pane()).map(Term::cwd).collect();
    assert!(trees.iter().all(|tree| cwds.contains(&Some(tree.clone()))), "{cwds:?}");
    assert!(run.workspace > 0);
}

#[test]
fn a_run_on_a_branch_that_exists_reuses_the_worktrees_its_roles_already_have() {
    let mut team = team("", "");
    git_in(team.repo.path(), &["branch", "feat"]);
    git_in(team.repo.path(), &["branch", "feat--coder-a"]);
    let existing = checkout_path(&team.worktrees.path().join("elsewhere"), team.repo.path(), "feat--coder-a");
    run_git(team.repo.path(), ["worktree", "add", existing.to_str().expect("utf8"), "feat--coder-a"])
        .expect("an earlier worktree");
    let existing = existing.canonicalize().expect("canonical");

    let response = start(&mut team, Some("feat"));

    let Response::Ok(value) = response else { panic!("did not start: {response:?}") };
    let run: RunInfo = serde_json::from_value(value).expect("the run");
    assert_eq!(seat(&run, "coder A").worktree.as_deref(), Some(existing.as_path()));
    let made = seat(&run, "coder B").worktree.clone().expect("a new one");
    assert_eq!(git::branch(&made).as_deref(), Some("feat--coder-b"));
}

#[test]
fn a_chosen_branch_that_is_new_while_a_role_branch_is_left_over_is_refused() {
    let mut team = team("", "");
    git_in(team.repo.path(), &["branch", "fresh--coder-a"]);

    let refused = error(start(&mut team, Some("fresh")));

    assert!(refused.contains("branch fresh--coder-a already exists"), "{refused}");
    assert!(team.app.runs.is_empty());
}

#[test]
fn a_default_branch_skips_one_whose_role_branches_are_left_over() {
    let mut team = team("", "");
    git_in(team.repo.path(), &["branch", "fix-it--coder-b"]);

    let branch = branch_of(team.run_task("Duel", "Fix it", None));

    assert!(suffixed(&branch, "fix-it"), "{branch}");
}

#[test]
fn a_role_worktree_that_cannot_be_made_opens_no_tab() {
    let mut team = team("", "");
    let blocked = checkout_path(team.worktrees.path(), team.repo.path(), "fix-the-login-form--coder-b");
    std::fs::create_dir_all(&blocked).expect("a folder in the way");

    let refused = error(start(&mut team, None));

    assert!(refused.contains("already exists") || refused.contains("exists"), "{refused}");
    assert!(team.app.runs.is_empty());
    let tabs: usize = team.app.projects[0].workspaces.iter().map(|w| w.tabs.len()).sum();
    assert_eq!(tabs, 1);
    let made = |branch: &str| checkout_path(team.worktrees.path(), team.repo.path(), branch);
    wait_until("the leftovers are cleaned up", || {
        !made("fix-the-login-form").exists() && !made("fix-the-login-form--coder-a").exists()
    });
    assert!(blocked.exists());
}

#[test]
fn a_locked_role_worktree_stops_the_removal_with_its_reason() {
    let mut team = team("", "");
    let run = started(&mut team);
    let tree = tree_of(&team, "coder A");
    run_git(team.repo.path(), ["worktree", "lock", "--reason", "busy", tree.to_str().expect("utf8")]).expect("lock");

    let refused = error(close_removing(&mut team, run.workspace, false));

    assert!(refused.contains("the worktree of `coder A`") && refused.contains("is locked (busy)"), "{refused}");
    assert!(refused.contains(&format!("git worktree unlock {}", tree.display())), "{refused}");
    assert!(!refused.contains(&run.workspace.to_string()) || refused.contains("in workspace"), "{refused}");
    assert!(tree.exists());
}

#[test]
fn a_role_whose_worktree_is_gone_on_restore_says_so_in_a_toast() {
    let mut team = team("", "");
    started(&mut team);
    team.until_got(4);
    let mut saved = team.app.state();
    let workspace = saved.projects[0].workspaces.iter_mut().find(|w| w.run.is_some()).expect("the run");
    let seat = workspace.run.as_ref().and_then(|run| run.members.iter().find(|s| s.role == "coder A")).expect("seat");
    let (tab, pane) = (seat.tab, seat.pane);
    workspace.tabs[tab].panes[pane].cwd = Some(PathBuf::from("/nonexistent/worktree"));
    workspace.tabs[tab].panes[pane].agent =
        Some(AgentState { kind: "claude".into(), conversation: "c1".into(), mode: None });
    let (mut again, _rx) = new_app(team.config.path().join("config.json"));

    again.restore(&saved, AREA);

    let toast = again.toast.as_ref().map(|toast| toast.message.clone()).unwrap_or_default();
    assert!(toast.contains("worktree of coder A is gone"), "{toast}");
}
