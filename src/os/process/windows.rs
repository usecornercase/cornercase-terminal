use std::io;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::os::windows::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::{self, Child, Command, ExitStatus};
use std::{ptr, thread};

use parking_lot::Mutex;
use windows_sys::Win32::Foundation::{
    ERROR_ACCESS_DENIED, FALSE, HANDLE, HANDLE_FLAG_INHERIT, HWND, LPARAM, STILL_ACTIVE, SetHandleInformation, TRUE,
};
use windows_sys::Win32::System::Console::{
    CTRL_BREAK_EVENT, CTRL_C_EVENT, CTRL_CLOSE_EVENT, CTRL_LOGOFF_EVENT, CTRL_SHUTDOWN_EVENT, GetConsoleWindow,
    GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, SetConsoleCtrlHandler,
};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT, JOB_OBJECT_LIMIT_BREAKAWAY_OK,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOBOBJECT_BASIC_LIMIT_INFORMATION, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JobObjectBasicProcessIdList, JobObjectExtendedLimitInformation, QueryInformationJobObject, SetInformationJobObject,
    TerminateJobObject,
};
use windows_sys::Win32::System::Threading::{
    CREATE_BREAKAWAY_FROM_JOB, CREATE_NEW_PROCESS_GROUP, CREATE_NO_WINDOW, GetExitCodeProcess, OpenProcess,
    PROCESS_ACCESS_RIGHTS, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE, TerminateProcess,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GW_OWNER, GetWindow, GetWindowThreadProcessId, IsWindowVisible, PostMessageW, WM_CLOSE,
};
use windows_sys::core::BOOL;

const KILLED: u32 = 1;

const LISTED: usize = 256;

const KILL_ON_CLOSE: JOB_OBJECT_LIMIT = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_BREAKAWAY_OK;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Pid(u32);

impl Pid {
    pub(crate) fn from_raw(raw: i32) -> Option<Self> {
        u32::try_from(raw).ok().filter(|raw| *raw > 0).map(Self)
    }

    pub(crate) fn as_raw(self) -> u32 {
        self.0
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
        self.creation_flags(CREATE_NEW_PROCESS_GROUP)
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

fn checked(done: BOOL) -> io::Result<()> {
    if done == FALSE { Err(io::Error::last_os_error()) } else { Ok(()) }
}

pub(crate) fn setsid() -> io::Result<Pid> {
    Err(io::ErrorKind::Unsupported.into())
}

pub(crate) fn alive(pid: i32) -> bool {
    let Some(pid) = u32::try_from(pid).ok().filter(|pid| *pid != 0) else { return false };
    match open(pid, PROCESS_QUERY_LIMITED_INFORMATION) {
        Ok(process) => exit_code(&process) == Some(STILL_ACTIVE.cast_unsigned()),
        Err(error) => error.raw_os_error() == i32::try_from(ERROR_ACCESS_DENIED).ok(),
    }
}

pub(crate) fn askable(pid: i32) -> bool {
    u32::try_from(pid).is_ok_and(|pid| !main_windows(pid).is_empty())
}

pub(crate) fn kill_process(pid: Pid, signal: Signal) -> io::Result<()> {
    match signal {
        Signal::Kill => terminate(pid.as_raw()),
        Signal::Term | Signal::Hup => close_windows(pid.as_raw()).map(|_| ()),
    }
}

fn open(pid: u32, access: PROCESS_ACCESS_RIGHTS) -> io::Result<OwnedHandle> {
    let handle = unsafe { OpenProcess(access, 0, pid) };
    if handle.is_null() {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { OwnedHandle::from_raw_handle(handle) })
}

fn exit_code(process: &OwnedHandle) -> Option<u32> {
    let mut code = 0;
    let read = unsafe { GetExitCodeProcess(process.as_raw_handle(), &raw mut code) };
    (read != 0).then_some(code)
}

fn terminate(pid: u32) -> io::Result<()> {
    let process = open(pid, PROCESS_TERMINATE)?;
    checked(unsafe { TerminateProcess(process.as_raw_handle(), KILLED) })
}

struct Found {
    pid: u32,
    windows: Vec<HWND>,
}

fn main_windows(pid: u32) -> Vec<HWND> {
    let mut found = Found { pid, windows: Vec::new() };
    let at: LPARAM = (&raw mut found).expose_provenance().cast_signed();
    unsafe { EnumWindows(Some(each_window), at) };
    found.windows
}

unsafe extern "system" fn each_window(window: HWND, at: LPARAM) -> BOOL {
    let found = unsafe { &mut *ptr::with_exposed_provenance_mut::<Found>(at.cast_unsigned()) };
    if owner(window) == found.pid && is_main(window) {
        found.windows.push(window);
    }
    TRUE
}

fn owner(window: HWND) -> u32 {
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(window, &raw mut pid) };
    pid
}

fn is_main(window: HWND) -> bool {
    unsafe { IsWindowVisible(window) != FALSE && GetWindow(window, GW_OWNER).is_null() }
}

fn close_windows(pid: u32) -> io::Result<usize> {
    let mut failed = None;
    let mut asked = 0;
    for window in main_windows(pid) {
        match checked(unsafe { PostMessageW(window, WM_CLOSE, 0, 0) }) {
            Ok(()) => asked += 1,
            Err(error) => failed = Some(error),
        }
    }
    failed.filter(|_| asked == 0).map_or(Ok(asked), Err)
}

pub(crate) struct Job {
    handle: Option<OwnedHandle>,
    pid: u32,
}

impl Job {
    pub(crate) fn of(child: &Child) -> Self {
        let handle = create().filter(|job| limit(job, KILL_ON_CLOSE) && assign(job, child));
        Self { handle, pid: child.id() }
    }

    pub(crate) fn signal(&self, signal: Signal) -> bool {
        match signal {
            Signal::Kill => self.terminate().is_ok(),
            Signal::Term | Signal::Hup => {
                self.members().into_iter().map(|pid| close_windows(pid).unwrap_or(0)).sum::<usize>() > 0
            }
        }
    }

    fn terminate(&self) -> io::Result<()> {
        match &self.handle {
            Some(job) => checked(unsafe { TerminateJobObject(job.as_raw_handle(), KILLED) }),
            None => terminate(self.pid),
        }
    }

    fn members(&self) -> Vec<u32> {
        self.handle.as_ref().map_or_else(|| vec![self.pid], listed)
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        if let Some(job) = &self.handle {
            limit(job, JOB_OBJECT_LIMIT_BREAKAWAY_OK);
        }
    }
}

fn size_u32<T>() -> u32 {
    u32::try_from(size_of::<T>()).unwrap_or(u32::MAX)
}

fn create() -> Option<OwnedHandle> {
    let job = unsafe { CreateJobObjectW(ptr::null(), ptr::null()) };
    (!job.is_null()).then(|| unsafe { OwnedHandle::from_raw_handle(job) })
}

fn limit(job: &OwnedHandle, flags: JOB_OBJECT_LIMIT) -> bool {
    let basic = JOBOBJECT_BASIC_LIMIT_INFORMATION { LimitFlags: flags, ..Default::default() };
    let info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION { BasicLimitInformation: basic, ..Default::default() };
    let (at, size) = ((&raw const info).cast(), size_u32::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>());
    unsafe { SetInformationJobObject(job.as_raw_handle(), JobObjectExtendedLimitInformation, at, size) != FALSE }
}

fn assign(job: &OwnedHandle, child: &Child) -> bool {
    unsafe { AssignProcessToJobObject(job.as_raw_handle(), child.as_raw_handle()) != FALSE }
}

#[repr(C)]
struct Listed {
    _assigned: u32,
    count: u32,
    pids: [usize; LISTED],
}

fn listed(job: &OwnedHandle) -> Vec<u32> {
    let mut list = Listed { _assigned: 0, count: 0, pids: [0; LISTED] };
    let (at, size) = ((&raw mut list).cast(), size_u32::<Listed>());
    unsafe { QueryInformationJobObject(job.as_raw_handle(), JobObjectBasicProcessIdList, at, size, ptr::null_mut()) };
    let count = usize::try_from(list.count).unwrap_or(0).min(LISTED);
    list.pids[..count].iter().filter_map(|pid| u32::try_from(*pid).ok()).collect()
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
    checked(unsafe { SetConsoleCtrlHandler(Some(stop_event), TRUE) })?;
    session_end::watch(&STOP);
    Ok(())
}

mod session_end;

#[cfg(test)]
mod tests;
