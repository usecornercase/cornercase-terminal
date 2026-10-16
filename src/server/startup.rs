use ratatui::layout::Rect;

use super::{Graphics, OTHER_BUILD, Server};
use crate::control::{
    self,
    start_server::{SizeFrom, Started},
};
use crate::error::Result;
use crate::host::pointer;
use crate::log;
use crate::persist::{self, Saver};
use crate::protocol::{self, Hello};
use crate::state;
use crate::version;

pub(super) struct Opened {
    restored: bool,
    complete: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Startup {
    Waiting,
    Headless,
    Windowed,
}

impl Server {
    pub(super) fn started(&self) -> bool {
        self.startup != Startup::Waiting
    }

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
        if !self.started() {
            self.startup = Startup::Windowed;
            self.app.set_theme(hello.theme);
            if let Err(e) = self.open_first_terminals() {
                log::error!("server", "could not open a first terminal", error = e);
            }
            self.app.start_plugins();
        } else if self.startup == Startup::Headless {
            self.startup = Startup::Windowed;
            log::info!("server", "first window gives its colours to the open terminals", client = id);
            self.app.retheme(hello.theme);
        }
    }

    pub(super) fn start_server(&mut self, client: u64, (cols, rows): (u16, u16)) {
        let already = self.started();
        if self.area.is_none() {
            self.area = Some(Rect::new(0, 0, cols, rows));
            self.size_from = SizeFrom::StartServer;
            log::info!("server", "size", cols = cols, rows = rows, from = "start-server");
        }
        let area = self.area.unwrap_or_default();
        let mut opened = Opened { restored: false, complete: true };
        if !already {
            self.startup = Startup::Headless;
            self.app.set_queue_area(area);
            match self.open_first_terminals() {
                Ok(done) => opened = done,
                Err(e) => log::error!("server", "could not open a first terminal", error = e),
            }
            self.app.start_plugins();
        }
        let started = Started {
            already,
            restored: opened.restored,
            complete: opened.complete,
            cols: area.width,
            rows: area.height,
            size: self.size_from,
            boot: control::server_token().to_string(),
            version: version::CURRENT.to_string(),
        };
        log::info!("server", "start-server answered", client = client, already = already);
        self.app.answer_start(client, &started);
    }

    pub(super) fn open_first_terminals(&mut self) -> Result<Opened> {
        let area = self.area.unwrap_or_default();
        let saved = state::load(&self.session);
        let restored = saved.is_some();
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
        Ok(Opened { restored, complete })
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
        self.size_from = SizeFrom::Window;
        for client in self.clients.iter_mut().filter(|c| area_changed || Some(c.id) == resized) {
            client.reset_screen(area);
        }
    }
}

#[cfg(test)]
mod tests;
