use std::io::Read as _;

use serde_json::Value;

use super::args::bot::{BotAction, EditArgs, HistoryArgs, NewArgs, SayArgs};
use super::{line, note, partial_unless, print_json, print_out};
use crate::bots::MESSAGE_MAX;
use crate::bots::chat::{Kind, Line};
use crate::client::{answer, ask, use_bots};
use crate::control::Command;
use crate::control::bots::{AgentInfo, BotHistory, BotInfo, BotRequest, BotSay, BotTarget, EditBot, NewBot};
use crate::error::{Error, Result};
use crate::queue::clock;

pub(super) use text::event_text;

mod text;

fn target(bot: String) -> BotTarget {
    BotTarget { bot }
}

fn send(request: BotRequest) -> Result<Value> {
    ask("bot", Command::Bot(request))
}

pub(super) fn run(action: BotAction) -> Result<bool> {
    use_bots()?;
    match action {
        BotAction::Agents(print) => agents(print.json),
        BotAction::New(args) => made(&args),
        BotAction::List(print) => list(print.json),
        BotAction::Edit(args) => edited(&args),
        BotAction::Remove(args) => quiet(BotRequest::Remove(target(args.bot))),
        BotAction::Say(args) => said(&args),
        BotAction::History(args) => history(&args),
        BotAction::Reset(args) => quiet(BotRequest::Reset(target(args.bot))),
        BotAction::Stop(args) => stopped(args.bot),
        BotAction::Seen(args) => quiet(BotRequest::Seen(target(args.bot))),
    }
}

fn quiet(request: BotRequest) -> Result<bool> {
    send(request)?;
    Ok(true)
}

fn agents(json: bool) -> Result<bool> {
    let value = send(BotRequest::Agents)?;
    if json {
        print_json(&value)?;
        return Ok(true);
    }
    let found: Vec<AgentInfo> = answer(value["agents"].clone())?;
    print_out(&text::agents(&found))?;
    Ok(true)
}

fn made(args: &NewArgs) -> Result<bool> {
    let new = NewBot {
        name: args.name.clone(),
        agent: args.agent.clone(),
        model: args.model.clone(),
        effort: args.effort.clone(),
        account: args.account.clone(),
        access: args.access.clone(),
        about: args.about.clone().unwrap_or_default(),
    };
    let value = send(BotRequest::New(new))?;
    if args.print.json {
        print_json(&value)?;
    } else {
        print_out(&format!("{}\n", value["id"].as_str().unwrap_or_default()))?;
    }
    Ok(true)
}

fn edited(args: &EditArgs) -> Result<bool> {
    let edit = EditBot {
        bot: args.bot.clone(),
        name: args.name.clone(),
        agent: args.agent.clone(),
        model: args.model.clone(),
        effort: args.effort.clone(),
        account: args.account.clone(),
        access: args.access.clone(),
    };
    let value = send(BotRequest::Edit(edit))?;
    if args.print.json {
        print_json(&value)?;
    }
    Ok(true)
}

fn list(json: bool) -> Result<bool> {
    let value = send(BotRequest::List)?;
    if json {
        print_json(&value)?;
        return Ok(true);
    }
    let bots: Vec<BotInfo> = answer(value["bots"].clone())?;
    print_out(&text::list(&bots, crate::time::now(), &clock::local_zone()))?;
    Ok(true)
}

fn stopped(bot: String) -> Result<bool> {
    let value = send(BotRequest::Stop(target(bot)))?;
    if value["stopped"] == false {
        print_out(&format!("{} was not answering\n", value["bot"].as_str().unwrap_or_default()))?;
    }
    Ok(true)
}

fn message(args: &SayArgs) -> Result<String> {
    let from = match (&args.file, &args.text) {
        (Some(file), _) => file.as_str(),
        (None, Some(text)) => text.as_str(),
        (None, None) => {
            return Err(Error::WrongUsage(String::from("what should the bot be told? give TEXT or --file FILE")));
        }
    };
    if from != "-" && args.file.is_none() {
        return Ok(from.to_string());
    }
    let mut text = String::new();
    if from == "-" {
        line::input().take(4 * MESSAGE_MAX as u64 + 1).read_to_string(&mut text)?;
    } else {
        std::fs::File::open(from)
            .and_then(|file| file.take(4 * MESSAGE_MAX as u64 + 1).read_to_string(&mut text))
            .map_err(|error| Error::WrongUsage(format!("cannot read {from}: {error}")))?;
    }
    Ok(text)
}

fn said(args: &SayArgs) -> Result<bool> {
    let say = BotSay { bot: args.bot.clone(), text: message(args)?, wait: args.wait };
    let value = send(BotRequest::Say(say))?;
    if args.print.json {
        print_json(&value)?;
    } else if !args.wait {
        print_out(&format!("{}\n", value["line"]["id"]))?;
    }
    let line: Line = answer(value["line"].clone())?;
    if !args.wait {
        return Ok(true);
    }
    if !args.print.json && line.kind == Kind::Bot {
        print_out(&format!("{}\n", line.text))?;
    } else if !args.print.json {
        note(&format!("{}\n", line.text));
    }
    partial_unless(line.kind == Kind::Bot)
}

fn history(args: &HistoryArgs) -> Result<bool> {
    let request = BotHistory { bot: args.bot.clone(), last: args.last, after: args.after };
    let value = send(BotRequest::History(request))?;
    if args.print.json {
        print_json(&value)?;
        return Ok(true);
    }
    let lines: Vec<Line> = answer(value["lines"].clone())?;
    let name = value["name"].as_str().unwrap_or(&args.bot);
    print_out(&text::chat(name, &lines, crate::time::now(), &clock::local_zone()))?;
    Ok(true)
}

#[cfg(test)]
mod tests;
