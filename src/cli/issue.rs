use std::fmt::Write as _;

use super::args::issue::{IssueAction, ListArgs, PeopleArgs, ReadArgs, SourcesArgs};
use super::status::tsv;
use super::{print_json, print_out};
use crate::client::{answer, ask};
use crate::control::issues::{Lists, PeopleInfo, ReadIssue, SourceState, Sources, Who};
use crate::control::{Command, IssueList, IssuePeople, IssueRead, IssueSources};
use crate::error::Result;
use crate::issues::Source;

const LIST_COLUMNS: [&str; 8] = ["source", "key", "state", "author", "updated_at", "title", "url", "branch"];
const PEOPLE_COLUMNS: [&str; 3] = ["handle", "name", "id"];

pub(super) fn run(action: IssueAction) -> Result<bool> {
    match action {
        IssueAction::Sources(args) => sources(&args).map(|()| true),
        IssueAction::List(args) => list(&args),
        IssueAction::Read(args) => read(&args).map(|()| true),
        IssueAction::People(args) => people(&args).map(|()| true),
    }
}

pub(crate) fn who(text: Option<&str>) -> Who {
    match text.map(str::trim) {
        None | Some("" | "anyone") => Who::Anyone,
        Some("me") => Who::Me,
        Some(handle) => Who::Person { handle: handle.trim_start_matches('@').to_string(), id: None },
    }
}

fn sources(args: &SourcesArgs) -> Result<()> {
    let value = ask("issue sources", Command::IssueSources(IssueSources { project: args.project }))?;
    if args.print.json {
        return print_json(&value);
    }
    print_out(&sources_text(&answer(value)?))
}

pub(crate) fn sources_text(sources: &Sources) -> String {
    let mut text = String::new();
    let width = sources.sources.iter().map(|s| s.source.len()).max().unwrap_or(0);
    for source in &sources.sources {
        let state = match source.state {
            SourceState::Connected => "connected",
            SourceState::Missing => "missing",
            SourceState::Failing => "failing",
            SourceState::Other => "unknown",
        };
        let detail = match source.state {
            SourceState::Missing => format!("missing {}", source.missing.join(", ")),
            SourceState::Failing => source.reason.clone().unwrap_or_default(),
            _ => {
                let from = match (source.from.as_deref(), source.env.as_deref()) {
                    (Some("env"), Some(env)) => format!("from {env}"),
                    (Some("gh"), _) => "through gh".into(),
                    _ => String::new(),
                };
                [source.account.clone().unwrap_or_default(), from]
                    .into_iter()
                    .filter(|part| !part.is_empty())
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        };
        let hidden = if source.tab { "" } else { "  (not a tab)" };
        let _ = writeln!(text, "{:width$}  {state:9}  {detail}{hidden}", source.source);
    }
    if !sources.places.is_empty() {
        text.push_str("\nplaces for Shortcut, Linear, Jira, Plane and Asana issues:\n");
        for place in &sources.places {
            let id = place
                .workspace
                .map_or_else(|| format!("project {}", place.project), |workspace| format!("workspace {workspace}"));
            let what = if place.worktree { "a new worktree" } else { "a new tab" };
            let here = if place.here { "  (where you are)" } else { "" };
            let _ = writeln!(text, "  {id}  {}  {what}{here}", place.label);
        }
    }
    text
}

fn list(args: &ListArgs) -> Result<bool> {
    let request = IssueList {
        project: args.project,
        source: args.source.clone(),
        closed: args.closed,
        people: [who(args.assignee.as_deref()), who(args.author.as_deref())],
        cached: args.cached,
    };
    let value = ask("issue list", Command::IssueList(request))?;
    let lists: Lists = answer(value.clone())?;
    if args.print.json {
        print_json(&value)?;
    } else {
        print_out(&tsv(&LIST_COLUMNS, &lists.issues)?)?;
    }
    Ok(failures(&lists))
}

pub(crate) fn failures(lists: &Lists) -> bool {
    let errors = failure_lines(lists);
    for line in &errors {
        eprintln!("{line}");
    }
    errors.is_empty()
}

pub(crate) fn failure_lines(lists: &Lists) -> Vec<String> {
    let name =
        |id: &str| Source::ALL.iter().find(|source| source.id() == id).map_or(id.to_string(), |s| s.name().into());
    let failed = lists.sources.iter().filter_map(|source| Some((&source.source, source.error.as_ref()?)));
    failed.map(|(source, error)| format!("{}: {error}", name(source))).collect()
}

fn read(args: &ReadArgs) -> Result<()> {
    let request = IssueRead { project: args.project, issue: args.issue.clone(), source: args.source.clone() };
    let value = ask("issue read", Command::IssueRead(request))?;
    if args.print.json {
        return print_json(&value);
    }
    print_out(&read_text(&answer(value)?))
}

pub(crate) fn read_text(read: &ReadIssue) -> String {
    let issue = &read.issue;
    let mut text = format!("{}  {}\n", issue.key, issue.title);
    if !read.info.is_empty() {
        let _ = writeln!(text, "{}", read.info.join(" · "));
    }
    let _ = writeln!(text, "{}\nbranch {}\n", issue.url, issue.branch);
    text.push_str(read.body.trim_end());
    text.push('\n');
    for comment in &read.comments {
        let _ = write!(text, "\n@{} · {}\n{}\n", comment.author, comment.created_at, comment.body.trim_end());
    }
    text
}

fn people(args: &PeopleArgs) -> Result<()> {
    let request = IssuePeople { project: args.project, source: args.source.clone(), cached: args.cached };
    let value = ask("issue people", Command::IssuePeople(request))?;
    if args.print.json {
        return print_json(&value);
    }
    let people: PeopleInfo = answer(value)?;
    print_out(&tsv(&PEOPLE_COLUMNS, &people.people)?)
}

#[cfg(test)]
mod tests;
