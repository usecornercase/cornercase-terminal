use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use super::App;
use super::closed_agents::forgotten;
use super::menus::{PaneAction, Target};
use super::overlay::{CLOSE_SUBMIT, CREATE_SUBMIT, Overlay, RENAME_SUBMIT};
use super::text_fields::FieldArea;
use crate::editor::Editor;
use crate::error::Result;
use crate::git::worktree;
use crate::project::Hierarchy;
use crate::project::group;
use crate::ui::{self, FormHit};

pub(super) const KEEP_PROJECTS_SUBMIT: &str = "keep projects";

pub(super) const CLOSE_PROJECTS_SUBMIT: &str = "close projects";

const NEW_GROUP_HINT: &str = "right-click a project or its ⋯ to move it into the group";

const WORKTREE_TOGGLE: &str = "with its own worktree";

impl App {
    pub(super) fn new_group_view(input: &Editor) -> ui::Overlay {
        ui::Overlay::Form(ui::Form {
            title: "New group",
            label: "name",
            value: input.clone(),
            hint: NEW_GROUP_HINT.into(),
            toggle: None,
            note: None,
            submit: CREATE_SUBMIT,
        })
    }

    pub(super) fn new_workspace_view(&self, overlay: &Overlay) -> Option<ui::Overlay> {
        let Overlay::NewWorkspace { project, input, worktree, error, creating } = overlay else { return None };
        let (project, worktree, creating) = (*project, *worktree, *creating);
        let home = self.home.as_deref();
        let repo = self.projects.project_index(project).map(|p| self.projects[p].path.clone()).unwrap_or_default();
        let path = if worktree == Some(true) {
            worktree::checkout_path(&self.config.worktrees_dir(home), &repo, input.text().trim())
        } else {
            repo
        };
        Some(ui::Overlay::Form(ui::Form {
            title: "New workspace",
            label: "name",
            value: input.clone(),
            hint: format!("in {}", crate::text::display_path(&path, home)),
            toggle: worktree.map(|on| ui::Toggle { label: WORKTREE_TOGGLE, on }),
            note: if creating {
                Some(ui::Note::Busy("creating…"))
            } else {
                error.as_deref().map(|e| ui::Note::Error(e.into())).or_else(|| self.setup_note(project, worktree))
            },
            submit: CREATE_SUBMIT,
        }))
    }

    pub(super) fn rename_view(target: Target, input: &Editor) -> ui::Overlay {
        ui::Overlay::Form(ui::Form {
            title: target.rename_title(),
            label: "name",
            value: input.clone(),
            hint: target.rename_hint().into(),
            toggle: None,
            note: None,
            submit: RENAME_SUBMIT,
        })
    }

    pub(super) fn group_style_view(&self, group: u64) -> Option<ui::Overlay> {
        Some(ui::Overlay::GroupStyle(self.group(group)?.clone()))
    }

    pub(super) fn delete_group_view(&self, group: u64) -> Option<ui::Overlay> {
        let (submit, extra) = self.dialog_labels(&Overlay::DeleteGroup { group });
        Some(confirm_view("Delete group", self.delete_group_message(group)?, submit, extra))
    }

    pub(super) fn close_project_view(&self, project: u64) -> Option<ui::Overlay> {
        Some(confirm_view("Close project", self.close_project_message(project)?, CLOSE_SUBMIT, None))
    }

    pub(super) fn close_workspace_view(&self, project: u64, workspace: u64) -> Option<ui::Overlay> {
        Some(confirm_view("Close workspace", self.close_workspace_message(project, workspace)?, CLOSE_SUBMIT, None))
    }

    pub(super) fn close_tab_view(&self, project: u64, workspace: u64, tab: u64) -> Option<ui::Overlay> {
        Some(confirm_view("Close tab", self.close_tab_message(project, workspace, tab)?, CLOSE_SUBMIT, None))
    }

    pub(super) fn close_pane_view(&self, pane: u64) -> Option<ui::Overlay> {
        Some(confirm_view("Close pane", self.close_pane_message(pane)?, CLOSE_SUBMIT, None))
    }

    pub(super) fn closed_from_outside(&mut self, ev: MouseEvent, pos: Position, area: Rect) -> bool {
        let outside = ev.kind == MouseEventKind::Down(MouseButton::Left) && self.outside_dialog(pos, area);
        if outside && self.popup().is_none() {
            self.cancel_form();
        }
        outside
    }

    fn outside_dialog(&self, pos: Position, area: Rect) -> bool {
        let Some(overlay) = &self.overlay else { return false };
        self.overlay_view(overlay, area).and_then(|o| o.area(area)).is_some_and(|r| !r.contains(pos))
    }

    pub(super) fn dialog_labels(&self, overlay: &Overlay) -> (&'static str, Option<&'static str>) {
        match overlay {
            Overlay::DeleteGroup { group } if self.group_projects(*group).next().is_some() => {
                (KEEP_PROJECTS_SUBMIT, Some(CLOSE_PROJECTS_SUBMIT))
            }
            _ => (overlay.submit_label(), None),
        }
    }

    pub(super) fn submit_extra(&mut self) {
        if let Some(Overlay::DeleteGroup { group }) = self.overlay {
            self.overlay = None;
            self.delete_group_and_projects(group);
        }
    }

    pub(super) fn group_style_mouse(&mut self, group: u64, ev: MouseEvent, pos: Position, area: Rect) {
        if ev.kind != MouseEventKind::Down(MouseButton::Left) {
            return;
        }
        let hit = ui::style_hit(area, pos);
        let known = match hit {
            Some(ui::StyleHit::Icon(i)) => self.style_group(group, Some(group::GROUP_ICONS[i]), None),
            Some(ui::StyleHit::Colour(i)) => self.style_group(group, None, Some(group::GROUP_COLOURS[i])),
            Some(ui::StyleHit::Done) | None => self.group_index(group).is_some(),
        };
        if !known || matches!(hit, Some(ui::StyleHit::Done)) {
            self.overlay = None;
        }
    }

    pub(super) fn toggle_worktree(&mut self) {
        if let Some(Overlay::NewWorkspace { worktree: Some(on), creating: false, .. }) = &mut self.overlay {
            *on = !*on;
        }
    }

    pub(super) fn form_key(&mut self, key: KeyEvent, area: Rect) -> Result<()> {
        match key.code {
            KeyCode::Esc => self.cancel_form(),
            KeyCode::Enter => self.submit_form(area)?,
            KeyCode::Tab => self.toggle_worktree(),
            _ if self.overlay.as_ref().is_some_and(Overlay::takes_input) => self.name_key(key, area),
            KeyCode::Up => self.scroll_dialog(-1, area),
            KeyCode::Down => self.scroll_dialog(1, area),
            _ => {}
        }
        Ok(())
    }

    fn name_key(&mut self, key: KeyEvent, area: Rect) {
        let Some((at, input)) = self.name_text(area) else { return };
        if input.one_line_key(key, at.width) {
            self.clear_form_error();
        }
    }

    pub(super) fn name_paste(&mut self, text: &str, area: Rect) {
        let Some((_, input)) = self.name_text(area) else { return };
        input.insert(text);
        self.clear_form_error();
    }

    fn clear_form_error(&mut self) {
        if let Some(overlay) = &mut self.overlay {
            overlay.clear_error();
        }
    }

    pub(super) fn name_text(&mut self, area: Rect) -> Option<(FieldArea, &mut Editor)> {
        let text = ui::form_text(area);
        let width = usize::from(text.width.max(1));
        let input = self.overlay.as_mut()?.input()?;
        Some((FieldArea { text, width, top: input.position(width).0, scrolls: false }, input))
    }

    fn name_click(&mut self, pos: Position, area: Rect) {
        if let Some((at, input)) = self.name_text(area).filter(|(at, _)| at.text.contains(pos)) {
            let (line, col) = at.spot(pos);
            input.place(at.width, line, col);
        }
    }

    pub(super) fn form_mouse(&mut self, ev: MouseEvent, pos: Position, area: Rect) -> Result<()> {
        if ev.kind != MouseEventKind::Down(MouseButton::Left) {
            return Ok(());
        }
        let Some(overlay) = &self.overlay else { return Ok(()) };
        let (submit, extra) = self.dialog_labels(overlay);
        match ui::form_hit(area, submit, extra, pos) {
            Some(FormHit::Submit) => self.submit_form(area)?,
            Some(FormHit::Extra) => self.submit_extra(),
            Some(FormHit::Cancel) => self.cancel_form(),
            Some(FormHit::Toggle) => self.toggle_worktree(),
            None => self.name_click(pos, area),
        }
        Ok(())
    }

    fn scroll_dialog(&mut self, delta: isize, area: Rect) {
        if matches!(self.overlay, Some(Overlay::PullRequest { .. })) {
            self.scroll_pull_request(delta, area);
        } else if matches!(self.overlay, Some(Overlay::MergeCheck { .. })) {
            self.scroll_merge_check(delta, area);
        } else if matches!(self.overlay, Some(Overlay::FlowMessage { .. })) {
            self.scroll_flow_message(delta, area);
        } else {
            self.scroll_update(delta, area);
        }
    }

    pub(super) fn cancel_form(&mut self) {
        match &self.overlay {
            Some(overlay) if overlay.busy() => {}
            Some(Overlay::Queue(_)) => self.close_queue_form(false),
            Some(Overlay::Flows { .. }) => self.close_flows(),
            Some(Overlay::RemoveAccount { .. }) => self.open_settings(),
            _ => self.overlay = None,
        }
    }

    pub(super) fn submit_form(&mut self, area: Rect) -> Result<()> {
        let Some(overlay) = self.overlay.take() else { return Ok(()) };
        self.overlay = match overlay {
            Overlay::NewWorkspace { project, input, worktree, creating: false, .. } => {
                self.create_workspace(project, input, worktree, area)?
            }
            Overlay::Rename { target, input } => self.submit_rename(target, &input.text()),
            Overlay::NewGroup { input } if input.text().trim().is_empty() => Some(Overlay::NewGroup { input }),
            Overlay::NewGroup { input } => self.submit_new_group(&input.text()),
            Overlay::GroupStyle { .. }
            | Overlay::Usage
            | Overlay::Remote
            | Overlay::Pairing(_)
            | Overlay::Forum(_)
            | Overlay::Worktrees(_)
            | Overlay::Ports(_)
            | Overlay::Processes(_) => None,
            Overlay::RemoveWorkspace { project, workspace, check, lock } => {
                self.confirm_removal(project, workspace, check, lock.is_some())
            }
            Overlay::DeleteGroup { group } => self.submit_delete_group(group),
            Overlay::RemoveAccount { kind, name } => Some(self.submit_remove_account(kind, &name)),
            Overlay::DiscardChanges(target) => self.submit_discard(target),
            Overlay::CloseProject { project } => self.submit_close_project(project),
            Overlay::CloseWorkspace { project, workspace } => self.submit_close_workspace(project, workspace),
            Overlay::CloseTab { project, workspace, tab } => self.submit_close_tab(project, workspace, tab),
            Overlay::ForgetAgents { workspace, .. } => self.submit_forget_agents(workspace),
            Overlay::ClosePane { pane } => self.submit_close_pane(pane, area)?,
            Overlay::Update(step) => self.submit_update(step),
            Overlay::Restart => self.submit_restart(),
            Overlay::PullRequest { workspace, .. } => self.submit_pull_request(workspace),
            Overlay::MergeCheck { workspace, scroll } => {
                self.check_merges(workspace);
                Some(Overlay::MergeCheck { workspace, scroll })
            }
            Overlay::FlowMessage { workspace, shown, .. } => self.submit_flow_message(workspace, &shown),
            busy => Some(busy),
        };
        Ok(())
    }

    fn submit_rename(&mut self, target: Target, input: &str) -> Option<Overlay> {
        let name = input.trim();
        self.rename(target, (!name.is_empty()).then(|| name.to_string()));
        None
    }

    fn submit_new_group(&mut self, input: &str) -> Option<Overlay> {
        self.add_group(input.trim().to_string());
        None
    }

    fn submit_delete_group(&mut self, group: u64) -> Option<Overlay> {
        self.delete_group(group);
        None
    }

    fn submit_close_project(&mut self, project: u64) -> Option<Overlay> {
        self.close_project(project);
        None
    }

    fn submit_close_workspace(&mut self, project: u64, workspace: u64) -> Option<Overlay> {
        if let Some(at) = self.projects.workspace_in(project, workspace) {
            self.close_workspace(at);
        }
        None
    }

    fn submit_close_tab(&mut self, project: u64, workspace: u64, tab: u64) -> Option<Overlay> {
        if let Some(at) = self.projects.tab_in(project, workspace, tab) {
            self.close_tab(at);
        }
        None
    }

    fn submit_close_pane(&mut self, pane: u64, area: Rect) -> Result<Option<Overlay>> {
        self.pane_action(pane, PaneAction::Close, area)?;
        Ok(None)
    }

    pub(super) fn delete_group_message(&self, id: u64) -> Option<String> {
        let message = format!("Delete the group {}?", self.group(id)?.name);
        let (projects, tabs) = self
            .group_projects(id)
            .fold((0, 0), |(n, tabs), p| (n + 1, tabs + p.workspaces.iter().map(|w| w.tabs.len()).sum::<usize>()));
        let (which, them) = match projects {
            0 => return Some(message),
            1 => ("its project".to_string(), "it"),
            n => (format!("its {n} projects"), "them"),
        };
        let stops = match tabs {
            0 => String::new(),
            1 => ", which stops 1 tab and the programs running in it".into(),
            n => format!(", which stops {n} tabs and the programs running in them"),
        };
        Some(format!(
            "{message} Keep {which} open, ungrouped, or close {them} too{stops}. Folders and worktrees stay on disk."
        ))
    }

    pub(super) fn close_project_message(&self, id: u64) -> Option<String> {
        let project = &self.projects[self.projects.project_index(id)?];
        let stopped = stopped_tabs(project.workspaces.iter().map(|w| w.tabs.len()).sum());
        let closed: usize = project.workspaces.iter().map(|w| self.agents.closed.count(w.id)).sum();
        let forgotten = forgotten(closed);
        Some(format!(
            "Close the project {}?{stopped}{forgotten} Folders and worktrees stay on disk.",
            self.project_label(project)
        ))
    }

    pub(super) fn close_workspace_message(&self, project: u64, workspace: u64) -> Option<String> {
        let workspace = self.projects.workspace(self.projects.workspace_in(project, workspace)?);
        let closed = self.forgotten_note(workspace.id);
        Some(format!("Close the workspace {}?{}{closed}", workspace.label(), stopped_tabs(workspace.tabs.len())))
    }

    pub(super) fn close_tab_message(&self, project: u64, workspace: u64, tab: u64) -> Option<String> {
        let name = self.projects.tab(self.projects.tab_in(project, workspace, tab)?).label(&self.config);
        Some(format!("Close the tab {name}? The programs running in it are stopped."))
    }

    pub(super) fn close_pane_message(&self, pane: u64) -> Option<String> {
        let term = self.projects.pane_by(pane)?;
        let name = term.program(&self.config).unwrap_or_else(|| "?".into());
        Some(format!("Close the pane running {name}? What runs in it is stopped."))
    }
}

pub(super) fn confirm_view(
    title: &'static str,
    message: String,
    submit: &'static str,
    extra: Option<&'static str>,
) -> ui::Overlay {
    ui::Overlay::Confirm(ui::Confirm { title, message, note: None, submit, extra, dangerous: false })
}

fn stopped_tabs(tabs: usize) -> String {
    match tabs {
        0 => String::new(),
        1 => " Its tab and the programs running in it are stopped.".into(),
        n => format!(" Its {n} tabs and the programs running in them are stopped."),
    }
}
