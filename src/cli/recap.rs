use std::fmt::Write as _;

use super::args::recap::RecapArgs;
use super::{partial_unless, print_json, print_out};
use crate::client::{answer, ask, use_recap};
use crate::control::{Command, RecapAsk, RecapInfo};
use crate::error::Result;
use crate::recap::text::plain;
use crate::time;

pub(super) fn run(args: &RecapArgs) -> Result<bool> {
    use_recap()?;
    let ask_for = RecapAsk {
        range: args.range.clone(),
        fresh: args.fresh,
        dossier: args.dossier,
        agent: args.agent.clone(),
        account: args.account.clone(),
        model: args.model.clone(),
        effort: args.effort.clone(),
        language: args.language.clone(),
        zone: Some(crate::queue::clock::local_zone()),
    };
    let value = ask("recap", Command::Recap(ask_for))?;
    let info: RecapInfo = answer(value.clone())?;
    if args.print.json {
        print_json(&value)?;
    } else {
        print_out(&text(&info))?;
    }
    partial_unless(info.error.is_none())
}

pub(crate) fn text(info: &RecapInfo) -> String {
    let mut text = String::new();
    if let Some(dossier) = &info.dossier {
        return dossier.clone();
    }
    if let Some(body) = &info.text {
        text.push_str(&heading(info));
        text.push_str(&plain(body, &info.places));
        text.push('\n');
    }
    if let Some(empty) = &info.empty {
        let _ = writeln!(text, "{empty}");
    }
    if let Some(error) = &info.error {
        let _ = writeln!(text, "recap unavailable: {error}");
    }
    text
}

fn heading(info: &RecapInfo) -> String {
    let mut parts = vec![info.label.clone().unwrap_or_else(|| info.range.clone())];
    parts.extend(info.title.clone());
    parts.push(format!("{} conversation{}", info.conversations, if info.conversations == 1 { "" } else { "s" }));
    if let Some(tokens) = info.tokens.filter(|tokens| tokens.total() > 0) {
        parts.push(format!("{} tokens", crate::agents::stats::tokens(tokens.total())));
    }
    if let Some(age) = info.age {
        parts.push(time::ago(std::time::Duration::from_secs(age)));
    }
    format!("{}\n\n", parts.join(" · "))
}

#[cfg(test)]
mod tests;
