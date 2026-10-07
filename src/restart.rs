use crate::agents;
use crate::control::{PaneInfo, Report};

const STOPS: &str = "Restarting stops every program running in cornercase's terminals.";
const NOTHING: &str = "Nothing is running in your terminals.";
const COMES_BACK: &str =
    "Your projects, workspaces, tabs and splits come back, each tab with a new shell in its folder.";
const RESUME: &str = "To pick up a Claude Code conversation afterwards, run `claude --continue` in its tab.";
const SHELLS: [&str; 12] = ["sh", "bash", "zsh", "fish", "dash", "ksh", "mksh", "tcsh", "csh", "nu", "elvish", "xonsh"];
const STATUSES: [(&str, &str); 4] =
    [("working", "working"), ("waiting", "waiting for you"), ("done", "done"), ("idle", "idle")];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Running {
    pub program: String,
    pub place: String,
    pub agent: Option<String>,
    pub status: Option<String>,
}

pub fn from_report(report: &Report) -> Vec<Running> {
    let mut running = Vec::new();
    for project in &report.projects {
        for workspace in &project.workspaces {
            let panes = workspace.tabs.iter().flat_map(|t| &t.panes).filter(|pane| busy(pane));
            running.extend(panes.map(|pane| Running {
                program: pane.program.clone().unwrap_or_else(|| "a program".into()),
                place: format!("{} › {}", project.name, workspace.name),
                agent: pane.agent.clone(),
                status: pane.status.clone(),
            }));
        }
    }
    running
}

fn busy(pane: &PaneInfo) -> bool {
    pane.agent.is_some()
        || pane
            .shell
            .map_or_else(|| pane.program.as_deref().is_some_and(|program| !SHELLS.contains(&program)), |shell| !shell)
}

pub fn generic() -> String {
    format!("{STOPS} {COMES_BACK}")
}

pub fn confirmation(running: &[Running]) -> String {
    if running.is_empty() {
        return format!("{NOTHING} {COMES_BACK}");
    }
    let (with_agent, others): (Vec<&Running>, Vec<&Running>) = running.iter().partition(|r| r.agent.is_some());
    let mut parts = Vec::new();
    if !with_agent.is_empty() {
        let by_status = STATUSES.iter().filter_map(|(status, said)| {
            let n = with_agent.iter().filter(|a| a.status.as_deref().unwrap_or("idle") == *status).count();
            (n > 0).then(|| format!("{n} {said}"))
        });
        parts.push(format!("{} ({})", count(with_agent.len(), "agent"), by_status.collect::<Vec<_>>().join(", ")));
    }
    if !others.is_empty() {
        let named: Vec<String> = others.iter().map(|r| format!("`{}` in {}", r.program, r.place)).collect();
        parts.push(format!("{} ({})", count(others.len(), program_word(&with_agent)), named.join(", ")));
    }
    let text = format!("{STOPS} Running now: {}. {COMES_BACK}", parts.join(" and "));
    if with_agent.iter().any(|a| a.agent.as_deref() == Some(agents::CLAUDE)) {
        return format!("{text} {RESUME}");
    }
    text
}

pub fn stopped(running: &[Running]) -> Option<String> {
    if running.is_empty() {
        return None;
    }
    let (with_agent, others): (Vec<&Running>, Vec<&Running>) = running.iter().partition(|r| r.agent.is_some());
    let parts: Vec<String> = [(with_agent.len(), "agent"), (others.len(), program_word(&with_agent))]
        .into_iter()
        .filter(|(n, _)| *n > 0)
        .map(|(n, word)| count(n, word))
        .collect();
    Some(format!("stopped {}", parts.join(" and ")))
}

fn program_word(with_agent: &[&Running]) -> &'static str {
    if with_agent.is_empty() { "program" } else { "other program" }
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
        }
    }

    fn report(panes: Vec<PaneInfo>) -> Report {
        let tab = TabInfo { panes, ..TabInfo::default() };
        let workspace = WorkspaceInfo { name: "main".into(), tabs: vec![tab], ..WorkspaceInfo::default() };
        let project = ProjectInfo { name: "shop".into(), workspaces: vec![workspace], ..ProjectInfo::default() };
        Report { projects: vec![project], ..Report::default() }
    }

    fn pane(program: &str, shell: Option<bool>) -> PaneInfo {
        PaneInfo { program: Some(program.into()), shell, ..PaneInfo::default() }
    }

    #[test]
    fn nothing_running_says_so() {
        assert_eq!(
            confirmation(&[]),
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
            confirmation(&all),
            "Restarting stops every program running in cornercase's terminals. Running now: \
             3 agents (2 working, 1 waiting for you) and 1 other program (`npm` in shop › main). \
             Your projects, workspaces, tabs and splits come back, each tab with a new shell in its folder. \
             To pick up a Claude Code conversation afterwards, run `claude --continue` in its tab."
        );
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
    fn a_shell_at_its_prompt_is_not_running() {
        let found = from_report(&report(vec![pane("zsh", Some(true)), pane("sleep", Some(false))]));
        assert_eq!(found, [running("sleep", None, None)]);
    }

    #[test]
    fn an_agent_runs_even_where_its_pane_says_shell() {
        let claude = PaneInfo { agent: Some("claude".into()), ..pane("claude", Some(true)) };
        assert_eq!(from_report(&report(vec![claude])), [running("claude", Some("claude"), None)]);
    }

    #[test]
    fn an_old_server_s_shells_are_told_apart_by_name() {
        let found = from_report(&report(vec![pane("zsh", None), pane("nvim", None)]));
        assert_eq!(found, [running("nvim", None, None)]);
    }
}
