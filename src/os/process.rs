pub(crate) use imp::{
    CommandExt, ExitStatusExt, Pid, Signal, alive, kill_process, kill_process_group, on_stop_signal, setsid,
    spawn_detached,
};

#[cfg(unix)]
mod imp {
    use std::io;
    use std::process::{Child, Command};
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

    pub(crate) fn spawn_detached(command: &mut Command) -> io::Result<Child> {
        command.spawn()
    }
}

#[cfg(windows)]
mod windows;

#[cfg(windows)]
use windows as imp;
