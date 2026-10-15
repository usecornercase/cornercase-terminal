use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::Span;

use super::View;
use super::drop::{Drag, Spot, landed};
use super::layout::Areas;
#[cfg(test)]
use super::rows::row_close_button;
use super::rows::{
    Band, GAP, RowButton, Rows, arrow, draw_hidden, draw_landing, draw_rail, marker, row_button, row_rect,
};
use super::sidebar::{NEW_BUTTON, SidebarRow, draw_group, draw_new_project, draw_project_band, sidebar_rows};
use super::widgets::{button_style, draw_button, draw_title};
use super::workspaces::{Guide, WorkspaceEntry, draw_guide, draw_tab_band, draw_workspace_band, pull_badge};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct TreeShape {
    pub(crate) groups: Vec<bool>,
    pub(crate) projects: Vec<ProjectShape>,
    pub(crate) tab_bar: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ProjectShape {
    pub(crate) group: Option<usize>,
    pub(crate) collapsed: bool,
    pub(crate) workspaces: Vec<WorkspaceShape>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct WorkspaceShape {
    pub(crate) collapsed: bool,
    pub(crate) tabs: Vec<u16>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TreeRow {
    Gap,
    Group(usize),
    Project(usize),
    Workspace(usize, usize),
    Tab(usize, usize, usize),
    NewTab(usize, usize),
    NewWorkspace(usize),
    Landing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TreeHit {
    Fold(TreeRow),
    Select(TreeRow),
    Close(TreeRow),
    Menu(TreeRow),
    NewTab(usize, usize),
    NewWorkspace(usize),
    NewProject,
}

impl TreeRow {
    fn workspace(self) -> Option<(usize, usize)> {
        match self {
            Self::Workspace(p, w) | Self::Tab(p, w, _) => Some((p, w)),
            _ => None,
        }
    }
}

impl TreeHit {
    pub(crate) fn workspace(self) -> Option<(usize, usize)> {
        match self {
            Self::Fold(row) | Self::Select(row) | Self::Close(row) | Self::Menu(row) => row.workspace(),
            Self::NewTab(p, w) => Some((p, w)),
            Self::NewWorkspace(_) | Self::NewProject => None,
        }
    }
}

pub(crate) fn tree_rows(shape: &TreeShape) -> Vec<TreeRow> {
    let groups: Vec<Option<usize>> = shape.projects.iter().map(|p| p.group).collect();
    let mut rows = Vec::new();
    for row in sidebar_rows(&groups, &shape.groups) {
        match row {
            SidebarRow::Gap => rows.push(TreeRow::Gap),
            SidebarRow::Group(g) => rows.push(TreeRow::Group(g)),
            SidebarRow::Project(p) => {
                rows.push(TreeRow::Project(p));
                let project = &shape.projects[p];
                if project.collapsed {
                    continue;
                }
                for (w, workspace) in project.workspaces.iter().enumerate() {
                    rows.push(TreeRow::Workspace(p, w));
                    if !workspace.collapsed && !shape.tab_bar {
                        rows.extend((0..workspace.tabs.len()).map(|t| TreeRow::Tab(p, w, t)));
                        rows.push(TreeRow::NewTab(p, w));
                    }
                }
                rows.push(TreeRow::NewWorkspace(p));
            }
            SidebarRow::Landing => {}
        }
    }
    rows
}

fn tree_height(shape: &TreeShape, row: TreeRow) -> u16 {
    match row {
        TreeRow::Gap | TreeRow::Landing => GAP,
        TreeRow::Tab(p, w, t) => {
            let lines = shape.projects.get(p).and_then(|p| p.workspaces.get(w)).and_then(|w| w.tabs.get(t));
            lines.copied().unwrap_or(1).max(1)
        }
        _ => 1,
    }
}

fn tree_depth(shape: &TreeShape, row: TreeRow) -> u16 {
    let project = |p: usize| u16::from(shape.projects.get(p).is_some_and(|p| p.group.is_some()));
    match row {
        TreeRow::Gap | TreeRow::Landing | TreeRow::Group(_) => 0,
        TreeRow::Project(p) => project(p),
        TreeRow::Workspace(p, _) | TreeRow::NewWorkspace(p) => project(p) + 1,
        TreeRow::Tab(p, ..) | TreeRow::NewTab(p, _) => project(p) + 2,
    }
}

fn tree_indent(shape: &TreeShape, row: TreeRow) -> u16 {
    2 + 2 * tree_depth(shape, row)
}

fn arrow_in(r: Rect, shape: &TreeShape, row: TreeRow) -> Rect {
    match row {
        TreeRow::Workspace(..) if shape.tab_bar => Rect::default(),
        TreeRow::Group(_) | TreeRow::Project(_) | TreeRow::Workspace(..) => {
            Rect { x: r.x.saturating_add(tree_indent(shape, row)), width: 2, height: 1, ..r }.intersection(r)
        }
        _ => Rect::default(),
    }
}

pub(crate) fn tree_layout_rows(list: Rect, shape: &TreeShape, rows: &[TreeRow], scroll: usize) -> Rows {
    Rows { list, heights: rows.iter().map(|r| tree_height(shape, *r)).collect(), button: 1, scroll }
}

pub(crate) fn tree_row(list: Rect, shape: &TreeShape, scroll: usize, row: TreeRow) -> Rect {
    let rows = tree_rows(shape);
    row_rect(&tree_layout_rows(list, shape, &rows, scroll), &rows, &row)
}

#[cfg(test)]
pub(crate) fn tree_arrow(list: Rect, shape: &TreeShape, scroll: usize, row: TreeRow) -> Rect {
    arrow_in(tree_row(list, shape, scroll, row), shape, row)
}

#[cfg(test)]
pub(crate) fn tree_close(list: Rect, shape: &TreeShape, scroll: usize, row: TreeRow) -> Rect {
    row_close_button(tree_row(list, shape, scroll, row), 1)
}

pub(crate) fn tree_active_row(
    rows: &[TreeRow],
    shape: &TreeShape,
    active: (usize, usize, Option<usize>),
) -> Option<usize> {
    let (p, w, t) = active;
    let find = |row: TreeRow| rows.iter().position(|r| *r == row);
    t.and_then(|t| find(TreeRow::Tab(p, w, t)))
        .or_else(|| find(TreeRow::Workspace(p, w)))
        .or_else(|| find(TreeRow::Project(p)))
        .or_else(|| find(TreeRow::Group(shape.projects.get(p)?.group?)))
}

pub(crate) fn tree_hit(list: Rect, shape: &TreeShape, scroll: usize, pos: Position) -> Option<TreeHit> {
    if !list.contains(pos) {
        return None;
    }
    let rows = tree_rows(shape);
    let layout = tree_layout_rows(list, shape, &rows, scroll);
    if layout.button().contains(pos) {
        return Some(TreeHit::NewProject);
    }
    let i = layout.at(pos)?;
    let (r, row) = (layout.item(i), rows[i]);
    let button = row_button(r, 1, pos);
    Some(match row {
        TreeRow::Gap | TreeRow::Landing => return None,
        TreeRow::NewTab(p, w) => TreeHit::NewTab(p, w),
        TreeRow::NewWorkspace(p) => TreeHit::NewWorkspace(p),
        _ if button == Some(RowButton::Close) => TreeHit::Close(row),
        _ if button == Some(RowButton::Menu) => TreeHit::Menu(row),
        TreeRow::Group(_) => TreeHit::Fold(row),
        _ if arrow_in(r, shape, row).contains(pos) => TreeHit::Fold(row),
        _ => TreeHit::Select(row),
    })
}

pub(crate) fn tree_pull(
    list: Rect,
    shape: &TreeShape,
    scroll: usize,
    (p, w): (usize, usize),
    entry: &WorkspaceEntry,
) -> Option<Rect> {
    let row = TreeRow::Workspace(p, w);
    let lead = usize::from(tree_indent(shape, row)) + 2;
    pull_badge(tree_row(list, shape, scroll, row), 1, lead, entry, shape.tab_bar || folded(shape, row))
}

pub(crate) struct TreeView {
    pub(crate) shape: TreeShape,
    pub(crate) workspaces: Vec<Vec<WorkspaceEntry>>,
}

fn tree_landing_indent(shape: &TreeShape, dragged: TreeRow, spot: Spot) -> u16 {
    match dragged {
        TreeRow::Workspace(..) | TreeRow::Tab(..) => tree_indent(shape, dragged),
        _ => spot.indent(),
    }
}

fn folded(shape: &TreeShape, row: TreeRow) -> bool {
    let project = |p: usize| shape.projects.get(p);
    match row {
        TreeRow::Project(p) => project(p).is_some_and(|p| p.collapsed),
        TreeRow::Workspace(p, w) => project(p).and_then(|p| p.workspaces.get(w)).is_some_and(|w| w.collapsed),
        _ => false,
    }
}

fn tree_guide(r: Rect, shape: &TreeShape, row: TreeRow) -> u16 {
    r.x.saturating_add(tree_indent(shape, row).saturating_sub(2))
}

fn tree_among_tabs(rows: &[TreeRow], i: usize) -> Option<TreeRow> {
    let above = i.checked_sub(1).and_then(|j| rows.get(j)).and_then(|row| row.workspace());
    let below = rows.get(i + 1).and_then(|row| match row {
        TreeRow::Tab(p, w, _) | TreeRow::NewTab(p, w) => Some((*p, *w)),
        _ => None,
    });
    above.filter(|_| above == below).map(|(p, w)| TreeRow::Tab(p, w, 0))
}

pub(super) fn draw_tree(f: &mut Frame, view: &View, areas: &Areas) {
    draw_title(f, view.muted, areas.title, "Projects");
    let Some(tree) = &view.tree else { return };
    let shape = &tree.shape;
    let rows = tree_rows(shape);
    let base = tree_layout_rows(areas.list, shape, &rows, view.projects_scroll);
    let (dragged, landing) = match view.drag {
        Some(Drag::Tree(row, landing)) => (Some(row), landing),
        _ => (None, None),
    };
    let indent = dragged.zip(landing).map(|(row, l)| tree_landing_indent(shape, row, l.spot));
    let (rows, line) = landed(rows, &base, landing, TreeRow::Gap, TreeRow::Landing);
    let layout = tree_layout_rows(areas.list, shape, &rows, base.first());
    let active = (view.active, view.active_workspace, view.active_tab);
    let marked = tree_active_row(&rows, shape, active).filter(|_| view.has_project);
    let (above, below) = layout.hidden();
    let workspace = |p: usize, w: usize| tree.workspaces.get(p).and_then(|ws| ws.get(w));
    for (i, &row) in rows.iter().enumerate().take(below.start).skip(above.end) {
        let (r, mark) = (layout.item(i), marked == Some(i));
        let bg = view.row_background(r, mark || view.dragging_tree_row(row));
        let lead = " ".repeat(usize::from(tree_indent(shape, row) - 2));
        let band = |lead: Vec<Span<'static>>| Band { r, pitch: 1, lead, bg };
        match row {
            TreeRow::Gap => {}
            TreeRow::Landing => {
                draw_landing(f, r, indent);
                if let Some(tab) = tree_among_tabs(&rows, i) {
                    draw_guide(f, view.line(), tree_guide(r, shape, tab), r, Guide::Bar);
                }
            }
            TreeRow::Group(g) => draw_group(f, view, g, band(vec![marker(mark)])),
            TreeRow::Project(p) => {
                let band = band(vec![marker(mark), Span::raw(lead), arrow(view.muted, folded(shape, row))]);
                draw_project_band(f, view, band, p, folded(shape, row));
            }
            TreeRow::Workspace(p, w) => {
                let Some(entry) = workspace(p, w) else { continue };
                let fold = if shape.tab_bar { Span::raw("  ") } else { arrow(view.muted, folded(shape, row)) };
                let band = band(vec![marker(mark), Span::raw(lead), fold]);
                let active = p == view.active && w == view.active_workspace;
                draw_workspace_band(f, view, band, entry, active, shape.tab_bar || folded(shape, row));
            }
            TreeRow::Tab(p, w, tab) => {
                let Some(entry) = workspace(p, w).and_then(|w| w.tabs.get(tab)) else { continue };
                draw_tab_band(f, view, band(vec![marker(mark), Span::raw(lead)]), entry, mark);
                draw_guide(f, view.line(), tree_guide(r, shape, row), r, Guide::Tee);
            }
            TreeRow::NewTab(p, w) => {
                if !workspace(p, w).is_some_and(|entry| entry.removing) {
                    let style = button_style(view, r, Style::default().fg(view.muted), Color::Cyan);
                    draw_button(f, r, &format!("{lead} "), "+ Tab", style);
                }
                draw_guide(f, view.line(), tree_guide(r, shape, row), r, Guide::End);
            }
            TreeRow::NewWorkspace(_) => {
                let style = button_style(view, r, Style::default().fg(Color::Cyan), Color::Cyan);
                draw_button(f, r, &format!("{lead} "), NEW_BUTTON, style);
            }
        }
        if mark {
            draw_rail(f, r);
        }
    }
    let named = |r| matches!(r, TreeRow::Group(_) | TreeRow::Project(_) | TreeRow::Workspace(..) | TreeRow::Tab(..));
    draw_hidden(f, view.muted, &layout, &rows, named);
    draw_new_project(f, view, layout.button());
    draw_landing(f, line, indent);
}
