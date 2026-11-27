use super::talk::{CANNOT_READ, Reading, unfound};
use super::{Asker, Handled, json};
use crate::agents::context::{self, Slice};
use crate::app::{App, AppEvent};
use crate::control::messages::{ReadMessages, Thread};
use crate::error;

impl App {
    pub(super) fn read_messages_request(&mut self, client: u64, asker: Asker, read: &ReadMessages) -> Handled {
        let pane = self.target(asker, read.pane, read.tab)?;
        if read.before.is_some() && read.after.is_some() {
            return Err("give --before or --after, not both".into());
        }
        let (term, agent) = self.reading_agent(pane, CANNOT_READ)?;
        let record = term.context.record_for(agent).ok_or_else(|| unfound(pane, agent))?;
        let conversation = term.context.conversation().map(str::to_string);
        let slice = Slice { last: read.last, before: read.before, after: read.after };
        self.read_record(
            Reading { client, pane, agent },
            "messages",
            move || {
                let mut read = record.page(slice)?;
                read.conversation = conversation;
                Ok(Some(read))
            },
            |request, result| AppEvent::Thread { request, result },
        );
        Ok(None)
    }

    pub(in crate::app) fn thread_read(&mut self, key: u64, result: error::Result<Option<context::Page>>) {
        self.read_answered(key, result, |pane, agent, result| match result {
            Ok(read) => Ok(json(&Thread::of(pane, agent, read.unwrap_or_default()))),
            Err(e) => Err(format!("cannot read the conversation of the {agent} agent in pane {pane}: {e}")),
        });
    }
}
