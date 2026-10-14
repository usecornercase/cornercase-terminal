use crossterm::event::{KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use super::App;
use super::input::wheel;
use super::overlay::Overlay;
use crate::config::{self, Config};
use crate::issues::Source;
use crate::issues::secrets;
use crate::log::{self, Level};
use crate::settings::{self, DONE, Page, Settings, Status};
use crate::ui::{self};

impl App {
    pub(super) fn set_config(&mut self, config: Config) {
        if log::enabled(Level::Info) {
            let keys = changed_keys(&self.config, &config);
            if !keys.is_empty() {
                log::info!("app", "settings changed", keys = keys.join(","));
            }
        }
        let jira = |c: &Config| (c.jira_site.clone(), c.jira_email.clone(), c.jira_jql.clone());
        if jira(&config) != jira(&self.config) {
            self.forget_issues(Source::Jira);
        }
        let plane = |c: &Config| (c.plane_url.clone(), c.plane_workspace.clone(), c.plane_filter.clone());
        if plane(&config) != plane(&self.config) {
            self.forget_issues(Source::Plane);
        }
        self.config = config;
    }

    pub(super) fn open_settings(&mut self) {
        let tokens = Source::REMOTE
            .into_iter()
            .map(|source| {
                let status = match self.token(source) {
                    None => Status::Missing,
                    Some((_, true)) => Status::Env,
                    Some((_, false)) => Status::Saved(self.issues.accounts.get(&source).cloned()),
                };
                (source, status)
            })
            .collect();
        let mut settings = Settings::new(self.config.clone(), self.home.clone(), tokens, self.settings_page);
        settings.awake = self.agents.awake.state().note();
        settings.remote = self.remote.note();
        settings.browsers = self.remote.web.paired();
        settings.telegram = self.telegram_settings();
        settings.plugins = self.plugin_rows();
        settings.catalogue = self.catalogue_view();
        self.overlay = Some(Overlay::Settings(Box::new(settings)));
    }

    pub(super) fn open_settings_on(&mut self, row: &settings::Row) {
        self.settings_page = row.page();
        self.open_settings();
        if let Some(Overlay::Settings(s)) = &mut self.overlay
            && let Some(i) = s.rows().iter().position(|r| r == row)
        {
            s.select(i);
        }
    }

    fn settings_pick_rows(area: Rect) -> usize {
        usize::from(ui::settings_pick_list(ui::settings_area(area)).height)
    }

    pub(super) fn settings_key(&mut self, key: KeyEvent, area: Rect) {
        let Some(Overlay::Settings(s)) = &mut self.overlay else { return };
        let action = s.key(key, Self::settings_pick_rows(area));
        self.settings_page = s.page;
        self.settings_act(action);
    }

    pub(super) fn settings_mouse(&mut self, ev: MouseEvent, pos: Position, area: Rect) {
        let rows = Self::settings_pick_rows(area);
        let Some(Overlay::Settings(s)) = &mut self.overlay else { return };
        if s.busy() {
            return;
        }
        if let (Some(pick), Some(delta)) = (&mut s.pick, wheel(ev.kind)) {
            pick.search.scroll_by(delta, pick.items.len(), rows);
            return;
        }
        match ev.kind {
            MouseEventKind::ScrollUp => s.select(s.cursor.saturating_sub(1)),
            MouseEventKind::ScrollDown => s.select(s.cursor + 1),
            _ => {}
        }
        let MouseEventKind::Down(MouseButton::Left) = ev.kind else { return };
        let rows_now = s.rows();
        let sections: Vec<&'static str> = rows_now.iter().map(settings::Row::section).collect();
        let removable: Vec<bool> = (0..rows_now.len()).map(|i| s.removal(i).is_some()).collect();
        let movable: Vec<bool> = rows_now.iter().map(settings::Row::movable).collect();
        let pick = s.pick.as_ref().map(|p| (s.pick_choices().len(), p.search.scroll()));
        let tabs = Page::ALL.map(Page::name);
        let layout = ui::SettingsLayout {
            tabs: &tabs,
            sections: &sections,
            removable: &removable,
            movable: &movable,
            cursor: s.cursor,
            pick,
        };
        let action = match ui::settings_hit(area, &layout, pos) {
            Some(ui::SettingsHit::Done) => settings::Action::Close,
            Some(ui::SettingsHit::Restart) => settings::Action::Restart,
            Some(ui::SettingsHit::Tab(i)) => {
                s.open_page(Page::ALL[i]);
                settings::Action::None
            }
            Some(ui::SettingsHit::Pick(i)) => s.choose(Some(i)),
            Some(ui::SettingsHit::Remove(i)) => s.removal(i).unwrap_or(settings::Action::None),
            Some(ui::SettingsHit::MoveUp(i)) => s.move_tab(i, settings::Move::Up),
            Some(ui::SettingsHit::MoveDown(i)) => s.move_tab(i, settings::Move::Down),
            Some(ui::SettingsHit::Row(i)) => {
                s.edit = None;
                s.select(i);
                s.activate()
            }
            None => settings::Action::None,
        };
        self.settings_page = s.page;
        self.settings_act(action);
    }

    fn settings_act(&mut self, action: settings::Action) {
        match action {
            settings::Action::None => {}
            settings::Action::Close => self.overlay = None,
            settings::Action::Restart => self.open_restart(),
            settings::Action::Save(config) => {
                if let Err(e) = config::save(&self.config_path, &config) {
                    if let Some(Overlay::Settings(s)) = &mut self.overlay {
                        s.notice = Some(format!("failed to save the settings: {e}"));
                    }
                    return;
                }
                self.set_config(*config);
            }
            settings::Action::CheckToken(source, token) => self.check_token(source, token),
            settings::Action::ForgetBrowsers => {
                let forgotten = self.forget_browsers();
                if let Some(Overlay::Settings(s)) = &mut self.overlay {
                    s.notice = Some(forgotten.map_or_else(|e| e, String::from));
                }
            }
            settings::Action::CheckTelegram(token) => self.check_telegram(token),
            settings::Action::RemoveTelegram => {
                let removed = self.remove_telegram();
                if let Some(Overlay::Settings(s)) = &mut self.overlay {
                    s.notice = Some(removed.map_or_else(|e| e, String::from));
                }
            }
            settings::Action::PairTelegram => {
                let refused = self.pair_telegram();
                if let (Some(why), Some(Overlay::Settings(s))) = (refused, &mut self.overlay) {
                    s.notice = Some(why.into());
                }
            }
            settings::Action::ForgetTelegram => {
                let forgotten = self.forget_telegram_chat();
                if let Some(Overlay::Settings(s)) = &mut self.overlay {
                    s.notice = Some(forgotten.into());
                }
            }
            settings::Action::Plugin(id, on) => self.switch_plugin(&id, on),
            settings::Action::OpenCatalogue => self.open_catalogue(),
            settings::Action::InstallPlugin(id) => self.open_install(&id),
            settings::Action::RemoveToken(source) => {
                let removed = source.secret_key().map(|key| secrets::remove(&self.issues.secrets_path, key));
                let Some(Overlay::Settings(s)) = &mut self.overlay else { return };
                if let Some(Err(e)) = removed {
                    s.notice = Some(format!("failed to remove the {}: {e}", source.token_name()));
                    return;
                }
                s.removed(source);
                self.forget_issues(source);
            }
        }
    }
}

pub(super) fn changed_keys(old: &Config, new: &Config) -> Vec<String> {
    let (Ok(serde_json::Value::Object(old)), Ok(serde_json::Value::Object(new))) =
        (serde_json::to_value(old), serde_json::to_value(new))
    else {
        return Vec::new();
    };
    new.into_iter().filter(|(key, value)| old.get(key) != Some(value)).map(|(key, _)| key).collect()
}

pub(super) fn settings_view(settings: &Settings) -> ui::Settings {
    let rows: Vec<ui::SettingsRow> = settings
        .rows()
        .iter()
        .enumerate()
        .map(|(i, row)| {
            let text = settings.row_text(row);
            ui::SettingsRow {
                section: row.section(),
                label: text.label,
                value: text.value,
                note: text.note,
                dangerous: text.dangerous,
                removable: settings.removal(i).is_some(),
                movable: row.movable(),
            }
        })
        .collect();
    let edit = settings.edit_field().map(|(label, value)| ui::SettingsEdit { label, value });
    let pick = settings.pick.as_ref().map(|pick| {
        let choices = settings.pick_choices();
        ui::SettingsPick {
            title: settings.pick_title(),
            filter: pick.search.query().to_string(),
            items: choices.iter().map(|item| (item.value.clone(), item.note.clone(), item.dangerous)).collect(),
            selected: settings.pick_selected(),
            scroll: pick.search.scroll(),
        }
    });
    let error = settings.edit.as_ref().and_then(|e| e.error.clone());
    let note = if settings.busy() { Some(ui::Note::Busy("checking…")) } else { error.map(ui::Note::Error) };
    let hint = settings.hint();
    ui::Settings {
        tabs: Page::ALL.iter().map(|p| p.name()).collect(),
        tab: settings.page.index(),
        rows,
        cursor: settings.cursor,
        edit,
        pick,
        note,
        hint,
        submit: DONE,
    }
}
