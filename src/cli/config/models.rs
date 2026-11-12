use crate::cli::{print_json, print_out};
use crate::client::{answer, ask};
use crate::control::config::AgentModelsInfo;
use crate::control::{AgentModels, Command};
use crate::error::Result;

pub(super) fn list(agent: String, as_json: bool) -> Result<()> {
    let value = ask("agent-models", Command::AgentModels(AgentModels { agent }))?;
    if as_json {
        return print_json(&value);
    }
    print_out(&lines(&answer(value)?))
}

fn lines(listed: &AgentModelsInfo) -> String {
    listed.models.iter().fold(String::new(), |text, model| text + model + "\n")
}

#[cfg(test)]
mod tests;
