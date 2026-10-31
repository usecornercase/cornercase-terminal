use std::io::BufReader;
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::sync::Arc;

use serde_json::Value;

use super::{Ends, Line};
use crate::control::start_server::Started;
use crate::control::{Report, Response, Snapshot};
use crate::protocol::{self, ClientMessage, ServerMessage};

struct Pipe {
    ours: UnixStream,
    why: Option<String>,
}

impl Ends for Pipe {
    fn cut(&self) {
        let _ = self.ours.shutdown(Shutdown::Both);
    }

    fn why(&self) -> Option<String> {
        self.why.clone()
    }
}

pub(crate) fn line(release: &str, why: Option<&str>) -> (Line, UnixStream) {
    let (ours, theirs) = UnixStream::pair().expect("a socket pair");
    let reader = BufReader::new(ours.try_clone().expect("a reader"));
    let writer = ours.try_clone().expect("a writer");
    let ends = Arc::new(Pipe { ours, why: why.map(str::to_string) });
    (Line { release: release.into(), reader: Box::new(reader), writer: Box::new(writer), ends }, theirs)
}

pub(crate) fn request(theirs: &mut UnixStream) -> Value {
    match protocol::recv::<ClientMessage>(theirs) {
        Ok(Some(ClientMessage::Request(text))) => serde_json::from_str(&text).expect("a JSON request"),
        other => panic!("not a request: {other:?}"),
    }
}

pub(crate) fn respond(theirs: &mut UnixStream, response: &Response) {
    let text = serde_json::to_string(response).expect("an answer");
    protocol::send(theirs, &ServerMessage::Response(text)).expect("answer the link");
}

pub(crate) fn answer(theirs: &mut UnixStream, boot: &str, version: &str) {
    request(theirs);
    let started = Started { boot: boot.into(), version: version.into(), ..Started::default() };
    respond(theirs, &Response::Ok(serde_json::to_value(started).expect("a JSON answer")));
}

pub(crate) fn serve(theirs: &mut UnixStream, revision: u64, report: Report) {
    let line = Snapshot { revision, report }.streamed().expect("a snapshot");
    protocol::send(theirs, &ServerMessage::Response(line)).expect("send a snapshot to the link");
}

pub(crate) fn ended(theirs: &mut UnixStream) -> bool {
    matches!(protocol::recv::<ClientMessage>(theirs), Ok(None))
}
