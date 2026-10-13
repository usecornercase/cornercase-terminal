use ratatui::Frame;
use ratatui::layout::{Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Paragraph};

use super::View;
use super::dialog::CANCEL_LABEL;
use super::theme::status_icon;
use super::widgets::button_width;
use crate::agents::activity::Status;

const UNDO_LABEL: &str = "undo";

const TOAST_ICON: &str = " ✓ ";

const BUG_ICON: &str = " ✗ ";

const RESTART_ICON: &str = " ↻ ";

const TOAST_MARGIN: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ToastIcon {
    Check,
    Agent(Status),
    Bug,
    Restart,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ToastButton {
    Undo,
    Cancel,
}

impl ToastButton {
    fn label(self) -> &'static str {
        match self {
            Self::Undo => UNDO_LABEL,
            Self::Cancel => CANCEL_LABEL,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Toast<'a> {
    pub(crate) message: &'a str,
    pub(crate) icon: ToastIcon,
    pub(crate) button: Option<ToastButton>,
}

pub(crate) fn toast_area(area: Rect, toast: Toast) -> Rect {
    let button = toast.button.map_or(0, |button| button.label().chars().count() + 3);
    let text = TOAST_ICON.chars().count() + toast.message.chars().count() + 1 + button;
    let width = u16::try_from(text).unwrap_or(u16::MAX).saturating_add(2).min(area.width);
    let height = 3.min(area.height);
    let x = area.right().saturating_sub(width + TOAST_MARGIN).max(area.x);
    let y = area.bottom().saturating_sub(height + TOAST_MARGIN).max(area.y);
    Rect::new(x, y, width, height)
}

pub(crate) fn toast_button(area: Rect, toast: Toast) -> Rect {
    let Some(button) = toast.button else { return Rect::default() };
    let r = toast_area(area, toast).inner(Margin::new(1, 1));
    let width = button_width(button.label()).min(r.width);
    Rect { x: r.right().saturating_sub(width + 1), width, ..r }
}

pub(super) fn draw_toast(f: &mut Frame, view: &View, toast: Toast) {
    let r = toast_area(f.area(), toast);
    let icon = match toast.icon {
        ToastIcon::Check => Span::styled(TOAST_ICON, Style::default().fg(Color::Green)),
        ToastIcon::Agent(status) => {
            let icon = status_icon(view.muted, status, view.spin);
            Span::styled(format!(" {} ", icon.content), icon.style)
        }
        ToastIcon::Bug => Span::styled(BUG_ICON, Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
        ToastIcon::Restart => Span::styled(RESTART_ICON, Style::default().fg(Color::Cyan)),
    };
    let border = Style::default().fg(icon.style.fg.unwrap_or(Color::Green));
    f.render_widget(Clear, r);
    f.render_widget(Block::bordered().border_type(BorderType::Rounded).border_style(border), r);
    f.render_widget(Paragraph::new(Line::from(vec![icon, Span::raw(toast.message)])), r.inner(Margin::new(1, 1)));
    let button = toast_button(f.area(), toast);
    if let Some(label) = toast.button.map(ToastButton::label).filter(|_| !button.is_empty()) {
        let lit = view.hover.is_some_and(|p| button.contains(p));
        let style = if lit {
            Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
        };
        f.render_widget(Paragraph::new(Span::styled(format!(" {label} "), style)), button);
    }
}
