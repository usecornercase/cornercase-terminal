use ratatui::layout::Rect;

use super::App;
use super::toast::NOT_READING;
use super::toast::Toast;
use crate::agents;
use crate::changes::Checkout;
use crate::error::Result;
use crate::host::clipboard;
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
        let Some((p, w)) = self.projects.workspace_index(target.workspace) else { return Ok(()) };
        let (rows, cols) = self.pane_size(area);
        let id = self.take_id();
        let file = target.dir.join(path).display().to_string();
        let args = ["-c".to_string(), EDIT_SCRIPT.to_string(), "sh".to_string(), line.max(1).to_string(), file];
        let env = [self.pane_env(id, None), self.editor_env.clone()].concat();
        let opts = SpawnOptions {
            id,
            shell: "/bin/sh",
            args: &args,
            env: &env,
            rows,
            cols,
            cwd: Some(target.dir.clone()),
            theme: self.terminal_theme(),
        };
        let term = Term::spawn(opts, self.tx.clone())?;
        let tab = Tab::new(self.take_id(), None, term);
        let place = Goto::Place { project: self.projects[p].id, workspace: Some(target.workspace), tab: Some(tab.id) };
        let workspace = &mut self.projects[p].workspaces[w];
        workspace.tabs.push(tab);
        workspace.active = workspace.tabs.len() - 1;
        self.goto(place);
        Ok(())
    }

    pub(super) fn ask_agent(&mut self, workspace: u64, text: &str) {
        let Some((p, w)) = self.projects.workspace_index(workspace) else { return };
        let ws = &self.projects[p].workspaces[w];
        let tabs = std::iter::once(ws.active).chain(0..ws.tabs.len()).filter(|&t| t < ws.tabs.len());
        let found = tabs.into_iter().find_map(|t| {
            let tab = &ws.tabs[t];
            let panes = std::iter::once(tab.active).chain(0..tab.panes.len()).filter(|&i| i < tab.panes.len());
            panes
                .into_iter()
                .find(|&i| agents::detect(&self.config, &tab.panes[i].foreground_args()).is_some())
                .map(|i| (t, tab.panes[i].id))
        });
        let Some((t, id)) = found else {
            self.outbox.host_writes.push(clipboard::osc52(text.trim_end()));
            self.toast = Some(Toast::new(NO_AGENT, ui::ToastIcon::Check));
            return;
        };
        if self.resume_holds(id) || self.sending(id) {
            self.outbox.host_writes.push(clipboard::osc52(text.trim_end()));
            self.jump_to_pane(id);
            let why = if self.sending(id) { BUSY } else { RESUMING };
            self.toast = Some(Toast::new(why, ui::ToastIcon::Check));
            return;
        }
        let tab = &mut self.projects[p].workspaces[w].tabs[t];
        let sent = tab.pane_by_mut(id).is_some_and(|term| {
            term.emulator.scroll_to_bottom();
            term.typed_paste(text);
            term.paste(text)
        });
        self.jump_to_pane(id);
        self.toast = Some(if sent {
            Toast::new(SENT_TO_AGENT, ui::ToastIcon::Check)
        } else {
            Toast::new(NOT_READING, ui::ToastIcon::Bug)
        });
    }
}
