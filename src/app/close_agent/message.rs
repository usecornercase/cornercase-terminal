use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

use super::{CloseAgent, Plan};
use crate::app::App;
use crate::app::worktrees::{MAX_LOCK_REASON, UNCOMMITTED, lock_note};
use crate::app::worktrees_dialog::wrap;
use crate::git::worktree;
use crate::project::{Hierarchy, Tab};
use crate::term::Term;
use crate::text::{display_path, truncate_right};
use crate::ui;

const INDENT: &str = "  ";

impl App {
    pub(super) fn close_agent_heading(&self, plan: &Plan) -> String {
        let agent =
            self.projects.pane_by(plan.pane).map_or_else(|| "the agent".into(), |term| self.closing_agent_name(term));
        let place = self.closing_place(plan.place);
        let what = match plan.remove.and_then(|(project, workspace)| self.projects.workspace_in(project, workspace)) {
            Some(_) => "Its workspace is removed, with its worktree folder.".to_string(),
            None if plan.closing.tabs.len() > 1 => {
                format!("Its tab and {} more close; nothing is deleted from disk.", plan.closing.tabs.len() - 1)
            }
            None => "Its tab closes; nothing is deleted from disk.".to_string(),
        };
        format!("Close {agent} in {place}? {what}")
    }

    pub(super) fn close_agent_notes(&self, dialog: &CloseAgent, area: Rect) -> Vec<Line<'static>> {
        let plan = &dialog.plan;
        let mut lines = self.closing_folder_line(plan);
        lines.extend(self.closing_run_lines(plan));
        lines.extend(section("Also stops:", self.closing_stops(plan)));
        lines.extend(section(
            "Child agents that close with it:",
            self.closing_children(plan, &plan.closing.inside, true),
        ));
        let elsewhere = self.closing_children(plan, &plan.closing.elsewhere, false);
        lines.extend(section("Child agents started elsewhere keep running:", elsewhere));
        lines.extend(self.closing_risks(dialog));
        lines.extend(branch_lines(dialog));
        wrap(lines, ui::update_notes(area).width)
    }

    pub(in crate::app) fn closing_agent_name(&self, term: &Term) -> String {
        term.agent.agent().map(str::to_string).or_else(|| term.program(&self.config)).unwrap_or_else(|| "agent".into())
    }

    pub(in crate::app) fn closing_place(&self, (project, workspace): (u64, u64)) -> String {
        let Some(at) = self.projects.workspace_in(project, workspace) else { return String::new() };
        format!("{} › {}", self.project_label(&self.projects[at.project()]), self.projects.workspace(at).label())
    }

    fn closing_folder_line(&self, plan: &Plan) -> Vec<Line<'static>> {
        let found = plan.remove.and_then(|(project, workspace)| self.projects.workspace_in(project, workspace));
        let Some(at) = found else { return Vec::new() };
        let path = display_path(&self.projects.workspace(at).path, self.home.as_deref());
        vec![Line::from(format!("Deletes the worktree folder {path}."))]
    }

    fn closing_run_lines(&self, plan: &Plan) -> Vec<Line<'static>> {
        let ending = plan.run.filter(|_| plan.remove.is_some());
        let Some(run) = ending.and_then(|_| self.run_of(plan.place.1)) else { return Vec::new() };
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
        let mut items: Vec<String> =
            tabs.flat_map(|at| self.closing_tab_items(plan, self.projects.tab(at), own)).collect();
        items.extend(self.closing_role_tabs(plan));
        items
    }

    fn closing_tab_items(&self, plan: &Plan, tab: &Tab, own: Option<u64>) -> Vec<String> {
        let others: Vec<&Term> =
            tab.panes.iter().filter(|term| term.id != plan.pane && !closing_child(plan, term)).collect();
        if Some(tab.id) == own {
            return others.iter().map(|term| format!("{}, in the same tab", self.closing_pane(term))).collect();
        }
        if others.is_empty() {
            return Vec::new();
        }
        vec![format!("the tab {}{}", tab.label(&self.config), self.closing_programs(&others))]
    }

    fn closing_role_tabs(&self, plan: &Plan) -> Vec<String> {
        let found = plan.remove.and_then(|(project, workspace)| self.projects.workspace_in(project, workspace));
        let Some(at) = found else { return Vec::new() };
        let project = &self.projects[at.project()];
        let rows = self.role_rows(at);
        let rows = self.projects.workspace(at).role_worktrees.iter().filter_map(|tree| {
            let row = project.workspaces.iter().find(|row| row.path == tree.path && rows.contains(&row.id))?;
            Some((tree.role.as_str(), row))
        });
        let tabs = rows.flat_map(|(role, row)| row.tabs.iter().map(move |tab| (role, tab)));
        tabs.map(|(role, tab)| {
            let panes: Vec<&Term> = tab.panes.iter().collect();
            format!("the tab {} in the worktree of `{role}`{}", tab.label(&self.config), self.closing_programs(&panes))
        })
        .collect()
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

    fn closing_risks(&self, dialog: &CloseAgent) -> Vec<Line<'static>> {
        let found = dialog.plan.remove.and_then(|(project, workspace)| self.projects.workspace_in(project, workspace));
        let Some(at) = found else { return Vec::new() };
        let mut lines = Vec::new();
        let closed = self.forgotten_note(self.projects.workspace(at).id);
        lines.extend((!closed.is_empty()).then(|| Line::from(closed.trim().to_string())));
        let roles = self.projects.workspace(at).role_worktrees.iter().filter(|tree| tree.path.is_dir()).count();
        match roles {
            0 => {}
            1 => lines.push(Line::from("The worktree of its role goes with it.")),
            n => lines.push(Line::from(format!("The worktrees of its {n} roles go with it."))),
        }
        if self.teardown_script(at.project()).is_some() {
            lines.push(Line::from("Its teardown runs first."));
        }
        let Some(found) = dialog.findings() else { return lines };
        if found.changed {
            lines.push(red(UNCOMMITTED.to_string()));
        }
        if let Some(lock) = &found.lock {
            lines.push(red(locked(lock)));
            lines.extend(lock_note(lock, false).map(red));
        }
        lines
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

fn red(text: String) -> Line<'static> {
    Line::from(Span::styled(text, Style::default().fg(Color::Red)))
}

fn locked(lock: &worktree::Lock) -> String {
    let reason = truncate_right(&lock.reason, MAX_LOCK_REASON);
    let locked = if reason.is_empty() { "is locked".to_string() } else { format!("is locked: {reason}") };
    match &lock.role {
        Some((name, path)) => format!("The worktree of `{name}` ({}) {locked}; it is unlocked first.", path.display()),
        None => format!("The worktree {locked}; it is unlocked first."),
    }
}

fn branch_lines(dialog: &CloseAgent) -> Vec<Line<'static>> {
    let found = dialog.findings().filter(|_| dialog.plan.remove.is_some());
    let Some(trees) = found.map(|found| &found.trees) else { return Vec::new() };
    let names: Vec<&str> = trees.iter().filter_map(|tree| tree.branch.as_deref()).collect();
    let mut lines: Vec<Line<'static>> = match (names.as_slice(), dialog.branches) {
        ([], _) => Vec::new(),
        ([one], false) => vec![Line::from(format!("The branch {one} is kept."))],
        (many, false) => vec![Line::from(format!("The branches {} are kept.", many.join(", ")))],
        ([one], true) => vec![Line::from(format!("Deletes the branch {one} once its worktree is gone."))],
        (many, true) => {
            vec![Line::from(format!("Deletes the branches {} once their worktrees are gone.", many.join(", ")))]
        }
    };
    if !dialog.branches {
        return lines;
    }
    for tree in trees.iter().filter(|tree| tree.branch.is_none()) {
        let folder = tree.path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
        lines.push(Line::from(format!("{folder}: no branch is checked out, so none is deleted")));
    }
    let lost = trees.iter().filter_map(|tree| Some(format!("{INDENT}{}: {}", tree.branch.as_ref()?, tree.lost?)));
    lines.extend(lost.map(red));
    lines
}
