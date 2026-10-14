use super::{ModeTable, claude, codex, gemini, grok, opencode, usage};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Known {
    Claude,
    Codex,
    Gemini,
    Opencode,
    Grok,
}

impl Known {
    pub(crate) const ALL: [Self; 5] = [Self::Claude, Self::Codex, Self::Gemini, Self::Opencode, Self::Grok];

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
        }
    }

    pub(crate) fn title(self) -> &'static str {
        match self {
            Self::Claude => claude::TITLE,
            Self::Codex => codex::TITLE,
            Self::Gemini => gemini::TITLE,
            Self::Opencode => opencode::TITLE,
            Self::Grok => grok::TITLE,
        }
    }

    pub(crate) fn modes(self) -> ModeTable {
        match self {
            Self::Claude => claude::MODES,
            Self::Codex => codex::MODES,
            Self::Gemini => gemini::MODES,
            Self::Opencode => opencode::MODES,
            Self::Grok => grok::MODES,
        }
    }

    pub(crate) fn model_alias(self) -> Option<&'static str> {
        match self {
            Self::Claude => claude::MODEL_ALIAS,
            Self::Codex => codex::MODEL_ALIAS,
            Self::Gemini => gemini::MODEL_ALIAS,
            Self::Opencode => opencode::MODEL_ALIAS,
            Self::Grok => grok::MODEL_ALIAS,
        }
    }

    pub(crate) fn model_flag(self) -> &'static str {
        match self {
            Self::Claude => claude::MODEL_FLAG,
            Self::Codex => codex::MODEL_FLAG,
            Self::Gemini => gemini::MODEL_FLAG,
            Self::Opencode => opencode::MODEL_FLAG,
            Self::Grok => grok::MODEL_FLAG,
        }
    }

    pub(crate) fn unattended(self) -> &'static [&'static str] {
        match self {
            Self::Claude => claude::UNATTENDED,
            Self::Codex => codex::UNATTENDED,
            Self::Gemini => gemini::UNATTENDED,
            Self::Opencode => opencode::UNATTENDED,
            Self::Grok => grok::UNATTENDED,
        }
    }

    pub(crate) fn models(self) -> &'static [&'static str] {
        match self {
            Self::Claude => claude::MODELS,
            Self::Codex => codex::MODELS,
            Self::Gemini => gemini::MODELS,
            Self::Opencode => opencode::MODELS,
            Self::Grok => grok::MODELS,
        }
    }

    pub(crate) fn catalogue(self) -> Option<fn() -> Vec<String>> {
        match self {
            Self::Claude => claude::CATALOGUE,
            Self::Codex => codex::CATALOGUE,
            Self::Gemini => gemini::CATALOGUE,
            Self::Opencode => opencode::CATALOGUE,
            Self::Grok => grok::CATALOGUE,
        }
    }

    pub(crate) fn status_by_folder(self) -> bool {
        match self {
            Self::Claude => claude::STATUS_BY_FOLDER,
            Self::Codex => codex::STATUS_BY_FOLDER,
            Self::Gemini => gemini::STATUS_BY_FOLDER,
            Self::Opencode => opencode::STATUS_BY_FOLDER,
            Self::Grok => grok::STATUS_BY_FOLDER,
        }
    }

    pub(crate) fn record_by_folder(self) -> bool {
        match self {
            Self::Claude => claude::RECORD_BY_FOLDER,
            Self::Codex => codex::RECORD_BY_FOLDER,
            Self::Gemini => gemini::RECORD_BY_FOLDER,
            Self::Opencode => opencode::RECORD_BY_FOLDER,
            Self::Grok => grok::RECORD_BY_FOLDER,
        }
    }

    pub(crate) fn normalized(self, args: &[String]) -> Vec<String> {
        match self {
            Self::Claude => claude::normalized(args),
            Self::Codex => codex::normalized(args),
            Self::Gemini => gemini::normalized(args),
            Self::Opencode => opencode::normalized(args),
            Self::Grok => grok::normalized(args),
        }
    }

    pub(crate) fn resume(self, args: Vec<String>, conversation: String) -> Option<Vec<String>> {
        match self {
            Self::Claude => claude::resume(args, conversation),
            Self::Codex => codex::resume(args, conversation),
            Self::Gemini => gemini::resume(args, conversation),
            Self::Opencode => opencode::RESUME,
            Self::Grok => grok::resume(args, conversation),
        }
    }

    pub(crate) fn runs_as(self, arg: &str) -> bool {
        match self {
            Self::Claude => claude::RUNS_AS,
            Self::Codex => codex::runs_as(arg),
            Self::Gemini => gemini::RUNS_AS,
            Self::Opencode => opencode::RUNS_AS,
            Self::Grok => grok::RUNS_AS,
        }
    }

    pub(crate) fn input_box(self, screen: impl FnOnce() -> String) -> Option<bool> {
        match self {
            Self::Claude => claude::screen::input_box(&screen()),
            Self::Codex => codex::INPUT_BOX,
            Self::Gemini => gemini::INPUT_BOX,
            Self::Opencode => opencode::INPUT_BOX,
            Self::Grok => grok::INPUT_BOX,
        }
    }

    pub(crate) fn survey(self, screen: &str) -> bool {
        match self {
            Self::Claude => claude::screen::survey(screen),
            Self::Codex => codex::SURVEY,
            Self::Gemini => gemini::SURVEY,
            Self::Opencode => opencode::SURVEY,
            Self::Grok => grok::SURVEY,
        }
    }

    pub(crate) fn holds_prompts(self) -> bool {
        match self {
            Self::Claude => claude::HOLDS_PROMPTS,
            Self::Codex => codex::HOLDS_PROMPTS,
            Self::Gemini => gemini::HOLDS_PROMPTS,
            Self::Opencode => opencode::HOLDS_PROMPTS,
            Self::Grok => grok::HOLDS_PROMPTS,
        }
    }

    pub(crate) fn local_command(self, text: &str) -> bool {
        match self {
            Self::Claude => claude::LOCAL_COMMAND,
            Self::Codex => codex::LOCAL_COMMAND,
            Self::Gemini => gemini::local_command(text),
            Self::Opencode => opencode::LOCAL_COMMAND,
            Self::Grok => grok::local_command(text),
        }
    }

    pub(crate) fn unfound(self) -> &'static str {
        match self {
            Self::Claude => claude::UNFOUND,
            Self::Codex => codex::UNFOUND,
            Self::Gemini => gemini::UNFOUND,
            Self::Opencode => opencode::UNFOUND,
            Self::Grok => grok::UNFOUND,
        }
    }

    pub(crate) fn usage(self) -> Option<usage::Spec> {
        match self {
            Self::Claude => claude::USAGE,
            Self::Codex => codex::USAGE,
            Self::Gemini => gemini::USAGE,
            Self::Opencode => opencode::USAGE,
            Self::Grok => grok::USAGE,
        }
    }
}

#[cfg(test)]
mod tests;
