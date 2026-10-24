use std::time::{Duration, Instant};

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use super::super::MessengerEvent;
use super::tags::Labels;
use super::{NAME, Setup};
use crate::app::{App, AppEvent, Overlay, Toast};
use crate::discord::forum::{Progress, guild_install_link};
use crate::discord::{Bot, Failure};
use crate::issues::Secret;
use crate::log::{self, Job, Level};
use crate::messenger::{Forum, Via};
use crate::text::CRUMB_SEPARATOR;
use crate::ui;

const INSTALL: &str = "open the link and add the app to your own server.";
const NO_SERVER: &str = "no server yet? In Discord: + › Create My Own › For me and my friends";
const IN_THE_BACKGROUND: Duration = Duration::from_secs(30);

impl App {
    pub(super) fn showing_forum(&self, via: Via) -> bool {
        matches!(self.overlay, Some(Overlay::Forum(shown)) if shown == via)
    }

    fn forum_wanted(&self, via: Via) -> bool {
        let messenger = self.messenger(via);
        let unready =
            via == Via::Discord && self.config.discord_forum && messenger.paired() && !messenger.forum_ready();
        self.showing_forum(via) || unready
    }

    fn forum_due(&self, via: Via) -> bool {
        let messenger = self.messenger(via);
        self.forum_wanted(via) && messenger.token.is_some() && messenger.forum.setup.every.idle()
    }

    fn forum_wait(&self, via: Via, now: Instant) -> Duration {
        let every = &self.messenger(via).forum.setup.every;
        if self.showing_forum(via) { every.wait(now) } else { every.wait_every(now, IN_THE_BACKGROUND) }
    }

    pub(in crate::app) fn next_forum_setup(&self, now: Instant) -> Option<Duration> {
        let due = Via::ALL.into_iter().filter(|via| self.forum_due(*via));
        due.map(|via| self.forum_wait(via, now)).min()
    }

    pub(in crate::app) fn set_up_forums(&mut self, now: Instant) {
        for via in Via::ALL {
            if self.forum_due(via) && self.forum_wait(via, now).is_zero() {
                self.read_forum(via, now);
            }
        }
    }

    pub(in crate::app) fn open_forum(&mut self, via: Via) {
        let setup = &mut self.messenger_mut(via).forum.setup;
        setup.every.closed();
        setup.relink = false;
        self.overlay = Some(Overlay::Forum(via));
    }

    pub(in crate::app::messenger) fn close_forum_setup(&mut self, via: Via) {
        if !self.forum_wanted(via) {
            self.messenger_mut(via).forum.setup.every.closed();
        }
    }

    fn read_forum(&mut self, via: Via, now: Instant) {
        let messenger = self.messenger_mut(via);
        let Some(token) = messenger.token.clone() else { return };
        messenger.forum.setup.every.start(now);
        let (chosen, saved) = (messenger.chosen_server(), messenger.saved.forum.clone());
        let api = messenger.api.clone();
        let job = Job::new(Level::Debug, via.target(), "set up the forum");
        let secret = Secret(token.clone());
        self.spawn_job(
            job,
            move || Ok(Bot::new(&api, &token).set_up(chosen, saved.as_ref())),
            move |answer, _| {
                let read = answer.unwrap_or_else(|e| Err(Failure::Unreachable(e.to_string())));
                Some(AppEvent::Messenger(MessengerEvent::Forum { via, token: secret, read }))
            },
        );
    }

    pub(in crate::app::messenger) fn forum_read(&mut self, via: Via, token: &Secret, read: Result<Progress, Failure>) {
        let messenger = self.messenger_mut(via);
        messenger.forum.setup.every.landed();
        if messenger.token.as_deref() != Some(token.0.as_str()) || !self.forum_wanted(via) {
            return;
        }
        let before = self.messenger_settings(via);
        let messenger = self.messenger_mut(via);
        match read {
            Ok(Progress { checklist, forum }) => {
                let setup = &mut messenger.forum.setup;
                setup.checklist = Some(checklist);
                match forum {
                    Some(Ok(forum)) => self.forum_made(via, forum),
                    Some(Err(failure)) => self.forum_failed(via, &failure),
                    None => {
                        setup.relink = false;
                        setup.every.later(Duration::ZERO);
                    }
                }
            }
            Err(failure) => self.forum_failed(via, &failure),
        }
        if self.messenger_settings(via) != before {
            self.refresh_messenger_note(via);
        }
    }

    fn forum_failed(&mut self, via: Via, failure: &Failure) {
        let setup = &mut self.messenger_mut(via).forum.setup;
        if let Failure::Wait(seconds) = failure {
            setup.every.later(Duration::from_secs(*seconds));
            return;
        }
        setup.relink = matches!(failure, Failure::Missing(_) | Failure::Removed);
        if *failure == Failure::Removed {
            setup.chosen = None;
        }
        setup.every.fail(via.target(), "cannot set up the forum", failure.to_string());
    }

    fn forum_made(&mut self, via: Via, forum: Forum) {
        log::info!(via.target(), "forum ready", guild = forum.guild, channel = forum.channel);
        let place = format!("{}{CRUMB_SEPARATOR}#{NAME}", forum.guild_name);
        let messenger = self.messenger_mut(via);
        messenger.forum.setup = Setup::default();
        if messenger.saved.forum.as_ref().is_some_and(|old| old.channel != forum.channel) {
            messenger.forum.posts.forget();
            messenger.forum.labels = Labels::default();
        }
        messenger.saved.forum = Some(forum);
        messenger.save();
        if self.showing_forum(via) {
            self.overlay = None;
        }
        let text = format!("{} forum ready in {place}", via.name());
        self.messages.toast = Some(Toast::new(text, ui::ToastIcon::Check));
        self.restart_link(via);
    }

    pub(in crate::app) fn forum_view(&self, via: Via) -> ui::ForumSetup {
        let messenger = self.messenger(via);
        let setup = &messenger.forum.setup;
        let checklist = setup.checklist.clone().unwrap_or_default();
        let steps = messenger.forum_steps(&checklist);
        let picking = checklist.servers.len() > 1 && checklist.pick(messenger.chosen_server()).is_none();
        let servers =
            if picking { checklist.servers.iter().map(|server| server.name.clone()).collect() } else { Vec::new() };
        let linking = checklist.guild_install && (checklist.servers.is_empty() || setup.relink);
        let url = linking.then(|| messenger.token.as_deref().and_then(guild_install_link)).flatten();
        ui::ForumSetup {
            steps: steps.to_vec(),
            servers,
            code: url.as_deref().map(ui::remote_access::code).unwrap_or_default(),
            notes: if url.is_some() { vec![INSTALL.into(), NO_SERVER.into()] } else { Vec::new() },
            url,
            problem: setup.every.failed.clone(),
        }
    }

    pub(in crate::app) fn forum_mouse(&mut self, via: Via, ev: MouseEvent, pos: Position, area: Rect) {
        if ev.kind != MouseEventKind::Down(MouseButton::Left) {
            return;
        }
        match ui::forum_hit(area, &self.forum_view(via), pos) {
            Some(ui::ForumHit::Done) => self.overlay = None,
            Some(ui::ForumHit::TurnOff) => self.turn_off_forum(),
            Some(ui::ForumHit::Server(i)) => self.pick_forum_server(via, i),
            None => {}
        }
    }

    fn pick_forum_server(&mut self, via: Via, i: usize) {
        let setup = &mut self.messenger_mut(via).forum.setup;
        let server = setup.checklist.as_ref().and_then(|checklist| checklist.servers.get(i)).map(|server| server.id);
        if server.is_some() {
            setup.chosen = server;
            setup.every.closed();
        }
    }

    fn turn_off_forum(&mut self) {
        let mut config = self.config.clone();
        config.discord_forum = false;
        if let Err(why) = self.write_config(&config) {
            self.messages.toast = Some(Toast::new(why, ui::ToastIcon::Bug));
            return;
        }
        self.overlay = None;
        self.messages.toast = Some(Toast::new("forum mode is off: agents message the DM", ui::ToastIcon::Check));
    }
}
