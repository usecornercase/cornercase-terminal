use std::fs::{self, OpenOptions};
use std::io::{self};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::error::{Error, Result};
use crate::protocol::{self, ClientMessage, ServerMessage};
use crate::{log, paths};

pub fn kill_server() -> Result<bool> {
    Ok(stop_server(&ClientMessage::KillServer)?.is_some())
}

pub fn restart_server() -> Result<bool> {
    match stop_server(&ClientMessage::Restart)? {
        Some(true) => kill_server(),
        answer => Ok(answer.is_some()),
    }
}

fn stop_server(msg: &ClientMessage) -> Result<Option<bool>> {
    let path = paths::socket();
    protocol::check_socket_dir(&path)?;
    let Ok(mut stream) = UnixStream::connect(&path) else { return Ok(None) };
    protocol::check_peer(&stream, protocol::own_uid())?;
    protocol::send(&mut stream, msg)?;
    let mut rejected = false;
    while let Ok(Some(answer)) = protocol::recv::<ServerMessage>(&mut stream) {
        rejected |= matches!(answer, ServerMessage::Rejected(_));
    }
    if !rejected {
        wait_for_exit(&path);
    }
    Ok(Some(rejected))
}

pub(crate) fn wait_for_exit(path: &Path) {
    let deadline = Instant::now() + SERVER_EXIT_TIMEOUT;
    while matches!(protocol::try_lock(path), Ok(None)) && Instant::now() < deadline {
        thread::sleep(POLL);
    }
}

pub(crate) fn connect_or_start(path: &Path) -> Result<UnixStream> {
    if let Ok(stream) = UnixStream::connect(path) {
        return Ok(stream);
    }
    let log = paths::log();
    let start_error = |e| Error::ServerStart { log: log.clone(), source: Some(e) };
    let deadline = Instant::now() + SERVER_START_TIMEOUT;
    let mut server = start_server(&log).map_err(start_error)?;
    loop {
        if let Ok(stream) = UnixStream::connect(path) {
            reap(server);
            return Ok(stream);
        }
        if Instant::now() >= deadline {
            reap(server);
            return Err(Error::ServerStart { log, source: None });
        }
        if matches!(server.try_wait(), Ok(Some(_))) {
            server = start_server(&log).map_err(start_error)?;
        }
        thread::sleep(POLL);
    }
}

fn start_server(log: &Path) -> io::Result<Child> {
    if let Some(dir) = log.parent() {
        fs::create_dir_all(dir)?;
    }
    let log = OpenOptions::new().create(true).append(true).mode(log::PRIVATE).open(log)?;
    log.set_permissions(fs::Permissions::from_mode(log::PRIVATE))?;
    Command::new(std::env::current_exe()?)
        .arg("server")
        .env_remove(protocol::RESTARTED_ENV)
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log)
        .spawn()
}

pub(crate) fn reap(mut child: Child) {
    thread::spawn(move || child.wait());
}

pub(crate) fn stop(mut child: Child) {
    let _ = child.kill();
    reap(child);
}
const SERVER_START_TIMEOUT: Duration = Duration::from_secs(5);
const SERVER_EXIT_TIMEOUT: Duration = Duration::from_secs(5);
const POLL: Duration = Duration::from_millis(20);
