use std::io;
use std::net::{IpAddr, Shutdown, SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::Sender;
use std::thread;
use std::time::{Duration, Instant};

use parking_lot::Mutex;

use super::auth::{Devices, Pairing};
use super::http::{Request, Response};
use super::{ACCEPT_POLL, Event, MAX_MESSAGE, PING_EVERY, SILENT_FOR, bind_now, spawn_counted};
use crate::log;
use crate::process::program;

mod cli;
mod code;
mod command;
mod message;
mod pairing;

pub(crate) use code::machine_code;

const MAX_OPEN: usize = 64;
const STOP_WAIT: Duration = Duration::from_secs(2);
const STOP_POLL: Duration = Duration::from_millis(20);

#[derive(Debug, Clone, Copy)]
pub(crate) struct Limits {
    pub(crate) argv_within: Duration,
    pub(crate) ping_every: Duration,
    pub(crate) silent_for: Duration,
    pub(crate) grace: Duration,
    pub(crate) longest_line: usize,
    pub(crate) queued_bytes: usize,
    pub(crate) largest_message: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            argv_within: Duration::from_secs(10),
            ping_every: PING_EVERY,
            silent_for: SILENT_FOR,
            grace: Duration::from_secs(2),
            longest_line: 16 * 1024 * 1024,
            queued_bytes: 4 * 1024 * 1024,
            largest_message: MAX_MESSAGE,
        }
    }
}

struct Connected {
    id: u64,
    device: String,
    stream: TcpStream,
}

struct Shared {
    accepting: bool,
    pairing: Option<Pairing>,
    taken: u64,
    devices: Devices,
    connections: Vec<Connected>,
    next: u64,
}

impl Shared {
    fn pair_with(&mut self, code: &str, name: &str, now: Instant) -> Option<io::Result<(String, String, u64)>> {
        self.pairing.as_ref().filter(|pairing| pairing.accepts(code, now))?;
        let added = self.devices.add_app(name).map(|(id, secret)| {
            self.pairing = None;
            self.taken += 1;
            (id, secret, self.taken)
        });
        Some(added)
    }
}

#[derive(Clone)]
struct Context {
    shared: Arc<Mutex<Shared>>,
    socket: PathBuf,
    program: PathBuf,
    limits: Limits,
    notify: Arc<dyn Fn(Event) + Send + Sync>,
    open: Arc<AtomicUsize>,
    stopping: Arc<AtomicBool>,
}

impl Context {
    fn tell(&self, event: Event) {
        (self.notify)(event);
    }

    fn admit(&self, credential: Option<&str>, stream: &TcpStream) -> Result<(u64, String), Refused> {
        let credential = credential.ok_or(Refused::NoSecret)?;
        let stream = stream.try_clone().map_err(|_| Refused::Closed)?;
        let mut shared = self.shared.lock();
        let device = shared.devices.check(credential).map(String::from).ok_or(Refused::UnknownSecret)?;
        if !shared.accepting {
            return Err(Refused::Closed);
        }
        shared.next += 1;
        let id = shared.next;
        shared.connections.push(Connected { id, device: device.clone(), stream });
        Ok((id, device))
    }

    fn unregister(&self, id: u64) {
        self.shared.lock().connections.retain(|connected| connected.id != id);
    }
}

enum Refused {
    NoSecret,
    UnknownSecret,
    Closed,
}

type Listeners = Arc<Mutex<Vec<(SocketAddr, TcpListener)>>>;

struct Accepting {
    port: u16,
    listeners: Listeners,
    stop: Arc<AtomicBool>,
    thread: thread::JoinHandle<()>,
}

impl Accepting {
    fn start(port: u16, context: &Context) -> Self {
        let listeners = Listeners::default();
        let stop = Arc::new(AtomicBool::new(false));
        let (shared, stopped, context) = (Arc::clone(&listeners), Arc::clone(&stop), context.clone());
        let thread = thread::spawn(move || accept(&shared, &context, &stopped));
        Self { port, listeners, stop, thread }
    }

    fn update(&self, addresses: &[IpAddr]) -> Listened {
        let wanted: Vec<SocketAddr> = addresses.iter().map(|ip| SocketAddr::new(*ip, self.port)).collect();
        let mut list = self.listeners.lock();
        list.retain(|(address, _)| wanted.contains(address));
        let mut failed = Vec::new();
        for address in &wanted {
            if list.iter().all(|(bound, _)| bound != address) {
                match bind_now(*address) {
                    Ok(listener) => list.push((*address, listener)),
                    Err(e) => failed.push((address.ip(), e)),
                }
            }
        }
        let routes = wanted.iter().filter(|address| list.iter().any(|(bound, _)| bound == *address)).copied().collect();
        Listened { routes, failed }
    }

    fn stop(self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = self.thread.join();
        self.listeners.lock().clear();
    }
}

#[derive(Debug)]
pub(crate) struct Listened {
    pub(crate) routes: Vec<SocketAddr>,
    pub(crate) failed: Vec<(IpAddr, io::Error)>,
}

pub(crate) struct Phone {
    context: Context,
    listening: Option<Accepting>,
}

impl Phone {
    pub(crate) fn new<E: From<Event> + Send + 'static>(apps: PathBuf, socket: PathBuf, notify: Sender<E>) -> Self {
        let shared = Shared {
            accepting: false,
            pairing: None,
            taken: 0,
            devices: Devices::load(apps),
            connections: Vec::new(),
            next: 0,
        };
        let notify: Arc<dyn Fn(Event) + Send + Sync> = Arc::new(move |event| _ = notify.send(E::from(event)));
        let context = Context {
            shared: Arc::new(Mutex::new(shared)),
            socket,
            program: program::own_binary(),
            limits: Limits::default(),
            notify,
            open: Arc::default(),
            stopping: Arc::default(),
        };
        Self { context, listening: None }
    }

    pub(crate) fn listen(&mut self, addresses: &[IpAddr], port: u16) -> Listened {
        if self.listening.as_ref().is_some_and(|accepting| accepting.port != port) {
            self.stop_listening();
        }
        self.context.shared.lock().accepting = true;
        let context = &self.context;
        let accepting = self.listening.get_or_insert_with(|| Accepting::start(port, context));
        accepting.update(addresses)
    }

    fn stop_listening(&mut self) {
        if let Some(accepting) = self.listening.take() {
            accepting.stop();
            log::info!("web", "phone app stopped listening");
        }
    }

    pub(crate) fn close(&mut self) {
        self.context.shared.lock().accepting = false;
        self.stop_listening();
        let context = &self.context;
        context.stopping.store(true, Ordering::SeqCst);
        let deadline = Instant::now() + STOP_WAIT;
        while !context.shared.lock().connections.is_empty() && Instant::now() < deadline {
            thread::sleep(STOP_POLL);
        }
        let mut shared = context.shared.lock();
        for connected in shared.connections.drain(..) {
            let _ = connected.stream.shutdown(Shutdown::Both);
        }
        shared.pairing = None;
        context.stopping.store(false, Ordering::SeqCst);
    }

    pub(crate) fn pair(&self, now: Instant) -> io::Result<(String, u64)> {
        let mut shared = self.context.shared.lock();
        let pairing = Pairing::shared(shared.pairing.take(), now)?;
        let token = pairing.token.clone();
        shared.pairing = Some(pairing);
        Ok((token, shared.taken))
    }

    pub(crate) fn unpair(&self) {
        self.context.shared.lock().pairing = None;
    }

    pub(crate) fn apps(&self) -> Vec<(String, String, u64)> {
        let shared = self.context.shared.lock();
        shared.devices.list().iter().map(|device| (device.id.clone(), device.name.clone(), device.paired)).collect()
    }

    pub(crate) fn forget(&self, id: &str) -> io::Result<bool> {
        let mut shared = self.context.shared.lock();
        let removed = shared.devices.remove(id)?;
        if removed {
            for connected in shared.connections.iter().filter(|connected| connected.device == id) {
                let _ = connected.stream.shutdown(Shutdown::Both);
            }
            log::info!("web", "phone app forgotten", device = id);
        }
        Ok(removed)
    }

    #[cfg(test)]
    pub(crate) fn open(&self) -> usize {
        self.context.open.load(Ordering::SeqCst)
    }

    #[cfg(test)]
    pub(crate) fn set_program(&mut self, program: PathBuf) {
        self.context.program = program;
    }

    #[cfg(test)]
    pub(crate) fn set_limits(&mut self, limits: Limits) {
        self.context.limits = limits;
    }

    #[cfg(test)]
    pub(crate) fn listening(&self) -> Vec<SocketAddr> {
        self.listening
            .as_ref()
            .map(|a| a.listeners.lock().iter().map(|(address, _)| *address).collect())
            .unwrap_or_default()
    }
}

impl Drop for Phone {
    fn drop(&mut self) {
        self.close();
    }
}

fn accept(listeners: &Listeners, context: &Context, stop: &AtomicBool) {
    while !stop.load(Ordering::SeqCst) {
        let accepted: Vec<TcpStream> = listeners
            .lock()
            .iter()
            .filter_map(|(_, listener)| listener.accept().ok().map(|(stream, _)| stream))
            .collect();
        if accepted.is_empty() {
            thread::sleep(ACCEPT_POLL);
        }
        for stream in accepted {
            serve_in_thread(stream, context);
        }
    }
}

fn serve_in_thread(stream: TcpStream, context: &Context) {
    let open = Arc::clone(&context.open);
    let context = context.clone();
    spawn_counted(stream, &open, MAX_OPEN, move |stream, request| serve(stream, &request, &context));
}

fn serve(mut stream: TcpStream, request: &Request, context: &Context) {
    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/") => pairing::pair(stream, request, context),
        ("GET", "/cli") => cli::run(stream, request, context),
        ("GET", _) => {
            let _ = Response::text("404 Not Found", "not found").send(&mut stream);
        }
        _ => {
            let _ = Response::text("405 Method Not Allowed", "only GET").send(&mut stream);
        }
    }
}

#[cfg(test)]
mod tests;
