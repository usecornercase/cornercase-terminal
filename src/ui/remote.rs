use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph, Widget};

use super::{muted, overlay_block, soft, surface_colour};
use crate::host::theme::HostTheme;
use crate::text::truncate_right;

pub(crate) const TITLE: &str = "Connection lost";
pub(crate) const SWITCH_TITLE: &str = "Switch machine";
pub(crate) const QUIT: &str = " quit ";
pub(crate) const BACK: &str = " back ";
pub(crate) const CONNECT: &str = " connect ";
pub(crate) const THIS_MACHINE: &str = "this machine";
const WIDTH: u16 = 60;
const HEIGHT: u16 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Button {
    Quit,
    Back,
    Connect,
}

impl Button {
    fn label(self) -> &'static str {
        match self {
            Self::Quit => QUIT,
            Self::Back => BACK,
            Self::Connect => CONNECT,
        }
    }
}

pub(crate) struct Notice<'a> {
    pub(crate) host: &'a str,
    pub(crate) retry_in: Option<u64>,
    pub(crate) error: Option<&'a str>,
    pub(crate) hovered: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Step<'a> {
    Asking,
    Connecting,
    Failed(&'a str),
}

pub(crate) struct Switch<'a> {
    pub(crate) from: &'a str,
    pub(crate) to: &'a str,
    pub(crate) step: Step<'a>,
    pub(crate) hovered: Option<Button>,
}

pub(crate) enum Shown<'a> {
    Lost(Notice<'a>),
    Switch(Switch<'a>),
}

impl Shown<'_> {
    pub(crate) fn buttons(&self) -> &'static [Button] {
        match self {
            Self::Lost(_) => &[Button::Quit],
            Self::Switch(Switch { step: Step::Asking, .. }) => &[Button::Connect, Button::Back],
            Self::Switch(_) => &[Button::Back],
        }
    }

    fn hovered(&self) -> Option<Button> {
        match self {
            Self::Lost(notice) => notice.hovered.then_some(Button::Quit),
            Self::Switch(switch) => switch.hovered,
        }
    }

    fn title(&self) -> &'static str {
        match self {
            Self::Lost(_) => TITLE,
            Self::Switch(_) => SWITCH_TITLE,
        }
    }

    fn texts(&self, width: usize) -> [String; 4] {
        match self {
            Self::Lost(notice) => {
                let status = match notice.retry_in {
                    Some(seconds) => format!("trying again in {seconds} s"),
                    None => "connecting…".into(),
                };
                [
                    format!("reconnecting to {}…", notice.host),
                    status,
                    notice.error.unwrap_or_default().into(),
                    format!("its shells keep running on {}", notice.host),
                ]
            }
            Self::Switch(Switch { from, to, step, .. }) => match step {
                Step::Asking => [
                    format!("connect to {to}?"),
                    "this window has not reached it before".into(),
                    format!("you stay on {from} until you connect"),
                    String::new(),
                ],
                Step::Connecting => [
                    format!("switching to {to}…"),
                    "connecting…".into(),
                    String::new(),
                    format!("you stay on {from} until it answers"),
                ],
                Step::Failed(error) => {
                    let [first, rest] = split_words(error, width);
                    [format!("could not switch to {to}"), first, rest, format!("you are still on {from}")]
                }
            },
        }
    }
}

fn split_words(text: &str, width: usize) -> [String; 2] {
    let mut first = String::new();
    let mut words = text.split(' ');
    let mut rest = Vec::new();
    for word in words.by_ref() {
        let used = first.chars().count();
        if used > 0 && used + 1 + word.chars().count() > width {
            rest.push(word);
            break;
        }
        if used > 0 {
            first.push(' ');
        }
        first.push_str(word);
    }
    rest.extend(words);
    [first, rest.join(" ")]
}

pub(crate) fn area(screen: Rect) -> Rect {
    centred(screen, WIDTH, HEIGHT)
}

pub(super) fn centred(screen: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(screen.width);
    let height = height.min(screen.height);
    Rect { x: screen.x + (screen.width - width) / 2, y: screen.y + (screen.height - height) / 2, width, height }
}

pub(crate) fn buttons(screen: Rect, shown: &[Button]) -> Vec<(Button, Rect)> {
    let r = area(screen);
    let y = r.bottom().saturating_sub(2).max(r.y);
    let mut right = r.right().saturating_sub(2);
    let mut placed = Vec::new();
    for &button in shown.iter().rev() {
        let label = u16::try_from(button.label().chars().count()).unwrap_or(u16::MAX);
        let width = label.min(r.width.saturating_sub(4));
        let x = right.saturating_sub(width).max(r.x);
        placed.push((button, Rect { x, y, width: width.min(right.saturating_sub(x)), height: 1 }));
        right = x.saturating_sub(1);
    }
    placed.reverse();
    placed
}

pub(crate) fn hit(screen: Rect, shown: &[Button], at: Position) -> Option<Button> {
    buttons(screen, shown).into_iter().find_map(|(button, r)| r.contains(at).then_some(button))
}

pub(crate) fn draw(buf: &mut Buffer, theme: &HostTheme, shown: &Shown) {
    let screen = buf.area;
    let r = area(screen);
    let grey = muted(theme);
    Clear.render(r, buf);
    let block = overlay_block(grey, shown.title());
    let inner = block.inner(r);
    block.render(r, buf);
    let width = usize::from(inner.width.saturating_sub(2));
    let placed = buttons(screen, shown.buttons());
    let taken: usize = placed.iter().map(|(button, _)| button.label().len() + 1).sum();
    let [bold, status, error, last] = shown.texts(width);
    let plain =
        |text: &str, room: usize| Line::from(Span::styled(truncate_right(text, room), Style::default().fg(grey)));
    let lines = vec![
        Line::default(),
        Line::from(Span::styled(truncate_right(&bold, width), Style::default().add_modifier(Modifier::BOLD))),
        plain(&status, width),
        plain(&error, width),
        Line::default(),
        plain(&last, width.saturating_sub(taken)),
    ];
    let text = Rect { x: inner.x + 1, width: inner.width.saturating_sub(2), ..inner };
    Paragraph::new(lines).render(text, buf);
    let surface = surface_colour(theme.is_light() == Some(true));
    for (button, at) in placed {
        let style = if shown.hovered() == Some(button) {
            Style::default().fg(Color::Black).bg(Color::Gray)
        } else {
            Style::default().fg(soft(theme)).bg(surface)
        };
        Paragraph::new(Span::styled(button.label(), style)).render(at, buf);
    }
}

#[cfg(test)]
mod tests;
