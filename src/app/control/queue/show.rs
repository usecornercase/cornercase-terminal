use super::{Handled, Stage, Waits};
use crate::agents::context::Pointer;
use crate::app::App;
use crate::app::control::json;
use crate::control::contract::{Code, Refusal};
use crate::control::{self, MessageInfo, QueueShow, QueueShown, ResultInfo};
use crate::error::Result;
use crate::queue::conversation::{Message, Talk};
use crate::queue::outcome::Outcome;
use crate::queue::{self, Listed};

fn result_info(outcome: &Outcome, conversation: bool) -> ResultInfo {
    ResultInfo {
        at: outcome.at,
        text: outcome.text.clone(),
        cut: outcome.cut,
        written: outcome.written.clone(),
        agent: outcome.agent.clone(),
        branch: outcome.branch.clone(),
        pull_request: outcome.pull_request.clone(),
        conversation,
    }
}

fn message_info(message: &Message) -> MessageInfo {
    MessageInfo {
        role: message.role.name().into(),
        text: message.text.clone(),
        written: message.written.clone(),
        cut: message.cut,
    }
}

impl App {
    pub(in crate::app) fn queue_shown(&self, id: u64) -> std::result::Result<QueueShown, Refusal> {
        let entry =
            self.panels.queued.queue().entry(id).ok_or_else(|| Refusal::new(Code::NotFound, queue::missing(id)))?;
        let readable = entry.result.as_ref().and_then(|result| result.record.as_ref()).is_some_and(Pointer::readable);
        Ok(QueueShown {
            entry: Listed::from(entry),
            result: entry.result.as_ref().map(|result| result_info(result, readable)),
            messages: None,
            earlier: false,
            note: queue::outcome::note(entry, readable),
        })
    }

    pub(in crate::app::control) fn queue_show_request(&mut self, client: u64, show: &QueueShow) -> Handled {
        let mut shown = self.queue_shown(show.id)?;
        if !show.conversation {
            return Ok(Some(json(&shown)));
        }
        let Some(pointer) = self.readable_record(show.id) else {
            shown.note.get_or_insert_with(|| "this entry has no conversation to read".into());
            return Ok(Some(json(&shown)));
        };
        let key = self.clients.requests.key();
        let waits = Waits { done: control::Done::default(), stage: Stage::Reading, timeout: None };
        self.clients.requests.push(client, key, waits);
        self.read_conversation(show.id, &pointer, crate::app::queue_results::Reader::Request(key));
        Ok(None)
    }

    pub(in crate::app) fn queue_conversation_answered(&mut self, key: u64, entry: u64, result: Result<Talk>) {
        self.answer_read(key, |app| {
            let mut shown = app.queue_shown(entry)?;
            match result {
                Ok(talk) => {
                    shown.messages = Some(talk.messages.iter().map(message_info).collect());
                    shown.earlier = talk.earlier;
                }
                Err(error) => shown.note = Some(format!("cannot read the conversation: {error}")),
            }
            Ok(json(&shown))
        });
    }
}
