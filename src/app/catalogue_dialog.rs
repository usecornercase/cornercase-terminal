use ratatui::layout::Rect;
use ratatui::text::Line;

use super::App;
use super::event::AppEvent;
use super::overlay::Overlay;
use super::plugins::PluginEvent;
use super::toast::Toast;
use crate::error::Result;
use crate::highlight::markdown;
use crate::log::{self, Job, Level};
use crate::plugins::catalogue::{self, Entry, Index, Mark, clean};
use crate::plugins::install::{self, Fetched};
use crate::plugins::registry::short;
use crate::plugins::{Places, Registry};
use crate::settings::{self, AvailableRow, Row};
use crate::ui;

const TITLE: &str = "Install plugin";
pub(super) const INSTALL_SUBMIT: &str = "install";
const RETRY_SUBMIT: &str = "try again";

#[derive(Debug, Default)]
pub(super) enum Fetch {
    #[default]
    Closed,
    Loading,
    Failed(String),
    Loaded(Index),
}

#[derive(Debug)]
pub(super) struct Installing {
    pub(super) entry: Entry,
    pub(super) step: Step,
}

#[derive(Debug)]
pub(super) enum Step {
    Fetching,
    Review(Box<Fetched>),
    Installing,
    Failed(String),
}

fn escape(text: &str) -> String {
    clean(text).chars().fold(String::new(), |mut out, c| {
        if "\\`*_[]<>#!|~".contains(c) {
            out.push('\\');
        }
        out.push(c);
        out
    })
}

fn listed(entry: &Entry) -> String {
    let at = if entry.reference.is_empty() {
        format!("commit {}", short(&entry.commit))
    } else {
        format!("{} (commit {})", escape(&entry.reference), short(&entry.commit))
    };
    format!(
        "Listed in the catalogue: {} {} from github.com/{} at {at}.",
        escape(&entry.name),
        escape(&entry.version),
        entry.repo
    )
}

fn notes(installing: &Installing) -> String {
    let mut lines = vec![listed(&installing.entry)];
    match &installing.step {
        Step::Review(fetched) => lines.extend(fetched.preview().iter().map(|line| escape(line))),
        Step::Fetching | Step::Installing | Step::Failed(_) => lines.push(
            "Fetching it shows every command it will run before anything is installed. Nothing in it runs until \
             you confirm."
                .into(),
        ),
    }
    lines.join("\n\n")
}

fn fetch(places: &Places, github: &str, entry: &Entry) -> std::result::Result<Fetched, String> {
    let fetched = install::fetch(&entry.origin(github), places)?;
    entry.matches(&fetched.commit, &fetched.manifest)?;
    Ok(fetched)
}

fn available_row(entry: &Entry, installed: &[crate::plugins::registry::Entry]) -> AvailableRow {
    let label = format!("{} {}", clean(&entry.name), clean(&entry.version));
    let about = format!("{} · {}", clean(&entry.description), clean(&entry.asks()));
    let (value, note, installable) = match entry.mark(installed) {
        Mark::Available => ("install".to_string(), about, true),
        Mark::Installed(how) => ("installed".to_string(), how, false),
        Mark::Unfit(why) => ("cannot install".to_string(), why, false),
    };
    AvailableRow { id: entry.id.clone(), label, value, note, installable }
}

impl App {
    pub(super) fn catalogue_view(&self) -> settings::Catalogue {
        match &self.plugins.catalogue {
            Fetch::Closed => settings::Catalogue::Closed,
            Fetch::Loading => settings::Catalogue::Loading,
            Fetch::Failed(e) => settings::Catalogue::Failed(e.clone()),
            Fetch::Loaded(index) => {
                let installed = Registry::load(&self.plugins.places.registry).entries;
                settings::Catalogue::Loaded(index.plugins.iter().map(|e| available_row(e, &installed)).collect())
            }
        }
    }

    fn show_catalogue(&mut self) {
        let view = self.catalogue_view();
        if let Some(Overlay::Settings(s)) = &mut self.overlay {
            s.catalogue = view;
        }
    }

    pub(super) fn open_catalogue(&mut self) {
        if matches!(self.plugins.catalogue, Fetch::Loading) {
            return;
        }
        self.plugins.catalogue = Fetch::Loading;
        self.show_catalogue();
        let url = self.plugins.catalogue_url.clone();
        let job = Job::new(Level::Info, "plugins", "catalogue");
        self.spawn_job(
            job,
            move || catalogue::fetch(&url),
            |fetched, job| {
                if let Ok(index) = &fetched {
                    job.note("plugins", index.plugins.len());
                }
                Some(AppEvent::Plugin(PluginEvent::Catalogue(fetched)))
            },
        );
    }

    pub(super) fn catalogue_fetched(&mut self, fetched: Result<Index>) {
        self.plugins.catalogue = match fetched {
            Ok(index) => Fetch::Loaded(index),
            Err(e) => Fetch::Failed(e.to_string()),
        };
        self.show_catalogue();
    }

    pub(super) fn open_install(&mut self, id: &str) {
        let Fetch::Loaded(index) = &self.plugins.catalogue else { return };
        let Some(entry) = index.plugins.iter().find(|entry| entry.id == id) else { return };
        let installed = Registry::load(&self.plugins.places.registry).entries;
        if entry.mark(&installed) != Mark::Available {
            return;
        }
        let entry = entry.clone();
        self.update.scroll = 0;
        self.start_fetch(entry);
    }

    fn start_fetch(&mut self, entry: Entry) {
        let (places, fetching) = (self.plugins.places.clone(), entry.clone());
        let github = self.plugins.github.clone();
        let id = entry.id.clone();
        let job = Job::new(Level::Info, "plugins", "fetch").with("plugin", &id).with("commit", short(&entry.commit));
        self.spawn_job(
            job,
            move || Ok(fetch(&places, &github, &fetching)),
            move |result, _| {
                let result = result.unwrap_or_else(|e| Err(e.to_string())).map(Box::new);
                Some(AppEvent::Plugin(PluginEvent::Fetched { id, result }))
            },
        );
        self.overlay = Some(Overlay::InstallPlugin(Box::new(Installing { entry, step: Step::Fetching })));
    }

    pub(super) fn plugin_fetched(&mut self, id: &str, result: std::result::Result<Box<Fetched>, String>) {
        let Some(Overlay::InstallPlugin(installing)) = &mut self.overlay else { return };
        if installing.entry.id != id || !matches!(installing.step, Step::Fetching) {
            return;
        }
        installing.step = match result {
            Ok(fetched) => Step::Review(fetched),
            Err(e) => Step::Failed(clean(&e)),
        };
        self.update.scroll = 0;
    }

    pub(super) fn install_notes(installing: &Installing, width: usize) -> Vec<Line<'static>> {
        markdown::render(&notes(installing), width)
    }

    pub(super) fn install_view(&self, installing: &Installing, area: Rect) -> ui::Overlay {
        let entry = &installing.entry;
        let note = match &installing.step {
            Step::Fetching => Some(ui::Note::Busy("fetching…")),
            Step::Review(_) => None,
            Step::Installing => Some(ui::Note::Busy("installing…")),
            Step::Failed(e) => Some(ui::Note::Error(e.clone())),
        };
        let submit = if matches!(installing.step, Step::Failed(_)) { RETRY_SUBMIT } else { INSTALL_SUBMIT };
        ui::Overlay::Update(ui::Update {
            title: TITLE,
            message: format!("Install {} {}? {}", clean(&entry.name), clean(&entry.version), clean(&entry.description)),
            notes: self.update_notes(area),
            scroll: self.update.scroll,
            note,
            submit,
            cancel: ui::CANCEL_LABEL,
        })
    }

    pub(super) fn submit_install(&mut self, installing: Installing) -> Option<Overlay> {
        let Installing { entry, step } = installing;
        let fetched = match step {
            Step::Review(fetched) => fetched,
            Step::Failed(_) => {
                self.start_fetch(entry);
                return self.overlay.take();
            }
            step @ (Step::Fetching | Step::Installing) => {
                return Some(Overlay::InstallPlugin(Box::new(Installing { entry, step })));
            }
        };
        let places = self.plugins.places.clone();
        let id = entry.id.clone();
        let job = Job::new(Level::Info, "plugins", "install").with("plugin", &id).with("commit", short(&entry.commit));
        self.spawn_job(
            job,
            move || Ok(install::install(*fetched, &places)),
            move |result, _| {
                let result = result.unwrap_or_else(|e| Err(e.to_string())).map(|installed| installed.manifest.name);
                Some(AppEvent::Plugin(PluginEvent::Installed { id, result }))
            },
        );
        Some(Overlay::InstallPlugin(Box::new(Installing { entry, step: Step::Installing })))
    }

    pub(super) fn plugin_installed(&mut self, id: &str, result: std::result::Result<String, String>) {
        let still_open = matches!(&self.overlay, Some(Overlay::InstallPlugin(i)) if i.entry.id == id);
        match result {
            Ok(name) => {
                log::info!("plugins", "installed", plugin = id);
                self.toast =
                    Some(Toast::new(format!("{} is installed, turned off", clean(&name)), ui::ToastIcon::Check));
                if still_open {
                    self.open_settings_on(&Row::Plugin(id.to_string()));
                    if let Some(Overlay::Settings(s)) = &mut self.overlay {
                        s.notice = Some(format!("{} is installed and off; enter turns it on", clean(&name)));
                    }
                }
            }
            Err(e) => {
                if let Some(Overlay::InstallPlugin(installing)) = &mut self.overlay
                    && still_open
                {
                    installing.step = Step::Failed(clean(&e));
                }
            }
        }
    }

    pub(super) fn cancel_install(&mut self, id: &str) {
        self.open_settings_on(&Row::Available(id.to_string()));
    }
}
