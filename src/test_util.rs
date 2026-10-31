macro_rules! fixture {
    ($path:literal) => {
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/", $path))
    };
}

pub(crate) use fixture;

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

pub(crate) const SHORTCUT_MEMBER: &str = r#"{"mention_name":"ana","workspace2":{"url_slug":"acme"}}"#;

const TIMEOUT: Duration = Duration::from_secs(5);
const POLL: Duration = Duration::from_millis(20);

pub(crate) fn wait_until(what: &str, cond: impl FnMut() -> bool) {
    wait_until_within(what, TIMEOUT, cond);
}

pub(crate) fn wait_until_within(what: &str, timeout: Duration, mut cond: impl FnMut() -> bool) {
    let deadline = Instant::now() + timeout;
    while !cond() {
        assert!(Instant::now() < deadline, "timed out waiting for: {what}");
        std::thread::sleep(POLL);
    }
}

pub(crate) fn this_pid() -> i32 {
    i32::try_from(std::process::id()).expect("pid fits in i32")
}

pub(crate) fn exited_pid() -> i32 {
    let mut child = std::process::Command::new("/bin/sh").arg("-c").arg("exit 0").spawn().expect("spawn sh");
    child.wait().expect("wait for sh");
    i32::try_from(child.id()).expect("pid fits in i32")
}

pub(crate) fn spawn_term(
    id: u64,
    args: &[String],
    cwd: Option<PathBuf>,
) -> (crate::term::Term, std::sync::mpsc::Receiver<crate::term::Event>) {
    let (tx, rx) = std::sync::mpsc::channel();
    let opts = crate::term::SpawnOptions {
        id,
        shell: "/bin/sh",
        args,
        env: &[],
        remove: &[],
        rows: 24,
        cols: 80,
        cwd,
        theme: &crate::host::theme::HostTheme::default(),
    };
    (crate::term::Term::spawn(opts, tx).expect("spawn /bin/sh"), rx)
}

pub(crate) struct TempDir(PathBuf);

impl TempDir {
    pub(crate) fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("cornercase-test-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        Self(dir.canonicalize().expect("canonicalize temp dir"))
    }

    pub(crate) fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub(crate) struct Locked {
    path: PathBuf,
    _dir: TempDir,
}

impl Locked {
    pub(crate) fn new() -> Self {
        let dir = TempDir::new();
        let path = dir.path().join("locked");
        std::fs::create_dir(&path).expect("create the locked folder");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).expect("lock the folder");
        let locked = Self { path, _dir: dir };
        let entered = std::process::Command::new("/bin/sh").arg("-c").arg("true").current_dir(&locked.path).status();
        assert!(entered.is_err(), "a locked folder can still be entered, so the test would prove nothing (root?)");
        locked
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for Locked {
    fn drop(&mut self) {
        let _ = std::fs::set_permissions(&self.path, std::fs::Permissions::from_mode(0o755));
    }
}

pub(crate) struct Sleeper(std::process::Child);

impl Sleeper {
    pub(crate) fn with_env(vars: &[(&str, &str)]) -> Self {
        Self::named(Path::new("/bin/sleep"), &std::env::temp_dir(), vars)
    }

    pub(crate) fn named(name: &Path, dir: &Path, vars: &[(&str, &str)]) -> Self {
        Self::spawn(name, dir, vars, std::process::Stdio::inherit())
    }

    pub(crate) fn holding(name: &Path, dir: &Path, vars: &[(&str, &str)], file: &Path) -> Self {
        Self::spawn(name, dir, vars, std::fs::File::open(file).expect("open the file to hold").into())
    }

    fn spawn(name: &Path, dir: &Path, vars: &[(&str, &str)], stdin: std::process::Stdio) -> Self {
        use std::os::unix::process::CommandExt;
        let child = std::process::Command::new("/bin/sleep")
            .arg0(name)
            .arg("30")
            .current_dir(dir)
            .env_clear()
            .envs(vars.iter().copied())
            .stdin(stdin)
            .spawn()
            .expect("spawn sleep");
        let sleeper = Self(child);
        wait_until("sleep starts", || {
            crate::process::args(sleeper.pid()).first().is_some_and(|a| Path::new(a) == name)
        });
        sleeper
    }

    pub(crate) fn pid(&self) -> i32 {
        i32::try_from(self.0.id()).expect("pid fits in i32")
    }

    pub(crate) fn stop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

impl Drop for Sleeper {
    fn drop(&mut self) {
        self.stop();
    }
}

pub(crate) struct Family(std::process::Child);

impl Family {
    pub(crate) fn new() -> Self {
        let child = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg("/bin/sh -c '/bin/sleep 30; :'; :")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn sh");
        let family = Self(child);
        wait_until("the grandchild starts", || family.grandchild().is_some());
        family
    }

    pub(crate) fn pid(&self) -> i32 {
        i32::try_from(self.0.id()).expect("pid fits in i32")
    }

    pub(crate) fn child(&self) -> Option<i32> {
        crate::process::children(self.pid()).first().copied()
    }

    pub(crate) fn grandchild(&self) -> Option<i32> {
        crate::process::children(self.child()?).first().copied()
    }
}

impl Drop for Family {
    fn drop(&mut self) {
        if let Some(pid) = self.grandchild().and_then(rustix::process::Pid::from_raw) {
            let _ = rustix::process::kill_process(pid, rustix::process::Signal::KILL);
        }
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

pub(crate) fn is_sh(name: &str) -> bool {
    matches!(name, "sh" | "bash" | "dash")
}

pub(crate) fn sleep_named(path: &Path) -> Running {
    let copied = std::process::Command::new("cp").arg("/bin/sleep").arg(path).status().expect("run cp");
    assert!(copied.success(), "copy sleep to {}", path.display());
    Running::spawn(std::process::Command::new(path).arg("30"))
}

pub(crate) fn write_executable(path: &Path, contents: &str) {
    let mut child = std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg("cat > \"$1\" && chmod 755 \"$1\"")
        .arg("sh")
        .arg(path)
        .stdin(std::process::Stdio::piped())
        .spawn()
        .expect("run sh");
    std::io::Write::write_all(&mut child.stdin.take().expect("stdin"), contents.as_bytes()).expect("write script");
    assert!(child.wait().expect("wait for sh").success(), "failed to write {}", path.display());
}

pub(crate) fn append(path: &Path, text: &str) -> std::fs::File {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new().append(true).open(path).expect("open agent records");
    file.write_all(text.as_bytes()).expect("append agent records");
    file
}

pub(crate) const TITLE_SIGNAL: &str = r#"if mv "$2/title" "$2/.shown" 2>/dev/null; then
    printf '\033]0;%s\007' "$(cat "$2/.shown")"
  fi"#;

pub(crate) fn signal(dir: &Path, name: &str, contents: &str) {
    let next = dir.join(format!(".{name}"));
    std::fs::write(&next, contents).expect("write an agent signal");
    std::fs::rename(next, dir.join(name)).expect("put the signal in place");
}

pub(crate) struct Running(std::process::Child);

impl Running {
    pub(crate) fn spawn(command: &mut std::process::Command) -> Self {
        Self(command.spawn().expect("spawn a fake"))
    }

    pub(crate) fn pid(&self) -> i32 {
        i32::try_from(self.0.id()).expect("pid fits in i32")
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

pub(crate) fn fake_tool(dir: &Path, name: &str, script: &str) -> PathBuf {
    let path = dir.join(name);
    write_executable(&path, &format!("#!/bin/sh\n{script}\n"));
    path
}

pub(crate) fn fake_gh(dir: &Path, script: &str) -> PathBuf {
    fake_tool(dir, "gh", script)
}

pub(crate) fn fake_glab(dir: &Path, script: &str) -> PathBuf {
    fake_tool(dir, "glab", script)
}

pub(crate) fn git(dir: &Path, args: &[&str]) {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=test", "-c", "user.email=test@example.com", "-c", "init.defaultBranch=main"])
        .args(args)
        .output()
        .expect("run git");
    assert!(out.status.success(), "git {args:?} failed: {}", String::from_utf8_lossy(&out.stderr));
}

pub(crate) fn git_repo(files: &[(&str, &str)]) -> TempDir {
    let repo = TempDir::new();
    git(repo.path(), &["init", "--quiet"]);
    for (name, contents) in files {
        let path = repo.path().join(name);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).expect("create folder");
        }
        std::fs::write(path, contents).expect("write file");
    }
    git(repo.path(), &["add", "--all"]);
    git(repo.path(), &["commit", "--quiet", "--allow-empty", "-m", "init"]);
    repo
}

pub(crate) struct Checkouts {
    pub(crate) repo: TempDir,
    trees: Vec<TempDir>,
}

impl Checkouts {
    pub(crate) fn new(files: &[(&str, &str)]) -> Self {
        Self { repo: git_repo(files), trees: Vec::new() }
    }

    pub(crate) fn add(&mut self, branch: &str, from: &str) -> PathBuf {
        let tree = TempDir::new();
        let path = tree.path().join(branch);
        git(self.repo.path(), &["worktree", "add", "--quiet", "-b", branch, &path.display().to_string(), from]);
        self.trees.push(tree);
        path
    }
}

pub(crate) fn clone_of(remote: &Path) -> (TempDir, PathBuf) {
    let dir = TempDir::new();
    git(dir.path(), &["clone", "--quiet", &remote.display().to_string(), "clone"]);
    let path = dir.path().join("clone");
    (dir, path)
}

type Route = (&'static str, u16, Vec<u8>);

fn answer(stream: &mut std::net::TcpStream, routes: &[Route], used: &mut [bool]) -> String {
    let request = read_request(stream);
    let line = request.lines().next().unwrap_or_default().to_string();
    let mut parts = line.split(' ');
    let method = parts.next().unwrap_or_default();
    let path = parts.next().unwrap_or_default().split('?').next().unwrap_or_default();
    let key = format!("{method} {path}");
    let matching: Vec<usize> = (0..routes.len()).filter(|&i| routes[i].0 == key).collect();
    let pick = matching.iter().copied().find(|&i| !used[i]).or_else(|| matching.last().copied());
    let (status, body) = pick.map_or((404, b"{}".as_slice()), |i| {
        used[i] = true;
        (routes[i].1, routes[i].2.as_slice())
    });
    let (location, body) = if (300..400).contains(&status) {
        (format!("Location: {}\r\n", String::from_utf8_lossy(body)), b"".as_slice())
    } else {
        (String::new(), body)
    };
    let head = format!(
        "HTTP/1.1 {status} X\r\n{location}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = std::io::Write::write_all(stream, &[head.as_bytes(), body].concat());
    request
}

fn serve(
    listener: &std::net::TcpListener,
    routes: &[Route],
    log: &parking_lot::Mutex<Vec<String>>,
    stopped: &std::sync::atomic::AtomicBool,
) {
    let mut used = vec![false; routes.len()];
    for stream in listener.incoming() {
        if stopped.load(Ordering::Relaxed) {
            return;
        }
        let Ok(mut stream) = stream else { continue };
        log.lock().push(answer(&mut stream, routes, &mut used));
    }
}

pub(crate) struct FakeHttp {
    addr: std::net::SocketAddr,
    requests: std::sync::Arc<parking_lot::Mutex<Vec<String>>>,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl FakeHttp {
    pub(crate) fn start<B: Into<Vec<u8>>>(routes: Vec<(&'static str, u16, B)>) -> Self {
        let routes: Vec<Route> = routes.into_iter().map(|(k, s, b)| (k, s, b.into())).collect();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind fake http");
        let addr = listener.local_addr().expect("fake http address");
        let requests = std::sync::Arc::new(parking_lot::Mutex::new(Vec::new()));
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (log, stopped) = (std::sync::Arc::clone(&requests), std::sync::Arc::clone(&stop));
        std::thread::spawn(move || serve(&listener, &routes, &log, &stopped));
        Self { addr, requests, stop }
    }

    pub(crate) fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    pub(crate) fn request(&self, i: usize) -> String {
        self.requests.lock().get(i).cloned().unwrap_or_default()
    }

    pub(crate) fn requests(&self) -> Vec<String> {
        self.requests.lock().clone()
    }
}

pub(crate) fn redirected(route: &'static str) -> (FakeHttp, FakeHttp) {
    let elsewhere = FakeHttp::start(vec![(route, 200, "{}".to_string())]);
    let path = route.split_once(' ').map_or(route, |(_, path)| path);
    let server = FakeHttp::start(vec![(route, 302, format!("{}{path}", elsewhere.url()))]);
    (server, elsewhere)
}

impl Drop for FakeHttp {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = std::net::TcpStream::connect(self.addr);
    }
}

fn read_request(stream: &mut std::net::TcpStream) -> String {
    use std::io::Read;
    let mut data = Vec::new();
    let mut buf = [0u8; 4096];
    while let Ok(n @ 1..) = stream.read(&mut buf) {
        data.extend_from_slice(&buf[..n]);
        let text = String::from_utf8_lossy(&data);
        if let Some(end) = text.find("\r\n\r\n") {
            let length = text[..end]
                .lines()
                .find_map(|l| l.to_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse().unwrap_or(0)))
                .unwrap_or(0usize);
            if data.len() >= end + 4 + length {
                break;
            }
        }
    }
    String::from_utf8_lossy(&data).into_owned()
}

pub(crate) fn decoded(request: &str) -> String {
    let line = request.lines().next().unwrap_or_default().replace('+', " ");
    let mut out = Vec::new();
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match (bytes[i], bytes.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(&String::from_utf8_lossy(h), 16).ok()))
        {
            (b'%', Some(byte)) => {
                out.push(byte);
                i += 3;
            }
            (byte, _) => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

const ANSI: [(u8, u8, u8); 16] = [
    (21, 22, 30),
    (247, 118, 142),
    (158, 206, 106),
    (224, 175, 104),
    (122, 162, 247),
    (187, 154, 247),
    (125, 207, 255),
    (169, 177, 214),
    (65, 72, 104),
    (247, 118, 142),
    (158, 206, 106),
    (224, 175, 104),
    (122, 162, 247),
    (187, 154, 247),
    (125, 207, 255),
    (192, 202, 245),
];

pub(crate) fn theme(background: libghostty_vt::style::RgbColor) -> crate::host::theme::HostTheme {
    use libghostty_vt::style::RgbColor;
    let mut theme = crate::host::theme::HostTheme {
        foreground: Some(RgbColor { r: 192, g: 202, b: 245 }),
        background: Some(background),
        ..crate::host::theme::HostTheme::default()
    };
    for (slot, (r, g, b)) in theme.palette.iter_mut().zip(ANSI) {
        *slot = Some(RgbColor { r, g, b });
    }
    theme
}

pub(crate) struct BrokenPipe;

impl std::io::Write for BrokenPipe {
    fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
        Err(std::io::ErrorKind::BrokenPipe.into())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
