use std::collections::BTreeMap;
use std::fmt::Write as _;

use super::noise::{fold_paste, trim_to};
use super::{Caps, Dossier, Group, Live, NEEDS_HEADING, Pulled, Talk, project_of, pull_of};
use crate::recap::range::{Range, stamp};
use crate::recap::{Choice, NEEDS_YOU, Place, failing, needs_you};

type Order = (u8, String, String, i64);

pub(super) fn order(group: &Group, choice: Choice) -> Order {
    let needs = group.live.as_ref().is_some_and(|pane| needs_you(pane.status.as_deref()));
    let rank = u8::from(!(choice == Choice::Now && needs));
    let project = project_of(group).map(|named| named.label.clone()).unwrap_or_default();
    let workspace = group.template.workspace.as_ref().map(|w| w.label.clone()).unwrap_or_default();
    let time = if choice == Choice::Now { -group.last } else { group.first };
    (rank, project, workspace, time)
}

struct Ids {
    places: BTreeMap<String, Place>,
    pulls: Vec<(String, u64, String)>,
}

impl Ids {
    fn pull(&mut self, workspace: &str, number: u64, place: Place) -> String {
        if let Some((id, _, _)) = self.pulls.iter().find(|(_, n, w)| *n == number && w == workspace) {
            return id.clone();
        }
        let id = format!("p{}", self.pulls.len() + 1);
        self.pulls.push((id.clone(), number, workspace.to_string()));
        self.places.insert(id.clone(), place);
        id
    }
}

fn place_of(group: &Group) -> Place {
    let template = &group.template;
    Place {
        kind: "conversation".into(),
        project: template.project.clone(),
        workspace: template
            .workspace
            .as_ref()
            .map(|w| crate::recap::Named { label: w.label.clone(), path: w.path.clone() }),
        branch: template.workspace.as_ref().and_then(|w| w.branch.clone()),
        tab: template.tab.clone(),
        agent: Some(template.agent.clone()),
        conversation: template.conversation.clone(),
        pane: template.pane,
        boot: template.boot.clone(),
        status: group.live.as_ref().map(|pane| pane.status.clone().unwrap_or_else(|| "open".into())),
        ..Place::default()
    }
}

fn state(group: &Group, zone: &str) -> String {
    if let Some(pane) = &group.live {
        let status = pane.status.as_deref().unwrap_or("open");
        return format!("now: {status} since {}", stamp(group.last, zone));
    }
    let while_ = group.last_status.as_deref().map(|status| format!(" while {status}")).unwrap_or_default();
    match group.ended {
        Some(at) => format!("closed at {}{while_}", stamp(at, zone)),
        None => format!("closed, last seen {}{while_}", stamp(group.last, zone)),
    }
}

fn heading(id: &str, group: &Group) -> String {
    let place = place_of(group);
    let name = place.name();
    let name = if name.is_empty() { "conversation".to_string() } else { name };
    format!("## [{id}] {name}\n")
}

fn facts(group: &Group, range: &Range, pull: Option<(&str, &Pulled)>) -> String {
    let template = &group.template;
    let agent = crate::agents::Known::of(&template.agent).map_or(template.agent.as_str(), |kind| kind.title());
    let mut line = format!("- {agent}");
    if let Some(account) = &template.account {
        let _ = write!(line, " · account {account}");
    }
    if let Some(workspace) = &template.workspace {
        if let Some(branch) = &workspace.branch {
            let _ = write!(line, " · branch {branch}");
        }
        if workspace.worktree {
            let _ = write!(line, " · worktree {}", workspace.path);
        }
    }
    let mut text = format!(
        "{line}\n- active {} → {} · {}\n",
        stamp(group.first, &range.zone),
        stamp(group.last, &range.zone),
        state(group, &range.zone)
    );
    if let Some((id, pulled)) = pull {
        let pull = &pulled.pull;
        let title = pulled.title.as_ref().map(|title| format!(" \"{title}\"")).unwrap_or_default();
        let checks = pull.ci.as_deref().map(|ci| format!(" · checks {ci}")).unwrap_or_default();
        let seen = if pulled.live { "" } else { " (as last seen)" };
        let _ = writeln!(text, "- pull request [{id}] #{}{title} · {}{checks}{seen}", pull.number, pull.state);
    }
    if let Some(record) = &template.record {
        let _ = writeln!(text, "- transcript {}", record.path());
    }
    text
}

fn body(group: &Group, talk: &Talk, caps: &Caps, range: &Range) -> String {
    let mut text = String::from("### Prompts\n");
    if talk.prompts.is_empty() {
        text.push_str("- (none in this range)\n");
    }
    let mut prompts: Vec<&super::Said> = talk.prompts.iter().collect();
    if range.choice == Choice::Now {
        prompts.reverse();
    }
    for said in prompts {
        let when = said.at.map(|at| format!("{} ", stamp(at, &range.zone))).unwrap_or_default();
        let _ = writeln!(text, "- {when}\"{}\"", trim_to(&fold_paste(&said.text), caps.prompt_chars));
    }
    if let Some(reply) = &talk.reply {
        let _ = writeln!(text, "### Last reply\n> {}", trim_to(reply, caps.reply_chars));
    }
    text.push_str(&waiting(group, talk, caps));
    text
}

fn waiting(group: &Group, talk: &Talk, caps: &Caps) -> String {
    let Some(pane) = group.live.as_ref().filter(|pane| pane.status.as_deref() == Some("waiting")) else {
        return String::new();
    };
    let mut text = String::from("### Waiting on\n");
    if let Some(reply) = &talk.reply {
        let _ = writeln!(text, "> {}", trim_to(reply, caps.reply_chars));
    }
    if caps.screen {
        for line in pane.screen.iter().rev().take(15).rev() {
            let _ = writeln!(text, "    {}", line.chars().take(160).collect::<String>().trim_end());
        }
    }
    text
}

fn merged(range: &Range, groups: &[Group], live: &Live, ids: &mut Ids) -> String {
    if range.choice != Choice::Week {
        return String::new();
    }
    let mut text = String::new();
    for group in groups {
        let Some((id, pulled)) = pull_id(group, live, ids).filter(|(_, pulled)| pulled.pull.state == "merged") else {
            continue;
        };
        let named = pulled.title.map(|title| format!(" \"{title}\"")).unwrap_or_default();
        let entry =
            format!("- [{id}] #{}{named} · merged, last seen {}\n", pulled.pull.number, stamp(group.last, &range.zone));
        if !text.contains(&entry) {
            text.push_str(&entry);
        }
    }
    if text.is_empty() { text } else { format!("## Merged pull requests\n{text}") }
}

fn pull_id(group: &Group, live: &Live, ids: &mut Ids) -> Option<(String, Pulled)> {
    let pulled = pull_of(group, live)?;
    let pull = &pulled.pull;
    let workspace = group.template.workspace.as_ref().map(|w| w.path.clone()).unwrap_or_default();
    let place = Place {
        kind: "pull".into(),
        number: Some(pull.number),
        url: Some(pull.url.clone()),
        state: Some(pull.state.clone()),
        checks: if pulled.live { pull.ci.clone() } else { None },
        project: group.template.project.clone(),
        workspace: group
            .template
            .workspace
            .as_ref()
            .map(|w| crate::recap::Named { label: w.label.clone(), path: w.path.clone() }),
        ..Place::default()
    };
    Some((ids.pull(&workspace, pull.number, place), pulled))
}

#[derive(Default)]
struct Tally {
    shortened: usize,
    left_out: usize,
}

fn must_stay(group: &Group, range: &Range) -> bool {
    range.choice == Choice::Now && group.live.as_ref().is_some_and(|pane| needs_you(pane.status.as_deref()))
}

pub(super) fn render(range: &Range, caps: &Caps, groups: &[Group], talks: &[Talk], live: &Live) -> Dossier {
    let mut ids = Ids { places: BTreeMap::new(), pulls: Vec::new() };
    let mut text = String::new();
    let mut tally = Tally::default();
    let mut shown = 0;
    for (i, (group, talk)) in groups.iter().zip(talks).enumerate() {
        let id = format!("c{}", i + 1);
        let pull = pull_id(group, live, &mut ids);
        let head =
            format!("{}{}", heading(&id, group), facts(group, range, pull.as_ref().map(|(id, p)| (id.as_str(), p))));
        let full = format!("{head}{}\n", body(group, talk, caps, range));
        if text.len() + head.len() > caps.total && !must_stay(group, range) {
            tally.left_out += 1;
            continue;
        }
        ids.places.insert(id, place_of(group));
        shown += 1;
        if text.len() + full.len() > caps.total {
            tally.shortened += 1;
            let _ = writeln!(text, "{head}");
        } else {
            text.push_str(&full);
        }
    }
    text.push_str(&merged(range, groups, live, &mut ids));
    let text = format!("{}{text}", needs(range, &ids.places));
    Dossier { text: finish(range, groups, &tally, &text), places: ids.places, conversations: shown }
}

fn number(id: &str) -> (char, usize) {
    (id.chars().next().unwrap_or(' '), id.get(1..).and_then(|digits| digits.parse().ok()).unwrap_or(0))
}

fn need_line(id: &str, place: &Place) -> Option<String> {
    if place.kind == "conversation" && needs_you(place.status.as_deref()) {
        let what =
            NEEDS_YOU.iter().find(|(name, _)| Some(*name) == place.status.as_deref()).map_or("", |(_, what)| what);
        return Some(format!("- [{id}] {what}\n"));
    }
    if place.kind == "pull" && failing(place.state.as_deref(), place.checks.as_deref()) {
        return Some(format!("- [{id}] #{} · checks failing\n", place.number.unwrap_or_default()));
    }
    None
}

fn needs(range: &Range, places: &BTreeMap<String, Place>) -> String {
    if range.choice != Choice::Now {
        return String::new();
    }
    let mut sorted: Vec<(&String, &Place)> = places.iter().collect();
    sorted.sort_by_key(|(id, _)| number(id));
    let lines: String = sorted.into_iter().filter_map(|(id, place)| need_line(id, place)).collect();
    let lines = if lines.is_empty() { String::from("- nothing needs you\n") } else { lines };
    format!("{NEEDS_HEADING}\n{lines}\n")
}

fn finish(range: &Range, groups: &[Group], tally: &Tally, text: &str) -> String {
    let mut projects: Vec<&str> =
        groups.iter().filter_map(|g| project_of(g)).map(|named| named.path.as_str()).collect();
    projects.sort_unstable();
    projects.dedup();
    let mut head = format!(
        "# Dossier · {} ({})\n{} conversations in {} projects. Ids: [c1]… are conversations, [p1]… pull requests.\n",
        range.label,
        range.zone,
        groups.len(),
        projects.len()
    );
    if tally.shortened > 0 {
        let _ = writeln!(
            head,
            "{} conversations were shortened to their header to keep the dossier small.",
            tally.shortened
        );
    }
    if tally.left_out > 0 {
        let _ = writeln!(head, "{} more conversations were left out to keep the dossier small.", tally.left_out);
    }
    format!("{head}\n{text}")
}
