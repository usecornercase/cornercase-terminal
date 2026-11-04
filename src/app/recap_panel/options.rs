use super::App;
use crate::agents::{self, Known};
use crate::recap::Options;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(in crate::app) struct Overrides {
    pub(in crate::app) agent: Option<String>,
    pub(in crate::app) account: Option<String>,
    pub(in crate::app) model: Option<String>,
    pub(in crate::app) effort: Option<String>,
    pub(in crate::app) language: Option<String>,
}

fn named(text: &str) -> Option<String> {
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

impl App {
    pub(in crate::app) fn recap_agent(&self, wanted: Option<&str>) -> Known {
        let from = |name: &str| Known::of(name).filter(|kind| kind.recap().is_some());
        wanted
            .and_then(from)
            .or_else(|| from(self.config.recap_agent.trim()))
            .or_else(|| from(self.config.agent.trim()))
            .unwrap_or(Known::Claude)
    }

    pub(in crate::app) fn recap_options(&self, overrides: &Overrides) -> Options {
        let kind = self.recap_agent(overrides.agent.as_deref());
        let pick = |given: &Option<String>, setting: &str| {
            given.clone().and_then(|text| named(&text)).or_else(|| named(setting))
        };
        let account = pick(&overrides.account, &self.config.recap_account)
            .filter(|name| crate::accounts::names(self.accounts.dir(), kind).contains(name));
        let effort = pick(&overrides.effort, &self.config.recap_effort)
            .filter(|level| agents::effort_refusal(kind.name(), level).is_none());
        Options {
            agent: kind.name().to_string(),
            account,
            model: pick(&overrides.model, &self.config.recap_model),
            effort,
            language: pick(&overrides.language, &self.config.recap_language).unwrap_or_else(|| "auto".into()),
            instructions: self.config.recap_instructions.trim().to_string(),
        }
    }
}
