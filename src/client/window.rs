use std::io::{self, Write, stdin, stdout};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use crossterm::event::{Event, KeyCode, KeyEventKind, MouseButton, MouseEventKind};
use ratatui::DefaultTerminal;
use ratatui::backend::Backend;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM, SIGWINCH};
use signal_hook::iterator::{Handle, Signals};

use super::probe::{env, finish_tmux_check, start_tmux_check, window_cell};
use super::{Ending, Guard, Host, INPUT_POLL, Link, Source, Target, input};
use crate::error::{Error, Result};
use crate::graphics::Support;
use crate::graphics::detect::{self, Facts, Outer};
use crate::host::notify::{self, Channel};
use crate::host::open::{Browse, Failed, Open, Openers};
use crate::host::sound::{self, Cue, Player};
use crate::host::theme::HostTheme;
use crate::paths;
use crate::plugins::{Kind, Places, Supervisor};
use crate::protocol::{self, ClientMessage, Hello, ServerMessage};
use crate::remote::{self, Stderr};
use crate::ui::remote::{self as notice, Notice};
use crate::{host::pointer, version};

enum Incoming {
    Input(Event),
    Activated(String),
    OpenFailed(Failed),
    InputEnded,
    Server(u64, Received),
    Linked(u64, Result<Link>),
    Signal,
    Tmux(Option<Outer>),
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
    Sound(Cue),
    Open(Open),
    Browse(Browse),
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
        Received::Message(ServerMessage::Sound(cue)) => Next::Sound(cue),
        Received::Message(ServerMessage::Open(open)) => Next::Open(open),
        Received::Message(ServerMessage::Browse(browse)) => Next::Browse(browse),
        Received::Message(ServerMessage::Detached | ServerMessage::Shutdown) | Received::Ended => {
            Next::End(Ending::Detached)
        }
    })
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
        if self.failures == 0 { Duration::ZERO } else { remote::backoff(self.failures) }
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
    pointer: Option<pointer::Terminal>,
    name: Option<String>,
    facts: Facts,
    graphics: Support,
    probe: String,
    tty: Option<String>,
    tmux_check: TmuxCheck,
    generation: u64,
    out: Option<Sender<ClientMessage>>,
    guard: Option<Guard>,
    lost: Option<Lost>,
    retries: Retries,
    player: Option<Player>,
    openers: Openers,
}

impl Window<'_> {
    fn link(&mut self, link: Link) -> Result<()> {
        self.generation += 1;
        let Link { reader, writer, guard } = link;
        let size = self.terminal.size()?;
        let out = spawn_writer(writer);
        let (version, build) = (protocol::VERSION, version::build_id());
        let (theme, notify, terminal) = (self.theme.clone(), self.notify, self.name.clone());
        let (pointer, graphics, probe) = (self.pointer, self.graphics.clone(), self.probe.clone());
        let (cols, rows) = (size.width, size.height);
        let hello = Hello {
            version,
            build,
            cols,
            rows,
            theme,
            notify,
            terminal,
            pointer,
            graphics,
            probe,
            zone: crate::queue::clock::local_zone(),
            openers: self.openers.ids(),
        };
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
            Incoming::Signal | Incoming::InputEnded => Ok(Some(Ending::Detached)),
            Incoming::Input(ev) => self.input(ev),
            Incoming::Activated(token) => {
                self.send_out(ClientMessage::Activated(token));
                Ok(None)
            }
            Incoming::OpenFailed(failed) => {
                self.send_out(ClientMessage::OpenFailed(failed));
                Ok(None)
            }
            Incoming::Tmux(outer) => {
                self.tmux_checked(outer);
                Ok(None)
            }
            Incoming::Server(generation, received) if generation == self.generation => self.handle_server(received),
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

    fn send_out(&self, message: ClientMessage) {
        if let Some(out) = &self.out {
            let _ = out.send(message);
        }
    }

    fn handle_server(&mut self, received: Received) -> Result<Option<Ending>> {
        match next(received, self.target.reconnects())? {
            Next::Show(bytes) => {
                self.retries.showed();
                let mut out = stdout();
                out.write_all(&bytes)?;
                out.flush()?;
            }
            Next::End(ending) => return Ok(Some(ending)),
            Next::Reconnect => self.lose()?,
            Next::Sound(cue) => {
                if let Some(player) = &mut self.player {
                    player.play(&cue, Instant::now());
                }
            }
            Next::Open(open) => {
                let tx = self.tx.clone();
                self.openers.open(&open, move |failed| {
                    let _ = tx.send(Incoming::OpenFailed(failed));
                });
            }
            Next::Browse(browse) => {
                let tx = self.tx.clone();
                self.openers.browse(browse, move |failed| {
                    let _ = tx.send(Incoming::OpenFailed(failed));
                });
            }
            Next::Nothing => {}
        }
        Ok(None)
    }

    fn input(&mut self, ev: Event) -> Result<Option<Ending>> {
        if matches!(ev, Event::Resize(..)) {
            self.facts.resized(window_cell());
            if self.decide_again() {
                self.send_graphics();
            }
            if self.facts.checks_tmux_again() {
                self.check_tmux();
            }
        }
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

    fn decide_again(&mut self) -> bool {
        let graphics = Support { id_hi: self.graphics.id_hi, ..detect::decide(&self.facts, env) };
        let changed = graphics != self.graphics;
        self.graphics = graphics;
        changed
    }

    fn send_graphics(&self) {
        if let Some(out) = &self.out {
            let _ = out.send(ClientMessage::Graphics(self.graphics.clone()));
        }
    }

    fn check_tmux(&mut self) {
        if self.tmux_check.running {
            self.tmux_check.again = true;
            return;
        }
        self.tmux_check.running = true;
        let (tx, tty) = (self.tx.clone(), self.tty.clone());
        thread::spawn(move || {
            let outer = start_tmux_check().and_then(|child| finish_tmux_check(child, tty.as_deref()));
            let _ = tx.send(Incoming::Tmux(outer));
        });
    }

    fn tmux_checked(&mut self, outer: Option<Outer>) {
        self.tmux_check.running = false;
        if let Some(outer @ Outer::Local { .. }) = outer {
            self.facts.tmux = Some(outer);
            if self.decide_again() {
                self.send_graphics();
            }
        }
        if std::mem::take(&mut self.tmux_check.again) {
            self.check_tmux();
        }
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

#[derive(Debug, Default)]
struct TmuxCheck {
    running: bool,
    again: bool,
}

pub(super) fn attach(link: Link, target: &Target, terminal: &mut DefaultTerminal, host: Host) -> Result<Ending> {
    let Host { theme, facts, graphics, probe, tty, probe_timed_out, pointer } = host;
    let name = facts.replies.terminal.clone();
    let notify = notify::detect(name.as_deref(), env);
    let (tx, rx) = mpsc::channel();
    let mut window = Window {
        terminal,
        target,
        tx: tx.clone(),
        theme,
        notify,
        pointer,
        name,
        facts,
        graphics,
        probe,
        tty,
        tmux_check: TmuxCheck::default(),
        generation: 0,
        out: None,
        guard: None,
        lost: None,
        retries: Retries::default(),
        player: local_player(),
        openers: openers_for(target),
    };
    window.link(link)?;
    let _threads = Threads::spawn(tx, probe_timed_out)?;
    let _plugins = matches!(target, Target::Local).then(|| {
        let places = Places::new(&paths::config(), &paths::session());
        Supervisor::start(Kind::Window, places, Vec::new())
    });
    run_window(&mut window, &rx)
}

fn local_player() -> Option<Player> {
    sound::local_machine(|name| std::env::var(name).ok())
        .then(|| Player::detect(paths::session().parent().map(Path::to_path_buf).unwrap_or_default()))
}

fn openers_for(target: &Target) -> Openers {
    if matches!(target, Target::Local) { Openers::detect(|name| std::env::var(name).ok()) } else { Openers::default() }
}

fn run_window(window: &mut Window<'_>, rx: &Receiver<Incoming>) -> Result<Ending> {
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

fn spawn_reader(generation: u64, mut reader: Box<dyn Source>, tx: Sender<Incoming>) {
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

struct InputEnd(Sender<Incoming>);

impl Drop for InputEnd {
    fn drop(&mut self) {
        let _ = self.0.send(Incoming::InputEnded);
    }
}

struct Threads {
    stop: Arc<AtomicBool>,
    stopped: Receiver<()>,
    signals: Handle,
}

impl Threads {
    fn spawn(tx: Sender<Incoming>, probe_timed_out: bool) -> Result<Self> {
        let mut signals = Signals::new([SIGTERM, SIGHUP, SIGINT, SIGWINCH]).map_err(Error::Signals)?;
        let handle = signals.handle();
        let signalled = tx.clone();
        thread::spawn(move || forward_signals(&mut signals, &signalled));
        let stop = Arc::new(AtomicBool::new(false));
        let (done, stopped) = mpsc::channel();
        let asked = Arc::clone(&stop);
        thread::Builder::new().name("input".into()).spawn(move || {
            let _done = done;
            read_input(&tx, &asked, probe_timed_out);
        })?;
        Ok(Self { stop, stopped, signals: handle })
    }
}

fn signal_incoming(signal: i32) -> Option<Incoming> {
    if signal != SIGWINCH {
        return Some(Incoming::Signal);
    }
    let (cols, rows) = crossterm::terminal::size().ok()?;
    Some(Incoming::Input(Event::Resize(cols, rows)))
}

fn forward_signals(signals: &mut Signals, tx: &Sender<Incoming>) {
    for signal in signals.forever() {
        let Some(incoming) = signal_incoming(signal) else { continue };
        if tx.send(incoming).is_err() || signal != SIGWINCH {
            break;
        }
    }
}

fn read_input(tx: &Sender<Incoming>, stop: &AtomicBool, probe_timed_out: bool) {
    let _ended = InputEnd(tx.clone());
    let input = stdin();
    input::run(
        stop,
        probe_timed_out,
        input::Source {
            poll: |wait| input::readable(&input, wait),
            read: |bytes| rustix::io::read(&input, bytes).map_err(Into::into),
        },
        |input| {
            tx.send(match input {
                input::Input::Event(event) => Incoming::Input(event),
                input::Input::Activated(token) => Incoming::Activated(token),
            })
            .is_ok()
        },
        thread::sleep,
    );
}

impl Drop for Threads {
    fn drop(&mut self) {
        self.signals.close();
        self.stop.store(true, Ordering::Relaxed);
        let _ = self.stopped.recv_timeout(INPUT_POLL * 3);
    }
}
const INCOMPATIBLE: &str = "the running cornercase server is incompatible with this build. \
    Run `cornercase kill-server` (it closes all its terminals) and start cornercase again";
const SECOND: Duration = Duration::from_secs(1);

#[cfg(test)]
mod tests;
