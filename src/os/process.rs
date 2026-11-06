use std::io;
use std::ops::{Deref, DerefMut};
use std::process::{Child, Command};

#[cfg(unix)]
pub(crate) use imp::kill_process_group;
pub(crate) use imp::{
    CommandExt, ExitStatusExt, Pid, Signal, alive, askable, kill_process, launch_folder, leave_launch_folder,
    on_end_signal, on_stop_signal, setsid, spawn_detached,
};

pub(crate) struct Grouped {
    child: Child,
    job: imp::Job,
}

impl Grouped {
    pub(crate) fn spawn(command: &mut Command) -> io::Result<Self> {
        let child = command.process_group(0).spawn()?;
        let job = imp::Job::of(&child);
        Ok(Self { child, job })
    }

    pub(crate) fn signal(&self, signal: Signal) -> bool {
        self.job.signal(signal)
    }
}

impl Deref for Grouped {
    type Target = Child;

    fn deref(&self) -> &Child {
        &self.child
    }
}

impl DerefMut for Grouped {
    fn deref_mut(&mut self) -> &mut Child {
        &mut self.child
    }
}

#[cfg(unix)]
mod imp {
    use std::io;
    use std::path::{Path, PathBuf};
    use std::process::{Child, Command};
    use std::thread;

    use rustix::io::Errno;
    use rustix::process::test_kill_process;
    use signal_hook::consts::{SIGHUP, SIGINT, SIGQUIT, SIGTERM};
    use signal_hook::iterator::Signals;

    pub(crate) use rustix::process::{Pid, Signal, kill_process, kill_process_group, setsid};
    pub(crate) use std::os::unix::process::{CommandExt, ExitStatusExt};

    pub(crate) fn alive(pid: i32) -> bool {
        Pid::from_raw(pid).is_some_and(|pid| !matches!(test_kill_process(pid), Err(Errno::SRCH)))
    }

    pub(crate) fn askable(_pid: i32) -> bool {
        true
    }

    pub(crate) struct Job(Option<Pid>);

    impl Job {
        pub(crate) fn of(child: &Child) -> Self {
            Self(i32::try_from(child.id()).ok().and_then(Pid::from_raw))
        }

        pub(crate) fn signal(&self, signal: Signal) -> bool {
            self.0.is_some_and(|group| kill_process_group(group, signal).is_ok())
        }
    }

    pub(crate) fn on_stop_signal(stop: impl FnOnce() + Send + 'static) -> io::Result<()> {
        on_signals(&[SIGTERM, SIGINT, SIGHUP], |signal| signal != SIGHUP, stop)
    }

    pub(crate) fn on_end_signal(stop: impl FnOnce() + Send + 'static) -> io::Result<()> {
        on_signals(&[SIGTERM, SIGINT, SIGHUP, SIGQUIT], |_| true, stop)
    }

    fn on_signals(
        signals: &[i32],
        ends: impl FnMut(i32) -> bool + Send + 'static,
        stop: impl FnOnce() + Send + 'static,
    ) -> io::Result<()> {
        let mut signals = Signals::new(signals)?;
        thread::spawn(move || {
            if signals.forever().any(ends) {
                stop();
            }
        });
        Ok(())
    }

    pub(crate) fn spawn_detached(command: &mut Command) -> io::Result<Child> {
        command.spawn()
    }

    #[expect(clippy::unnecessary_wraps, reason = "it keeps the Windows signature, where moving can fail")]
    pub(crate) fn leave_launch_folder(_to: &Path) -> io::Result<()> {
        Ok(())
    }

    pub(crate) fn launch_folder() -> Option<PathBuf> {
        std::env::current_dir().ok()
    }
}

#[cfg(windows)]
mod windows;

#[cfg(windows)]
use windows as imp;
