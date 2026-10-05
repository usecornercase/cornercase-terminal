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
use crate::protocol::{self, ClientMessage, Hello, ServerMessage};
use crate::state::{self, Saver};

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
    saver: Saver,
    restart: Option<PathBuf>,
    tx: Sender<ServerEvent>,
}

pub fn run() -> Result<()> {
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
    app.set_issue_cache(state::path().with_file_name(ISSUE_CACHE_FILE));
    let mut server = Server {
        app,
        clients: Vec::new(),
        area: None,
        next_client: 1,
        uses: 0,
        started: false,
        build: protocol::build_id(),
        saver: Saver::new(state::path(), None),
        restart: None,
        tx,
    };
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
    fn serve(&mut self, rx: &Receiver<ServerEvent>) {
        loop {
            self.app.refresh(Instant::now());
            self.draw();
            self.save();
            let first = match rx.recv_timeout(self.app.tick().unwrap_or(TICK)) {
                Ok(ev) => Some(ev),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => return,
            };
            for ev in first.into_iter().chain(std::iter::from_fn(|| rx.try_recv().ok())) {
                if self.handle(ev).is_break() {
                    return;
                }
            }
        }
    }

    fn save(&mut self) {
        if self.started
            && let Err(e) = self.saver.observe(self.app.state(), Instant::now())
        {
            eprintln!("cornercase server: failed to save the session: {e}");
        }
    }

    fn draw(&mut self) {
        let Some(area) = self.area else { return };
        let Self { app, clients, .. } = self;
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
        let saved = state::load(&state::path());
        if let Some(saved) = &saved {
            self.app.restore(saved, area)?;
        }
        if self.app.is_empty() {
            self.app.open_here(area)?;
        }
        self.saver = Saver::new(state::path(), saved);
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
        let before = self.clients.len();
        self.clients.retain(|c| c.id != id);
        if self.clients.len() != before {
            self.fit(None);
        }
    }

    fn shutdown(&mut self) {
        if self.restart.is_some()
            && self.started
            && let Err(e) = state::save(&state::path(), &self.app.state())
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
}
