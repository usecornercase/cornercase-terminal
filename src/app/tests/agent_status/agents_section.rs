use super::*;

const TALL: Rect = Rect { x: 0, y: 0, width: 100, height: 30 };
const SMALL: Rect = Rect { x: 0, y: 0, width: 80, height: 30 };

fn statuses(app: &App) -> Vec<Option<Status>> {
    app.agents_view().entries.iter().map(|e| e.status).collect()
}

fn agent_pos(app: &App, area: Rect, i: usize) -> Position {
    let a = app.layout(area).shown(app.screen.nav);
    let r = ui::agent_row(a.agents_list, a.pitch, app.agent_places().len(), app.scroll.of(List::Agents), i);
    Position::new(r.x + 4, r.y + r.height / 2)
}

fn focus(app: &App) -> (usize, Option<u64>) {
    (app.active, app.term().map(|t| t.id))
}

fn two_agents() -> (App, Receiver<AppEvent>, Vec<TempDir>, FakeClaude, FakeCodex) {
    let (mut app, rx, dirs) = app_with(2);
    app.config.agents_section = true;
    app.active = 0;
    let claude = FakeClaude::running(FakeClaude::SILENT);
    claude.start(&mut app);
    pump_until(&mut app, &rx, "claude runs", |a| runs(a, agents::Known::Claude.name()));
    app.active = 1;
    let codex = FakeCodex::new("019a1234-5678-7000-8000-000000000001", "gpt-5.4", false);
    type_line(&mut app, &codex.command_line());
    watch_until(&mut app, &rx, "both agents are listed", |a| a.agent_places().len() == 2);
    (app, rx, dirs, claude, codex)
}

#[test]
fn lists_every_agent_and_a_click_shows_its_pane() {
    let (mut app, rx, _dirs, claude, codex) = two_agents();
    let pane = app.layout(TALL).pane;
    let codex_pane = app.term().map(|t| t.id);
    right_click(&mut app, Position::new(pane.x + 1, pane.y + 1));
    pick(&mut app, "split right");
    app.active = 0;
    render(&mut app, TALL);
    let agents: Vec<String> = app.agents_view().entries.into_iter().map(|e| e.agent).collect();

    let at = agent_pos(&app, TALL, 1);
    click_in(&mut app, at, TALL);
    let shown = focus(&app);
    claude.signal("quit");
    codex.signal("quit", "");
    drop(rx);

    assert_eq!((agents, shown), (vec!["claude".to_string(), "codex".to_string()], (1, codex_pane)));
}

#[test]
fn the_rows_follow_an_agent_as_it_starts_changes_and_quits() {
    let (mut app, rx, _dirs) = app_with(1);
    app.config.agents_section = true;
    let claude = FakeClaude::new();
    let before = statuses(&app);

    claude.start(&mut app);
    watch_until(&mut app, &rx, "claude works", |a| statuses(a) == [Some(Status::Working)]);
    claude.signal("finish");
    watch_until(&mut app, &rx, "claude goes idle", |a| statuses(a) == [Some(Status::Idle)]);
    claude.signal("quit");
    watch_until(&mut app, &rx, "the row goes", |a| statuses(a).is_empty());

    assert_eq!(before, []);
}

#[test]
fn the_switch_shows_and_hides_the_section() {
    let (mut app, _rx, _dirs) = app_with(1);
    let off = text(&render(&mut app, TALL), app.layout(TALL).agents_list).trim().to_string();

    app.config.agents_section = true;
    let list = app.layout(TALL).agents_list;
    let on = text(&render(&mut app, TALL), list).trim().to_string();

    assert_eq!((off.as_str(), on.as_str()), ("", "no agents running"));
}

#[test]
fn dragging_its_line_resizes_it_and_the_session_keeps_it() {
    let (mut app, _rx, _dirs) = app_with(1);
    app.config.agents_section = true;
    let line = app.layout(TALL).agents_border;
    let before = app.layout(TALL).agents.height;

    mouse_in(&mut app, MouseEventKind::Down(MouseButton::Left), Position::new(3, line.y), TALL);
    mouse_in(&mut app, MouseEventKind::Drag(MouseButton::Left), Position::new(3, line.y - 3), TALL);
    mouse_in(&mut app, MouseEventKind::Up(MouseButton::Left), Position::new(3, line.y - 3), TALL);

    assert_eq!(
        (app.layout(TALL).agents.height, app.state().widths.and_then(|w| w.agents)),
        (before + 3, Some(before + 3))
    );
}

#[test]
fn the_wheel_scrolls_it() {
    let (mut app, rx, _dirs, claude, codex) = two_agents();
    let area = Rect { height: 20, ..TALL };
    let at = app.layout(area).agents_list.as_position();

    mouse_in(&mut app, MouseEventKind::ScrollDown, at, area);
    let scroll = app.scroll.of(List::Agents);
    claude.signal("quit");
    codex.signal("quit", "");
    drop(rx);

    assert_eq!(scroll, 1);
}

#[test]
fn compact_mode_opens_it_from_the_projects_menu_and_a_click_shows_the_pane() {
    let (mut app, rx, _dirs, claude, codex) = two_agents();
    let claude_pane = app.projects[0].workspaces[0].tabs[0].panes[0].id;
    app.screen.nav = Some(ui::Nav::Projects);
    render(&mut app, SMALL);

    let button = app.layout(SMALL).shown(app.screen.nav).agents_button.as_position();
    click_in(&mut app, button, SMALL);
    let nav = app.screen.nav;
    let at = agent_pos(&app, SMALL, 0);
    click_in(&mut app, at, SMALL);
    claude.signal("quit");
    codex.signal("quit", "");
    drop(rx);

    assert_eq!((nav, app.screen.nav, focus(&app)), (Some(ui::Nav::Agents), None, (0, Some(claude_pane))));
}
