use super::{ModeTable, claude, codex, copilot, cursor, gemini, grok, opencode, usage, vibe};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Known {
    Claude,
    Codex,
    Gemini,
    Opencode,
    Grok,
    Cursor,
    Copilot,
    Vibe,
}

impl Known {
    pub(crate) const ALL: [Self; 8] =
        [Self::Claude, Self::Codex, Self::Gemini, Self::Opencode, Self::Grok, Self::Cursor, Self::Copilot, Self::Vibe];

    pub(crate) fn of(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.name() == name)
    }

    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Claude => claude::NAME,
            Self::Codex => codex::NAME,
            Self::Gemini => gemini::NAME,
            Self::Opencode => opencode::NAME,
            Self::Grok => grok::NAME,
            Self::Cursor => cursor::NAME,
            Self::Copilot => copilot::NAME,
            Self::Vibe => vibe::NAME,
        }
    }

    pub(crate) fn command(self) -> &'static str {
        match self {
            Self::Claude => claude::COMMAND,
            Self::Codex => codex::COMMAND,
            Self::Gemini => gemini::COMMAND,
            Self::Opencode => opencode::COMMAND,
            Self::Grok => grok::COMMAND,
            Self::Cursor => cursor::COMMAND,
            Self::Copilot => copilot::COMMAND,
            Self::Vibe => vibe::COMMAND,
        }
    }

    pub(crate) fn title(self) -> &'static str {
        match self {
            Self::Claude => claude::TITLE,
            Self::Codex => codex::TITLE,
            Self::Gemini => gemini::TITLE,
            Self::Opencode => opencode::TITLE,
            Self::Grok => grok::TITLE,
            Self::Cursor => cursor::TITLE,
            Self::Copilot => copilot::TITLE,
            Self::Vibe => vibe::TITLE,
        }
    }

    pub(crate) fn modes(self) -> ModeTable {
        match self {
            Self::Claude => claude::MODES,
            Self::Codex => codex::MODES,
            Self::Gemini => gemini::MODES,
            Self::Opencode => opencode::MODES,
            Self::Grok => grok::MODES,
            Self::Cursor => cursor::MODES,
            Self::Copilot => copilot::MODES,
            Self::Vibe => vibe::MODES,
        }
    }

    pub(crate) fn add_dir(self) -> Option<&'static str> {
        match self {
            Self::Claude => claude::ADD_DIR,
            Self::Codex => codex::ADD_DIR,
            Self::Gemini => gemini::ADD_DIR,
            Self::Opencode => opencode::ADD_DIR,
            Self::Grok => grok::ADD_DIR,
            Self::Cursor => cursor::ADD_DIR,
            Self::Copilot => copilot::ADD_DIR,
            Self::Vibe => vibe::ADD_DIR,
        }
    }

    pub(crate) fn account(self) -> Option<super::AccountHome> {
        match self {
            Self::Claude => claude::ACCOUNT,
            Self::Codex => codex::ACCOUNT,
            Self::Gemini => gemini::ACCOUNT,
            Self::Opencode => opencode::ACCOUNT,
            Self::Grok => grok::ACCOUNT,
            Self::Cursor => cursor::ACCOUNT,
            Self::Copilot => copilot::ACCOUNT,
            Self::Vibe => vibe::ACCOUNT,
        }
    }

    pub(crate) fn login(self, text: &str) -> crate::accounts::login::Login {
        match self {
            Self::Claude => claude::login(text),
            Self::Codex => codex::login(text),
            Self::Gemini | Self::Opencode | Self::Grok | Self::Cursor | Self::Copilot | Self::Vibe => {
                crate::accounts::login::Login::default()
            }
        }
    }

    pub(crate) fn effort(self) -> Option<super::Effort> {
        match self {
            Self::Claude => claude::EFFORT,
            Self::Codex => codex::EFFORT,
            Self::Gemini => gemini::EFFORT,
            Self::Opencode => opencode::EFFORT,
            Self::Grok => grok::EFFORT,
            Self::Cursor => cursor::EFFORT,
            Self::Copilot => copilot::EFFORT,
            Self::Vibe => vibe::EFFORT,
        }
    }

    pub(crate) fn compact(self) -> Option<super::Compact> {
        match self {
            Self::Claude => claude::COMPACT,
            Self::Codex => codex::COMPACT,
            Self::Gemini => gemini::COMPACT,
            Self::Opencode => opencode::COMPACT,
            Self::Grok => grok::COMPACT,
            Self::Cursor => cursor::COMPACT,
            Self::Copilot => copilot::COMPACT,
            Self::Vibe => vibe::COMPACT,
        }
    }

    pub(crate) fn compacts(self, text: &str) -> bool {
        self.compact().is_some_and(|compact| text.split_whitespace().next() == Some(compact.command))
    }

    pub(crate) fn model_alias(self) -> Option<&'static str> {
        match self {
            Self::Claude => claude::MODEL_ALIAS,
            Self::Codex => codex::MODEL_ALIAS,
            Self::Gemini => gemini::MODEL_ALIAS,
            Self::Opencode => opencode::MODEL_ALIAS,
            Self::Grok => grok::MODEL_ALIAS,
            Self::Cursor => cursor::MODEL_ALIAS,
            Self::Copilot => copilot::MODEL_ALIAS,
            Self::Vibe => vibe::MODEL_ALIAS,
        }
    }

    pub(crate) fn model_flag(self) -> Option<&'static str> {
        match self {
            Self::Claude => Some(claude::MODEL_FLAG),
            Self::Codex => Some(codex::MODEL_FLAG),
            Self::Gemini => Some(gemini::MODEL_FLAG),
            Self::Opencode => Some(opencode::MODEL_FLAG),
            Self::Grok => Some(grok::MODEL_FLAG),
            Self::Cursor => Some(cursor::MODEL_FLAG),
            Self::Copilot => Some(copilot::MODEL_FLAG),
            Self::Vibe => vibe::MODEL_FLAG,
        }
    }

    pub(crate) fn unattended(self) -> &'static [&'static str] {
        match self {
            Self::Claude => claude::UNATTENDED,
            Self::Codex => codex::UNATTENDED,
            Self::Gemini => gemini::UNATTENDED,
            Self::Opencode => opencode::UNATTENDED,
            Self::Grok => grok::UNATTENDED,
            Self::Cursor => cursor::UNATTENDED,
            Self::Copilot => copilot::UNATTENDED,
            Self::Vibe => vibe::UNATTENDED,
        }
    }

    pub(crate) fn models(self) -> &'static [&'static str] {
        match self {
            Self::Claude => claude::MODELS,
            Self::Codex => codex::MODELS,
            Self::Gemini => gemini::MODELS,
            Self::Opencode => opencode::MODELS,
            Self::Grok => grok::MODELS,
            Self::Cursor => cursor::MODELS,
            Self::Copilot => copilot::MODELS,
            Self::Vibe => vibe::MODELS,
        }
    }

    pub(crate) fn catalogue(self) -> Option<fn() -> Vec<String>> {
        match self {
            Self::Claude => claude::CATALOGUE,
            Self::Codex => codex::CATALOGUE,
            Self::Gemini => gemini::CATALOGUE,
            Self::Opencode => opencode::CATALOGUE,
            Self::Grok => grok::CATALOGUE,
            Self::Cursor => cursor::CATALOGUE,
            Self::Copilot => copilot::CATALOGUE,
            Self::Vibe => vibe::CATALOGUE,
        }
    }

    pub(crate) fn status_by_folder(self) -> bool {
        match self {
            Self::Claude => claude::STATUS_BY_FOLDER,
            Self::Codex => codex::STATUS_BY_FOLDER,
            Self::Gemini => gemini::STATUS_BY_FOLDER,
            Self::Opencode => opencode::STATUS_BY_FOLDER,
            Self::Grok => grok::STATUS_BY_FOLDER,
            Self::Cursor => cursor::STATUS_BY_FOLDER,
            Self::Copilot => copilot::STATUS_BY_FOLDER,
            Self::Vibe => vibe::STATUS_BY_FOLDER,
        }
    }

    pub(crate) fn record_by_folder(self) -> bool {
        match self {
            Self::Claude => claude::RECORD_BY_FOLDER,
            Self::Codex => codex::RECORD_BY_FOLDER,
            Self::Gemini => gemini::RECORD_BY_FOLDER,
            Self::Opencode => opencode::RECORD_BY_FOLDER,
            Self::Grok => grok::RECORD_BY_FOLDER,
            Self::Cursor => cursor::RECORD_BY_FOLDER,
            Self::Copilot => copilot::RECORD_BY_FOLDER,
            Self::Vibe => vibe::RECORD_BY_FOLDER,
        }
    }

    pub(crate) fn normalized(self, args: &[String]) -> Vec<String> {
        match self {
            Self::Claude => claude::normalized(args),
            Self::Codex => codex::normalized(args),
            Self::Gemini => gemini::normalized(args),
            Self::Opencode => opencode::normalized(args),
            Self::Grok => grok::normalized(args),
            Self::Cursor => cursor::normalized(args),
            Self::Copilot => copilot::normalized(args),
            Self::Vibe => vibe::normalized(args),
        }
    }

    pub(crate) fn resume(self, args: Vec<String>, conversation: String) -> Option<Vec<String>> {
        match self {
            Self::Claude => claude::resume(args, conversation),
            Self::Codex => codex::resume(args, conversation),
            Self::Gemini => gemini::resume(args, conversation),
            Self::Opencode => opencode::RESUME,
            Self::Grok => grok::resume(args, conversation),
            Self::Cursor => cursor::resume(args, conversation),
            Self::Copilot => copilot::resume(args, conversation),
            Self::Vibe => vibe::resume(args, conversation),
        }
    }

    pub(crate) fn runs_as(self, arg: &str) -> bool {
        match self {
            Self::Claude => claude::RUNS_AS,
            Self::Codex => codex::runs_as(arg),
            Self::Gemini => gemini::RUNS_AS,
            Self::Opencode => opencode::RUNS_AS,
            Self::Grok => grok::RUNS_AS,
            Self::Cursor => cursor::runs_as(arg),
            Self::Copilot => copilot::runs_as(arg),
            Self::Vibe => vibe::runs_as(arg),
        }
    }

    pub(crate) fn input_box(self, screen: impl FnOnce() -> String) -> Option<bool> {
        match self {
            Self::Claude => claude::screen::input_box(&screen()),
            Self::Codex => codex::INPUT_BOX,
            Self::Gemini => gemini::INPUT_BOX,
            Self::Opencode => opencode::INPUT_BOX,
            Self::Grok => grok::INPUT_BOX,
            Self::Cursor => cursor::INPUT_BOX,
            Self::Copilot => copilot::INPUT_BOX,
            Self::Vibe => vibe::INPUT_BOX,
        }
    }

    pub(crate) fn draft(self, lines: impl FnOnce() -> Vec<(String, String)>) -> Option<bool> {
        match self {
            Self::Claude => claude::screen::draft(&lines()),
            Self::Codex => codex::INPUT_BOX,
            Self::Gemini => gemini::INPUT_BOX,
            Self::Opencode => opencode::INPUT_BOX,
            Self::Grok => grok::INPUT_BOX,
            Self::Cursor => cursor::INPUT_BOX,
            Self::Copilot => copilot::INPUT_BOX,
            Self::Vibe => vibe::INPUT_BOX,
        }
    }

    pub(crate) fn survey(self, screen: &str) -> bool {
        match self {
            Self::Claude => claude::screen::survey(screen),
            Self::Codex => codex::SURVEY,
            Self::Gemini => gemini::SURVEY,
            Self::Opencode => opencode::SURVEY,
            Self::Grok => grok::SURVEY,
            Self::Cursor => cursor::SURVEY,
            Self::Copilot => copilot::SURVEY,
            Self::Vibe => vibe::SURVEY,
        }
    }

    pub(crate) fn holds_prompts(self) -> bool {
        match self {
            Self::Claude => claude::HOLDS_PROMPTS,
            Self::Codex => codex::HOLDS_PROMPTS,
            Self::Gemini => gemini::HOLDS_PROMPTS,
            Self::Opencode => opencode::HOLDS_PROMPTS,
            Self::Grok => grok::HOLDS_PROMPTS,
            Self::Cursor => cursor::HOLDS_PROMPTS,
            Self::Copilot => copilot::HOLDS_PROMPTS,
            Self::Vibe => vibe::HOLDS_PROMPTS,
        }
    }

    pub(crate) fn local_command(self, text: &str) -> bool {
        match self {
            Self::Claude => claude::LOCAL_COMMAND,
            Self::Codex => codex::LOCAL_COMMAND,
            Self::Gemini => gemini::local_command(text),
            Self::Opencode => opencode::LOCAL_COMMAND,
            Self::Grok => grok::local_command(text),
            Self::Cursor => cursor::local_command(text),
            Self::Copilot => copilot::local_command(text),
            Self::Vibe => vibe::local_command(text),
        }
    }

    pub(crate) fn unfound(self) -> &'static str {
        match self {
            Self::Claude => claude::UNFOUND,
            Self::Codex => codex::UNFOUND,
            Self::Gemini => gemini::UNFOUND,
            Self::Opencode => opencode::UNFOUND,
            Self::Grok => grok::UNFOUND,
            Self::Cursor => cursor::UNFOUND,
            Self::Copilot => copilot::UNFOUND,
            Self::Vibe => vibe::UNFOUND,
        }
    }

    pub(crate) fn usage(self) -> Option<usage::Spec> {
        match self {
            Self::Claude => claude::USAGE,
            Self::Codex => codex::USAGE,
            Self::Gemini => gemini::USAGE,
            Self::Opencode => opencode::USAGE,
            Self::Grok => grok::USAGE,
            Self::Cursor => cursor::USAGE,
            Self::Copilot => copilot::USAGE,
            Self::Vibe => vibe::USAGE,
        }
    }
}

#[cfg(test)]
mod tests;
