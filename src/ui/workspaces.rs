use std::cmp::Ordering;

use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::View;
use super::chrome::draw_back;
use super::drop::landed;
use super::layout::Areas;
use super::rows::{
    Band, GAP, RowButton, Rows, Tags, arrow, draw_hidden, draw_landing, draw_rail, draw_row_buttons, marker, room,
    row_button, row_close_button, row_menu_button, row_rect,
};
use super::sidebar::NEW_BUTTON;
use super::theme::{pull_tag, severity_color, stack_tag, status_icon};
use super::widgets::{button_style, dim, draw_band, draw_button, draw_title, middle, sidebar_hovered};
use crate::agents::activity::{self, Status};
use crate::agents::usage::Severity;
use crate::pull_request::Mark;
use crate::text::{cells, clip_cells, truncate_cells, truncate_right};

mod machine;

pub(crate) use machine::MachineTag;
use machine::seen_note;
pub(super) use machine::{dim_row, machine_span, spin};

const SHELL_ICON: &str = "›";

const PROGRAM_ICON: &str = "▸";

pub(crate) const ICON_WIDTH: usize = 2;

const BEHIND_ICON: &str = "↓";
const SHARED_ICON: &str = "⇄";

pub(crate) const REOPEN_ICON: &str = "▶";

const REMOVING_LABEL: &str = "removing…";
const SETTING_UP_LABEL: &str = "setting up…";

const CONTEXT_SEPARATOR: &str = " · ";

const MIN_MODEL_WIDTH: usize = 4;
const MIN_ACCOUNT_WIDTH: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceRow {
    Gap,
    Workspace(usize),
    Tab(usize, usize),
    NewTab(usize),
    Landing,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TabLines {
    lines: Vec<Vec<u16>>,
    listed: bool,
    folded: Vec<bool>,
    bare: Vec<bool>,
}

impl TabLines {
    pub(crate) fn new(lines: Vec<Vec<u16>>, listed: bool) -> Self {
        let lines = if listed { lines } else { vec![Vec::new(); lines.len()] };
        Self { lines, listed, folded: Vec::new(), bare: Vec::new() }
    }

    pub(crate) fn folded(self, folded: Vec<bool>) -> Self {
        Self { folded, ..self }
    }

    pub(crate) fn bare(self, bare: Vec<bool>) -> Self {
        Self { bare, ..self }
    }

    fn adds_tabs(&self, w: usize) -> bool {
        self.listed && self.bare.get(w) != Some(&true)
    }

    pub(crate) fn is_folded(&self, w: usize) -> bool {
        self.listed && self.folded.get(w) == Some(&true)
    }
}

impl From<Vec<Vec<u16>>> for TabLines {
    fn from(lines: Vec<Vec<u16>>) -> Self {
        Self::new(lines, true)
    }
}

pub(crate) fn workspace_rows(tabs: &TabLines) -> Vec<WorkspaceRow> {
    tabs.lines
        .iter()
        .enumerate()
        .flat_map(|(w, lines)| {
            let open = !tabs.is_folded(w);
            (w > 0)
                .then_some(WorkspaceRow::Gap)
                .into_iter()
                .chain(std::iter::once(WorkspaceRow::Workspace(w)))
                .chain((0..lines.len()).filter(move |_| open).map(move |t| WorkspaceRow::Tab(w, t)))
                .chain((tabs.adds_tabs(w) && open).then_some(WorkspaceRow::NewTab(w)))
        })
        .collect()
}

fn workspace_rows_layout(list: Rect, pitch: u16, rows: &[WorkspaceRow], tabs: &TabLines, scroll: usize) -> Rows {
    let heights = rows
        .iter()
        .map(|row| match *row {
            WorkspaceRow::Gap | WorkspaceRow::Landing => GAP,
            WorkspaceRow::Tab(w, t) => pitch.max(tabs.lines[w][t]),
            WorkspaceRow::Workspace(_) | WorkspaceRow::NewTab(_) => pitch,
        })
        .collect();
    Rows { list, heights, button: pitch, scroll }
}

pub(crate) fn workspace_layout(list: Rect, pitch: u16, tabs: &TabLines, scroll: usize) -> Rows {
    workspace_rows_layout(list, pitch, &workspace_rows(tabs), tabs, scroll)
}

pub fn new_workspace_button(list: Rect, pitch: u16, tabs: &TabLines) -> Rect {
    workspace_layout(list, pitch, tabs, 0).button()
}

pub fn workspace_row(list: Rect, pitch: u16, tabs: &TabLines, scroll: usize, row: WorkspaceRow) -> Rect {
    row_rect(&workspace_layout(list, pitch, tabs, scroll), &workspace_rows(tabs), &row)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorkspaceHit {
    Workspace(usize),
    CloseWorkspace(usize),
    WorkspaceMenu(usize),
    Fold(usize),
    Tab(usize, usize),
    CloseTab(usize, usize),
    TabMenu(usize, usize),
    NewTab(usize),
    NewWorkspace,
}

impl WorkspaceHit {
    pub(crate) fn workspace(self) -> Option<usize> {
        match self {
            Self::Workspace(w)
            | Self::CloseWorkspace(w)
            | Self::WorkspaceMenu(w)
            | Self::Fold(w)
            | Self::Tab(w, _)
            | Self::CloseTab(w, _)
            | Self::TabMenu(w, _)
            | Self::NewTab(w) => Some(w),
            Self::NewWorkspace => None,
        }
    }
}

pub(crate) fn fold_button(row: Rect) -> Rect {
    Rect { width: 4, ..row }.intersection(row)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Badge {
    Off,
    Attention,
    Highest,
}

pub(crate) fn workspace_hit(
    list: Rect,
    pitch: u16,
    tabs: &TabLines,
    scroll: usize,
    pos: Position,
) -> Option<WorkspaceHit> {
    if !list.contains(pos) {
        return None;
    }
    let layout = workspace_layout(list, pitch, tabs, scroll);
    if layout.button().contains(pos) {
        return Some(WorkspaceHit::NewWorkspace);
    }
    let i = layout.at(pos)?;
    let button = row_button(layout.item(i), pitch, pos);
    Some(match (workspace_rows(tabs)[i], button) {
        (WorkspaceRow::Gap | WorkspaceRow::Landing, _) => return None,
        (WorkspaceRow::Workspace(w), Some(RowButton::Close)) => WorkspaceHit::CloseWorkspace(w),
        (WorkspaceRow::Workspace(w), Some(RowButton::Menu)) => WorkspaceHit::WorkspaceMenu(w),
        (WorkspaceRow::Workspace(w), None) if tabs.listed && fold_button(layout.item(i)).contains(pos) => {
            WorkspaceHit::Fold(w)
        }
        (WorkspaceRow::Workspace(w), None) => WorkspaceHit::Workspace(w),
        (WorkspaceRow::Tab(w, t), Some(RowButton::Close)) => WorkspaceHit::CloseTab(w, t),
        (WorkspaceRow::Tab(w, t), Some(RowButton::Menu)) => WorkspaceHit::TabMenu(w, t),
        (WorkspaceRow::Tab(w, t), None) => WorkspaceHit::Tab(w, t),
        (WorkspaceRow::NewTab(w), _) => WorkspaceHit::NewTab(w),
    })
}

#[derive(Default)]
pub(crate) struct WorkspaceEntry {
    pub(crate) name: String,
    pub(crate) collapsed: bool,
    pub(crate) tabs: Vec<TabEntry>,
    pub(crate) behind: u32,
    pub(crate) shared: usize,
    pub(crate) pull: Option<Mark>,
    pub(crate) removing: bool,
    pub(crate) setting_up: bool,
    pub(crate) closed: usize,
    pub(crate) machine: Option<MachineTag>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TabEntry {
    pub(crate) name: String,
    pub(crate) status: Option<Status>,
    pub(crate) running: bool,
    pub(crate) details: Details,
    pub(crate) others: usize,
}

impl TabEntry {
    pub(crate) fn icon(&self, muted: Color, frame: Option<u8>) -> Span<'static> {
        match self.status {
            Some(status) => status_icon(muted, status, frame),
            None => Span::styled(if self.running { PROGRAM_ICON } else { SHELL_ICON }, dim(muted)),
        }
    }

    pub(crate) fn prefix(&self, muted: Color, bg: Style, frame: Option<u8>) -> [Span<'static>; 2] {
        let icon = self.icon(muted, frame);
        [Span::styled(icon.content, bg.patch(icon.style)), Span::styled(" ", bg)]
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Details {
    pub(crate) model: Option<String>,
    pub(crate) account: Option<String>,
    pub(crate) percent: Option<u16>,
    pub(crate) subagents: usize,
    pub(crate) memory: Option<u64>,
    pub(crate) compacted: bool,
}

impl Details {
    pub fn lines(&self) -> u16 {
        1 + u16::from(
            self.model.is_some()
                || self.account.is_some()
                || self.percent.is_some()
                || self.subagents > 0
                || self.memory.is_some()
                || self.compacted,
        )
    }
}

impl From<&str> for TabEntry {
    fn from(name: &str) -> Self {
        Self::from(name.to_string())
    }
}

impl From<String> for TabEntry {
    fn from(name: String) -> Self {
        Self { name, status: None, running: false, details: Details::default(), others: 0 }
    }
}

pub(super) fn draw_workspaces(f: &mut Frame, view: &View, areas: &Areas) {
    if areas.back.is_empty() {
        draw_title(f, view.muted, areas.workspaces_title, "Workspaces");
    } else {
        let name = view.shown_project().and_then(|p| view.projects.get(p)).map(|p| p.name.as_str());
        draw_back(f, view, areas, name.unwrap_or_default());
    }
    if view.shown_project().is_none() {
        return;
    }

    let list = areas.workspaces_list;
    let tabs = view.tab_lines();
    let base = workspace_layout(list, areas.pitch, &tabs, view.workspaces_scroll);
    let landing = view.workspaces_landing();
    let (rows, landing_line) = landed(workspace_rows(&tabs), &base, landing, WorkspaceRow::Gap, WorkspaceRow::Landing);
    let layout = workspace_rows_layout(list, areas.pitch, &rows, &tabs, base.first());
    let paint =
        WorkspacePaint { view, pitch: areas.pitch, tabs: &tabs, rows: &rows, indent: landing.map(|l| l.spot.indent()) };
    for (i, &row) in rows.iter().enumerate() {
        let r = layout.item(i);
        if !r.is_empty() {
            paint.row(f, i, row, r);
        }
    }

    draw_hidden(f, view.muted, &layout, &rows, |r| matches!(r, WorkspaceRow::Workspace(_) | WorkspaceRow::Tab(..)));

    let r = layout.button();
    let accent = Style::default().fg(Color::Cyan);
    if !view.remote_project {
        draw_button(f, r, " ", NEW_BUTTON, button_style(view, r, accent, Color::Cyan));
    }
    draw_landing(f, landing_line, paint.indent);
}

struct WorkspacePaint<'a> {
    view: &'a View<'a>,
    pitch: u16,
    tabs: &'a TabLines,
    rows: &'a [WorkspaceRow],
    indent: Option<u16>,
}

impl WorkspacePaint<'_> {
    fn row(&self, f: &mut Frame, index: usize, row: WorkspaceRow, r: Rect) {
        let view = self.view;
        let guide = r.x.saturating_add(2);
        match row {
            WorkspaceRow::Gap => {}
            WorkspaceRow::Landing => {
                draw_landing(f, r, self.indent);
                if among_tabs(self.rows, index) {
                    draw_guide(f, view.line(), guide, r, Guide::Bar);
                }
            }
            WorkspaceRow::Workspace(w) => self.workspace(f, row, w, r),
            WorkspaceRow::Tab(w, t) => {
                let active = w == view.active_workspace && view.active_tab == Some(t);
                let bg = view.row_background(r, active || view.dragging_workspace_row(row));
                let band = Band { r, pitch: self.pitch, lead: vec![marker(active), Span::raw("  ")], bg };
                let entry = &view.workspaces[w];
                draw_tab_band(f, view, band, (&entry.tabs[t], entry.machine.as_ref()), active);
                let end = if ends_tabs(self.rows, index, w) { Guide::End } else { Guide::Tee };
                draw_guide(f, view.line(), guide, r, end);
                if active {
                    draw_rail(f, r);
                }
            }
            WorkspaceRow::NewTab(w) => {
                if !view.workspaces[w].removing {
                    let style = button_style(view, r, dim(view.muted), Color::Cyan);
                    draw_button(f, r, "   ", "+ Tab", style);
                }
                draw_guide(f, view.line(), guide, r, Guide::End);
            }
        }
    }

    fn workspace(&self, f: &mut Frame, row: WorkspaceRow, w: usize, r: Rect) {
        let view = self.view;
        let folded = self.tabs.is_folded(w);
        let marked = folded && w == view.active_workspace;
        let bg = view.row_background(r, marked || view.dragging_workspace_row(row));
        let fold = if self.tabs.listed { arrow(view.muted, folded) } else { Span::raw("  ") };
        let band = Band { r, pitch: self.pitch, lead: vec![marker(marked), fold], bg };
        let badge = if folded { Badge::Highest } else { Badge::Attention };
        let line = WorkspaceLine { entry: &view.workspaces[w], active: w == view.active_workspace, badge };
        draw_workspace_band(f, view, band, line);
        if !folded {
            draw_guide(f, view.line(), r.x.saturating_add(2), r, Guide::Top);
        }
        if marked {
            draw_rail(f, r);
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum Guide {
    Top,
    Tee,
    End,
    Bar,
}

pub(super) fn draw_guide(f: &mut Frame, colour: Color, x: u16, r: Rect, guide: Guide) {
    if x >= r.right() {
        return;
    }
    let mid = middle(r).y;
    let buf = f.buffer_mut();
    for y in r.top()..r.bottom() {
        let symbol = match (guide, y.cmp(&mid)) {
            (Guide::Tee, Ordering::Equal) => "├",
            (Guide::End, Ordering::Equal) => "└",
            (Guide::Bar, _)
            | (Guide::Top | Guide::Tee, Ordering::Greater)
            | (Guide::Tee | Guide::End, Ordering::Less) => "│",
            _ => continue,
        };
        let cell = &mut buf[(x, y)];
        if cell.symbol() == " " {
            cell.set_symbol(symbol).set_fg(colour);
        }
    }
}

fn ends_tabs(rows: &[WorkspaceRow], i: usize, w: usize) -> bool {
    let next = rows.iter().skip(i + 1).find(|row| **row != WorkspaceRow::Landing);
    !matches!(next, Some(WorkspaceRow::Tab(v, _) | WorkspaceRow::NewTab(v)) if *v == w)
}

fn among_tabs(rows: &[WorkspaceRow], i: usize) -> bool {
    let above = i.checked_sub(1).and_then(|j| rows.get(j)).and_then(|row| match row {
        WorkspaceRow::Workspace(w) | WorkspaceRow::Tab(w, _) => Some(*w),
        _ => None,
    });
    let below = rows.get(i + 1).and_then(|row| match row {
        WorkspaceRow::Tab(w, _) | WorkspaceRow::NewTab(w) => Some(*w),
        _ => None,
    });
    above.is_some() && above == below
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tag {
    Pull,
    Stack,
    Shared,
    Reopen,
}

#[derive(Default)]
struct Places {
    pull: Option<usize>,
    stack: Option<usize>,
    shared: Option<usize>,
    reopen: Option<usize>,
}

impl Places {
    fn of(&self, tag: Tag) -> Option<usize> {
        match tag {
            Tag::Pull => self.pull,
            Tag::Stack => self.stack,
            Tag::Shared => self.shared,
            Tag::Reopen => self.reopen,
        }
    }
}

fn marks(entry: &WorkspaceEntry, badge: Badge, muted: Color, spin: Option<u8>) -> (Vec<Span<'static>>, Places) {
    if entry.removing {
        return (vec![Span::styled(REMOVING_LABEL, Style::default().fg(muted))], Places::default());
    }
    let statuses = entry.tabs.iter().map(|t| t.status);
    let badge = match badge {
        Badge::Off => None,
        Badge::Attention => activity::attention(statuses),
        Badge::Highest => activity::highest(statuses),
    }
    .map(|status| status_icon(muted, status, spin));
    let reopen = (entry.closed > 0).then(|| Span::styled(REOPEN_ICON, Style::default().fg(Color::Cyan)));
    let setting = entry.setting_up.then(|| Span::styled(SETTING_UP_LABEL, Style::default().fg(muted)));
    let reopen_at = usize::from(badge.is_some());
    let pull_at = reopen_at + usize::from(reopen.is_some()) + usize::from(setting.is_some());
    let pull = entry.pull.map(|mark| pull_tag(muted, mark));
    let stack = entry.pull.and_then(|mark| mark.stack).map(|stack| stack_tag(muted, stack));
    let shared = (entry.shared > 0)
        .then(|| Span::styled(format!("{SHARED_ICON}{}", entry.shared), Style::default().fg(Color::Yellow)));
    let places = Places {
        pull: pull.is_some().then_some(pull_at),
        stack: stack.is_some().then_some(pull_at + 1),
        shared: shared.is_some().then_some(pull_at + usize::from(pull.is_some()) + usize::from(stack.is_some())),
        reopen: reopen.is_some().then_some(reopen_at),
    };
    let behind = Some(entry.behind)
        .filter(|n| *n > 0)
        .map(|n| Span::styled(format!("{BEHIND_ICON}{n}"), Style::default().fg(Color::Yellow)));
    (
        badge.into_iter().chain(reopen).chain(setting).chain(pull).chain(stack).chain(shared).chain(behind).collect(),
        places,
    )
}

pub(super) fn tag_badge(
    row: Rect,
    pitch: u16,
    lead: usize,
    (entry, badge): (&WorkspaceEntry, Badge),
    tag: Tag,
) -> Option<Rect> {
    let room = room(row, pitch, lead);
    let (spans, places) = marks(entry, badge, Color::Reset, None);
    let (x, width) = Tags::fit(spans, room).place(places.of(tag)?, lead + room)?;
    let x = u16::try_from(usize::from(row.x) + x).ok()?;
    let rect = Rect::new(x, row.y, u16::try_from(width).ok()?, pitch.min(row.height)).intersection(row);
    (!rect.is_empty()).then_some(rect)
}

pub(crate) fn workspace_tag(
    layout: &Rows,
    pitch: u16,
    tabs: &TabLines,
    w: usize,
    (entry, tag): (&WorkspaceEntry, Tag),
) -> Option<Rect> {
    let badge = if tabs.is_folded(w) { Badge::Highest } else { Badge::Attention };
    let row = row_rect(layout, &workspace_rows(tabs), &WorkspaceRow::Workspace(w));
    tag_badge(row, pitch, 4, (entry, badge), tag)
}

#[derive(Clone, Copy)]
pub(super) struct WorkspaceLine<'a> {
    pub(super) entry: &'a WorkspaceEntry,
    pub(super) active: bool,
    pub(super) badge: Badge,
}

pub(super) fn draw_workspace_band(f: &mut Frame, view: &View, band: Band, line: WorkspaceLine) {
    let WorkspaceLine { entry, active, badge } = line;
    let colour = match (entry.removing, active) {
        (true, _) => view.muted,
        (false, true) => Color::Reset,
        (false, false) => view.soft,
    };
    let style = Style::default().fg(colour).add_modifier(Modifier::BOLD);
    let room = band.room();
    let machine = entry.machine.as_ref();
    let (mut spans, places) = marks(entry, badge, view.muted, spin(machine, view.spin));
    spans.extend(machine.map(|machine| machine_span(machine, room, view.muted)));
    let mut marks = Tags::fit(spans, room);
    for (tag, add) in [
        (Tag::Pull, Modifier::UNDERLINED),
        (Tag::Stack, Modifier::UNDERLINED),
        (Tag::Shared, Modifier::UNDERLINED),
        (Tag::Reopen, Modifier::BOLD | Modifier::UNDERLINED),
    ] {
        if let Some(rect) = tag_badge(band.r, band.pitch, band.lead_width(), (entry, badge), tag)
            && let Some(at) = places.of(tag)
            && sidebar_hovered(view, rect)
        {
            marks.highlight(at, add);
        }
    }
    let allocation = room.saturating_sub(marks.reserved());
    let name = clip_cells(&truncate_cells(&entry.name, allocation), allocation);
    let used = cells(&name);
    let mut line = band.lead;
    line.push(Span::styled(name, style));
    marks.push_onto(&mut line, used, room);
    draw_band(f, band.r, Line::from(line), band.bg);
    if !entry.removing && machine.is_none() {
        draw_row_buttons(f, view, band.r, band.pitch, band.bg);
    }
    dim_row(f, band.r, machine);
}

pub(super) fn draw_tab_band(
    f: &mut Frame,
    view: &View,
    band: Band,
    (tab, machine): (&TabEntry, Option<&MachineTag>),
    active: bool,
) {
    let style = Style::default().fg(if active { Color::Reset } else { view.soft });
    let indent = band.lead_width() + ICON_WIDTH;
    let max = band.room().saturating_sub(ICON_WIDTH);
    let others = (tab.others > 0).then(|| Span::styled(format!("+{}", tab.others), Style::default().fg(view.muted)));
    let marks = Tags::fit(others.into_iter().collect(), max);
    let name = if marks.is_empty() {
        truncate_right(&tab.name, max)
    } else {
        let room = max.saturating_sub(marks.reserved());
        truncate_right(&tab.name, room).chars().take(room).collect()
    };
    let used = name.chars().count();
    let mut line = band.lead;
    line.extend(tab.prefix(view.muted, Style::default(), spin(machine, view.spin)));
    line.push(Span::styled(name, style));
    marks.push_onto(&mut line, used, max);
    draw_band(f, band.r, Line::from(line), band.bg);
    let note = seen_note(tab, machine);
    if tab.lines(machine) > 1 {
        let buttons = row_menu_button(band.r, band.pitch).union(row_close_button(band.r, band.pitch));
        let details = if note.is_some() { &Details::default() } else { &tab.details };
        let line = DetailLine { row: band.r, buttons, details, agent: note.as_deref(), indent };
        draw_details(f, view.muted, line);
    }
    if machine.is_none() {
        draw_row_buttons(f, view, band.r, band.pitch, band.bg);
    }
    dim_row(f, band.r, machine);
}

#[derive(Clone, Copy)]
pub(super) struct DetailLine<'a> {
    pub(super) row: Rect,
    pub(super) buttons: Rect,
    pub(super) details: &'a Details,
    pub(super) agent: Option<&'a str>,
    pub(super) indent: usize,
}

pub(super) fn draw_details(f: &mut Frame, muted: Color, line: DetailLine) {
    let DetailLine { row, buttons, details, agent, indent } = line;
    let r = Rect { y: middle(row).y + 1, height: 1, ..row }.intersection(row);
    let reserved = if buttons.bottom() > r.y { usize::from(buttons.width) } else { 0 };
    let dim = Style::default().fg(muted);
    let percent = percent_span(details, dim);
    let subagents = (details.subagents > 0).then(|| Span::styled(activity::subagents_text(details.subagents), dim));
    let memory = details.memory.map(|bytes| Span::styled(crate::text::size(bytes.max(crate::text::MB / 2)), dim));
    let lead = agent.map(|agent| Span::styled(agent.to_string(), dim));
    let fixed: Vec<Span> = percent.into_iter().chain(subagents).chain(memory).collect();
    let separator = CONTEXT_SEPARATOR.chars().count();
    let fixed_width = lead.iter().chain(&fixed).map(|s| s.width() + separator).sum::<usize>();
    let free = usize::from(r.width).saturating_sub(reserved + 1);
    let indent = indent.min(free.saturating_sub(fixed_width.saturating_sub(separator)));
    let room = free.saturating_sub(indent + fixed_width);
    let account_width = details.account.as_ref().map_or(0, |name| name.chars().count() + separator);
    let model_room = room.saturating_sub(account_width);
    let alone = fixed.is_empty() && lead.is_none() && details.account.is_none();
    let model = details
        .model
        .as_ref()
        .filter(|_| alone || model_room >= MIN_MODEL_WIDTH)
        .map(|model| Span::styled(truncate_right(model, model_room), dim));
    let account_room = if model.is_some() { account_width.saturating_sub(separator) } else { room };
    let account = details
        .account
        .as_ref()
        .filter(|name| account_room >= name.chars().count().min(MIN_ACCOUNT_WIDTH))
        .map(|name| Span::styled(truncate_right(name, account_room), dim));
    let parts = lead.into_iter().chain(model).chain(account).chain(fixed);
    let mut line = vec![Span::raw(" ".repeat(indent))];
    line.extend(parts.flat_map(|part| [Span::styled(CONTEXT_SEPARATOR, dim), part]).skip(1));
    f.render_widget(Paragraph::new(Line::from(line)), r);
}

fn percent_span(details: &Details, dim: Style) -> Option<Span<'static>> {
    let shown = details.percent.map(|used| {
        let level = match Severity::of(used) {
            Severity::Normal => dim,
            severity => Style::default().fg(severity_color(severity)),
        };
        Span::styled(format!("{used}%"), level)
    });
    shown.or_else(|| details.compacted.then(|| Span::styled("compacted", dim)))
}
