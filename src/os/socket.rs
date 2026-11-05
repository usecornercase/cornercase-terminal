#[cfg(windows)]
pub(crate) use imp::peer_uid;
pub(crate) use imp::{UnixListener, UnixStream, bind_without_listening};

#[cfg(unix)]
mod imp {
    use std::io;
    use std::os::unix::net::UnixDatagram;
    use std::path::Path;

    pub(crate) use std::os::unix::net::{UnixListener, UnixStream};

    pub(crate) fn bind_without_listening(path: &Path) -> io::Result<()> {
        UnixDatagram::bind(path).map(drop)
    }
}

#[cfg(windows)]
mod windows;

#[cfg(windows)]
use windows as imp;
