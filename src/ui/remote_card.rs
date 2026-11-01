use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

use super::View;
use super::remote::centred;
use super::widgets::overlay_block;
use crate::agents::activity::Status;
use crate::text::{truncate_left, truncate_right};

const WIDTH: u16 = 52;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Retry {
    In(u64),
    Now,
    Because(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RemoteCard {
    pub(crate) machine: String,
    pub(crate) agent: Option<(String, Option<Status>)>,
    pub(crate) folder: String,
    pub(crate) down: Option<Retry>,
}

impl RemoteCard {
    fn title(&self) -> String {
        match self.down {
            Some(_) => format!("{} unreachable", self.machine),
            None => format!("runs on {}", self.machine),
        }
    }

    fn lines(&self) -> Vec<String> {
        match &self.down {
            None => vec![self.doing()],
            Some(retry) => vec![retrying(retry), self.keeps()],
        }
    }

    fn doing(&self) -> String {
        match &self.agent {
            Some((agent, status)) => format!("{agent} {}", state(*status)),
            None => "no agent there".into(),
        }
    }

    fn keeps(&self) -> String {
        match &self.agent {
            Some((agent, _)) => format!("{agent} keeps working there"),
            None => "its shells keep running there".into(),
        }
    }
}

fn state(status: Option<Status>) -> String {
    match status {
        Some(Status::Waiting) => "is waiting for you".into(),
        Some(Status::Failed) => "failed".into(),
        Some(status) => format!("is {}", status.name()),
        None => format!("is {}", Status::Idle.name()),
    }
}

fn retrying(retry: &Retry) -> String {
    match retry {
        Retry::In(seconds) => format!("reconnecting in {seconds} s"),
        Retry::Now => "reconnecting…".into(),
        Retry::Because(why) => why.clone(),
    }
}

pub(super) fn draw_remote_card(f: &mut Frame, view: &View, card: &RemoteCard, area: Rect) {
    let lines = card.lines();
    let height = u16::try_from(lines.len()).unwrap_or(u16::MAX).saturating_add(5);
    let r = centred(area, WIDTH, height);
    let title = card.title();
    let block = overlay_block(view.muted, &title);
    let inner = block.inner(r);
    f.render_widget(Clear, r);
    f.render_widget(block, r);
    let width = usize::from(inner.width.saturating_sub(2));
    let grey = Style::default().fg(view.muted);
    let mut text = vec![Line::default()];
    text.extend(lines.iter().enumerate().map(|(i, line)| {
        let style = if i == 0 { Style::default().add_modifier(Modifier::BOLD) } else { grey };
        Line::from(Span::styled(truncate_right(line, width), style))
    }));
    text.push(Line::from(Span::styled(truncate_left(&card.folder, width), grey)));
    let body = Rect { x: inner.x + 1, width: inner.width.saturating_sub(2), ..inner };
    f.render_widget(Paragraph::new(text), body);
}
