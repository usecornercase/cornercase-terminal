use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, IsTerminal, Write, stdin, stdout};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crossterm::event::{
    self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
    KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use crossterm::execute;
use ratatui::DefaultTerminal;
use rustix::event::{PollFd, PollFlags, Timespec, poll};
use serde::de::DeserializeOwned;
use serde_json::Value;
use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};
use signal_hook::iterator::Signals;

use crate::control::{self, Request, Response};
use crate::error::{Error, Result};
use crate::host_theme::{HostTheme, ThemeProbe};
use crate::notify;
use crate::protocol::{self, ClientMessage, Hello, ServerMessage};
use crate::restart;
use crate::update::{self, CURRENT, Install, Outcome};

const THEME_QUERY_TIMEOUT: Duration = Duration::from_secs(1);
const SERVER_START_TIMEOUT: Duration = Duration::from_secs(5);
const SERVER_EXIT_TIMEOUT: Duration = Duration::from_secs(5);
const POLL: Duration = Duration::from_millis(20);
const STATUS_TIMEOUT: Duration = Duration::from_secs(1);
const INCOMPATIBLE: &str = "the running cornercase server is incompatible with this build. \
    Run `cornercase kill-server` (it closes all its terminals) and start cornercase again";
const OTHER_BUILD: &str = "The running cornercase server comes from another build; cornercase was probably updated.";
const RESTART: &str = "Restart it now? [y/N] ";
const INSIDE: &str = "This terminal is one of them, so it closes too.";

pub fn run() -> Result<()> {
    if std::env::var_os(protocol::NESTED_ENV).is_some() {
        return Err(Error::Nested);
    }
    match open() {
        Err(Error::Rejected(_))
            if stdin().is_terminal()
                && confirm(&format!(
                    "{OTHER_BUILD}\n{}\n{RESTART}",
                    restart::confirmation(running_now().as_deref())
                )) =>
        {
            kill_server()?;
            open()
        }
        result => result,
    }
}

fn confirm(question: &str) -> bool {
    print!("{question}");
    let _ = stdout().flush();
    let mut answer = String::new();
    stdin().lock().read_line(&mut answer).is_ok() && matches!(answer.trim().to_lowercase().as_str(), "y" | "yes")
}

enum Ending {
    Detached,
    Restart,
}

fn open() -> Result<()> {
    let path = protocol::socket_path();
    protocol::check_socket_dir(&path)?;
    let exe = std::env::current_exe();
    let stream = connect_or_start(&path)?;
    protocol::check_peer(&stream, protocol::own_uid())?;

    let terminal = ratatui::init();
    let _ = execute!(
        stdout(),
        EnableBracketedPaste,
        EnableMouseCapture,
        PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
    );
    install_panic_hook();
    let result = attach(stream, &terminal);
    restore_input_modes();
    ratatui::restore();
    match result? {
        Ending::Restart => {
            wait_for_exit(&path);
            Err(Command::new(exe?).exec().into())
        }
        Ending::Detached => Ok(()),
    }
}

fn connect() -> Result<UnixStream> {
    let path = protocol::socket_path();
    protocol::check_socket_dir(&path)?;
    let stream = UnixStream::connect(&path).map_err(|_| Error::NoServer)?;
    protocol::check_peer(&stream, protocol::own_uid())?;
    Ok(stream)
}

pub fn ask(name: &'static str, command: control::Command) -> Result<Value> {
    ask_on(connect()?, name, command)
}

fn ask_on(mut stream: UnixStream, name: &'static str, command: control::Command) -> Result<Value> {
    let caller = std::env::var(control::PANE_ENV).ok().and_then(|id| id.parse().ok());
    let server = std::env::var(control::SERVER_ENV).ok();
    let request =
        serde_json::to_string(&Request { caller, server, command }).map_err(|e| Error::Control(e.to_string()))?;
    protocol::send(&mut stream, &ClientMessage::Request(request))?;
    loop {
        match protocol::recv::<ServerMessage>(&mut stream) {
            Ok(Some(ServerMessage::Response(text))) => {
                return match serde_json::from_str(&text) {
                    Ok(Response::Ok(value)) => Ok(value),
                    Ok(Response::Error(message)) => Err(Error::Control(message)),
                    Err(e) => Err(Error::Control(format!("cannot read the server's answer: {e}"))),
                };
            }
            Ok(Some(ServerMessage::Rejected(_))) => return Err(Error::OldServer(name)),
            Err(e) if e.kind() == io::ErrorKind::InvalidData => return Err(Error::OldServer(name)),
            Ok(Some(ServerMessage::Frame(_) | ServerMessage::Detached)) => {}
            Ok(Some(ServerMessage::Shutdown | ServerMessage::Restart(_)) | None) | Err(_) => {
                return Err(Error::ServerGone);
            }
        }
    }
}

pub fn answer<T: DeserializeOwned>(value: Value) -> Result<T> {
    serde_json::from_value(value).map_err(|e| Error::Control(format!("cannot read the server's answer: {e}")))
}

pub fn running_now() -> Option<Vec<restart::Running>> {
    let stream = connect().ok()?;
    stream.set_read_timeout(Some(STATUS_TIMEOUT)).ok()?;
    let value = ask_on(stream, "status", control::Command::Status(control::Status {})).ok()?;
    let report: control::Report = answer(value).ok()?;
    Some(restart::from_report(&report))
}

pub fn kill_server() -> Result<bool> {
    Ok(stop_server(&ClientMessage::KillServer)?.is_some())
}

pub fn restart_server() -> Result<bool> {
    match stop_server(&ClientMessage::Restart)? {
        Some(true) => kill_server(),
        answer => Ok(answer.is_some()),
    }
}

fn stop_server(msg: &ClientMessage) -> Result<Option<bool>> {
    let path = protocol::socket_path();
    protocol::check_socket_dir(&path)?;
    let Ok(mut stream) = UnixStream::connect(&path) else { return Ok(None) };
    protocol::check_peer(&stream, protocol::own_uid())?;
    protocol::send(&mut stream, msg)?;
    let mut rejected = false;
    while let Ok(Some(answer)) = protocol::recv::<ServerMessage>(&mut stream) {
        rejected |= matches!(answer, ServerMessage::Rejected(_));
    }
    if !rejected {
        wait_for_exit(&path);
    }
    Ok(Some(rejected))
}

pub fn update(check_only: bool, yes: bool) -> Result<bool> {
    let url = std::env::var(update::LATEST_ENV).ok();
    if cfg!(debug_assertions) && url.is_none() {
        return Err(Error::DevelopmentBuild);
    }
    let url = url.unwrap_or_else(|| update::DEFAULT_LATEST.into());
    let target = update::target();
    let install = update::install(&fs::canonicalize(std::env::current_exe()?)?, target);
    if check_only {
        match update::check(&url, CURRENT)? {
            Some(release) => {
                let command = match install {
                    Install::Replace(_) => "cornercase update",
                    Install::Command(command) => command,
                };
                announce(&release.version, command);
            }
            None => println!("cornercase {CURRENT} is up to date"),
        }
        return Ok(true);
    }
    match update::install_latest(&url, CURRENT, &install, target)? {
        Outcome::UpToDate => println!("cornercase {CURRENT} is up to date"),
        Outcome::Manual(version, command) => {
            announce(&version, command);
            return Ok(false);
        }
        Outcome::Updated(version) => {
            println!("updated cornercase {CURRENT} → {version}");
            offer_restart(yes)?;
        }
    }
    Ok(true)
}

fn announce(version: &str, command: &str) {
    println!("cornercase {version} is out (you have {CURRENT}). Update it with:\n{command}");
}

fn offer_restart(yes: bool) -> Result<()> {
    if server_running()?
        && !restart_if_confirmed(Some(&format!("The running cornercase server still runs {CURRENT}.")), yes)?
    {
        println!(
            "the server keeps running {CURRENT}; run `cornercase kill-server` and start cornercase to use the new one"
        );
    }
    Ok(())
}

pub fn restart(yes: bool) -> Result<()> {
    if !server_running()? {
        eprintln!("no cornercase server is running");
    } else if !restart_if_confirmed(None, yes)? {
        let hint = if stdin().is_terminal() { "" } else { "; pass --yes to restart it without asking" };
        println!("the server keeps running{hint}");
    }
    Ok(())
}

fn server_running() -> Result<bool> {
    let path = protocol::socket_path();
    protocol::check_socket_dir(&path)?;
    Ok(UnixStream::connect(&path).is_ok())
}

fn restart_if_confirmed(intro: Option<&str>, yes: bool) -> Result<bool> {
    let inside = std::env::var_os(protocol::NESTED_ENV).is_some();
    let lead: Vec<&str> = [intro, inside.then_some(INSIDE)].into_iter().flatten().collect();
    let running = if yes || stdin().is_terminal() { running_now() } else { None };
    let stops = restart::confirmation(running.as_deref());
    let question =
        if lead.is_empty() { format!("{stops}\n{RESTART}") } else { format!("{}\n{stops}\n{RESTART}", lead.join(" ")) };
    let confirmed = yes || (stdin().is_terminal() && confirm(&question));
    if !confirmed {
        return Ok(false);
    }
    if yes {
        println!("{stops}");
    }
    if inside {
        println!("restarting the cornercase server");
    }
    if restart_server()? {
        println!("restarted the cornercase server; your session comes back the next time cornercase starts");
        if let Some(line) = running.as_deref().and_then(restart::stopped) {
            println!("{line}");
        }
    }
    Ok(true)
}

fn wait_for_exit(path: &Path) {
    let deadline = Instant::now() + SERVER_EXIT_TIMEOUT;
    while matches!(protocol::try_lock(path), Ok(None)) && Instant::now() < deadline {
        thread::sleep(POLL);
    }
}

fn connect_or_start(path: &Path) -> Result<UnixStream> {
    if let Ok(stream) = UnixStream::connect(path) {
        return Ok(stream);
    }
    let log = protocol::log_path(path);
    let start_error = |e| Error::ServerStart { log: log.clone(), source: Some(e) };
    let deadline = Instant::now() + SERVER_START_TIMEOUT;
    let mut server = start_server(&log).map_err(start_error)?;
    loop {
        if let Ok(stream) = UnixStream::connect(path) {
            reap(server);
            return Ok(stream);
        }
        if Instant::now() >= deadline {
            reap(server);
            return Err(Error::ServerStart { log, source: None });
        }
        if matches!(server.try_wait(), Ok(Some(_))) {
            server = start_server(&log).map_err(start_error)?;
        }
        thread::sleep(POLL);
    }
}

fn start_server(log: &Path) -> io::Result<Child> {
    let log = OpenOptions::new().create(true).append(true).open(log)?;
    Command::new(std::env::current_exe()?)
        .arg("server")
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log)
        .spawn()
}

fn reap(mut child: Child) {
    thread::spawn(move || child.wait());
}

fn install_panic_hook() {
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_input_modes();
        prev_hook(info);
    }));
}

fn restore_input_modes() {
    let _ = execute!(stdout(), PopKeyboardEnhancementFlags, DisableBracketedPaste, DisableMouseCapture);
}

fn attach(stream: UnixStream, terminal: &DefaultTerminal) -> Result<Ending> {
    let (theme, name) = query_host();
    let theme = HostTheme { truecolor: truecolor(), ..theme };
    let notify = notify::detect(name.as_deref(), |var| std::env::var(var).ok());
    let size = terminal.size()?;
    let mut writer = stream.try_clone()?;
    let (version, build) = (protocol::VERSION, protocol::build_id());
    let hello = Hello { version, build, cols: size.width, rows: size.height, theme, notify };
    protocol::send(&mut writer, &ClientMessage::Hello(Box::new(hello)))?;
    spawn_input_thread(writer);
    spawn_signal_thread(stream.try_clone()?)?;
    receive(stream)
}

fn receive(mut stream: UnixStream) -> Result<Ending> {
    let mut out = stdout();
    loop {
        match protocol::recv::<ServerMessage>(&mut stream) {
            Ok(Some(ServerMessage::Frame(bytes))) => {
                out.write_all(&bytes)?;
                out.flush()?;
            }
            Ok(Some(ServerMessage::Rejected(reason))) => return Err(Error::Rejected(reason)),
            Err(e) if e.kind() == io::ErrorKind::InvalidData => return Err(Error::Rejected(INCOMPATIBLE.into())),
            Ok(Some(ServerMessage::Restart(_))) => return Ok(Ending::Restart),
            Ok(Some(ServerMessage::Response(_))) => {}
            Ok(Some(ServerMessage::Detached | ServerMessage::Shutdown) | None) | Err(_) => return Ok(Ending::Detached),
        }
    }
}

fn truecolor() -> bool {
    std::env::var("COLORTERM").is_ok_and(|v| matches!(v.as_str(), "truecolor" | "24bit"))
}

fn query_host() -> (HostTheme, Option<String>) {
    let mut out = stdout();
    if out.write_all(HostTheme::query().as_bytes()).and_then(|()| out.flush()).is_err() {
        return (HostTheme::default(), None);
    }
    let input = stdin();
    let deadline = Instant::now() + THEME_QUERY_TIMEOUT;
    let mut probe = ThemeProbe::default();
    let mut buf = [0u8; 4096];
    while !probe.is_done() {
        let Ok(left) = Timespec::try_from(deadline.saturating_duration_since(Instant::now())) else { break };
        let mut fds = [PollFd::new(&input, PollFlags::IN)];
        if !matches!(poll(&mut fds, Some(&left)), Ok(1..)) {
            break;
        }
        match rustix::io::read(&input, &mut buf) {
            Ok(n @ 1..) => probe.feed(&buf[..n]),
            _ => break,
        }
    }
    let name = probe.terminal().map(str::to_owned);
    (probe.finish(), name)
}

fn spawn_input_thread(mut writer: UnixStream) {
    thread::spawn(move || {
        while let Ok(ev) = event::read() {
            if protocol::send(&mut writer, &ClientMessage::Event(ev)).is_err() {
                return;
            }
        }
    });
}

fn spawn_signal_thread(stream: UnixStream) -> Result<()> {
    let mut signals = Signals::new([SIGTERM, SIGHUP, SIGINT]).map_err(Error::Signals)?;
    thread::spawn(move || {
        if signals.forever().next().is_some() {
            let _ = stream.shutdown(Shutdown::Both);
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    mod restart {
        use super::*;

        #[test]
        fn never_takes_the_path_the_server_sends() {
            let (client, mut server) = UnixStream::pair().expect("a socket pair");
            protocol::send(&mut server, &ServerMessage::Restart(PathBuf::from("/tmp/not-cornercase"))).expect("send");

            assert!(matches!(receive(client), Ok(Ending::Restart)));
        }
    }
}
