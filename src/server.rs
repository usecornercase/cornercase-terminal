use std::fs::File;
use std::io;
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
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;
use ratatui::{Terminal, TerminalOptions, Viewport};
use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};
use signal_hook::iterator::Signals;

use crate::app::{App, AppEvent, Streamed};
use crate::control::start_server::SizeFrom;
use crate::control::{self, Ids, What};
use crate::error::{Error, Result};
use crate::host::notify::{self, Channel};
use crate::host::open::Opener;
use crate::host::pointer;
use crate::host::sound;
use crate::host::theme::HostTheme;
use crate::log::{self, Level};
use crate::panics;
use crate::paths;
use crate::persist::{self, Saver};
use crate::protocol::{self, ClientMessage, Folder, ServerMessage};
use crate::queue;
use crate::state;
use crate::todo;
use crate::version;

mod backend;
mod images;
mod startup;
mod surfaces;

use backend::{CropBackend, FrameWriter};
use images::Graphics;
use startup::Startup;

const TICK: Duration = Duration::from_millis(500);
const SLOW_STEP: Duration = Duration::from_millis(100);
const FRAME: Duration = Duration::from_millis(16);
const SAVE_EVERY: Duration = Duration::from_millis(500);
const WRITE_TIMEOUT: Duration = Duration::from_secs(1);
const EVENTS_QUEUED: usize = 1024;
const SNAPSHOTS_QUEUED: usize = 4;
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
    Surfaces,
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
            Self::Surfaces => "surfaces",
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
            Self::Answer
                | Self::Flush
                | Self::Draw
                | Self::Surfaces
                | Self::Save
                | Self::Event(ServerEvent::App(AppEvent::Output(..)))
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

type Screen = Terminal<CropBackend>;

struct Client {
    id: u64,
    size: Option<(u16, u16)>,
    zone: String,
    openers: Vec<Opener>,
    used: u64,
    notify: Channel,
    browser: bool,
    sent: notify::Sent,
    pointer: Option<pointer::Terminal>,
    shape: Option<pointer::Shape>,
    graphics: Graphics,
    screen: Option<Screen>,
    out: Outbox,
    writer: JoinHandle<()>,
    dropped: u64,
    held: Option<String>,
}

impl Client {
    fn send(&self, msg: ServerMessage) {
        self.out.send(msg);
    }

    fn leave(&self, msg: ServerMessage) {
        let closes = self.sent.closes();
        if !closes.is_empty() {
            self.send(ServerMessage::Frame(closes));
        }
        self.send(msg);
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
        if self.out.queued() < SNAPSHOTS_QUEUED
            && let Some(text) = self.held.take()
        {
            self.send(ServerMessage::Response(text));
        }
        if self.dropped == 0 || self.out.queued() >= EVENTS_QUEUED {
            return;
        }
        log::warning!("server", "events dropped", client = self.id, count = self.dropped);
        let time = crate::time::Stamp(SystemTime::now()).to_string();
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
        let _ = screen.draw(|f| placed = app.draw(f, &sight, now));
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
    startup: Startup,
    size_from: SizeFrom,
    build: String,
    session: PathBuf,
    saver: Saver<state::State>,
    todo_saver: Saver<todo::Saved>,
    queue_saver: Saver<queue::Saved>,
    stats_saver: crate::agents::stats::Saver,
    restart: Option<PathBuf>,
    tx: Sender<ServerEvent>,
    drawn: Option<Instant>,
    printed_at: Option<Instant>,
    changed: bool,
    printed: bool,
    observed: Option<Instant>,
}

pub(crate) fn run() -> Result<()> {
    panics::log_to_stderr();
    let _ = rustix::process::setsid();
    let path = paths::socket();
    let (listener, _lock) = bind(&path)?;
    let level = log::level_from_env();
    if let Err(e) = log::start(&paths::log(), level) {
        eprintln!("cornercase server: cannot write its log to {}: {e}", paths::log().display());
    }
    let (tx, rx) = mpsc::channel();
    spawn_acceptor(listener, tx.clone());
    spawn_signal_thread(tx.clone())?;
    let (app_tx, app_rx) = mpsc::channel();
    spawn_forwarder(app_rx, tx.clone());

    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
    let mut app = App::new(shell, HostTheme::default(), paths::config(), app_tx);
    let session = paths::session();
    log::info!(
        "server",
        "started",
        version = version::CURRENT,
        build = version::build_id(),
        pid = std::process::id(),
        os = std::env::consts::OS,
        arch = std::env::consts::ARCH,
        socket = path.display(),
        session = session.display(),
        level = format!("{level:?}").to_lowercase(),
    );
    if let Some(tree) = paths::dev_source() {
        name_the_source(&session, tree);
        app.set_pane_socket(&path);
    }
    app.set_issue_cache(paths::issue_cache(&session));
    app.set_plugin_places(crate::plugins::Places::new(&paths::config(), &session));
    let todos = todo::load(&paths::todos(&session));
    let todo_saver = Saver::new(paths::todos(&session), Some(todos.saved()));
    app.set_todos(todos);
    app.set_queue(queue::load(&paths::queue(&session), crate::time::now()));
    app.set_queue_path(paths::queue(&session));
    app.set_setup_dir(path.parent().unwrap_or(Path::new(".")));
    app.set_runs_dir(&paths::runs(&session));
    app.set_accounts_dir(&paths::accounts_of(&session));
    let stats = crate::agents::stats::load(&paths::stats(&session), SystemTime::now());
    log::info!(
        "stats",
        "loaded",
        since = stats.view(SystemTime::now()).since,
        hours = stats.saved(SystemTime::now()).hours.len()
    );
    app.set_stats(stats);
    app.set_star(paths::star(&session));
    let mut server = Server::new(app, session, todo_saver, tx);
    server.serve(&rx);
    let _ = std::fs::remove_file(&path);
    server.shutdown();
    log::info!("server", "stopped", restart = server.restart.is_some());
    Ok(())
}

fn bind(path: &Path) -> Result<(UnixListener, File)> {
    let listen_error =
        |cause| Error::Listen { path: path.to_path_buf(), cause, custom: Folder::current() == Folder::Given };
    protocol::check_socket_dir(path, Folder::current())?;
    let lock = lock(path)?;
    let _ = std::fs::remove_file(path);
    Ok((UnixListener::bind(path).map_err(listen_error)?, lock))
}

fn lock(path: &Path) -> Result<File> {
    protocol::try_lock(path)
        .map_err(|cause| Error::Listen { path: path.to_path_buf(), cause, custom: Folder::current() == Folder::Given })?
        .ok_or_else(|| Error::ServerRunning(path.to_path_buf()))
}

fn name_the_source(session: &Path, tree: &str) {
    let path = paths::source(session);
    let written =
        path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(&path, format!("{tree}\n")));
    if let Err(error) = written {
        log::warning!("server", "cannot name the source tree", path = path.display(), error = error);
    }
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
        let queue_path = paths::queue(&session);
        let queue_baseline = queue::read_saved(&queue_path).unwrap_or_else(|| app.queue_saved());
        let queue_saver = Saver::new(queue_path, Some(queue_baseline));
        let stats_saver = crate::agents::stats::Saver::new(paths::stats(&session), app.stats().revision());
        Self {
            app,
            clients: Vec::new(),
            area: None,
            next_client: 1,
            uses: 0,
            startup: Startup::Waiting,
            size_from: SizeFrom::Window,
            build: version::build_id(),
            saver: Saver::new(session.clone(), None),
            session,
            todo_saver,
            queue_saver,
            stats_saver,
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
            self.contain(Step::Surfaces, &run);
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
        let (traced, refresh) = (step.traced(), matches!(step, Step::Refresh));
        let flow = self.run_step(step);
        if refresh {
            self.app.trace_refresh();
        } else if traced {
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
            Step::Surfaces => self.surfaces(),
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
                (Some(client), Streamed::Snapshot(text)) => client.held = Some(text),
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
        if self.started() {
            match self.saver.observe(self.app.state(), now) {
                Ok(true) => log::debug!("server", "session saved", path = self.session.display()),
                Ok(false) => {}
                Err(e) => log::error!("server", "failed to save the session", error = e),
            }
        }
        match self.stats_saver.observe(self.app.stats(), SystemTime::now(), now) {
            Ok(true) => log::debug!("stats", "saved"),
            Ok(false) => {}
            Err(e) => log::error!("stats", "failed to save", error = e),
        }
        match self.queue_saver.observe(self.app.queue_saved(), now) {
            Ok(true) => log::debug!("server", "queue saved"),
            Ok(false) => {}
            Err(e) => log::error!("server", "failed to save the queue", error = e),
        }
        match self.todo_saver.observe(self.app.todos_saved(), now) {
            Ok(true) => log::debug!("server", "todo list saved"),
            Ok(false) => {}
            Err(e) => log::error!("server", "failed to save the todo list", error = e),
        }
    }

    fn flush(&mut self) {
        let Self { app, clients, area, .. } = self;
        if let Some(area) = *area
            && clients.iter().any(|c| c.pointer.is_some())
        {
            let shape = app.pointer(area);
            for client in clients.iter_mut().filter(|c| c.screen.is_some()) {
                if let Some(terminal) = client.pointer
                    && client.shape != Some(shape)
                {
                    client.shape = Some(shape);
                    client.send(ServerMessage::Frame(terminal.encode(shape)));
                }
            }
        }
        for bytes in app.take_host_writes() {
            for client in clients.iter().filter(|c| c.screen.is_some()) {
                client.send(ServerMessage::Frame(bytes.clone()));
            }
        }
        for notification in app.take_notifications() {
            for client in clients.iter_mut().filter(|c| c.screen.is_some()) {
                let channel = notification.channel_for(client.notify).id();
                log::info!("server", "notification sent", client = client.id, channel = channel);
                client.sent.push(&notification, client.notify);
                client.send(ServerMessage::Frame(notification.encode(client.notify)));
            }
        }
        if let Some(cue) = sound::loudest(&app.take_sounds()) {
            for client in clients.iter().filter(|c| c.screen.is_some() && !c.browser) {
                log::info!("server", "sound sent", client = client.id, kind = cue.kind.id());
                log::debug!("server", "sound path", client = client.id, path = cue.path);
                client.send(ServerMessage::Sound(cue.clone()));
            }
        }
        for (id, open) in app.take_opens() {
            if let Some(client) = clients.iter().find(|c| c.id == id && c.screen.is_some() && !c.browser) {
                log::info!("server", "open sent", client = id, opener = open.opener.id());
                client.send(ServerMessage::Open(open));
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
        if let ServerEvent::App(AppEvent::Output(id, _)) = &ev {
            self.printed = self.printed || self.app.shows(*id);
        } else {
            self.changed = true;
            self.app.touch_surfaces();
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
            ServerEvent::Message(_, ClientMessage::Activated(token)) => self.app.notification_clicked(&token),
            ServerEvent::Message(id, ClientMessage::OpenFailed(failed)) => self.app.open_failed(id, &failed),
            ServerEvent::Message(id, ClientMessage::Restart) => {
                log::info!("server", "restart requested", client = id);
                self.restart = Some(std::env::current_exe().unwrap_or_default());
            }
            ServerEvent::Message(id, ClientMessage::Request(text)) => {
                self.app.request(id, &text, self.area, Instant::now());
                if let Some((client, size)) = self.app.take_start() {
                    self.start_server(client, size);
                }
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
        self.clients.push(Client {
            id,
            size: None,
            zone: "UTC".into(),
            openers: Vec::new(),
            used: 0,
            notify,
            browser: false,
            sent: notify::Sent::default(),
            pointer: None,
            shape: None,
            graphics,
            screen: None,
            out,
            writer,
            dropped: 0,
            held: None,
        });
    }

    fn client_mut(&mut self, id: u64) -> Option<&mut Client> {
        self.clients.iter_mut().find(|c| c.id == id)
    }

    fn input(&mut self, id: u64, ev: Event) {
        let Some(client) = self.client_mut(id).filter(|c| c.size.is_some()) else { return };
        if let Event::Resize(cols, rows) = ev {
            client.size = Some((cols, rows));
            self.fit(Some(id));
            return;
        }
        if let Event::FocusGained | Event::FocusLost = ev {
            self.app.window_focus(id, ev == Event::FocusGained);
            return;
        }
        let used = is_use(&ev);
        if used {
            self.touch(id);
            self.fit(None);
        }
        let Some(area) = self.area else { return };
        if let Some(client) = self.clients.iter().find(|client| client.id == id) {
            self.app.set_queue_zone(client.zone.clone());
            self.app.set_window(id, &client.openers);
        }
        if let Err(e) = self.app.handle_event(AppEvent::Input(ev), area) {
            log::warning!("app", "input failed", error = e);
        }
        if used {
            self.app.window_used();
        }
        self.restart = self.app.take_restart();
        if self.restart.is_some() {
            log::info!("server", "restart chosen in the window", client = id);
        }
        if self.app.take_detach() {
            log::info!("server", "client detached", client = id);
            if let Some(client) = self.client_mut(id) {
                client.leave(ServerMessage::Detached);
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
            if self.clients.iter().all(|c| c.size.is_none()) {
                self.app.window_gone();
            }
        }
    }

    fn shutdown(&mut self) {
        self.answer();
        self.app.stop_plugins();
        if let Err(e) = self.stats_saver.flush(self.app.stats(), SystemTime::now()) {
            log::error!("stats", "failed to save", error = e);
        }
        if let Err(e) =
            self.todo_saver.observe(self.app.todos_saved(), Instant::now()).and_then(|_| self.todo_saver.flush())
        {
            log::error!("server", "failed to save the todo list", error = e);
        }
        if self.restart.is_some()
            && self.started()
            && let Err(e) = persist::save(&self.session, &self.app.state())
        {
            log::error!("server", "failed to save the session", error = e);
        }
        if let Err(e) =
            self.queue_saver.observe(self.app.queue_saved(), Instant::now()).and_then(|_| self.queue_saver.flush())
        {
            log::error!("server", "failed to save the queue", error = e);
        }
        for client in self.clients.drain(..) {
            client.leave(self.restart.clone().map_or(ServerMessage::Shutdown, ServerMessage::Restart));
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
mod tests;
