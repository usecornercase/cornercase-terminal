use serde_json::{Value, json};

use super::{ReadArgs, print_json, print_out, say};
use crate::client::ask;
use crate::control::{self, LastMessage};
use crate::error::{Error, Result};

pub(super) fn run(read: &ReadArgs) -> Result<bool> {
    let targets = targets(read)?;
    if targets.len() > 1 {
        let panes: Vec<Value> = targets.iter().map(message).collect();
        let done = panes.iter().all(|pane| pane.get("error").is_none());
        if read.print.json {
            print_json(&json!({ "panes": panes }))?;
        } else {
            print_out(&blocks(&panes))?;
        }
        return Ok(done);
    }
    let LastMessage { pane, tab } = targets.into_iter().next().unwrap_or_default();
    let value = if read.last_message {
        ask("read --last-message", control::Command::LastMessage(LastMessage { pane, tab }))?
    } else {
        let lines = read.lines.and_then(|n| usize::try_from(n).ok());
        ask("read", control::Command::Read(control::Read { pane, tab, lines }))?
    };
    say(value, read.print.json, |done| done.text.clone().into_iter().collect())?;
    Ok(true)
}

fn targets(read: &ReadArgs) -> Result<Vec<LastMessage>> {
    if !read.last_message && read.panes.len() + read.tabs.len() > 1 {
        return Err(Error::WrongUsage("reading several panes needs --last-message".into()));
    }
    let mut targets = Vec::new();
    for target in read
        .panes
        .iter()
        .map(|id| LastMessage { pane: Some(*id), tab: None })
        .chain(read.tabs.iter().map(|id| LastMessage { pane: None, tab: Some(*id) }))
    {
        if !targets.contains(&target) {
            targets.push(target);
        }
    }
    Ok(targets)
}

fn message(target: &LastMessage) -> Value {
    match ask("read --last-message", control::Command::LastMessage(target.clone())) {
        Ok(value) => value,
        Err(error) => {
            let mut value = json!({ "error": error.to_string() });
            if let Some(pane) = target.pane {
                value["pane"] = pane.into();
            } else if let Some(tab) = target.tab {
                value["tab"] = tab.into();
            }
            value
        }
    }
}

fn blocks(panes: &[Value]) -> String {
    panes
        .iter()
        .map(|pane| {
            let (kind, id) = if pane.get("pane").is_some() { ("pane", &pane["pane"]) } else { ("tab", &pane["tab"]) };
            if let Some(error) = pane["error"].as_str() {
                return format!("{kind} {id}\nerror: {error}\n");
            }
            let agent = pane["agent"].as_str().unwrap_or("unknown");
            let turn_over = pane["turn_over"].as_bool().map_or("unknown", |over| if over { "true" } else { "false" });
            let written = pane["written"].as_str().unwrap_or("unknown");
            let text = pane["text"].as_str().unwrap_or_default();
            format!("{kind} {id}  {agent}  turn_over={turn_over}  written={written}\n{text}\n")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use clap::Parser;
    use rstest::rstest;

    use super::*;
    use crate::cli::{Cli, Command, Control};

    fn read(args: &[&str]) -> ReadArgs {
        let cli = Cli::try_parse_from(["cornercase", "read"].into_iter().chain(args.iter().copied())).expect("parse");
        let Some(Command::Control(Control::Read(read))) = cli.command else { panic!("not read") };
        read
    }

    #[test]
    fn panes_come_before_tabs_and_repeated_ids_are_read_once() {
        let read = read(&["--last-message", "--tab", "9", "--pane", "5", "--pane", "4", "--pane", "5", "--tab", "9"]);

        assert_eq!(
            targets(&read).expect("targets"),
            [
                LastMessage { pane: Some(5), tab: None },
                LastMessage { pane: Some(4), tab: None },
                LastMessage { pane: None, tab: Some(9) },
            ]
        );
    }

    #[rstest]
    #[case::panes(&["--pane", "4", "--pane", "5"])]
    #[case::tabs(&["--tab", "4", "--tab", "5"])]
    #[case::both(&["--pane", "4", "--tab", "5"])]
    #[case::repeated(&["--pane", "4", "--pane", "4"])]
    fn several_screens_are_wrong_usage(#[case] args: &[&str]) {
        assert!(matches!(targets(&read(args)), Err(Error::WrongUsage(_))));
    }

    #[test]
    fn messages_and_errors_have_their_own_blocks_with_unknown_metadata_left_clear() {
        let panes = [
            json!({ "pane": 5, "agent": "codex", "turn_over": true, "written": "2026-10-10T12:00:00.000Z", "text": "PR opened.\nCI passed." }),
            json!({ "pane": 6, "error": "no agent" }),
            json!({ "pane": 7, "agent": "claude", "turn_over": false, "text": "Still working." }),
            json!({ "tab": 8, "error": "no tab" }),
            json!({ "pane": 9, "agent": "opencode", "text": "Status unknown." }),
        ];

        assert_eq!(
            blocks(&panes),
            "pane 5  codex  turn_over=true  written=2026-10-10T12:00:00.000Z\nPR opened.\nCI passed.\n\n\
             pane 6\nerror: no agent\n\n\
             pane 7  claude  turn_over=false  written=unknown\nStill working.\n\n\
             tab 8\nerror: no tab\n\n\
             pane 9  opencode  turn_over=unknown  written=unknown\nStatus unknown.\n"
        );
    }
}
