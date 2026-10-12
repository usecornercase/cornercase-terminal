use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

use crate::ui::View;
use crate::ui::widgets::{hovered, overlay_block, rail};

pub fn menu_area(area: Rect, at: Position, items: &[impl AsRef<str>]) -> Rect {
    let longest = items.iter().map(|item| item.as_ref().chars().count()).max().unwrap_or(0);
    let width = u16::try_from(longest).unwrap_or(u16::MAX).saturating_add(4).min(area.width);
    let height = u16::try_from(items.len()).unwrap_or(u16::MAX).saturating_add(2).min(area.height);
    let x = at.x.min(area.right().saturating_sub(width));
    let y = at.y.saturating_add(1).min(area.bottom().saturating_sub(height));
    Rect::new(x, y, width, height)
}

pub fn menu_item(menu: Rect, i: usize) -> Rect {
    let y = menu.y.saturating_add(1).saturating_add(u16::try_from(i).unwrap_or(u16::MAX));
    Rect::new(menu.x + 1, y, menu.width.saturating_sub(2), 1).intersection(menu)
}

pub fn menu_hit(menu: Rect, items: usize, pos: Position) -> Option<usize> {
    (0..items).find(|&i| menu_item(menu, i).contains(pos))
}

pub(in crate::ui) fn draw_menu(f: &mut Frame, view: &View, at: Position, items: &[String]) {
    let menu = menu_area(f.area(), at, items);
    f.render_widget(Clear, menu);
    f.render_widget(overlay_block(view.muted, ""), menu);
    for (i, item) in items.iter().enumerate() {
        let r = menu_item(menu, i);
        let lit = hovered(view, r);
        let bg = if lit { Style::default().bg(view.surface()) } else { Style::default() };
        let text = if lit { bg.add_modifier(Modifier::BOLD) } else { bg };
        let line = Line::from(vec![rail(lit, bg), Span::styled(format!("{item} "), text)]);
        f.render_widget(Paragraph::new(line).style(bg), r);
    }
}
