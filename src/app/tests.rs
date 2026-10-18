use std::path::Path;
use std::sync::mpsc::{self, Receiver};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;

use super::actions::NO_AGENT;
use super::dialogs::CLOSE_PROJECTS_SUBMIT;
use super::dialogs::KEEP_PROJECTS_SUBMIT;
use super::input::WHEEL_ROWS;
use super::interaction::DOUBLE_CLICK;
use super::menus::MenuAction;
use super::menus::PaneAction;
use super::menus::Target;
use super::overlay::CLOSE_SUBMIT;
use super::overlay::CREATE_SUBMIT;
use super::overlay::DELETE_SUBMIT;
use super::overlay::FORCE_REMOVE_SUBMIT;
use super::overlay::LATER;
use super::overlay::REMOVE_SUBMIT;
use super::overlay::RENAME_SUBMIT;
use super::overlay::RESTART_SUBMIT;
use super::overlay::RETRY_UPDATE_SUBMIT;
use super::overlay::UNLOCK_SUBMIT;
use super::overlay::UPDATE_SUBMIT;
use super::panes::pane_cell;
use super::picker_dialog::PICKER_SUBMIT;
use super::session::RESUME_GRACE;
use super::toast::BUG_FOR;
use super::toast::COPIED;
#[cfg(target_os = "linux")]
use super::toast::NOT_READING;
use super::toast::TOAST_FOR;
use super::toast::UNDO_FOR;
use super::update_dialog::RESTART_LABEL;
use super::update_dialog::UPDATE_AVAILABLE;
use super::update_dialog::UpdateStep;
use super::worktrees::COUNT_BEHIND_EVERY;
use super::worktrees::Check;
use super::worktrees::UNCOMMITTED;
use super::worktrees::lock_message;
use super::worktrees::lock_note;
use super::*;
use crate::agents::AgentState;
use crate::agents::context::{self};
use crate::changes::Checkout;
use crate::config;
use crate::git;
use crate::host::clipboard;
use crate::host::notify::{self};
use crate::issues::browser::{Browser, Screen};
use crate::issues::{self, Secret};
use crate::paths;
use crate::process;
use crate::project::picker::Picker;
use crate::project::search::Finder;
use crate::project::search::{Goto, Kind};
use crate::project::split::{self};
use crate::project::{Phase, Workspace};
use crate::settings::{Settings, Status};
use crate::state;
use crate::state::{PaneState, TabState, WorkspaceState};
use crate::term::Term;
use crate::term::mouse;
use crate::test_util::{Locked, TempDir, git_repo, is_sh, wait_until, wait_until_within};
use crate::ui::SidebarRow;
use crate::ui::WorkspaceRow;
use crate::ui::changes::{self as panel, Action as HunkAction};
use crate::update::{self, Release};
use crate::version;

fn drain(app: &mut App, rx: &Receiver<AppEvent>) {
    drain_in(app, rx, AREA);
}

fn drain_in(app: &mut App, rx: &Receiver<AppEvent>, area: Rect) {
    while let Ok(ev) = rx.try_recv() {
        app.handle_event(ev, area).expect("handle event");
    }
}

fn render(app: &mut App, area: Rect) -> Terminal<TestBackend> {
    let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).expect("test backend");
    terminal.draw(|f| _ = app.draw(f, &Sight::default(), Instant::now())).expect("draw");
    terminal
}

fn render_frame(app: &mut App, area: Rect, sight: &Sight) -> (Buffer, Option<Placed>) {
    let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).expect("terminal");
    let mut placed = None;
    let buffer = terminal.draw(|f| placed = app.draw(f, sight, Instant::now())).expect("draw").buffer.clone();
    (buffer, placed)
}

fn write_config(dir: &Path, config: &Config) -> PathBuf {
    let path = dir.join("config.json");
    config::save(&path, config).expect("write config");
    path
}

fn drag(app: &mut App, from: Position, to: Position) {
    press(app, from);
    mouse(app, MouseEventKind::Drag(MouseButton::Left), to);
    mouse(app, MouseEventKind::Up(MouseButton::Left), to);
}

fn panel_area(app: &App) -> Rect {
    app.layout(AREA).changes
}

fn fake_shortcut<B: Into<Vec<u8>>>(app: &mut App, routes: Vec<(&'static str, u16, B)>) -> crate::test_util::FakeHttp {
    let server = crate::test_util::FakeHttp::start(routes);
    app.issues.apis.shortcut = format!("{}/api/v3", server.url());
    server
}

fn closing_last_tab() -> (App, Receiver<AppEvent>, Vec<TempDir>) {
    let (mut app, rx, dirs) = app_with(1);
    click_close(&mut app, WorkspaceRow::Tab(0, 0));
    pump_until(&mut app, &rx, "the tab closes", |a| a.projects[0].workspaces[0].tabs.is_empty());
    (app, rx, dirs)
}

fn pointer_after_drag(app: &mut App, away: Position) -> pointer::Shape {
    mouse(app, MouseEventKind::Drag(MouseButton::Left), away);
    let dragging = app.pointer(AREA);
    mouse(app, MouseEventKind::Up(MouseButton::Left), away);
    dragging
}

fn running_sleep(app: &mut App) {
    type_line(app, "sleep 30");
    wait_until("sleep runs", || app.term().and_then(|t| t.program(&app.config)).as_deref() == Some("sleep"));
}

const AREA: Rect = Rect { x: 0, y: 0, width: 100, height: 20 };

fn areas() -> ui::Areas {
    ui::layout(AREA, state::Widths::default())
}

fn no_config() -> PathBuf {
    std::env::temp_dir().join("cornercase-test-no-config").join("config.json")
}

fn new_app(config_path: PathBuf) -> (App, Receiver<AppEvent>) {
    let (tx, rx) = mpsc::channel();
    let mut app = App::new("/bin/sh".into(), HostTheme::default(), config_path, tx);
    app.config.sidebar = config::Sidebar::SideBySide.id().into();
    app.config.sound = false;
    app.typing_holds = Duration::ZERO;
    if app.config.gh == config::DEFAULT_GH {
        app.config.gh = "/nonexistent/gh".into();
        app.config.glab = "/nonexistent/glab".into();
    }
    (app, rx)
}

fn empty_app() -> (App, Receiver<AppEvent>) {
    new_app(no_config())
}

fn app() -> (App, Receiver<AppEvent>) {
    let (mut app, rx) = empty_app();
    app.open_here(AREA).expect("open the first project");
    (app, rx)
}

fn app_with(n: usize) -> (App, Receiver<AppEvent>, Vec<TempDir>) {
    let dirs: Vec<TempDir> = (0..n).map(|_| TempDir::new()).collect();
    let (mut app, rx) = empty_app();
    for dir in &dirs {
        app.open_project(dir.path().to_path_buf(), AREA).expect("open project");
    }
    (app, rx, dirs)
}

fn app_in(dir: &Path, config_path: PathBuf) -> (App, Receiver<AppEvent>) {
    let (mut app, rx) = new_app(config_path);
    app.open_project(dir.to_path_buf(), AREA).expect("open project");
    (app, rx)
}

fn repo_with_edit() -> TempDir {
    let repo = git_repo(&[("a.txt", "one\ntwo\n")]);
    std::fs::write(repo.path().join("a.txt"), "one\nTWO\n").expect("edit");
    repo
}

fn shop_with_topic_worktree() -> (TempDir, TempDir) {
    let shop = repo_with_edit();
    let worktree = TempDir::new();
    let checkout = worktree.path().join("topic");
    crate::test_util::git(
        shop.path(),
        &["worktree", "add", "--quiet", "-b", "topic", checkout.to_str().expect("path")],
    );
    std::fs::write(checkout.join("new.txt"), "new\n").expect("edit worktree");
    (shop, worktree)
}

fn term(app: &App, p: usize) -> &Term {
    app.projects[p].workspace().and_then(Workspace::tab).and_then(Tab::pane).expect("the project has a pane")
}

fn tab_term(app: &App, w: usize, t: usize) -> &Term {
    app.projects[app.active].workspaces[w].tabs[t].pane().expect("the tab has a pane")
}

fn canonical(dir: &TempDir) -> PathBuf {
    dir.path().canonicalize().expect("canonicalize")
}

fn click_button(app: &mut App, area: Rect, labels: &[&str], rects: &[Rect], label: &str) {
    let i = labels.iter().position(|b| *b == label).unwrap_or_else(|| panic!("no {label} button"));
    click_in(app, rects[i].as_position(), area);
}

fn overlay_shown(app: &App, area: Rect) -> ui::Overlay {
    app.overlay.as_ref().and_then(|o| app.overlay_view(o, area)).expect("a dialog is shown")
}

fn toast(app: &App) -> Option<&str> {
    app.toast.as_ref().map(|t| t.message.as_str())
}

fn screen(app: &mut App) -> String {
    app.term_mut().and_then(|t| t.emulator.snapshot().ok()).map(|s| s.contents()).unwrap_or_default()
}

fn numbered(term: &mut Term, lines: usize) {
    let text: Vec<String> = (1..=lines).map(|i| format!("line-{i}")).collect();
    term.feed(format!("\x1b[2J\x1b[H{}", text.join("\r\n")).as_bytes());
}

fn shows(app: &mut App, rx: &Receiver<AppEvent>, text: &str) {
    wait_until(text, || {
        drain(app, rx);
        screen(app).contains(text)
    });
}

fn type_in_pane(app: &mut App, rx: &Receiver<AppEvent>, word: &str) {
    type_text(app, &format!("echo {word}-\"\"typed"));
    send_key(app, KeyCode::Enter, KeyModifiers::NONE);
    wait_until("the pane gets the keys", || {
        drain(app, rx);
        screen(app).contains(&format!("{word}-typed"))
    });
}

fn refreshing(app: &mut App, rx: &Receiver<AppEvent>, what: &str, cond: impl Fn(&App) -> bool) {
    wait_until(what, || {
        drain(app, rx);
        app.refresh(Instant::now());
        cond(app)
    });
}

fn pump_until(app: &mut App, rx: &Receiver<AppEvent>, what: &str, cond: impl Fn(&App) -> bool) {
    wait_until(what, || {
        drain(app, rx);
        cond(app)
    });
}

fn send_key(app: &mut App, code: KeyCode, mods: KeyModifiers) {
    app.handle_event(AppEvent::Input(Event::Key(KeyEvent::new(code, mods))), AREA).expect("handle key");
}

fn mouse(app: &mut App, kind: MouseEventKind, pos: Position) {
    mouse_in(app, kind, pos, AREA);
}

fn mouse_in(app: &mut App, kind: MouseEventKind, pos: Position, area: Rect) {
    let ev = MouseEvent { kind, column: pos.x, row: pos.y, modifiers: KeyModifiers::NONE };
    app.handle_event(AppEvent::Input(Event::Mouse(ev)), area).expect("handle mouse");
}

fn mouse_down(app: &mut App, button: MouseButton, pos: Position) {
    mouse(app, MouseEventKind::Down(button), pos);
}

fn click_in(app: &mut App, pos: Position, area: Rect) {
    mouse_in(app, MouseEventKind::Down(MouseButton::Left), pos, area);
    mouse_in(app, MouseEventKind::Up(MouseButton::Left), pos, area);
}

fn click(app: &mut App, pos: Position) {
    click_in(app, pos, AREA);
}

fn press(app: &mut App, pos: Position) {
    mouse_down(app, MouseButton::Left, pos);
}

fn right_click(app: &mut App, pos: Position) {
    mouse_down(app, MouseButton::Right, pos);
    mouse(app, MouseEventKind::Up(MouseButton::Right), pos);
}

fn type_line(app: &mut App, line: &str) {
    app.handle_event(AppEvent::Input(Event::Paste(format!("{line}\r"))), AREA).expect("handle paste");
}

fn type_text(app: &mut App, text: &str) {
    for c in text.chars() {
        send_key(app, KeyCode::Char(c), KeyModifiers::NONE);
    }
}

fn submit_text(app: &mut App, text: &str) {
    type_text(app, text);
    send_key(app, KeyCode::Enter, KeyModifiers::NONE);
}

fn list() -> Rect {
    areas().list
}

fn entry_pos() -> Position {
    Position::new(list().x + 3, list().y)
}

fn sidebar_pos(app: &App, row: SidebarRow) -> Position {
    let r = ui::entry_row(list(), 1, &app.sidebar_rows(), app.scroll.projects, row);
    Position::new(r.x + 3, r.y)
}

fn sidebar_close(app: &App, row: SidebarRow) -> Position {
    ui::close_button(list(), 1, &app.sidebar_rows(), app.scroll.projects, row).as_position()
}

fn row_rect(app: &App, row: WorkspaceRow) -> Rect {
    ui::workspace_row(areas().workspaces_list, areas().pitch, &app.tab_lines(), app.scroll.workspaces, row)
}

fn row_pos(app: &App, row: WorkspaceRow) -> Position {
    let r = row_rect(app, row);
    Position::new(r.x + 6, r.y)
}

fn row_close(app: &App, row: WorkspaceRow) -> Position {
    ui::row_close_button(row_rect(app, row), areas().pitch).as_position()
}

fn click_row(app: &mut App, row: WorkspaceRow) {
    let pos = row_pos(app, row);
    click(app, pos);
}

fn click_close(app: &mut App, row: WorkspaceRow) {
    let pos = row_close(app, row);
    click(app, pos);
}

fn right_click_row(app: &mut App, row: WorkspaceRow) {
    let pos = row_pos(app, row);
    right_click(app, pos);
}

fn new_workspace_pos(app: &App) -> Position {
    ui::new_workspace_button(areas().workspaces_list, areas().pitch, &app.tab_lines()).as_position()
}

fn click_new_workspace(app: &mut App) {
    let new = new_workspace_pos(app);
    click(app, new);
    pick(app, "new workspace");
}

fn form_value(app: &App) -> Option<&str> {
    match &app.overlay {
        Some(Overlay::NewWorkspace { input, .. } | Overlay::Rename { input, .. }) => Some(input),
        _ => None,
    }
}

fn confirmation(app: &App) -> Option<String> {
    match app.overlay_view(app.overlay.as_ref()?, AREA)? {
        ui::Overlay::Confirm(confirm) => Some(confirm.message),
        _ => None,
    }
}

fn form_error(app: &App) -> Option<&str> {
    match &app.overlay {
        Some(Overlay::NewWorkspace { error, .. }) => error.as_deref(),
        Some(Overlay::RemoveWorkspace { check: Check::Changed, .. }) => Some(UNCOMMITTED),
        _ => None,
    }
}

fn clear_input(app: &mut App) {
    while form_value(app).is_some_and(|v| !v.is_empty()) {
        send_key(app, KeyCode::Backspace, KeyModifiers::NONE);
    }
}

fn form_button(submit: &str, which: usize) -> Position {
    ui::form_buttons(ui::form_area(AREA), submit)[which].as_position()
}

fn menu_labels(app: &App) -> Vec<String> {
    let Some(Overlay::Menu { actions, .. }) = &app.overlay else { panic!("the menu is not open") };
    app.menu_labels(actions)
}

fn menu_item_at(app: &App, i: usize) -> Position {
    let Some(Overlay::Menu { at, .. }) = &app.overlay else { panic!("the menu is not open") };
    ui::menu_item(ui::menu_area(AREA, *at, &menu_labels(app)), i).as_position()
}

fn pick(app: &mut App, label: &str) {
    let i = menu_labels(app).iter().position(|l| l == label).expect("the menu has the item");
    let pos = menu_item_at(app, i);
    click(app, pos);
}

fn plain(projects: usize) -> Vec<SidebarRow> {
    ui::sidebar_rows(&vec![None; projects], &[])
}

fn new_project_pos(app: &App) -> Position {
    ui::new_project_button(list(), 1, &app.sidebar_rows()).as_position()
}

fn click_new_project(app: &mut App) {
    let new = new_project_pos(app);
    click(app, new);
    pick(app, "open project");
}

fn workspace_labels(app: &App) -> Vec<String> {
    app.projects[app.active].workspaces.iter().map(Workspace::label).collect()
}

fn with_worktrees_config() -> (TempDir, TempDir, PathBuf) {
    let (worktrees, config) = (TempDir::new(), TempDir::new());
    let worktrees_dir = worktrees.path().display().to_string();
    let config_path = write_config(config.path(), &Config { worktrees_dir, ..Config::default() });
    (worktrees, config, config_path)
}

mod agent_status;
mod click_outside;
mod compact;
mod context_menu;
mod groups;
mod icon_bar;
mod keys;
mod lock_text;
mod merged_pull_requests;
mod muted_text;
mod picker;
mod plugins;
mod project_settings;
mod projects;
mod remove_worktree;
mod rename;
mod reorder;
mod resize_columns;
mod restore;
mod scrollback;
mod search;
mod search_sessions;
mod select_text;
mod settings;
mod shortcuts;
mod shows;
mod sidebar_clicks;
mod sidebar_scroll;
mod splits;
mod stacked_sidebar;
mod star_prompt;
mod start_flow_dialog;
mod tab_bar;
mod tabs;
mod tree_sidebar;
mod workspace_folds;
mod workspaces;

#[test]
fn resize_fits_terminals_to_the_pane() {
    let (mut app, _rx) = app();
    app.resize(Rect::new(0, 0, 100, 10));
    assert_eq!(
        term(&app, 0).emulator.size().expect("size"),
        (10, 100 - ui::SIDEBAR_WIDTH - ui::WORKSPACES_WIDTH - ui::PANE_PADDING)
    );
}

mod bugs;
mod changes_panel;
mod control_requests;
mod esc_closes_the_side_panel;
mod files_panel;
mod flow_end;
mod flow_panel;
mod flow_runs;
mod flows_dialog;
mod issue_asana;
mod issue_list;
mod path_links;
mod pull_requests;
mod queue;
mod remote_access;
mod restarting;
mod todo_panel;
mod trace;
mod updates;
mod usage_modal;
