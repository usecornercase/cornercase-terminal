use crate::cli::{print_json, print_out};
use crate::client::{answer, ask};
use crate::control::{Command, ForumServer, ForumServerInfo};
use crate::error::Result;

pub(super) fn pick(id: String, as_json: bool) -> Result<()> {
    let value = ask("forum-server", Command::ForumServer(ForumServer { id }))?;
    if as_json {
        return print_json(&value);
    }
    print_out(&picked(&answer(value)?))
}

fn picked(server: &ForumServerInfo) -> String {
    format!("the forum goes in {}; `cornercase status --app` says when it is ready\n", server.name)
}

#[cfg(test)]
mod tests;
