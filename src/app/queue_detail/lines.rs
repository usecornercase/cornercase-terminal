use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

use crate::control::QueueShown;
use crate::editor::wrap_text;
use crate::queue::clock::display;
use crate::queue::conversation::{MESSAGE_CAP, Message, Talk};
use crate::queue::outcome::{Pull, TEXT_CAP};
use crate::queue::{self};

const LABEL: usize = 10;

fn dim(text: impl Into<String>, muted: Color) -> Line<'static> {
    Line::from(Span::styled(text.into(), Style::default().fg(muted)))
}

fn row(label: &str, value: String, muted: Color) -> Line<'static> {
    Line::from(vec![Span::styled(format!("{label:<LABEL$}"), Style::default().fg(muted)), Span::raw(value)])
}

fn pull(pull: &Pull) -> String {
    let mut parts = vec![format!("#{} {}", pull.number, pull.title), pull.state.clone()];
    parts.extend(pull.checks.as_ref().map(|checks| format!("checks {checks}")));
    parts.join(" · ")
}

fn facts(shown: &QueueShown, entry: &queue::Entry, zone: &str, muted: Color) -> Vec<Line<'static>> {
    let task = &entry.task;
    let result = shown.result.as_ref();
    let mut lines = Vec::new();
    lines.push(match entry.started {
        Some(at) => row("started", display(at, zone), muted),
        None => row("scheduled", display(task.at, zone), muted),
    });
    if let Some(result) = result {
        lines.push(row("result", display(result.at, zone), muted));
    }
    let flow = task.flow.as_ref().map(|flow| format!("flow {flow}"));
    let agent = result.and_then(|result| result.agent.clone()).or(flow).unwrap_or_else(|| task.agent.clone());
    lines.push(row("agent", agent, muted));
    let branch = result.and_then(|result| result.branch.clone()).or_else(|| task.place.branch.clone());
    lines.extend(branch.map(|branch| row("branch", branch, muted)));
    lines.extend(result.and_then(|result| result.pull_request.as_ref()).map(|found| row("pull", pull(found), muted)));
    lines
}

pub(super) fn result(
    shown: &QueueShown,
    entry: &queue::Entry,
    (zone, width): (&str, usize),
    muted: Color,
) -> Vec<Line<'static>> {
    let mut lines = facts(shown, entry, zone, muted);
    lines.push(Line::default());
    if let Some(result) = shown.result.as_ref().filter(|result| !result.text.is_empty()) {
        lines.extend(wrap_text(&result.text, width).into_iter().map(Line::from));
        if result.cut {
            lines.push(dim(format!("… only the first {} KiB are kept", TEXT_CAP / 1024), muted));
        }
    }
    if let Some(note) = &shown.note {
        lines.extend(wrap_text(note, width).into_iter().map(|line| dim(line, muted)));
    }
    lines
}

fn header(message: &Message, zone: &str) -> String {
    let role = match message.role {
        crate::agents::context::Role::Prompt => "prompt",
        crate::agents::context::Role::Reply => "reply",
    };
    let at =
        message.written.as_deref().and_then(crate::time::Stamp::parse).map(|at| display(crate::time::secs(at), zone));
    at.map_or_else(|| role.to_string(), |at| format!("{role} · {at}"))
}

pub(super) fn talk(talk: &Talk, (zone, width): (&str, usize), muted: Color) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if talk.earlier {
        lines.push(dim("(earlier messages are left out)", muted));
        lines.push(Line::default());
    }
    for message in &talk.messages {
        lines.push(dim(header(message, zone), muted));
        lines.extend(wrap_text(&message.text, width).into_iter().map(Line::from));
        if message.cut {
            lines.push(dim(format!("… cut at {} KiB", MESSAGE_CAP / 1024), muted));
        }
        lines.push(Line::default());
    }
    lines
}

pub(super) fn plain(talk: &Talk, zone: &str) -> String {
    let parts: Vec<String> =
        talk.messages.iter().map(|message| format!("{}\n{}", header(message, zone), message.text)).collect();
    parts.join("\n\n")
}
