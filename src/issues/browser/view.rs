use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use super::{Browser, Room, Screen};
use crate::issues::{Detail, Issue, branch};
use crate::markdown;
use crate::time;

impl Browser {
    pub fn start_hint(&self, issue: &Issue) -> String {
        match self.place_for(issue) {
            Some(place) if place.worktree => format!("in its own worktree, on {}, in {}", branch(issue), place.label),
            Some(place) => format!("in a new tab in {}", place.label),
            None if self.worktrees => format!("in its own worktree, on {}", branch(issue)),
            None => "in a new tab".into(),
        }
    }

    fn start_lines(&self, issue: &Issue) -> Vec<Line<'static>> {
        let dim = Style::default().fg(Color::DarkGray);
        let row = |label: &str, spans: Vec<Span<'static>>| {
            Line::from([vec![Span::styled(format!("{label:<8}"), dim)], spans].concat())
        };
        let agent = match self.agents.pick() {
            Some(kind) => {
                let start = self.agents.starts.get(&kind);
                let mut spans = vec![Span::styled(kind.clone(), Style::default().add_modifier(Modifier::BOLD))];
                if let Some(mode) = start.and_then(|s| s.mode.clone()) {
                    let style = if crate::agents::is_dangerous(&mode) {
                        Style::default().fg(Color::Red)
                    } else {
                        Style::default()
                    };
                    spans.push(Span::raw(" · "));
                    spans.push(Span::styled(mode, style));
                }
                if let Some(start) = start {
                    spans.push(Span::styled(format!("  {}", start.command), dim));
                }
                let notes = self.agents.notes(&kind);
                if !notes.is_empty() {
                    spans.push(Span::styled(format!("  ({notes})"), dim));
                }
                spans
            }
            None => vec![Span::styled("none chosen yet: start asks which one", dim)],
        };
        let prompt = crate::issues::prompt(&self.agents.prompt, issue, &branch(issue));
        let send = if self.agents.submit { "sent for you" } else { "typed, you press enter" };
        vec![
            row("start", vec![Span::raw(self.start_hint(issue))]),
            row("agent", agent),
            row("prompt", vec![Span::raw(crate::text::one_line(&prompt)), Span::styled(format!("  ({send})"), dim)]),
        ]
    }

    pub fn detail_lines(&self, room: Room) -> Vec<Line<'static>> {
        let Screen::Detail { issue, detail, raw, .. } = &self.screen else { return Vec::new() };
        let width = room.detail_width;
        let start = self.start_lines(issue);
        detail_lines(issue, detail.as_ref().and_then(|d| d.as_ref().ok()), width, time::now(), *raw, start)
    }
}

fn raw_lines(text: &str, width: usize) -> Vec<Line<'static>> {
    let width = width.max(1);
    text.replace('\t', "    ")
        .split('\n')
        .flat_map(|line| {
            let chars: Vec<char> = line.trim_end().chars().collect();
            if chars.is_empty() {
                return vec![Line::default()];
            }
            chars.chunks(width).map(|chunk| Line::from(chunk.iter().collect::<String>())).collect()
        })
        .collect()
}

pub fn detail_lines(
    issue: &Issue,
    detail: Option<&Detail>,
    width: usize,
    now: i64,
    raw: bool,
    start: Vec<Line<'static>>,
) -> Vec<Line<'static>> {
    let body = |text: &str| if raw { raw_lines(text, width) } else { markdown::render(text, width) };
    let dim = Style::default().fg(Color::DarkGray);
    let bold = Style::default().add_modifier(Modifier::BOLD);
    let segments = [(issue.key.clone(), bold.fg(Color::Cyan)), (format!(" {}", issue.title), bold)];
    let mut lines = markdown::wrap_segments(&segments, width);
    let mut info: Vec<String> = detail.map_or_else(|| vec![issue.state.clone()], |d| d.info.clone());
    let age = crate::issues::age_of(&issue.updated_at, now);
    info.extend((!age.is_empty()).then(|| format!("updated {age} ago")));
    lines.extend(markdown::wrap_text(&info.join(" · "), dim, width));
    lines.extend(markdown::wrap_text(&issue.url, dim, width));
    if !start.is_empty() {
        lines.push(Line::default());
        lines.extend(start);
    }
    let Some(detail) = detail else { return lines };
    lines.push(Line::default());
    if detail.body.trim().is_empty() {
        lines.push(Line::from(Span::styled("No description.", dim)));
    } else {
        lines.extend(body(&detail.body));
    }
    for comment in &detail.comments {
        lines.push(Line::default());
        let age = crate::issues::age_of(&comment.created_at, now);
        let head = if age.is_empty() {
            format!("── @{} ", comment.author)
        } else {
            format!("── @{} · {age} ago ", comment.author)
        };
        let fill = width.saturating_sub(head.chars().count());
        lines.push(Line::from(Span::styled(format!("{head}{}", "─".repeat(fill)), dim)));
        lines.extend(body(&comment.body));
    }
    lines
}
