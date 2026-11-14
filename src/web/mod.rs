use std::io;
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::Sender;
use std::thread;
use std::time::{Duration, Instant};

use parking_lot::Mutex;

use crate::log;

pub(crate) mod auth;
mod bridge;
mod http;
mod page;
pub(crate) mod phone;
pub(crate) mod tailscale;

use http::{Request, Response};

pub(crate) const PAIR_PATH: &str = "/pair";
const MAX_CONNECTIONS: usize = 32;
const HEAD_TIMEOUT: Duration = Duration::from_secs(10);
const COOKIE_AGE: u64 = 365 * 24 * 60 * 60;
const ACCEPT_POLL: Duration = Duration::from_millis(50);
const REBIND_FOR: Duration = Duration::from_secs(1);
const POLL: Duration = Duration::from_millis(10);
const WRITE_TIMEOUT: Duration = Duration::from_secs(10);
const PING_EVERY: Duration = Duration::from_secs(15);
const SILENT_FOR: Duration = Duration::from_secs(45);
const MAX_MESSAGE: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Event {
    Paired(u64),
    Browsers(usize),
    AppPaired(u64),
}

struct Shared {
    pairing: Option<auth::Pairing>,
    taken: u64,
    devices: auth::Devices,
    owner: Option<String>,
    browsers: usize,
    connections: Vec<(u64, TcpStream)>,
    next: u64,
}

impl Shared {
    fn pair_with(&mut self, token: &str, browser: &str) -> Option<io::Result<(String, String, u64)>> {
        self.pairing.as_ref().filter(|pairing| pairing.accepts(token, Instant::now()))?;
        let added = self.devices.add(browser).map(|(id, cookie)| {
            self.pairing = None;
            self.taken += 1;
            (id, cookie, self.taken)
        });
        Some(added)
    }
}

#[derive(Clone)]
struct Context {
    shared: Arc<Mutex<Shared>>,
    socket: PathBuf,
    notify: Arc<dyn Fn(Event) + Send + Sync>,
    open: Arc<AtomicUsize>,
}

impl Context {
    fn tell(&self, event: Event) {
        (self.notify)(event);
    }

    fn register(&self, stream: &TcpStream) -> Option<u64> {
        let clone = stream.try_clone().ok()?;
        let mut shared = self.shared.lock();
        shared.next += 1;
        let id = shared.next;
        shared.connections.push((id, clone));
        Some(id)
    }

    fn unregister(&self, id: u64) {
        self.shared.lock().connections.retain(|(c, _)| *c != id);
    }

    fn browser(&self, delta: isize) {
        let count = {
            let mut shared = self.shared.lock();
            shared.browsers = shared.browsers.saturating_add_signed(delta);
            shared.browsers
        };
        self.tell(Event::Browsers(count));
    }
}

struct Listening {
    port: u16,
    stop: Arc<AtomicBool>,
    thread: thread::JoinHandle<()>,
}

pub(crate) struct Web {
    context: Context,
    listening: Option<Listening>,
}

impl Web {
    pub(crate) fn new<E: From<Event> + Send + 'static>(devices: PathBuf, socket: PathBuf, notify: Sender<E>) -> Self {
        let shared = Shared {
            pairing: None,
            taken: 0,
            devices: auth::Devices::load(devices),
            owner: None,
            browsers: 0,
            connections: Vec::new(),
            next: 0,
        };
        let notify: Arc<dyn Fn(Event) + Send + Sync> = Arc::new(move |event| _ = notify.send(E::from(event)));
        let context = Context { shared: Arc::new(Mutex::new(shared)), socket, notify, open: Arc::default() };
        Self { context, listening: None }
    }

    pub(crate) fn listen(&mut self, port: u16) -> io::Result<()> {
        if self.listening.as_ref().is_some_and(|l| l.port == port) {
            return Ok(());
        }
        self.close();
        let listener = bind(SocketAddr::from(([127, 0, 0, 1], port)))?;
        let stop = Arc::new(AtomicBool::new(false));
        let (context, stopped) = (self.context.clone(), Arc::clone(&stop));
        let thread = thread::spawn(move || accept(&listener, &context, &stopped));
        log::info!("web", "listening", port = port);
        self.listening = Some(Listening { port, stop, thread });
        Ok(())
    }

    pub(crate) fn close(&mut self) {
        let Some(listening) = self.listening.take() else { return };
        listening.stop.store(true, Ordering::SeqCst);
        let _ = listening.thread.join();
        self.disconnect_all();
        self.context.shared.lock().pairing = None;
        log::info!("web", "stopped listening", port = listening.port);
    }

    #[cfg(test)]
    pub(crate) fn listening(&self) -> Option<u16> {
        self.listening.as_ref().map(|l| l.port)
    }

    pub(crate) fn set_owner(&self, owner: Option<String>) {
        self.context.shared.lock().owner = owner;
    }

    pub(crate) fn pair(&self, now: Instant) -> io::Result<(String, u64)> {
        let mut shared = self.context.shared.lock();
        let pairing = auth::Pairing::shared(shared.pairing.take(), now)?;
        let token = pairing.token.clone();
        shared.pairing = Some(pairing);
        Ok((token, shared.taken))
    }

    pub(crate) fn unpair(&self) {
        self.context.shared.lock().pairing = None;
    }

    pub(crate) fn paired(&self) -> usize {
        self.context.shared.lock().devices.len()
    }

    pub(crate) fn forget(&self) -> io::Result<()> {
        self.context.shared.lock().devices.clear()?;
        self.disconnect_all();
        log::info!("web", "paired browsers forgotten");
        Ok(())
    }

    fn disconnect_all(&self) {
        for (_, stream) in self.context.shared.lock().connections.drain(..) {
            let _ = stream.shutdown(Shutdown::Both);
        }
    }
}

impl Drop for Web {
    fn drop(&mut self) {
        self.close();
    }
}

fn bind_now(address: SocketAddr) -> io::Result<TcpListener> {
    let listener = TcpListener::bind(address)?;
    listener.set_nonblocking(true)?;
    Ok(listener)
}

fn bind(address: SocketAddr) -> io::Result<TcpListener> {
    let deadline = Instant::now() + REBIND_FOR;
    loop {
        match bind_now(address) {
            Err(e) if e.kind() == io::ErrorKind::AddrInUse && Instant::now() < deadline => thread::sleep(ACCEPT_POLL),
            other => return other,
        }
    }
}

fn accept(listener: &TcpListener, context: &Context, stop: &AtomicBool) {
    while !stop.load(Ordering::SeqCst) {
        let Ok((stream, _)) = listener.accept() else {
            thread::sleep(ACCEPT_POLL);
            continue;
        };
        let served = context.clone();
        spawn_counted(stream, &context.open, MAX_CONNECTIONS, move |mut stream, request| {
            if let Some(response) = route(&request, &served, &stream) {
                let _ = response.send(&mut stream);
            }
        });
    }
}

fn spawn_counted(
    stream: TcpStream,
    open: &Arc<AtomicUsize>,
    most: usize,
    work: impl FnOnce(TcpStream, Request) + Send + 'static,
) {
    if stream.set_nonblocking(false).is_err() {
        return;
    }
    if open.fetch_add(1, Ordering::SeqCst) >= most {
        open.fetch_sub(1, Ordering::SeqCst);
        return;
    }
    let open = Arc::clone(open);
    thread::spawn(move || {
        read_head(stream, work);
        open.fetch_sub(1, Ordering::SeqCst);
    });
}

fn read_head(mut stream: TcpStream, work: impl FnOnce(TcpStream, Request)) {
    let _ = stream.set_read_timeout(Some(HEAD_TIMEOUT));
    let _ = stream.set_write_timeout(Some(HEAD_TIMEOUT));
    let Ok(Some(request)) = Request::read(&mut stream) else { return };
    work(stream, request);
}

fn route(request: &Request, context: &Context, stream: &TcpStream) -> Option<Response> {
    if request.method != "GET" {
        return Some(Response::text("405 Method Not Allowed", "only GET"));
    }
    if let Some(asset) = page::asset(&request.path) {
        return Some(asset);
    }
    if !owner_allowed(request, context) {
        log::warning!("web", "refused another tailnet user", path = request.path);
        return Some(Response::html("403 Forbidden", page::OTHER_USER));
    }
    match request.path.as_str() {
        PAIR_PATH => Some(pair(request, context)),
        "/" if paired(request, context) => Some(Response::html("200 OK", page::INDEX)),
        "/" => Some(Response::html("401 Unauthorized", page::NOT_PAIRED)),
        "/ws" if paired(request, context) && request.same_origin() && request.wants_websocket() => {
            let Ok(stream) = stream.try_clone() else { return None };
            bridge::run(stream, request, context);
            None
        }
        "/ws" if !request.wants_websocket() => Some(Response::text("426 Upgrade Required", "a WebSocket is needed")),
        "/ws" => {
            log::warning!(
                "web",
                "refused a connection",
                paired = paired(request, context),
                origin = request.header("origin").unwrap_or("none"),
                host = request.header("host").unwrap_or("none"),
                forwarded = request.header("x-forwarded-host").unwrap_or("none"),
            );
            Some(Response::text("403 Forbidden", "not paired"))
        }
        _ => Some(Response::text("404 Not Found", "not found")),
    }
}

fn owner_allowed(request: &Request, context: &Context) -> bool {
    let login = request.header("tailscale-user-login");
    if login.is_none() && !request.forwarded() {
        return true;
    }
    login.is_some() && context.shared.lock().owner.as_deref() == login
}

fn paired(request: &Request, context: &Context) -> bool {
    request.cookie(auth::COOKIE).is_some_and(|cookie| context.shared.lock().devices.check(cookie).is_some())
}

fn pair(request: &Request, context: &Context) -> Response {
    let token = request.query("t").unwrap_or_default();
    let browser = request.header("user-agent").unwrap_or("unknown browser");
    let added = context.shared.lock().pair_with(token, browser);
    match added {
        Some(Ok((id, cookie, count))) => {
            log::info!("web", "browser paired", device = id, browser = browser);
            context.tell(Event::Paired(count));
            let secure = if request.header("tailscale-user-login").is_some() { "; Secure" } else { "" };
            let value =
                format!("{}={cookie}; Path=/; Max-Age={COOKIE_AGE}; HttpOnly; SameSite=Lax{secure}", auth::COOKIE);
            Response::redirect("/").with("Set-Cookie", value)
        }
        Some(Err(e)) => {
            log::error!("web", "could not save a paired browser", error = e);
            Response::html("500 Internal Server Error", page::NOT_PAIRED)
        }
        None => {
            log::warning!("web", "refused a pairing code");
            Response::html("403 Forbidden", page::EXPIRED)
        }
    }
}
