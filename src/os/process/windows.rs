use std::io;
use std::os::windows::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::{self, Child, Command, ExitStatus};
use std::thread;

use parking_lot::Mutex;
use windows_sys::Win32::Foundation::{
    ERROR_ACCESS_DENIED, FALSE, HANDLE, HANDLE_FLAG_INHERIT, SetHandleInformation, TRUE,
};
use windows_sys::Win32::System::Console::{
    CTRL_BREAK_EVENT, CTRL_C_EVENT, CTRL_CLOSE_EVENT, CTRL_LOGOFF_EVENT, CTRL_SHUTDOWN_EVENT, GetConsoleWindow,
    GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, SetConsoleCtrlHandler,
};
use windows_sys::Win32::System::Threading::{CREATE_BREAKAWAY_FROM_JOB, CREATE_NO_WINDOW};
use windows_sys::core::BOOL;

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
        match run_in_place(self) {
            Ok(code) => process::exit(code),
            Err(error) => error,
        }
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

fn checked(done: BOOL) -> io::Result<()> {
    if done == FALSE { Err(io::Error::last_os_error()) } else { Ok(()) }
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

pub(crate) fn spawn_detached(command: &mut Command) -> io::Result<Child> {
    for which in [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
        let _ = not_inherited(unsafe { GetStdHandle(which) });
    }
    match command.creation_flags(CREATE_NO_WINDOW | CREATE_BREAKAWAY_FROM_JOB).spawn() {
        Err(error) if error.raw_os_error() == i32::try_from(ERROR_ACCESS_DENIED).ok() => {
            command.creation_flags(CREATE_NO_WINDOW).spawn()
        }
        spawned => spawned,
    }
}

static LAUNCH: Mutex<Option<PathBuf>> = Mutex::new(None);

pub(crate) fn leave_launch_folder(to: &Path) -> io::Result<()> {
    *LAUNCH.lock() = Some(std::env::current_dir()?);
    std::env::set_current_dir(to)
}

pub(crate) fn launch_folder() -> Option<PathBuf> {
    launch_in(&LAUNCH)
}

pub(super) fn launch_in(kept: &Mutex<Option<PathBuf>>) -> Option<PathBuf> {
    kept.lock().clone().or_else(|| std::env::current_dir().ok())
}

pub(super) fn not_inherited(handle: HANDLE) -> io::Result<()> {
    checked(unsafe { SetHandleInformation(handle, HANDLE_FLAG_INHERIT, 0) })
}

extern "system" fn leave_to_child(_event: u32) -> BOOL {
    TRUE
}

pub(super) fn run_in_place(command: &mut Command) -> io::Result<i32> {
    unsafe { SetConsoleCtrlHandler(Some(leave_to_child), TRUE) };
    let status = command.status();
    unsafe { SetConsoleCtrlHandler(Some(leave_to_child), FALSE) };
    Ok(status?.code().unwrap_or(1))
}

pub(super) type Stop = Box<dyn FnOnce() + Send>;

static STOP: Mutex<Option<Stop>> = Mutex::new(None);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Answer {
    Pass,
    Handled,
    Wait,
}

pub(super) fn answer(event: u32, windowless: bool, stop: &Mutex<Option<Stop>>) -> Answer {
    let ending = match event {
        CTRL_C_EVENT | CTRL_BREAK_EVENT if windowless => return Answer::Handled,
        CTRL_C_EVENT | CTRL_BREAK_EVENT => false,
        CTRL_CLOSE_EVENT | CTRL_LOGOFF_EVENT | CTRL_SHUTDOWN_EVENT => true,
        _ => return Answer::Pass,
    };
    let taken = stop.lock().take();
    match taken {
        Some(stop) => {
            stop();
            if ending { Answer::Wait } else { Answer::Handled }
        }
        None if ending => Answer::Wait,
        None => Answer::Pass,
    }
}

extern "system" fn stop_event(event: u32) -> BOOL {
    let windowless = unsafe { GetConsoleWindow() }.is_null();
    match answer(event, windowless, &STOP) {
        Answer::Pass => FALSE,
        Answer::Handled => TRUE,
        Answer::Wait => loop {
            thread::park();
        },
    }
}

pub(crate) fn on_stop_signal(stop: impl FnOnce() + Send + 'static) -> io::Result<()> {
    *STOP.lock() = Some(Box::new(stop));
    unsafe { SetConsoleCtrlHandler(None, FALSE) };
    checked(unsafe { SetConsoleCtrlHandler(Some(stop_event), TRUE) })
}

#[cfg(test)]
mod tests;
