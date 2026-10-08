use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

const TIMEOUT: Duration = Duration::from_secs(5);
const POLL: Duration = Duration::from_millis(20);

pub fn wait_until(what: &str, cond: impl FnMut() -> bool) {
    wait_until_within(what, TIMEOUT, cond);
}

pub fn wait_until_within(what: &str, timeout: Duration, mut cond: impl FnMut() -> bool) {
    let deadline = Instant::now() + timeout;
    while !cond() {
        assert!(Instant::now() < deadline, "timed out waiting for: {what}");
        std::thread::sleep(POLL);
    }
}

pub fn this_pid() -> i32 {
    i32::try_from(std::process::id()).expect("pid fits in i32")
}

pub fn exited_pid() -> i32 {
    let mut child = std::process::Command::new("/bin/sh").arg("-c").arg("exit 0").spawn().expect("spawn sh");
    child.wait().expect("wait for sh");
    i32::try_from(child.id()).expect("pid fits in i32")
}

pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("cornercase-test-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        Self(dir.canonicalize().expect("canonicalize temp dir"))
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub struct Locked {
    path: PathBuf,
    _dir: TempDir,
}

impl Locked {
    pub fn new() -> Self {
        let dir = TempDir::new();
        let path = dir.path().join("locked");
        std::fs::create_dir(&path).expect("create the locked folder");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).expect("lock the folder");
        let locked = Self { path, _dir: dir };
        let entered = std::process::Command::new("/bin/sh").arg("-c").arg("true").current_dir(&locked.path).status();
        assert!(entered.is_err(), "a locked folder can still be entered, so the test would prove nothing (root?)");
        locked
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for Locked {
    fn drop(&mut self) {
        let _ = std::fs::set_permissions(&self.path, std::fs::Permissions::from_mode(0o755));
    }
}

pub struct Sleeper(std::process::Child);

impl Sleeper {
    pub fn with_env(vars: &[(&str, &str)]) -> Self {
        Self::named(Path::new("/bin/sleep"), &std::env::temp_dir(), vars)
    }

    pub fn named(name: &Path, dir: &Path, vars: &[(&str, &str)]) -> Self {
        Self::spawn(name, dir, vars, std::process::Stdio::inherit())
    }

    pub fn holding(name: &Path, dir: &Path, vars: &[(&str, &str)], file: &Path) -> Self {
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

    pub fn pid(&self) -> i32 {
        i32::try_from(self.0.id()).expect("pid fits in i32")
    }

    pub fn stop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

impl Drop for Sleeper {
    fn drop(&mut self) {
        self.stop();
    }
}

pub struct Family(std::process::Child);

impl Family {
    pub fn new() -> Self {
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

    pub fn pid(&self) -> i32 {
        i32::try_from(self.0.id()).expect("pid fits in i32")
    }

    pub fn child(&self) -> Option<i32> {
        crate::process::children(self.pid()).first().copied()
    }

    pub fn grandchild(&self) -> Option<i32> {
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

pub fn is_sh(name: &str) -> bool {
    matches!(name, "sh" | "bash" | "dash")
}

pub fn write_executable(path: &Path, contents: &str) {
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

pub struct FakeCodex {
    pub dir: TempDir,
    pub home: PathBuf,
    pub script: PathBuf,
    pub rollout: PathBuf,
}

impl FakeCodex {
    pub fn new(id: &str, model: &str, wrapper: bool) -> Self {
        let dir = TempDir::new();
        let sessions = dir.path().join("sessions/2026/10/05");
        std::fs::create_dir_all(&sessions).expect("create sessions");
        let rollout = sessions.join(format!("rollout-2026-10-05T12-00-00-{id}.jsonl"));
        let fixture = include_str!("../tests/fixtures/codex/0.160.0/context.jsonl")
            .replace("019a1234-5678-7000-8000-000000000001", id)
            .replace("gpt-5.4", model);
        std::fs::write(&rollout, fixture).expect("write rollout");
        let native = dir.path().join("codex");
        write_executable(
            &native,
            r#"#!/bin/sh
exec 3>> "$1"
while [ -d "$2" ] && [ ! -e "$2/quit" ]; do
  if mv "$2/switch" "$2/.switched" 2>/dev/null; then
    next=$(cat "$2/.switched")
    exec 3>&-
    exec 3>> "$next"
  fi
  if mv "$2/title" "$2/.shown" 2>/dev/null; then
    printf '\033]0;%s\007' "$(cat "$2/.shown")"
  fi
  sleep 0.02
done
"#,
        );
        let script = if wrapper {
            let wrapper = dir.path().join("node_modules/@openai/codex/bin/codex.js");
            std::fs::create_dir_all(wrapper.parent().expect("wrapper parent")).expect("create wrapper folder");
            write_executable(&wrapper, "#!/bin/sh\n\"$1\" \"$2\" \"$3\" &\nwait\n");
            wrapper
        } else {
            native
        };
        let home = dir.path().to_path_buf();
        Self { dir, home, script, rollout }
    }

    pub fn args(&self) -> Vec<PathBuf> {
        let mut args = Vec::new();
        if self.script.extension().is_some_and(|e| e == "js") {
            args.push(self.dir.path().join("codex"));
        }
        args.extend([self.rollout.clone(), self.dir.path().to_path_buf()]);
        args
    }

    pub fn command_line(&self) -> String {
        let args: Vec<String> =
            std::iter::once(self.script.clone()).chain(self.args()).map(|p| p.to_string_lossy().into_owned()).collect();
        format!(
            "env CODEX_HOME={} {}",
            crate::agents::quote(&self.home.to_string_lossy()),
            crate::agents::join_args(&args),
        )
    }

    pub fn signal(&self, name: &str, contents: &str) {
        let next = self.dir.path().join(format!(".{name}"));
        std::fs::write(&next, contents).expect("signal fake codex");
        std::fs::rename(next, self.dir.path().join(name)).expect("put the signal in place");
    }

    pub fn append(&self, text: &str) {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new().append(true).open(&self.rollout).expect("open rollout");
        file.write_all(text.as_bytes()).expect("append rollout");
    }

    pub fn serve(&self, cwd: &Path) -> FakeDaemon {
        let text = std::fs::read_to_string(&self.rollout).expect("read rollout");
        std::fs::write(&self.rollout, text.replace("/tmp/project", &cwd.to_string_lossy())).expect("session folder");
        let script = self.dir.path().join("daemon/codex");
        std::fs::create_dir_all(script.parent().expect("daemon folder")).expect("create daemon folder");
        write_executable(&script, "#!/bin/sh\nexec 3>> \"$2\"\nwhile [ ! -e \"$3/quit\" ]; do sleep 0.02; done\n");
        let child = std::process::Command::new(&script)
            .arg("app-server")
            .arg(&self.rollout)
            .arg(self.dir.path())
            .spawn()
            .expect("spawn fake daemon");
        let pid = i32::try_from(child.id()).expect("pid");
        let state = self.home.join("app-server-daemon");
        std::fs::create_dir_all(&state).expect("create daemon state");
        std::fs::write(state.join("daemon.pid"), format!("{{\"pid\":{pid}}}")).expect("write daemon pid");
        wait_until("the daemon opens the rollout", || crate::process::open_files(pid).contains(&self.rollout));
        FakeDaemon(child)
    }

    pub fn agent(&self, cwd: &Path) -> Sleeper {
        Sleeper::named(&self.dir.path().join("agent/codex"), cwd, &[("CODEX_HOME", &self.home.to_string_lossy())])
    }
}

pub struct FakeDaemon(std::process::Child);

impl Drop for FakeDaemon {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

impl Drop for FakeCodex {
    fn drop(&mut self) {
        self.signal("quit", "");
    }
}

pub struct FakeOpencode {
    pub dir: TempDir,
    pub script: PathBuf,
    pub database: PathBuf,
}

impl FakeOpencode {
    pub const SESSION: &str = "ses_fake";

    pub fn new() -> Self {
        let dir = TempDir::new();
        let data = dir.path().join("data/opencode");
        let cache = dir.path().join("cache/opencode");
        for folder in [&data, &cache] {
            std::fs::create_dir_all(folder).expect("create opencode folders");
        }
        let database = data.join("opencode.db");
        rusqlite::Connection::open(&database)
            .and_then(|db| {
                db.execute_batch(include_str!("../tests/fixtures/opencode/1.18.35/schema.sql"))?;
                db.execute(
                    "INSERT INTO project (id, worktree, time_created, time_updated, sandboxes) \
                     VALUES ('global', '/', 0, 0, '[]')",
                    [],
                )
            })
            .expect("create the database");
        std::fs::write(cache.join("models.json"), include_str!("../tests/fixtures/opencode/1.18.35/models.json"))
            .expect("write models.json");
        let script = dir.path().join("bin/opencode");
        std::fs::create_dir_all(script.parent().expect("bin folder")).expect("create bin folder");
        write_executable(
            &script,
            "#!/bin/sh\nexec 3< \"$1\"\nwhile [ -d \"$2\" ] && [ ! -e \"$2/quit\" ]; do sleep 0.02; done\n",
        );
        Self { dir, script, database }
    }

    pub fn command_line(&self) -> String {
        let quote = |path: &Path| crate::agents::quote(&path.to_string_lossy());
        format!(
            "env XDG_CACHE_HOME={} {} {} {}",
            quote(&self.dir.path().join("cache")),
            quote(&self.script),
            quote(&self.database),
            quote(self.dir.path()),
        )
    }

    pub fn start(&self, cwd: &Path) -> Sleeper {
        let cache = self.dir.path().join("cache");
        Sleeper::holding(&self.script, cwd, &[("XDG_CACHE_HOME", &cache.to_string_lossy())], &self.database)
    }

    pub fn write(&self, session: &str, cwd: &Path, messages: &str) {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).expect("time").as_millis();
        let now = i64::try_from(now).expect("milliseconds");
        let db = rusqlite::Connection::open(&self.database).expect("open the database");
        db.execute(
            "INSERT INTO session (id, project_id, slug, directory, title, version, time_created, time_updated) \
             VALUES (?1, 'global', 'fake', ?2, 'fake', '1.18.35', ?3, ?3) \
             ON CONFLICT(id) DO UPDATE SET directory = excluded.directory, time_updated = excluded.time_updated",
            rusqlite::params![session, cwd.to_string_lossy(), now],
        )
        .expect("write the session");
        for data in messages.lines().filter(|line| !line.trim().is_empty()) {
            let (count, last): (i64, i64) = db
                .query_row("SELECT COUNT(*), COALESCE(MAX(time_created), 0) FROM message", [], |r| {
                    Ok((r.get(0)?, r.get(1)?))
                })
                .expect("count messages");
            db.execute(
                "INSERT INTO message (id, session_id, time_created, time_updated, data) VALUES (?1, ?2, ?3, ?3, ?4)",
                rusqlite::params![format!("msg_{count:06}"), session, now.max(last + 1), data],
            )
            .expect("write a message");
        }
    }

    pub fn parts(&self, session: &str, message: &str, parts: &str) {
        let db = rusqlite::Connection::open(&self.database).expect("open the database");
        for data in parts.lines().filter(|line| !line.trim().is_empty()) {
            let count: i64 = db.query_row("SELECT COUNT(*) FROM part", [], |r| r.get(0)).expect("count parts");
            db.execute(
                "INSERT INTO part (id, message_id, session_id, time_created, time_updated, data) \
                 VALUES (?1, ?2, ?3, ?4, ?4, ?5)",
                rusqlite::params![format!("prt_{count:06}"), message, session, count, data],
            )
            .expect("write a part");
        }
    }

    pub fn quit(&self) {
        std::fs::write(self.dir.path().join("quit"), "").expect("signal fake opencode");
    }
}

impl Drop for FakeOpencode {
    fn drop(&mut self) {
        self.quit();
    }
}

pub fn fake_gh(dir: &Path, script: &str) -> PathBuf {
    let path = dir.join("gh");
    write_executable(&path, &format!("#!/bin/sh\n{script}\n"));
    path
}

pub fn git(dir: &Path, args: &[&str]) {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=test", "-c", "user.email=test@example.com", "-c", "init.defaultBranch=main"])
        .args(args)
        .output()
        .expect("run git");
    assert!(out.status.success(), "git {args:?} failed: {}", String::from_utf8_lossy(&out.stderr));
}

pub fn git_repo(files: &[(&str, &str)]) -> TempDir {
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

pub struct FakeHttp {
    addr: std::net::SocketAddr,
    requests: std::sync::Arc<parking_lot::Mutex<Vec<String>>>,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl FakeHttp {
    pub fn start<B: Into<Vec<u8>>>(routes: Vec<(&'static str, u16, B)>) -> Self {
        let routes: Vec<(&'static str, u16, Vec<u8>)> = routes.into_iter().map(|(k, s, b)| (k, s, b.into())).collect();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind fake http");
        let addr = listener.local_addr().expect("fake http address");
        let requests = std::sync::Arc::new(parking_lot::Mutex::new(Vec::new()));
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (log, stopped) = (std::sync::Arc::clone(&requests), std::sync::Arc::clone(&stop));
        std::thread::spawn(move || {
            let mut used = vec![false; routes.len()];
            for stream in listener.incoming() {
                if stopped.load(Ordering::Relaxed) {
                    return;
                }
                let Ok(mut stream) = stream else { continue };
                let request = read_request(&mut stream);
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
                log.lock().push(request);
                let head = format!(
                    "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = std::io::Write::write_all(&mut stream, &[head.as_bytes(), body].concat());
            }
        });
        Self { addr, requests, stop }
    }

    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    pub fn request(&self, i: usize) -> String {
        self.requests.lock().get(i).cloned().unwrap_or_default()
    }

    pub fn requests(&self) -> Vec<String> {
        self.requests.lock().clone()
    }
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
