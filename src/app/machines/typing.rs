use crossterm::event::KeyEvent;

use crate::app::App;
use crate::app::toast::{NOT_READING, Toast};
use crate::control::contract::Code;
use crate::control::terminal::{InputEvent, Key};
use crate::log;
use crate::machines::stream::orders::{Answer, Order};
use crate::ui;

impl App {
    pub(in crate::app) fn machine_key(&mut self, key: KeyEvent) -> bool {
        let shown = self.shown_remote().is_some();
        if let Some(key) = Key::of(&key).filter(|_| shown) {
            self.remote_input(InputEvent::Key(key));
        }
        shown
    }

    pub(in crate::app) fn machine_paste(&mut self, text: &str) -> bool {
        let shown = self.shown_remote().is_some();
        if shown {
            self.remote_input(InputEvent::Paste(text.into()));
        }
        shown
    }

    pub(in crate::app) fn nothing_sent(&mut self) {
        if let Some(shown) = self.shown_remote() {
            let message = format!("{} unreachable: nothing was sent", shown.machine);
            self.messages.toast = Some(Toast::new(message, ui::ToastIcon::Bug));
        }
    }

    pub(super) fn remote_answered(&mut self, stream: u64, answer: &Answer) {
        match answer {
            Answer::NotReading => self.messages.toast = Some(Toast::new(NOT_READING, ui::ToastIcon::Bug)),
            Answer::Refused { order, refusal } if refusal.code == Code::Unavailable && acted(order) => {
                self.nothing_sent();
            }
            Answer::Refused { .. } => {}
            Answer::Selected(text) => return self.copy_remote(text.as_deref()),
            Answer::Done { .. } => return,
        }
        log::debug!("machines", "remote order refused", stream = stream);
    }

    fn remote_input(&mut self, event: InputEvent) {
        let watched = self.machines.watched.as_mut().filter(|watched| watched.live().is_some());
        let Some(watched) = watched else { return self.nothing_sent() };
        let dropped = watched.type_in(event);
        self.typed_dropped(dropped);
    }
}

fn acted(order: &Order) -> bool {
    !matches!(order, Order::Size { .. } | Order::Show(_) | Order::Redraw(_))
}
