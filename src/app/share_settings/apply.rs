use std::collections::BTreeMap;
use std::path::Path;

use serde_json::Value;

use crate::app::App;
use crate::app::control::{malformed, needs_a_repository};
use crate::config::{Config, ProjectSettings};
use crate::control::config::SetupPreview;
use crate::control::contract::Refusal;
use crate::log;
use crate::settings::Page;
use crate::settings::share::preview::{self, OpenRepository};
use crate::settings::share::{self, Repository, Section, Shared, settings_of};
use crate::text;

const ROWLESS: [&str; 7] =
    ["prompt", "gh", "glab", "agent_modes", "agent_commands", "trust_prompt_pattern", "agent_args"];
const NOTHING: &str = "nothing to import: the ticked sections already match the settings in use";

struct Imported {
    config: Config,
    settings: usize,
    repositories: usize,
    notices: Vec<String>,
}

impl App {
    pub(in crate::app) fn import_setup(
        &mut self,
        shared: &Shared,
        sections: &[Section],
    ) -> Result<Vec<String>, Refusal> {
        self.readable_config()?;
        let imported = self.imported(shared, sections)?;
        if imported.config == self.config {
            return Ok([vec![NOTHING.to_string()], imported.notices].concat());
        }
        self.write_config(&imported.config).map_err(|e| self.config_refusal(e))?;
        let ids: Vec<&str> = sections.iter().map(|section| section.id()).collect();
        let (settings, repositories) = (imported.settings, imported.repositories);
        log::info!(
            "config",
            "config imported",
            sections = ids.join(","),
            settings = settings,
            repositories = repositories
        );
        let (settings, repositories) = share::counts(settings, repositories);
        let first = format!("imported {settings} and {repositories}");
        Ok([vec![first], imported.notices].concat())
    }

    pub(in crate::app) fn preview_setup(&self, shared: &Shared, sections: &[Section]) -> Result<SetupPreview, Refusal> {
        self.imported(shared, sections)?;
        let mut preview = preview::preview(shared, &self.settings_model(Page::default()), &self.setup_here());
        preview.sections.retain(|section| sections.iter().any(|ticked| ticked.id() == section.id));
        Ok(preview)
    }

    fn imported(&self, shared: &Shared, sections: &[Section]) -> Result<Imported, Refusal> {
        let current = settings_of(&self.config);
        let changed = changed(shared, sections, &current);
        let mut next = self.config.clone();
        for (key, value) in changed.iter().filter(|(key, _)| ROWLESS.contains(key)) {
            next = preview::with_value(&next, key, value).map_err(malformed)?;
        }
        let mut model = self.settings_model(Page::default());
        model.take_config(next);
        let mut notices = Vec::new();
        for (key, value) in changed.iter().filter(|(key, _)| !ROWLESS.contains(key)) {
            notices.extend(model.apply(key, value).map_err(Refusal::from)?);
        }
        let repositories = if sections.contains(&Section::Repositories) {
            self.import_repositories(&mut model.config, &shared.repositories)?
        } else {
            0
        };
        let after = settings_of(&model.config);
        let settings = changed.iter().filter(|(key, _)| after.get(*key) != current.get(*key)).count();
        notices.extend(shared.unknown.iter().map(|key| preview::unknown(key)));
        Ok(Imported { config: model.config, settings, repositories, notices })
    }

    fn import_repositories(&self, config: &mut Config, repositories: &[Repository]) -> Result<usize, Refusal> {
        let (here, home) = (self.setup_here(), self.home.as_deref());
        let matched: Vec<(&Repository, &OpenRepository)> = repositories
            .iter()
            .flat_map(|repository| preview::matches(repository, &here).into_iter().map(move |open| (repository, open)))
            .collect();
        let mut claimed: Vec<(u64, &str)> = Vec::new();
        let mut set = 0;
        for (repository, open) in matched {
            claim(&mut claimed, open, repository, home)?;
            let before = config.project(&open.folder, home);
            let settings = ProjectSettings {
                setup: repository.setup.clone(),
                teardown: repository.teardown.clone(),
                builds: before.builds.clone(),
            };
            needs_a_repository(&open.folder, &settings)?;
            config.set_project(&open.folder, home, settings);
            set += usize::from(config.project(&open.folder, home) != before);
        }
        Ok(set)
    }
}

fn changed<'a>(
    shared: &'a Shared,
    sections: &[Section],
    current: &BTreeMap<String, Value>,
) -> Vec<(&'static str, &'a Value)> {
    sections
        .iter()
        .flat_map(|section| section.keys())
        .filter_map(|key| {
            shared.settings.get(*key).filter(|value| current.get(*key) != Some(*value)).map(|v| (*key, v))
        })
        .collect()
}

fn claim<'a>(
    claimed: &mut Vec<(u64, &'a str)>,
    open: &OpenRepository,
    repository: &'a Repository,
    home: Option<&Path>,
) -> Result<(), Refusal> {
    let remote = repository.remotes.first().map_or("", String::as_str);
    if let Some((_, first)) = claimed.iter().find(|(id, _)| *id == open.id) {
        let path = text::display_path(&open.folder, home);
        return Err(malformed(format!("{first} and {remote} both match {path}: keep one of them in the file")));
    }
    claimed.push((open.id, remote));
    Ok(())
}
