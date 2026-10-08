use crate::activity::Status;
use crate::agents;
use crate::control::{PaneInfo, Report};
use crate::ui::CRUMB_SEPARATOR;

const STOPS: &str = "Restarting stops every program running in cornercase's terminals.";
const NOTHING: &str = "Nothing is running in your terminals.";
const COMES_BACK: &str =
    "Your projects, workspaces, tabs and splits come back, each tab with a new shell in its folder.";
const RESUME: &str = "To pick up a Claude Code conversation afterwards, run `claude --continue` in its tab.";
const SHELLS: [&str; 17] = [
    "sh", "bash", "zsh", "fish", "dash", "ksh", "mksh", "oksh", "loksh", "yash", "tcsh", "csh", "nu", "elvish",
    "xonsh", "pwsh", "ion",
];
const STATUSES: [(Status, &str); 4] = [
    (Status::Working, "working"),
    (Status::Waiting, "waiting for you"),
    (Status::Done, "done"),
    (Status::Idle, "idle"),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Running {
    pub program: String,
    pub place: String,
    pub agent: Option<String>,
    pub status: Option<String>,
    pub resumes: bool,
}

pub fn from_report(report: &Report) -> Vec<Running> {
    let mut running = Vec::new();
    for project in &report.projects {
        for workspace in &project.workspaces {
            let panes = workspace.tabs.iter().flat_map(|t| &t.panes).filter(|pane| !pane.caller && busy(pane));
            running.extend(panes.map(|pane| Running {
                program: pane.program.clone().unwrap_or_else(|| "a program".into()),
                place: format!("{}{CRUMB_SEPARATOR}{}", project.name, workspace.name),
                agent: pane.agent.clone(),
                status: pane.status.clone(),
                resumes: pane.resumes,
            }));
        }
    }
    running
}

fn busy(pane: &PaneInfo) -> bool {
    if pane.agent.is_some() {
        return true;
    }
    match pane.at_prompt {
        Some(at_prompt) => !at_prompt,
        None => pane.program.as_deref().is_some_and(|program| !SHELLS.contains(&program)),
    }
}

pub fn confirmation(running: Option<&[Running]>) -> String {
    let Some(running) = running else { return format!("{STOPS} {COMES_BACK}") };
    if running.is_empty() {
        return format!("{NOTHING} {COMES_BACK}");
    }
    let now: Vec<String> =
        parts(running).into_iter().map(|(counted, detail)| format!("{counted} ({detail})")).collect();
    let text = format!("{STOPS} Running now: {}. {COMES_BACK}", now.join(" and "));
    match running.iter().filter(|r| r.resumes).count() {
        0 if running.iter().any(|r| r.agent.as_deref() == Some(agents::CLAUDE)) => format!("{text} {RESUME}"),
        0 => text,
        1 => format!("{text} 1 agent conversation resumes in its tab."),
        n => format!("{text} {n} agent conversations resume in their tabs."),
    }
}

pub fn stopped(running: &[Running]) -> Option<String> {
    let counted: Vec<String> = parts(running).into_iter().map(|(counted, _)| counted).collect();
    (!counted.is_empty()).then(|| format!("stopped {}", counted.join(" and ")))
}

fn parts(running: &[Running]) -> Vec<(String, String)> {
    let (with_agent, others): (Vec<&Running>, Vec<&Running>) = running.iter().partition(|r| r.agent.is_some());
    let mut parts = Vec::new();
    if !with_agent.is_empty() {
        parts.push((count(with_agent.len(), "agent"), by_status(&with_agent)));
    }
    if !others.is_empty() {
        let word = if with_agent.is_empty() { "program" } else { "other program" };
        let named: Vec<String> = others.iter().map(|r| format!("`{}` in {}", r.program, r.place)).collect();
        parts.push((count(others.len(), word), named.join(", ")));
    }
    parts
}

fn by_status(with_agent: &[&Running]) -> String {
    let counted = STATUSES.iter().filter_map(|(status, said)| {
        let n =
            with_agent.iter().filter(|a| a.status.as_deref().unwrap_or(Status::Idle.name()) == status.name()).count();
        (n > 0).then(|| format!("{n} {said}"))
    });
    counted.collect::<Vec<_>>().join(", ")
}

fn count(n: usize, word: &str) -> String {
    if n == 1 { format!("1 {word}") } else { format!("{n} {word}s") }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::control::{PaneInfo, ProjectInfo, Report, TabInfo, WorkspaceInfo};

    fn running(program: &str, agent: Option<&str>, status: Option<&str>) -> Running {
        Running {
            program: program.into(),
            place: "shop › main".into(),
            agent: agent.map(str::to_string),
            status: status.map(str::to_string),
            resumes: false,
        }
    }

    fn report(panes: Vec<PaneInfo>) -> Report {
        let tab = TabInfo { panes, ..TabInfo::default() };
        let workspace = WorkspaceInfo { name: "main".into(), tabs: vec![tab], ..WorkspaceInfo::default() };
        let project = ProjectInfo { name: "shop".into(), workspaces: vec![workspace], ..ProjectInfo::default() };
        Report { projects: vec![project], ..Report::default() }
    }

    fn pane(program: &str, at_prompt: Option<bool>) -> PaneInfo {
        PaneInfo { program: Some(program.into()), at_prompt, ..PaneInfo::default() }
    }

    #[test]
    fn nothing_running_says_so() {
        assert_eq!(
            confirmation(Some(&[])),
            "Nothing is running in your terminals. Your projects, workspaces, tabs and splits come back, \
             each tab with a new shell in its folder."
        );
    }

    #[test]
    fn agents_are_counted_by_what_they_do_and_programs_are_named() {
        let all = [
            running("claude", Some("claude"), Some("working")),
            running("codex", Some("codex"), Some("working")),
            running("claude", Some("claude"), Some("waiting")),
            running("npm", None, None),
        ];
        assert_eq!(
            confirmation(Some(&all)),
            "Restarting stops every program running in cornercase's terminals. Running now: \
             3 agents (2 working, 1 waiting for you) and 1 other program (`npm` in shop › main). \
             Your projects, workspaces, tabs and splits come back, each tab with a new shell in its folder. \
             To pick up a Claude Code conversation afterwards, run `claude --continue` in its tab."
        );
    }

    #[rstest]
    #[case::one(1, "1 agent conversation resumes in its tab.")]
    #[case::two(2, "2 agent conversations resume in their tabs.")]
    fn conversations_that_resume_are_counted_instead_of_the_hint(#[case] n: usize, #[case] said: &str) {
        let agents = [running("claude", Some("claude"), None), running("codex", Some("codex"), None)];
        let all: Vec<Running> =
            agents.into_iter().enumerate().map(|(i, agent)| Running { resumes: i < n, ..agent }).collect();
        let text = confirmation(Some(&all));
        assert_eq!((text.ends_with(said), text.contains("--continue")), (true, false));
    }

    #[rstest]
    #[case::nothing(&[], None)]
    #[case::one_program(&[("sleep", None)], Some("stopped 1 program"))]
    #[case::agents_and_programs(
        &[("claude", Some("claude")), ("nvim", None), ("npm", None)],
        Some("stopped 1 agent and 2 other programs")
    )]
    fn stopped_counts_what_ran(#[case] ran: &[(&str, Option<&str>)], #[case] said: Option<&str>) {
        let ran: Vec<Running> = ran.iter().map(|&(program, agent)| running(program, agent, None)).collect();
        assert_eq!(stopped(&ran).as_deref(), said);
    }

    #[test]
    fn a_pane_at_its_prompt_is_not_running() {
        let found = from_report(&report(vec![pane("zsh", Some(true)), pane("sleep", Some(false))]));
        assert_eq!(found, [running("sleep", None, None)]);
    }

    #[test]
    fn an_agent_runs_even_where_its_pane_says_at_prompt() {
        let claude = PaneInfo { agent: Some("claude".into()), ..pane("claude", Some(true)) };
        assert_eq!(from_report(&report(vec![claude])), [running("claude", Some("claude"), None)]);
    }

    #[test]
    fn the_pane_that_asks_is_not_listed() {
        let asking = PaneInfo { caller: true, ..pane("cornercase", Some(false)) };
        assert_eq!(from_report(&report(vec![asking, pane("nvim", Some(false))])), [running("nvim", None, None)]);
    }

    #[test]
    fn an_old_server_s_shells_are_told_apart_by_name() {
        let found = from_report(&report(vec![pane("zsh", None), pane("nvim", None)]));
        assert_eq!(found, [running("nvim", None, None)]);
    }
}
