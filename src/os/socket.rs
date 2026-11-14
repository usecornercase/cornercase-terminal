#[cfg(windows)]
pub(crate) use imp::peer_uid;
pub(crate) use imp::{UnixListener, UnixStream, bind_without_listening, peer_pid};

#[cfg(unix)]
mod imp {
    use std::io;
    use std::os::unix::net::UnixDatagram;
    use std::path::Path;

    pub(crate) use std::os::unix::net::{UnixListener, UnixStream};

    pub(crate) fn bind_without_listening(path: &Path) -> io::Result<()> {
        UnixDatagram::bind(path).map(drop)
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn peer_pid(stream: &UnixStream) -> io::Result<i32> {
        let credentials = rustix::net::sockopt::socket_peercred(stream)?;
        Ok(credentials.pid.as_raw_nonzero().get())
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn peer_pid(stream: &UnixStream) -> io::Result<i32> {
        use std::os::fd::AsRawFd;

        let mut pid: libc::pid_t = 0;
        let mut size = libc::socklen_t::try_from(size_of::<libc::pid_t>()).unwrap_or(4);
        let at = (&raw mut pid).cast();
        let read =
            unsafe { libc::getsockopt(stream.as_raw_fd(), libc::SOL_LOCAL, libc::LOCAL_PEERPID, at, &raw mut size) };
        if read == 0 { Ok(pid) } else { Err(io::Error::last_os_error()) }
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    pub(crate) fn peer_pid(_stream: &UnixStream) -> io::Result<i32> {
        Err(io::ErrorKind::Unsupported.into())
    }
}

#[cfg(windows)]
mod windows;

#[cfg(windows)]
use windows as imp;

#[cfg(test)]
mod tests;
