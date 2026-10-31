use std::io::{self, BufRead, BufReader, IsTerminal, Read, Write, stdin, stdout};
use std::net::Shutdown;
use std::os::fd::{AsFd, BorrowedFd};
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, ChildStdout, Command};
use std::sync::atomic::Ordering;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crossterm::event::{
    DisableBracketedPaste, DisableFocusChange, DisableMouseCapture, EnableBracketedPaste, EnableFocusChange,
    EnableMouseCapture, KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use crossterm::execute;
use ratatui::{TerminalOptions, Viewport};

use self::probe::probe_host;
use self::window::attach;
use crate::error::{Error, Result};
use crate::graphics::Support;
use crate::graphics::detect::Facts;
use crate::host::theme::HostTheme;
use crate::remote::{Remote, Stderr, Use};
use crate::{host::pointer, paths, process, protocol, update::restart};

pub(crate) mod input;
mod probe;
mod requests;
mod server_process;
mod window;

pub(crate) use requests::{
    answer, ask, ask_starting, close_unused_link, connect, follow, follow_all, report_now, running_now,
};
pub(crate) use requests::{
    answered, request, use_account_login, use_account_restart, use_account_switch, use_accounts, use_agent_folds,
    use_build_folders, use_close_agent_tabs, use_close_unlock, use_focus_reveal, use_host_events, use_issue_names,
    use_issues, use_notices, use_popup_terminal, use_queue_undo, use_surface_patches, use_todo_panel, use_view,
};
pub(crate) use requests::{input_ended, reader_gone_within, use_remote, used_remote, watched};
pub(crate) use requests::{line_done, share, unshare};
pub(crate) use server_process::{Found, find, find_or_start, running, wait_for_exit};
pub(crate) use server_process::{kill_server, restart_server};

const OTHER_BUILD: &str = "The running cornercase server comes from another build; cornercase was probably updated.";
pub(crate) const RESTART: &str = "Restart it now? [y/N] ";
const UPDATE_THERE: &str = "Run it there now? [y/N] ";
const INPUT_POLL: Duration = Duration::from_millis(100);

pub(crate) fn run() -> Result<()> {
    if std::env::var_os(protocol::NESTED_ENV).is_some() {
        return Err(Error::Nested);
    }
    match open(&Target::Local, std::env::var_os(protocol::RESTARTED_ENV).is_some()) {
        Err(Error::Rejected(_))
            if stdin().is_terminal()
                && confirm(&format!(
                    "{OTHER_BUILD}\n{}\n{RESTART}",
                    restart::confirmation(running_now().as_deref())
                )) =>
        {
            kill_server()?;
            open(&Target::Local, false)
        }
        result => result,
    }
}

pub(crate) fn remote(remote: &Remote) -> Result<()> {
    let target = Target::Remote(remote.clone());
    let host = remote.host();
    let asks = stdin().is_terminal();
    match open(&target, false) {
        Err(Error::Rejected(_))
            if asks
                && confirm(&format!(
                    "The cornercase server on {host} comes from another build; cornercase was probably updated there.\n{}\n{RESTART}",
                    restart::confirmation(None)
                )) =>
        {
            remote.kill_server()?;
            open(&target, false)
        }
        Err(Error::Rejected(_)) => Err(Error::Rejected(format!(
            "the cornercase server on `{host}` comes from another build. \
             Run `cornercase kill-server` there (it closes all its terminals) and attach again"
        ))),
        Err(e @ (Error::RemoteOlder { .. } | Error::RemoteTooOld(_)))
            if asks && confirm(&format!("{e}.\n{UPDATE_THERE}")) =>
        {
            remote.update()?;
            open(&target, false)
        }
        result => result,
    }
}

pub(crate) fn confirm(question: &str) -> bool {
    print!("{question}");
    let _ = stdout().flush();
    let mut answer = String::new();
    stdin().lock().read_line(&mut answer).is_ok() && matches!(answer.trim().to_lowercase().as_str(), "y" | "yes")
}

#[derive(Debug)]
enum Ending {
    Detached,
    Restart,
    GaveUp,
}

#[derive(Debug, Clone)]
enum Target {
    Local,
    Remote(Remote),
}

impl Target {
    fn connect(&self, again: bool) -> Result<Link> {
        match self {
            Self::Local => {
                let found = find_or_start()?;
                Link::socket(found.stream)
            }
            Self::Remote(remote) => Link::remote(remote, Use::Window { again }),
        }
    }

    fn reconnects(&self) -> bool {
        matches!(self, Self::Remote(_))
    }

    fn host(&self) -> &str {
        match self {
            Self::Local => "",
            Self::Remote(remote) => remote.host(),
        }
    }
}

pub(crate) trait Source: Read + Send {
    fn fd(&self) -> BorrowedFd<'_>;

    fn buffered(&self) -> bool {
        false
    }

    fn close(&self) {}
}

impl Source for UnixStream {
    fn fd(&self) -> BorrowedFd<'_> {
        self.as_fd()
    }

    fn close(&self) {
        let _ = self.shutdown(Shutdown::Both);
    }
}

impl Source for BufReader<ChildStdout> {
    fn fd(&self) -> BorrowedFd<'_> {
        self.get_ref().as_fd()
    }

    fn buffered(&self) -> bool {
        !self.buffer().is_empty()
    }
}

impl<S: Source + ?Sized> Source for &mut S {
    fn fd(&self) -> BorrowedFd<'_> {
        (**self).fd()
    }

    fn buffered(&self) -> bool {
        (**self).buffered()
    }

    fn close(&self) {
        (**self).close();
    }
}

impl Source for Box<dyn Source> {
    fn fd(&self) -> BorrowedFd<'_> {
        (**self).fd()
    }

    fn buffered(&self) -> bool {
        (**self).buffered()
    }

    fn close(&self) {
        (**self).close();
    }
}

pub(crate) struct Link {
    reader: Box<dyn Source>,
    writer: Box<dyn Write + Send>,
    guard: Guard,
}

#[derive(Default)]
struct Guard {
    child: Option<Child>,
    stderr: Option<Stderr>,
    collector: Option<JoinHandle<()>>,
    remote: Option<Remote>,
}

impl Drop for Guard {
    fn drop(&mut self) {
        if let Some(child) = self.child.take() {
            process::stop(child);
        }
    }
}

impl Link {
    fn socket(stream: UnixStream) -> Result<Self> {
        Ok(Self { reader: Box::new(stream.try_clone()?), writer: Box::new(stream), guard: Guard::default() })
    }

    fn remote(remote: &Remote, usage: Use) -> Result<Self> {
        let piped = remote.connect(usage)?;
        Ok(Self {
            reader: Box::new(piped.reader),
            writer: Box::new(piped.writer),
            guard: Guard {
                child: Some(piped.child),
                stderr: Some(piped.stderr),
                collector: piped.collector,
                remote: Some(remote.clone()),
            },
        })
    }
}

fn open(target: &Target, restarted: bool) -> Result<()> {
    let link = target.connect(false).inspect_err(|_| {
        if restarted {
            release_screen();
        }
    })?;
    let exe = std::env::current_exe();

    let mut terminal = if restarted {
        ratatui::init_with_options(TerminalOptions { viewport: Viewport::Fullscreen })
    } else {
        ratatui::init()
    };
    let host = probe_host();
    let mut forget = host.graphics.forget();
    forget.extend(host.pointer.map(pointer::Terminal::reset).unwrap_or_default());
    let _ = execute!(
        stdout(),
        EnableBracketedPaste,
        EnableMouseCapture,
        EnableFocusChange,
        PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
    );
    install_panic_hook(forget.clone());
    let result = attach(link, target, &mut terminal, host);
    write_out(&forget);
    if !matches!(result, Ok(Ending::Restart)) {
        restore_input_modes();
        ratatui::restore();
    }
    match result? {
        Ending::Restart => come_back(exe),
        Ending::Detached => Ok(()),
        Ending::GaveUp => {
            println!("stopped reconnecting to {}; its shells keep running there", target.host());
            Ok(())
        }
    }
}

fn come_back(exe: io::Result<PathBuf>) -> Result<()> {
    restore_input_modes();
    let _ = crossterm::terminal::disable_raw_mode();
    wait_for_exit(&paths::socket());
    let error = match exe {
        Ok(exe) => Command::new(exe).env(protocol::RESTARTED_ENV, "1").exec(),
        Err(e) => e,
    };
    release_screen();
    Err(error.into())
}

fn install_panic_hook(forget: Vec<u8>) {
    let main = thread::current().id();
    let failures = std::sync::atomic::AtomicU64::new(0);
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if thread::current().id() == main {
            write_out(&forget);
            restore_input_modes();
            prev_hook(info);
        } else if (failures.fetch_add(1, Ordering::Relaxed) + 1).is_power_of_two() {
            input::report(&format!("{info}"));
        }
    }));
}

fn write_out(bytes: &[u8]) {
    if !bytes.is_empty() {
        let mut out = stdout();
        let _ = out.write_all(bytes).and_then(|()| out.flush());
    }
}

fn restore_input_modes() {
    let _ =
        execute!(stdout(), PopKeyboardEnhancementFlags, DisableBracketedPaste, DisableMouseCapture, DisableFocusChange);
}

fn release_screen() {
    let _ = execute!(stdout(), DisableBracketedPaste, DisableMouseCapture, DisableFocusChange);
    ratatui::restore();
}

struct Host {
    theme: HostTheme,
    facts: Facts,
    graphics: Support,
    probe: String,
    tty: Option<String>,
    probe_timed_out: bool,
    pointer: Option<pointer::Terminal>,
}
