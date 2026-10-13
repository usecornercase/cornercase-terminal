use super::{ModeTable, claude, codex, gemini, opencode, usage};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Known {
    Claude,
    Codex,
    Gemini,
    Opencode,
}

impl Known {
    pub(crate) const ALL: [Self; 4] = [Self::Claude, Self::Codex, Self::Gemini, Self::Opencode];

    pub(crate) fn of(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.name() == name)
    }

    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Claude => claude::NAME,
            Self::Codex => codex::NAME,
            Self::Gemini => gemini::NAME,
            Self::Opencode => opencode::NAME,
        }
    }

    pub(crate) fn title(self) -> &'static str {
        match self {
            Self::Claude => claude::TITLE,
            Self::Codex => codex::TITLE,
            Self::Gemini => gemini::TITLE,
            Self::Opencode => opencode::TITLE,
        }
    }

    pub(crate) fn modes(self) -> ModeTable {
        match self {
            Self::Claude => claude::MODES,
            Self::Codex => codex::MODES,
            Self::Gemini => gemini::MODES,
            Self::Opencode => opencode::MODES,
        }
    }

    pub(crate) fn normalized(self, args: &[String]) -> Vec<String> {
        match self {
            Self::Claude => claude::normalized(args),
            Self::Codex => codex::normalized(args),
            Self::Gemini => gemini::normalized(args),
            Self::Opencode => opencode::normalized(args),
        }
    }

    pub(crate) fn resume(self, args: Vec<String>, conversation: String) -> Option<Vec<String>> {
        match self {
            Self::Claude => claude::resume(args, conversation),
            Self::Codex => codex::resume(args, conversation),
            Self::Gemini => gemini::resume(args, conversation),
            Self::Opencode => opencode::RESUME,
        }
    }

    pub(crate) fn runs_as(self, arg: &str) -> bool {
        match self {
            Self::Claude => claude::RUNS_AS,
            Self::Codex => codex::runs_as(arg),
            Self::Gemini => gemini::RUNS_AS,
            Self::Opencode => opencode::RUNS_AS,
        }
    }

    pub(crate) fn input_box(self, screen: impl FnOnce() -> String) -> Option<bool> {
        match self {
            Self::Claude => claude::screen::input_box(&screen()),
            Self::Codex => codex::INPUT_BOX,
            Self::Gemini => gemini::INPUT_BOX,
            Self::Opencode => opencode::INPUT_BOX,
        }
    }

    pub(crate) fn survey(self, screen: &str) -> bool {
        match self {
            Self::Claude => claude::screen::survey(screen),
            Self::Codex => codex::SURVEY,
            Self::Gemini => gemini::SURVEY,
            Self::Opencode => opencode::SURVEY,
        }
    }

    pub(crate) fn holds_prompts(self) -> bool {
        match self {
            Self::Claude => claude::HOLDS_PROMPTS,
            Self::Codex => codex::HOLDS_PROMPTS,
            Self::Gemini => gemini::HOLDS_PROMPTS,
            Self::Opencode => opencode::HOLDS_PROMPTS,
        }
    }

    pub(crate) fn local_command(self, text: &str) -> bool {
        match self {
            Self::Claude => claude::LOCAL_COMMAND,
            Self::Codex => codex::LOCAL_COMMAND,
            Self::Gemini => gemini::local_command(text),
            Self::Opencode => opencode::LOCAL_COMMAND,
        }
    }

    pub(crate) fn unfound(self) -> &'static str {
        match self {
            Self::Claude => claude::UNFOUND,
            Self::Codex => codex::UNFOUND,
            Self::Gemini => gemini::UNFOUND,
            Self::Opencode => opencode::UNFOUND,
        }
    }

    pub(crate) fn usage(self) -> Option<usage::Spec> {
        match self {
            Self::Claude => claude::USAGE,
            Self::Codex => codex::USAGE,
            Self::Gemini => gemini::USAGE,
            Self::Opencode => opencode::USAGE,
        }
    }
}

#[cfg(test)]
mod tests;
