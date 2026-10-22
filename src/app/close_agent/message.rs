use ratatui::layout::Rect;
use ratatui::text::Line;

use super::Plan;
use crate::app::App;
use crate::app::worktrees_dialog::wrap;
use crate::project::{Hierarchy, Tab};
use crate::term::Term;
use crate::ui;

const INDENT: &str = "  ";

impl App {
    pub(super) fn close_agent_heading(&self, plan: &Plan) -> String {
        let agent =
            self.projects.pane_by(plan.pane).map_or_else(|| "the agent".into(), |term| self.closing_agent_name(term));
        let place = self.closing_place(plan.place);
        let what = match plan.closing.tabs.len() {
            0 | 1 => "Its tab closes".to_string(),
            n => format!("Its tab and {} more close", n - 1),
        };
        format!("Close {agent} in {place}? {what}; nothing is deleted from disk.")
    }

    pub(super) fn close_agent_notes(&self, plan: &Plan, area: Rect) -> Vec<Line<'static>> {
        let mut lines = self.closing_run_lines(plan);
        lines.extend(section("Also stops:", self.closing_stops(plan)));
        lines.extend(section(
            "Child agents that close with it:",
            self.closing_children(plan, &plan.closing.inside, true),
        ));
        let elsewhere = self.closing_children(plan, &plan.closing.elsewhere, false);
        lines.extend(section("Child agents started elsewhere keep running:", elsewhere));
        wrap(lines, ui::update_notes(area).width)
    }

    pub(in crate::app) fn closing_agent_name(&self, term: &Term) -> String {
        term.agent.agent().map(str::to_string).or_else(|| term.program(&self.config)).unwrap_or_else(|| "agent".into())
    }

    pub(in crate::app) fn closing_place(&self, (project, workspace): (u64, u64)) -> String {
        let Some(at) = self.projects.workspace_in(project, workspace) else { return String::new() };
        format!("{} › {}", self.project_label(&self.projects[at.project()]), self.projects.workspace(at).label())
    }

    fn closing_run_lines(&self, plan: &Plan) -> Vec<Line<'static>> {
        let Some(run) = plan.run.and_then(|_| self.run_of(plan.place.1)) else { return Vec::new() };
        let team: Vec<String> = run
            .members
            .iter()
            .map(|member| {
                let agent =
                    self.projects.pane_by(member.pane).map_or_else(|| "gone".into(), |t| self.closing_agent_name(t));
                format!("{} ({agent})", member.role)
            })
            .collect();
        let name = if run.flow.name.is_empty() { String::new() } else { format!(" {}", run.flow.name) };
        vec![Line::from(format!("Ends the flow run{name}; its team stops: {}.", team.join(", ")))]
    }

    fn closing_stops(&self, plan: &Plan) -> Vec<String> {
        let own = self.projects.tab_of_pane(plan.pane).map(|at| self.projects.tab(at).id);
        let tabs = plan.closing.tabs.iter().filter_map(|&id| self.projects.tab_index(id));
        tabs.flat_map(|at| self.closing_tab_items(plan, self.projects.tab(at), own)).collect()
    }

    fn closing_tab_items(&self, plan: &Plan, tab: &Tab, own: Option<u64>) -> Vec<String> {
        let others: Vec<&Term> = tab
            .panes
            .iter()
            .filter(|term| term.id != plan.pane && !closing_child(plan, term) && !self.ending_member(plan, term.id))
            .collect();
        if Some(tab.id) == own {
            return others.iter().map(|term| format!("{}, in the same tab", self.closing_pane(term))).collect();
        }
        if others.is_empty() {
            return Vec::new();
        }
        vec![format!("the tab {}{}", tab.label(&self.config), self.closing_programs(&others))]
    }

    fn ending_member(&self, plan: &Plan, pane: u64) -> bool {
        let run = plan.run.and_then(|_| self.run_of(plan.place.1));
        run.is_some_and(|run| run.members.iter().any(|member| member.pane == pane))
    }

    pub(in crate::app) fn closing_programs(&self, panes: &[&Term]) -> String {
        let running: Vec<String> = panes
            .iter()
            .filter(|term| term.agent.status().is_some() || !term.shell_in_foreground())
            .map(|term| self.closing_pane(term))
            .collect();
        if running.is_empty() { String::new() } else { format!(", running {}", running.join(", ")) }
    }

    pub(in crate::app) fn closing_pane(&self, term: &Term) -> String {
        if let Some(agent) = term.agent.agent().filter(|_| term.agent.status().is_some()) {
            return agent.to_string();
        }
        if term.shell_in_foreground() {
            return "a shell at its prompt".into();
        }
        term.program(&self.config).unwrap_or_else(|| "a program".into())
    }

    fn closing_children(&self, plan: &Plan, ids: &[u64], inside: bool) -> Vec<String> {
        let agents = ids.iter().filter_map(|&id| Some((self.projects.pane_by(id)?, self.projects.tab_of_pane(id)?)));
        agents
            .filter(|(term, _)| term.agent.status().is_some() && term.id != plan.pane)
            .map(|(term, at)| {
                let workspace = self.projects.workspace(at.workspace());
                if inside {
                    format!(
                        "{}, in the tab {}",
                        self.closing_agent_name(term),
                        self.projects.tab(at).label(&self.config)
                    )
                } else {
                    format!(
                        "{} in {}",
                        self.closing_agent_name(term),
                        self.closing_place((self.projects[at.project()].id, workspace.id))
                    )
                }
            })
            .collect()
    }
}

fn closing_child(plan: &Plan, term: &Term) -> bool {
    term.agent.status().is_some() && plan.closing.inside.contains(&term.id)
}

pub(in crate::app) fn section(title: &str, items: Vec<String>) -> Vec<Line<'static>> {
    if items.is_empty() {
        return Vec::new();
    }
    std::iter::once(Line::from(title.to_string()))
        .chain(items.into_iter().map(|item| Line::from(format!("{INDENT}{item}"))))
        .collect()
}
