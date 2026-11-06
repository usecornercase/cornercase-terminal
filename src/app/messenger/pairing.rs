use std::time::{Duration, Instant};

use super::{Messenger, MessengerEvent, Phase, chat};
use crate::app::{App, AppEvent, Overlay, Toast};
use crate::control::contract::{Code, Refusal};
use crate::control::pair::PairKey;
use crate::issues::Secret;
use crate::log::{self, Job, Level};
use crate::messenger::{Advice, Chat, Owned, Via};
use crate::ui;
use crate::web::auth::Pairing;

pub(super) const NO_BOT: &str = "paste the bot's token first";
const SAVE_AGAIN_IN: Duration = Duration::from_mins(1);

impl Messenger {
    fn save_chat(&mut self, chat: Chat) -> Result<(), String> {
        let mut saved = self.saved.clone();
        saved.chat = Some(chat);
        saved
            .save(&self.path)
            .map_err(|e| format!("could not save the paired chat in {}: {e}", self.path.display()))?;
        self.saved = saved;
        Ok(())
    }
}

impl App {
    pub(in crate::app) fn drop_chat_code(&mut self, via: Via) {
        if !self.pairing_held(PairKey::Chat(via)) {
            let messenger = self.messenger_mut(via);
            messenger.pairing = None;
            messenger.owner.closed();
        }
    }

    pub(super) fn hold_chat_code(&mut self, via: Via, now: Instant) {
        let held = self.pairing_held(PairKey::Chat(via));
        let messenger = self.messenger_mut(via);
        if !via.pairs_by_owner() {
            match Pairing::shared(messenger.pairing.take().filter(|_| held), now) {
                Ok(pairing) => messenger.pairing = Some(pairing),
                Err(e) => log::error!(via.target(), "cannot make a pairing code", error = e),
            }
        } else if !held {
            messenger.owner.closed();
        }
    }

    pub(in crate::app) fn request_chat_code(&mut self, via: Via, now: Instant) -> Result<(), Refusal> {
        if let Some(refusal) = self.chat_down(via) {
            return Err(refusal);
        }
        self.hold_chat_code(via, now);
        let shown = self.chat_shown(via);
        if shown.url.is_some() {
            return Ok(());
        }
        Err(Refusal::new(Code::Failed, shown.lines.into_iter().next().unwrap_or_default()))
    }

    pub(in crate::app) fn chat_down(&self, via: Via) -> Option<Refusal> {
        let messenger = self.messenger(via);
        if messenger.bot().is_none() {
            return Some(Refusal::new(Code::Conflict, NO_BOT));
        }
        (messenger.phase == Phase::Rejected).then(|| Refusal::new(Code::Conflict, via.rejected()))
    }

    fn owner_due(&self, via: Via) -> bool {
        let messenger = self.messenger(via);
        via.pairs_by_owner()
            && self.pairing_held(PairKey::Chat(via))
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

    pub(super) fn connect(&mut self, via: Via, chat: Chat) -> bool {
        let (id, name) = (chat.id, chat.name.clone());
        let messenger = self.messenger_mut(via);
        if let Err(why) = messenger.save_chat(chat) {
            messenger.owner.later(SAVE_AGAIN_IN);
            messenger.owner.fail(via.target(), "cannot save the paired chat", why);
            return false;
        }
        log::info!(via.target(), "paired", chat = id);
        messenger.pairing = None;
        if matches!(self.overlay, Some(Overlay::Pairing(shown)) if shown == via) {
            self.overlay = None;
        }
        self.chat_paired(via, &name);
        self.messages.toast = Some(Toast::new(PairKey::Chat(via).connected(Some(&name)), ui::ToastIcon::Check));
        self.refresh_messenger_note(via);
        true
    }
}
