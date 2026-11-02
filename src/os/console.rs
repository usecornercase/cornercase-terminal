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

#[cfg(not(unix))]
mod imp {
    use std::convert::Infallible;
    use std::fs::File;
    use std::io;
    use std::marker::PhantomData;

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

    pub(crate) fn redirect_stderr(_file: &File) -> io::Result<()> {
        Err(io::ErrorKind::Unsupported.into())
    }
}
