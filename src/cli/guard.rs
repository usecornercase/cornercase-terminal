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

fn defaults_here(command: &Command) -> bool {
    let mentions = |text: Option<String>| text.is_some_and(|text| text.contains(crate::cli::help::HERE));
    command.get_arguments().any(|arg| {
        mentions(arg.get_help().map(ToString::to_string)) || mentions(arg.get_long_help().map(ToString::to_string))
    })
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

pub(super) fn refusal(command: &Command, matches: &ArgMatches, own: &str) -> Option<String> {
    let (top, _) = matches.subcommand()?;
    if crate::bots::SERVER_STOPS.contains(&top) {
        return Some(String::from("a bot cannot stop its own server"));
    }
    let mut path = vec![top];
    let (leaf, found) = walk(command.find_subcommand(top)?, matches.subcommand()?.1, &mut path);
    if access_change(&path, found) {
        return Some(String::from("a bot cannot change a bot's access"));
    }
    if path == ["bot", "say"] && found.get_one::<String>("bot").is_some_and(|bot| bot.eq_ignore_ascii_case(own)) {
        return Some(String::from("a bot cannot write to itself"));
    }
    let places = PLACES.iter().chain(["all"].iter());
    if defaults_here(leaf) && !places.into_iter().any(|long| named(leaf, found, long)) {
        let flags = flags(leaf);
        return Some(format!(
            "a bot runs in no pane: name the place with {flags} (cornercase status --json lists the ids)"
        ));
    }
    None
}

#[cfg(test)]
mod tests;
