use std::ops::Range;

use ratatui::layout::{Position, Rect};

use super::rows::{Rows, Zone, more_above};
use super::sidebar::SidebarRow;
use super::tree::{TreeRow, TreeShape, tree_layout_rows, tree_rows};
use super::workspaces::{TabLines, WorkspaceRow, workspace_rows};

pub(crate) fn tree_drop(
    list: Rect,
    shape: &TreeShape,
    scroll: usize,
    dragged: TreeRow,
    pos: Position,
) -> Option<Landing> {
    let rows = tree_rows(shape);
    let layout = tree_layout_rows(list, shape, &rows, scroll);
    let zone = layout.zone(pos)?;
    match dragged {
        TreeRow::Group(_) => {
            let header = |r| if let TreeRow::Group(g) = r { Some(g) } else { None };
            let (at, g) = block_drop(&rows, Over { layout: &layout, zone }, dragged, TreeRow::Gap, header)?;
            Some(Landing { at, spot: Spot::Group(g) })
        }
        TreeRow::Project(_) => tree_project_drop(&rows, &layout, zone, rows.iter().position(|r| *r == dragged)?),
        TreeRow::Workspace(p, w) => {
            let ours = shape.projects.get(p)?.workspaces.iter().filter(|w| !w.machine).count();
            tree_workspace_drop(&rows, &layout, zone, (p, ours), w)
        }
        TreeRow::Tab(p, w, t) => {
            let tab = |r| if let TreeRow::Tab(q, v, u) = r { (q == p && v == w).then_some(u) } else { None };
            sibling_drop(
                &rows,
                Over { layout: &layout, zone },
                (TreeRow::Workspace(p, w), TreeRow::NewTab(p, w)),
                tab,
                t,
            )
        }
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Spot {
    Group(usize),
    Project { group: Option<usize>, before: Option<usize> },
    Workspace(usize),
    Tab(usize),
}

impl Spot {
    pub(super) fn indent(self) -> u16 {
        match self {
            Self::Project { group: Some(_), .. } | Self::Tab(_) => 4,
            Self::Group(_) | Self::Project { group: None, .. } | Self::Workspace(_) => 2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Landing {
    pub(crate) at: usize,
    pub(crate) spot: Spot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Drag {
    Sidebar(SidebarRow, Option<Landing>),
    Workspaces(WorkspaceRow, Option<Landing>),
    Tree(TreeRow, Option<Landing>),
    Bar(usize, Option<usize>),
    Todo(u64),
}

trait ListRow: Copy + PartialEq {
    const GAP: Self;
    fn group(self) -> Option<usize>;
    fn project(self) -> Option<usize>;
}

impl ListRow for SidebarRow {
    const GAP: Self = Self::Gap;

    fn group(self) -> Option<usize> {
        if let Self::Group(g) = self { Some(g) } else { None }
    }

    fn project(self) -> Option<usize> {
        if let Self::Project(p) = self { Some(p) } else { None }
    }
}

impl ListRow for TreeRow {
    const GAP: Self = Self::Gap;

    fn group(self) -> Option<usize> {
        if let Self::Group(g) = self { Some(g) } else { None }
    }

    fn project(self) -> Option<usize> {
        if let Self::Project(p) = self { Some(p) } else { None }
    }
}

fn group_of<R: ListRow>(rows: &[R], i: usize) -> Option<usize> {
    rows[..=i].iter().rev().find_map(|r| r.group())
}

fn project_spot<R: ListRow>(rows: &[R], dragged: Range<usize>, at: usize) -> Spot {
    if let Some(j) = (at..rows.len()).find(|j| !dragged.contains(j))
        && let Some(q) = rows[j].project()
    {
        return Spot::Project { group: group_of(rows, j), before: Some(q) };
    }
    let group = match (0..at).rev().find(|k| !dragged.contains(k)) {
        Some(k) if rows[k] == R::GAP => group_of(rows, k - 1),
        Some(k) => group_of(rows, k),
        None => None,
    };
    Spot::Project { group, before: None }
}

#[derive(Clone, Copy)]
struct Over<'a> {
    layout: &'a Rows,
    zone: Zone,
}

impl Over<'_> {
    fn boundary(self, place: impl Fn(usize) -> Option<usize>) -> Option<usize> {
        self.layout.boundary(self.zone, place)
    }
}

fn project_drop(rows: &[SidebarRow], over: Over, dragged: usize) -> Option<Landing> {
    let at = over.boundary(|i| match rows[i] {
        _ if i == dragged => None,
        SidebarRow::Project(_) if i < dragged => Some(i),
        SidebarRow::Project(_) | SidebarRow::Group(_) => Some(i + 1),
        SidebarRow::Gap | SidebarRow::Landing => Some(i),
    })?;
    Some(Landing { at, spot: project_spot(rows, dragged..dragged + 1, at) })
}

fn block_drop<R: Copy + PartialEq>(
    rows: &[R],
    over: Over,
    dragged: R,
    gap: R,
    header: impl Fn(R) -> Option<usize>,
) -> Option<(usize, usize)> {
    let d = rows.iter().position(|r| *r == dragged)?;
    let len = rows.len();
    let is_header = |j: usize| header(rows[j]).is_some();
    let block = |i: usize| (0..=i).rev().take_while(|&j| rows[j] != gap).find(|&j| is_header(j));
    let at = over.boundary(|i| match block(i) {
        Some(h) if h == d => None,
        Some(h) if h < d => Some(h),
        Some(h) => Some((h..len).find(|&j| rows[j] == gap).unwrap_or(len)),
        None => Some((i..len).find(|&j| is_header(j)).unwrap_or(len)),
    })?;
    let count = rows.iter().filter(|r| header(**r).is_some()).count();
    Some((at, rows[at..].iter().find_map(|r| header(*r)).unwrap_or(count)))
}

fn sibling_drop<R: Copy + PartialEq>(
    rows: &[R],
    over: Over,
    (header, end): (R, R),
    tab: impl Fn(R) -> Option<usize>,
    t: usize,
) -> Option<Landing> {
    let header = rows.iter().position(|r| *r == header)?;
    let end = rows.iter().position(|r| *r == end)?;
    let at = over.boundary(|i| match tab(rows[i]) {
        Some(u) if u == t => None,
        Some(u) if u < t => Some(i),
        Some(_) => Some(i + 1),
        None if i <= header => Some(header + 1),
        None => Some(end),
    })?;
    let at = at.clamp(header + 1, end);
    Some(Landing { at, spot: Spot::Tab(at - header - 1) })
}

fn tab_drop(rows: &[WorkspaceRow], over: Over, w: usize, t: usize) -> Option<Landing> {
    let tab = |r| if let WorkspaceRow::Tab(v, u) = r { (v == w).then_some(u) } else { None };
    sibling_drop(rows, over, (WorkspaceRow::Workspace(w), WorkspaceRow::NewTab(w)), tab, t)
}

fn inside(parent: TreeRow, row: TreeRow) -> bool {
    match (parent, row) {
        (
            TreeRow::Project(p),
            TreeRow::Workspace(q, _) | TreeRow::Tab(q, ..) | TreeRow::NewTab(q, _) | TreeRow::NewWorkspace(q),
        ) => p == q,
        (TreeRow::Workspace(p, w), TreeRow::Tab(q, v, _) | TreeRow::NewTab(q, v)) => p == q && w == v,
        _ => false,
    }
}

fn block_end(rows: &[TreeRow], i: usize) -> usize {
    (i + 1..rows.len()).find(|&j| !inside(rows[i], rows[j])).unwrap_or(rows.len())
}

fn owner(rows: &[TreeRow], i: usize, is_owner: impl Fn(TreeRow) -> bool) -> usize {
    (0..=i).rev().find(|&j| is_owner(rows[j]) && (j == i || inside(rows[j], rows[i]))).unwrap_or(i)
}

fn block_drop_at(
    rows: &[TreeRow],
    layout: &Rows,
    zone: Zone,
    d: usize,
    is_owner: impl Fn(TreeRow) -> bool,
) -> Option<usize> {
    let dragged = d..block_end(rows, d);
    let at = layout.boundary(zone, |i| {
        let o = owner(rows, i, &is_owner);
        match rows[o] {
            _ if dragged.contains(&o) => None,
            row if is_owner(row) && o < d => Some(o),
            row if is_owner(row) => Some(block_end(rows, o)),
            TreeRow::Group(_) => Some(o + 1),
            _ => Some(o),
        }
    })?;
    let o = owner(rows, at.min(rows.len().saturating_sub(1)), &is_owner);
    Some(if at < rows.len() && o < at && is_owner(rows[o]) { block_end(rows, o) } else { at })
}

fn tree_project_drop(rows: &[TreeRow], layout: &Rows, zone: Zone, d: usize) -> Option<Landing> {
    let at = block_drop_at(rows, layout, zone, d, |r| matches!(r, TreeRow::Project(_)))?;
    Some(Landing { at, spot: project_spot(rows, d..block_end(rows, d), at) })
}

fn tree_workspace_drop(
    rows: &[TreeRow],
    layout: &Rows,
    zone: Zone,
    (p, ours): (usize, usize),
    moved: usize,
) -> Option<Landing> {
    let header = rows.iter().position(|r| *r == TreeRow::Project(p))?;
    let theirs = |r| matches!(r, TreeRow::Workspace(q, v) if q == p && v >= ours);
    let end = rows.iter().position(|&r| r == TreeRow::NewWorkspace(p) || theirs(r))?;
    let from = rows.iter().position(|r| *r == TreeRow::Workspace(p, moved))?;
    let is_workspace = |r| matches!(r, TreeRow::Workspace(q, v) if q == p && v < ours);
    let at = block_drop_at(rows, layout, zone, from, is_workspace)?.clamp(header + 1, end);
    let index = |r: &TreeRow| if let TreeRow::Workspace(q, v) = *r { (q == p).then_some(v) } else { None };
    let before =
        rows[at..end].iter().find_map(index).unwrap_or_else(|| rows[header..end].iter().filter_map(index).count());
    Some(Landing { at, spot: Spot::Workspace(before) })
}

pub(crate) fn sidebar_drop(layout: &Rows, rows: &[SidebarRow], dragged: SidebarRow, pos: Position) -> Option<Landing> {
    let over = Over { layout, zone: layout.zone(pos)? };
    match dragged {
        SidebarRow::Project(_) => project_drop(rows, over, rows.iter().position(|r| *r == dragged)?),
        SidebarRow::Group(_) => {
            let header = |r| if let SidebarRow::Group(g) = r { Some(g) } else { None };
            let (at, g) = block_drop(rows, over, dragged, SidebarRow::Gap, header)?;
            Some(Landing { at, spot: Spot::Group(g) })
        }
        SidebarRow::Gap | SidebarRow::Landing => None,
    }
}

pub(crate) fn workspace_drop(layout: &Rows, tabs: &TabLines, dragged: WorkspaceRow, pos: Position) -> Option<Landing> {
    let over = Over { layout, zone: layout.zone(pos)? };
    let rows = workspace_rows(tabs);
    match dragged {
        WorkspaceRow::Workspace(_) => {
            let header = |r| if let WorkspaceRow::Workspace(w) = r { Some(w) } else { None };
            let (at, w) = block_drop(&rows, over, dragged, WorkspaceRow::Gap, header)?;
            Some(Landing { at, spot: Spot::Workspace(w) })
        }
        WorkspaceRow::Tab(w, t) => tab_drop(&rows, over, w, t),
        WorkspaceRow::Gap | WorkspaceRow::NewTab(_) | WorkspaceRow::Landing => None,
    }
}

pub(super) fn landed<R: Copy + PartialEq>(
    rows: Vec<R>,
    layout: &Rows,
    landing: Option<Landing>,
    gap: R,
    mark: R,
) -> (Vec<R>, Rect) {
    let (first, end) = (layout.first(), layout.end());
    let Some(at) = landing.map(|l| l.at).filter(|at| (first..=end).contains(at)) else {
        return (rows, Rect::default());
    };
    let mut drawn = rows;
    let beside = [at.checked_sub(1).filter(|&g| g >= first), Some(at).filter(|&g| g < end)];
    if let Some(g) = beside.into_iter().flatten().find(|&g| drawn[g] == gap) {
        drawn[g] = mark;
        return (drawn, Rect::default());
    }
    if at == first {
        return (drawn, more_above(layout.list));
    }
    if at == end {
        let last = layout.item(end - 1);
        return (drawn, Rect { y: last.bottom(), height: 1, ..last });
    }
    drawn.insert(at, mark);
    (drawn, Rect::default())
}
