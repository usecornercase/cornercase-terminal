use std::path::Path;

use clap::CommandFactory;

use super::*;
use crate::bots::sample;

fn named(access: Access) -> Bot {
    Bot { access, ..sample("prs", "PRs") }
}

#[test]
fn the_opening_has_the_brief_the_two_files_and_then_the_text() {
    let text = opening(&named(Access::Cornercase), Path::new("/b/prs"), ("review PRs", "likes short"), "hi there");
    let at = |needle: &str| text.find(needle).unwrap_or_else(|| panic!("{needle} in {text}"));
    assert!(at("You are PRs") < at("`/b/prs`"));
    assert!(at("`/b/prs`") < at("## bot.md"));
    assert!(at("## bot.md\n\nreview PRs") < at("## memory.md\n\nlikes short"));
    assert!(at("## memory.md") < at("## The user says\n\nhi there"));
    assert!(text.ends_with("hi there"));
}

#[test]
fn each_access_says_what_it_allows() {
    let reach = brief(&named(Access::Cornercase), Path::new("/b"));
    let full = brief(&named(Access::Full), Path::new("/b"));
    assert!(reach.contains("write only in your folder"), "{reach}");
    assert!(full.contains("change any file and run anything"), "{full}");
    assert!(!full.contains("write only in your folder"));
}

#[test]
fn empty_files_read_empty() {
    assert_eq!(capped("bot.md", "  \n"), "(empty)");
}

#[test]
fn a_long_file_is_cut_at_8_kb_on_a_character_boundary_and_says_so() {
    let long = "é".repeat(5_000);
    let text = capped("memory.md", &long);
    assert!(text.ends_with("… (cut at 8 KB; read memory.md for the rest)"), "{text}");
    let kept = text.split('\n').next().expect("the text");
    assert!(kept.len() <= NOTES_CAP);
    assert!(kept.chars().all(|c| c == 'é'));
    assert_eq!(capped("bot.md", &"a".repeat(NOTES_CAP)).len(), NOTES_CAP);
}

fn commands_named(text: &str) -> Vec<Vec<String>> {
    let mut found = Vec::new();
    for quoted in text.split('`').skip(1).step_by(2) {
        let mut words = quoted.split_whitespace();
        if words.next() != Some("cornercase") {
            continue;
        }
        let path: Vec<String> = words
            .take_while(|word| word.chars().all(|c| c.is_ascii_lowercase() || c == '-') && !word.starts_with('-'))
            .map(str::to_string)
            .collect();
        if !path.is_empty() {
            found.push(path);
        }
    }
    found
}

#[test]
fn every_command_the_brief_names_exists() {
    let mut all = Vec::new();
    for access in [Access::Cornercase, Access::Full] {
        all.extend(commands_named(&brief(&named(access), Path::new("/b"))));
    }
    assert!(all.len() >= 8, "{all:?}");
    for path in all {
        let mut args = vec![String::from("cornercase")];
        args.extend(path.clone());
        args.push(String::from("--help"));
        let error = crate::cli::Cli::command().try_get_matches_from(args).expect_err("help stops the parse");
        assert_eq!(error.kind(), clap::error::ErrorKind::DisplayHelp, "cornercase {}", path.join(" "));
    }
}
