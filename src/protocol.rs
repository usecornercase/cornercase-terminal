use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::{DirBuilderExt, MetadataExt};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crossterm::event::Event;
use rustix::fs::{FlockOperation, flock};
use rustix::io::Errno;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::error::Error;
use crate::graphics::Support;
use crate::host_theme::HostTheme;
use crate::notify::Channel;
use crate::{pointer, process};

pub const VERSION: u32 = 3;
pub const SOCKET_ENV: &str = "CORNERCASE_SOCKET";
pub const NESTED_ENV: &str = "CORNERCASE";
pub const RESTARTED_ENV: &str = "CORNERCASE_RESTARTED";
const MAX_MESSAGE: usize = 64 * 1024 * 1024;

#[derive(Debug, Serialize, Deserialize)]
pub enum ClientMessage {
    KillServer,
    Hello(Box<Hello>),
    Event(Event),
    Restart,
    Request(String),
    Graphics(Support),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Hello {
    pub version: u32,
    pub build: String,
    pub cols: u16,
    pub rows: u16,
    pub theme: HostTheme,
    pub notify: Channel,
    pub terminal: Option<String>,
    pub pointer: Option<pointer::Terminal>,
    pub graphics: Support,
    pub probe: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ServerMessage {
    Rejected(String),
    Frame(Vec<u8>),
    Detached,
    Shutdown,
    Restart(PathBuf),
    Response(String),
}

pub fn send<T: Serialize>(w: &mut impl Write, msg: &T) -> io::Result<()> {
    let body = postcard::to_stdvec(msg).map_err(io::Error::other)?;
    let len = u32::try_from(body.len()).map_err(io::Error::other)?;
    let mut frame = Vec::with_capacity(4 + body.len());
    frame.extend_from_slice(&len.to_le_bytes());
    frame.extend_from_slice(&body);
    w.write_all(&frame)?;
    w.flush()
}

pub fn recv<T: DeserializeOwned>(r: &mut impl Read) -> io::Result<Option<T>> {
    let mut len = [0u8; 4];
    match r.read_exact(&mut len) {
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        result => result?,
    }
    let len = usize::try_from(u32::from_le_bytes(len)).map_err(io::Error::other)?;
    if len > MAX_MESSAGE {
        return Err(io::Error::new(io::ErrorKind::InvalidData, format!("message of {len} bytes is too big")));
    }
    let mut body = vec![0; len];
    r.read_exact(&mut body)?;
    postcard::from_bytes(&body).map(Some).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

pub fn socket_path() -> PathBuf {
    if let Some(path) = std::env::var_os(SOCKET_ENV) {
        return path.into();
    }
    let dir = std::env::var_os("XDG_RUNTIME_DIR").map_or_else(
        || std::env::temp_dir().join(format!("cornercase-{}", rustix::process::getuid().as_raw())),
        |dir| PathBuf::from(dir).join("cornercase"),
    );
    dir.join("server.sock")
}

pub fn lock_path(socket: &Path) -> PathBuf {
    socket.with_extension("lock")
}

pub fn try_lock(socket: &Path) -> io::Result<Option<File>> {
    let file = OpenOptions::new().create(true).write(true).truncate(false).open(lock_path(socket))?;
    match flock(&file, FlockOperation::NonBlockingLockExclusive) {
        Ok(()) => Ok(Some(file)),
        Err(Errno::WOULDBLOCK) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn check_socket_dir(socket: &Path) -> Result<(), Error> {
    let dir = match socket.parent().filter(|dir| !dir.as_os_str().is_empty()) {
        Some(dir) => dir.to_path_buf(),
        None => std::env::current_dir()?,
    };
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&dir)?;
    let meta = std::fs::symlink_metadata(&dir)?;
    let kind = meta.file_type();
    match unsafe_dir_reason(kind.is_symlink(), kind.is_dir(), meta.uid(), meta.mode(), own_uid()) {
        Some(reason) => {
            Err(Error::UnsafeSocketDir { path: dir, reason, custom: std::env::var_os(SOCKET_ENV).is_some() })
        }
        None => Ok(()),
    }
}

fn unsafe_dir_reason(is_symlink: bool, is_dir: bool, owner: u32, mode: u32, uid: u32) -> Option<&'static str> {
    if is_symlink {
        Some("is a symbolic link")
    } else if !is_dir {
        Some("is not a folder")
    } else if owner != uid {
        Some("belongs to another user")
    } else if mode & 0o022 != 0 {
        Some("can be written by other users")
    } else {
        None
    }
}

pub fn own_uid() -> u32 {
    rustix::process::getuid().as_raw()
}

pub fn check_peer(stream: &UnixStream, uid: u32) -> Result<(), Error> {
    let peer = process::peer_uid(stream)?;
    if peer == uid { Ok(()) } else { Err(Error::OtherUser(peer)) }
}

pub fn build_id() -> String {
    std::env::current_exe()
        .and_then(|exe| exe.metadata())
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or_else(|| "unknown".into(), |since| since.as_nanos().to_string())
}

#[cfg(test)]
mod tests;
