use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Clear, Paragraph};

use super::chrome::{ISSUES_LABEL, TODO_LABEL, USAGE_LABEL, draw_column_border};
use super::widgets::{put, sidebar_hovered};
use super::{Overlay, View, files, flow, icons, remote_access};

pub(crate) const WIDTH: u16 = 4;

const SLOT: u16 = 3;

const PITCH: u16 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Item {
    Issues,
    Changes,
    Files,
    Todo,
    Flow,
    Update,
    Remote,
    Usage,
    Settings,
    Quit,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[expect(clippy::struct_excessive_bools, reason = "each flag says whether one button shows")]
pub(crate) struct Shown {
    pub(crate) issues: bool,
    pub(crate) changes: bool,
    pub(crate) files: bool,
    pub(crate) flow: bool,
    pub(crate) update: bool,
    pub(crate) remote: bool,
}

impl Shown {
    fn of(view: &View) -> Self {
        Self {
            issues: view.issues && view.has_project,
            changes: view.changes_button.is_some(),
            files: view.has_project,
            flow: view.flow_run,
            update: view.update.is_some(),
            remote: view.remote.is_some(),
        }
    }
}

impl Item {
    fn icon(self) -> &'static str {
        match self {
            Self::Issues => icons::ISSUES,
            Self::Changes => icons::NERD.changes,
            Self::Files => icons::NERD.files,
            Self::Todo => icons::NERD.todo,
            Self::Flow => icons::FLOW,
            Self::Update => icons::UPDATE,
            Self::Remote => icons::REMOTE,
            Self::Usage => icons::USAGE,
            Self::Settings => icons::SETTINGS,
            Self::Quit => icons::QUIT,
        }
    }

    fn label(self, view: &View) -> String {
        match self {
            Self::Issues => ISSUES_LABEL.into(),
            Self::Changes => view.changes_button.as_ref().map_or_else(String::new, |b| b.label.clone()),
            Self::Files => files::LABEL.into(),
            Self::Todo => TODO_LABEL.into(),
            Self::Flow => flow::LABEL.into(),
            Self::Update => view.update.clone().unwrap_or_default(),
            Self::Remote => remote_access::LABEL.into(),
            Self::Usage => USAGE_LABEL.into(),
            Self::Settings => "Settings".into(),
            Self::Quit => "Quit".into(),
        }
    }

    fn open(self, view: &View) -> bool {
        match self {
            Self::Issues => matches!(view.overlay, Some(Overlay::Issues(_))),
            Self::Changes => view.changes_button.as_ref().is_some_and(|b| b.open),
            Self::Files => view.files.is_some(),
            Self::Todo => view.todo.is_some(),
            Self::Flow => view.flow.is_some(),
            Self::Remote => matches!(view.overlay, Some(Overlay::Remote(_))),
            Self::Usage => matches!(view.overlay, Some(Overlay::Usage(_))),
            Self::Settings => matches!(view.overlay, Some(Overlay::Settings(_) | Overlay::Flows(_))),
            Self::Update | Self::Quit => false,
        }
    }

    fn badge(self, view: &View) -> Option<Span<'static>> {
        let muted = Style::default().fg(view.muted);
        match self {
            Self::Changes => view
                .changes_button
                .as_ref()
                .filter(|b| b.files > 0)
                .map(|b| Span::styled(icons::count(b.files), Style::default().fg(Color::Cyan))),
            Self::Todo => (view.todo_dot && view.todo_pending).then(|| Span::styled(icons::DOT, muted)),
            Self::Remote => {
                view.remote.filter(|&on| on).map(|_| Span::styled(icons::DOT, Style::default().fg(Color::Green)))
            }
            _ => None,
        }
    }
}

pub(crate) fn items(bar: Rect, shown: Shown) -> Vec<(Item, Rect)> {
    if bar.width < SLOT || bar.height == 0 {
        return Vec::new();
    }
    let slot = |y| Rect { x: bar.x, y, width: SLOT, height: 1 };
    let bottom = [
        (Item::Quit, true),
        (Item::Settings, true),
        (Item::Usage, true),
        (Item::Remote, shown.remote),
        (Item::Update, shown.update),
    ];
    let mut placed = Vec::new();
    let mut floor = bar.bottom();
    for (item, _) in bottom.into_iter().filter(|(_, on)| *on) {
        let Some(y) = floor.checked_sub(PITCH).filter(|&y| y > bar.y) else { break };
        floor = y;
        placed.push((item, slot(y)));
    }
    let top = [
        (Item::Issues, shown.issues),
        (Item::Changes, shown.changes),
        (Item::Files, shown.files),
        (Item::Todo, true),
        (Item::Flow, shown.flow),
    ];
    let mut y = bar.y + 1;
    for (item, _) in top.into_iter().filter(|(_, on)| *on) {
        if y + PITCH > floor {
            break;
        }
        placed.push((item, slot(y)));
        y += PITCH;
    }
    placed
}

pub(crate) fn hit(bar: Rect, shown: Shown, pos: Position) -> Option<Item> {
    items(bar, shown).into_iter().find_map(|(item, r)| r.contains(pos).then_some(item))
}

pub(super) fn draw(f: &mut Frame, view: &View, bar: Rect) {
    let border = Rect { x: bar.right().saturating_sub(1), width: bar.width.min(1), ..bar };
    draw_column_border(f, view.line(), border);
    let buf = f.buffer_mut();
    for (item, r) in items(bar, Shown::of(view)) {
        let lit = sidebar_hovered(view, r);
        let open = item.open(view);
        let bg = if lit { Style::default().bg(view.hover_fill()) } else { Style::default() };
        buf.set_style(r, bg);
        if open {
            put(buf, r.x, r.y, "▌", bg.fg(Color::Cyan), r.right());
        }
        let fg = match item {
            Item::Quit if lit => Color::Red,
            Item::Update => Color::Cyan,
            _ if open => Color::Cyan,
            _ if lit => Color::Reset,
            _ => view.soft,
        };
        let style = if open || lit { bg.fg(fg).add_modifier(Modifier::BOLD) } else { bg.fg(fg) };
        put(buf, r.x + 1, r.y, item.icon(), style, r.right());
        if let Some(badge) = item.badge(view).filter(|_| r.bottom() < bar.bottom()) {
            put(buf, r.right() - 1, r.bottom(), &badge.content, badge.style, r.right());
        }
    }
}

pub(super) fn draw_tooltip(f: &mut Frame, view: &View, bar: Rect) {
    let Some((item, r)) = items(bar, Shown::of(view)).into_iter().find(|(_, r)| sidebar_hovered(view, *r)) else {
        return;
    };
    let text = format!(" {} ", item.label(view));
    let screen = f.area();
    let width = u16::try_from(text.chars().count()).unwrap_or(u16::MAX).min(screen.right().saturating_sub(bar.right()));
    let tip = Rect { x: bar.right(), y: r.y, width, height: 1 };
    let style = Style::default().fg(Color::Reset).bg(view.surface()).add_modifier(Modifier::BOLD);
    f.render_widget(Clear, tip);
    f.render_widget(Paragraph::new(Span::styled(text, style)), tip);
}
