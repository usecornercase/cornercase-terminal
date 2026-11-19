use super::{HEAD, Text, failed, fronts_agent};
use crate::agents;
use crate::agents::activity::Status;
use crate::app::App;
use crate::app::control::Asker;
use crate::config::Config;
use crate::control::contract::{Code, Refusal};
use crate::project::Hierarchy;
use crate::term::Term;

const NEEDS_TEXT: &str = "a bot's send needs a text: Enter alone is a key";
const NOT_A_PROMPT: &str = "a bot writes prompts to an agent, not its commands: a text starting with `/` or `!` is the \
                            agent's own command or its shell; start the text with a word";

fn refused(why: &str) -> Refusal {
    Refusal::new(Code::Failed, format!("{HEAD}, {why}"))
}

fn runs_none(pane: u64) -> Refusal {
    refused(&format!("and pane {pane} runs none; `cornercase status --panes` lists the agents"))
}

fn asks(pane: u64) -> Refusal {
    refused(&format!("and the agent in pane {pane} asks something: only you answer an agent's questions, in its tab"))
}

pub(super) fn not_a_prompt(text: &str, agent: Option<agents::Known>) -> bool {
    text.trim_start().starts_with(['/', '!']) || agent.is_some_and(|agent| agent.local_command(text))
}

pub(in crate::app) fn takes_a_bots_enter(config: &Config, term: &Term) -> bool {
    fronts_agent(config, term) && term.agent.status() != Some(Status::Waiting)
}

impl App {
    pub(super) fn bot_types(
        &self,
        asker: Asker,
        (pane, tab): (Option<u64>, Option<u64>),
        text: Text,
    ) -> Result<(), Refusal> {
        if text == Text::Missing {
            return Err(failed(NEEDS_TEXT));
        }
        let pane = self.target(asker, pane, tab)?;
        self.types_into(pane)?;
        let said = match text {
            Text::Given(text) => Some(text),
            Text::Todo(id) => self.panels.todos.item(id).map(|item| item.text.clone()),
            Text::Kept | Text::Missing => None,
        };
        let agent = self.projects.pane_by(pane).and_then(|term| self.foreground_agent(term));
        if said.is_some_and(|said| not_a_prompt(&said, agent)) { Err(failed(NOT_A_PROMPT)) } else { Ok(()) }
    }

    pub(in crate::app::control) fn bot_prompts(
        &self,
        client: u64,
        kind: &str,
        prompt: Option<&str>,
    ) -> Result<(), Refusal> {
        let agent = agents::Known::of(kind);
        let refused = self.clients.bots.is_bot(client) && prompt.is_some_and(|prompt| not_a_prompt(prompt, agent));
        if refused { Err(failed(NOT_A_PROMPT)) } else { Ok(()) }
    }

    pub(in crate::app::control) fn types_into(&self, pane: u64) -> Result<(), Refusal> {
        let term = self.projects.pane_by(pane).ok_or_else(|| crate::app::control::none("pane", pane))?;
        if !fronts_agent(&self.config, term) {
            return Err(runs_none(pane));
        }
        if term.agent.status() == Some(Status::Waiting) {
            return Err(asks(pane));
        }
        if term.dialog() {
            return Err(refused(&format!(
                "and the agent in pane {pane} shows a dialog, a panel or its shell mode instead of its input box"
            )));
        }
        if self.resume_holds(pane) {
            return Err(refused(&format!("and pane {pane} is starting or resuming its agent")));
        }
        Ok(())
    }

    pub(in crate::app) fn refuse_bots_enter(&mut self, key: u64, pane: u64) {
        let fronts = self.projects.pane_by(pane).is_some_and(|term| fronts_agent(&self.config, term));
        self.answer_read(key, |_| Err(if fronts { asks(pane) } else { runs_none(pane) }));
    }
}
