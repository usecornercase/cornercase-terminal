use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;

use crate::agents::activity::{self, Status};
use crate::agents::usage::Severity;
use crate::host::theme::HostTheme;
use crate::pull_request::{Ci, Mark, Outcome, State};

pub(super) const BRAND_COLOR: Color = Color::Indexed(99);

pub(super) const DARK_SURFACE: Color = Color::Indexed(236);

pub(super) const LIGHT_SURFACE: Color = Color::Indexed(254);

pub(super) const DARK_HOVER: Color = Color::Indexed(235);

pub(super) const LIGHT_HOVER: Color = Color::Indexed(255);

const DARK_MUTED: Color = Color::Indexed(243);

const LIGHT_MUTED: Color = Color::Indexed(245);

const DARK_SOFT: Color = Color::Indexed(250);

const LIGHT_SOFT: Color = Color::Indexed(240);

pub(super) const DARK_LINE: Color = Color::Indexed(239);

const LIGHT_LINE: Color = Color::Indexed(250);

pub(super) const WAITING_COLOR: Color = Color::Indexed(208);

pub(super) fn severity_color(severity: Severity) -> Color {
    match severity {
        Severity::Normal => Color::Green,
        Severity::Warning => WAITING_COLOR,
        Severity::Critical => Color::Red,
    }
}

pub(super) fn line_colour(light: bool) -> Color {
    if light { LIGHT_LINE } else { DARK_LINE }
}

pub(super) fn surface_colour(light: bool) -> Color {
    if light { LIGHT_SURFACE } else { DARK_SURFACE }
}

pub(super) fn hover_colour(light: bool) -> Color {
    if light { LIGHT_HOVER } else { DARK_HOVER }
}

pub(crate) fn muted(theme: &HostTheme) -> Color {
    if theme.muted_is_readable() {
        Color::DarkGray
    } else if theme.is_light() == Some(true) {
        LIGHT_MUTED
    } else {
        DARK_MUTED
    }
}

pub(crate) fn soft(theme: &HostTheme) -> Color {
    if theme.soft_is_readable() {
        Color::Gray
    } else if theme.is_light() == Some(true) {
        LIGHT_SOFT
    } else {
        DARK_SOFT
    }
}

pub(super) fn status_icon(muted: Color, status: Status, frame: Option<u8>) -> Span<'static> {
    let (glyph, style) = match status {
        Status::Idle => ("○", Style::default().fg(muted)),
        Status::Working => {
            let glyph = frame.map_or(activity::MOON, |f| activity::SPINNER[usize::from(f) % activity::SPINNER.len()]);
            (glyph, Style::default().fg(Color::Yellow))
        }
        Status::Done => ("✓", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
        Status::Pending => ("●", Style::default().fg(BRAND_COLOR).add_modifier(Modifier::BOLD)),
        Status::Failed => ("!", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
        Status::Waiting => ("!", Style::default().fg(WAITING_COLOR).add_modifier(Modifier::BOLD)),
    };
    Span::styled(glyph, style)
}

pub(crate) fn outcome_icon(muted: Color, outcome: Outcome) -> Span<'static> {
    let (glyph, colour) = match outcome {
        Outcome::Failed => ("✗", Color::Red),
        Outcome::Pending => ("•", Color::Yellow),
        Outcome::Passed => ("✓", Color::Green),
        Outcome::Skipped => ("-", muted),
    };
    Span::styled(glyph, Style::default().fg(colour))
}

pub(super) fn pull_tag(muted: Color, mark: Mark) -> Span<'static> {
    let number = format!("#{}", mark.number);
    let (text, style) = match (mark.state, mark.ci) {
        (State::Merged, _) => (number, Style::default().fg(BRAND_COLOR)),
        (State::Closed, _) => (number, Style::default().fg(muted).add_modifier(Modifier::CROSSED_OUT)),
        (_, None) => (number, Style::default().fg(muted)),
        (_, Some(ci)) => {
            let outcome = match ci {
                Ci::Failing => Outcome::Failed,
                Ci::Pending => Outcome::Pending,
                Ci::Passing => Outcome::Passed,
            };
            let icon = outcome_icon(muted, outcome);
            (format!("{number}{}", icon.content), icon.style)
        }
    };
    Span::styled(text, style)
}
