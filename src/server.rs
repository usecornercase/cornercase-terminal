use std::fs::File;
use std::io::{self, Write};
use std::net::Shutdown;
use std::ops::ControlFlow;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crossterm::event::{Event, MouseEventKind};
use ratatui::backend::{Backend, ClearType, CrosstermBackend, WindowSize};
use ratatui::buffer::Cell;
use ratatui::layout::{Position, Rect, Size};
use ratatui::{Terminal, TerminalOptions, Viewport};
use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};
use signal_hook::iterator::Signals;

use crate::app::{App, AppEvent};
use crate::config;
use crate::error::{Error, Result};
use crate::host_theme::HostTheme;
use crate::notify::Channel;
use crate::panics;
use crate::protocol::{self, ClientMessage, Hello, ServerMessage};
use crate::state::{self, Saver};
use crate::todo;

const TICK: Duration = Duration::from_millis(500);
const ISSUE_CACHE_FILE: &str = "issues.json";
const WRITE_TIMEOUT: Duration = Duration::from_secs(1);
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
    Draw,
    Save,
    Event(ServerEvent),
}

struct FrameWriter {
    buf: Vec<u8>,
    out: Sender<ServerMessage>,
}

impl Write for FrameWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.buf.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        if !self.buf.is_empty() {
            let _ = self.out.send(ServerMessage::Frame(std::mem::take(&mut self.buf)));
        }
        Ok(())
    }
}

struct CropBackend {
    inner: CrosstermBackend<FrameWriter>,
    area: Rect,
    visible: Rect,
}

impl Backend for CropBackend {
    type Error = io::Error;

    fn draw<'a, I>(&mut self, content: I) -> io::Result<()>
    where
        I: Iterator<Item = (u16, u16, &'a Cell)>,
    {
        let visible = self.visible;
        self.inner.draw(content.filter(|&(x, y, _)| visible.contains(Position::new(x, y))))
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
    screen: Option<Screen>,
    out: Sender<ServerMessage>,
    writer: JoinHandle<()>,
}

impl Client {
    fn send(&self, msg: ServerMessage) {
        let _ = self.out.send(msg);
    }

    fn reset_screen(&mut self, area: Rect) {
        let Some((width, height)) = self.size else { return };
        self.send(ServerMessage::Frame(CLEAR_SCREEN.to_vec()));
        let inner = CrosstermBackend::new(FrameWriter { buf: Vec::new(), out: self.out.clone() });
        let backend = CropBackend { inner, area, visible: Rect::new(0, 0, width, height) };
        self.screen = Terminal::with_options(backend, TerminalOptions { viewport: Viewport::Fixed(area) }).ok();
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
}

pub fn run() -> Result<()> {
    panics::log_to_stderr();
    let _ = rustix::process::setsid();
    let path = protocol::socket_path();
    let (listener, _lock) = bind(&path)?;
    let (tx, rx) = mpsc::channel();
    spawn_acceptor(listener, tx.clone());
    spawn_signal_thread(tx.clone())?;
    let (app_tx, app_rx) = mpsc::channel();
    spawn_forwarder(app_rx, tx.clone());

    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
    let mut app = App::new(shell, HostTheme::default(), config::path(), app_tx);
    let session = state::path();
    app.set_issue_cache(session.with_file_name(ISSUE_CACHE_FILE));
    let todos = todo::load(&todo::path(&session));
    let todo_saver = Saver::new(todo::path(&session), Some(todos.saved()));
    app.set_todos(todos);
    let mut server = Server::new(app, session, todo_saver, tx);
    server.serve(&rx);
    let _ = std::fs::remove_file(&path);
    server.shutdown();
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

fn spawn_client_writer(mut stream: UnixStream, rx: Receiver<ServerMessage>) -> JoinHandle<()> {
    thread::spawn(move || {
        for msg in rx {
            if protocol::send(&mut stream, &msg).is_err() {
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
        }
    }

    fn serve(&mut self, rx: &Receiver<ServerEvent>) {
        self.serve_with(rx, Self::step);
    }

    fn serve_with(&mut self, rx: &Receiver<ServerEvent>, run: impl Fn(&mut Self, Step) -> ControlFlow<()>) {
        loop {
            for step in [Step::Refresh, Step::Answer, Step::Draw, Step::Save] {
                self.contain(step, &run);
            }
            let tick = self.app.tick(Instant::now()).map_or(TICK, |tick| tick.min(TICK));
            let first = match rx.recv_timeout(tick) {
                Ok(ev) => Some(ev),
                Err(RecvTimeoutError::Timeout) => None,
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
        match step {
            Step::Refresh => self.app.refresh(Instant::now()),
            Step::Answer => self.answer(),
            Step::Draw => self.draw(),
            Step::Save => self.save(),
            Step::Event(ev) => return self.handle(ev),
        }
        ControlFlow::Continue(())
    }

    fn contain(&mut self, step: Step, run: impl Fn(&mut Self, Step) -> ControlFlow<()>) -> Option<ControlFlow<()>> {
        let drawing = matches!(step, Step::Draw);
        let interacting = drawing || matches!(step, Step::Event(ServerEvent::Message(_, ClientMessage::Event(_))));
        let done = panics::contain(|| run(self, step));
        if done.is_none() {
            if interacting {
                self.app.reset_interaction();
            }
            if drawing && let Some(area) = self.area {
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
    }

    fn save(&mut self) {
        if self.started
            && let Err(e) = self.saver.observe(self.app.state(), Instant::now())
        {
            eprintln!("cornercase server: failed to save the session: {e}");
        }
        if let Err(e) = self.todo_saver.observe(self.app.todos_saved(), Instant::now()) {
            eprintln!("cornercase server: failed to save the todo lists: {e}");
        }
    }

    fn draw(&mut self) {
        let Self { app, clients, area, .. } = self;
        for bytes in app.take_host_writes() {
            for client in clients.iter().filter(|c| c.screen.is_some()) {
                client.send(ServerMessage::Frame(bytes.clone()));
            }
        }
        for notification in app.take_notifications() {
            for client in clients.iter().filter(|c| c.screen.is_some()) {
                client.send(ServerMessage::Frame(notification.encode(client.notify)));
            }
        }
        let Some(area) = *area else { return };
        app.resize(area);
        for screen in clients.iter_mut().filter_map(|c| c.screen.as_mut()) {
            let _ = screen.draw(|f| app.draw(f));
        }
    }

    fn handle(&mut self, ev: ServerEvent) -> ControlFlow<()> {
        match ev {
            ServerEvent::Message(_, ClientMessage::KillServer) | ServerEvent::Shutdown => return ControlFlow::Break(()),
            ServerEvent::App(ev) => self.handle_app(ev),
            ServerEvent::Accepted(stream) => self.accept(stream),
            ServerEvent::Message(id, ClientMessage::Hello(hello)) => self.hello(id, *hello),
            ServerEvent::Message(id, ClientMessage::Event(ev)) => self.input(id, ev),
            ServerEvent::Message(_, ClientMessage::Restart) => {
                self.restart = Some(std::env::current_exe().unwrap_or_default());
            }
            ServerEvent::Message(id, ClientMessage::Request(text)) => {
                self.app.request(id, &text, self.area, Instant::now());
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
            eprintln!("cornercase server: {e}");
        }
    }

    fn accept(&mut self, stream: UnixStream) {
        if let Err(e) = protocol::check_peer(&stream, protocol::own_uid()) {
            eprintln!("cornercase server: {e}");
            return;
        }
        let Ok(reader) = stream.try_clone() else { return };
        let _ = stream.set_write_timeout(Some(WRITE_TIMEOUT));
        let id = self.next_client;
        self.next_client += 1;
        let (out, out_rx) = mpsc::channel();
        let writer = spawn_client_writer(stream, out_rx);
        spawn_client_reader(id, reader, self.tx.clone());
        self.clients.push(Client { id, size: None, used: 0, notify: Channel::Bell, screen: None, out, writer });
    }

    fn client_mut(&mut self, id: u64) -> Option<&mut Client> {
        self.clients.iter_mut().find(|c| c.id == id)
    }

    fn hello(&mut self, id: u64, hello: Hello) {
        if hello.version != protocol::VERSION || hello.build != self.build {
            self.reject(id, OTHER_BUILD);
            return;
        }
        let Some(client) = self.client_mut(id) else { return };
        client.size = Some((hello.cols, hello.rows));
        client.notify = hello.notify;
        self.touch(id);
        self.fit(Some(id));
        if !self.started {
            self.started = true;
            self.app.set_theme(hello.theme);
            if let Err(e) = self.open_first_terminals() {
                eprintln!("cornercase server: {e}");
            }
        }
    }

    fn open_first_terminals(&mut self) -> Result<()> {
        let area = self.area.unwrap_or_default();
        let saved = state::load(&self.session);
        let complete = saved.as_ref().is_none_or(|saved| self.app.restore(saved, area));
        if !complete {
            match state::back_up(&self.session) {
                Ok(backup) => {
                    eprintln!("cornercase server: the session came back incomplete, a copy is in {}", backup.display());
                }
                Err(e) => eprintln!("cornercase server: failed to keep a copy of the session: {e}"),
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
            eprintln!("cornercase server: {e}");
        }
        self.restart = self.app.take_restart();
        if self.app.take_detach() {
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
        self.area = Some(area);
        for client in self.clients.iter_mut().filter(|c| area_changed || Some(c.id) == resized) {
            client.reset_screen(area);
        }
    }

    fn reject(&mut self, id: u64, reason: &str) {
        if let Some(client) = self.client_mut(id) {
            client.send(ServerMessage::Rejected(reason.into()));
        }
        self.remove(id);
    }

    fn remove(&mut self, id: u64) {
        self.app.forget(id);
        let before = self.clients.len();
        self.clients.retain(|c| c.id != id);
        if self.clients.len() != before {
            self.fit(None);
        }
    }

    fn shutdown(&mut self) {
        if let Err(e) =
            self.todo_saver.observe(self.app.todos_saved(), Instant::now()).and_then(|()| self.todo_saver.flush())
        {
            eprintln!("cornercase server: failed to save the todo lists: {e}");
        }
        if self.restart.is_some()
            && self.started
            && let Err(e) = state::save(&self.session, &self.app.state())
        {
            eprintln!("cornercase server: failed to save the session: {e}");
        }
        for client in self.clients.drain(..) {
            client.send(self.restart.clone().map_or(ServerMessage::Shutdown, ServerMessage::Restart));
            let Client { out, screen, writer, .. } = client;
            drop((out, screen));
            let _ = writer.join();
        }
    }
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
                panes: vec![PaneState { cwd: Some(dir.to_path_buf()), right_clicks: false }],
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
            _dir: TempDir,
        }

        struct Client {
            tx: Sender<ServerEvent>,
            frames: Receiver<ServerMessage>,
            screen: vt100::Parser,
            clears: usize,
        }

        impl Attached {
            pub(super) fn new() -> Self {
                let dir = TempDir::new();
                let saved = one_shell_in(dir.path());
                Self::with(dir, &saved)
            }

            pub(super) fn with(dir: TempDir, saved: &State) -> Self {
                let session = dir.path().join("session.json");
                state::save(&session, saved).expect("save a session");
                let (tx, rx) = mpsc::channel();
                let (app_tx, app_rx) = mpsc::channel();
                spawn_forwarder(app_rx, tx.clone());
                let app = App::new("/bin/sh".into(), HostTheme::default(), dir.path().join("config.json"), app_tx);
                let todo_saver = Saver::new(dir.path().join("todos.json"), None);
                let mut server = Server::new(app, session, todo_saver, tx.clone());
                let (ours, mut theirs) = UnixStream::pair().expect("a socket pair");
                let _ = server.handle(ServerEvent::Accepted(ours));
                let (version, build, theme) = (protocol::VERSION, protocol::build_id(), HostTheme::default());
                let hello = Hello { version, build, cols: COLS, rows: ROWS, theme, notify: Channel::Bell };
                let _ = server.handle(ServerEvent::Message(1, ClientMessage::Hello(Box::new(hello))));
                let (frames_tx, frames) = mpsc::channel();
                thread::spawn(move || {
                    while let Ok(Some(msg)) = protocol::recv::<ServerMessage>(&mut theirs) {
                        if frames_tx.send(msg).is_err() {
                            return;
                        }
                    }
                });
                let client = Client { tx, frames, screen: vt100::Parser::new(ROWS, COLS, 0), clears: 0 };
                Self { server, rx, client, _dir: dir }
            }

            fn serve(
                self,
                run: impl Fn(&mut Server, Step) -> ControlFlow<()>,
                script: impl FnOnce(&mut Client) + Send + 'static,
            ) {
                let Self { mut server, rx, mut client, _dir } = self;
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

            fn input(&self, ev: Event) {
                self.send(ServerEvent::Message(1, ClientMessage::Event(ev)));
            }

            fn key(&self, code: KeyCode) {
                self.input(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)));
            }

            fn type_line(&self, line: &str) {
                line.chars().map(KeyCode::Char).chain([KeyCode::Enter]).for_each(|code| self.key(code));
            }

            fn right_click_in_the_pane(&self) {
                let kind = MouseEventKind::Down(MouseButton::Right);
                self.input(Event::Mouse(MouseEvent { kind, column: COLS - 10, row: 3, modifiers: KeyModifiers::NONE }));
            }

            fn text(&self) -> String {
                self.screen.screen().contents()
            }

            fn until(&mut self, what: &str, cond: impl Fn(&Self) -> bool) {
                wait_until(what, || {
                    while let Ok(msg) = self.frames.try_recv() {
                        if let ServerMessage::Frame(bytes) = msg {
                            self.clears += usize::from(bytes == CLEAR_SCREEN);
                            self.screen.process(&bytes);
                        }
                    }
                    cond(self)
                });
            }

            fn shows(&mut self, text: &str) {
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
                client.shows("quit");
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
}
