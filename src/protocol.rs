use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};

use crossterm::event::Event;
use rustix::fs::{FlockOperation, flock};
use rustix::io::Errno;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::error::Error;
use crate::graphics::Support;
use crate::host::notify::Channel;
use crate::host::open::{Browse, Failed, Open, Opener};
use crate::host::sound::Cue;
use crate::host::theme::HostTheme;
use crate::paths;
use crate::{host::pointer, process};

pub(crate) const VERSION: u32 = 9;
pub const NESTED_ENV: &str = "CORNERCASE";
pub const REMOTE_ENV: &str = "CORNERCASE_REMOTE";
pub(crate) const RESTARTED_ENV: &str = "CORNERCASE_RESTARTED";
pub(crate) const TIED_ENV: &str = "CORNERCASE_TIED";
pub(crate) const BROWSER: &str = "browser";
const MAX_MESSAGE: usize = 64 * 1024 * 1024;

#[derive(Debug, Serialize, Deserialize)]
pub(crate) enum ClientMessage {
    KillServer,
    Hello(Box<Hello>),
    Event(Event),
    Restart,
    Request(String),
    Graphics(Support),
    Activated(String),
    OpenFailed(Failed),
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Hello {
    pub(crate) version: u32,
    pub(crate) build: String,
    pub(crate) cols: u16,
    pub(crate) rows: u16,
    pub(crate) theme: HostTheme,
    pub(crate) notify: Channel,
    pub(crate) terminal: Option<String>,
    pub(crate) pointer: Option<pointer::Terminal>,
    pub(crate) graphics: Support,
    pub(crate) probe: String,
    pub(crate) zone: String,
    pub(crate) openers: Vec<Opener>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) enum ServerMessage {
    Rejected(String),
    Frame(Vec<u8>),
    Detached,
    Shutdown,
    Restart(PathBuf),
    Response(String),
    Sound(Cue),
    Open(Open),
    Browse(Browse),
}

pub(crate) fn send<T: Serialize>(w: &mut impl Write, msg: &T) -> io::Result<()> {
    let body = postcard::to_stdvec(msg).map_err(io::Error::other)?;
    let len = u32::try_from(body.len()).map_err(io::Error::other)?;
    let mut frame = Vec::with_capacity(4 + body.len());
    frame.extend_from_slice(&len.to_le_bytes());
    frame.extend_from_slice(&body);
    w.write_all(&frame)?;
    w.flush()
}

pub(crate) fn recv<T: DeserializeOwned>(r: &mut impl Read) -> io::Result<Option<T>> {
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

#[cfg_attr(not(test), expect(dead_code, reason = "the machines' links skip such messages in the next commits"))]
pub(crate) fn undecoded(error: &io::Error) -> bool {
    matches!(error.get_ref(), Some(inner) if inner.is::<postcard::Error>())
}

pub(crate) fn try_lock(socket: &Path) -> io::Result<Option<File>> {
    let file = OpenOptions::new().create(true).write(true).truncate(false).open(paths::lock(socket))?;
    match flock(&file, FlockOperation::NonBlockingLockExclusive) {
        Ok(()) => Ok(Some(file)),
        Err(Errno::WOULDBLOCK) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Folder {
    Own,
    Given,
}

impl Folder {
    pub(crate) fn current() -> Self {
        if paths::custom_socket().is_some() { Self::Given } else { Self::Own }
    }
}

pub(crate) const MAX_SOCKET_PATH: usize = if cfg!(target_os = "macos") { 103 } else { 107 };

fn socket_folder(socket: &Path) -> io::Result<PathBuf> {
    match socket.parent().filter(|dir| !dir.as_os_str().is_empty()) {
        Some(dir) => Ok(dir.to_path_buf()),
        None => std::env::current_dir(),
    }
}

pub(crate) fn check_socket_dir(socket: &Path, folder: Folder) -> Result<(), Error> {
    let length = socket.as_os_str().len();
    if length > MAX_SOCKET_PATH {
        return Err(Error::SocketTooLong {
            path: socket.to_path_buf(),
            length,
            max: MAX_SOCKET_PATH,
            custom: folder == Folder::Given,
        });
    }
    let dir = socket_folder(socket)?;
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&dir)?;
    let meta = std::fs::symlink_metadata(&dir)?;
    let kind = meta.file_type();
    let reason = unsafe_dir_reason(kind.is_symlink(), kind.is_dir(), meta.uid(), meta.mode(), own_uid());
    let refuse = |reason| Error::UnsafeSocketDir { path: dir.clone(), reason, custom: folder == Folder::Given };
    let tighten = || std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
    match reason {
        Some("can be written by other users") if folder == Folder::Own => {
            tighten().map_err(|_| refuse("can be written by other users"))
        }
        Some(reason) => Err(refuse(reason)),
        None if folder == Folder::Own && meta.mode() & 0o077 != 0 => Ok(tighten()?),
        None => Ok(()),
    }
}

pub(crate) fn socket_dir_is_safe(socket: &Path) -> bool {
    let Ok(dir) = socket_folder(socket) else { return false };
    let Ok(meta) = std::fs::symlink_metadata(&dir) else { return false };
    let kind = meta.file_type();
    unsafe_dir_reason(kind.is_symlink(), kind.is_dir(), meta.uid(), meta.mode(), own_uid()).is_none()
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

pub(crate) fn check_peer(stream: &UnixStream, uid: u32) -> Result<(), Error> {
    let peer = process::peer_uid(stream)?;
    if peer == uid { Ok(()) } else { Err(Error::OtherUser(peer)) }
}

#[cfg(test)]
mod tests;
