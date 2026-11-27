use serde_json::Value;

use super::talk::{Reading, cannot_read, unfound};
use super::{Asker, Handled, json};
use crate::agents::activity::Status;
use crate::agents::context;
use crate::app::{App, AppEvent};
use crate::control::pending::{Asking, PendingQuestion};
use crate::error;

const ONLY_CLAUDE: &str = "cornercase reads what an agent asks only from Claude Code's records";

impl App {
    pub(super) fn pending_question_request(
        &mut self,
        client: u64,
        asker: Asker,
        question: &PendingQuestion,
    ) -> Handled {
        let pane = self.target(asker, question.pane, question.tab)?;
        let (term, agent) = self.reading_agent(pane, ONLY_CLAUDE)?;
        if !agent.tells_awaited() {
            return Err(cannot_read(pane, &format!("runs {}", agent.name()), ONLY_CLAUDE));
        }
        if term.agent.status() != Some(Status::Waiting) {
            return Ok(Some(Value::Null));
        }
        let record = term.context.record_for(agent).ok_or_else(|| unfound(pane, agent))?;
        let reading = Reading { client, pane, agent };
        self.read_record(
            reading,
            "pending question",
            move || record.awaited(),
            |request, result| AppEvent::Awaited { request, result },
        );
        Ok(None)
    }

    pub(in crate::app) fn awaited_read(&mut self, key: u64, result: error::Result<Option<context::Awaited>>) {
        self.read_answered(key, result, |pane, agent, result| match result {
            Ok(Some(awaited)) => Ok(json(&Asking { pane, agent, awaited })),
            Ok(None) => Ok(Value::Null),
            Err(e) => Err(format!("cannot read what the {agent} agent in pane {pane} asks: {e}")),
        });
    }
}
