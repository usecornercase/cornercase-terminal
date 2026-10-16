use ratatui::layout::Rect;

use super::{Graphics, OTHER_BUILD, Server};
use crate::error::Result;
use crate::host::pointer;
use crate::log;
use crate::persist::{self, Saver};
use crate::protocol::{self, Hello};
use crate::state;

impl Server {
    pub(super) fn hello(&mut self, id: u64, hello: Hello) {
        if hello.version != protocol::VERSION || hello.build != self.build {
            log::warning!("server", "client from another build", client = id, build = hello.build);
            self.reject(id, OTHER_BUILD);
            return;
        }
        log::info!(
            "server",
            "client attached",
            client = id,
            cols = hello.cols,
            rows = hello.rows,
            terminal = hello.terminal.as_deref().unwrap_or("unknown"),
            notify = hello.notify.id(),
            pointer = hello.pointer.map_or("none", pointer::Terminal::id),
            background = if hello.theme.background.is_some() { "known" } else { "unknown" },
            truecolor = hello.theme.truecolor,
            images = hello.graphics.images(),
            cell = hello.graphics.cell_size(),
            tmux = hello.graphics.tmux.id(),
            missing = hello.graphics.why_not(),
            id_hi = hello.graphics.id_hi,
            probe = hello.probe,
        );
        let Some(client) = self.client_mut(id) else { return };
        client.size = Some((hello.cols, hello.rows));
        client.notify = hello.notify;
        client.zone = hello.zone;
        client.pointer = hello.pointer;
        let background = hello.theme.background.map(|c| (c.r, c.g, c.b));
        client.graphics = Graphics::new(id, hello.graphics, background);
        self.touch(id);
        self.fit(Some(id));
        if !self.started {
            self.started = true;
            self.app.set_theme(hello.theme);
            if let Err(e) = self.open_first_terminals() {
                log::error!("server", "could not open a first terminal", error = e);
            }
            self.app.start_plugins();
        }
    }

    pub(super) fn open_first_terminals(&mut self) -> Result<()> {
        let area = self.area.unwrap_or_default();
        let saved = state::load(&self.session);
        let complete = saved.as_ref().is_none_or(|saved| self.app.restore(saved, area));
        match &saved {
            Some(saved) => log::info!(
                "server",
                "session restored",
                projects = saved.projects.len(),
                complete = complete,
                path = self.session.display()
            ),
            None => log::info!("server", "no session to restore", path = self.session.display()),
        }
        if !complete {
            match persist::back_up(&self.session) {
                Ok(backup) => log::warning!("server", "the session came back incomplete", copy = backup.display()),
                Err(e) => log::error!("server", "failed to keep a copy of the session", error = e),
            }
        }
        self.saver = Saver::new(self.session.clone(), saved);
        if self.app.is_empty() || !(complete || self.app.has_terms()) {
            self.app.open_here(area)?;
        }
        Ok(())
    }

    pub(super) fn fit(&mut self, resized: Option<u64>) {
        let latest = self.clients.iter().filter(|c| c.size.is_some()).max_by_key(|c| c.used).and_then(|c| c.size);
        let Some((cols, rows)) = latest else { return };
        let area = Rect::new(0, 0, cols, rows);
        let area_changed = self.area != Some(area);
        if area_changed {
            log::info!("server", "size", cols = cols, rows = rows);
        }
        self.area = Some(area);
        for client in self.clients.iter_mut().filter(|c| area_changed || Some(c.id) == resized) {
            client.reset_screen(area);
        }
    }
}
