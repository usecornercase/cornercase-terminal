use std::fmt::Write as _;
use std::path::Path;

use crate::agents::Known;
use crate::agents::headless::BotSupport;
use crate::queue::repeat::Repeat;

use super::{Access, Bot, NOTES_CAP};

const WHO: &str = "You are {name}, a cornercase bot: an assistant that lives outside any project. The user talks to you in a chat in cornercase. Answer briefly, in the user's language, in Markdown.";
const FOLDER: &str = "Your folder is `{folder}`. `bot.md` is what you are for: update it when the user gives you a job or changes one. `memory.md` is what you keep between conversations: add what will matter next time, keep it short and prune it.";
const SEE: &str = "How to see the user's work:
- `cornercase status --json`: projects, workspaces, tabs and agents, with their ids, and each project's and workspace's folder (`path`).
- `cornercase search`, `cornercase recap`, `cornercase worktrees list`, `cornercase read --pane ID`.";
const SEE_GIT: &str = "- A project's git, read-only: `git -C 'PATH' log …`, `git -C 'PATH' show …`, `git -C 'PATH' diff …` and `git -C 'PATH' status`, where PATH is a project's `path` in `cornercase status --json` (or a workspace's, for its own worktree), in single quotes. Your folder is not a repository: `git` without `-C 'PATH'` finds nothing.";
const SEE_GH: &str = "- `gh pr list`, `gh pr view`, `gh pr checks`, `gh issue list` and `gh issue view`, read-only, with `-R` and a project's `repo` from `cornercase status --json`: `gh pr list -R github.com/OWNER/REPO`.";
const ACT: &str = "How to act, through cornercase:
- `cornercase start --project ID [--worktree BRANCH] [--issue KEY] --prompt TEXT` starts an agent in a project. The user sees it like any other agent.
- `cornercase send --pane ID` writes to an agent; `cornercase notify` sends a notice.
- Always name the project: you run in no pane, so there is no \"here\".";
const REACH: &str = "Your access is `cornercase`: you can read anything, run the cornercase command line, run the read-only git and gh above, search the web, and write only in your folder. Run one command per call, or a few joined by `|`, `&&` or `;` when each is allowed on its own: a shell loop (`for`, `while`), `$(…)`, a redirect (`>`) and git's `--output` are refused. Never change a project's files yourself; start an agent to do it.";
const REACH_FOLDER: &str = "Your access is `cornercase`: you can run the cornercase command line and the read-only gh above, search the web, and read and write only in your folder; you cannot run git: see the user's work through cornercase. Run one command per call: a shell loop (`for`, `while`), `$(…)` and a redirect (`>`) are refused. Never change a project's files yourself; start an agent to do it.";
const BOUNDED: &str = "Through cornercase you write prompts to agents: `cornercase send --pane ID --enter` with a text that does not start with `/` or `!`. You cannot press keys or answer an agent's questions (the user does, in its tab), start an agent in a mode that never asks, run flows or plugins, close what you did not start, stop processes, discard changes, remove worktrees, change accounts or machines, or act on another machine.";
const FULL: &str = "Your access is `full`: you can change any file and run anything, unattended. Still start an agent for work on a project, unless the user asked you to do it yourself.";
const SCHEDULE: &str = "To schedule work, run `cornercase queue add --every 'weekday 09:00' --prompt '…' --name '…'` (repeats: `30m`, `2h`, `day 09:00`, `weekday 09:00`, `monday 09:00`, `3d 10:00`; at least 30 minutes), or `--at 'tomorrow 9:00'` for once. `cornercase queue list --bot {id} --json` lists yours; `queue pause`, `resume`, `edit` and `remove` change them. Tell the user what you scheduled and when it runs next, in their time. When a routine runs you start a new conversation that has only this brief and your files, so write in `memory.md` what the next run needs.";
const FOLLOW: &str = "When an agent you started finishes, you get a message with its last message. Tell the user what matters in one or two lines. If the user asked you to go on after it (open a PR, start the next step), do it.";
const NOT_FOLLOWING: &str =
    "You are not told when an agent you started finishes: when the user asks, look with `cornercase status --json`.";
pub(crate) const SECRETS: &str = "When a site or a tool asks for a password or a token, run `cornercase bot secret list` to see the names the user saved for you, and `cornercase bot secret get NAME` to read one. Use it only where it is needed. Never write a secret's value in your answer, in `bot.md` or in `memory.md`: write its name and where it is used. If the secret you need is not saved, run `cornercase bot secret ask NAME --about 'why you need it'`, tell the user what you asked for, and end your answer: you are written to when they save it. Never ask for a password or a token in the chat. If the user writes one there, do not use it: tell them it is now in the chat's file, and to give it in the dialog instead.";
const SETUP: &str = "## You were just made\n\nThe user has just made you, and `bot.md` above is what they wrote you are for. Set yourself up now, in a few steps, and do not do the work itself unless `bot.md` asks for it now. Schedule what it asks for at a time or again and again (`cornercase queue add`, as above), but first look at what is scheduled already (`cornercase queue list --bot {id} --json`): a setup that was cut short may have done some of it. Write in `memory.md` what your runs will need. Then tell the user, in a few lines, what you will do, what you scheduled and when it runs next, and what you need from them, if anything. Your runs can look commands up when they run, so do not read every `--help` now.";
pub(crate) const TOLD_KEPT: usize = 10;
const TOLD_CAP: usize = 16 * 1024;

pub(crate) struct Told {
    pub(crate) label: String,
    pub(crate) clock: String,
    pub(crate) text: String,
}

pub(crate) fn brief(bot: &Bot, folder: &Path) -> String {
    let who = WHO.replace("{name}", &bot.name);
    let folder = FOLDER.replace("{folder}", &folder.display().to_string());
    let access = match bot.access {
        Access::Cornercase if reads_only_folder(bot) => [REACH_FOLDER, BOUNDED].join(" "),
        Access::Cornercase => [REACH, BOUNDED].join(" "),
        Access::Full => FULL.to_string(),
    };
    let schedule = SCHEDULE.replace("{id}", &bot.id);
    let later = if bot.follow { FOLLOW } else { NOT_FOLLOWING };
    [who.as_str(), &folder, &seeing(bot), ACT, &schedule, SECRETS, &access, later].join("\n\n")
}

fn support(bot: &Bot) -> Option<BotSupport> {
    Known::of(&bot.agent).and_then(Known::bot)
}

fn reads_only_folder(bot: &Bot) -> bool {
    support(bot).is_some_and(|support| !support.reads_anywhere)
}

fn seeing(bot: &Bot) -> String {
    let git = bot.access == Access::Full || support(bot).is_some_and(|support| support.reads_git);
    let parts = [Some(SEE), git.then_some(SEE_GIT), Some(SEE_GH)];
    parts.into_iter().flatten().collect::<Vec<_>>().join("\n")
}

#[cfg(test)]
pub(crate) fn taught(text: &str, path: &str) -> Vec<String> {
    let spans = text.split('`').skip(1).step_by(2);
    let commands = spans.filter(|span| span.starts_with("git ") || span.starts_with("gh "));
    commands.map(|span| span.trim_end_matches(" …").replace("PATH", path).replace("OWNER/REPO", "o/r")).collect()
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

fn told_one(answer: &Told) -> String {
    let (text, cut) = crate::queue::outcome::cap(&answer.text);
    let tail = if cut { "\n… (cut)" } else { "" };
    let who =
        if answer.clock.is_empty() { answer.label.clone() } else { format!("{}, {}", answer.label, answer.clock) };
    format!("[{who}]\n{text}{tail}")
}

pub(crate) fn told(answers: &[Told], bot: &str) -> Option<String> {
    if answers.is_empty() {
        return None;
    }
    let mut kept: Vec<String> = Vec::new();
    let mut size = 0;
    for answer in answers.iter().rev() {
        let one = told_one(answer);
        if size + one.len() > TOLD_CAP && !kept.is_empty() {
            break;
        }
        size += one.len();
        kept.push(one);
    }
    kept.reverse();
    let left = answers.len() - kept.len();
    let mut text = format!("## Since your last message, your routines answered\n\n{}", kept.join("\n\n"));
    if left > 0 {
        let _ = write!(
            text,
            "\n\nand {}: `cornercase bot history {bot}`",
            crate::text::plural(left, "older answer", "older answers")
        );
    }
    Some(text)
}

pub(crate) fn noted(notes: &[String]) -> Option<String> {
    if notes.is_empty() {
        return None;
    }
    let lines: Vec<String> = notes.iter().map(|note| format!("- {note}")).collect();
    Some(format!("## Since your last message\n\n{}", lines.join("\n")))
}

pub(crate) fn joined_told(told: Option<String>, noted: Option<String>) -> Option<String> {
    match (told, noted) {
        (Some(told), Some(noted)) => Some(format!("{told}\n\n{noted}")),
        (told, noted) => told.or(noted),
    }
}

pub(crate) fn said(from_cornercase: bool, text: &str) -> String {
    let who = if from_cornercase { "cornercase" } else { "The user" };
    format!("## {who} says\n\n{text}")
}

pub(crate) fn opening(bot: &Bot, folder: &Path, notes: (&str, &str), told: Option<&str>, text: &str) -> String {
    opened(bot, folder, notes, told, text.to_string())
}

pub(crate) fn setup_opening(bot: &Bot, folder: &Path, notes: (&str, &str), told: Option<&str>) -> String {
    opened(bot, folder, notes, told, setup_section(bot))
}

pub(crate) fn setup_section(bot: &Bot) -> String {
    SETUP.replace("{id}", &bot.id)
}

fn opened(bot: &Bot, folder: &Path, notes: (&str, &str), told: Option<&str>, last: String) -> String {
    let mut parts = files(bot, folder, notes);
    parts.extend(told.map(str::to_string));
    parts.push(last);
    parts.join("\n\n")
}

fn files(bot: &Bot, folder: &Path, notes: (&str, &str)) -> Vec<String> {
    let (about, memory) = notes;
    vec![
        brief(bot, folder),
        format!("## bot.md\n\n{}", capped("bot.md", about)),
        format!("## memory.md\n\n{}", capped("memory.md", memory)),
    ]
}

pub(crate) fn resumed(told: Option<&str>, text: &str) -> String {
    match told {
        Some(told) => format!("{told}\n\n{text}"),
        None => text.to_string(),
    }
}

pub(crate) fn run_opening(bot: &Bot, folder: &Path, notes: (&str, &str), text: &str) -> String {
    let mut parts = files(bot, folder, notes);
    parts.push(text.to_string());
    parts.join("\n\n")
}

pub(crate) fn run_text(label: &str, repeat: Option<Repeat>, when: &str, prompt: &str) -> String {
    match repeat {
        Some(repeat) => format!("This is your routine \"{label}\" (every {repeat}). {prompt}"),
        None => format!("This is your reminder \"{label}\" (once, at {when}). {prompt}"),
    }
}

#[cfg(test)]
mod tests;
