use crossterm::event::{KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use super::App;
use super::input::wheel;
use super::overlay::Overlay;
use super::toast::Toast;
use crate::settings::share::dialog::{Action, Dialog, Page, Room, copied};
use crate::settings::share::{self, Section, Shared};
use crate::ui;

impl App {
    pub(super) fn open_share_settings(&mut self) {
        self.overlay = Some(Overlay::ShareSettings(Box::new(Dialog::new(""))));
    }

    pub(super) fn share_pasted(&mut self, text: &str, area: Rect) {
        self.open_share_settings();
        self.share_settings_paste(text, area);
    }

    fn share_dialog(&mut self) -> Option<&mut Dialog> {
        match &mut self.overlay {
            Some(Overlay::ShareSettings(dialog)) => Some(dialog),
            _ => None,
        }
    }

    pub(super) fn share_settings_key(&mut self, key: KeyEvent, area: Rect) {
        let Some(dialog) = self.share_dialog() else { return };
        let action = dialog.key(key, room(dialog, area));
        self.share_act(action);
    }

    pub(super) fn share_settings_paste(&mut self, text: &str, area: Rect) {
        if let Some(dialog) = self.share_dialog() {
            let room = room(dialog, area);
            dialog.paste(text, room);
        }
    }

    pub(super) fn share_settings_mouse(&mut self, ev: MouseEvent, pos: Position, area: Rect) {
        let Some(dialog) = self.share_dialog() else { return };
        let room = room(dialog, area);
        if let Some(delta) = wheel(ev.kind) {
            dialog.scroll(delta, room);
            return;
        }
        if ev.kind != MouseEventKind::Down(MouseButton::Left) {
            return;
        }
        let view = ui::ShareSettings::of(dialog);
        let action = match ui::share_settings_hit(area, &view, pos) {
            Some(ui::ShareHit::Button(i)) => dialog.press(view.buttons[i]),
            Some(ui::ShareHit::Line(i)) => {
                if let Some(section) = dialog.lines().get(i).and_then(|line| line.section) {
                    dialog.toggle(section);
                }
                Action::None
            }
            Some(ui::ShareHit::Text { line, col }) => {
                dialog.place(line, col, room);
                Action::None
            }
            None => Action::None,
        };
        self.share_act(action);
    }

    fn share_act(&mut self, action: Action) {
        match action {
            Action::None => {}
            Action::Back => self.open_settings(),
            Action::Copy => self.copy_setup(),
            Action::Preview(text) => self.preview_shared(&text),
            Action::Import(shared, sections) => self.import_shared(&shared, &sections),
        }
    }

    fn copy_setup(&mut self) {
        let exported = match self.export_setup() {
            Ok(exported) => exported,
            Err(why) => {
                if let Some(dialog) = self.share_dialog() {
                    dialog.refused(why);
                }
                return;
            }
        };
        self.copy(&exported.text);
        let icon = if exported.warnings.is_empty() { ui::ToastIcon::Check } else { ui::ToastIcon::Bug };
        self.messages.toast = Some(Toast::new(copied(&exported), icon));
        if let Some(dialog) = self.share_dialog() {
            dialog.warnings = exported.warnings;
        }
    }

    fn preview_shared(&mut self, text: &str) {
        let previewed = share::parse(text).and_then(|shared| {
            let preview = self.preview_setup(&shared, &Section::ALL).map_err(|refusal| refusal.message)?;
            Ok((shared, preview))
        });
        if let Some(dialog) = self.share_dialog() {
            dialog.previewed(previewed);
        }
    }

    fn import_shared(&mut self, shared: &Shared, sections: &[Section]) {
        let notices = match self.import_setup(shared, sections) {
            Ok(notices) => notices,
            Err(refusal) => {
                if let Some(dialog) = self.share_dialog() {
                    dialog.refused(refusal.message);
                }
                return;
            }
        };
        self.open_settings();
        let (first, rest) = notices.split_first().map_or((String::new(), &[][..]), |(f, r)| (f.clone(), r));
        if let (Some(Overlay::Settings(settings)), false) = (&mut self.overlay, rest.is_empty()) {
            settings.notice = Some(rest.join(" · "));
        }
        self.messages.toast = Some(Toast::new(first, ui::ToastIcon::Check));
    }
}

fn room(dialog: &Dialog, area: Rect) -> Room {
    let r = match dialog.page {
        Page::Paste { .. } => ui::share_settings_text(area),
        Page::Preview { .. } => ui::share_settings_list(area),
    };
    Room { width: usize::from(r.width), height: usize::from(r.height) }
}
