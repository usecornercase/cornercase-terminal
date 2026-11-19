use ratatui::layout::Rect;

use crate::app::App;
use crate::app::flow_panel::COPY;
use crate::app::overlay::{OPEN_SUBMIT, Overlay};
use crate::app::toast::Toast;
use crate::bots::chat::Kind;
use crate::host::open::{Link, link_host};
use crate::text::{truncate_left, truncate_right};
use crate::ui;

const ADDRESS_ROWS: usize = 3;
const ASKS: &str = " wrote this address. Open it in your browser?";

impl App {
    pub(in crate::app) fn press_link(&mut self, at: usize, drawn: &str) {
        let url = self.whole_address(drawn);
        if !self.can_browse() {
            return self.copy(&url);
        }
        if let Err(reason) = link_host(&url) {
            return self.copy_refused(&url, reason);
        }
        self.overlay = Some(Overlay::OpenLink { bot: self.panels.bots.live[at].key, url });
    }

    fn whole_address(&self, drawn: &str) -> String {
        let Some(shown) = &self.panels.bots.shown else { return drawn.to_string() };
        let answers = shown.lines.iter().rev().filter(|line| line.kind == Kind::Bot).map(|line| line.text.as_str());
        ui::whole_address(drawn, answers)
    }

    fn copy_refused(&mut self, url: &str, reason: &str) {
        self.copy(url);
        let message = format!("copied the address: {reason}, so it is not opened");
        self.messages.toast = Some(Toast::new(message, ui::ToastIcon::Check));
    }

    pub(in crate::app) fn open_link_view(&self, bot: u64, url: &str, area: Rect) -> Option<ui::Overlay> {
        let name = &self.panels.bots.live[self.panels.bots.by_key(bot)?].bot.name;
        let width = ui::confirm_width(area).max(1);
        let host = link_host(url).unwrap_or_default();
        let room = width.saturating_sub("goes to ".len());
        Some(ui::Overlay::Confirm(ui::Confirm {
            title: "Open link",
            message: link_message(name, url, width),
            note: Some(ui::Note::Info(format!("goes to {}", truncate_left(host, room)))),
            submit: OPEN_SUBMIT,
            extra: Some(COPY),
            dangerous: false,
        }))
    }

    pub(in crate::app) fn submit_open_link(&mut self, url: String) -> Option<Overlay> {
        if !self.can_browse() {
            self.copy(&url);
            return None;
        }
        if let Err(reason) = link_host(&url) {
            self.copy_refused(&url, reason);
            return None;
        }
        self.send_link(Link { url });
        None
    }

    pub(in crate::app) fn copy_open_link(&mut self) {
        if let Some(Overlay::OpenLink { url, .. }) = self.overlay.take() {
            self.copy(&url);
        }
    }
}

fn link_message(name: &str, url: &str, width: usize) -> String {
    let room = width.saturating_sub(ASKS.len());
    let first = if room >= 2 {
        format!("{}{ASKS}", truncate_right(name, room))
    } else {
        truncate_right(&format!("{name}{ASKS}"), width)
    };
    let mut rows = vec![first];
    let chunks: Vec<&str> = url.as_bytes().chunks(width).filter_map(|chunk| std::str::from_utf8(chunk).ok()).collect();
    if chunks.len() <= ADDRESS_ROWS {
        rows.extend(chunks.iter().map(|chunk| (*chunk).to_string()));
    } else {
        rows.extend(chunks[..ADDRESS_ROWS - 1].iter().map(|chunk| (*chunk).to_string()));
        let rest = &url[width * (ADDRESS_ROWS - 1)..];
        rows.push(truncate_right(rest, width));
    }
    rows.join("\n")
}
