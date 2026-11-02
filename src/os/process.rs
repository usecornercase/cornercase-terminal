pub(crate) use imp::{
    CommandExt, ExitStatusExt, Pid, Signal, alive, kill_process, kill_process_group, on_stop_signal, setsid,
};

#[cfg(unix)]
mod imp {
    use std::io;
    use std::thread;

    use rustix::io::Errno;
    use rustix::process::test_kill_process;
    use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};
    use signal_hook::iterator::Signals;

    pub(crate) use rustix::process::{Pid, Signal, kill_process, kill_process_group, setsid};
    pub(crate) use std::os::unix::process::{CommandExt, ExitStatusExt};

    pub(crate) fn alive(pid: i32) -> bool {
        Pid::from_raw(pid).is_some_and(|pid| !matches!(test_kill_process(pid), Err(Errno::SRCH)))
    }

    pub(crate) fn on_stop_signal(stop: impl FnOnce() + Send + 'static) -> io::Result<()> {
        let mut signals = Signals::new([SIGTERM, SIGINT, SIGHUP])?;
        thread::spawn(move || {
            if signals.forever().any(|signal| signal != SIGHUP) {
                stop();
            }
        });
        Ok(())
    }
}

#[cfg(not(unix))]
mod imp {
    use std::io;
    use std::process::{Command, ExitStatus};

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) struct Pid;

    impl Pid {
        pub(crate) fn from_raw(raw: i32) -> Option<Self> {
            (raw > 0).then_some(Self)
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) enum Signal {
        Hup,
        Kill,
        Term,
    }

    impl Signal {
        pub(crate) const HUP: Self = Self::Hup;
        pub(crate) const KILL: Self = Self::Kill;
        pub(crate) const TERM: Self = Self::Term;
    }

    pub(crate) trait CommandExt {
        fn process_group(&mut self, pgroup: i32) -> &mut Command;
        fn exec(&mut self) -> io::Error;
        #[cfg(test)]
        fn arg0(&mut self, arg: impl AsRef<std::ffi::OsStr>) -> &mut Command;
    }

    impl CommandExt for Command {
        fn process_group(&mut self, _pgroup: i32) -> &mut Command {
            self
        }

        fn exec(&mut self) -> io::Error {
            unsupported()
        }

        #[cfg(test)]
        fn arg0(&mut self, _arg: impl AsRef<std::ffi::OsStr>) -> &mut Command {
            self
        }
    }

    pub(crate) trait ExitStatusExt {
        fn signal(&self) -> Option<i32>;
    }

    impl ExitStatusExt for ExitStatus {
        fn signal(&self) -> Option<i32> {
            None
        }
    }

    fn unsupported() -> io::Error {
        io::ErrorKind::Unsupported.into()
    }

    pub(crate) fn kill_process(_pid: Pid, _signal: Signal) -> io::Result<()> {
        Err(unsupported())
    }

    pub(crate) fn kill_process_group(_group: Pid, _signal: Signal) -> io::Result<()> {
        Err(unsupported())
    }

    pub(crate) fn setsid() -> io::Result<Pid> {
        Err(unsupported())
    }

    pub(crate) fn alive(_pid: i32) -> bool {
        false
    }

    pub(crate) fn on_stop_signal(_stop: impl FnOnce() + Send + 'static) -> io::Result<()> {
        Err(unsupported())
    }
}
