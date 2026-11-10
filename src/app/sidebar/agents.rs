use std::collections::HashSet;

use crossterm::event::{MouseButton, MouseEventKind};
use ratatui::layout::{Position, Rect};

use crate::agents::activity::{self, Status};
use crate::agents::tree::Node;
use crate::app::App;
use crate::app::agent_watch::{AgentPlace, Listed};
use crate::app::interaction::Grab;
use crate::app::layout::List;
use crate::app::machine_links::{RemoteAt, RemoteShown, ThereAgent};
use crate::app::menus::Marked;
use crate::app::menus::MenuAction;
use crate::app::overlay::Overlay;
use crate::flows;
use crate::project::Hierarchy;
use crate::term::Term;
use crate::ui::{self};

struct AgentAt {
    grab: Grab,
    row: Rect,
    fold: Option<bool>,
    on_fold: bool,
    on_close: bool,
}

struct Marks {
    shown: Option<RemoteShown>,
    tab: Option<u64>,
    focused: Option<u64>,
}

struct AgentRow {
    status: Option<Status>,
    agent: String,
    project: String,
    workspace: Option<String>,
    seat: (Option<String>, bool),
    details: ui::Details,
    collapsed: bool,
    active: bool,
    machine: Option<ui::MachineTag>,
}

fn agent_entry(row: AgentRow, node: Node, hidden: &[Option<Status>]) -> ui::AgentEntry {
    let (role, place) = row.seat;
    ui::AgentEntry {
        status: row.status,
        agent: row.agent,
        project: row.project,
        workspace: row.workspace,
        role,
        place,
        details: row.details,
        active: row.active,
        depth: node.depth,
        last: node.last,
        rails: node.rails,
        fold: node.has_children.then_some(row.collapsed),
        hidden: hidden.len(),
        attention: activity::attention(hidden.iter().copied()),
        machine: row.machine,
    }
}

impl App {
    fn seated(&self, term: &Term, depth: usize) -> (Option<String>, bool) {
        let Some((run, role)) = self.seat(term.id) else { return (None, true) };
        let under = depth > 0
            && role != flows::ORCHESTRATOR
            && term
                .parent
                .as_ref()
                .and_then(|parent| self.seat(parent.pane))
                .is_some_and(|(theirs, _)| theirs.id == run.id);
        (Some(role.to_string()), !under)
    }

    pub(in crate::app) fn agents_view(&self) -> ui::AgentsView {
        let narrow = self.screen.drawn.agents_list.width < ui::MIN_MENU_ROW_WIDTH;
        let (listed, nodes) = self.shown_agents(narrow);
        let marks =
            Marks { shown: self.shown_remote(), tab: self.focus().tab, focused: self.term().map(|term| term.id) };
        let entries = nodes.into_iter().map(|node| self.shown_entry(&listed, &marks, node)).collect();
        ui::AgentsView { entries, scroll: self.scroll.of(List::Agents) }
    }

    fn shown_entry(&self, listed: &[Listed], marks: &Marks, node: Node) -> ui::AgentEntry {
        let row = match &listed[node.index] {
            Listed::Here(place) => self.here_row(listed, *place, &node, marks),
            Listed::There(there) => self.there_row(listed, there, &node, marks),
        };
        let hidden: Vec<Option<Status>> = node.hidden.iter().map(|&h| self.listed_status(&listed[h])).collect();
        agent_entry(row, node, &hidden)
    }

    fn here_row(&self, listed: &[Listed], place: AgentPlace, node: &Node, marks: &Marks) -> AgentRow {
        let (at, i) = place;
        let (project, workspace) = (&self.projects[at.project()], self.projects.workspace(at.workspace()));
        let tab = self.projects.tab(at);
        let term = &tab.panes[i];
        let mut hidden = node.hidden.iter().filter_map(|&h| match listed[h] {
            Listed::Here(place) => Some(self.placed(place).id),
            Listed::There(_) => None,
        });
        AgentRow {
            status: term.agent.status(),
            agent: term.agent.agent().unwrap_or_default().to_string(),
            project: self.project_label(project),
            workspace: (project.workspaces.len() > 1).then(|| workspace.label()),
            seat: self.seated(term, node.depth),
            details: ui::Details {
                compacted: self.shows_compaction(term.context.compaction().hides),
                ..self.details(term.context.context(), None, term.context.subagents(), self.pane_account(term))
            },
            collapsed: term.collapsed,
            active: marks.shown.is_none()
                && ((marks.tab == Some(tab.id) && tab.active == i) || hidden.any(|id| Some(id) == marks.focused)),
            machine: None,
        }
    }

    fn there_row(&self, listed: &[Listed], there: &ThereAgent, node: &Node, marks: &Marks) -> AgentRow {
        let pane = there.pane;
        let on = |there: &ThereAgent| marks.shown.as_ref().is_some_and(|shown| there.shown_in(shown));
        let under = node.hidden.iter().any(|&h| matches!(&listed[h], Listed::There(hidden) if on(hidden)));
        let subagents = usize::try_from(pane.subagents).unwrap_or(usize::MAX);
        let paired = there.paired.and_then(|id| self.projects.project_index(id));
        AgentRow {
            status: Some(there.status),
            agent: pane.agent.clone().unwrap_or_default(),
            project: paired.map_or_else(|| there.project.name.clone(), |p| self.project_label(&self.projects[p])),
            workspace: (there.project.workspaces.len() > 1).then(|| there.workspace.name.clone()),
            seat: there.seat(node.depth),
            details: ui::Details {
                compacted: self.shows_compaction(pane.compacted),
                ..self.context_details((pane.model.clone(), pane.context), None, subagents, None)
            },
            collapsed: pane.collapsed,
            active: on(there) || under,
            machine: Some(there.tag.clone()),
        }
    }

    pub(in crate::app) fn agent_layout(&self, areas: &ui::Areas) -> ui::Rows {
        ui::agent_rows(areas.agents_list, areas.pitch, self.shown_agents(false).1.len(), self.scroll.of(List::Agents))
    }

    fn agent_row_at(&self, areas: &ui::Areas, pos: Position) -> Option<AgentAt> {
        let narrow = areas.agents_list.width < ui::MIN_MENU_ROW_WIDTH;
        let (listed, nodes) = self.shown_agents(narrow);
        let (list, pitch, scroll) = (areas.agents_list, areas.pitch, self.scroll.of(List::Agents));
        let i = ui::agent_hit(list, pitch, nodes.len(), scroll, pos)?;
        let node = &nodes[i];
        let (grab, collapsed, closes) = match &listed[node.index] {
            Listed::Here(place) => {
                let term = self.placed(*place);
                (Grab::Agent(term.id), term.collapsed, true)
            }
            Listed::There(there) => (Grab::Remote(there.at()), there.pane.collapsed, false),
        };
        let row = ui::agent_row(list, pitch, nodes.len(), scroll, i);
        let fold = node.has_children.then_some(collapsed);
        Some(AgentAt {
            grab,
            row,
            fold,
            on_fold: fold.is_some() && ui::agent_fold(row, node.depth).contains(pos),
            on_close: closes && ui::agent_close(row, pitch).contains(pos),
        })
    }

    pub(in crate::app) fn agents_mouse(&mut self, areas: &ui::Areas, pos: Position, kind: MouseEventKind, area: Rect) {
        match kind {
            _ if !areas.agents_list.contains(pos) => {}
            MouseEventKind::Down(MouseButton::Left) => self.press_agent(areas, pos, area),
            MouseEventKind::Down(MouseButton::Right) => self.open_agent_menu(areas, pos),
            _ => {}
        }
    }

    fn open_agent_menu(&mut self, areas: &ui::Areas, pos: Position) {
        let Some(AgentAt { grab, fold, .. }) = self.agent_row_at(areas, pos) else { return };
        match (grab, fold) {
            (Grab::Agent(pane), fold) => {
                let actions = self.agent_row_actions(pane, fold);
                self.overlay = Some(Overlay::Menu { at: pos, actions });
            }
            (Grab::Remote(RemoteAt::Agent { aim, .. }), Some(_)) => self.agent_fold_menu(aim, pos),
            _ => {}
        }
    }

    pub(in crate::app) fn agent_row_actions(&self, pane: u64, fold: Option<bool>) -> Vec<MenuAction> {
        self.mark_action(Marked::Pane(pane))
            .into_iter()
            .chain(fold.map(|collapsed| MenuAction::AgentFold(pane, !collapsed)))
            .chain([MenuAction::ViewContext { pane, named: false }, MenuAction::CloseAgent(pane)])
            .collect()
    }

    fn press_agent(&mut self, areas: &ui::Areas, pos: Position, area: Rect) {
        let Some(at) = self.agent_row_at(areas, pos) else { return };
        match at.grab {
            Grab::Agent(pane) if at.on_close => self.ask_close_agent(pane),
            grab => self.grab_with(grab, at.row, area, at.on_fold),
        }
    }

    pub(in crate::app) fn set_agent_fold(&mut self, pane: u64, collapsed: bool) -> bool {
        let Some(term) = self.projects.pane_by_mut(pane) else { return false };
        term.collapsed = collapsed;
        self.clamp_agents_scroll();
        true
    }

    pub(in crate::app) fn clamp_agents_scroll(&mut self) {
        self.scroll.set(List::Agents, self.agent_layout(&self.screen.drawn).scrolled(0));
    }

    pub(super) fn toggle_agent_fold(&mut self, pane: u64) {
        if let Some(term) = self.projects.pane_by(pane) {
            self.set_agent_fold(pane, !term.collapsed);
        }
    }

    fn listed_ancestors(&self, pane: u64) -> Vec<u64> {
        let live = self.running_panes();
        let mut seen = HashSet::from([pane]);
        let mut found = Vec::new();
        let mut at = pane;
        while let Some(term) = self.projects.pane_by(at) {
            if term.agent.status().is_none() {
                break;
            }
            let Some(parent) = Self::live_parent(term, &live) else { break };
            let listed = self.projects.pane_by(parent).is_some_and(|ancestor| ancestor.agent.status().is_some());
            if !seen.insert(parent) || !listed {
                break;
            }
            found.push(parent);
            at = parent;
        }
        found
    }

    pub(in crate::app) fn reveal_agent(&mut self, pane: u64) -> bool {
        let mut changed = false;
        for ancestor in self.listed_ancestors(pane) {
            if let Some(term) = self.projects.pane_by_mut(ancestor) {
                changed |= std::mem::take(&mut term.collapsed);
            }
        }
        changed
    }
}
