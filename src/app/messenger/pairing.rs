use std::time::{Duration, Instant};

use super::{MessengerEvent, chat};
use crate::app::{App, AppEvent, Overlay, Toast};
use crate::issues::Secret;
use crate::log::{self, Job, Level};
use crate::messenger::{Advice, Chat, Owned, Via};
use crate::ui;

impl App {
    fn owner_due(&self, via: Via) -> bool {
        let messenger = self.messenger(via);
        via.pairs_by_owner()
            && matches!(self.overlay, Some(Overlay::Pairing(shown)) if shown == via)
            && messenger.token.is_some()
            && messenger.owner.idle()
    }

    pub(in crate::app) fn next_pairing(&self, now: Instant) -> Option<Duration> {
        Via::ALL.into_iter().filter(|via| self.owner_due(*via)).map(|via| self.pairing_wait(via, now)).min()
    }

    fn pairing_wait(&self, via: Via, now: Instant) -> Duration {
        self.messenger(via).owner.wait(now)
    }

    pub(in crate::app) fn pair_by_owner(&mut self, now: Instant) {
        for via in Via::ALL {
            if self.owner_due(via) && self.pairing_wait(via, now).is_zero() {
                self.ask_owner(via, now);
            }
        }
    }

    fn ask_owner(&mut self, via: Via, now: Instant) {
        let messenger = self.messenger_mut(via);
        let Some(token) = messenger.token.clone() else { return };
        messenger.owner.start(now);
        let api = messenger.api.clone();
        let job = Job::new(Level::Debug, via.target(), "pair");
        let secret = Secret(token.clone());
        self.spawn_job(
            job,
            move || Ok(via.pair(&api, &token, &chat::welcome(via))),
            move |answer, _| {
                let (advice, result) = answer.unwrap_or_else(|e| (Advice::Unread, Err(e.to_string())));
                Some(AppEvent::Messenger(MessengerEvent::Paired { via, token: secret, advice, result }))
            },
        );
    }

    pub(super) fn owner_answered(&mut self, via: Via, token: &Secret, advice: Advice, result: Result<Owned, String>) {
        let messenger = self.messenger_mut(via);
        messenger.owner.landed();
        if messenger.token.as_deref() != Some(token.0.as_str()) {
            return;
        }
        if let Advice::Read(advice) = advice
            && messenger.advice != advice
        {
            messenger.advice = advice;
            self.refresh_messenger_note(via);
        }
        let messenger = self.messenger_mut(via);
        match result {
            Ok(Owned::Chat(chat)) => {
                messenger.owner.closed();
                self.connect(via, chat);
            }
            Ok(Owned::Later(wait)) => messenger.owner.later(wait),
            Err(why) => messenger.owner.fail(via.target(), "cannot pair", why),
        }
    }

    pub(super) fn connect(&mut self, via: Via, chat: Chat) {
        log::info!(via.target(), "paired", chat = chat.id);
        let who = if chat.name.is_empty() { String::new() } else { format!(" with {}", chat.name) };
        let messenger = self.messenger_mut(via);
        messenger.pairing = None;
        messenger.saved.chat = Some(chat);
        messenger.save();
        if matches!(self.overlay, Some(Overlay::Pairing(shown)) if shown == via) {
            self.overlay = None;
        }
        self.messages.toast = Some(Toast::new(format!("{} connected{who}", via.name()), ui::ToastIcon::Check));
        self.refresh_messenger_note(via);
    }
}
