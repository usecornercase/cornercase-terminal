macro_rules! fixture {
    ($path:literal) => {
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/", $path))
    };
}

pub(crate) use fixture;

mod program;

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use crate::os::fs::canonicalize;
#[cfg(unix)]
use crate::os::fs::set_mode;
#[cfg(unix)]
use crate::os::process::{Pid, Signal, kill_process};

pub(crate) const SHORTCUT_MEMBER: &str = r#"{"mention_name":"ana","workspace2":{"url_slug":"acme"}}"#;

const PROGRAM: &str = include_str!("test_util/program.rs");
const PROGRAM_MAIN: &str = r#"fn main() {
    let program = std::env::current_exe().expect("the fake program's path");
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(run(&program, &args, &mut std::io::stdout(), &mut std::io::stderr()));
}"#;

const PID_WINDOWS_NEVER_GIVES: i32 = i32::MAX - 3;

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
    if cfg!(windows) {
        return PID_WINDOWS_NEVER_GIVES;
    }
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
        Self(canonicalize(&dir).expect("canonicalize temp dir"))
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

#[cfg(unix)]
pub(crate) struct Locked {
    path: PathBuf,
    _dir: TempDir,
}

#[cfg(unix)]
impl Locked {
    pub(crate) fn new() -> Self {
        let dir = TempDir::new();
        let path = dir.path().join("locked");
        std::fs::create_dir(&path).expect("create the locked folder");
        set_mode(&path, 0o000).expect("lock the folder");
        let locked = Self { path, _dir: dir };
        let entered = std::process::Command::new("/bin/sh").arg("-c").arg("true").current_dir(&locked.path).status();
        assert!(entered.is_err(), "a locked folder can still be entered, so the test would prove nothing (root?)");
        locked
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(unix)]
impl Drop for Locked {
    fn drop(&mut self) {
        let _ = set_mode(&self.path, 0o755);
    }
}

#[cfg(unix)]
pub(crate) struct Sleeper(std::process::Child);

#[cfg(unix)]
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
        use crate::os::process::CommandExt;
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

#[cfg(unix)]
impl Drop for Sleeper {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(unix)]
pub(crate) struct Family(std::process::Child);

#[cfg(unix)]
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

#[cfg(unix)]
impl Drop for Family {
    fn drop(&mut self) {
        if let Some(pid) = self.grandchild().and_then(Pid::from_raw) {
            let _ = kill_process(pid, Signal::KILL);
        }
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[cfg(unix)]
pub(crate) fn is_sh(name: &str) -> bool {
    matches!(name, "sh" | "bash" | "dash")
}

#[cfg(unix)]
pub(crate) fn child_running(parent: i32, program: &str) -> i32 {
    let mut found = None;
    wait_until(&format!("a child of {parent} runs {program}"), || {
        found = crate::process::children(parent)
            .into_iter()
            .find(|pid| crate::process::name(*pid).as_deref() == Some(program));
        found.is_some()
    });
    found.expect("the child")
}

#[cfg(unix)]
pub(crate) fn sleep_named(path: &Path) -> Running {
    let copied = std::process::Command::new("cp").arg("/bin/sleep").arg(path).status().expect("run cp");
    assert!(copied.success(), "copy sleep to {}", path.display());
    Running::spawn(std::process::Command::new(path).arg("30"))
}

pub(crate) fn write_executable(path: &Path, contents: &str) {
    if cfg!(windows) {
        std::fs::write(path, contents).unwrap_or_else(|e| panic!("failed to write {}: {e}", path.display()));
        return;
    }
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

#[cfg(unix)]
pub(crate) fn append(path: &Path, text: &str) -> std::fs::File {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new().append(true).open(path).expect("open agent records");
    file.write_all(text.as_bytes()).expect("append agent records");
    file
}

#[cfg(unix)]
pub(crate) const TITLE_SIGNAL: &str = r#"if mv "$2/title" "$2/.shown" 2>/dev/null; then
    printf '\033]0;%s\007' "$(cat "$2/.shown")"
  fi"#;

#[cfg(unix)]
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

    #[cfg(windows)]
    pub(crate) fn first_line(&mut self) -> String {
        use std::io::BufRead;
        let output = self.0.stdout.take().expect("the output is piped");
        let (line, read) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut first = String::new();
            let _ = std::io::BufReader::new(output).read_line(&mut first);
            let _ = line.send(first);
        });
        let first = read.recv_timeout(Duration::from_secs(30));
        first.expect("the child writes its first line within 30 s").trim().to_string()
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

pub(crate) fn fake_program(dir: &Path, name: &str) -> PathBuf {
    let path = dir.join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
    let built = built_program();
    if std::fs::hard_link(built, &path).is_err() {
        copy_program(built, &path);
    }
    path
}

fn copy_program(from: &Path, to: &Path) {
    if cfg!(windows) {
        std::fs::copy(from, to).expect("copy the fake program");
        return;
    }
    let copied = std::process::Command::new("cp").arg(from).arg(to).status().expect("run cp");
    assert!(copied.success(), "copy the fake program to {}", to.display());
}

fn built_program() -> &'static Path {
    static BUILT: OnceLock<PathBuf> = OnceLock::new();
    BUILT.get_or_init(|| build_program(&program_cache()))
}

fn program_cache() -> PathBuf {
    let test_binary = std::env::current_exe().expect("the test binary's path");
    test_binary.parent().expect("the test binary's folder").to_path_buf()
}

fn build_program(cache: &Path) -> PathBuf {
    let source = format!("{PROGRAM}\n{PROGRAM_MAIN}\n");
    let mut hasher = std::hash::DefaultHasher::new();
    std::hash::Hash::hash(&source, &mut hasher);
    let stem = format!("cornercase-test-program-{:016x}", std::hash::Hasher::finish(&hasher));
    let built = cache.join(format!("{stem}{}", std::env::consts::EXE_SUFFIX));
    if !built.exists() {
        compile_program(&source, &cache.join(format!("{stem}-{}", std::process::id())), &built);
    }
    built
}

fn compile_program(source: &str, work: &Path, built: &Path) {
    let _ = std::fs::remove_dir_all(work);
    std::fs::create_dir(work).expect("create the fake program's build folder");
    let (main, output) = (work.join("main.rs"), work.join(format!("program{}", std::env::consts::EXE_SUFFIX)));
    std::fs::write(&main, source).expect("write the fake program's source");
    let status = std::process::Command::new("rustc")
        .args(["--edition", "2024", "-C", "debuginfo=0", "-o"])
        .arg(&output)
        .arg(&main)
        .status()
        .expect("run rustc");
    assert!(status.success(), "rustc could not build the fake program");
    let _ = std::fs::rename(&output, built);
    let _ = std::fs::remove_dir_all(work);
    assert!(built.exists(), "the fake program is not at {}", built.display());
}

#[cfg(unix)]
pub(crate) fn fake_tool(dir: &Path, name: &str, script: &str) -> PathBuf {
    let path = dir.join(name);
    write_executable(&path, &format!("#!/bin/sh\n{script}\n"));
    path
}

pub(crate) const NPM_SHIM: &str = "@ECHO off\r\nGOTO start\r\n:find_dp0\r\nSET dp0=%~dp0\r\nEXIT /b\r\n:start\r\n\
                                   SETLOCAL\r\nCALL :find_dp0\r\n\r\nIF EXIST \"%dp0%\\node.exe\" (\r\n  SET \
                                   \"_prog=%dp0%\\node.exe\"\r\n) ELSE (\r\n  SET \"_prog=node\"\r\n  SET \
                                   PATHEXT=%PATHEXT:;.JS;=;%\r\n)\r\n\r\nendLocal & goto #_undefined_# 2>NUL || \
                                   title %COMSPEC% & \"%_prog%\"  \
                                   \"%dp0%\\node_modules\\@anthropic-ai\\claude-code\\cli.js\" %*\r\n";
pub(crate) const NPM_SCRIPT: [&str; 4] = ["node_modules", "@anthropic-ai", "claude-code", "cli.js"];

pub(crate) fn under(dir: &Path, parts: &[&str]) -> PathBuf {
    parts.iter().fold(dir.to_path_buf(), |path, part| path.join(part))
}

pub(crate) fn npm_install(dir: &Path, shim: &str, files: &[&[&str]]) -> PathBuf {
    for parts in files {
        let file = under(dir, parts);
        std::fs::create_dir_all(file.parent().expect("a folder")).expect("make the folder");
        std::fs::write(file, "").expect("write the file");
    }
    let path = dir.join("claude.cmd");
    std::fs::write(&path, shim).expect("write the shim");
    path
}

#[cfg(unix)]
pub(crate) fn fake_gh(dir: &Path, script: &str) -> PathBuf {
    fake_tool(dir, "gh", script)
}

#[cfg(unix)]
pub(crate) fn fake_glab(dir: &Path, script: &str) -> PathBuf {
    fake_tool(dir, "glab", script)
}

pub(crate) fn link_folder(target: &Path, link: &Path) {
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, link).expect("a symbolic link");
    #[cfg(windows)]
    crate::os::fs::make_junction(link, target).expect("a junction");
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

pub(crate) fn sign_head_and_show_signatures(dir: &Path) {
    let read = |args: &[&str]| {
        let out = std::process::Command::new("git").arg("-C").arg(dir).args(args).output().expect("run git");
        assert!(out.status.success(), "git {args:?} failed: {}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8_lossy(&out.stdout).into_owned()
    };
    let raw = read(&["cat-file", "commit", "HEAD"]);
    let (headers, message) = raw.split_once("\n\n").expect("a commit");
    let signature = "gpgsig -----BEGIN PGP SIGNATURE-----\n \n not really signed\n -----END PGP SIGNATURE-----";
    let folder = TempDir::new();
    let commit = folder.path().join("commit");
    std::fs::write(&commit, format!("{headers}\n{signature}\n\n{message}")).expect("write the commit");
    let signed = read(&["hash-object", "-t", "commit", "-w", &commit.display().to_string()]);
    git(dir, &["update-ref", "HEAD", signed.trim()]);
    git(dir, &["config", "log.showSignature", "true"]);
    git(dir, &["config", "gpg.program", "git"]);
}

pub(crate) fn head(dir: &Path) -> String {
    let out = std::process::Command::new("git").arg("-C").arg(dir).args(["rev-parse", "HEAD"]).output().expect("git");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
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

pub(crate) fn os_path(path: &str) -> String {
    if cfg!(windows) && path.starts_with('/') { format!("C:{path}") } else { path.to_string() }
}

#[cfg(not(unix))]
pub(crate) fn verbatim(path: &Path) -> PathBuf {
    PathBuf::from(format!(r"\\?\{}", path.display()))
}

#[cfg(windows)]
pub(crate) fn holding(file: &Path, vars: &[(&str, &str)]) -> Running {
    holding_reading(file, vars, std::process::Stdio::piped())
}

#[cfg(windows)]
pub(crate) fn holding_reading(file: &Path, vars: &[(&str, &str)], input: std::process::Stdio) -> Running {
    let held = std::fs::OpenOptions::new().create(true).append(true).open(file).expect("open the file to hold");
    Running::spawn(
        std::process::Command::new("findstr")
            .arg("held")
            .envs(vars.iter().copied())
            .stdin(input)
            .stdout(held)
            .stderr(std::process::Stdio::null()),
    )
}

type Route = (&'static str, u16, Vec<u8>);

fn answer(
    stream: &mut std::net::TcpStream,
    routes: &[Route],
    used: &mut [bool],
    log: &parking_lot::Mutex<Vec<String>>,
) {
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
    log.lock().push(request);
    let _ = std::io::Write::write_all(stream, &[head.as_bytes(), body].concat());
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
        answer(&mut stream, routes, &mut used, log);
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

pub(crate) fn free_port() -> u16 {
    static NEXT: std::sync::atomic::AtomicU16 = std::sync::atomic::AtomicU16::new(0);
    let base = 30_000 + u16::try_from(std::process::id() % 80).expect("fits") * 32;
    loop {
        let port = base + NEXT.fetch_add(1, Ordering::SeqCst);
        if std::net::TcpListener::bind(("127.0.0.1", port)).is_ok() {
            return port;
        }
    }
}

pub(crate) fn http_get(port: u16, path: &str) -> String {
    use std::io::{Read, Write};
    let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).expect("connect");
    write!(stream, "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n").expect("request");
    let mut answer = String::new();
    stream.read_to_string(&mut answer).expect("answer");
    answer
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

#[cfg(test)]
mod tests;
