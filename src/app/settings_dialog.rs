use std::time::Instant;

use crossterm::event::{KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use super::App;
use super::input::wheel;
use super::overlay::Overlay;
use crate::config::{self, Config};
use crate::issues::Source;
use crate::issues::secrets;
use crate::log::{self, Level};
use crate::messenger::Via;
use crate::settings::{self, DONE, Page, Settings, Status};
use crate::ui::{self};

impl App {
    pub(super) fn set_config(&mut self, config: Config) {
        if config == self.config {
            return;
        }
        if log::enabled(Level::Info) {
            let changes = config::Changes::between(&self.config, &config);
            if !changes.is_empty() {
                log::info!("app", "settings changed", keys = changes.keys().join(","));
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
        let asana = |c: &Config| (c.asana_workspace.clone(), c.asana_projects.clone());
        if asana(&config) != asana(&self.config) {
            self.forget_issues(Source::Asana);
        }
        if config.gitlab_url != self.config.gitlab_url {
            self.forget_issues(Source::Gitlab);
        }
        self.config = config;
        if let Some(Overlay::Settings(s)) = &mut self.overlay {
            s.take_config(self.config.clone());
        }
        self.settings_changed();
    }

    pub(super) fn open_settings(&mut self) {
        if self.settings_page == Page::Flows {
            self.open_flows_tab();
            return;
        }
        let settings = self.settings_model(self.settings_page);
        self.overlay = Some(Overlay::Settings(Box::new(settings)));
    }

    fn token_statuses(&self) -> Vec<(Source, Status)> {
        Source::REMOTE
            .into_iter()
            .map(|source| {
                let status = match self.token(source) {
                    None => Status::Missing,
                    Some((_, true)) => Status::Env,
                    Some((_, false)) => Status::Saved(self.issues.accounts.get(&source).cloned()),
                };
                (source, status)
            })
            .collect()
    }

    pub(super) fn settings_model(&self, page: Page) -> Settings {
        let mut settings = Settings::new(self.config.clone(), self.home.clone(), self.token_statuses(), page);
        settings.awake = self.agents.awake.state().note();
        settings.pulls = self.pull_requests_note();
        settings.merged = self.merged_note();
        settings.remote = self.remote.note();
        settings.browsers = self.remote.paired();
        for via in Via::ALL {
            *settings.messenger_mut(via) = self.messenger_settings(via);
        }
        settings.plugins = self.plugin_rows();
        settings.accounts = self.accounts_listed();
        settings
    }

    pub(super) fn refresh_open_settings(&mut self) {
        let tokens = self.token_statuses();
        let browsers = self.remote.paired();
        let messengers: Vec<_> = Via::ALL.iter().map(|via| self.messenger_settings(*via)).collect();
        let Some(Overlay::Settings(s)) = &mut self.overlay else { return };
        s.tokens = tokens;
        s.browsers = browsers;
        for (via, view) in Via::ALL.into_iter().zip(messengers) {
            *s.messenger_mut(via) = view;
        }
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
        self.settings_handled(action, area);
    }

    fn settings_handled(&mut self, action: settings::Action, area: Rect) {
        if self.settings_page == Page::Flows {
            self.open_flows_tab();
            return;
        }
        self.settings_act(action, area);
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
        let action = settings_click(s, area, pos);
        self.settings_page = s.page;
        self.settings_handled(action, area);
    }

    pub(super) fn remove_token(&mut self, source: Source) -> Result<String, String> {
        if let Some(key) = source.secret_key() {
            secrets::remove(&self.issues.secrets_path, key)
                .map_err(|e| format!("failed to remove the {}: {e}", source.token_name()))?;
        }
        self.forget_issues(source);
        self.settings_changed();
        if let Some(Overlay::Settings(s)) = &mut self.overlay {
            s.removed(source);
        }
        Ok(format!("the {} {} was removed", source.name(), source.token_name()))
    }

    fn settings_message(&mut self, notice: String) {
        if let Some(Overlay::Settings(s)) = &mut self.overlay {
            s.notice = Some(notice);
        }
    }

    fn settings_act(&mut self, action: settings::Action, area: Rect) {
        match action {
            settings::Action::None => {}
            settings::Action::Close => self.overlay = None,
            settings::Action::Restart => self.open_restart(),
            settings::Action::Save(config) => {
                if let Err(e) = self.write_config(&config) {
                    self.settings_message(e);
                }
            }
            settings::Action::CheckToken(source, token) => self.check_token(source, token),
            settings::Action::ForgetBrowsers => {
                let forgotten = self.forget_browsers();
                self.settings_message(forgotten.map_or_else(|e| e, String::from));
            }
            settings::Action::CheckBot(via, token) => self.check_messenger(via, token),
            settings::Action::RemoveBot(via) => {
                let removed = self.remove_messenger(via);
                self.settings_message(removed.unwrap_or_else(|e| e));
            }
            settings::Action::Pair(via) => {
                if let Some(why) = self.pair(via) {
                    self.settings_message(why.into());
                }
            }
            settings::Action::ForgetChat(via) => {
                let forgotten = self.forget_chat(via);
                self.settings_message(forgotten);
            }
            settings::Action::Plugin(id, on) => self.switch_plugin(&id, on),
            settings::Action::Worktrees => self.open_worktrees(None),
            settings::Action::RemoveToken(source) => {
                if let Err(e) = self.remove_token(source) {
                    self.settings_message(e);
                }
            }
            settings::Action::Accounts(order) => self.account_order(&order, area),
            settings::Action::RemoveAccount(kind, name) => self.overlay = Some(Overlay::RemoveAccount { kind, name }),
        }
        self.watch_accounts_page(Instant::now());
    }
}

fn settings_click(s: &mut Settings, area: Rect, pos: Position) -> settings::Action {
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
    match ui::settings_hit(area, &layout, pos) {
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
    }
}

fn row_view(settings: &Settings, i: usize, row: &settings::Row) -> ui::SettingsRow {
    let text = settings.row_text(row);
    ui::SettingsRow {
        section: row.section(),
        label: text.label,
        value: text.value,
        note: text.note,
        dangerous: text.dangerous,
        muted: false,
        removable: settings.removal(i).is_some(),
        movable: row.movable(),
    }
}

fn pick_view(settings: &Settings, pick: &settings::Pick) -> ui::SettingsPick {
    let choices = settings.pick_choices();
    ui::SettingsPick {
        title: settings.pick_title(),
        filter: pick.search.query().to_string(),
        items: choices.iter().map(|item| (item.value.clone(), item.note.clone(), item.dangerous)).collect(),
        selected: settings.pick_selected(),
        scroll: pick.search.scroll(),
    }
}

pub(super) fn settings_view(settings: &Settings) -> ui::Settings {
    let rows: Vec<ui::SettingsRow> =
        settings.rows().iter().enumerate().map(|(i, row)| row_view(settings, i, row)).collect();
    let edit = settings.edit_field().map(|(label, value)| ui::SettingsEdit { label, value });
    let pick = settings.pick.as_ref().map(|pick| pick_view(settings, pick));
    let error = settings.edit.as_ref().and_then(|e| e.error.clone());
    let note = if settings.busy() { Some(ui::Note::Busy("checking…")) } else { error.map(ui::Note::Error) };
    ui::Settings {
        tabs: Page::ALL.iter().map(|p| p.name()).collect(),
        tab: settings.page.index(),
        rows,
        cursor: settings.cursor,
        edit,
        pick,
        note,
        hint: settings.hint(),
        submit: DONE,
    }
}
