use std::path::Path;

use serde_json::Value;

use super::{Action, PickItem, Row, Settings};
use crate::agents;
use crate::config::{Config, ShellChoice};
use crate::control::config::{Choice, Kind, Setting};
use crate::process;
use crate::term::shell::{self, Found};

const DEFAULT: &str = "default";
const ANOTHER: &str = "another program…";

pub(crate) const QUESTION: &str = "Which shell should new panes start?";
pub(crate) const PROGRAM_FIELD: &str = "the shell new panes start: a full path, one starting with ~/, or a name on the \
                                        PATH (empty is the default)";
pub(crate) const ARGS_FIELD: &str = "arguments of the shell, such as -NoLogo or --login; quote a word with a space or \
                                     a \\ in single quotes (empty: none)";

#[derive(Debug, Default)]
pub(crate) struct ShellsHere {
    pub(crate) default: String,
    pub(crate) found: Vec<Found>,
    pub(crate) problem: Option<String>,
}

pub(crate) fn name_of(program: &str) -> &str {
    let name = Path::new(program).file_name().and_then(|name| name.to_str()).unwrap_or(program);
    process::program_name(name)
}

fn with(config: &Config, program: String, args: Vec<String>) -> Config {
    Config { shell: ShellChoice { program, args }, ..config.clone() }
}

fn item(value: &str, note: &str) -> PickItem {
    PickItem { value: value.into(), note: note.into(), dangerous: false }
}

impl Settings {
    fn default_shell_name(&self) -> &str {
        name_of(&self.shells.default)
    }

    fn program(&self) -> &str {
        self.config.shell.program.trim()
    }

    pub(super) fn shell_row(&self, row: &Row) -> (String, String, String, bool) {
        if *row == Row::ShellArgs {
            let args = &self.config.shell.args;
            let value = if args.is_empty() { "none".to_string() } else { agents::join_args(args) };
            let note = "given to the shell when a pane starts, such as -NoLogo or --login";
            return ("shell arguments".into(), value, note.into(), false);
        }
        let value = match self.program() {
            "" => format!("{DEFAULT} ({})", self.default_shell_name()),
            program => program.to_string(),
        };
        match &self.shells.problem {
            Some(why) => {
                let note = format!("cannot start: {why}; new panes start {}", self.default_shell_name());
                ("shell".into(), value, note, true)
            }
            None => ("shell".into(), value, "the program a new pane starts; open panes keep theirs".into(), false),
        }
    }

    pub(super) fn shell_items(&self) -> Vec<PickItem> {
        let mut items = vec![item(DEFAULT, &self.shells.default)];
        items.extend(self.shells.found.iter().map(|found| item(&found.value, &found.note)));
        let program = self.program();
        let listed = program.is_empty() || self.shells.found.iter().any(|found| found.value == program);
        items.push(item(ANOTHER, if listed { "" } else { program }));
        items
    }

    pub(super) fn shell_current(&self) -> String {
        let program = self.program();
        if program.is_empty() {
            DEFAULT.into()
        } else if self.shells.found.iter().any(|found| found.value == program) {
            program.into()
        } else {
            ANOTHER.into()
        }
    }

    pub(super) fn activate_shell(&mut self, row: Row) -> Action {
        if row == Row::ShellArgs {
            return self.start_edit(row, agents::join_args(&self.config.shell.args));
        }
        let items = self.shell_items();
        self.open_pick(row, items);
        Action::None
    }

    pub(super) fn edit_shell(&mut self) {
        self.start_edit(Row::Shell, self.config.shell.program.clone());
    }

    pub(super) fn shell_chosen(&mut self, value: &str) -> Action {
        match value {
            ANOTHER => {
                self.edit_shell();
                Action::None
            }
            DEFAULT => self.save_shell(String::new()),
            program => self.save_shell(program.to_string()),
        }
    }

    fn save_shell(&mut self, program: String) -> Action {
        let args = if program == self.config.shell.program { self.config.shell.args.clone() } else { Vec::new() };
        let notice = if program.is_empty() {
            format!("new panes start your default shell, {}", self.default_shell_name())
        } else {
            format!("new panes start {program}; open panes keep theirs")
        };
        self.save(with(&self.config, program, args), notice)
    }

    pub(super) fn submit_shell(&mut self) -> Action {
        let Some(edit) = &mut self.edit else { return Action::None };
        let input = edit.input.trim().to_string();
        if !input.is_empty()
            && !input.eq_ignore_ascii_case(DEFAULT)
            && let Err(why) = shell::resolve(&input, self.home.as_deref())
        {
            edit.error = Some(format!("`{input}`: {why}"));
            return Action::None;
        }
        self.edit = None;
        let program = if input.eq_ignore_ascii_case(DEFAULT) { String::new() } else { input };
        self.save_shell(program)
    }

    pub(super) fn submit_shell_args(&mut self) -> Action {
        let Some(edit) = self.edit.take() else { return Action::None };
        let args = agents::split_args(&edit.input);
        let shown = match self.program() {
            "" => self.default_shell_name().to_string(),
            program => program.to_string(),
        };
        let notice = if args.is_empty() {
            format!("{shown} starts with no arguments")
        } else {
            format!("{shown} starts with: {}", agents::join_args(&args))
        };
        self.save(with(&self.config, self.config.shell.program.clone(), args), notice)
    }

    pub(super) fn shell_setting(&self, whole: &Row) -> Setting {
        if *whole == Row::ShellArgs {
            let mut setting = self.base(whole, "shell.args", Kind::Text);
            let args = agents::join_args(&self.config.shell.args);
            setting.value = Value::String(args);
            setting.default = Value::String(String::new());
            setting.field = Some(ARGS_FIELD.into());
            return setting;
        }
        let mut setting = self.base(whole, "shell", Kind::Text);
        setting.value = Value::String(self.config.shell.program.clone());
        setting.default = Value::String(String::new());
        setting.field = Some(PROGRAM_FIELD.into());
        setting.question = Some(QUESTION.into());
        setting.choices = self
            .shells
            .found
            .iter()
            .map(|found| Choice { value: found.value.clone(), note: found.note.clone(), dangerous: false })
            .collect();
        setting
    }
}

#[cfg(test)]
mod tests;
