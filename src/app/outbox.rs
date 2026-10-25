use crate::host::clipboard;
use crate::host::notify::Notification;
use crate::host::open::{Browse, Open};
use crate::host::sound::Cue;

#[derive(Default)]
pub(super) struct Outbox {
    host_writes: Vec<Vec<u8>>,
    notifications: Vec<Notification>,
    sounds: Vec<Cue>,
    opens: Vec<(u64, Open)>,
    browses: Vec<(u64, Browse)>,
    detach: bool,
    restart: bool,
    start: Option<(u64, (u16, u16))>,
}

impl Outbox {
    pub(super) fn copy(&mut self, text: &str) {
        self.host_writes.push(clipboard::osc52(text));
    }

    pub(super) fn notify(&mut self, notification: Option<Notification>) {
        self.notifications.extend(notification);
    }

    pub(super) fn play(&mut self, cue: Cue) {
        self.sounds.push(cue);
    }

    pub(super) fn open(&mut self, client: u64, open: Open) {
        self.opens.push((client, open));
    }

    pub(super) fn browse(&mut self, client: u64, browse: Browse) {
        self.browses.push((client, browse));
    }

    pub(super) fn detach(&mut self) {
        self.detach = true;
    }

    pub(super) fn restart(&mut self) {
        self.restart = true;
    }

    pub(super) fn start(&mut self, client: u64, size: (u16, u16)) {
        self.start = Some((client, size));
    }

    pub(super) fn take_host_writes(&mut self) -> Vec<Vec<u8>> {
        std::mem::take(&mut self.host_writes)
    }

    pub(super) fn take_notifications(&mut self) -> Vec<Notification> {
        std::mem::take(&mut self.notifications)
    }

    pub(super) fn take_sounds(&mut self) -> Vec<Cue> {
        std::mem::take(&mut self.sounds)
    }

    pub(super) fn take_opens(&mut self) -> Vec<(u64, Open)> {
        std::mem::take(&mut self.opens)
    }

    pub(super) fn take_browses(&mut self) -> Vec<(u64, Browse)> {
        std::mem::take(&mut self.browses)
    }

    pub(super) fn take_detach(&mut self) -> bool {
        std::mem::take(&mut self.detach)
    }

    pub(super) fn take_restart(&mut self) -> bool {
        std::mem::take(&mut self.restart)
    }

    pub(super) fn take_start(&mut self) -> Option<(u64, (u16, u16))> {
        self.start.take()
    }
}
