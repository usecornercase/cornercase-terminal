use crossterm::event::{KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use super::secrets_dialog::{ONE_LINE, pasted};
use super::section::access_words;
use crate::agents::Known;
use crate::app::App;
use crate::app::overlay::Overlay;
use crate::app::toast::Toast;
use crate::bots::asks::{Form, Hit, Step};
use crate::control::bots::{SecretAnswer, SecretGive};
use crate::ui;

impl App {
    pub(in crate::app) fn open_bot_ask(&mut self, key: u64, ask: u64) {
        let Some(at) = self.panels.bots.by_key(key) else { return };
        let Some(found) = self.panels.bots.live[at].asks.iter().find(|found| found.id == ask) else { return };
        self.overlay = Some(Overlay::BotAsk(Box::new(Form::new(key, found))));
    }

    fn bot_ask_view(&self, form: &Form) -> Option<ui::bot_ask::BotAsk> {
        let live = &self.panels.bots.live[self.panels.bots.by_key(form.bot)?];
        let ask = live.asks.iter().find(|ask| ask.id == form.ask)?;
        let bot = &live.bot;
        let (access, full) = access_words(bot.access);
        let fields = form.fields.iter().map(|field| ui::bot_ask::AskField {
            name: field.name.clone(),
            value: field.value.masked(),
            replaces: ask.replace && live.secrets.find(&field.name).is_some(),
        });
        Some(ui::bot_ask::BotAsk {
            title: format!("{} asks for a secret", bot.name),
            agent: Known::of(&bot.agent).map_or_else(|| bot.agent.clone(), |kind| kind.title().to_string()),
            access: access.to_string(),
            full,
            about: ask.about.clone(),
            fields: fields.collect(),
            focus: form.focus,
            error: form.error.clone(),
        })
    }

    pub(in crate::app) fn ask_view(&self, form: &Form) -> Option<ui::Overlay> {
        Some(ui::Overlay::BotAsk(Box::new(self.bot_ask_view(form)?)))
    }

    fn ask_form(&mut self) -> Option<&mut Form> {
        match &mut self.overlay {
            Some(Overlay::BotAsk(form)) => Some(form),
            _ => None,
        }
    }

    fn ask_width(&self, area: Rect, at: Option<usize>) -> usize {
        let Some(Overlay::BotAsk(form)) = &self.overlay else { return 1 };
        let Some(view) = self.bot_ask_view(form) else { return 1 };
        usize::from(ui::bot_ask::text(area, &view, at.unwrap_or(form.focus)).width.max(1))
    }

    pub(in crate::app) fn bot_ask_key(&mut self, key: KeyEvent, area: Rect) {
        let width = self.ask_width(area, None);
        let Some(form) = self.ask_form() else { return };
        let step = form.key(key, width);
        self.ask_step(step);
    }

    pub(in crate::app) fn bot_ask_mouse(&mut self, ev: MouseEvent, pos: Position, area: Rect) {
        if ev.kind != MouseEventKind::Down(MouseButton::Left) {
            return;
        }
        let Some(Overlay::BotAsk(form)) = &self.overlay else { return };
        let Some(hit) = self.bot_ask_view(form).and_then(|view| ui::bot_ask::hit(area, &view, pos)) else { return };
        let width = match hit {
            Hit::Field { at, .. } => self.ask_width(area, Some(at)),
            Hit::Button(_) => 1,
        };
        let Some(form) = self.ask_form() else { return };
        let step = form.hit(hit, width);
        self.ask_step(step);
    }

    pub(in crate::app) fn bot_ask_paste(&mut self, text: &str, area: Rect) {
        let width = self.ask_width(area, None);
        let Some(form) = self.ask_form() else { return };
        let text = pasted(text);
        if text.chars().any(char::is_control) {
            form.error = Some(String::from(ONE_LINE));
        } else {
            form.insert(text, width);
        }
    }

    fn ask_target(&self) -> Option<(String, u64)> {
        let Some(Overlay::BotAsk(form)) = &self.overlay else { return None };
        let at = self.panels.bots.by_key(form.bot)?;
        Some((self.panels.bots.live[at].bot.id.clone(), form.ask))
    }

    fn ask_step(&mut self, step: Step) {
        if step == Step::Later {
            self.overlay = None;
            return;
        }
        let Some((bot, ask)) = self.ask_target() else { return };
        match step {
            Step::None | Step::Later => {}
            Step::Never => {
                if let Err(refusal) = self.bot_secret_decline(None, &SecretAnswer { bot, ask }) {
                    self.overlay = None;
                    self.messages.toast = Some(Toast::new(refusal.message, ui::ToastIcon::Bug));
                }
            }
            Step::Save => {
                let values = self.ask_form().map(|form| form.values()).unwrap_or_default();
                let refused = self.bot_secret_give(None, &SecretGive { bot, ask, values }).err();
                if let (Some(refusal), Some(form)) = (refused, self.ask_form()) {
                    form.error = Some(refusal.message);
                }
            }
        }
    }
}
