use std::fmt::Write as _;

use serde_json::Value;

use super::{send, target};
use crate::cli::args::bot::{SecretAction, SecretGetArgs, SecretListArgs, SecretRemoveArgs, SecretSetArgs};
use crate::cli::{config, guard, print_json, print_out};
use crate::client::{self, use_bot_secrets};
use crate::control::bots::{BotRequest, SecretGet, SecretRemove, SecretSet};
use crate::error::{Error, Result};
use crate::queue::clock;

const INPUT_MAX: u64 = (1 << 16) + 1;

pub(super) fn run(action: SecretAction) -> Result<bool> {
    use_bot_secrets()?;
    match action {
        SecretAction::Set(args) => set(&args),
        SecretAction::List(args) => list(&args),
        SecretAction::Remove(args) => remove(&args),
        SecretAction::Get(args) => get(&args),
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

#[cfg(test)]
mod tests;
