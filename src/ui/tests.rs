use std::collections::HashMap;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::text::{Line, Span};
use rstest::rstest;

use super::agents_section::draw_agent;
use super::chrome::{ISSUES_LABEL, SEARCH_ICON, SEARCH_PLACEHOLDER};
use super::dialog::form::form_rows;
use super::dialog::group_style::ICON_CELL;
use super::dialog::issues::issues_rows;
use super::dialog::picker::picker_rows;
use super::dialog::settings::settings_area;
use super::dialog::usage::usage_rows;
use super::dialog::worktrees::list_rows;
use super::dialog::{INPUT_PROMPT, list_item, list_item_by, tabs_in};
use super::drop::landed;
use super::layout::{COMPACT_BUTTON_WIDTH, HEADER_HEIGHT};
use super::pane::TAGLINE;
use super::rows::{Band, CLOSE_BUTTON_WIDTH, COMPACT_CLOSE_WIDTH, MENU_BUTTON_WIDTH, MIN_MENU_ROW_WIDTH};
use super::theme::{DARK_HOVER, DARK_LINE, DARK_SURFACE, LIGHT_HOVER, LIGHT_SURFACE, WAITING_COLOR};
use super::workspaces::{GB, MB, draw_tab_band, memory_size};
use super::*;
use crate::agents::usage::Severity;
use crate::host::theme::HostTheme;
use crate::project::split::{Node, Place};
use crate::term::emulator::Emulator;
use crate::term::emulator::Snapshot;

const W: u16 = 100;
const H: u16 = 18;
const AREA: Rect = Rect { x: 0, y: 0, width: W, height: H };
const SMALL: Rect = Rect { x: 0, y: 0, width: 80, height: 30 };

fn view(names: &[&str]) -> View<'static> {
    View {
        projects: names
            .iter()
            .map(|n| ProjectEntry { name: (*n).to_string(), workspaces: 1, group: None, status: None })
            .collect(),
        sidebar: Sidebar::SideBySide,
        muted: Color::DarkGray,
        soft: Color::Gray,
        todo_dot: true,
        counts: true,
        ..View::default()
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
        bands: Vec::new(),
        footer: false,
        accounts: Vec::new(),
    }
}

fn close_x() -> u16 {
    list().right() - 2
}

mod accounts_bar;
mod activity;
mod agent_status;
mod agents_section;
mod borders;
mod buttons;
mod changes_layout;
mod combobox;
mod compact;
mod dialogs;
mod draw;
mod groups;
mod hover;
mod issues;
mod layout;
mod muted;
mod overlay;
mod pane_footer;
mod picker;
mod plugin_buttons;
mod queue;
mod reorder;
mod resuming;
mod row_hover;
mod scrollback;
mod search;
mod settings;
mod share_settings;
mod sidebar_hit;
mod sidebar_scroll;
mod splits;
mod stacked;
mod star;
mod tab_bar_drawing;
mod tab_context;
mod tall_rows;
mod toast;
mod tour;
mod tree_drawing;
mod tree_drop;
mod tree_layout;
mod tree_rows;
mod widths;
mod workspace_folds;
mod workspaces_column;
mod worktrees_dialog;
