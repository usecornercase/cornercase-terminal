use ratatui::layout::Rect;
use ratatui::text::Line;

use super::App;
use super::event::AppEvent;
use super::overlay::Overlay;
use super::plugins::PluginEvent;
use crate::error::{Error, Result};
use crate::highlight::markdown;
use crate::log::{self, Job, Level};
use crate::plugins::catalogue::{self, Entry, Index, Mark, clean};
use crate::plugins::{Places, Registry};
use crate::settings::{self, AvailableRow, Row};
use crate::ui;

const TITLE: &str = "Install plugin";
pub(super) const INSTALL_SUBMIT: &str = "install";

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Step {
    Ask,
    Installing,
    Failed(String),
}

fn install(places: &Places, entry: &Entry) -> Result<String> {
    let _ = places;
    Err(Error::Plugin(format!(
        "this cornercase cannot install plugins yet; clone {} at {} and `cornercase plugin link` it",
        entry.github(),
        &entry.commit[..7]
    )))
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

fn preview(entry: &Entry) -> String {
    let at = if entry.reference.is_empty() {
        format!("commit {}", &entry.commit[..12])
    } else {
        format!("{} (commit {})", escape(&entry.reference), &entry.commit[..12])
    };
    let platforms = if entry.platforms.is_empty() { "every platform".into() } else { entry.platforms.join(", ") };
    let asks = if entry.permissions.is_empty() {
        "nothing".to_string()
    } else {
        entry.permissions.iter().map(|p| escape(p)).collect::<Vec<_>>().join(", ")
    };
    let mut lines = vec![
        format!("**From** github.com/{} at {at}", escape(&entry.source())),
        format!("**Version** {}", escape(&entry.version)),
        format!("**Runs on** {platforms}"),
        format!("**Asks for** {asks}, shown and not enforced"),
    ];
    if !entry.sha256.is_empty() {
        let targets: Vec<&str> = entry.sha256.keys().map(String::as_str).collect();
        lines.push(format!("**Downloads checked** by sha256 for {}", targets.join(", ")));
    }
    lines.push(
        "Reviewed for cornercase at this commit. It is added turned off: nothing in it runs until you turn it on \
         under Installed. A plugin runs as you and can type into every terminal."
            .into(),
    );
    lines.join("\n\n")
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
        self.update.scroll = 0;
        self.overlay = Some(Overlay::InstallPlugin(Box::new(Installing { entry: entry.clone(), step: Step::Ask })));
    }

    pub(super) fn install_notes(entry: &Entry, width: usize) -> Vec<Line<'static>> {
        markdown::render(&preview(entry), width)
    }

    pub(super) fn install_view(&self, installing: &Installing, area: Rect) -> ui::Overlay {
        let entry = &installing.entry;
        let note = match &installing.step {
            Step::Ask => None,
            Step::Installing => Some(ui::Note::Busy("installing…")),
            Step::Failed(e) => Some(ui::Note::Error(e.clone())),
        };
        ui::Overlay::Update(ui::Update {
            title: TITLE,
            message: format!("Install {} {}? {}", clean(&entry.name), clean(&entry.version), clean(&entry.description)),
            notes: self.update_notes(area),
            scroll: self.update.scroll,
            note,
            submit: INSTALL_SUBMIT,
            cancel: ui::CANCEL_LABEL,
        })
    }

    pub(super) fn submit_install(&mut self, installing: Installing) -> Option<Overlay> {
        if installing.step == Step::Installing {
            return Some(Overlay::InstallPlugin(Box::new(installing)));
        }
        let (places, entry) = (self.plugins.places.clone(), installing.entry.clone());
        let id = entry.id.clone();
        let job = Job::new(Level::Info, "plugins", "install").with("plugin", &id).with("commit", &entry.commit);
        self.spawn_job(
            job,
            move || install(&places, &entry),
            move |result, _| Some(AppEvent::Plugin(PluginEvent::Installed { id, result })),
        );
        Some(Overlay::InstallPlugin(Box::new(Installing { step: Step::Installing, ..installing })))
    }

    pub(super) fn plugin_installed(&mut self, id: &str, result: Result<String>) {
        let still_open = matches!(&self.overlay, Some(Overlay::InstallPlugin(i)) if i.entry.id == id);
        match result {
            Ok(name) => {
                log::info!("plugins", "installed", plugin = id);
                if still_open {
                    self.open_settings_on(&Row::Plugin(id.to_string()));
                    if let Some(Overlay::Settings(s)) = &mut self.overlay {
                        s.notice = Some(format!("{name} is installed and off; enter turns it on"));
                    }
                }
            }
            Err(e) => {
                if let Some(Overlay::InstallPlugin(installing)) = &mut self.overlay
                    && still_open
                {
                    installing.step = Step::Failed(e.to_string());
                }
            }
        }
    }

    pub(super) fn cancel_install(&mut self, id: &str) {
        self.open_settings_on(&Row::Available(id.to_string()));
    }
}
