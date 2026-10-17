use std::io::{self};
use std::os::unix::net::UnixStream;
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::control::{self, Request, Response};
use crate::error::{Error, Result};
use crate::protocol::{self, ClientMessage, ServerMessage};
use crate::{paths, update::restart};

pub(crate) fn connect() -> Result<UnixStream> {
    let path = paths::socket();
    protocol::check_socket_dir(&path)?;
    let stream = UnixStream::connect(&path).map_err(|_| Error::NoServer)?;
    protocol::check_peer(&stream, protocol::own_uid())?;
    Ok(stream)
}

pub(crate) fn ask(name: &'static str, command: control::Command) -> Result<Value> {
    ask_on(connect()?, name, command)
}

pub(crate) fn ask_starting(name: &'static str, command: control::Command) -> Result<Value> {
    let path = paths::socket();
    protocol::check_socket_dir(&path)?;
    let stream = super::connect_or_start(&path)?;
    protocol::check_peer(&stream, protocol::own_uid())?;
    ask_on(stream, name, command)
}

fn ask_on(mut stream: UnixStream, name: &'static str, command: control::Command) -> Result<Value> {
    request(&mut stream, command)?;
    loop {
        match protocol::recv::<ServerMessage>(&mut stream) {
            Ok(Some(ServerMessage::Response(text))) => return answered(&text),
            Ok(Some(ServerMessage::Rejected(_))) => return Err(Error::OldServer(name)),
            Err(e) if e.kind() == io::ErrorKind::InvalidData => return Err(Error::OldServer(name)),
            Ok(Some(ServerMessage::Frame(_) | ServerMessage::Detached)) => {}
            Ok(Some(ServerMessage::Shutdown | ServerMessage::Restart(_)) | None) | Err(_) => {
                return Err(Error::ServerGone);
            }
        }
    }
}

pub(crate) fn request(stream: &mut UnixStream, command: control::Command) -> Result<()> {
    let caller = std::env::var(control::PANE_ENV).ok().and_then(|id| id.parse().ok());
    let server = std::env::var(control::SERVER_ENV).ok();
    let request = serde_json::to_string(&Request { caller, server, contract: None, boot: None, command })
        .map_err(|e| Error::Control(e.to_string()))?;
    protocol::send(stream, &ClientMessage::Request(request))?;
    Ok(())
}

pub(crate) fn answered(text: &str) -> Result<Value> {
    match serde_json::from_str(text) {
        Ok(Response::Ok(value)) => Ok(value),
        Ok(Response::Error(message)) => Err(Error::Control(message)),
        Err(e) => Err(Error::Control(format!("cannot read the server's answer: {e}"))),
    }
}

pub(crate) fn follow(
    name: &'static str,
    command: control::Command,
    mut each: impl FnMut(Value) -> Result<bool>,
) -> Result<()> {
    let mut stream = connect()?;
    let reader = stream.try_clone()?;
    ask_on(reader, name, command)?;
    loop {
        match protocol::recv::<ServerMessage>(&mut stream) {
            Ok(Some(ServerMessage::Response(text))) => {
                if each(answered(&text)?)? {
                    return Ok(());
                }
            }
            Ok(Some(ServerMessage::Detached | ServerMessage::Shutdown | ServerMessage::Restart(_))) => return Ok(()),
            Ok(Some(ServerMessage::Frame(_) | ServerMessage::Rejected(_))) => {}
            Ok(None) | Err(_) => return Err(Error::Control(LOST.into())),
        }
    }
}

pub(crate) fn answer<T: DeserializeOwned>(value: Value) -> Result<T> {
    serde_json::from_value(value).map_err(|e| Error::Control(format!("cannot read the server's answer: {e}")))
}

pub(crate) fn report_now() -> Option<control::Report> {
    let stream = connect().ok()?;
    stream.set_read_timeout(Some(STATUS_TIMEOUT)).ok()?;
    let value = ask_on(stream, "status", control::Command::Status(control::Status {})).ok()?;
    answer(value).ok()
}

pub(crate) fn running_now() -> Option<Vec<restart::Running>> {
    report_now().map(|report| restart::from_report(&report))
}
const STATUS_TIMEOUT: Duration = Duration::from_secs(1);
const LOST: &str = "the cornercase server closed the connection without stopping: it crashed, or this command \
    fell too far behind reading its events";

#[cfg(test)]
mod tests;
