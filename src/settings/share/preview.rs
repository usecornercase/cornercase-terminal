use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use serde_json::Value;

use super::{Repository, Section, Shared, secrets, settings_of};
use crate::agents;
use crate::config::{self, Config, ProjectSettings};
use crate::control::config::{NoteKind, SectionPreview, SetupChange, SetupNote, SetupPreview};
use crate::settings::Settings;
use crate::text;

const NOTHING: &str = "nothing";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct OpenRepository {
    pub(crate) id: u64,
    pub(crate) folder: PathBuf,
    pub(crate) remotes: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Here {
    pub(crate) home: Option<PathBuf>,
    pub(crate) projects: Vec<OpenRepository>,
    pub(crate) connected: Vec<String>,
}

pub(crate) fn preview(shared: &Shared, settings: &Settings, here: &Here) -> SetupPreview {
    let next = merged(&settings.config, &shared.settings);
    let after = Settings::new(next.clone(), here.home.clone(), settings.tokens.clone(), settings.page);
    let compared = Compared {
        shared,
        here,
        now: &settings.config,
        current: settings_of(&settings.config),
        before: Rows::of(settings),
        after: Rows::of(&after),
        next,
    };
    SetupPreview {
        sections: Section::ALL.into_iter().map(|section| compared.section(section)).collect(),
        notes: compared.notes(),
    }
}

pub(crate) fn matches<'a>(repository: &Repository, here: &'a Here) -> Vec<&'a OpenRepository> {
    here.projects.iter().filter(|project| shares_a_remote(repository, project)).collect()
}

pub(crate) fn with_value(config: &Config, key: &str, value: &Value) -> Result<Config, String> {
    let unread = |e: &dyn std::fmt::Display| format!("a value of `{key}` cannot be read: {e}");
    let Value::Object(mut keys) = serde_json::to_value(config).map_err(|e| unread(&e))? else {
        return Err(unread(&"the settings are not an object"));
    };
    keys.insert(key.to_string(), value.clone());
    serde_json::from_value(Value::Object(keys)).map_err(|e| unread(&e))
}

pub(crate) fn unknown(key: &str) -> String {
    format!("this cornercase does not know {key}; update it to import that setting")
}

fn merged(config: &Config, values: &BTreeMap<String, Value>) -> Config {
    values.iter().fold(config.clone(), |config, (key, value)| with_value(&config, key, value).unwrap_or(config))
}

fn shares_a_remote(repository: &Repository, project: &OpenRepository) -> bool {
    project.remotes.iter().any(|remote| repository.remotes.iter().any(|wanted| same_remote(remote, wanted)))
}

fn same_remote(a: &str, b: &str) -> bool {
    match (a.split_once('/'), b.split_once('/')) {
        (Some((host_a, path_a)), Some((host_b, path_b))) => host_a.eq_ignore_ascii_case(host_b) && path_a == path_b,
        _ => a == b,
    }
}

fn note(kind: NoteKind, text: String) -> SetupNote {
    SetupNote { kind, text }
}

fn risk(text: String) -> SetupNote {
    note(NoteKind::Risk, text)
}

fn missing(text: String) -> SetupNote {
    note(NoteKind::Missing, text)
}

fn compact(value: Option<&Value>) -> String {
    value.map_or_else(String::new, Value::to_string)
}

fn commands(settings: &ProjectSettings) -> String {
    let parts: Vec<String> = [("setup", &settings.setup), ("teardown", &settings.teardown)]
        .into_iter()
        .filter(|(_, commands)| !commands.is_empty())
        .map(|(name, commands)| format!("{name}: {commands}"))
        .collect();
    if parts.is_empty() { NOTHING.into() } else { parts.join("; ") }
}

fn mode_in(config: &Config, kind: &str) -> Option<String> {
    agents::mode_of(kind, &agents::args(config, kind), &agents::modes(config, kind))
}

struct Rows(BTreeMap<String, (String, String)>);

impl Rows {
    fn of(settings: &Settings) -> Self {
        Self(settings.describe().into_iter().map(|setting| (setting.key, (setting.label, setting.shown))).collect())
    }

    fn label(&self, key: &str) -> String {
        self.0.get(key).map_or_else(|| key.to_string(), |(label, _)| label.clone())
    }

    fn shown(&self, key: &str, value: Option<&Value>) -> String {
        match self.0.get(key) {
            Some((_, shown)) if !shown.is_empty() => shown.clone(),
            _ => compact(value),
        }
    }
}

struct Compared<'a> {
    shared: &'a Shared,
    here: &'a Here,
    now: &'a Config,
    next: Config,
    current: BTreeMap<String, Value>,
    before: Rows,
    after: Rows,
}

impl Compared<'_> {
    fn section(&self, section: Section) -> SectionPreview {
        let (changes, mut notes) = match section {
            Section::Repositories => self.repositories(),
            _ => (section.keys().iter().filter_map(|key| self.change(key)).collect(), self.notes_of(section)),
        };
        let mut seen = BTreeSet::new();
        notes.retain(|note| seen.insert(note.text.clone()));
        SectionPreview { id: section.id().into(), changes, notes }
    }

    fn changed(&self, key: &str) -> Option<&Value> {
        self.shared.settings.get(key).filter(|value| self.current.get(key) != Some(*value))
    }

    fn change(&self, key: &str) -> Option<SetupChange> {
        let value = self.changed(key)?;
        let current = self.current.get(key);
        let (mut from, mut to) = (self.before.shown(key, current), self.after.shown(key, Some(value)));
        if from == to {
            (from, to) = (compact(current), compact(Some(value)));
        }
        Some(SetupChange { key: key.into(), label: self.before.label(key), from, to })
    }

    fn notes_of(&self, section: Section) -> Vec<SetupNote> {
        match section {
            Section::Agents => self.agent_notes(),
            Section::Worktrees => self.worktree_notes(),
            Section::Interface => self.sound_notes(),
            Section::Issues | Section::Repositories => Vec::new(),
        }
    }

    fn agent_notes(&self) -> Vec<SetupNote> {
        let (now, next) = (self.now, &self.next);
        let mut notes = Vec::new();
        for (kind, program) in
            next.agent_commands.iter().filter(|(kind, program)| now.agent_commands.get(*kind) != Some(*program))
        {
            notes.push(risk(format!("runs {program} as the {kind} agent")));
            notes.extend(self.program_note(program));
        }
        if self.changed("agent").is_some() && next.agent != agents::AUTO {
            notes.extend(self.program_note(&agents::command(next, &next.agent)));
        }
        notes.extend(self.dangerous_modes());
        if next.accept_trust_prompts && !now.accept_trust_prompts {
            notes.push(risk("answers agents' trust questions by itself".into()));
        }
        notes
    }

    fn dangerous_modes(&self) -> Vec<SetupNote> {
        let kinds: BTreeSet<&String> = self.next.agent_args.keys().chain(self.next.agent_modes.keys()).collect();
        kinds
            .into_iter()
            .filter_map(|kind| {
                let mode = mode_in(&self.next, kind).filter(|mode| agents::is_dangerous(mode))?;
                (mode_in(self.now, kind).as_ref() != Some(&mode)).then(|| risk(format!("starts {kind} in {mode} mode")))
            })
            .collect()
    }

    fn worktree_notes(&self) -> Vec<SetupNote> {
        let mut notes = Vec::new();
        let dir = &self.next.worktrees_dir;
        if self.changed("worktrees_dir").is_some() && !self.exists(dir) {
            notes.push(missing(format!("{dir} does not exist here yet: it will be made with the first worktree")));
        }
        for (key, program, place) in [("gh", &self.next.gh, "GitHub"), ("glab", &self.next.glab, "GitLab")] {
            if self.changed(key).is_some() {
                notes.push(risk(format!("runs {program} for {place}")));
                notes.extend(self.program_note(program));
            }
        }
        notes
    }

    fn sound_notes(&self) -> Vec<SetupNote> {
        [("sound_waiting", &self.next.sound_waiting), ("sound_done", &self.next.sound_done)]
            .into_iter()
            .filter(|(key, path)| self.changed(key).is_some() && !path.trim().is_empty() && !self.exists(path))
            .map(|(_, path)| missing(format!("{path} does not exist here")))
            .collect()
    }

    fn exists(&self, path: &str) -> bool {
        config::expand_home(path.trim(), self.here.home.as_deref()).exists()
    }

    fn program_note(&self, program: &str) -> Option<SetupNote> {
        let name = program.split_whitespace().next()?;
        if name.contains('/') {
            (!self.exists(name)).then(|| missing(format!("{name} does not exist here")))
        } else {
            (!agents::installed(name)).then(|| missing(format!("{name} is not on the PATH here")))
        }
    }

    fn repositories(&self) -> (Vec<SetupChange>, Vec<SetupNote>) {
        let repositories = &self.shared.repositories;
        let mut notes: Vec<SetupNote> = repositories
            .iter()
            .filter(|repository| matches(repository, self.here).is_empty())
            .map(|repository| {
                let remote = repository.remotes.first().map_or("", String::as_str);
                note(
                    NoteKind::Repository,
                    format!("{remote}: no open project has this remote; open it and import again"),
                )
            })
            .collect();
        let changes: Vec<SetupChange> = repositories
            .iter()
            .flat_map(|repository| {
                matches(repository, self.here)
                    .into_iter()
                    .filter_map(|project| self.repository_change(repository, project))
            })
            .collect();
        let risky = changes.iter().filter(|change| change.to != NOTHING);
        notes.extend(
            risky.map(|change| risk(format!("runs these setup commands in every new worktree of {}", change.label))),
        );
        (changes, notes)
    }

    fn repository_change(&self, repository: &Repository, project: &OpenRepository) -> Option<SetupChange> {
        let home = self.here.home.as_deref();
        let now = self.now.project(&project.folder, home);
        let wanted =
            ProjectSettings { setup: repository.setup.clone(), teardown: repository.teardown.clone() }.normalised();
        (wanted != now).then(|| SetupChange {
            key: Section::Repositories.id().into(),
            label: text::display_path(&project.folder, home),
            from: commands(&now),
            to: commands(&wanted),
        })
    }

    fn notes(&self) -> Vec<SetupNote> {
        let unknown = self.shared.unknown.iter().map(|key| note(NoteKind::Unknown, unknown(key)));
        let connect = self.shared.connect.iter().filter(|tracker| !self.here.connected.contains(tracker));
        let connect = connect.map(|tracker| note(NoteKind::Connect, format!("connect {tracker} in Settings › Issues")));
        let secrets = secrets::warnings(self.shared).into_iter().map(|warning| note(NoteKind::Secret, warning));
        unknown.chain(connect).chain(secrets).collect()
    }
}

#[cfg(test)]
mod tests;
