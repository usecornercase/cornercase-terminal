use rstest::rstest;

use super::*;

fn options(language: &str, instructions: &str) -> Options {
    Options { language: language.into(), instructions: instructions.into(), ..Options::default() }
}

#[test]
fn the_prompt_carries_the_choice_the_language_and_the_instructions() {
    let text = prompt(Choice::Yesterday, "yesterday · fri 31 oct", &options("Spanish", "group by client"));
    assert!(text.contains("stand-up for yesterday · fri 31 oct"));
    assert!(text.contains("Write in Spanish."));
    assert!(text.ends_with("Also: group by client"));
    assert!(text.contains("dossier.md"));
}

#[test]
fn auto_means_the_language_of_the_prompts() {
    let text = prompt(Choice::Now, "now", &options("auto", ""));
    assert!(text.contains("the language most of the developer's prompts are written in"));
    assert!(!text.contains("Also:"));
}

fn in_order(text: &str, parts: &[&str]) -> bool {
    let mut from = 0;
    parts.iter().all(|part| match text[from..].find(part) {
        Some(at) => {
            from += at + part.len();
            true
        }
        None => false,
    })
}

#[rstest]
#[case::now(
    Choice::Now,
    &["## Needs you", "## In progress", "## Done today"],
    &[NEEDS_HEADING, "at most 15 words", "Closed halfway:"],
    &["## On you"],
)]
#[case::yesterday(
    Choice::Yesterday,
    &["## Done", "## In progress", "## On you"],
    &["One bullet per workspace", "At most 12 bullets"],
    &[],
)]
#[case::week(Choice::Week, &[], &["per project", "Merged:", "Open:", "At most 25 lines"], &[])]
fn each_choice_asks_for_its_sections_in_order_with_their_limits(
    #[case] choice: Choice,
    #[case] sections: &[&str],
    #[case] has: &[&str],
    #[case] lacks: &[&str],
) {
    let text = prompt(choice, "yesterday", &Options::default());
    assert!(in_order(&text, sections), "{text}");
    for part in has {
        assert!(text.contains(part), "{part}");
    }
    for part in lacks {
        assert!(!text.contains(part), "{part}");
    }
}

#[test]
fn quotes_are_the_exception_in_every_choice() {
    for choice in Choice::ALL {
        let text = prompt(choice, "", &Options::default());
        assert!(text.contains("quote a prompt only when it is a few words long"));
        assert!(!text.contains("literally") && !text.contains("never paraphrase"));
    }
}
