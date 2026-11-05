use std::time::Duration;

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use super::App;
use crate::app::input::wheel;
use crate::app::toast::Toast;
use crate::control;
use crate::project::search::Goto;
use crate::project::{Hierarchy, Project, Workspace};
use crate::recap::text::{self, Block};
use crate::recap::{Choice, Place};
use crate::ui::ToastIcon;
use crate::ui::recap::{self, Hit, State, View};

impl App {
    pub(in crate::app) fn recap_view(&self, area: Rect) -> View {
        let choice = self.panels.recap.choice;
        let run = self.panels.recap.run(choice);
        let blocks: Vec<Block> =
            run.last.as_ref().map(|recap| text::parse(&recap.text, &recap.places)).unwrap_or_default();
        let line = run.last.as_ref().map(header_line).unwrap_or_default();
        let state = if run.running {
            State::Running(running_text(run.progress.load(std::sync::atomic::Ordering::Relaxed)))
        } else if let Some(error) = &run.error {
            State::Failed(error.clone())
        } else if let Some(empty) = run.empty.filter(|_| run.last.is_none()) {
            State::Empty(empty.to_string())
        } else {
            State::Idle
        };
        let panel = self.layout(area).shown(self.screen.nav).changes;
        let mut view = View {
            choice: choice.index(),
            line,
            state,
            blocks,
            scroll: 0,
            muted: self.muted_color(),
            busy: run.running,
        };
        let total = recap::total(panel, &view);
        let room = usize::from(recap::body(panel).height);
        view.scroll = self.panels.recap.scroll.min(total.saturating_sub(room));
        view
    }

    pub(in crate::app) fn recap_mouse(&mut self, ev: MouseEvent, pos: Position, panel: Rect, area: Rect) {
        let view = self.recap_view(area);
        if let Some(delta) = wheel(ev.kind) {
            let total = recap::total(panel, &view);
            let room = usize::from(recap::body(panel).height);
            let top = (isize::try_from(view.scroll).unwrap_or(0) + delta).max(0);
            self.panels.recap.scroll = usize::try_from(top).unwrap_or(0).min(total.saturating_sub(room));
            return;
        }
        if ev.kind != MouseEventKind::Down(MouseButton::Left) {
            return;
        }
        match recap::hit(panel, &view, pos) {
            Some(Hit::Close) => self.close_panel(),
            Some(Hit::Refresh | Hit::Retry) => self.retry_recap(),
            Some(Hit::Choice(i)) => self.show_recap(Choice::ALL[i.min(2)]),
            Some(Hit::Link(id)) => self.recap_go(&id),
            None => {}
        }
    }

    fn recap_place(&self, id: &str) -> Option<Place> {
        let choice = self.panels.recap.choice;
        self.panels.recap.run(choice).last.as_ref()?.places.get(id).cloned()
    }

    pub(in crate::app) fn recap_go(&mut self, id: &str) {
        let Some(place) = self.recap_place(id) else { return };
        if place.kind == "pull" {
            self.recap_open_pull(&place);
            return;
        }
        if let Some(pane) = self.recap_live_pane(&place) {
            self.jump_to_pane(pane);
        } else if !self.recap_goto_place(&place) {
            self.messages.toast = Some(Toast::new(format!("{} is closed", place.name()), ToastIcon::Check));
        }
    }

    fn recap_live_pane(&self, place: &Place) -> Option<u64> {
        let (agent, conversation) = (place.agent.as_deref()?, place.conversation.as_deref()?);
        self.projects
            .terms()
            .find(|term| {
                term.context.kind().is_some_and(|kind| kind.name() == agent)
                    && term.context.conversation() == Some(conversation)
            })
            .map(|term| term.id)
            .or_else(|| {
                let pane = place.pane.filter(|_| place.boot.as_deref() == Some(control::server_token()))?;
                self.projects.pane_by(pane).map(|term| term.id)
            })
    }

    fn recap_goto_place(&mut self, place: &Place) -> bool {
        let Some(path) = place.project.as_ref().map(|named| named.path.as_str()) else { return false };
        let Some(project) = self.projects.iter().find(|project| project.path.to_string_lossy() == path) else {
            return false;
        };
        let workspace = place
            .workspace
            .as_ref()
            .and_then(|named| project.workspaces.iter().find(|w| w.path.to_string_lossy() == named.path))
            .map(|workspace: &Workspace| workspace.id);
        let project: &Project = project;
        self.goto(Goto::Place { project: project.id, workspace, tab: None });
        true
    }

    fn recap_open_pull(&mut self, place: &Place) {
        let workspace = place.workspace.as_ref().and_then(|named| {
            self.projects.workspaces().find(|w| w.path.to_string_lossy() == named.path).map(|w| w.id)
        });
        match workspace {
            Some(workspace) => self.open_pull_request(workspace),
            None => {
                if let Some(url) = place.url.clone() {
                    self.copy(&url);
                }
            }
        }
    }
}

fn running_text(steps: usize) -> String {
    if steps == 0 {
        String::from("reading your conversations…")
    } else {
        format!("reading your conversations… {steps} steps")
    }
}

fn header_line(recap: &crate::recap::Recap) -> String {
    let title = crate::agents::Known::of(&recap.agent).map_or(recap.agent.as_str(), |kind| kind.title());
    let mut parts = vec![recap.label.clone(), title.to_string()];
    parts.push(format!("{} conversation{}", recap.conversations, if recap.conversations == 1 { "" } else { "s" }));
    if recap.tokens.total() > 0 {
        parts.push(format!("{} tokens", crate::agents::stats::tokens(recap.tokens.total())));
    }
    let age = u64::try_from(crate::time::now() - recap.written_at).unwrap_or(0);
    parts.push(crate::time::ago(Duration::from_secs(age)));
    parts.join(" · ")
}
