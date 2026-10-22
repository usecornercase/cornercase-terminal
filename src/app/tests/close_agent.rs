use rstest::rstest;

use super::worktrees_dialog::{Setup, alive, path_of, with};
use super::*;
use crate::agents::Known;
use crate::agents::activity::Activity;
use crate::app::close_agent::CloseAgent;
use crate::app::worktrees::RemoveOptions;
use crate::flows::run::{Member, RoleWorktree, Run};
use crate::flows::{Flow, ORCHESTRATOR};
use crate::test_util::git;

pub(super) fn agent(app: &mut App, w: usize, t: usize) -> u64 {
    let term = app.projects[0].workspaces[w].tabs[t].pane_mut().expect("a pane");
    term.agent.follow(Some(Known::Claude.name()));
    term.agent.update(Some(Activity::Idle), true, Instant::now());
    term.id
}

fn child(app: &mut App, (w, t): (usize, usize), parent: u64) -> u64 {
    let id = agent(app, w, t);
    app.projects.pane_by_mut(id).expect("the child").parent = Some(parent);
    id
}

fn shell_child(app: &mut App, (w, t): (usize, usize), parent: u64) -> u64 {
    let term = app.projects[0].workspaces[w].tabs[t].pane_mut().expect("a pane");
    term.parent = Some(parent);
    term.id
}

pub(super) fn tabs(app: &mut App, w: usize, more: usize) {
    for _ in 0..more {
        app.push_tab(workspace_at(app, 0, w), AREA, None).expect("another tab");
    }
}

fn dialog(app: &App) -> &CloseAgent {
    match &app.overlay {
        Some(Overlay::CloseAgent(dialog)) => dialog,
        _ => panic!("the close agent dialog is open"),
    }
}

fn shown(app: &App) -> ui::Update {
    match overlay_shown(app, AREA) {
        ui::Overlay::Update(update) => update,
        _ => panic!("the close agent dialog is shown"),
    }
}

fn text(app: &App) -> String {
    let view = shown(app);
    let notes: String = view.notes.iter().map(ToString::to_string).collect();
    format!("{}\n{notes}", view.message)
}

fn confirm(app: &mut App) {
    let [submit, _] = ui::update_buttons(AREA, shown(app).submit, ui::CANCEL_LABEL);
    click(app, Position::new(submit.x + 1, submit.y));
}

fn closed(app: &mut App, rx: &Receiver<AppEvent>, w: usize, left: usize) {
    pump_until(app, rx, "the tabs close", |a| a.projects[0].workspaces[w].tabs.len() == left);
}

fn running(app: &App, pane: u64) -> bool {
    app.projects.pane_by(pane).and_then(Term::shell_pid).is_some_and(process::alive)
}

fn tab_panes(app: &App, w: usize) -> Vec<u64> {
    app.projects[0].workspaces[w].terms().map(|t| t.id).collect()
}

fn in_worktree() -> (Setup, u64) {
    let mut s = with(&["wt"]);
    let pane = agent(&mut s.app, 1, 0);
    (s, pane)
}

fn asked(s: &mut Setup, pane: u64) {
    s.app.ask_close_agent(pane);
}

#[test]
fn the_row_menu_offers_close_agent() {
    let (mut app, _rx, _dirs) = app_with(1);
    let pane = agent(&mut app, 0, 0);

    right_click_first_agent(&mut app);
    let last = menu_labels(&app).last().cloned();
    pick(&mut app, "close agent");

    assert_eq!((last.as_deref(), dialog(&app).plan.pane), (Some("close agent"), pane));
}

#[derive(Debug, Clone, Copy)]
enum Leave {
    Escape,
    Outside,
    Cancel,
}

fn leave(app: &mut App, way: Leave) {
    match way {
        Leave::Escape => send_key(app, KeyCode::Esc, KeyModifiers::NONE),
        Leave::Outside => click(app, Position::new(0, 0)),
        Leave::Cancel => {
            let [_, cancel] = ui::update_buttons(AREA, shown(app).submit, ui::CANCEL_LABEL);
            click(app, Position::new(cancel.x + 1, cancel.y));
        }
    }
}

#[rstest]
#[case::escape(Leave::Escape)]
#[case::a_press_outside(Leave::Outside)]
#[case::cancel(Leave::Cancel)]
fn nothing_stops_before_confirming(#[case] way: Leave) {
    let (mut s, pane) = in_worktree();
    tabs(&mut s.app, 1, 1);
    asked(&mut s, pane);

    leave(&mut s.app, way);

    let path = path_of(&s, "wt");
    assert_eq!((s.app.overlay.is_none(), alive(&s.app, &path), path.exists()), (true, true, true));
}

#[test]
fn a_child_elsewhere_keeps_running_when_its_parent_leaves_its_own_worktree() {
    let mut s = with(&["wt"]);
    tabs(&mut s.app, 1, 1);
    let parent = agent(&mut s.app, 1, 0);
    let kid = child(&mut s.app, (0, 0), parent);
    asked(&mut s, parent);
    let named = text(&s.app).contains("keep running");

    confirm(&mut s.app);

    closed(&mut s.app, &s.rx, 1, 1);
    assert_eq!((named, running(&s.app, kid), path_of(&s, "wt").exists()), (true, true, true));
}

#[test]
fn an_agent_in_the_main_checkout_closes_its_tab_and_its_childrens_tabs() {
    let (mut app, rx, _dirs) = app_with(1);
    tabs(&mut app, 0, 3);
    let parent = agent(&mut app, 0, 0);
    let other = agent(&mut app, 0, 1);
    let kid = child(&mut app, (0, 2), parent);
    shell_child(&mut app, (0, 3), kid);

    app.ask_close_agent(parent);
    confirm(&mut app);

    closed(&mut app, &rx, 0, 1);
    assert_eq!(tab_panes(&app, 0), [other]);
}

#[test]
fn other_tabs_of_the_main_checkout_stay() {
    let (mut app, rx, _dirs) = app_with(1);
    tabs(&mut app, 0, 2);
    let parent = agent(&mut app, 0, 0);
    let other = agent(&mut app, 0, 1);
    let shell = tab_panes(&app, 0)[2];

    app.ask_close_agent(parent);
    let message = text(&app);
    confirm(&mut app);

    closed(&mut app, &rx, 0, 2);
    let kept = (running(&app, other), running(&app, shell), message.contains("Also stops"));
    assert_eq!((app.overlay.is_none(), kept), (true, (true, true, false)));
}

#[test]
fn a_split_tab_closes_whole_and_names_the_other_pane() {
    let (mut app, rx, _dirs) = app_with(1);
    tabs(&mut app, 0, 1);
    let pane = agent(&mut app, 0, 0);
    app.pane_action(pane, PaneAction::Split(Dir::Right), AREA).expect("split");
    running_sleep(&mut app);

    app.ask_close_agent(pane);
    let message = text(&app);
    confirm(&mut app);

    closed(&mut app, &rx, 0, 1);
    assert!(message.contains("sleep, in the same tab"), "{message}");
}

#[test]
fn an_agent_in_its_own_worktree_closes_only_its_tab() {
    let (mut s, pane) = in_worktree();
    tabs(&mut s.app, 1, 1);
    let path = path_of(&s, "wt");
    asked(&mut s, pane);
    let view = shown(&s.app);

    confirm(&mut s.app);

    closed(&mut s.app, &s.rx, 1, 1);
    let heading = view.message.contains("Its tab closes; nothing is deleted from disk.");
    assert_eq!((heading, view.submit, view.note), (true, CLOSE_SUBMIT, None));
    assert_eq!(
        (s.app.projects[0].workspaces.len(), path.exists(), branch_exists(s.repo.path(), "wt")),
        (2, true, true)
    );
}

#[test]
fn changes_and_a_lock_are_left_alone() {
    let (mut s, pane) = in_worktree();
    tabs(&mut s.app, 1, 1);
    let path = path_of(&s, "wt");
    std::fs::write(path.join("draft.txt"), "draft").expect("write a change");
    git(s.repo.path(), &["worktree", "lock", "--reason", "testing", path.to_str().expect("utf-8")]);
    asked(&mut s, pane);
    let (message, submit) = (text(&s.app), shown(&s.app).submit);

    confirm(&mut s.app);

    closed(&mut s.app, &s.rx, 1, 1);
    let quiet = !message.contains(UNCOMMITTED) && !message.contains("locked");
    let listed = std::process::Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .current_dir(s.repo.path())
        .output()
        .expect("git worktree list");
    let locked = String::from_utf8_lossy(&listed.stdout).contains("locked");
    assert_eq!((quiet, submit, path.join("draft.txt").exists(), locked), (true, CLOSE_SUBMIT, true, true));
}

pub(super) fn team(s: &mut Setup) -> (u64, u64, u64) {
    let role = path_of(s, "coder");
    s.app.projects[0].workspaces[1].role_worktrees.push(RoleWorktree {
        role: "coder".into(),
        path: role,
        branch: "coder".into(),
    });
    tabs(&mut s.app, 1, 1);
    let lead = agent(&mut s.app, 1, 0);
    let coder = child(&mut s.app, (1, 1), lead);
    let kid = child(&mut s.app, (2, 0), coder);
    let mut run = Run::new(s.app.projects[0].workspaces[1].id, Flow::default(), "x".into());
    run.members = vec![Member { role: ORCHESTRATOR.into(), pane: lead }, Member { role: "coder".into(), pane: coder }];
    s.app.runs.push(run);
    (lead, coder, kid)
}

#[test]
fn a_run_member_closes_its_whole_team_and_nothing_else() {
    let mut s = with(&["wt", "coder"]);
    let (_, coder, kid) = team(&mut s);
    tabs(&mut s.app, 1, 1);
    let shell = tab_panes(&s.app, 1)[2];
    let (wt, role) = (path_of(&s, "wt"), path_of(&s, "coder"));
    asked(&mut s, coder);
    let message = text(&s.app);

    confirm(&mut s.app);

    closed(&mut s.app, &s.rx, 1, 1);
    pump_until(&mut s.app, &s.rx, "the team stops", |a| a.projects[0].workspaces[1].terms().count() == 1);
    s.app.forget_runs();
    let named = message.contains("Ends the flow run") && message.contains("coder (claude)");
    assert!(named && message.contains("keep running") && !message.contains("worktree of"), "{message}");
    assert_eq!(
        (tab_panes(&s.app, 1), running(&s.app, kid), wt.exists(), role.exists()),
        (vec![shell], true, true, true)
    );
    assert!(s.app.runs.all().is_empty());
}

#[test]
fn an_agent_a_member_started_closes_alone() {
    let mut s = with(&["wt", "coder"]);
    let (_, coder, _) = team(&mut s);
    tabs(&mut s.app, 1, 1);
    let helper = child(&mut s.app, (1, 2), coder);
    asked(&mut s, helper);
    let message = text(&s.app);
    let plan = dialog(&s.app).plan.clone();

    confirm(&mut s.app);

    closed(&mut s.app, &s.rx, 1, 2);
    assert_eq!((plan.run, message.contains("Ends the flow run"), plan.closing.tabs.len()), (None, false, 1));
    assert_eq!(s.app.runs.all().len(), 1);
}

#[test]
fn a_child_elsewhere_keeps_running_and_is_named() {
    let mut s = with(&["wt"]);
    let parent = agent(&mut s.app, 0, 0);
    let kid = child(&mut s.app, (1, 0), parent);

    s.app.ask_close_agent(parent);
    let (message, submit) = (text(&s.app), shown(&s.app).submit);
    confirm(&mut s.app);

    closed(&mut s.app, &s.rx, 0, 0);
    let root = s.app.agent_tree(false).iter().all(|(_, node)| node.depth == 0);
    let named = message.contains("keep running") && message.contains("claude in ") && message.contains("› wt");
    assert_eq!((named, submit, running(&s.app, kid), root), (true, CLOSE_SUBMIT, true, true), "{message}");
}

#[test]
fn a_new_child_after_opening_shows_the_confirmation_again() {
    let (mut app, rx, _dirs) = app_with(1);
    tabs(&mut app, 0, 1);
    let parent = agent(&mut app, 0, 0);
    app.ask_close_agent(parent);
    let kid = child(&mut app, (0, 1), parent);

    confirm(&mut app);
    let again = (dialog(&app).plan.closing.inside.clone(), running(&app, kid), text(&app).contains("close with it"));
    confirm(&mut app);

    closed(&mut app, &rx, 0, 0);
    assert_eq!(again, (vec![kid], true, true));
}

#[test]
fn a_pane_that_closed_meanwhile_closes_the_dialog() {
    let (mut app, rx, _dirs) = app_with(1);
    tabs(&mut app, 0, 1);
    let parent = agent(&mut app, 0, 0);
    let other = tab_panes(&app, 0)[1];
    app.ask_close_agent(parent);
    app.close_tab(tab_at(&app, 0, 0, 0));
    closed(&mut app, &rx, 0, 1);

    confirm(&mut app);

    assert_eq!((app.overlay.is_none(), running(&app, other)), (true, true));
}

#[test]
fn a_workspace_being_removed_refuses_the_close() {
    let (mut s, pane) = in_worktree();
    asked(&mut s, pane);
    let (project, workspace) = (s.app.projects[0].id, s.app.projects[0].workspaces[1].id);
    s.app.start_removal(project, workspace, RemoveOptions::default(), None);

    confirm(&mut s.app);
    let after = (s.app.overlay.is_none(), toast(&s.app).is_some_and(|text| text.contains("being removed")));
    s.app.ask_close_agent(pane);

    assert_eq!((after, s.app.overlay.is_none()), ((true, true), true));
}

#[test]
fn pressing_the_close_button_opens_the_confirmation() {
    let (mut app, _rx, _dirs) = app_with(1);
    let pane = agent(&mut app, 0, 0);
    app.config.agents_section = true;
    let a = app.layout(AREA);
    let row = ui::agent_row(a.agents_list, a.pitch, app.agent_places().len(), app.scroll.of(List::Agents), 0);
    let close = ui::agent_close(row, a.pitch).as_position();

    press(&mut app, close);

    assert_eq!((dialog(&app).plan.pane, app.screen.drag.is_none()), (pane, true));
}

#[test]
fn pressing_beside_the_close_button_still_grabs_the_row() {
    let (mut app, _rx, _dirs) = app_with(1);
    agent(&mut app, 0, 0);
    app.config.agents_section = true;
    let a = app.layout(AREA);
    let row = ui::agent_row(a.agents_list, a.pitch, app.agent_places().len(), app.scroll.of(List::Agents), 0);

    press(&mut app, Position::new(row.x + 4, row.y));

    assert_eq!((app.overlay.is_none(), app.screen.drag.is_some()), (true, true));
}
