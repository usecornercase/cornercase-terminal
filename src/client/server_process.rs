use std::fs::{self, OpenOptions};
use std::io::{self};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::error::{Error, Result};
use crate::process::reap;
use crate::protocol::{self, ClientMessage, Folder, ServerMessage};
use crate::{control, log, paths};

pub(crate) fn kill_server() -> Result<bool> {
    Ok(stop_server(&ClientMessage::KillServer)?.is_some())
}

pub(crate) fn restart_server() -> Result<bool> {
    match stop_server(&ClientMessage::Restart)? {
        Some(true) => kill_server(),
        answer => Ok(answer.is_some()),
    }
}

fn stop_server(msg: &ClientMessage) -> Result<Option<bool>> {
    let Some(Found { socket, mut stream }) = find()? else { return Ok(None) };
    protocol::send(&mut stream, msg)?;
    let mut rejected = false;
    while let Ok(Some(answer)) = protocol::recv::<ServerMessage>(&mut stream) {
        rejected |= matches!(answer, ServerMessage::Rejected(_));
    }
    if !rejected {
        wait_for_exit(&socket);
    }
    Ok(Some(rejected))
}

pub(crate) fn wait_for_exit(path: &Path) {
    let deadline = Instant::now() + SERVER_EXIT_TIMEOUT;
    while matches!(protocol::try_lock(path), Ok(None)) && Instant::now() < deadline {
        thread::sleep(POLL);
    }
}

pub(crate) struct Found {
    pub(crate) socket: PathBuf,
    pub(crate) stream: UnixStream,
}

pub(crate) fn find() -> Result<Option<Found>> {
    let socket = paths::socket();
    protocol::check_socket_dir(&socket, Folder::current())?;
    find_among(&socket, &paths::earlier_sockets())
}

fn find_among(socket: &Path, earlier: &[PathBuf]) -> Result<Option<Found>> {
    let candidates = std::iter::once(socket).chain(
        earlier.iter().map(PathBuf::as_path).filter(|path| *path != socket && protocol::socket_dir_is_safe(path)),
    );
    for path in candidates {
        if let Ok(stream) = UnixStream::connect(path) {
            protocol::check_peer(&stream, protocol::own_uid())?;
            return Ok(Some(Found { socket: path.to_path_buf(), stream }));
        }
    }
    Ok(None)
}

pub(crate) fn running() -> Result<UnixStream> {
    find()?.map(|found| found.stream).ok_or(Error::NoServer)
}

pub(crate) fn find_or_start() -> Result<Found> {
    if let Some(found) = find()? {
        return Ok(found);
    }
    let socket = paths::socket();
    can_listen(&socket, Folder::current())?;
    let stream = start(&socket)?;
    protocol::check_peer(&stream, protocol::own_uid())?;
    Ok(Found { socket, stream })
}

fn can_listen(socket: &Path, folder: Folder) -> Result<()> {
    let listen_error = |cause| Error::Listen { path: socket.to_path_buf(), cause, custom: folder == Folder::Given };
    let Some(_lock) = protocol::try_lock(socket).map_err(listen_error)? else { return Ok(()) };
    let _ = fs::remove_file(socket);
    let bound = UnixListener::bind(socket).map_err(listen_error);
    drop(bound);
    let _ = fs::remove_file(socket);
    Ok(())
}

fn start(path: &Path) -> Result<UnixStream> {
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
            return Err(if matches!(protocol::try_lock(path), Ok(None)) {
                Error::ServerUnreachable(path.to_path_buf())
            } else {
                Error::ServerStart { log, source: None }
            });
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
        .env_remove(control::PANE_ENV)
        .env_remove(control::SERVER_ENV)
        .env_remove(protocol::REMOTE_ENV)
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log)
        .spawn()
}
const SERVER_START_TIMEOUT: Duration = Duration::from_secs(5);
const SERVER_EXIT_TIMEOUT: Duration = Duration::from_secs(5);
const POLL: Duration = Duration::from_millis(20);

#[cfg(test)]
mod tests;
