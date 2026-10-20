use std::time::{Instant, SystemTime};

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};
use ratatui::text::Line;

use super::App;
use super::control::conflict;
use super::event::AppEvent;
use super::input::wheel;
use super::overlay::Overlay;
use super::overlay::RESTART_SUBMIT;
use super::toast::Toast;
use crate::control::contract::Refusal;
use crate::control::{InstallInfo, NoUpdate, Notes, ReleaseInfo, UpdateInfo};
use crate::error::Result;
use crate::highlight::markdown;
use crate::log::{self, Job, Level};
use crate::ui::{self};
use crate::update::restart;
use crate::update::{self, Install, Release, Updates};
use crate::version;

pub(super) struct UpdateState {
    pub(super) updates: Updates,
    pub(super) scroll: usize,
    pub(super) restart_list: Vec<restart::Running>,
    pub(super) listed: Option<Instant>,
    pub(super) checks: Vec<u64>,
    pub(super) installs: Vec<u64>,
}

impl UpdateState {
    pub(super) fn from_env() -> Self {
        Self {
            updates: Updates::from_env(),
            scroll: 0,
            restart_list: Vec::new(),
            listed: None,
            checks: Vec::new(),
            installs: Vec::new(),
        }
    }
}

pub(super) const UPDATE_AVAILABLE: &str = "a new cornercase is out";

const UPDATE_TITLE: &str = "Update";

const RESTART_TITLE: &str = "Restart";

const RESTART_MESSAGE: &str = "Restart cornercase now? Its server starts again and every client comes back.";

pub(super) const RESTART_LABEL: &str = "↻ restart";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum UpdateStep {
    Ask,
    Updating,
    Failed(String),
    Installed,
    Manual(&'static str),
}

#[derive(Debug)]
pub(crate) enum UpdateEvent {
    Checked(Result<Option<Release>>),
    Installed(Result<()>),
}

impl UpdateEvent {
    pub(crate) fn name(&self) -> &'static str {
        match self {
            Self::Checked(_) => "update checked",
            Self::Installed(_) => "updated",
        }
    }
}

impl App {
    pub(super) fn update_answered(&mut self, event: UpdateEvent) {
        match event {
            UpdateEvent::Checked(result) => self.update_checked(result),
            UpdateEvent::Installed(result) => self.updated(result),
        }
    }

    pub(super) fn check_updates(&mut self, now: Instant) {
        if !self.config.check_updates || !self.update.updates.due(now) {
            return;
        }
        self.start_update_check(now);
    }

    pub(in crate::app) fn start_update_check(&mut self, now: Instant) {
        if self.update.updates.checking {
            return;
        }
        self.update.updates.checked = Some(now);
        self.update.updates.checking = true;
        let url = self.update.updates.url.clone();
        let check = move || {
            let found = update::check(&url, version::CURRENT)?;
            Ok(found.map(|release| update::with_changelog(release, version::CURRENT)))
        };
        let job = Job::new(Level::Info, "update", "check");
        self.spawn_job(job, check, |found, _| Some(AppEvent::Update(UpdateEvent::Checked(found))));
    }

    pub(super) fn update_checked(&mut self, result: Result<Option<Release>>) {
        let updates = &mut self.update.updates;
        updates.checking = false;
        updates.answered = Some(SystemTime::now());
        let outcome = match &result {
            Ok(_) => {
                updates.error = None;
                Ok(())
            }
            Err(e) => {
                updates.error = Some(e.to_string());
                Err(e.to_string())
            }
        };
        if let Ok(Some(release)) = result
            && !self.update.updates.installed
            && !self.update.updates.installing
        {
            log::info!("update", "release found", version = release.version);
            if self.update.updates.available.as_ref().is_none_or(|known| known.version != release.version) {
                self.messages.toast = Some(Toast::new(UPDATE_AVAILABLE, ui::ToastIcon::Check));
            }
            self.update.updates.available = Some(release);
        }
        self.answer_update_checks(&outcome);
    }

    pub(super) fn running(&self) -> Vec<restart::Running> {
        restart::from_report(&self.report(None))
    }

    pub(super) fn list_running(&mut self, now: Instant) {
        self.update.restart_list = self.running();
        self.update.listed = Some(now);
    }

    pub(super) fn update_label(&self) -> Option<String> {
        if self.update.updates.installed {
            return Some(RESTART_LABEL.into());
        }
        self.update.updates.available.as_ref().map(|release| format!("↑ {}", release.version))
    }

    pub(super) fn open_update(&mut self) {
        let step = match &self.update.updates.install {
            _ if self.update.updates.installed => UpdateStep::Installed,
            _ if self.update.updates.installing => UpdateStep::Updating,
            Install::Replace(_) => UpdateStep::Ask,
            Install::Command(command) => UpdateStep::Manual(command),
        };
        self.update.scroll = 0;
        if step == UpdateStep::Installed {
            self.list_running(Instant::now());
        }
        self.overlay = Some(Overlay::Update(step));
    }

    pub(super) fn update_mouse(&mut self, ev: MouseEvent, pos: Position, area: Rect) -> Result<()> {
        if let Some(delta) = wheel(ev.kind) {
            self.scroll_update(delta, area);
            return Ok(());
        }
        let Some(overlay) = &self.overlay else { return Ok(()) };
        if ev.kind != MouseEventKind::Down(MouseButton::Left) {
            return Ok(());
        }
        let [submit, cancel] = ui::update_buttons(area, overlay.submit_label(), overlay.cancel_label());
        if submit.contains(pos) {
            self.submit_form(area)?;
        } else if cancel.contains(pos) {
            self.cancel_form();
        }
        Ok(())
    }

    pub(super) fn scroll_update(&mut self, delta: isize, area: Rect) {
        if matches!(self.overlay, Some(Overlay::Update(_) | Overlay::Restart)) {
            let lines = self.update_notes(area).len();
            self.update.scroll = ui::update_scroll(area, lines, self.update.scroll.saturating_add_signed(delta));
        }
    }

    pub(super) fn update_notes(&self, area: Rect) -> Vec<Line<'static>> {
        let width = usize::from(ui::update_notes(area).width);
        if self.overlay.as_ref().is_some_and(Overlay::lists_running) {
            return markdown::render(&restart::confirmation(Some(&self.update.restart_list)), width);
        }
        let Some(release) = self.update.updates.available.as_ref().filter(|r| !r.notes.is_empty()) else {
            return Vec::new();
        };
        let notes: Vec<String> =
            release.notes.iter().map(|(version, notes)| format!("**What's new in {version}**\n\n{notes}")).collect();
        markdown::render(&notes.join("\n\n"), width)
    }

    pub(super) fn submit_restart(&mut self) -> Option<Overlay> {
        self.outbox.restart = true;
        None
    }

    pub(super) fn submit_update(&mut self, step: UpdateStep) -> Option<Overlay> {
        match step {
            UpdateStep::Ask | UpdateStep::Failed(_) => {
                self.install_update().ok()?;
                Some(Overlay::Update(UpdateStep::Updating))
            }
            UpdateStep::Installed => {
                self.outbox.restart = true;
                None
            }
            UpdateStep::Manual(command) => {
                self.copy(command);
                None
            }
            UpdateStep::Updating => Some(Overlay::Update(step)),
        }
    }

    pub(in crate::app) fn install_update(&mut self) -> std::result::Result<(), Refusal> {
        let updates = &self.update.updates;
        if updates.installing {
            return Ok(());
        }
        let Some(release) = updates.available.clone() else {
            return Err(conflict("there is no release found to install; ask for `update-check` first"));
        };
        let exe = match updates.install.on(update::target()) {
            Install::Replace(exe) => exe,
            Install::Command(command) => {
                return Err(conflict(format!("this cornercase cannot replace itself here; update it with: {command}")));
            }
        };
        let Some(target) = update::target() else { return Err(conflict("this platform has no release to install")) };
        self.update.updates.installing = true;
        self.update.updates.install_error = None;
        let job = Job::new(Level::Info, "update", "install").with("version", &release.version);
        self.spawn_job(
            job,
            move || update::update(&release, target, &exe),
            |result, _| Some(AppEvent::Update(UpdateEvent::Installed(result))),
        );
        Ok(())
    }

    pub(super) fn updated(&mut self, result: Result<()>) {
        self.update.updates.installing = false;
        let outcome = match &result {
            Ok(()) => Ok(()),
            Err(e) => Err(e.to_string()),
        };
        let step = match result {
            Ok(()) => {
                self.update.updates.installed = true;
                self.update.updates.install_error = None;
                self.update.scroll = 0;
                self.list_running(Instant::now());
                UpdateStep::Installed
            }
            Err(e) => {
                self.update.updates.install_error = Some(e.to_string());
                UpdateStep::Failed(e.to_string())
            }
        };
        if let Some(Overlay::Update(current)) = &mut self.overlay {
            *current = step;
        }
        self.answer_update_installs(&outcome);
    }

    pub(in crate::app) fn update_info(&self) -> UpdateInfo {
        let updates = &self.update.updates;
        let release = updates.available.as_ref().map(|release| ReleaseInfo {
            version: release.version.clone(),
            notes: release
                .notes
                .iter()
                .map(|(version, text)| Notes { version: version.clone(), text: text.clone() })
                .collect(),
        });
        let none = release.is_none().then(|| self.no_update());
        let install = match updates.install.on(update::target()) {
            Install::Replace(path) => InstallInfo::Replace { path },
            Install::Command(command) => InstallInfo::Command { command: command.into() },
        };
        UpdateInfo {
            release,
            none,
            checking: updates.checking,
            checked: updates.answered.map(|at| crate::time::Stamp(at).to_string()),
            error: updates.error.clone(),
            install,
            installing: updates.installing,
            installed: updates.installed,
            install_error: updates.install_error.clone(),
        }
    }

    fn no_update(&self) -> NoUpdate {
        let updates = &self.update.updates;
        if !updates.enabled {
            NoUpdate::Disabled
        } else if updates.answered.is_some() {
            if updates.error.is_some() { NoUpdate::Failed } else { NoUpdate::UpToDate }
        } else if !self.config.check_updates {
            NoUpdate::Off
        } else {
            NoUpdate::Unchecked
        }
    }

    pub(super) fn update_view(&self, step: &UpdateStep, area: Rect) -> ui::Overlay {
        let version = self.update.updates.available.as_ref().map_or("", |r| r.version.as_str());
        let current = version::CURRENT;
        let message = match step {
            UpdateStep::Installed => format!("cornercase {version} is installed. Restart to use it."),
            UpdateStep::Manual(command) => {
                format!("cornercase {version} is out (you have {current}). Update it with:\n{command}")
            }
            UpdateStep::Ask | UpdateStep::Updating | UpdateStep::Failed(_) => {
                let exe = match &self.update.updates.install {
                    Install::Replace(exe) => crate::text::display_path(exe, self.paths.home.as_deref()),
                    Install::Command(_) => String::new(),
                };
                format!(
                    "cornercase {version} is out (you have {current}). Updating replaces {exe}; \
                     your terminals keep running until you restart."
                )
            }
        };
        let note = match step {
            UpdateStep::Updating => Some(ui::Note::Busy("downloading…")),
            UpdateStep::Failed(error) => Some(ui::Note::Error(error.clone())),
            _ => None,
        };
        let overlay = Overlay::Update(step.clone());
        let (submit, cancel) = (overlay.submit_label(), overlay.cancel_label());
        let notes = self.update_notes(area);
        ui::Overlay::Update(ui::Update {
            title: UPDATE_TITLE,
            message,
            notes,
            scroll: self.update.scroll,
            note,
            submit,
            extra: None,
            cancel,
        })
    }

    pub(super) fn open_restart(&mut self) {
        self.update.scroll = 0;
        self.list_running(Instant::now());
        self.overlay = Some(Overlay::Restart);
    }

    pub(super) fn restart_view(&self, area: Rect) -> ui::Overlay {
        ui::Overlay::Update(ui::Update {
            title: RESTART_TITLE,
            message: RESTART_MESSAGE.into(),
            notes: self.update_notes(area),
            scroll: self.update.scroll,
            note: None,
            submit: RESTART_SUBMIT,
            extra: None,
            cancel: ui::CANCEL_LABEL,
        })
    }
}
