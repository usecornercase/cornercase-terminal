use std::io::{self, BufRead as _};

use super::args::terminal::{InputArgs, LinkArgs, RatioArgs, ResizeArgs, ScrollArgs, SelectionArgs, Side};
use super::{print_json, print_out};
use crate::client::{answer, ask};
use crate::control::terminal::{End, InputEvent, Linked, Scrolled, Selection, Sent, Why};
use crate::control::{self, Done};
use crate::error::{Error, Result};

fn events(args: &[String]) -> Result<Vec<InputEvent>> {
    let mut events = Vec::new();
    for text in args {
        if text == "-" {
            for line in io::stdin().lock().lines() {
                let line = line?;
                if !line.trim().is_empty() {
                    events.push(event(&line)?);
                }
            }
        } else {
            events.push(event(text)?);
        }
    }
    Ok(events)
}

fn event(text: &str) -> Result<InputEvent> {
    serde_json::from_str(text).map_err(|e| Error::WrongUsage(format!("not an input event: {e}: {text}")))
}

fn skipped(why: Why) -> &'static str {
    match why {
        Why::Mouse => "the program does not read the mouse that way",
        Why::Release => "the program does not take key releases",
        Why::Key => "cornercase has no encoding for this key",
    }
}

pub(super) fn input(args: &InputArgs) -> Result<()> {
    let request = control::Input { pane: Some(args.pane), size: None, events: events(&args.events)? };
    let value = ask("input", control::Command::Input(request))?;
    if args.print.json {
        return print_json(&value);
    }
    for skip in answer::<Sent>(value)?.skipped {
        eprintln!("warning: event {} was not sent: {}", skip.event + 1, skipped(skip.why));
    }
    Ok(())
}

pub(super) fn scroll(args: &ScrollArgs) -> Result<()> {
    let lines = args.up.map(|rows| -rows).or(args.down);
    let to = if args.top {
        Some(End::Top)
    } else if args.bottom {
        Some(End::Bottom)
    } else {
        None
    };
    let value = ask("scroll", control::Command::Scroll(control::Scroll { pane: Some(args.pane), lines, to }))?;
    if args.print.json {
        return print_json(&value);
    }
    answer::<Scrolled>(value).map(drop)
}

pub(super) fn selection(args: &SelectionArgs) -> Result<()> {
    let request = Selection {
        pane: Some(args.pane),
        size: None,
        from: args.from,
        to: args.to,
        rectangle: args.rectangle,
        clear: args.clear,
        copy: args.copy,
    };
    let value = ask("selection", control::Command::Selection(request))?;
    if args.print.json {
        return print_json(&value);
    }
    match answer::<Done>(value)?.text {
        Some(text) => print_out(&format!("{text}\n")),
        None => Ok(()),
    }
}

pub(super) fn resize(args: &ResizeArgs) -> Result<()> {
    let probe = control::Surface { tab: None, cols: args.cols, rows: args.rows };
    probe.area().map_err(Error::Control)?;
    let request = control::Resize { surface: Some(args.surface), cols: args.cols, rows: args.rows };
    let value = ask("resize", control::Command::Resize(request))?;
    if args.print.json { print_json(&value) } else { Ok(()) }
}

pub(super) fn ratio(args: &RatioArgs) -> Result<()> {
    let path = args.path.iter().map(|side| *side == Side::Second).collect();
    let request = control::Ratio { tab: Some(args.tab), path, ratio: args.ratio };
    let value = ask("ratio", control::Command::Ratio(request))?;
    if args.print.json { print_json(&value) } else { Ok(()) }
}

pub(super) fn link(args: &LinkArgs) -> Result<()> {
    let request = control::Link { pane: Some(args.pane), size: None, cell: Some(args.cell) };
    let value = ask("link", control::Command::Link(request))?;
    if args.print.json {
        return print_json(&value);
    }
    match answer::<Linked>(value)?.link {
        Some(link) => print_out(&format!("{}\n", link.location())),
        None => Ok(()),
    }
}
