use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

use super::{BRAND_COLOR, View, hovered, overlay_block, rail, truncate_right};
use crate::shortcuts::{self, Step};

const GAP: u16 = 2;
const KEY_GAP: usize = 2;

pub struct Item {
    pub key: &'static str,
    pub label: &'static str,
    pub group: bool,
    pub clickable: bool,
}

impl From<&shortcuts::Item> for Item {
    fn from(item: &shortcuts::Item) -> Self {
        Self {
            key: item.key,
            label: item.label,
            group: matches!(item.step, Some(Step::Open(_))),
            clickable: item.step.is_some(),
        }
    }
}

pub struct Keys {
    pub title: String,
    pub items: Vec<Item>,
    pub hint: String,
}

impl Keys {
    fn key_width(&self) -> usize {
        self.items.iter().map(|i| i.key.chars().count()).max().unwrap_or(0)
    }

    fn cell_width(&self) -> u16 {
        let label = self.items.iter().map(|i| i.label.chars().count()).max().unwrap_or(0);
        u16::try_from(1 + self.key_width() + KEY_GAP + label + 1).unwrap_or(u16::MAX)
    }

    fn columns(&self, room: u16) -> u16 {
        let cell = self.cell_width();
        let count = u16::try_from(self.items.len()).unwrap_or(u16::MAX).max(1);
        let fit = (room.saturating_add(GAP) / cell.saturating_add(GAP)).clamp(1, count);
        count.div_ceil(count.div_ceil(fit))
    }

    fn rows(&self, columns: u16) -> u16 {
        u16::try_from(self.items.len().div_ceil(usize::from(columns.max(1)))).unwrap_or(u16::MAX)
    }
}

pub fn frame(pane: Rect, screen: Rect, keys: &Keys) -> Rect {
    let rows = keys.rows(keys.columns(pane.width.saturating_sub(2)));
    if rows.saturating_add(4) <= pane.height { pane } else { screen }
}

pub fn area(frame: Rect, keys: &Keys) -> Rect {
    let columns = keys.columns(frame.width.saturating_sub(2));
    let rows = keys.rows(columns);
    let cells = columns * keys.cell_width() + (columns - 1) * GAP;
    let hint = u16::try_from(keys.hint.chars().count() + 2).unwrap_or(u16::MAX);
    let width = cells.max(hint).saturating_add(2).min(frame.width);
    let height = rows.saturating_add(4).min(frame.height);
    let x = frame.x + (frame.width - width) / 2;
    Rect::new(x, frame.bottom() - height, width, height)
}

pub fn item(menu: Rect, keys: &Keys, i: usize) -> Rect {
    let columns = keys.columns(menu.width.saturating_sub(2));
    let rows = usize::from(keys.rows(columns).max(1));
    let (column, row) = (u16::try_from(i / rows).unwrap_or(u16::MAX), u16::try_from(i % rows).unwrap_or(u16::MAX));
    let body = Rect::new(menu.x + 1, menu.y + 1, menu.width.saturating_sub(2), menu.height.saturating_sub(4));
    let x = body.x.saturating_add(column.saturating_mul(keys.cell_width() + GAP));
    Rect::new(x, body.y.saturating_add(row), keys.cell_width(), 1).intersection(body)
}

pub fn hit(menu: Rect, keys: &Keys, pos: Position) -> Option<usize> {
    (0..keys.items.len()).find(|&i| keys.items[i].clickable && item(menu, keys, i).contains(pos))
}

pub fn draw(f: &mut Frame, view: &View, keys: &Keys, pane: Rect) {
    let menu = area(frame(pane, f.area(), keys), keys);
    f.render_widget(Clear, menu);
    f.render_widget(overlay_block(view.muted, &keys.title), menu);
    let key_width = keys.key_width();
    for (i, entry) in keys.items.iter().enumerate() {
        let r = item(menu, keys, i);
        if r.is_empty() {
            continue;
        }
        let lit = entry.clickable && hovered(view, r);
        let bg = if lit { Style::default().bg(view.surface()) } else { Style::default() };
        let bold = if lit { bg.add_modifier(Modifier::BOLD) } else { bg };
        let label = if entry.group { bold.fg(BRAND_COLOR) } else { bold };
        let key = format!("{:<key_width$}{}", entry.key, " ".repeat(KEY_GAP));
        let line = Line::from(vec![
            rail(lit, bg),
            Span::styled(key, bg.fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::styled(entry.label, label),
        ]);
        f.render_widget(Paragraph::new(line).style(bg), r);
    }
    let hint = Rect::new(menu.x + 2, menu.bottom().saturating_sub(2), menu.width.saturating_sub(4), 1);
    if menu.height >= 4 {
        let text = truncate_right(&keys.hint, usize::from(hint.width));
        f.render_widget(Paragraph::new(Span::styled(text, Style::default().fg(view.muted))), hint);
    }
}
