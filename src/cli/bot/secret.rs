use std::fmt::Write as _;
use std::io::{IsTerminal as _, Read as _};

use serde_json::Value;

use super::{send, target};
use crate::cli::args::bot::{
    SecretAction, SecretAnswerArgs, SecretAskArgs, SecretGetArgs, SecretListArgs, SecretRemoveArgs, SecretSetArgs,
};
use crate::cli::{config, guard, print_json, print_out};
use crate::client::{self, use_bot_asks, use_bot_secrets};
use crate::control::bots::{BotRequest, SecretAnswer, SecretAsk, SecretGet, SecretGive, SecretRemove, SecretSet};
use crate::error::{Error, Result};
use crate::queue::clock;

const INPUT_MAX: u64 = (1 << 16) + 1;
const VALUES_MAX: u64 = 256 * 1024;
const NOT_AN_OBJECT: &str = "standard input is not a JSON object of name to value";

pub(super) fn run(action: SecretAction) -> Result<bool> {
    use_bot_secrets()?;
    match action {
        SecretAction::Set(args) => set(&args),
        SecretAction::List(args) => list(&args),
        SecretAction::Remove(args) => remove(&args),
        SecretAction::Get(args) => get(&args),
        SecretAction::Ask(args) => ask(&args),
        SecretAction::Give(args) => give(&args),
        SecretAction::Decline(args) => decline(&args),
    }
}

fn piped(bot: &str, name: &str) -> String {
    let line = match client::used_remote() {
        Some(remote) => remote.line(&format!("bot secret set {bot} {name}")),
        None => format!("cornercase bot secret set {bot} {name}"),
    };
    format!("printf '%s' \"$VALUE\" | {line}")
}

pub(super) fn without_one_break(mut text: String) -> String {
    if text.ends_with('\n') {
        text.pop();
        if text.ends_with('\r') {
            text.pop();
        }
    }
    text
}

fn value_of(args: &SecretSetArgs) -> Result<String> {
    let prompt = format!("{}: ", args.name);
    let text =
        config::read_hidden("value", &prompt, (&piped(&args.bot, &args.name), INPUT_MAX)).map_err(
            |error| match error {
                Error::Io(io) if io.kind() == std::io::ErrorKind::InvalidData => {
                    Error::WrongUsage(String::from("the value is not UTF-8 text"))
                }
                other => other,
            },
        )?;
    if text.len() as u64 >= INPUT_MAX {
        return Err(Error::WrongUsage(format!(
            "the value is over {} bytes; a secret takes at most 8,192",
            INPUT_MAX - 1
        )));
    }
    Ok(without_one_break(text))
}

fn set(args: &SecretSetArgs) -> Result<bool> {
    let value = crate::issues::Secret(value_of(args)?);
    let set = SecretSet { bot: args.bot.clone(), name: args.name.clone(), value };
    let answer = send(BotRequest::SecretSet(set))?;
    if args.print.json {
        print_json(&answer)?;
    } else {
        print_out(&format!("{}\n", args.name))?;
    }
    Ok(true)
}

fn own(bot: Option<&String>) -> Result<String> {
    bot.cloned()
        .or_else(guard::own_bot)
        .ok_or_else(|| Error::WrongUsage(String::from("name the bot: cornercase bot secret list BOT")))
}

fn listing(answer: &Value) -> String {
    let id = answer["bot"].as_str().unwrap_or_default();
    let Some(secrets) = answer["secrets"].as_array().filter(|secrets| !secrets.is_empty()) else {
        return format!("{id} has no secrets\n");
    };
    let (now, zone) = (crate::time::now(), clock::local_zone());
    let mut text = String::new();
    for secret in secrets {
        let saved = clock::when(secret["saved"].as_i64().unwrap_or_default(), now, &zone);
        let _ = writeln!(text, "{}\tsaved {saved}", secret["name"].as_str().unwrap_or_default());
    }
    let _ = writeln!(text, "key: {}", answer["key"].as_str().unwrap_or("file"));
    text
}

fn list(args: &SecretListArgs) -> Result<bool> {
    let answer = send(BotRequest::SecretList(target(own(args.bot.as_ref())?)))?;
    if args.print.json {
        print_json(&answer)?;
    } else {
        print_out(&listing(&answer))?;
    }
    Ok(true)
}

fn remove(args: &SecretRemoveArgs) -> Result<bool> {
    let request = SecretRemove { bot: args.bot.clone(), name: args.name.clone(), all: args.all };
    let answer = send(BotRequest::SecretRemove(request))?;
    if args.print.json {
        print_json(&answer)?;
    } else if args.all {
        let count = answer["removed"].as_u64().unwrap_or_default();
        print_out(&format!("removed {count} secret{}\n", if count == 1 { "" } else { "s" }))?;
    }
    Ok(true)
}

fn get(args: &SecretGetArgs) -> Result<bool> {
    let answer = send(BotRequest::SecretGet(SecretGet { name: args.name.clone() }))?;
    if args.print.json {
        print_json(&answer)?;
    } else {
        print_out(&format!("{}\n", answer["value"].as_str().unwrap_or_default()))?;
    }
    Ok(true)
}

fn ask(args: &SecretAskArgs) -> Result<bool> {
    use_bot_asks()?;
    let request = SecretAsk { names: args.names.clone(), about: args.about.clone(), replace: args.replace };
    let answer = send(BotRequest::SecretAsk(request))?;
    if args.print.json {
        print_json(&answer)?;
    } else {
        print_out(&asked_line(&answer))?;
    }
    Ok(true)
}

fn asked_line(answer: &Value) -> String {
    let names: Vec<&str> = answer["names"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
    let already = if answer["already"].as_bool() == Some(true) { "already asked" } else { "asked" };
    format!("{already}: {} (ask {})\n", names.join(", "), answer["ask"].as_u64().unwrap_or_default())
}

pub(super) fn values_of(text: &str) -> Result<std::collections::BTreeMap<String, crate::issues::Secret>> {
    let not_an_object = || Error::WrongUsage(String::from(NOT_AN_OBJECT));
    let Ok(Value::Object(map)) = serde_json::from_str::<Value>(text) else { return Err(not_an_object()) };
    map.into_iter()
        .map(|(name, value)| match value {
            Value::String(value) => Ok((name, crate::issues::Secret(value))),
            _ => Err(not_an_object()),
        })
        .collect()
}

fn read_values(args: &SecretAnswerArgs) -> Result<String> {
    if std::io::stdin().is_terminal() {
        return Err(Error::WrongUsage(format!(
            "pipe the values in as one JSON object: printf '%s' '{{\"NAME\":\"VALUE\"}}' | cornercase bot secret give {} {}",
            args.bot, args.ask
        )));
    }
    let mut text = String::new();
    crate::cli::line::input()
        .take(VALUES_MAX + 1)
        .read_to_string(&mut text)
        .map_err(|_| Error::WrongUsage(String::from(NOT_AN_OBJECT)))?;
    if text.len() as u64 > VALUES_MAX {
        return Err(Error::WrongUsage(format!("standard input is over {VALUES_MAX} bytes")));
    }
    Ok(text)
}

fn give(args: &SecretAnswerArgs) -> Result<bool> {
    use_bot_asks()?;
    let values = values_of(&read_values(args)?)?;
    let answer = send(BotRequest::SecretGive(SecretGive { bot: args.bot.clone(), ask: args.ask, values }))?;
    if args.print.json {
        print_json(&answer)?;
    } else {
        let saved = answer["saved"].as_array().into_iter().flatten().filter_map(Value::as_str);
        let text = saved.fold(String::new(), |mut text, name| {
            let _ = writeln!(text, "{name}");
            text
        });
        print_out(&text)?;
    }
    Ok(true)
}

fn decline(args: &SecretAnswerArgs) -> Result<bool> {
    use_bot_asks()?;
    let answer = send(BotRequest::SecretDecline(SecretAnswer { bot: args.bot.clone(), ask: args.ask }))?;
    if args.print.json {
        print_json(&answer)?;
    }
    Ok(true)
}

#[cfg(test)]
mod tests;
