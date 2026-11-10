use super::dossier::NEEDS_HEADING;
use super::{Choice, Options};
use std::fmt::Write as _;

const COMMON: &str = "Read dossier.md in this folder. It lists a developer's conversations with coding agents, each under an id such as [c3], with its place, its state, its latest prompts, the agent's last reply, what it waits on and its pull request (ids such as [p1]). Read a transcript, or run `cornercase search`, only when the dossier is not enough. Change no file. The recap is read at a glance in a narrow side panel, so keep it short. Start a bullet about a conversation or a pull request with its id in square brackets: the panel shows it as the place's name, so do not repeat the project or the workspace. Never name anything by a path. Say what was asked in a few words of your own; quote a prompt only when it is a few words long and is the clearest way to say it. Answer with the recap only, in Markdown with `##` headings and `-` bullets, without a preamble or a closing line.";

const YESTERDAY: &str = "One bullet per workspace (a line of work), not per conversation, starting with the id of its latest conversation or of its pull request, and saying in one sentence of at most 15 words what was done, where it is or what it waits on. At most 12 bullets in all.";

const WEEK: &str = "Write the last 7 days per project, one `## <project>` section each, the busiest first, leaving out a project with nothing worth a bullet. In each: 2 or 3 bullets for its themes (1 for a small project), each in at most 15 words; then, when pull requests were merged, a bullet `Merged:` with their ids only, such as `- Merged: [p1] [p4]`; then a bullet `Open:` saying in at most 15 words what is still open. At most 25 lines in all.";

fn now() -> String {
    format!(
        "Write where things stand now, in these sections and in this order, leaving out an empty one. `## Needs you`: one bullet for each item of the dossier's `{NEEDS_HEADING}` list, and nothing else, saying in at most 15 words what it waits on (the question asked, the check that fails); a conversation the dossier gives as closed never goes here. `## In progress`: one bullet per workspace with an open conversation that is not in Needs you, by the id of its latest conversation, saying in at most 12 words what is being done and how far it got; at most 6 bullets. `## Done today`: one bullet per workspace or pull request whose work was finished in the dossier's range, in at most 10 words; at most 6 bullets, and the rest in one last bullet such as `- 3 more in api and web`. If conversations were closed while still working or waiting, end with one line that is not a section: `Closed halfway:` and their ids, at most 5."
    )
}

fn kind(choice: Choice, label: &str) -> String {
    match choice {
        Choice::Now => now(),
        Choice::Yesterday => format!(
            "Write a stand-up for {label}, with the sections `## Done`, `## In progress` and `## On you`, in this order, leaving out an empty one. {YESTERDAY}"
        ),
        Choice::Week => String::from(WEEK),
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
