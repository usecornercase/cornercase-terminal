pub(crate) use imp::{AsFd, BorrowedFd, OwnedFd, redirect_stderr, stdin, stdout};

#[cfg(unix)]
mod imp {
    use std::fs::File;
    use std::io;

    pub(crate) use rustix::stdio::{stdin, stdout};
    pub(crate) use std::os::fd::{AsFd, BorrowedFd, OwnedFd};

    pub(crate) fn redirect_stderr(file: &File) -> io::Result<()> {
        rustix::stdio::dup2_stderr(file)?;
        Ok(())
    }
}

#[cfg(windows)]
mod imp {
    use std::convert::Infallible;
    use std::fs::File;
    use std::io;
    use std::marker::PhantomData;
    use std::os::windows::io::{AsRawHandle, OwnedHandle};

    use parking_lot::Mutex;
    use windows_sys::Win32::Foundation::FALSE;
    use windows_sys::Win32::System::Console::{STD_ERROR_HANDLE, SetStdHandle};

    #[derive(Debug, Clone, Copy)]
    pub(crate) struct BorrowedFd<'fd>(PhantomData<&'fd ()>);

    impl BorrowedFd<'_> {
        #[expect(clippy::trivially_copy_pass_by_ref, clippy::unused_self, reason = "it keeps std's signature")]
        pub(crate) fn try_clone_to_owned(&self) -> io::Result<OwnedFd> {
            Err(io::ErrorKind::Unsupported.into())
        }
    }

    #[derive(Debug)]
    pub(crate) struct OwnedFd(Infallible);

    pub(crate) trait AsFd {
        fn as_fd(&self) -> BorrowedFd<'_>;
    }

    impl<T: ?Sized> AsFd for T {
        fn as_fd(&self) -> BorrowedFd<'_> {
            BorrowedFd(PhantomData)
        }
    }

    pub(crate) fn stdin() -> BorrowedFd<'static> {
        BorrowedFd(PhantomData)
    }

    pub(crate) fn stdout() -> BorrowedFd<'static> {
        BorrowedFd(PhantomData)
    }

    #[derive(Debug)]
    pub(super) struct Taken {
        current: Option<OwnedHandle>,
        previous: Option<OwnedHandle>,
    }

    impl Taken {
        pub(super) const fn new() -> Self {
            Self { current: None, previous: None }
        }

        pub(super) fn take(&mut self, handle: OwnedHandle) -> Option<OwnedHandle> {
            let closing = self.previous.take();
            self.previous = self.current.replace(handle);
            closing
        }
    }

    static TAKEN: Mutex<Taken> = Mutex::new(Taken::new());

    pub(crate) fn redirect_stderr(file: &File) -> io::Result<()> {
        let handle = OwnedHandle::from(file.try_clone()?);
        if unsafe { SetStdHandle(STD_ERROR_HANDLE, handle.as_raw_handle()) } == FALSE {
            return Err(io::Error::last_os_error());
        }
        let closing = TAKEN.lock().take(handle);
        drop(closing);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
