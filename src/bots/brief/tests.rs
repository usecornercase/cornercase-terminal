use std::path::Path;

use clap::CommandFactory;

use super::*;
use crate::bots::sample;
use crate::queue::repeat::Repeat;

fn named(access: Access) -> Bot {
    Bot { access, ..sample("prs", "PRs") }
}

#[test]
fn the_opening_has_the_brief_the_two_files_and_then_the_text() {
    let text = opening(
        &named(Access::Cornercase),
        Path::new("/b/prs"),
        ("review PRs", "likes short"),
        None,
        &said(false, "hi there"),
    );
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
fn a_bot_whose_agent_reads_only_its_folder_is_told_so() {
    let gemini = Bot { agent: String::from("gemini"), ..named(Access::Cornercase) };
    let told = brief(&gemini, Path::new("/b"));
    assert!(told.contains("read and write only in your folder"), "{told}");
    assert!(!told.contains("read anything"), "{told}");
    let claude = brief(&named(Access::Cornercase), Path::new("/b"));
    assert!(claude.contains("read anything"), "{claude}");
    let full = Bot { access: Access::Full, ..gemini };
    assert!(!brief(&full, Path::new("/b")).contains("only in your folder"));
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
        all.extend(commands_named(&setup_section(&named(access))));
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

#[test]
fn a_setup_opening_has_the_brief_the_two_files_and_the_setup_section_last() {
    let bot = named(Access::Cornercase);
    let text = setup_opening(&bot, Path::new("/b/prs"), ("review PRs", "likes short"), None);
    let at = |needle: &str| text.find(needle).unwrap_or_else(|| panic!("{needle} in {text}"));
    assert!(at("## bot.md\n\nreview PRs") < at("## memory.md\n\nlikes short"));
    assert!(at("## memory.md") < at("## You were just made"));
    assert!(text.ends_with(&setup_section(&bot)), "{text}");
    assert!(text.contains("cornercase queue list --bot prs --json"));
    assert!(!text.contains("## The user says"));
    let told = setup_opening(&bot, Path::new("/b/prs"), ("a", "b"), Some("## Since your last message\nx"));
    assert!(told.find("## Since your last message") < told.find("## You were just made"));
}

#[test]
fn the_brief_teaches_scheduling_with_the_bots_own_id() {
    let text = brief(&named(Access::Cornercase), Path::new("/b"));
    assert!(text.contains("cornercase queue add --every 'weekday 09:00'"), "{text}");
    assert!(text.contains("cornercase queue list --bot prs --json"), "{text}");
    assert!(!text.contains("Routines, and what happens"), "{text}");
    assert!(text.contains("When an agent you started finishes, you get a message with its last message"), "{text}");
}

fn flags_named(text: &str) -> Vec<(Vec<String>, String)> {
    let mut found = Vec::new();
    for quoted in text.split('`').skip(1).step_by(2) {
        let Some(rest) = quoted.strip_prefix("cornercase ") else { continue };
        let words: Vec<&str> = rest.split_whitespace().collect();
        let path: Vec<String> = words
            .iter()
            .take_while(|word| word.chars().all(|c| c.is_ascii_lowercase() || c == '-') && !word.starts_with('-'))
            .map(|word| (*word).to_string())
            .collect();
        for word in &words[path.len()..] {
            if let Some(flag) = word.trim_matches(['[', ']']).strip_prefix("--") {
                let flag: String = flag.chars().take_while(|c| c.is_ascii_lowercase() || *c == '-').collect();
                found.push((path.clone(), flag));
            }
        }
    }
    found
}

#[test]
fn every_flag_the_brief_names_exists_on_its_command() {
    let bot = named(Access::Cornercase);
    let mut flags = flags_named(&brief(&bot, Path::new("/b")));
    flags.extend(flags_named(&setup_section(&bot)));
    assert!(flags.iter().any(|(path, flag)| path == &["queue", "list"] && flag == "bot"), "{flags:?}");
    for (path, flag) in flags {
        let command = crate::cli::Cli::command();
        let mut command = &command;
        for name in &path {
            command = command.find_subcommand(name).unwrap_or_else(|| panic!("cornercase {}", path.join(" ")));
        }
        let known = command.get_arguments().any(|arg| arg.get_long() == Some(flag.as_str()));
        assert!(known, "cornercase {} has no --{flag}", path.join(" "));
    }
}

fn answer(label: &str, text: &str) -> Told {
    Told { label: label.into(), clock: "09:00".into(), text: text.into() }
}

#[test]
fn a_run_opening_has_the_brief_the_two_files_and_the_routines_text_but_no_user_heading() {
    let text = run_opening(
        &named(Access::Cornercase),
        Path::new("/b/prs"),
        ("review", "notes"),
        &run_text("Review PRs", Repeat::parse("weekday 09:00", "UTC", 0).ok(), "09:00", "Check."),
    );
    assert!(text.contains("## memory.md\n\nnotes"));
    assert!(text.ends_with(r#"This is your routine "Review PRs" (every weekday 09:00). Check."#), "{text}");
    assert!(!text.contains("The user says"));
    assert_eq!(run_text("Once", None, "Mon 9:00", "Do."), r#"This is your reminder "Once" (once, at Mon 9:00). Do."#);
}

#[test]
fn what_the_routines_answered_goes_in_front_of_the_message_and_is_capped() {
    assert_eq!(told(&[], "prs"), None);
    let one = told(&[answer("Review PRs", "Two are blocked.")], "prs").expect("told");
    assert!(
        one.starts_with("## Since your last message, your routines answered\n\n[Review PRs, 09:00]\nTwo are blocked.")
    );
    assert_eq!(resumed(Some(&one), &said(false, "ok")), format!("{one}\n\n## The user says\n\nok"));
    assert_eq!(resumed(None, "ok"), "ok");
    let opened = opening(&named(Access::Cornercase), Path::new("/b"), ("a", "b"), Some(&one), &said(false, "go"));
    assert!(opened.find("## memory.md").expect("memory") < opened.find("## Since your last").expect("told"));
    assert!(opened.find("## Since your last").expect("told") < opened.find("## The user says").expect("says"));
}

#[test]
fn a_long_answer_and_a_long_list_of_answers_are_capped() {
    let long = told(&[answer("Big", &"x".repeat(9_000))], "prs").expect("told");
    assert!(long.contains("… (cut)") && long.len() < 5_000, "{}", long.len());
    let many: Vec<Told> = (0..10).map(|i| answer(&format!("Run {i}"), &"y".repeat(4_000))).collect();
    let capped = told(&many, "prs").expect("told");
    assert!(capped.len() <= TOLD_CAP + 400, "{}", capped.len());
    assert!(capped.contains("[Run 9, 09:00]") && !capped.contains("[Run 0, 09:00]"));
    assert!(capped.contains("older answers: `cornercase bot history prs`"), "{capped}");
}

#[test]
fn the_brief_says_how_it_hears_of_agents_and_not_when_follow_is_off() {
    let on = brief(&named(Access::Cornercase), Path::new("/b"));
    assert!(on.contains("When an agent you started finishes, you get a message"), "{on}");
    let off = brief(&Bot { follow: false, ..named(Access::Cornercase) }, Path::new("/b"));
    assert!(off.contains("You are not told when an agent you started finishes"), "{off}");
    assert!(!off.contains("you get a message"), "{off}");
}

#[test]
fn a_report_is_headed_cornercase_says() {
    assert_eq!(said(true, "an agent finished"), "## cornercase says\n\nan agent finished");
    assert_eq!(said(false, "hi"), "## The user says\n\nhi");
}

mod git {
    use super::*;

    fn on(agent: &str, access: Access) -> String {
        brief(&Bot { agent: agent.to_string(), ..named(access) }, Path::new("/b"))
    }

    #[test]
    fn the_brief_names_the_projects_path_and_teaches_git_with_it() {
        let claude = on("claude", Access::Cornercase);
        for wanted in ["`path`", "git -C 'PATH' log …", "-R"] {
            assert!(claude.contains(wanted), "{wanted} in {claude}");
        }
        assert!(on("gemini", Access::Full).contains("git -C 'PATH' log …"));
    }

    #[test]
    fn the_access_says_one_command_per_call_and_no_loops() {
        for told in [REACH, REACH_FOLDER] {
            assert!(told.contains("one command per call") || told.contains("One command per call"), "{told}");
            assert!(told.contains("`for`"), "{told}");
        }
    }

    #[rstest::rstest]
    #[case::gemini("gemini")]
    #[case::copilot("copilot")]
    fn an_agent_that_cannot_run_git_is_not_taught_it(#[case] agent: &str) {
        let told = on(agent, Access::Cornercase);
        assert!(!told.contains("git -C"), "{told}");
        assert!(told.contains("you cannot run git"), "{told}");
    }
}
