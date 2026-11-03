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
    use std::os::windows::io::{FromRawHandle, IntoRawHandle};

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

    pub(crate) fn redirect_stderr(file: &File) -> io::Result<()> {
        let handle = file.try_clone()?.into_raw_handle();
        if unsafe { SetStdHandle(STD_ERROR_HANDLE, handle) } == FALSE {
            let error = io::Error::last_os_error();
            drop(unsafe { File::from_raw_handle(handle) });
            return Err(error);
        }
        Ok(())
    }
}
