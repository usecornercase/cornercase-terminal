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

#[test]
fn each_choice_asks_for_its_own_format() {
    assert!(prompt(Choice::Now, "", &Options::default()).contains("## On you"));
    assert!(prompt(Choice::Week, "", &Options::default()).contains("per project"));
}
