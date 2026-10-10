use std::fs::File;
use std::io::{self, Write};
use std::net::Shutdown;
use std::ops::ControlFlow;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime};

use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseEventKind};
use ratatui::backend::{Backend, ClearType, CrosstermBackend, WindowSize};
use ratatui::buffer::Cell;
use ratatui::layout::{Position, Rect, Size};
use ratatui::{Terminal, TerminalOptions, Viewport};
use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};
use signal_hook::iterator::Signals;

use crate::app::{App, AppEvent, Streamed};
use crate::config;
use crate::control::{self, Ids, What};
use crate::error::{Error, Result};
use crate::host_theme::HostTheme;
use crate::log::{self, Level};
use crate::notify::Channel;
use crate::panics;
use crate::protocol::{self, ClientMessage, Hello, ServerMessage};
use crate::state::{self, Saver};
use crate::todo;

mod images;

use images::Graphics;

const TICK: Duration = Duration::from_millis(500);
const SLOW_STEP: Duration = Duration::from_millis(100);
const FRAME: Duration = Duration::from_millis(16);
const SAVE_EVERY: Duration = Duration::from_millis(500);
const ISSUE_CACHE_FILE: &str = "issues.json";
const WRITE_TIMEOUT: Duration = Duration::from_secs(1);
const EVENTS_QUEUED: usize = 1024;
const CLEAR_SCREEN: &[u8] = b"\x1b[H\x1b[2J";
const OTHER_BUILD: &str = "the running cornercase server comes from another build. \
    Run `cornercase kill-server` (it closes all its terminals) and start cornercase again";

enum ServerEvent {
    App(AppEvent),
    Accepted(UnixStream),
    Message(u64, ClientMessage),
    Incompatible(u64),
    Gone(u64),
    Shutdown,
}

enum Step {
    Refresh,
    Answer,
    Flush,
    Draw,
    Save,
    Event(ServerEvent),
}

impl Step {
    fn name(&self) -> &'static str {
        match self {
            Self::Refresh => "refresh",
            Self::Answer => "answer",
            Self::Flush => "flush",
            Self::Draw => "draw",
            Self::Save => "save",
            Self::Event(ServerEvent::App(AppEvent::Output(..))) => "output",
            Self::Event(ServerEvent::App(_)) => "job answer",
            Self::Event(ServerEvent::Accepted(_)) => "connection",
            Self::Event(ServerEvent::Message(_, ClientMessage::Event(_))) => "input",
            Self::Event(ServerEvent::Message(_, ClientMessage::Request(_))) => "request",
            Self::Event(ServerEvent::Message(..)) => "message",
            Self::Event(ServerEvent::Incompatible(_) | ServerEvent::Gone(_)) => "client gone",
            Self::Event(ServerEvent::Shutdown) => "signal",
        }
    }

    fn traced(&self) -> bool {
        !matches!(
            self,
            Self::Answer | Self::Flush | Self::Draw | Self::Save | Self::Event(ServerEvent::App(AppEvent::Output(..)))
        )
    }
}

#[derive(Clone)]
struct Outbox {
    tx: Sender<ServerMessage>,
    queued: Arc<AtomicUsize>,
}

impl Outbox {
    fn send(&self, msg: ServerMessage) {
        self.queued.fetch_add(1, Ordering::Relaxed);
        if self.tx.send(msg).is_err() {
            self.queued.fetch_sub(1, Ordering::Relaxed);
        }
    }

    fn queued(&self) -> usize {
        self.queued.load(Ordering::Relaxed)
    }
}

struct FrameWriter {
    buf: Vec<u8>,
    out: Outbox,
}

impl Write for FrameWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.buf.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        if !self.buf.is_empty() {
            self.out.send(ServerMessage::Frame(std::mem::take(&mut self.buf)));
        }
        Ok(())
    }
}

struct CropBackend {
    inner: CrosstermBackend<FrameWriter>,
    area: Rect,
    visible: Rect,
    watched: Option<Rect>,
    damaged: bool,
}

impl Backend for CropBackend {
    type Error = io::Error;

    fn draw<'a, I>(&mut self, content: I) -> io::Result<()>
    where
        I: Iterator<Item = (u16, u16, &'a Cell)>,
    {
        let (visible, watched) = (self.visible, self.watched);
        let mut damaged = false;
        let shown = content.filter(|&(x, y, _)| visible.contains(Position::new(x, y)));
        let drawn = self.inner.draw(shown.inspect(|&(x, y, _)| {
            damaged |= watched.is_some_and(|r| r.contains(Position::new(x, y)));
        }));
        self.damaged |= damaged;
        drawn
    }

    fn hide_cursor(&mut self) -> io::Result<()> {
        self.inner.hide_cursor()
    }

    fn show_cursor(&mut self) -> io::Result<()> {
        self.inner.show_cursor()
    }

    fn get_cursor_position(&mut self) -> io::Result<Position> {
        self.inner.get_cursor_position()
    }

    fn set_cursor_position<P: Into<Position>>(&mut self, position: P) -> io::Result<()> {
        let position = position.into();
        if !self.visible.contains(position) {
            return self.inner.hide_cursor();
        }
        self.inner.set_cursor_position(position)
    }

    fn clear(&mut self) -> io::Result<()> {
        self.inner.clear()
    }

    fn clear_region(&mut self, clear_type: ClearType) -> io::Result<()> {
        self.inner.clear_region(clear_type)
    }

    fn size(&self) -> io::Result<Size> {
        Ok(self.area.as_size())
    }

    fn window_size(&mut self) -> io::Result<WindowSize> {
        Ok(WindowSize { columns_rows: self.area.as_size(), pixels: Size::default() })
    }

    fn flush(&mut self) -> io::Result<()> {
        Backend::flush(&mut self.inner)
    }
}

type Screen = Terminal<CropBackend>;

struct Client {
    id: u64,
    size: Option<(u16, u16)>,
    used: u64,
    notify: Channel,
    graphics: Graphics,
    screen: Option<Screen>,
    out: Outbox,
    writer: JoinHandle<()>,
    dropped: u64,
}

impl Client {
    fn send(&self, msg: ServerMessage) {
        self.out.send(msg);
    }

    fn stream(&mut self, text: String) {
        self.catch_up();
        if self.dropped > 0 || self.out.queued() >= EVENTS_QUEUED {
            self.dropped += 1;
        } else {
            self.send(ServerMessage::Response(text));
        }
    }

    fn catch_up(&mut self) {
        if self.dropped == 0 || self.out.queued() >= EVENTS_QUEUED {
            return;
        }
        log::warning!("server", "events dropped", client = self.id, count = self.dropped);
        let time = log::Stamp(SystemTime::now()).to_string();
        let notice = control::Event { time, what: What::Dropped { count: self.dropped }, ids: Ids::default() };
        if let Some(text) = notice.streamed() {
            self.send(ServerMessage::Response(text));
        }
        self.dropped = 0;
    }

    fn reset_screen(&mut self, area: Rect) {
        let Some((width, height)) = self.size else { return };
        self.send(ServerMessage::Frame(CLEAR_SCREEN.to_vec()));
        self.graphics.reset(Instant::now());
        let inner = CrosstermBackend::new(FrameWriter { buf: Vec::new(), out: self.out.clone() });
        let visible = Rect::new(0, 0, width, height);
        let backend = CropBackend { inner, area, visible, watched: None, damaged: false };
        self.screen = Terminal::with_options(backend, TerminalOptions { viewport: Viewport::Fixed(area) }).ok();
    }

    fn draw(&mut self, app: &mut App, picture: Option<u64>, now: Instant) {
        let (Some(screen), Some((cols, rows))) = (self.screen.as_mut(), self.size) else { return };
        let sight = self.graphics.sight(Rect::new(0, 0, cols, rows), picture);
        screen.backend_mut().watched = self.graphics.watched();
        let mut placed = None;
        let _ = screen.draw(|f| placed = app.draw(f, &sight));
        if std::mem::take(&mut screen.backend_mut().damaged) {
            self.graphics.damaged();
        }
        let bytes = self.graphics.after(picture, placed.as_ref(), now);
        if !bytes.is_empty() {
            self.send(ServerMessage::Frame(bytes));
        }
    }
}

struct Server {
    app: App,
    clients: Vec<Client>,
    area: Option<Rect>,
    next_client: u64,
    uses: u64,
    started: bool,
    build: String,
    session: PathBuf,
    saver: Saver,
    todo_saver: Saver<todo::Saved>,
    restart: Option<PathBuf>,
    tx: Sender<ServerEvent>,
    drawn: Option<Instant>,
    printed_at: Option<Instant>,
    changed: bool,
    printed: bool,
    observed: Option<Instant>,
}

pub fn run() -> Result<()> {
    panics::log_to_stderr();
    let _ = rustix::process::setsid();
    let path = protocol::socket_path();
    let (listener, _lock) = bind(&path)?;
    let level = log::level_from_env();
    if let Err(e) = log::start(&log::path(), level) {
        eprintln!("cornercase server: cannot write its log to {}: {e}", log::path().display());
    }
    let (tx, rx) = mpsc::channel();
    spawn_acceptor(listener, tx.clone());
    spawn_signal_thread(tx.clone())?;
    let (app_tx, app_rx) = mpsc::channel();
    spawn_forwarder(app_rx, tx.clone());

    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
    let mut app = App::new(shell, HostTheme::default(), config::path(), app_tx);
    let session = state::path();
    log::info!(
        "server",
        "started",
        version = crate::update::CURRENT,
        build = protocol::build_id(),
        pid = std::process::id(),
        os = std::env::consts::OS,
        arch = std::env::consts::ARCH,
        socket = path.display(),
        session = session.display(),
        level = format!("{level:?}").to_lowercase(),
    );
    app.set_issue_cache(session.with_file_name(ISSUE_CACHE_FILE));
    let todos = todo::load(&todo::path(&session));
    let todo_saver = Saver::new(todo::path(&session), Some(todos.saved()));
    app.set_todos(todos);
    let mut server = Server::new(app, session, todo_saver, tx);
    server.serve(&rx);
    let _ = std::fs::remove_file(&path);
    server.shutdown();
    log::info!("server", "stopped", restart = server.restart.is_some());
    Ok(())
}

fn bind(path: &Path) -> Result<(UnixListener, File)> {
    let listen_error = |source| Error::Listen { path: path.to_path_buf(), source };
    protocol::check_socket_dir(path)?;
    let lock = lock(path)?;
    let _ = std::fs::remove_file(path);
    Ok((UnixListener::bind(path).map_err(listen_error)?, lock))
}

fn lock(path: &Path) -> Result<File> {
    protocol::try_lock(path)
        .map_err(|source| Error::Listen { path: path.to_path_buf(), source })?
        .ok_or_else(|| Error::ServerRunning(path.to_path_buf()))
}

fn spawn_acceptor(listener: UnixListener, tx: Sender<ServerEvent>) {
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            if tx.send(ServerEvent::Accepted(stream)).is_err() {
                return;
            }
        }
    });
}

fn spawn_signal_thread(tx: Sender<ServerEvent>) -> Result<()> {
    let mut signals = Signals::new([SIGTERM, SIGINT, SIGHUP]).map_err(Error::Signals)?;
    thread::spawn(move || {
        if signals.forever().any(|signal| signal != SIGHUP) {
            let _ = tx.send(ServerEvent::Shutdown);
        }
    });
    Ok(())
}

fn spawn_forwarder(app_rx: Receiver<AppEvent>, tx: Sender<ServerEvent>) {
    thread::spawn(move || {
        for ev in app_rx {
            if tx.send(ServerEvent::App(ev)).is_err() {
                return;
            }
        }
    });
}

fn spawn_client_writer(
    mut stream: UnixStream,
    rx: Receiver<ServerMessage>,
    queued: Arc<AtomicUsize>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        for msg in rx {
            let sent = protocol::send(&mut stream, &msg);
            queued.fetch_sub(1, Ordering::Relaxed);
            if sent.is_err() {
                break;
            }
        }
        let _ = stream.shutdown(Shutdown::Both);
    })
}

fn spawn_client_reader(id: u64, mut stream: UnixStream, tx: Sender<ServerEvent>) {
    thread::spawn(move || {
        let last = loop {
            match protocol::recv::<ClientMessage>(&mut stream) {
                Ok(Some(msg)) => {
                    if tx.send(ServerEvent::Message(id, msg)).is_err() {
                        return;
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::InvalidData => break ServerEvent::Incompatible(id),
                Ok(None) | Err(_) => break ServerEvent::Gone(id),
            }
        };
        let _ = tx.send(last);
    });
}

impl Server {
    fn new(app: App, session: PathBuf, todo_saver: Saver<todo::Saved>, tx: Sender<ServerEvent>) -> Self {
        Self {
            app,
            clients: Vec::new(),
            area: None,
            next_client: 1,
            uses: 0,
            started: false,
            build: protocol::build_id(),
            saver: Saver::new(session.clone(), None),
            session,
            todo_saver,
            restart: None,
            tx,
            drawn: None,
            printed_at: None,
            changed: true,
            printed: false,
            observed: None,
        }
    }

    fn serve(&mut self, rx: &Receiver<ServerEvent>) {
        self.serve_with(rx, Self::step);
    }

    fn serve_with(&mut self, rx: &Receiver<ServerEvent>, run: impl Fn(&mut Self, Step) -> ControlFlow<()>) {
        loop {
            for step in [Step::Refresh, Step::Answer, Step::Flush] {
                if self.contain(step, &run) == Some(ControlFlow::Break(())) {
                    return;
                }
            }
            if self.due(Instant::now()) {
                self.contain(Step::Draw, &run);
            }
            self.contain(Step::Save, &run);
            let now = Instant::now();
            let tick = [self.app.tick(now), self.next_frame(now)].into_iter().flatten().fold(TICK, Duration::min);
            let first = match rx.recv_timeout(tick) {
                Ok(ev) => Some(ev),
                Err(RecvTimeoutError::Timeout) => {
                    self.changed = true;
                    None
                }
                Err(RecvTimeoutError::Disconnected) => return,
            };
            for ev in first.into_iter().chain(std::iter::from_fn(|| rx.try_recv().ok())) {
                if self.contain(Step::Event(ev), &run) == Some(ControlFlow::Break(())) {
                    return;
                }
            }
        }
    }

    fn step(&mut self, step: Step) -> ControlFlow<()> {
        let traced = step.traced();
        let flow = self.run_step(step);
        if traced {
            self.app.trace();
        }
        if self.restart.is_none()
            && let Some(exe) = self.app.take_restart()
        {
            log::info!("server", "restart once no agent is at work");
            self.restart = Some(exe);
        }
        if self.restart.is_some() { ControlFlow::Break(()) } else { flow }
    }

    fn run_step(&mut self, step: Step) -> ControlFlow<()> {
        match step {
            Step::Refresh => self.app.refresh(Instant::now()),
            Step::Answer => self.answer(),
            Step::Flush => self.flush(),
            Step::Draw => self.draw(),
            Step::Save => self.save(Instant::now()),
            Step::Event(ev) => return self.handle(ev),
        }
        ControlFlow::Continue(())
    }

    fn contain(&mut self, step: Step, run: impl Fn(&mut Self, Step) -> ControlFlow<()>) -> Option<ControlFlow<()>> {
        let drawing = matches!(step, Step::Draw);
        let interacting = drawing || matches!(step, Step::Event(ServerEvent::Message(_, ClientMessage::Event(_))));
        let name = step.name();
        let started = Instant::now();
        let done = panics::contain(|| run(self, step));
        let took = started.elapsed();
        if took >= SLOW_STEP {
            log::warning!("server", "slow step", step = name, ms = took.as_millis());
        } else if drawing && took >= FRAME {
            log::debug!("server", "slow frame", ms = took.as_millis());
        }
        if done.is_none() {
            log::error!("server", "step panicked", step = name, interacting = interacting);
            if interacting {
                self.app.reset_interaction();
            }
            if drawing && let Some(area) = self.area {
                self.changed = true;
                for client in &mut self.clients {
                    client.reset_screen(area);
                }
            }
            self.app.answer_lost_requests();
            self.app.report_bug();
        }
        done
    }

    fn answer(&mut self) {
        for (id, text) in self.app.take_answers() {
            if let Some(client) = self.clients.iter().find(|c| c.id == id) {
                client.send(ServerMessage::Response(text));
            }
        }
        let mut ended = Vec::new();
        for (id, streamed) in self.app.take_events() {
            match (self.client_mut(id), streamed) {
                (Some(client), Streamed::Event(text)) => client.stream(text),
                (Some(_), Streamed::End) => ended.push(id),
                (None, _) => {}
            }
        }
        self.clients.iter_mut().for_each(Client::catch_up);
        for id in ended {
            if let Some(client) = self.client_mut(id) {
                client.send(ServerMessage::Detached);
            }
            self.remove(id);
        }
    }

    fn save(&mut self, now: Instant) {
        if self.observed.is_some_and(|at| now.saturating_duration_since(at) < SAVE_EVERY) {
            return;
        }
        self.observed = Some(now);
        if self.started {
            match self.saver.observe(self.app.state(), now) {
                Ok(true) => log::debug!("server", "session saved", path = self.session.display()),
                Ok(false) => {}
                Err(e) => log::error!("server", "failed to save the session", error = e),
            }
        }
        match self.todo_saver.observe(self.app.todos_saved(), now) {
            Ok(true) => log::debug!("server", "todo list saved"),
            Ok(false) => {}
            Err(e) => log::error!("server", "failed to save the todo list", error = e),
        }
    }

    fn flush(&mut self) {
        let Self { app, clients, .. } = self;
        for bytes in app.take_host_writes() {
            for client in clients.iter().filter(|c| c.screen.is_some()) {
                client.send(ServerMessage::Frame(bytes.clone()));
            }
        }
        for notification in app.take_notifications() {
            for client in clients.iter().filter(|c| c.screen.is_some()) {
                log::info!("server", "notification sent", client = client.id, channel = client.notify.id());
                client.send(ServerMessage::Frame(notification.encode(client.notify)));
            }
        }
    }

    fn due(&self, now: Instant) -> bool {
        let since = |at: Option<Instant>| at.map_or(Duration::MAX, |at| now.saturating_duration_since(at));
        self.changed || since(self.drawn) >= TICK || (self.printed && since(self.printed_at) >= FRAME)
    }

    fn next_frame(&self, now: Instant) -> Option<Duration> {
        let images = self.clients.iter().filter_map(|c| c.graphics.wake(now)).min();
        let at = self.printed_at.filter(|_| self.printed);
        let printed = at.map(|at| FRAME.saturating_sub(now.saturating_duration_since(at)));
        printed.into_iter().chain(images).min()
    }

    fn draw(&mut self) {
        let now = Instant::now();
        self.drawn = Some(now);
        if self.printed {
            self.printed_at = Some(now);
        }
        self.changed = false;
        self.printed = false;
        let Self { app, clients, area, .. } = self;
        let Some(area) = *area else { return };
        app.resize(area);
        let picture = app.picture();
        for client in clients.iter_mut() {
            client.draw(app, picture, now);
        }
    }

    fn handle(&mut self, ev: ServerEvent) -> ControlFlow<()> {
        if log::enabled(Level::Debug) {
            describe(&ev);
        }
        match &ev {
            ServerEvent::App(AppEvent::Output(id, _)) => self.printed = self.printed || self.app.shows(*id),
            _ => self.changed = true,
        }
        match ev {
            ServerEvent::Message(id, ClientMessage::KillServer) => {
                log::info!("server", "kill-server received", client = id);
                return ControlFlow::Break(());
            }
            ServerEvent::Shutdown => {
                log::info!("server", "stopping on a signal");
                return ControlFlow::Break(());
            }
            ServerEvent::App(ev) => self.handle_app(ev),
            ServerEvent::Accepted(stream) => self.accept(stream),
            ServerEvent::Message(id, ClientMessage::Hello(hello)) => self.hello(id, *hello),
            ServerEvent::Message(id, ClientMessage::Event(ev)) => self.input(id, ev),
            ServerEvent::Message(id, ClientMessage::Restart) => {
                log::info!("server", "restart requested", client = id);
                self.restart = Some(std::env::current_exe().unwrap_or_default());
            }
            ServerEvent::Message(id, ClientMessage::Request(text)) => {
                self.app.request(id, &text, self.area, Instant::now());
            }
            ServerEvent::Message(id, ClientMessage::Graphics(graphics)) => {
                log::info!(
                    "server",
                    "client images changed",
                    client = id,
                    images = graphics.images(),
                    cell = graphics.cell_size(),
                    missing = graphics.why_not()
                );
                if let Some(client) = self.client_mut(id) {
                    client.graphics.update(graphics);
                }
            }
            ServerEvent::Incompatible(id) => self.reject(id, OTHER_BUILD),
            ServerEvent::Gone(id) => self.remove(id),
        }
        if self.restart.is_some() {
            return ControlFlow::Break(());
        }
        ControlFlow::Continue(())
    }

    fn handle_app(&mut self, ev: AppEvent) {
        if let Err(e) = self.app.handle_event(ev, self.area.unwrap_or_default()) {
            log::error!("app", "an event failed", error = e);
        }
    }

    fn accept(&mut self, stream: UnixStream) {
        if let Err(e) = protocol::check_peer(&stream, protocol::own_uid()) {
            log::warning!("server", "connection refused", error = e);
            return;
        }
        let Ok(reader) = stream.try_clone() else { return };
        let _ = stream.set_write_timeout(Some(WRITE_TIMEOUT));
        let id = self.next_client;
        self.next_client += 1;
        let (tx, out_rx) = mpsc::channel();
        let out = Outbox { tx, queued: Arc::new(AtomicUsize::new(0)) };
        let writer = spawn_client_writer(stream, out_rx, Arc::clone(&out.queued));
        spawn_client_reader(id, reader, self.tx.clone());
        let (notify, graphics) = (Channel::Bell, Graphics::default());
        self.clients.push(Client { id, size: None, used: 0, notify, graphics, screen: None, out, writer, dropped: 0 });
    }

    fn client_mut(&mut self, id: u64) -> Option<&mut Client> {
        self.clients.iter_mut().find(|c| c.id == id)
    }

    fn hello(&mut self, id: u64, hello: Hello) {
        if hello.version != protocol::VERSION || hello.build != self.build {
            log::warning!("server", "client from another build", client = id, build = hello.build);
            self.reject(id, OTHER_BUILD);
            return;
        }
        log::info!(
            "server",
            "client attached",
            client = id,
            cols = hello.cols,
            rows = hello.rows,
            terminal = hello.terminal.as_deref().unwrap_or("unknown"),
            notify = hello.notify.id(),
            background = if hello.theme.background.is_some() { "known" } else { "unknown" },
            truecolor = hello.theme.truecolor,
            images = hello.graphics.images(),
            cell = hello.graphics.cell_size(),
            tmux = hello.graphics.tmux.id(),
            missing = hello.graphics.why_not(),
            id_hi = hello.graphics.id_hi,
            probe = hello.probe,
        );
        let Some(client) = self.client_mut(id) else { return };
        client.size = Some((hello.cols, hello.rows));
        client.notify = hello.notify;
        let background = hello.theme.background.map(|c| (c.r, c.g, c.b));
        client.graphics = Graphics::new(id, hello.graphics, background);
        self.touch(id);
        self.fit(Some(id));
        if !self.started {
            self.started = true;
            self.app.set_theme(hello.theme);
            if let Err(e) = self.open_first_terminals() {
                log::error!("server", "could not open a first terminal", error = e);
            }
        }
    }

    fn open_first_terminals(&mut self) -> Result<()> {
        let area = self.area.unwrap_or_default();
        let saved = state::load(&self.session);
        let complete = saved.as_ref().is_none_or(|saved| self.app.restore(saved, area));
        match &saved {
            Some(saved) => log::info!(
                "server",
                "session restored",
                projects = saved.projects.len(),
                complete = complete,
                path = self.session.display()
            ),
            None => log::info!("server", "no session to restore", path = self.session.display()),
        }
        if !complete {
            match state::back_up(&self.session) {
                Ok(backup) => log::warning!("server", "the session came back incomplete", copy = backup.display()),
                Err(e) => log::error!("server", "failed to keep a copy of the session", error = e),
            }
        }
        self.saver = Saver::new(self.session.clone(), saved);
        if self.app.is_empty() || !(complete || self.app.has_terms()) {
            self.app.open_here(area)?;
        }
        Ok(())
    }

    fn input(&mut self, id: u64, ev: Event) {
        let Some(client) = self.client_mut(id).filter(|c| c.size.is_some()) else { return };
        if let Event::Resize(cols, rows) = ev {
            client.size = Some((cols, rows));
            self.fit(Some(id));
            return;
        }
        if is_use(&ev) {
            self.touch(id);
            self.fit(None);
        }
        let Some(area) = self.area else { return };
        if let Err(e) = self.app.handle_event(AppEvent::Input(ev), area) {
            log::warning!("app", "input failed", error = e);
        }
        self.restart = self.app.take_restart();
        if self.restart.is_some() {
            log::info!("server", "restart chosen in the window", client = id);
        }
        if self.app.take_detach() {
            log::info!("server", "client detached", client = id);
            if let Some(client) = self.client_mut(id) {
                client.send(ServerMessage::Detached);
            }
            self.remove(id);
        }
    }

    fn touch(&mut self, id: u64) {
        self.uses += 1;
        let uses = self.uses;
        if let Some(client) = self.client_mut(id) {
            client.used = uses;
        }
    }

    fn fit(&mut self, resized: Option<u64>) {
        let latest = self.clients.iter().filter(|c| c.size.is_some()).max_by_key(|c| c.used).and_then(|c| c.size);
        let Some((cols, rows)) = latest else { return };
        let area = Rect::new(0, 0, cols, rows);
        let area_changed = self.area != Some(area);
        if area_changed {
            log::info!("server", "size", cols = cols, rows = rows);
        }
        self.area = Some(area);
        for client in self.clients.iter_mut().filter(|c| area_changed || Some(c.id) == resized) {
            client.reset_screen(area);
        }
    }

    fn reject(&mut self, id: u64, reason: &str) {
        log::warning!("server", "client rejected", client = id);
        if let Some(client) = self.client_mut(id) {
            client.send(ServerMessage::Rejected(reason.into()));
        }
        self.remove(id);
    }

    fn remove(&mut self, id: u64) {
        if let Some(client) = self.clients.iter().find(|c| c.id == id) {
            let level = if client.size.is_some() { Level::Info } else { Level::Debug };
            log::event!(level, "server", "client gone", client = id);
        }
        self.app.forget(id);
        let before = self.clients.len();
        self.clients.retain(|c| c.id != id);
        if self.clients.len() != before {
            self.fit(None);
        }
    }

    fn shutdown(&mut self) {
        self.answer();
        if let Err(e) =
            self.todo_saver.observe(self.app.todos_saved(), Instant::now()).and_then(|_| self.todo_saver.flush())
        {
            log::error!("server", "failed to save the todo list", error = e);
        }
        if self.restart.is_some()
            && self.started
            && let Err(e) = state::save(&self.session, &self.app.state())
        {
            log::error!("server", "failed to save the session", error = e);
        }
        for client in self.clients.drain(..) {
            client.send(self.restart.clone().map_or(ServerMessage::Shutdown, ServerMessage::Restart));
            let Client { out, screen, writer, .. } = client;
            drop((out, screen));
            let _ = writer.join();
        }
    }
}

fn describe(ev: &ServerEvent) {
    match ev {
        ServerEvent::App(AppEvent::Output(..)) => {}
        ServerEvent::App(ev) => log::debug!("server", "job answered", job = ev.name()),
        ServerEvent::Accepted(_) => log::debug!("server", "connection"),
        ServerEvent::Message(id, ClientMessage::Event(ev)) => {
            if let Some(input) = input(ev) {
                log::debug!("server", "input", client = id, event = input);
            }
        }
        ServerEvent::Message(id, ClientMessage::Request(_)) => log::debug!("server", "request", client = id),
        ServerEvent::Message(id, ClientMessage::Hello(_)) => log::debug!("server", "hello", client = id),
        ServerEvent::Message(..) | ServerEvent::Incompatible(_) | ServerEvent::Gone(_) | ServerEvent::Shutdown => {}
    }
}

fn input(ev: &Event) -> Option<String> {
    let with = |modifiers: KeyModifiers| {
        if modifiers.is_empty() { String::new() } else { format!(" with {modifiers}") }
    };
    Some(match ev {
        Event::Key(key) => {
            let code = match key.code {
                KeyCode::Char(_) => "a character".to_string(),
                code => code.to_string(),
            };
            let kind = if key.kind == KeyEventKind::Press { String::new() } else { format!(" {:?}", key.kind) };
            format!("key {code}{}{kind}", with(key.modifiers))
        }
        Event::Mouse(mouse) if mouse.kind == MouseEventKind::Moved => return None,
        Event::Mouse(mouse) => {
            format!("mouse {:?} at {},{}{}", mouse.kind, mouse.column, mouse.row, with(mouse.modifiers))
        }
        Event::Paste(text) => format!("paste of {} bytes", text.len()),
        Event::Resize(cols, rows) => format!("resize {cols}x{rows}"),
        Event::FocusGained => "focus gained".into(),
        Event::FocusLost => "focus lost".into(),
    })
}

fn is_use(ev: &Event) -> bool {
    match ev {
        Event::Key(_) | Event::Paste(_) => true,
        Event::Mouse(mouse) => mouse.kind != MouseEventKind::Moved,
        Event::FocusGained | Event::FocusLost | Event::Resize(..) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::TempDir;

    mod lock {
        use super::*;

        #[test]
        fn lets_only_one_server_hold_a_socket() {
            let tmp = TempDir::new();
            let socket = tmp.path().join("server.sock");
            let _first = lock(&socket).expect("first lock");

            assert!(matches!(lock(&socket), Err(Error::ServerRunning(_))));
        }

        #[test]
        fn is_free_again_once_the_holder_is_gone() {
            let tmp = TempDir::new();
            let socket = tmp.path().join("server.sock");
            drop(lock(&socket).expect("first lock"));

            crate::test_util::wait_until("the lock is free", || lock(&socket).is_ok());
        }
    }

    mod a_bug {
        use std::cell::Cell;

        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent};

        use super::*;
        use crate::graphics::Support;
        use crate::state::{PaneState, ProjectState, State, TabState, WorkspaceState};
        use crate::test_util::wait_until;

        const COLS: u16 = 100;
        const ROWS: u16 = 20;
        const BUG: &str = "cornercase hit a bug, see server.log";
        const PANE_MENU: &str = "split right";
        const NOBODY: u64 = u64::MAX;
        const BUGGY: ServerEvent = ServerEvent::Gone(NOBODY);
        const BUGGY_KEY: KeyCode = KeyCode::F(12);

        fn buggy(step: &Step) -> bool {
            match step {
                Step::Event(ServerEvent::Gone(NOBODY)) => true,
                Step::Event(ServerEvent::Message(_, ClientMessage::Event(Event::Key(key)))) => key.code == BUGGY_KEY,
                _ => false,
            }
        }

        fn buggy_events(server: &mut Server, step: Step) -> ControlFlow<()> {
            assert!(!buggy(&step), "a bug while handling an event");
            server.step(step)
        }

        pub(super) fn one_shell_in(dir: &Path) -> State {
            let tabs = vec![TabState {
                name: None,
                panes: vec![PaneState { cwd: Some(dir.to_path_buf()), right_clicks: false, agent: None }],
                active: 0,
                layout: None,
            }];
            let path = dir.to_path_buf();
            let workspace = WorkspaceState {
                path: path.clone(),
                name: None,
                worktree: false,
                tabs,
                active: 0,
                base: None,
                collapsed: false,
            };
            let project = ProjectState {
                path,
                name: None,
                group: None,
                workspaces: vec![workspace],
                active: 0,
                collapsed: false,
            };
            State { version: state::VERSION, projects: vec![project], ..State::default() }
        }

        pub(super) struct Attached {
            pub(super) server: Server,
            rx: Receiver<ServerEvent>,
            client: Client,
            dir: TempDir,
        }

        pub(super) struct Client {
            id: u64,
            tx: Sender<ServerEvent>,
            frames: Receiver<ServerMessage>,
            screen: vt100::Parser,
            pub(super) raw: Vec<u8>,
            clears: usize,
        }

        fn connect(server: &mut Server, tx: Sender<ServerEvent>, id: u64, graphics: Support) -> Client {
            let (ours, mut theirs) = UnixStream::pair().expect("a socket pair");
            let _ = server.handle(ServerEvent::Accepted(ours));
            let (version, build, theme) = (protocol::VERSION, protocol::build_id(), HostTheme::default());
            let (notify, probe) = (Channel::Bell, String::new());
            let hello =
                Hello { version, build, cols: COLS, rows: ROWS, theme, notify, terminal: None, graphics, probe };
            let _ = server.handle(ServerEvent::Message(id, ClientMessage::Hello(Box::new(hello))));
            let (frames_tx, frames) = mpsc::channel();
            thread::spawn(move || {
                while let Ok(Some(msg)) = protocol::recv::<ServerMessage>(&mut theirs) {
                    if frames_tx.send(msg).is_err() {
                        return;
                    }
                }
            });
            Client { id, tx, frames, screen: vt100::Parser::new(ROWS, COLS, 0), raw: Vec::new(), clears: 0 }
        }

        impl Attached {
            pub(super) fn new() -> Self {
                Self::seeing(Support::default())
            }

            pub(super) fn seeing(graphics: Support) -> Self {
                let dir = TempDir::new();
                let saved = one_shell_in(dir.path());
                Self::with_graphics(dir, &saved, graphics)
            }

            pub(super) fn with(dir: TempDir, saved: &State) -> Self {
                Self::with_graphics(dir, saved, Support::default())
            }

            fn with_graphics(dir: TempDir, saved: &State, graphics: Support) -> Self {
                let session = dir.path().join("session.json");
                state::save(&session, saved).expect("save a session");
                let (tx, rx) = mpsc::channel();
                let (app_tx, app_rx) = mpsc::channel();
                spawn_forwarder(app_rx, tx.clone());
                let app = App::new("/bin/sh".into(), HostTheme::default(), dir.path().join("config.json"), app_tx);
                let todo_saver = Saver::new(dir.path().join("todos.json"), None);
                let mut server = Server::new(app, session, todo_saver, tx.clone());
                let client = connect(&mut server, tx, 1, graphics);
                Self { server, rx, client, dir }
            }

            pub(super) fn dir(&self) -> &Path {
                self.dir.path()
            }

            pub(super) fn join(&mut self, graphics: Support) -> Client {
                let id = self.server.next_client;
                connect(&mut self.server, self.client.tx.clone(), id, graphics)
            }

            pub(super) fn shows_frame(&self, bytes: &[u8]) {
                self.received(|msg| matches!(msg, ServerMessage::Frame(frame) if frame == bytes));
            }

            pub(super) fn received(&self, last: impl Fn(&ServerMessage) -> bool) -> Vec<ServerMessage> {
                let mut got = Vec::new();
                wait_until("the message arrives", || {
                    got.extend(std::iter::from_fn(|| self.client.frames.try_recv().ok()));
                    got.iter().any(&last)
                });
                got
            }

            pub(super) fn serve(
                self,
                run: impl Fn(&mut Server, Step) -> ControlFlow<()>,
                script: impl FnOnce(&mut Client) + Send + 'static,
            ) {
                let Self { mut server, rx, mut client, dir: _dir } = self;
                let stop = client.tx.clone();
                let script = thread::spawn(move || {
                    let done = panics::contain(|| script(&mut client));
                    let _ = stop.send(ServerEvent::Shutdown);
                    done.expect("the script runs to its end");
                });
                server.serve_with(&rx, run);
                script.join().expect("the script passes");
            }
        }

        impl Client {
            fn send(&self, ev: ServerEvent) {
                self.tx.send(ev).expect("the server listens");
            }

            pub(super) fn input(&self, ev: Event) {
                self.send(ServerEvent::Message(self.id, ClientMessage::Event(ev)));
            }

            pub(super) fn key(&self, code: KeyCode) {
                self.input(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)));
            }

            pub(super) fn click(&self, column: u16, row: u16) {
                for kind in [MouseEventKind::Down(MouseButton::Left), MouseEventKind::Up(MouseButton::Left)] {
                    self.input(Event::Mouse(MouseEvent { kind, column, row, modifiers: KeyModifiers::NONE }));
                }
            }

            pub(super) fn click_on(&mut self, text: &str) {
                self.shows(text);
                let rows = self.screen.screen().rows(0, COLS).collect::<Vec<_>>();
                let (row, column) = rows
                    .iter()
                    .enumerate()
                    .rev()
                    .find_map(|(y, line)| Some((y, line.find(text)?)))
                    .expect("the text is on screen");
                let column = u16::try_from(rows[row][..column].chars().count()).expect("on screen");
                self.click(column + 1, u16::try_from(row).expect("on screen"));
            }

            pub(super) fn type_line(&self, line: &str) {
                line.chars().map(KeyCode::Char).chain([KeyCode::Enter]).for_each(|code| self.key(code));
            }

            fn right_click_in_the_pane(&self) {
                for kind in [MouseEventKind::Down(MouseButton::Right), MouseEventKind::Up(MouseButton::Right)] {
                    let ev = MouseEvent { kind, column: COLS - 10, row: 3, modifiers: KeyModifiers::NONE };
                    self.input(Event::Mouse(ev));
                }
            }

            pub(super) fn text(&self) -> String {
                self.screen.screen().contents()
            }

            pub(super) fn until(&mut self, what: &str, cond: impl Fn(&Self) -> bool) {
                wait_until(what, || {
                    while let Ok(msg) = self.frames.try_recv() {
                        if let ServerMessage::Frame(bytes) = msg {
                            self.clears += usize::from(bytes == CLEAR_SCREEN);
                            self.screen.process(&bytes);
                            self.raw.extend_from_slice(&bytes);
                        }
                    }
                    cond(self)
                });
            }

            pub(super) fn count(&self, needle: &[u8]) -> usize {
                self.raw.windows(needle.len()).filter(|w| *w == needle).count()
            }

            pub(super) fn shows(&mut self, text: &str) {
                self.until(text, |client| client.text().contains(text));
            }
        }

        #[test]
        fn an_event_that_panics_is_dropped_and_the_next_ones_are_handled() {
            Attached::new().serve(buggy_events, |client| {
                client.send(BUGGY);
                client.shows(BUG);
                client.type_line("echo still-\"\"alive");
                client.shows("still-alive");
            });
        }

        #[test]
        fn a_panic_while_drawing_starts_every_screen_over() {
            let armed = Cell::new(false);
            let run = move |server: &mut Server, step: Step| {
                armed.set(armed.get() || buggy(&step));
                assert!(!(matches!(step, Step::Draw) && armed.replace(false)), "a bug while drawing");
                server.step(step)
            };
            Attached::new().serve(run, |client| {
                client.shows("Quit");
                let before = client.clears;
                client.send(BUGGY);
                client.until("the screen is cleared", |client| client.clears > before);
                client.shows(BUG);
            });
        }

        #[test]
        fn only_a_panic_in_what_the_user_does_closes_what_is_open() {
            Attached::new().serve(buggy_events, |client| {
                client.right_click_in_the_pane();
                client.shows(PANE_MENU);
                client.send(BUGGY);
                client.shows(BUG);
                assert!(client.text().contains(PANE_MENU));
                client.key(BUGGY_KEY);
                client.until("the menu closes", |client| !client.text().contains(PANE_MENU));
            });
        }
    }

    mod streaming {
        use super::*;
        use crate::control::{Event, Response};

        fn client() -> (Client, Receiver<ServerMessage>) {
            let (tx, rx) = mpsc::channel();
            let out = Outbox { tx, queued: Arc::new(AtomicUsize::new(0)) };
            let writer = thread::spawn(|| {});
            let graphics = Graphics::default();
            (
                Client {
                    id: 1,
                    size: None,
                    used: 0,
                    notify: Channel::Bell,
                    graphics,
                    screen: None,
                    out,
                    writer,
                    dropped: 0,
                },
                rx,
            )
        }

        fn texts(rx: &Receiver<ServerMessage>) -> Vec<String> {
            rx.try_iter()
                .map(|msg| match msg {
                    ServerMessage::Response(text) => text,
                    _ => panic!("not an answer"),
                })
                .collect()
        }

        #[test]
        fn a_reader_that_falls_behind_loses_events_then_is_told_how_many() {
            let (mut client, rx) = client();
            for n in 0..EVENTS_QUEUED + 3 {
                client.stream(n.to_string());
            }
            let queued = texts(&rx);

            client.out.queued.store(0, Ordering::Relaxed);
            client.stream("next".into());

            let after = texts(&rx);
            let Ok(Response::Ok(notice)) = serde_json::from_str(&after[0]) else { panic!("not a notice: {after:?}") };
            let notice: Event = serde_json::from_value(notice).expect("an event");
            assert_eq!(queued.len(), EVENTS_QUEUED);
            assert_eq!((notice.what, &after[1..]), (What::Dropped { count: 3 }, &["next".to_string()][..]));
        }

        #[test]
        fn the_notice_goes_out_once_there_is_room_even_without_a_new_event() {
            let (mut client, rx) = client();
            client.out.queued.store(EVENTS_QUEUED, Ordering::Relaxed);
            client.stream("lost".into());

            client.catch_up();
            let full = texts(&rx);
            client.out.queued.store(0, Ordering::Relaxed);
            client.catch_up();

            assert_eq!((full.len(), texts(&rx).len(), client.dropped), (0, 1, 0));
        }
    }

    mod restarting_when_idle {
        use super::a_bug::Attached;
        use super::*;
        use crate::test_util::wait_until;

        #[test]
        fn the_command_is_answered_before_every_client_is_told() {
            let mut attached = Attached::new();
            let request = r#"{"command":"restart-when-idle","args":{}}"#.to_string();
            let _ = attached.server.handle(ServerEvent::Message(1, ClientMessage::Request(request)));

            wait_until("no agent is at work", || attached.server.step(Step::Refresh).is_break());
            attached.server.shutdown();

            let told = attached.received(|msg| matches!(msg, ServerMessage::Restart(_)));
            let last: Vec<&ServerMessage> = told.iter().filter(|msg| !matches!(msg, ServerMessage::Frame(_))).collect();
            assert!(
                matches!(last[..], [ServerMessage::Response(text), ServerMessage::Restart(_)] if text.starts_with(r#"{"ok""#)),
                "{last:?}"
            );
        }
    }

    mod restore {
        use super::a_bug::{Attached, one_shell_in};
        use super::*;
        use crate::test_util::Locked;

        fn backup(attached: &Attached) -> PathBuf {
            attached.server.session.with_extension("json.bak")
        }

        #[test]
        fn a_complete_restore_leaves_no_backup() {
            let attached = Attached::new();

            assert!(!backup(&attached).exists());
        }

        #[test]
        fn an_incomplete_restore_keeps_the_session_it_read() {
            let (dir, locked) = (TempDir::new(), Locked::new());
            let mut saved = one_shell_in(dir.path());
            saved.projects.extend(one_shell_in(locked.path()).projects);

            let attached = Attached::with(dir, &saved);

            assert_eq!(state::load(&backup(&attached)), Some(saved));
        }

        #[test]
        fn a_session_whose_every_tab_fails_still_opens_a_shell() {
            let locked = Locked::new();

            let attached = Attached::with(TempDir::new(), &one_shell_in(locked.path()));

            assert!(attached.server.app.has_terms(), "a shell opens in the current folder");
        }

        #[test]
        fn a_session_saved_without_tabs_comes_back_as_it_was() {
            let dir = TempDir::new();
            let mut saved = one_shell_in(dir.path());
            saved.projects[0].workspaces[0].tabs.clear();

            let attached = Attached::with(dir, &saved);

            assert_eq!(attached.server.app.state().projects.len(), 1);
        }
    }

    mod showing_images {
        use super::a_bug::{Attached, Client};
        use super::*;
        use crate::graphics::{CellSize, Protocol, Support, Tmux};
        use crate::ui;

        const TRANSMIT: &[u8] = b"a=t,";
        const PLACE: &[u8] = b"a=p,";
        const KITTY: &[u8] = b"\x1b_G";
        const ITERM: &[u8] = b"\x1b]1337;File=";

        fn support(protocol: Protocol) -> Support {
            Support {
                protocol: Some(protocol),
                missing: None,
                cell: Some(CellSize { width: 10, height: 20 }),
                tmux: Tmux::None,
                id_hi: 42,
            }
        }

        fn with_image(graphics: Support) -> Attached {
            let attached = Attached::seeing(graphics);
            let pixels = image::RgbaImage::from_pixel(400, 200, image::Rgba([200, 30, 30, 255]));
            pixels.save_with_format(attached.dir().join("logo.png"), image::ImageFormat::Png).expect("a png");
            attached
        }

        fn open_the_image(client: &mut Client) {
            client.type_line("echo lo\"\"go.png");
            client.click_on("logo.png");
            client.shows("400×200");
        }

        fn settings() -> Position {
            ui::full_layout(Rect::new(0, 0, 100, 20), ui::Widths::default(), true, ui::Sidebar::default(), false)
                .settings
                .as_position()
        }

        #[test]
        fn kitty_gets_the_image_once_and_only_a_new_placement_when_the_window_shrinks() {
            let attached = with_image(support(Protocol::Kitty));
            attached.serve(Server::step, |client| {
                open_the_image(client);
                client.until("the image is sent", |c| c.count(TRANSMIT) == 1);
                let placed = client.count(PLACE);

                client.input(Event::Resize(100, 12));

                client.until("the image is placed again", |c| c.count(PLACE) > placed);
                assert_eq!(client.count(TRANSMIT), 1);
            });
        }

        #[test]
        fn an_inline_image_waits_while_a_dialog_is_open_and_comes_back_after() {
            let attached = with_image(support(Protocol::Iterm));
            attached.serve(Server::step, |client| {
                open_the_image(client);
                client.until("the image is sent", |c| c.count(ITERM) == 1);

                let at = settings();
                client.click(at.x, at.y);
                client.shows("Worktrees");
                assert_eq!(client.count(ITERM), 1, "nothing while the dialog is open");
                client.key(KeyCode::Esc);

                client.until("the image is sent again", |c| c.count(ITERM) == 2);
            });
        }

        #[test]
        fn a_window_without_images_gets_none_while_another_gets_its_own() {
            let mut attached = with_image(support(Protocol::Kitty));
            let mut blind = attached.join(Support::default());
            attached.serve(Server::step, move |client| {
                open_the_image(client);
                client.until("the image is sent", |c| c.count(TRANSMIT) == 1);

                blind.shows("400×200");
                blind.until("the window says why", |c| !c.text().contains("reading"));
                assert_eq!((blind.count(KITTY), blind.count(ITERM)), (0, 0));
            });
        }
    }

    mod drawing {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        use super::a_bug::Attached;
        use super::*;
        use crate::clipboard;
        use crate::control::{Report, Response};

        fn drawn_just_now(attached: &mut Attached) -> Instant {
            attached.server.draw();
            attached.server.drawn.expect("a frame was drawn")
        }

        fn shown_pane(attached: &mut Attached) -> u64 {
            let server = &mut attached.server;
            server.app.request(1, r#"{"command":"status","args":{}}"#, server.area, Instant::now());
            let (_, text) = server.app.take_answers().pop().expect("an answer");
            let Ok(Response::Ok(value)) = serde_json::from_str(&text) else { panic!("status failed: {text}") };
            let report: Report = serde_json::from_value(value).expect("a report");
            report.projects[0].workspaces[0].tabs[0].panes[0].id
        }

        #[test]
        fn output_nobody_can_see_draws_nothing() {
            let mut attached = Attached::new();
            let now = drawn_just_now(&mut attached);

            let _ = attached.server.handle(ServerEvent::App(AppEvent::Output(u64::MAX, b"hidden".to_vec())));

            assert!(!attached.server.due(now));
        }

        #[test]
        fn output_on_screen_draws_at_most_once_a_frame() {
            let mut attached = Attached::new();
            let pane = shown_pane(&mut attached);
            let now = drawn_just_now(&mut attached);
            attached.server.printed_at = Some(now);

            let _ = attached.server.handle(ServerEvent::App(AppEvent::Output(pane, b"shown".to_vec())));

            let server = &attached.server;
            assert_eq!((server.due(now), server.due(now + FRAME), server.next_frame(now)), (false, true, Some(FRAME)));
        }

        #[test]
        fn input_draws_at_once() {
            let mut attached = Attached::new();
            let now = drawn_just_now(&mut attached);
            let key = Event::Key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));

            let _ = attached.server.handle(ServerEvent::Message(1, ClientMessage::Event(key)));

            assert!(attached.server.due(now));
        }

        #[test]
        fn a_key_does_not_hold_back_its_echo() {
            let mut attached = Attached::new();
            let pane = shown_pane(&mut attached);
            drawn_just_now(&mut attached);
            let key = Event::Key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
            let _ = attached.server.handle(ServerEvent::Message(1, ClientMessage::Event(key)));
            attached.server.draw();

            let _ = attached.server.handle(ServerEvent::App(AppEvent::Output(pane, b"a".to_vec())));

            assert!(attached.server.due(Instant::now()));
        }

        #[test]
        fn a_quiet_server_still_draws_every_tick() {
            let mut attached = Attached::new();
            let now = drawn_just_now(&mut attached);

            assert_eq!((attached.server.due(now), attached.server.due(now + TICK)), (false, true));
        }

        #[test]
        fn output_nobody_can_see_still_sends_what_it_copied() {
            let mut attached = Attached::new();
            let pane = shown_pane(&mut attached);
            let server = &mut attached.server;
            server.app.request(1, r#"{"command":"new-tab","args":{"focus":true}}"#, server.area, Instant::now());
            drawn_just_now(&mut attached);
            let copy = b"\x1b]52;c;aGVsbG8=\x07".to_vec();

            let _ = attached.server.handle(ServerEvent::App(AppEvent::Output(pane, copy)));
            attached.server.flush();

            assert!(!attached.server.app.shows(pane));
            attached.shows_frame(&clipboard::osc52("hello"));
        }

        #[test]
        fn a_settled_session_is_still_saved() {
            let mut attached = Attached::new();
            let session = attached.server.session.clone();
            std::fs::remove_file(&session).expect("forget the saved session");
            attached.server.saver = Saver::new(session.clone(), None);
            let t0 = Instant::now();

            attached.server.save(t0);
            attached.server.save(t0 + SAVE_EVERY / 2);
            attached.server.save(t0 + state::SETTLE + SAVE_EVERY);

            assert!(session.exists(), "the session settles and is written");
        }
    }
}
