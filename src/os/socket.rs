pub(crate) use imp::{UnixListener, UnixStream};

#[cfg(unix)]
mod imp {
    pub(crate) use std::os::unix::net::{UnixListener, UnixStream};
}

#[cfg(not(unix))]
mod imp {
    use std::convert::Infallible;
    use std::io::{self, Read, Write};
    use std::iter::Empty;
    use std::net::Shutdown;
    use std::path::Path;
    use std::time::Duration;

    #[derive(Debug)]
    pub(crate) struct UnixStream(Infallible);

    impl UnixStream {
        pub(crate) fn connect(_path: impl AsRef<Path>) -> io::Result<Self> {
            Err(io::ErrorKind::Unsupported.into())
        }

        pub(crate) fn try_clone(&self) -> io::Result<Self> {
            match self.0 {}
        }

        pub(crate) fn set_read_timeout(&self, _timeout: Option<Duration>) -> io::Result<()> {
            match self.0 {}
        }

        pub(crate) fn set_write_timeout(&self, _timeout: Option<Duration>) -> io::Result<()> {
            match self.0 {}
        }

        pub(crate) fn shutdown(&self, _how: Shutdown) -> io::Result<()> {
            match self.0 {}
        }
    }

    impl Read for UnixStream {
        fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
            match self.0 {}
        }
    }

    impl Write for UnixStream {
        fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
            match self.0 {}
        }

        fn flush(&mut self) -> io::Result<()> {
            match self.0 {}
        }
    }

    #[derive(Debug)]
    pub(crate) struct UnixListener(Infallible);

    impl UnixListener {
        pub(crate) fn bind(_path: impl AsRef<Path>) -> io::Result<Self> {
            Err(io::ErrorKind::Unsupported.into())
        }

        pub(crate) fn incoming(&self) -> Empty<io::Result<UnixStream>> {
            match self.0 {}
        }
    }
}
