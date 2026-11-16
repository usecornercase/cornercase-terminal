use clap::parser::ValueSource;
use clap::{ArgMatches, Command};

const PLACES: [&str; 6] = ["workspace", "project", "pane", "tab", "group", "path"];

fn walk<'a>(command: &'a Command, matches: &'a ArgMatches, path: &mut Vec<&'a str>) -> (&'a Command, &'a ArgMatches) {
    let next = matches.subcommand().and_then(|(name, sub)| command.find_subcommand(name).map(|found| (found, sub)));
    match next {
        Some((found, sub)) => {
            path.push(found.get_name());
            walk(found, sub, path)
        }
        None => (command, matches),
    }
}

fn place_flag<'a>(command: &'a Command, long: &str) -> Option<&'a clap::Arg> {
    command.get_arguments().find(|arg| arg.get_long() == Some(long))
}

fn named(command: &Command, matches: &ArgMatches, long: &str) -> bool {
    place_flag(command, long)
        .is_some_and(|arg| matches.value_source(arg.get_id().as_str()) == Some(ValueSource::CommandLine))
}

fn arg_says(arg: &clap::Arg, words: &str) -> bool {
    let mentions = |text: Option<String>| text.is_some_and(|text| text.contains(words));
    mentions(arg.get_help().map(ToString::to_string)) || mentions(arg.get_long_help().map(ToString::to_string))
}

fn says(command: &Command, words: &str) -> bool {
    command.get_arguments().any(|arg| arg_says(arg, words))
}

fn defaults_here(command: &Command) -> bool {
    says(command, crate::cli::help::HERE)
}

fn flags(command: &Command) -> String {
    let listed: Vec<String> = PLACES
        .iter()
        .filter_map(|long| place_flag(command, long))
        .map(|arg| {
            let value = arg.get_value_names().and_then(|names| names.first()).map_or("ID", |name| name.as_str());
            format!("--{} {value}", arg.get_long().unwrap_or_default())
        })
        .collect();
    listed.join(" or ")
}

fn access_change(path: &[&str], matches: &ArgMatches) -> bool {
    match path {
        ["bot", "edit"] => matches.value_source("access") == Some(ValueSource::CommandLine),
        ["bot", "new"] => matches.get_one::<String>("access").is_some_and(|access| access == "full"),
        _ => false,
    }
}

fn stops(top: &str) -> Option<String> {
    crate::bots::SERVER_STOPS.iter().any(|stop| stop.command == top).then(|| String::from(crate::bots::STOP_REFUSED))
}

fn access(path: &[&str], found: &ArgMatches) -> Option<String> {
    access_change(path, found).then(|| String::from(crate::bots::ACCESS_CHANGE))
}

fn itself(path: &[&str], found: &ArgMatches, own: &str) -> Option<String> {
    let said =
        path == ["bot", "say"] && found.get_one::<String>("bot").is_some_and(|bot| bot.eq_ignore_ascii_case(own));
    said.then(|| String::from("a bot cannot write to itself"))
}

fn writes_a_file(leaf: &Command, found: &ArgMatches) -> Option<String> {
    let help = crate::cli::help::TO_FILE;
    let arg = leaf.get_arguments().find(|arg| {
        let given = found.value_source(arg.get_id().as_str()) == Some(ValueSource::CommandLine);
        given && arg_says(arg, help)
    })?;
    let long = arg.get_long().unwrap_or_default();
    Some(format!("a bot cannot have cornercase write to a file: leave out --{long} to print it instead"))
}

fn window(path: &[&str], found: &ArgMatches) -> Option<String> {
    let why = match path {
        ["proxy"] => crate::bots::WINDOW_REFUSED,
        ["remote"] if found.get_one::<String>("destination").is_some() => crate::bots::WINDOW_REFUSED,
        ["server"] => "a bot cannot run cornercase's server",
        _ => return None,
    };
    Some(String::from(why))
}

fn in_no_place(leaf: &Command, found: &ArgMatches) -> Option<String> {
    let places = PLACES.iter().chain(["all"].iter());
    if !defaults_here(leaf) || places.into_iter().any(|long| named(leaf, found, long)) {
        return None;
    }
    let flags = flags(leaf);
    Some(format!("a bot runs in no pane: name the place with {flags} (cornercase status --json lists the ids)"))
}

pub(super) fn refusal(command: &Command, matches: &ArgMatches, own: &str) -> Option<String> {
    let (top, _) = matches.subcommand()?;
    if let Some(stopped) = stops(top) {
        return Some(stopped);
    }
    let mut path = vec![top];
    let (leaf, found) = walk(command.find_subcommand(top)?, matches.subcommand()?.1, &mut path);
    access(&path, found)
        .or_else(|| window(&path, found))
        .or_else(|| itself(&path, found, own))
        .or_else(|| writes_a_file(leaf, found))
        .or_else(|| in_no_place(leaf, found))
}

pub(super) fn run_refusal(command: &Command, matches: &ArgMatches, own: &str) -> Option<String> {
    match matches.subcommand() {
        None => Some(String::from(crate::bots::WINDOW_REFUSED)),
        Some(_) => refusal(command, matches, own),
    }
}

pub(super) fn own_bot() -> Option<String> {
    std::env::var(crate::bots::BOT_ENV).ok().filter(|id| !id.is_empty())
}

#[cfg(test)]
mod tests;
