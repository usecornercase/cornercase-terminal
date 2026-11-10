use super::args::Target;
use super::args::held::HeldArgs;
use super::{print_json, print_out};
use crate::client::{answer, ask};
use crate::control::{Command, HeldAction, HeldInfo, HeldRequest};
use crate::error::Result;

pub(super) fn run(args: &HeldArgs) -> Result<()> {
    let Target { pane, tab, .. } = args.target;
    let action = if args.button.send_now { HeldAction::SendNow } else { HeldAction::WaitTurn };
    let value = ask("held", Command::Held(HeldRequest { pane, tab, action }))?;
    if args.print.json {
        return print_json(&value);
    }
    print_out(&text(&answer(value)?))
}

fn text(held: &HeldInfo) -> String {
    let HeldInfo { pane, held_sends, held_queued, after_draft, .. } = *held;
    let pasted = if after_draft {
        format!("the message is in pane {pane} after the person's draft, not sent: their Enter sends both\n")
    } else {
        String::new()
    };
    let count = match held_sends {
        0 => return format!("{pasted}nothing is held in pane {pane} now\n"),
        1 => "1 message".to_string(),
        n => format!("{n} messages"),
    };
    let queued = if held_queued { ", queued to go right after the person's" } else { "" };
    format!("{pasted}{count} held in pane {pane}{queued}\n")
}

#[cfg(test)]
mod tests;
