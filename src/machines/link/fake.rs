use std::io::BufReader;
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::sync::Arc;

use parking_lot::Mutex;
use serde_json::Value;

use super::{Connect, Ends, Line};
use crate::control::start_server::Started;
use crate::control::{Report, Response, Snapshot};
use crate::error::{Error, Result};
use crate::host::open::{Browse, Open, Opener};
use crate::host::sound::{Cue, Kind};
use crate::protocol::{self, ClientMessage, ServerMessage};
use crate::remote::Killer;

pub(crate) const REFUSED: &str = "ssh: connect to host down port 22: Connection refused";

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

pub(crate) fn refused() -> Error {
    Error::Ssh { host: "down".into(), reason: REFUSED.into() }
}

pub(crate) struct Once(pub(crate) Mutex<Option<Line>>);

impl Connect for Once {
    fn connect(&self, _: &Killer) -> Result<Line> {
        self.0.lock().take().ok_or_else(refused)
    }
}

pub(crate) struct Hanging;

impl Connect for Hanging {
    fn connect(&self, killer: &Killer) -> Result<Line> {
        let mut ssh = std::process::Command::new("/bin/sleep").arg("60").spawn().expect("run sleep");
        killer.arm(&ssh);
        let _ = ssh.wait();
        killer.disarm();
        Err(refused())
    }
}

pub(crate) fn window_messages() -> Vec<ServerMessage> {
    vec![
        ServerMessage::Frame(b"\x1b]52;c;cm0gLXJmIH4K\x07".to_vec()),
        ServerMessage::Rejected("go away".into()),
        ServerMessage::Sound(Cue { kind: Kind::NeedsYou, path: "/tmp/ding.wav".into() }),
        ServerMessage::Open(Open { opener: Opener::Default, path: "/etc/passwd".into(), line: None }),
        ServerMessage::Browse(Browse { ip: None, port: 80 }),
    ]
}
