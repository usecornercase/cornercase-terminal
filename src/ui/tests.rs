use ratatui::Terminal;
use ratatui::backend::TestBackend;
use rstest::rstest;

use super::*;
use crate::emulator::Emulator;
use crate::host_theme::HostTheme;

const W: u16 = 100;
const H: u16 = 18;
const AREA: Rect = Rect { x: 0, y: 0, width: W, height: H };
const SMALL: Rect = Rect { x: 0, y: 0, width: 80, height: 30 };

fn view(names: &[&str]) -> View<'static> {
    View {
        groups: Vec::new(),
        projects: names
            .iter()
            .map(|n| ProjectEntry { name: (*n).to_string(), workspaces: 1, group: None, status: None })
            .collect(),
        active: 0,
        has_project: false,
        workspaces: Vec::new(),
        active_workspace: 0,
        active_tab: None,
        issues: false,
        hover: None,
        widths: Widths::default(),
        sidebar: Sidebar::SideBySide,
        resizing: None,
        light: false,
        muted: Color::DarkGray,
        projects_scroll: 0,
        workspaces_scroll: 0,
        tab: None,
        overlay: None,
        toast: None,
        nav: None,
        update: None,
        remote: None,
        changes: None,
        changes_button: None,
        todo: None,
        todo_dot: true,
        todo_pending: false,
        files: None,
        attention: None,
        drag: None,
        tree: None,
        agents: None,
        counts: true,
        tab_bar: None,
    }
}

fn plain(projects: usize) -> Vec<SidebarRow> {
    sidebar_rows(&vec![None; projects], &[])
}

fn tabs(counts: &[usize]) -> TabLines {
    TabLines::from(counts.iter().map(|&n| vec![Details::default().lines(); n]).collect::<Vec<_>>())
}

fn corners_of_its_area(view: &View) -> (String, String) {
    let r = view.overlay.as_ref().and_then(|o| o.area(AREA)).expect("a dialog");
    let t = render(view);
    let cell = |x, y| t.backend().buffer()[(x, y)].symbol().to_string();
    (cell(r.x, r.y), cell(r.right() - 1, r.bottom() - 1))
}

fn render(view: &View) -> Terminal<TestBackend> {
    render_sized(view, W, H)
}

fn render_sized(view: &View, width: u16, height: u16) -> Terminal<TestBackend> {
    let mut t = Terminal::new(TestBackend::new(width, height)).expect("test backend");
    t.draw(|f| draw(f, view)).expect("draw");
    t
}

fn areas() -> Areas {
    layout(AREA, Widths::default())
}

fn row_text(t: &Terminal<TestBackend>, r: Rect) -> String {
    (r.x..r.right()).map(|x| t.backend().buffer()[(x, r.y)].symbol().to_string()).collect()
}

fn list() -> Rect {
    areas().list
}

fn screen(bytes: &[u8]) -> Snapshot {
    let mut emu = Emulator::new(
        H,
        W - SIDEBAR_WIDTH - WORKSPACES_WIDTH - PANE_PADDING,
        0,
        &HostTheme::default(),
        Box::new(|_| {}),
    )
    .expect("emulator");
    emu.feed(bytes);
    emu.snapshot().expect("snapshot")
}

fn single(screen: Snapshot) -> TabView {
    TabView {
        layout: Node::Leaf(0),
        screens: vec![screen],
        active: 0,
        dim_inactive: true,
        dragging: None,
        link: None,
        landing: None,
    }
}

fn close_x() -> u16 {
    list().right() - 2
}

mod agent_status;
mod agents_section;
mod borders;
mod buttons;
mod changes_layout;
mod compact;
mod dialogs;
mod display_path;
mod draw;
mod folder_name;
mod groups;
mod hover;
mod issues;
mod layout;
mod muted;
mod overlay;
mod picker;
mod reorder;
mod row_hover;
mod scrollback;
mod search;
mod settings;
mod sidebar_hit;
mod sidebar_scroll;
mod sidebar_setting;
mod splits;
mod stacked;
mod tab_bar_drawing;
mod tab_context;
mod tall_rows;
mod toast;
mod tree_drawing;
mod tree_drop;
mod tree_layout;
mod tree_rows;
mod truncate_left;
mod truncate_right;
mod widths;
mod workspaces_column;
