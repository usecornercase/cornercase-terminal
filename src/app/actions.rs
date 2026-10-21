use ratatui::layout::Rect;

use super::App;
use super::toast::NOT_READING;
use super::toast::Toast;
use crate::accounts::Accounts;
use crate::agents;
use crate::changes::Checkout;
use crate::error::Result;
use crate::project::search::Goto;
use crate::project::{Hierarchy, Tab};
use crate::term::{SpawnOptions, Term};
use crate::ui::{self};

const SENT_TO_AGENT: &str = "sent to the agent";

pub(super) const NO_AGENT: &str = "no agent here, so the reference is copied";

const RESUMING: &str = "the agent is resuming its conversation, so the reference is copied";

const BUSY: &str = "a message is going into the agent's input, so the reference is copied";

const EDIT_SCRIPT: &str = "exec ${VISUAL:-${EDITOR:-vi}} \"+$1\" \"$2\"";

impl App {
    pub(super) fn open_in_editor(&mut self, target: &Checkout, path: &str, line: u32, area: Rect) -> Result<()> {
        let Some(at) = self.projects.workspace_index(target.workspace) else { return Ok(()) };
        let (rows, cols) = self.pane_size(area);
        let id = self.take_id();
        let file = target.dir.join(path).display().to_string();
        let args = ["-c".to_string(), EDIT_SCRIPT.to_string(), "sh".to_string(), line.max(1).to_string(), file];
        let env = [self.pane_env(id, None, &Accounts::default()), self.shells.editor_env().to_vec()].concat();
        let opts = SpawnOptions {
            id,
            shell: "/bin/sh",
            args: &args,
            env: &env,
            remove: &[],
            rows,
            cols,
            cwd: Some(target.dir.clone()),
            theme: self.terminal_theme(),
        };
        let term = Term::spawn(opts, self.tx.clone())?;
        let tab = Tab::new(self.take_id(), None, term);
        let project = self.projects[at.project()].id;
        let place = Goto::Place { project, workspace: Some(target.workspace), tab: Some(tab.id) };
        let workspace = self.projects.workspace_mut(at);
        workspace.tabs.push(tab);
        workspace.active = workspace.tabs.len() - 1;
        self.goto(place);
        Ok(())
    }

    pub(super) fn ask_agent(&mut self, workspace: u64, text: &str) {
        let Some(at) = self.projects.workspace_index(workspace) else { return };
        let ws = self.projects.workspace(at);
        let found = ws.tab().into_iter().chain(&ws.tabs).find_map(|tab| {
            let mut panes = tab.pane().into_iter().chain(&tab.panes);
            panes.find(|term| agents::detect(&self.config, &term.foreground_args()).is_some()).map(|term| term.id)
        });
        let Some(id) = found else {
            self.outbox.copy(text.trim_end());
            self.messages.toast = Some(Toast::new(NO_AGENT, ui::ToastIcon::Check));
            return;
        };
        if self.resume_holds(id) || self.sending(id) {
            self.outbox.copy(text.trim_end());
            self.jump_to_pane(id);
            let why = if self.sending(id) { BUSY } else { RESUMING };
            self.messages.toast = Some(Toast::new(why, ui::ToastIcon::Check));
            return;
        }
        let sent = self.projects.pane_by_mut(id).is_some_and(|term| {
            term.emulator.scroll_to_bottom();
            term.typed_paste(text);
            term.paste(text)
        });
        self.jump_to_pane(id);
        self.messages.toast = Some(if sent {
            Toast::new(SENT_TO_AGENT, ui::ToastIcon::Check)
        } else {
            Toast::new(NOT_READING, ui::ToastIcon::Bug)
        });
    }
}
