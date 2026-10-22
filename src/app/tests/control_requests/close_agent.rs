use rstest::rstest;

use super::super::close_agent::{agent, tabs, team};
use super::super::worktrees_dialog::{Setup, alive, commit_in, path_of, with};
use super::*;
use crate::control::arrange::AgentClosing;

fn close_agent(pane: u64, check: bool) -> Command {
    Command::CloseAgent(wire::CloseAgent { pane, check, ..wire::CloseAgent::default() })
}

fn closing(response: Response) -> AgentClosing {
    match response {
        Response::Ok(value) => serde_json::from_value(value).expect("an agent closing"),
        Response::Error(message) => panic!("close-agent was refused: {message}"),
    }
}

fn in_worktree() -> (Setup, u64, PathBuf) {
    let mut s = with(&["wt"]);
    let pane = agent(&mut s.app, 1, 0);
    let path = path_of(&s, "wt");
    (s, pane, path)
}

fn untouched(s: &Setup, path: &Path) -> bool {
    alive(&s.app, path) && path.exists()
}

#[test]
fn check_stops_nothing() {
    let (mut s, pane, path) = in_worktree();
    tabs(&mut s.app, 1, 1);
    std::fs::write(path.join("notes.txt"), "draft").expect("write file");

    let found = closing(now(&mut s.app, None, close_agent(pane, true)));

    assert_eq!((found.pane, found.agent.as_deref(), found.closed), (pane, Some("claude"), false));
    assert_eq!((found.removes, found.branches.len(), found.tabs.len(), found.stops.len()), (None, 0, 1, 1));
    assert!(untouched(&s, &path));
}

#[rstest]
#[case::a_shell(false, "conflict")]
#[case::a_pane_that_is_not_there(true, "not-found")]
fn a_shell_pane_is_refused(#[case] missing: bool, #[case] code: &str) {
    let (mut app, _rx, _dirs) = app_with(1);
    let pane = if missing { 999 } else { app.projects[0].workspaces[0].tabs[0].panes[0].id };

    let refused = refused(&mut app, "close-agent", &json!({"pane": pane}));

    assert_eq!(refused.code, code, "{}", refused.message);
    assert_eq!(app.projects[0].workspaces[0].tabs.len(), 1);
}

#[test]
fn a_pane_without_a_worktree_closes_its_tab_and_keeps_the_others() {
    let (mut app, rx, _dirs) = app_with(1);
    tabs(&mut app, 0, 1);
    let pane = agent(&mut app, 0, 1);
    let other = app.projects[0].workspaces[0].tabs[0].id;

    let closed = closing(now(&mut app, None, close_agent(pane, false)));

    assert_eq!((closed.removes, closed.closed, closed.stops.len()), (None, true, 1));
    pump_until(&mut app, &rx, "the agent's tab goes", |a| a.projects[0].workspaces[0].tabs.len() == 1);
    assert_eq!(app.projects[0].workspaces[0].tabs[0].id, other);
}

#[test]
fn an_agent_in_its_own_worktree_closes_its_tabs_and_keeps_the_worktree() {
    let (mut s, pane, path) = in_worktree();
    tabs(&mut s.app, 1, 1);
    std::fs::write(path.join("notes.txt"), "draft").expect("write file");
    commit_in(&path);
    let everything =
        wire::CloseAgent { pane, force: true, unlock: true, delete_branch: true, ..wire::CloseAgent::default() };

    let closed = closing(now(&mut s.app, None, Command::CloseAgent(everything)));

    assert_eq!((closed.closed, closed.removes, closed.branches.len()), (true, None, 0));
    pump_until(&mut s.app, &s.rx, "the tabs go", |a| a.projects[0].workspaces[1].tabs.len() == 1);
    assert_eq!(s.app.projects[0].workspaces.len(), 2);
    assert!(path.join("notes.txt").exists() && branch_exists(s.repo.path(), "wt"));
}

#[test]
fn a_run_member_closes_its_team_and_names_the_run() {
    let mut s = with(&["wt", "coder"]);
    let (lead, coder, kid) = team(&mut s);
    let run = s.app.runs.all()[0].id;

    let closed = closing(now(&mut s.app, None, close_agent(coder, false)));

    let panes = |refs: &[crate::control::arrange::PaneRef]| refs.iter().map(|r| r.pane).collect::<Vec<_>>();
    assert_eq!((closed.run, closed.closed, closed.removes), (Some(run), true, None));
    let stops = panes(&closed.stops);
    assert!(stops.contains(&lead) && stops.contains(&coder) && !stops.contains(&kid), "{stops:?}");
    assert_eq!(panes(&closed.keeps), [kid]);
    assert!(path_of(&s, "coder").exists());
}
