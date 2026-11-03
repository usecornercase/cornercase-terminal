use std::io::{self, Read, Write};
use std::iter;
use std::net::Shutdown;
use std::path::Path;
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::Networking::WinSock::{
    self as winsock, AF_UNIX, INVALID_SOCKET, SD_BOTH, SD_RECEIVE, SD_SEND, SIO_AF_UNIX_GETPEERPID, SO_RCVTIMEO,
    SO_SNDTIMEO, SOCK_STREAM, SOCKADDR, SOCKADDR_UN, SOCKET, SOCKET_ERROR, SOL_SOCKET, WSA_FLAG_NO_HANDLE_INHERIT,
    WSA_FLAG_OVERLAPPED, WSADATA,
};
use windows_sys::Win32::System::IO::CancelIoEx;

use crate::os::fs::process_uid;

const WINSOCK_2_2: u16 = 0x0202;
const BACKLOG: i32 = 128;

#[derive(Debug)]
struct Socket {
    raw: SOCKET,
    unread: AtomicBool,
}

impl Socket {
    fn new() -> io::Result<Self> {
        started()?;
        let flags = WSA_FLAG_OVERLAPPED | WSA_FLAG_NO_HANDLE_INHERIT;
        Self::own(unsafe { winsock::WSASocketW(i32::from(AF_UNIX), SOCK_STREAM, 0, null(), 0, flags) })
    }

    fn own(raw: SOCKET) -> io::Result<Self> {
        if raw == INVALID_SOCKET {
            return Err(last_error());
        }
        Ok(Self { raw, unread: AtomicBool::new(false) })
    }

    fn set_timeout(&self, option: i32, timeout: Option<Duration>) -> io::Result<()> {
        let millis = millis(timeout)?;
        let length = length_of::<u32>();
        checked(unsafe { winsock::setsockopt(self.raw, SOL_SOCKET, option, (&raw const millis).cast(), length) })
    }
}

impl Drop for Socket {
    fn drop(&mut self) {
        unsafe { winsock::closesocket(self.raw) };
    }
}

fn started() -> io::Result<()> {
    static STARTED: OnceLock<i32> = OnceLock::new();
    let code = *STARTED.get_or_init(|| {
        let mut data = WSADATA::default();
        unsafe { winsock::WSAStartup(WINSOCK_2_2, &raw mut data) }
    });
    if code == 0 { Ok(()) } else { Err(io::Error::from_raw_os_error(code)) }
}

fn millis(timeout: Option<Duration>) -> io::Result<u32> {
    match timeout {
        None => Ok(0),
        Some(timeout) if timeout.is_zero() => {
            Err(io::Error::new(io::ErrorKind::InvalidInput, "cannot set a 0 duration timeout"))
        }
        Some(timeout) => Ok(u32::try_from(timeout.as_millis().max(1)).unwrap_or(u32::MAX)),
    }
}

fn address(path: &Path) -> io::Result<(SOCKADDR_UN, i32)> {
    let invalid = |reason| io::Error::new(io::ErrorKind::InvalidInput, reason);
    let bytes = path.to_str().ok_or_else(|| invalid("the socket path is not valid Unicode"))?.as_bytes();
    let mut address = SOCKADDR_UN { sun_family: AF_UNIX, ..SOCKADDR_UN::default() };
    if bytes.contains(&0) {
        return Err(invalid("paths must not contain interior null bytes"));
    }
    if bytes.len() >= address.sun_path.len() {
        return Err(invalid("path must be shorter than SUN_LEN"));
    }
    for (slot, byte) in address.sun_path.iter_mut().zip(bytes) {
        *slot = byte.cast_signed();
    }
    let length = length_of::<u16>() + i32::try_from(bytes.len()).map_err(io::Error::other)? + 1;
    Ok((address, length))
}

fn length_of<T>() -> i32 {
    i32::try_from(size_of::<T>()).unwrap_or(i32::MAX)
}

fn buffer_length(length: usize) -> i32 {
    i32::try_from(length).unwrap_or(i32::MAX)
}

fn checked(result: i32) -> io::Result<()> {
    if result == SOCKET_ERROR { Err(last_error()) } else { Ok(()) }
}

fn last_error() -> io::Error {
    io::Error::from_raw_os_error(unsafe { winsock::WSAGetLastError() })
}

#[derive(Debug)]
pub(crate) struct UnixStream(Arc<Socket>);

impl UnixStream {
    pub(crate) fn connect(path: impl AsRef<Path>) -> io::Result<Self> {
        let socket = Socket::new()?;
        let (address, length) = address(path.as_ref())?;
        checked(unsafe { winsock::connect(socket.raw, (&raw const address).cast::<SOCKADDR>(), length) })?;
        Ok(Self(Arc::new(socket)))
    }

    #[cfg(test)]
    pub(crate) fn pair() -> io::Result<(Self, Self)> {
        use std::sync::atomic::AtomicUsize;

        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("cornercase-pair-{}-{n}.sock", std::process::id()));
        let listener = UnixListener::bind(&path)?;
        let paired = Self::connect(&path).and_then(|ours| Ok((ours, listener.accept()?)));
        let _ = std::fs::remove_file(&path);
        paired
    }

    #[expect(clippy::unnecessary_wraps, reason = "it keeps std's signature, where cloning can fail")]
    pub(crate) fn try_clone(&self) -> io::Result<Self> {
        Ok(Self(Arc::clone(&self.0)))
    }

    pub(crate) fn set_read_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        self.0.set_timeout(SO_RCVTIMEO, timeout)
    }

    pub(crate) fn set_write_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        self.0.set_timeout(SO_SNDTIMEO, timeout)
    }

    pub(crate) fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        let how = match how {
            Shutdown::Read => SD_RECEIVE,
            Shutdown::Write => SD_SEND,
            Shutdown::Both => SD_BOTH,
        };
        if how != SD_SEND {
            self.0.unread.store(true, Ordering::SeqCst);
        }
        checked(unsafe { winsock::shutdown(self.0.raw, how) })?;
        if how != SD_SEND {
            unsafe { CancelIoEx(self.0.raw as HANDLE, null()) };
        }
        Ok(())
    }

    fn peer_pid(&self) -> io::Result<u32> {
        let mut pid = 0u32;
        let mut returned = 0u32;
        let size = length_of::<u32>().cast_unsigned();
        let out = (&raw mut pid).cast();
        checked(unsafe {
            winsock::WSAIoctl(
                self.0.raw,
                SIO_AF_UNIX_GETPEERPID,
                null(),
                0,
                out,
                size,
                &raw mut returned,
                null_mut(),
                None,
            )
        })?;
        Ok(pid)
    }
}

impl Read for UnixStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let read = unsafe { winsock::recv(self.0.raw, buf.as_mut_ptr(), buffer_length(buf.len()), 0) };
        match usize::try_from(read) {
            Ok(read) => Ok(read),
            Err(_) if self.0.unread.load(Ordering::SeqCst) => Ok(0),
            Err(_) => Err(last_error()),
        }
    }
}

impl Write for UnixStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let sent = unsafe { winsock::send(self.0.raw, buf.as_ptr(), buffer_length(buf.len()), 0) };
        usize::try_from(sent).map_err(|_| last_error())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(crate) fn peer_uid(stream: &UnixStream) -> io::Result<u32> {
    process_uid(stream.peer_pid()?)
}

#[derive(Debug)]
pub(crate) struct UnixListener(Socket);

impl UnixListener {
    pub(crate) fn bind(path: impl AsRef<Path>) -> io::Result<Self> {
        let socket = Socket::new()?;
        let (address, length) = address(path.as_ref())?;
        checked(unsafe { winsock::bind(socket.raw, (&raw const address).cast::<SOCKADDR>(), length) })?;
        checked(unsafe { winsock::listen(socket.raw, BACKLOG) })?;
        Ok(Self(socket))
    }

    pub(crate) fn incoming(&self) -> impl Iterator<Item = io::Result<UnixStream>> + '_ {
        iter::repeat_with(|| self.accept())
    }

    fn accept(&self) -> io::Result<UnixStream> {
        let raw = unsafe { winsock::accept(self.0.raw, null_mut(), null_mut()) };
        Socket::own(raw).map(|socket| UnixStream(Arc::new(socket)))
    }
}

#[cfg(test)]
mod tests;
