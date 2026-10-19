use std::fmt::Write as _;

use super::args::tour::{TourArgs, TourCommand};
use super::{print_json, print_out};
use crate::client::{answer, ask};
use crate::control::Command;
use crate::control::tour::{TourAction, TourAnswer, TourRequest};
use crate::error::Result;

fn request(action: Option<&TourCommand>) -> TourRequest {
    let simple = |action| TourRequest { action, ..TourRequest::default() };
    match action {
        None => TourRequest { action: TourAction::Start, ..TourRequest::default() },
        Some(TourCommand::List) => simple(TourAction::List),
        Some(TourCommand::Start { tour, step, no_card }) => TourRequest {
            action: TourAction::Start,
            tour: tour.clone(),
            step: step.clone(),
            card: Some(!no_card),
        },
        Some(TourCommand::Next) => simple(TourAction::Next),
        Some(TourCommand::Back) => simple(TourAction::Back),
        Some(TourCommand::Skip) => simple(TourAction::Skip),
        Some(TourCommand::End) => simple(TourAction::End),
        Some(TourCommand::Complete { tour, step }) => TourRequest {
            action: TourAction::Complete,
            tour: Some(tour.clone()),
            step: Some(step.clone()),
            card: None,
        },
        Some(TourCommand::Reset { tour }) => {
            TourRequest { action: TourAction::Reset, tour: tour.clone(), ..TourRequest::default() }
        }
    }
}

pub(super) fn run(args: &TourArgs) -> Result<()> {
    let request = request(args.action.as_ref());
    let list = request.action == TourAction::List;
    let value = ask("tour", Command::Tour(request))?;
    let answered: TourAnswer = answer(value.clone())?;
    if args.print.json {
        return print_json(&value);
    }
    print_out(&text(&answered, list))
}

pub(crate) fn text(answer: &TourAnswer, list: bool) -> String {
    let mut text = String::new();
    let state = &answer.state;
    match &state.active {
        Some(active) => {
            let _ = writeln!(text, "on screen: {} step {} ({})", active.tour, active.index + 1, active.step);
        }
        None => text.push_str("no tour on screen\n"),
    }
    if !list {
        return text;
    }
    for tour in &answer.tours {
        let progress = state.progress.iter().find(|p| p.tour == tour.id);
        let done = progress.map_or(0, |p| p.done.len());
        let mark = if progress.is_some_and(|p| p.finished) { " ✓".to_string() } else { format!(" {done}/{}", tour.steps.len()) };
        let _ = writeln!(text, "{}  {}{mark}  {}", tour.id, tour.title, tour.docs);
        for step in &tour.steps {
            let seen = progress.is_some_and(|p| p.done.contains(&step.id));
            let _ = writeln!(text, "  {} {}  {}", if seen { "✓" } else { "·" }, step.id, step.title);
        }
    }
    text
}

#[cfg(test)]
mod tests;
