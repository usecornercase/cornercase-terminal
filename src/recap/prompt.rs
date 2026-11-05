use super::{Choice, Options};
use std::fmt::Write as _;

const COMMON: &str = "Read dossier.md in this folder. It lists a developer's conversations with coding agents, each under an id such as [c3], with the prompts they sent, the agent's last reply, what it waits on, its pull request (ids such as [p1]) and its transcript's path. Read a transcript, or run `cornercase search`, only when the dossier is not enough. Change no file. Quote the developer's prompts literally, in quotes; never paraphrase them. Name every conversation and pull request by its id in square brackets, never by a path. Answer with the recap only, in Markdown with `##` headings and `-` bullets, without a preamble.";

fn kind(choice: Choice, label: &str) -> String {
    match choice {
        Choice::Now => String::from(
            "Write where things stand now. Use the sections `## On you` (a question an agent asked, failing checks, work closed halfway, answers it waits for), then `## In progress`, then `## Done` (since yesterday), leaving out the empty ones. One bullet per conversation, saying what the developer asked (quoted), where it is, and what is on them.",
        ),
        Choice::Yesterday => format!(
            "Write a stand-up for {label}, with the sections `## Done`, `## In progress` and `## On you`. One bullet per conversation, saying what the developer asked (quoted) and how it ended."
        ),
        Choice::Week => String::from(
            "Write the last 7 days per project, one `## <project>` section each: the big themes, what was closed (merged pull requests, by id) and what is still open. Keep little detail per conversation.",
        ),
    }
}

fn language(options: &Options) -> String {
    let wanted = options.language.trim();
    if wanted.is_empty() || wanted.eq_ignore_ascii_case("auto") {
        String::from("the language most of the developer's prompts are written in")
    } else {
        wanted.to_string()
    }
}

pub(crate) fn prompt(choice: Choice, label: &str, options: &Options) -> String {
    let mut text = format!("{COMMON}\n\n{}\n\nWrite in {}.", kind(choice, label), language(options));
    let extra = options.instructions.trim();
    if !extra.is_empty() {
        let _ = write!(text, "\n\nAlso: {extra}");
    }
    text
}

#[cfg(test)]
mod tests;
