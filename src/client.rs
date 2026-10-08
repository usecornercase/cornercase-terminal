use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, IsTerminal, Read, Write, stdin, stdout};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use crossterm::event::{
    self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture, Event, KeyCode,
    KeyEventKind, KeyboardEnhancementFlags, MouseButton, MouseEventKind, PopKeyboardEnhancementFlags,
    PushKeyboardEnhancementFlags,
};
use crossterm::execute;
use ratatui::DefaultTerminal;
use ratatui::backend::Backend;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use rustix::event::{PollFd, PollFlags, Timespec, poll};
use serde::de::DeserializeOwned;
use serde_json::Value;
use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};
use signal_hook::iterator::{Handle, Signals};

use crate::control::{self, Request, Response};
use crate::error::{Error, Result};
use crate::host_theme::{HostTheme, ThemeProbe};
use crate::log;
use crate::notify::{self, Channel};
use crate::protocol::{self, ClientMessage, Hello, ServerMessage};
use crate::remote::{self, Remote, Stderr};
use crate::restart;
use crate::ui::remote::{self as notice, Notice};
use crate::update::{self, CURRENT, Install, Outcome};

const THEME_QUERY_TIMEOUT: Duration = Duration::from_secs(1);
const SERVER_START_TIMEOUT: Duration = Duration::from_secs(5);
const SERVER_EXIT_TIMEOUT: Duration = Duration::from_secs(5);
const POLL: Duration = Duration::from_millis(20);
const STATUS_TIMEOUT: Duration = Duration::from_secs(1);
const LOST: &str = "the cornercase server closed the connection without stopping: it crashed, or this command \
    fell too far behind reading its events";
const INCOMPATIBLE: &str = "the running cornercase server is incompatible with this build. \
    Run `cornercase kill-server` (it closes all its terminals) and start cornercase again";
const OTHER_BUILD: &str = "The running cornercase server comes from another build; cornercase was probably updated.";
const RESTART: &str = "Restart it now? [y/N] ";
const INSIDE: &str = "This terminal is one of them, so it closes too.";
const UPDATE_THERE: &str = "Run it there now? [y/N] ";
const MAX_BACKOFF: Duration = Duration::from_secs(15);
const SECOND: Duration = Duration::from_secs(1);
const INPUT_POLL: Duration = Duration::from_millis(100);

pub fn run() -> Result<()> {
    if std::env::var_os(protocol::NESTED_ENV).is_some() {
        return Err(Error::Nested);
    }
    match open(&Target::Local) {
        Err(Error::Rejected(_))
            if stdin().is_terminal()
                && confirm(&format!(
                    "{OTHER_BUILD}\n{}\n{RESTART}",
                    restart::confirmation(running_now().as_deref())
                )) =>
        {
            kill_server()?;
            open(&Target::Local)
        }
        result => result,
    }
}

pub fn remote(remote: &Remote) -> Result<()> {
    let target = Target::Remote(remote.clone());
    let host = remote.host();
    let asks = stdin().is_terminal();
    match open(&target) {
        Err(Error::Rejected(_))
            if asks
                && confirm(&format!(
                    "The cornercase server on {host} comes from another build; cornercase was probably updated there.\n{}\n{RESTART}",
                    restart::confirmation(None)
                )) =>
        {
            remote.kill_server()?;
            open(&target)
        }
        Err(Error::Rejected(_)) => Err(Error::Rejected(format!(
            "the cornercase server on `{host}` comes from another build. \
             Run `cornercase kill-server` there (it closes all its terminals) and attach again"
        ))),
        Err(e @ (Error::RemoteOlder { .. } | Error::RemoteTooOld(_)))
            if asks && confirm(&format!("{e}.\n{UPDATE_THERE}")) =>
        {
            remote.update()?;
            open(&target)
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
                let path = protocol::socket_path();
                protocol::check_socket_dir(&path)?;
                let stream = connect_or_start(&path)?;
                protocol::check_peer(&stream, protocol::own_uid())?;
                Link::socket(stream)
            }
            Self::Remote(remote) => remote.connect(again),
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

pub struct Link {
    reader: Box<dyn Read + Send>,
    writer: Box<dyn Write + Send>,
    guard: Guard,
}

#[derive(Default)]
struct Guard {
    child: Option<Child>,
    stderr: Option<Stderr>,
}

impl Drop for Guard {
    fn drop(&mut self) {
        if let Some(child) = self.child.take() {
            stop(child);
        }
    }
}

impl Link {
    fn socket(stream: UnixStream) -> Result<Self> {
        Ok(Self { reader: Box::new(stream.try_clone()?), writer: Box::new(stream), guard: Guard::default() })
    }

    pub fn piped(reader: impl Read + Send + 'static, writer: ChildStdin, child: Child, stderr: Stderr) -> Self {
        Self {
            reader: Box::new(reader),
            writer: Box::new(writer),
            guard: Guard { child: Some(child), stderr: Some(stderr) },
        }
    }
}

fn open(target: &Target) -> Result<()> {
    let link = target.connect(false)?;
    let exe = std::env::current_exe();

    let mut terminal = ratatui::init();
    let _ = execute!(
        stdout(),
        EnableBracketedPaste,
        EnableMouseCapture,
        PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
    );
    install_panic_hook();
    let result = attach(link, target, &mut terminal);
    restore_input_modes();
    ratatui::restore();
    match result? {
        Ending::Restart => {
            wait_for_exit(&protocol::socket_path());
            Err(Command::new(exe?).exec().into())
        }
        Ending::Detached => Ok(()),
        Ending::GaveUp => {
            println!("stopped reconnecting to {}; its shells keep running there", target.host());
            Ok(())
        }
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
    request(&mut stream, command)?;
    loop {
        match protocol::recv::<ServerMessage>(&mut stream) {
            Ok(Some(ServerMessage::Response(text))) => return answered(&text),
            Ok(Some(ServerMessage::Rejected(_))) => return Err(Error::OldServer(name)),
            Err(e) if e.kind() == io::ErrorKind::InvalidData => return Err(Error::OldServer(name)),
            Ok(Some(ServerMessage::Frame(_) | ServerMessage::Detached)) => {}
            Ok(Some(ServerMessage::Shutdown | ServerMessage::Restart(_)) | None) | Err(_) => {
                return Err(Error::ServerGone);
            }
        }
    }
}

fn request(stream: &mut UnixStream, command: control::Command) -> Result<()> {
    let caller = std::env::var(control::PANE_ENV).ok().and_then(|id| id.parse().ok());
    let server = std::env::var(control::SERVER_ENV).ok();
    let request =
        serde_json::to_string(&Request { caller, server, command }).map_err(|e| Error::Control(e.to_string()))?;
    protocol::send(stream, &ClientMessage::Request(request))?;
    Ok(())
}

fn answered(text: &str) -> Result<Value> {
    match serde_json::from_str(text) {
        Ok(Response::Ok(value)) => Ok(value),
        Ok(Response::Error(message)) => Err(Error::Control(message)),
        Err(e) => Err(Error::Control(format!("cannot read the server's answer: {e}"))),
    }
}

pub fn follow(
    name: &'static str,
    command: control::Command,
    mut each: impl FnMut(Value) -> Result<bool>,
) -> Result<()> {
    let mut stream = connect()?;
    let reader = stream.try_clone()?;
    ask_on(reader, name, command)?;
    loop {
        match protocol::recv::<ServerMessage>(&mut stream) {
            Ok(Some(ServerMessage::Response(text))) => {
                if each(answered(&text)?)? {
                    return Ok(());
                }
            }
            Ok(Some(ServerMessage::Detached | ServerMessage::Shutdown | ServerMessage::Restart(_))) => return Ok(()),
            Ok(Some(ServerMessage::Frame(_) | ServerMessage::Rejected(_))) => {}
            Ok(None) | Err(_) => return Err(Error::Control(LOST.into())),
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

pub(crate) fn connect_or_start(path: &Path) -> Result<UnixStream> {
    if let Ok(stream) = UnixStream::connect(path) {
        return Ok(stream);
    }
    let log = log::path();
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
    if let Some(dir) = log.parent() {
        fs::create_dir_all(dir)?;
    }
    let log = OpenOptions::new().create(true).append(true).mode(log::PRIVATE).open(log)?;
    log.set_permissions(fs::Permissions::from_mode(log::PRIVATE))?;
    Command::new(std::env::current_exe()?)
        .arg("server")
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log)
        .spawn()
}

pub(crate) fn reap(mut child: Child) {
    thread::spawn(move || child.wait());
}

pub(crate) fn stop(mut child: Child) {
    let _ = child.kill();
    reap(child);
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

enum Incoming {
    Input(Event),
    Server(u64, Received),
    Linked(u64, Result<Link>),
    Signal,
}

enum Received {
    Message(ServerMessage),
    Invalid,
    Ended,
}

enum Next {
    Show(Vec<u8>),
    End(Ending),
    Reconnect,
    Nothing,
}

fn next(received: Received, reconnects: bool) -> Result<Next> {
    Ok(match received {
        Received::Message(ServerMessage::Frame(bytes)) => Next::Show(bytes),
        Received::Message(ServerMessage::Rejected(reason)) => return Err(Error::Rejected(reason)),
        Received::Invalid => return Err(Error::Rejected(INCOMPATIBLE.into())),
        Received::Message(ServerMessage::Restart(_)) | Received::Ended if reconnects => Next::Reconnect,
        Received::Message(ServerMessage::Restart(_)) => Next::End(Ending::Restart),
        Received::Message(ServerMessage::Response(_)) => Next::Nothing,
        Received::Message(ServerMessage::Detached | ServerMessage::Shutdown) | Received::Ended => {
            Next::End(Ending::Detached)
        }
    })
}

fn backoff(failures: u32) -> Duration {
    SECOND.saturating_mul(1 << failures.saturating_sub(1).min(4)).min(MAX_BACKOFF)
}

fn until_next_second(left: Duration) -> Duration {
    match left.subsec_nanos() {
        0 => left.min(SECOND),
        nanos => Duration::from_nanos(nanos.into()),
    }
}

fn seconds(left: Duration) -> u64 {
    u64::try_from(left.as_millis().div_ceil(1000)).unwrap_or(u64::MAX)
}

#[derive(Debug, Default)]
struct Retries {
    failures: u32,
    shown: bool,
}

impl Retries {
    fn linked(&mut self) {
        self.shown = false;
    }

    fn showed(&mut self) {
        self.shown = true;
        self.failures = 0;
    }

    fn lost(&mut self) -> Duration {
        if !self.shown {
            self.failures += 1;
        }
        self.delay()
    }

    fn failed(&mut self) -> Duration {
        self.failures += 1;
        self.delay()
    }

    fn delay(&self) -> Duration {
        if self.failures == 0 { Duration::ZERO } else { backoff(self.failures) }
    }
}

struct Lost {
    retry_at: Option<Instant>,
    error: Option<String>,
    stderr: Option<Stderr>,
    hovered: bool,
}

struct Window<'a> {
    terminal: &'a mut DefaultTerminal,
    target: &'a Target,
    tx: Sender<Incoming>,
    theme: HostTheme,
    notify: Channel,
    name: Option<String>,
    generation: u64,
    out: Option<Sender<ClientMessage>>,
    guard: Option<Guard>,
    lost: Option<Lost>,
    retries: Retries,
}

impl Window<'_> {
    fn link(&mut self, link: Link) -> Result<()> {
        self.generation += 1;
        let Link { reader, writer, guard } = link;
        let size = self.terminal.size()?;
        let out = spawn_writer(writer);
        let (version, build) = (protocol::VERSION, protocol::build_id());
        let (theme, notify, terminal) = (self.theme.clone(), self.notify, self.name.clone());
        let hello = Hello { version, build, cols: size.width, rows: size.height, theme, notify, terminal };
        let _ = out.send(ClientMessage::Hello(Box::new(hello)));
        spawn_reader(self.generation, reader, self.tx.clone());
        self.out = Some(out);
        self.guard = Some(guard);
        self.lost = None;
        self.retries.linked();
        Ok(())
    }

    fn wait(&self, now: Instant) -> Duration {
        match self.lost.as_ref().and_then(|lost| lost.retry_at) {
            Some(at) => until_next_second(at.saturating_duration_since(now)),
            None => Duration::MAX,
        }
    }

    fn handle(&mut self, incoming: Incoming) -> Result<Option<Ending>> {
        match incoming {
            Incoming::Signal => Ok(Some(Ending::Detached)),
            Incoming::Input(ev) => self.input(ev),
            Incoming::Server(generation, received) if generation == self.generation => {
                match next(received, self.target.reconnects())? {
                    Next::Show(bytes) => {
                        self.retries.showed();
                        let mut out = stdout();
                        out.write_all(&bytes)?;
                        out.flush()?;
                    }
                    Next::End(ending) => return Ok(Some(ending)),
                    Next::Reconnect => self.lose()?,
                    Next::Nothing => {}
                }
                Ok(None)
            }
            Incoming::Linked(generation, linked) if generation == self.generation => {
                match linked {
                    Ok(link) => self.link(link)?,
                    Err(e) if remote::fatal(&e) => return Err(e),
                    Err(e) => self.failed(&e)?,
                }
                Ok(None)
            }
            Incoming::Server(..) | Incoming::Linked(..) => Ok(None),
        }
    }

    fn input(&mut self, ev: Event) -> Result<Option<Ending>> {
        if let Some(out) = &self.out {
            let _ = out.send(ClientMessage::Event(ev));
            return Ok(None);
        }
        let screen = self.screen()?;
        let Some(lost) = &mut self.lost else { return Ok(None) };
        match ev {
            Event::Key(key) if key.code == KeyCode::Esc && key.kind != KeyEventKind::Release => {
                return Ok(Some(Ending::GaveUp));
            }
            Event::Mouse(mouse) => {
                let on = notice::hits_quit(screen, Position::new(mouse.column, mouse.row));
                if on && mouse.kind == MouseEventKind::Down(MouseButton::Left) {
                    return Ok(Some(Ending::GaveUp));
                }
                if on != lost.hovered {
                    lost.hovered = on;
                    self.show_notice()?;
                }
            }
            Event::Resize(..) => {
                self.terminal.clear()?;
                self.show_notice()?;
            }
            _ => {}
        }
        Ok(None)
    }

    fn tick(&mut self, now: Instant) -> Result<()> {
        let Some(lost) = &mut self.lost else { return Ok(()) };
        if lost.retry_at.is_some_and(|at| at <= now) {
            lost.retry_at = None;
            let (target, tx, generation) = (self.target.clone(), self.tx.clone(), self.generation);
            thread::spawn(move || {
                let _ = tx.send(Incoming::Linked(generation, target.connect(true)));
            });
        }
        self.show_notice()
    }

    fn lose(&mut self) -> Result<()> {
        self.generation += 1;
        self.out = None;
        let stderr = self.guard.take().and_then(|mut guard| guard.stderr.take());
        let retry_at = Some(Instant::now() + self.retries.lost());
        self.lost = Some(Lost { retry_at, error: None, stderr, hovered: false });
        self.tick(Instant::now())
    }

    fn failed(&mut self, error: &Error) -> Result<()> {
        let Some(lost) = &mut self.lost else { return Ok(()) };
        lost.retry_at = Some(Instant::now() + self.retries.failed());
        lost.error = Some(remote::reason(error));
        self.show_notice()
    }

    fn screen(&self) -> Result<Rect> {
        let size = self.terminal.size()?;
        Ok(Rect::new(0, 0, size.width, size.height))
    }

    fn show_notice(&mut self) -> Result<()> {
        let screen = self.screen()?;
        let Some(lost) = &self.lost else { return Ok(()) };
        let error = lost.error.clone().or_else(|| lost.stderr.as_ref().and_then(Stderr::last));
        let retry_in = lost.retry_at.map(|at| seconds(at.saturating_duration_since(Instant::now())));
        let shown = Notice { host: self.target.host(), retry_in, error: error.as_deref(), hovered: lost.hovered };
        let mut buf = Buffer::empty(screen);
        notice::draw(&mut buf, &self.theme, &shown);
        let r = notice::area(screen);
        let cells = buf.content.iter().enumerate().filter_map(|(i, cell)| {
            let (x, y) = buf.pos_of(i);
            r.contains(Position::new(x, y)).then_some((x, y, cell))
        });
        let backend = self.terminal.backend_mut();
        backend.draw(cells)?;
        backend.hide_cursor()?;
        Backend::flush(backend)?;
        Ok(())
    }
}

fn attach(link: Link, target: &Target, terminal: &mut DefaultTerminal) -> Result<Ending> {
    let (theme, name) = query_host();
    let theme = HostTheme { truecolor: truecolor(), ..theme };
    let notify = notify::detect(name.as_deref(), |var| std::env::var(var).ok());
    let (tx, rx) = mpsc::channel();
    let mut window = Window {
        terminal,
        target,
        tx: tx.clone(),
        theme,
        notify,
        name,
        generation: 0,
        out: None,
        guard: None,
        lost: None,
        retries: Retries::default(),
    };
    window.link(link)?;
    let _threads = Threads::spawn(tx)?;
    loop {
        let ending = match rx.recv_timeout(window.wait(Instant::now())) {
            Ok(incoming) => window.handle(incoming)?,
            Err(RecvTimeoutError::Timeout) => {
                window.tick(Instant::now())?;
                None
            }
            Err(RecvTimeoutError::Disconnected) => Some(Ending::Detached),
        };
        if let Some(ending) = ending {
            return Ok(ending);
        }
    }
}

fn spawn_writer(mut writer: Box<dyn Write + Send>) -> Sender<ClientMessage> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        for msg in rx {
            if protocol::send(&mut writer, &msg).is_err() {
                return;
            }
        }
    });
    tx
}

fn spawn_reader(generation: u64, mut reader: Box<dyn Read + Send>, tx: Sender<Incoming>) {
    thread::spawn(move || {
        loop {
            let received = match protocol::recv::<ServerMessage>(&mut reader) {
                Ok(Some(msg)) => Received::Message(msg),
                Err(e) if e.kind() == io::ErrorKind::InvalidData => Received::Invalid,
                Ok(None) | Err(_) => Received::Ended,
            };
            let last = !matches!(received, Received::Message(_));
            if tx.send(Incoming::Server(generation, received)).is_err() || last {
                return;
            }
        }
    });
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

struct Threads {
    stop: Arc<AtomicBool>,
    stopped: Receiver<()>,
    signals: Handle,
}

impl Threads {
    fn spawn(tx: Sender<Incoming>) -> Result<Self> {
        let mut signals = Signals::new([SIGTERM, SIGHUP, SIGINT]).map_err(Error::Signals)?;
        let handle = signals.handle();
        let signalled = tx.clone();
        thread::spawn(move || {
            if signals.forever().next().is_some() {
                let _ = signalled.send(Incoming::Signal);
            }
        });
        let stop = Arc::new(AtomicBool::new(false));
        let (done, stopped) = mpsc::channel();
        let asked = Arc::clone(&stop);
        thread::spawn(move || {
            let _done = done;
            while !asked.load(Ordering::Relaxed) {
                match event::poll(INPUT_POLL) {
                    Ok(false) => {}
                    Ok(true) => {
                        let Ok(ev) = event::read() else { return };
                        if tx.send(Incoming::Input(ev)).is_err() {
                            return;
                        }
                    }
                    Err(_) => return,
                }
            }
        });
        Ok(Self { stop, stopped, signals: handle })
    }
}

impl Drop for Threads {
    fn drop(&mut self) {
        self.signals.close();
        self.stop.store(true, Ordering::Relaxed);
        let _ = self.stopped.recv_timeout(INPUT_POLL * 3);
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    mod restart {
        use super::*;

        fn restart() -> Received {
            Received::Message(ServerMessage::Restart(PathBuf::from("/tmp/not-cornercase")))
        }

        #[test]
        fn starts_this_binary_again_and_never_the_path_the_server_sends() {
            assert!(matches!(next(restart(), false), Ok(Next::End(Ending::Restart))));
        }

        #[test]
        fn reconnects_to_a_remote_server() {
            assert!(matches!(next(restart(), true), Ok(Next::Reconnect)));
        }
    }

    mod connection {
        use super::*;

        #[test]
        fn a_local_server_that_goes_away_ends_the_window() {
            assert!(matches!(next(Received::Ended, false), Ok(Next::End(Ending::Detached))));
        }

        #[test]
        fn a_remote_one_is_reconnected() {
            assert!(matches!(next(Received::Ended, true), Ok(Next::Reconnect)));
        }

        #[test]
        fn quit_ends_a_remote_window_too() {
            assert!(matches!(next(Received::Message(ServerMessage::Detached), true), Ok(Next::End(Ending::Detached))));
        }

        #[test]
        fn a_rejection_is_never_retried() {
            assert!(next(Received::Message(ServerMessage::Rejected("no".into())), true).is_err());
        }
    }

    mod reconnecting {
        use super::*;

        #[test]
        fn waits_longer_after_each_failure_up_to_a_limit() {
            let waits: Vec<u64> = (1..=7).map(|n| backoff(n).as_secs()).collect();

            assert_eq!(waits, [1, 2, 4, 8, 15, 15, 15]);
        }

        #[test]
        fn a_link_that_showed_the_screen_reconnects_at_once() {
            let mut retries = Retries::default();
            retries.linked();
            retries.showed();

            assert_eq!(retries.lost(), Duration::ZERO);
        }

        #[test]
        fn a_link_that_ends_before_its_first_frame_waits_longer_each_time() {
            let mut retries = Retries::default();
            let waits: Vec<Duration> = (0..3)
                .map(|_| {
                    retries.linked();
                    retries.lost()
                })
                .collect();

            assert_eq!(waits, [SECOND, SECOND * 2, SECOND * 4]);
        }

        #[test]
        fn failed_attempts_and_early_ends_add_up() {
            let mut retries = Retries::default();
            retries.linked();
            retries.showed();
            retries.lost();
            retries.failed();
            retries.linked();

            assert_eq!(retries.lost(), SECOND * 2);
        }

        #[test]
        fn counts_down_in_whole_seconds() {
            assert_eq!(seconds(Duration::from_millis(3400)), 4);
            assert_eq!(seconds(Duration::ZERO), 0);
            assert_eq!(until_next_second(Duration::from_millis(3400)), Duration::from_millis(400));
            assert_eq!(until_next_second(Duration::from_secs(3)), SECOND);
        }
    }
}
