use std::path::Path;

use super::{Access, Bot, NOTES_CAP};

const WHO: &str = "You are {name}, a cornercase bot: an assistant that lives outside any project. The user talks to you in a chat in cornercase. Answer briefly, in the user's language, in Markdown.";
const FOLDER: &str = "Your folder is `{folder}`. `bot.md` is what you are for: update it when the user gives you a job or changes one. `memory.md` is what you keep between conversations: add what will matter next time, keep it short and prune it.";
const SEE: &str = "How to see the user's work:
- `cornercase status --json`: projects, workspaces, tabs and agents, with their ids.
- `cornercase search`, `cornercase recap`, `cornercase worktrees list`, `cornercase read --pane ID`.
- `git` and `gh`, read-only.";
const ACT: &str = "How to act, through cornercase:
- `cornercase start --project ID [--worktree BRANCH] [--issue KEY] --prompt TEXT` starts an agent in a project. The user sees it like any other agent.
- `cornercase send --pane ID` writes to an agent; `cornercase notify` sends a notice.
- Always name the project: you run in no pane, so there is no \"here\".";
const REACH: &str = "Your access is `cornercase`: you can read anything, run the cornercase command line, run read-only git and gh, search the web, and write only in your folder. Never change a project's files yourself; start an agent to do it.";
const FULL: &str = "Your access is `full`: you can change any file and run anything, unattended. Still start an agent for work on a project, unless the user asked you to do it yourself.";
const LATER: &str =
    "Routines, and what happens when an agent you started finishes, come in later releases of cornercase.";

pub(crate) fn brief(bot: &Bot, folder: &Path) -> String {
    let who = WHO.replace("{name}", &bot.name);
    let folder = FOLDER.replace("{folder}", &folder.display().to_string());
    let access = match bot.access {
        Access::Cornercase => REACH,
        Access::Full => FULL,
    };
    [who.as_str(), &folder, SEE, ACT, access, LATER].join("\n\n")
}

pub(crate) fn capped(file: &str, text: &str) -> String {
    let text = text.trim();
    if text.is_empty() {
        return String::from("(empty)");
    }
    if text.len() <= NOTES_CAP {
        return text.to_string();
    }
    let mut end = NOTES_CAP;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n… (cut at 8 KB; read {file} for the rest)", &text[..end])
}

pub(crate) fn opening(bot: &Bot, folder: &Path, notes: (&str, &str), text: &str) -> String {
    let (about, memory) = notes;
    let parts = [
        brief(bot, folder),
        format!("## bot.md\n\n{}", capped("bot.md", about)),
        format!("## memory.md\n\n{}", capped("memory.md", memory)),
        format!("## The user says\n\n{text}"),
    ];
    parts.join("\n\n")
}

#[cfg(test)]
mod tests;
