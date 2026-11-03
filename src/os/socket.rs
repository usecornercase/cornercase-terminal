#[cfg(windows)]
pub(crate) use imp::peer_uid;
pub(crate) use imp::{UnixListener, UnixStream};

#[cfg(unix)]
mod imp {
    pub(crate) use std::os::unix::net::{UnixListener, UnixStream};
}

#[cfg(windows)]
mod windows;

#[cfg(windows)]
use windows as imp;
