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
use super::sidebar::{
    NEW_BUTTON, SidebarRow, draw_group, draw_new_project, draw_project_band, machine_rows, sidebar_rows,
};
use super::widgets::{button_style, draw_button, draw_title};
use super::workspaces::{
    Badge, Guide, Tag, WorkspaceEntry, WorkspaceLine, draw_guide, draw_tab_band, draw_workspace_band, tag_badge,
};

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
    pub(crate) machine: bool,
    pub(crate) workspaces: Vec<WorkspaceShape>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct WorkspaceShape {
    pub(crate) collapsed: bool,
    pub(crate) machine: bool,
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
    let groups: Vec<Option<usize>> = shape.projects.iter().filter(|p| !p.machine).map(|p| p.group).collect();
    let mut rows = Vec::new();
    for row in machine_rows(sidebar_rows(&groups, &shape.groups), groups.len()..shape.projects.len()) {
        match row {
            SidebarRow::Gap => rows.push(TreeRow::Gap),
            SidebarRow::Group(g) => rows.push(TreeRow::Group(g)),
            SidebarRow::Project(p) => push_project(shape, p, &mut rows),
            SidebarRow::Landing => {}
        }
    }
    rows
}

fn push_project(shape: &TreeShape, p: usize, rows: &mut Vec<TreeRow>) {
    rows.push(TreeRow::Project(p));
    let project = &shape.projects[p];
    if project.collapsed {
        return;
    }
    for (w, workspace) in project.workspaces.iter().enumerate() {
        rows.push(TreeRow::Workspace(p, w));
        if !workspace.collapsed && !shape.tab_bar {
            rows.extend((0..workspace.tabs.len()).map(|t| TreeRow::Tab(p, w, t)));
            rows.extend((!workspace.machine).then_some(TreeRow::NewTab(p, w)));
        }
    }
    rows.extend((!project.machine).then_some(TreeRow::NewWorkspace(p)));
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

fn badge_of(shape: &TreeShape, row: TreeRow) -> Badge {
    match (shape.tab_bar, folded(shape, row)) {
        (true, _) => Badge::Attention,
        (false, true) => Badge::Highest,
        (false, false) => Badge::Off,
    }
}

fn arrow_in(r: Rect, shape: &TreeShape, row: TreeRow) -> Rect {
    match row {
        TreeRow::Workspace(..) if shape.tab_bar => Rect::default(),
        TreeRow::Project(p) if theirs(shape, p) => Rect::default(),
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

pub(crate) fn tree_tag(
    list: Rect,
    shape: &TreeShape,
    scroll: usize,
    (p, w): (usize, usize),
    (entry, tag): (&WorkspaceEntry, Tag),
) -> Option<Rect> {
    let row = TreeRow::Workspace(p, w);
    let lead = usize::from(tree_indent(shape, row)) + 2;
    tag_badge(tree_row(list, shape, scroll, row), 1, lead, (entry, badge_of(shape, row)), tag)
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

fn theirs(shape: &TreeShape, p: usize) -> bool {
    shape.projects.get(p).is_some_and(|p| p.machine)
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

struct TreePaint<'a> {
    view: &'a View<'a>,
    tree: &'a TreeView,
    rows: &'a [TreeRow],
    indent: Option<u16>,
}

impl TreePaint<'_> {
    fn workspace(&self, p: usize, w: usize) -> Option<&WorkspaceEntry> {
        self.tree.workspaces.get(p).and_then(|ws| ws.get(w))
    }

    fn row(&self, f: &mut Frame, index: usize, r: Rect, mark: bool) -> Option<()> {
        let (view, shape, row) = (self.view, &self.tree.shape, self.rows[index]);
        let bg = view.row_background(r, mark || view.dragging_tree_row(row));
        let lead = " ".repeat(usize::from(tree_indent(shape, row) - 2));
        let band = |lead: Vec<Span<'static>>| Band { r, pitch: 1, lead, bg };
        match row {
            TreeRow::Gap => {}
            TreeRow::Landing => {
                draw_landing(f, r, self.indent);
                if let Some(tab) = tree_among_tabs(self.rows, index) {
                    draw_guide(f, view.line(), tree_guide(r, shape, tab), r, Guide::Bar);
                }
            }
            TreeRow::Group(g) => draw_group(f, view, g, band(vec![marker(mark)])),
            TreeRow::Project(project) => {
                let fold = if theirs(shape, project) { Span::raw("  ") } else { arrow(view.muted, folded(shape, row)) };
                let band = band(vec![marker(mark), Span::raw(lead), fold]);
                draw_project_band(f, view, band, project, folded(shape, row));
            }
            TreeRow::Workspace(project, workspace) => {
                let entry = self.workspace(project, workspace)?;
                let fold = if shape.tab_bar { Span::raw("  ") } else { arrow(view.muted, folded(shape, row)) };
                let band = band(vec![marker(mark), Span::raw(lead), fold]);
                let active = project == view.active && workspace == view.active_workspace;
                draw_workspace_band(f, view, band, WorkspaceLine { entry, active, badge: badge_of(shape, row) });
            }
            TreeRow::Tab(project, workspace, tab) => {
                let workspace = self.workspace(project, workspace)?;
                let entry = (workspace.tabs.get(tab)?, workspace.machine.as_ref());
                draw_tab_band(f, view, band(vec![marker(mark), Span::raw(lead)]), entry, mark);
                let last = workspace.machine.is_some() && tab + 1 == workspace.tabs.len();
                draw_guide(f, view.line(), tree_guide(r, shape, row), r, if last { Guide::End } else { Guide::Tee });
            }
            TreeRow::NewTab(..) | TreeRow::NewWorkspace(_) => self.button(f, row, r, &lead),
        }
        Some(())
    }

    fn button(&self, f: &mut Frame, row: TreeRow, r: Rect, lead: &str) {
        let (view, shape) = (self.view, &self.tree.shape);
        if let TreeRow::NewTab(project, workspace) = row {
            if !self.workspace(project, workspace).is_some_and(|entry| entry.removing) {
                let style = button_style(view, r, Style::default().fg(view.muted), Color::Cyan);
                draw_button(f, r, &format!("{lead} "), "+ Tab", style);
            }
            draw_guide(f, view.line(), tree_guide(r, shape, row), r, Guide::End);
        } else {
            let style = button_style(view, r, Style::default().fg(Color::Cyan), Color::Cyan);
            draw_button(f, r, &format!("{lead} "), NEW_BUTTON, style);
        }
    }
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
    let marked = tree_active_row(&rows, shape, active).filter(|_| view.marked_project().is_some());
    let (above, below) = layout.hidden();
    let paint = TreePaint { view, tree, rows: &rows, indent };
    for i in (0..rows.len()).take(below.start).skip(above.end) {
        let (r, mark) = (layout.item(i), marked == Some(i));
        if paint.row(f, i, r, mark).is_some() && mark {
            draw_rail(f, r);
        }
    }
    let named = |r| matches!(r, TreeRow::Group(_) | TreeRow::Project(_) | TreeRow::Workspace(..) | TreeRow::Tab(..));
    draw_hidden(f, view.muted, &layout, &rows, named);
    draw_new_project(f, view, layout.button());
    draw_landing(f, line, indent);
}
