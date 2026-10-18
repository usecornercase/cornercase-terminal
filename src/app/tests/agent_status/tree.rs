use super::*;
use crate::flows::run::{Member, Run};
use crate::flows::{Flow, ORCHESTRATOR};

const TALL: Rect = Rect { x: 0, y: 0, width: 100, height: 30 };

struct Family {
    app: App,
    rx: Receiver<AppEvent>,
    _dirs: Vec<TempDir>,
    parent: u64,
    other: u64,
    child: u64,
}

fn family(activity: Activity) -> Family {
    let (mut app, rx, dirs) = app_with(1);
    app.config.agents_section = true;
    app.push_tab(0, 0, TALL, None).expect("a second tab");
    app.push_tab(0, 0, TALL, None).expect("a third tab");
    let mut ids = Vec::new();
    for tab in &mut app.projects[0].workspaces[0].tabs {
        let term = tab.pane_mut().expect("a pane");
        term.agent.follow(Some(agents::Known::Claude.name()));
        term.agent.update(Some(activity), activity != Activity::Waiting, Instant::now());
        ids.push(term.id);
    }
    let (parent, other, child) = (ids[0], ids[1], ids[2]);
    app.projects[0].workspaces[0].tabs[2].pane_mut().expect("the child").parent = Some(parent);
    render(&mut app, TALL);
    Family { app, rx, _dirs: dirs, parent, other, child }
}

fn listed(app: &App) -> Vec<u64> {
    app.agent_places().into_iter().map(|(p, w, t, i)| app.projects[p].workspaces[w].tabs[t].panes[i].id).collect()
}

fn shape(app: &App) -> Vec<(usize, bool, Vec<bool>)> {
    app.agents_view().entries.into_iter().map(|e| (e.depth, e.last, e.rails)).collect()
}

fn row_pos(app: &App, i: usize) -> Position {
    let a = app.layout(TALL).shown(app.nav);
    let r = ui::agent_row(a.agents_list, a.pitch, app.agent_places().len(), app.scroll.agents, i);
    Position::new(r.x + 4, r.y + r.height / 2)
}

fn pane_status(app: &App, pane: u64) -> Option<Status> {
    app.projects.pane_by(pane).and_then(|term| term.agent.status())
}

#[test]
fn a_child_is_listed_right_under_its_parent() {
    let f = family(Activity::Idle);

    assert_eq!(
        (listed(&f.app), shape(&f.app)),
        (vec![f.parent, f.child, f.other], vec![(0, false, vec![]), (1, true, vec![]), (0, true, vec![])])
    );
}

#[test]
fn a_click_on_a_child_shows_its_pane() {
    let mut f = family(Activity::Idle);
    let at = row_pos(&f.app, 1);

    click_in(&mut f.app, at, TALL);

    assert_eq!(f.app.term().map(|t| t.id), Some(f.child));
}

#[test]
fn a_right_click_on_a_child_offers_marking_its_pane() {
    let mut f = family(Activity::Idle);
    let at = row_pos(&f.app, 1);

    mouse_in(&mut f.app, MouseEventKind::Down(MouseButton::Right), at, TALL);
    mouse_in(&mut f.app, MouseEventKind::Up(MouseButton::Right), at, TALL);
    assert_eq!(menu_labels(&f.app), ["mark as pending review"]);
    pick(&mut f.app, "mark as pending review");

    assert_eq!(
        (pane_status(&f.app, f.child), pane_status(&f.app, f.parent)),
        (Some(Status::Pending), Some(Status::Idle))
    );
}

#[test]
fn closing_the_parent_puts_the_child_back_in_its_place() {
    let mut f = family(Activity::Idle);

    f.app.close_tab(0, 0, 0);
    wait_until("the parent's tab closes", || {
        drain(&mut f.app, &f.rx);
        f.app.projects[0].workspaces[0].tabs.len() == 2
    });
    let kept = f.app.projects.pane_by(f.child).and_then(|term| term.parent);

    assert_eq!(
        (listed(&f.app), shape(&f.app), kept),
        (vec![f.other, f.child], vec![(0, false, vec![]), (0, true, vec![])], Some(f.parent))
    );
}

#[test]
fn the_key_for_the_next_agent_follows_the_tree() {
    let mut f = family(Activity::Waiting);
    f.app.config.prefix_key = "ctrl+]".into();
    let mut visited = Vec::new();

    for _ in 0..3 {
        send_key(&mut f.app, KeyCode::Char(']'), KeyModifiers::CONTROL);
        send_key(&mut f.app, KeyCode::Char('a'), KeyModifiers::NONE);
        visited.extend(f.app.term().map(|t| t.id));
    }

    assert_eq!(visited, [f.child, f.other, f.parent]);
}

fn in_a_run(f: &mut Family) {
    let workspace = f.app.projects[0].workspaces[0].id;
    let mut run = Run::new(workspace, Flow::default(), "fix it".into());
    run.members.push(Member { role: ORCHESTRATOR.into(), pane: f.parent });
    run.members.push(Member { role: "coder".into(), pane: f.child });
    f.app.runs.push(run);
}

fn labels(app: &App) -> Vec<(Option<String>, bool)> {
    app.agents_view().entries.into_iter().map(|e| (e.role, e.place)).collect()
}

fn named(role: &str, place: bool) -> (Option<String>, bool) {
    (Some(role.to_string()), place)
}

#[test]
fn members_are_named_by_their_role_and_the_orchestrator_keeps_its_place() {
    let mut f = family(Activity::Idle);
    in_a_run(&mut f);

    assert_eq!(labels(&f.app), [named(ORCHESTRATOR, true), named("coder", false), (None, true)]);
}

#[rstest]
#[case::its_agent_exited(false)]
#[case::its_tab_closed(true)]
fn a_member_whose_orchestrator_is_not_listed_shows_its_place_and_its_role(#[case] close: bool) {
    let mut f = family(Activity::Idle);
    in_a_run(&mut f);

    if close {
        f.app.close_tab(0, 0, 0);
        wait_until("the parent's tab closes", || {
            drain(&mut f.app, &f.rx);
            f.app.projects[0].workspaces[0].tabs.len() == 2
        });
    } else {
        f.app.projects.pane_by_mut(f.parent).expect("the orchestrator").agent.follow(None);
    }

    let coder = f.app.agents_view().entries.into_iter().map(|e| (e.role, e.place)).find(|(role, _)| role.is_some());
    assert_eq!(coder, Some(named("coder", true)));
}

#[test]
fn a_renamed_tab_does_not_change_the_role() {
    let mut f = family(Activity::Idle);
    in_a_run(&mut f);
    f.app.projects[0].workspaces[0].tabs[2].name = Some("tester".into());
    f.app.projects[0].workspaces[0].tabs[1].name = Some("coder".into());

    assert_eq!(labels(&f.app), [named(ORCHESTRATOR, true), named("coder", false), (None, true)]);
}

#[test]
fn an_agent_a_member_starts_keeps_its_line() {
    let mut f = family(Activity::Idle);
    in_a_run(&mut f);
    f.app.push_tab(0, 0, TALL, None).expect("a fourth tab");
    let term = f.app.projects[0].workspaces[0].tabs[3].pane_mut().expect("a pane");
    term.agent.follow(Some(agents::Known::Claude.name()));
    term.agent.update(Some(Activity::Idle), true, Instant::now());
    term.parent = Some(f.child);

    let view = f.app.agents_view().entries;

    assert_eq!(
        view.iter().map(|e| (e.depth, e.role.clone(), e.place)).collect::<Vec<_>>(),
        [(0, Some(ORCHESTRATOR.to_string()), true), (1, Some("coder".into()), false), (2, None, true), (0, None, true)]
    );
}

#[test]
fn a_pane_keeps_its_seat_when_another_agent_runs_in_it() {
    let mut f = family(Activity::Idle);
    in_a_run(&mut f);

    let term = f.app.projects.pane_by_mut(f.child).expect("the coder");
    term.agent.follow(Some(agents::Known::Codex.name()));
    term.agent.update(Some(Activity::Idle), true, Instant::now());

    assert_eq!(labels(&f.app)[1], named("coder", false));
}

#[test]
fn status_gives_each_member_its_role() {
    let mut f = family(Activity::Idle);
    in_a_run(&mut f);

    let rows = f.app.report(None).panes();
    let role = |pane: u64| rows.iter().find(|row| row.pane == pane).map(|row| row.role.clone());

    assert_eq!(
        (role(f.parent), role(f.child), role(f.other)),
        (Some(Some(ORCHESTRATOR.to_string())), Some(Some("coder".to_string())), Some(None))
    );
}

#[test]
fn a_role_that_goes_changes_what_watchers_see() {
    let mut f = family(Activity::Idle);
    in_a_run(&mut f);
    let before = f.app.refreshed();

    f.app.runs[0].members.retain(|member| member.role != "coder");

    assert_ne!(f.app.refreshed(), before);
}
