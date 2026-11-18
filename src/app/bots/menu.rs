use ratatui::layout::Position;

use crate::app::App;
use crate::app::dialogs::rename_form;
use crate::app::menus::MenuAction;
use crate::app::opening::Subject;
use crate::app::overlay::{Overlay, REMOVE_SUBMIT, RENAME_SUBMIT};
use crate::app::toast::Toast;
use crate::bots::routines::{Button, buttons};
use crate::control::bots::EditBot;
use crate::editor::Editor;
use crate::ui;

pub(in crate::app) const STOP_FIRST: &str = "stop it first";

const RENAME_HINT: &str = "leave it empty to keep the current name";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::app) enum BotItem {
    Rename,
    Edit,
    Routines,
    NewConversation,
    OpenFolder,
    Remove,
    Routine(u64, Button),
}

impl App {
    pub(in crate::app) fn bot_menu_label(action: MenuAction) -> Option<String> {
        let MenuAction::Bot(_, item) = action else { return None };
        Some(
            match item {
                BotItem::Routine(_, button) => button.label(),
                BotItem::Rename => "rename",
                BotItem::Edit => "edit…",
                BotItem::Routines => "routines…",
                BotItem::NewConversation => "new conversation",
                BotItem::OpenFolder => "open folder",
                BotItem::Remove => "remove…",
            }
            .to_string(),
        )
    }

    pub(in crate::app) fn bot_menu_disabled(&self, action: MenuAction) -> bool {
        let MenuAction::Bot(key, BotItem::NewConversation) = action else { return false };
        self.panels.bots.by_key(key).is_some_and(|at| self.panels.bots.live[at].turn.is_some())
    }

    pub(in crate::app) fn bot_action(&mut self, action: MenuAction, at: Position) -> Option<MenuAction> {
        let MenuAction::Bot(key, item) = action else { return Some(action) };
        let bot = self.panels.bots.by_key(key)?;
        match item {
            BotItem::Rename => self.ask_rename_bot(key),
            BotItem::Edit => self.open_bot_form(Some(key)),
            BotItem::Routines => self.open_bot_routines(key, None),
            BotItem::NewConversation => {
                let id = self.panels.bots.live[bot].bot.id.clone();
                self.new_conversation(&id);
            }
            BotItem::OpenFolder => self.open_menu_or_copy(Subject::Bot(key), at),
            BotItem::Remove => self.overlay = Some(Overlay::RemoveBot { bot: key }),
            BotItem::Routine(entry, button) => self.press_card_routine(bot, entry, button),
        }
        None
    }

    fn press_card_routine(&mut self, at: usize, entry: u64, button: Button) {
        let id = &self.panels.bots.live[at].bot.id;
        let yours = self.panels.queued.queue().entry(entry).is_some_and(|found| {
            found.is_routine() && found.bot() == Some(id.as_str()) && buttons(found).contains(&button)
        });
        if yours {
            self.press_routine(entry, button);
        }
    }

    pub(in crate::app) fn open_routine_menu(&mut self, key: u64, routine: u64, pos: Position) {
        let Some(entry) = self.panels.queued.queue().entry(routine) else { return };
        let actions =
            buttons(entry).into_iter().map(|button| MenuAction::Bot(key, BotItem::Routine(routine, button))).collect();
        self.overlay = Some(Overlay::Menu { at: pos, actions });
    }

    fn ask_rename_bot(&mut self, key: u64) {
        let Some(at) = self.panels.bots.by_key(key) else { return };
        let input = Editor::new(&self.panels.bots.live[at].bot.name);
        self.overlay = Some(Overlay::RenameBot { bot: key, input, error: None });
    }

    pub(in crate::app) fn rename_bot_view(input: &Editor, error: Option<&str>) -> ui::Overlay {
        let mut view = rename_form("Rename bot", RENAME_HINT, input);
        if let ui::Overlay::Form(form) = &mut view {
            form.note = error.map(|text| ui::Note::Error(text.into()));
            form.submit = RENAME_SUBMIT;
        }
        view
    }

    pub(in crate::app) fn bot_view(&self, overlay: &Overlay) -> Option<ui::Overlay> {
        match overlay {
            Overlay::BotForm(form) => Some(ui::Overlay::BotForm(form.clone())),
            Overlay::RenameBot { input, error, .. } => Some(Self::rename_bot_view(input, error.as_deref())),
            Overlay::RemoveBot { bot } => self.remove_bot_view(*bot),
            _ => None,
        }
    }

    pub(in crate::app) fn remove_bot_label(&self, key: u64) -> &'static str {
        let busy = self.panels.bots.by_key(key).is_some_and(|at| self.panels.bots.live[at].answering());
        if busy { STOP_FIRST } else { REMOVE_SUBMIT }
    }

    fn remove_bot_text(&self, at: usize) -> String {
        let live = &self.panels.bots.live[at];
        let names: Vec<&str> = self
            .bot_entries(&live.bot.id)
            .filter(|entry| !entry.is_run())
            .map(|entry| entry.task.label.as_str())
            .collect();
        let entries = if names.is_empty() {
            String::from("its chat, its memory and its folder")
        } else {
            format!("its chat, its memory, its folder and its queue entries ({})", names.join(", "))
        };
        let text = format!("Remove bot {}? {entries} are deleted. Agents it started keep running.", live.bot.name);
        text.replacen("? its", "? Its", 1)
    }

    pub(in crate::app) fn remove_bot_view(&self, key: u64) -> Option<ui::Overlay> {
        let at = self.panels.bots.by_key(key)?;
        let busy = self.remove_bot_label(key) == STOP_FIRST;
        let message = self.remove_bot_text(at);
        Some(ui::Overlay::Confirm(ui::Confirm {
            title: "Remove bot",
            message,
            note: None,
            submit: self.remove_bot_label(key),
            extra: None,
            dangerous: !busy,
        }))
    }

    pub(in crate::app) fn submit_rename_bot(&mut self, key: u64, input: Editor) -> Option<Overlay> {
        let text = input.text();
        let at = self.panels.bots.by_key(key)?;
        if text.trim().is_empty() {
            return None;
        }
        let edit = EditBot { bot: self.panels.bots.live[at].bot.id.clone(), name: Some(text), ..EditBot::default() };
        match self.bot_edit(&edit) {
            Ok(_) => None,
            Err(refusal) => Some(Overlay::RenameBot { bot: key, input, error: Some(refusal.message) }),
        }
    }

    pub(in crate::app) fn submit_remove_bot(&mut self, key: u64) -> Option<Overlay> {
        let at = self.panels.bots.by_key(key)?;
        let (id, name) = (self.panels.bots.live[at].bot.id.clone(), self.panels.bots.live[at].bot.name.clone());
        if self.panels.bots.live[at].answering() {
            let _ = self.bot_stop(&id);
            return Some(Overlay::RemoveBot { bot: key });
        }
        match self.bot_remove(&id) {
            Ok((_, entries)) if entries.iter().any(|entry| !entry.is_run()) => {
                let count = entries.iter().filter(|entry| !entry.is_run()).count();
                let message = format!(
                    "removed bot {name} and its {}",
                    crate::text::plural(count, "queue entry", "queue entries")
                );
                self.messages.toast = Some(Toast::new(message, ui::ToastIcon::Check));
            }
            Ok(_) => {}
            Err(refusal) => self.messages.toast = Some(Toast::new(refusal.message, ui::ToastIcon::Bug)),
        }
        None
    }
}

impl Overlay {
    pub(in crate::app) fn names_bot(&self, key: u64, id: &str) -> bool {
        match self {
            Self::RenameBot { bot, .. } | Self::RemoveBot { bot } => *bot == key,
            Self::BotForm(form) => form.editing.as_deref() == Some(id),
            Self::BotRoutines(dialog) => dialog.bot == key,
            Self::Queue(form) => form.bot() == Some(id),
            Self::Menu { actions, .. } => {
                actions.iter().any(|action| matches!(action, MenuAction::Bot(found, _) if *found == key))
            }
            _ => false,
        }
    }
}
