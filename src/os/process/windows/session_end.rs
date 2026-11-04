use std::cell::Cell;
use std::io;
use std::ptr;
use std::sync::mpsc::{self, Sender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use parking_lot::Mutex;
use windows_sys::Win32::Foundation::{ERROR_CLASS_ALREADY_EXISTS, HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::Threading::SetProcessShutdownParameters;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, MSG, RegisterClassW, WM_CLOSE, WM_ENDSESSION,
    WM_QUERYENDSESSION, WNDCLASSW,
};
use windows_sys::core::PCWSTR;
use windows_sys::w;

use super::Stop;
use crate::log;

const FIRST: u32 = 0x3FF;
const WAIT: Duration = Duration::from_secs(4);
const CLASS: PCWSTR = w!("cornercase session end");

#[derive(Clone, Copy)]
struct Ending {
    stop: &'static Mutex<Option<Stop>>,
    wait: Duration,
}

thread_local! {
    static ENDING: Cell<Option<Ending>> = const { Cell::new(None) };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reply {
    Default,
    Allow,
    Handled,
    Wait,
}

fn reply(message: u32, ending: WPARAM, stop: &Mutex<Option<Stop>>) -> Reply {
    match message {
        WM_QUERYENDSESSION => Reply::Allow,
        WM_ENDSESSION if ending == 0 => Reply::Handled,
        WM_ENDSESSION => {
            start(stop);
            Reply::Wait
        }
        WM_CLOSE => {
            start(stop);
            Reply::Handled
        }
        _ => Reply::Default,
    }
}

fn start(stop: &Mutex<Option<Stop>>) {
    let taken = stop.lock().take();
    if let Some(stop) = taken {
        stop();
    }
}

extern "system" fn window_message(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let Some(ending) = ENDING.get() else { return unsafe { DefWindowProcW(window, message, wparam, lparam) } };
    match reply(message, wparam, ending.stop) {
        Reply::Default => unsafe { DefWindowProcW(window, message, wparam, lparam) },
        Reply::Allow => 1,
        Reply::Handled => 0,
        Reply::Wait => {
            thread::sleep(ending.wait);
            0
        }
    }
}

pub(super) fn watch(stop: &'static Mutex<Option<Stop>>) {
    ask_first();
    if let Err(error) = open(Ending { stop, wait: WAIT }) {
        log::warning!("server", "cannot hear when Windows ends the session", error = error);
    }
}

fn ask_first() {
    unsafe { SetProcessShutdownParameters(FIRST, 0) };
}

fn open(ending: Ending) -> io::Result<(usize, JoinHandle<()>)> {
    let (opened, window) = mpsc::channel();
    let thread = thread::Builder::new().name("session end".into()).spawn(move || serve(ending, &opened))?;
    let window = window.recv().map_err(|_| io::Error::other("the session end window's thread ended"))??;
    Ok((window, thread))
}

fn serve(ending: Ending, opened: &Sender<io::Result<usize>>) {
    ENDING.set(Some(ending));
    match create() {
        Ok(window) => {
            let _ = opened.send(Ok(window.expose_provenance()));
            pump();
        }
        Err(error) => {
            let _ = opened.send(Err(error));
        }
    }
}

fn create() -> io::Result<HWND> {
    register()?;
    let window = unsafe {
        CreateWindowExW(
            0,
            CLASS,
            w!("cornercase server"),
            0,
            0,
            0,
            0,
            0,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null(),
        )
    };
    if window.is_null() { Err(io::Error::last_os_error()) } else { Ok(window) }
}

fn register() -> io::Result<()> {
    let class = WNDCLASSW { lpfnWndProc: Some(window_message), lpszClassName: CLASS, ..WNDCLASSW::default() };
    if unsafe { RegisterClassW(&raw const class) } != 0 {
        return Ok(());
    }
    let error = io::Error::last_os_error();
    if error.raw_os_error() == i32::try_from(ERROR_CLASS_ALREADY_EXISTS).ok() { Ok(()) } else { Err(error) }
}

fn pump() {
    let mut message = MSG::default();
    while unsafe { GetMessageW(&raw mut message, ptr::null_mut(), 0, 0) } > 0 {
        unsafe { DispatchMessageW(&raw const message) };
    }
}

#[cfg(test)]
mod tests;
