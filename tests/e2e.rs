use std::fmt::Write as _;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use cornercase::activity::{CLAUDE_DIR_ENV, CLAUDE_SESSION_ENV};
use cornercase::control::{PANE_ENV, PaneInfo, Report};
use cornercase::protocol::{NESTED_ENV, SOCKET_ENV};
use cornercase::split::{self, Dir};
use cornercase::ui::{self, SidebarRow, WorkspaceRow};
use cornercase::update;
use parking_lot::Mutex;
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use ratatui::layout::{Position, Rect};
use sha2::{Digest, Sha256};

const ROWS: u16 = 24;
const COLS: u16 = 100;
const TIMEOUT: Duration = Duration::from_secs(10);
const POLL: Duration = Duration::from_millis(30);
const AREA: Rect = Rect { x: 0, y: 0, width: COLS, height: ROWS };
const HOST_THEME_REPLY: &[u8] = b"\x1b]11;rgb:12/56/9a\x1b\\\x1bP>|ghostty 1.2.0\x1b\\\x1b[?62;22c";

struct Session {
    dir: PathBuf,
}

impl Session {
    fn new() -> Arc<Self> {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("cornercase-e2e-sock-{}-{n}", std::process::id()));
        std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&dir).expect("create socket dir");
        Arc::new(Self { dir })
    }

    fn socket(&self) -> PathBuf {
        self.dir.join("server.sock")
    }

    fn claude_dir(&self) -> PathBuf {
        self.dir.join("claude")
    }

    fn socket_var(&self) -> String {
        format!("{SOCKET_ENV}={}", self.socket().display())
    }

    #[cfg(target_os = "linux")]
    fn servers(&self) -> usize {
        let socket = self.socket_var();
        let Ok(procs) = std::fs::read_dir("/proc") else { return 0 };
        procs
            .flatten()
            .filter(|entry| {
                let cmdline = std::fs::read(entry.path().join("cmdline")).unwrap_or_default();
                let environ = std::fs::read(entry.path().join("environ")).unwrap_or_default();
                cmdline.split(|b| *b == 0).nth(1) == Some(b"server".as_slice())
                    && environ.split(|b| *b == 0).any(|var| var == socket.as_bytes())
            })
            .count()
    }

    #[cfg(target_os = "macos")]
    fn servers(&self) -> usize {
        let socket = self.socket_var();
        let ps = std::process::Command::new("ps").args(["-xEww", "-o", "command="]).output().expect("run ps");
        String::from_utf8_lossy(&ps.stdout)
            .lines()
            .filter(|line| {
                line.split_whitespace().nth(1) == Some("server") && line.split_whitespace().any(|word| word == socket)
            })
            .count()
    }

    fn wait_for_saved(&self, what: &str, cond: impl Fn(&str) -> bool) {
        self.wait_for_file("server.json", what, cond);
    }

    fn wait_for_file(&self, name: &str, what: &str, cond: impl Fn(&str) -> bool) {
        let path = self.dir.join(name);
        let deadline = Instant::now() + TIMEOUT;
        while !cond(&std::fs::read_to_string(&path).unwrap_or_default()) {
            assert!(Instant::now() < deadline, "timed out waiting for: {what}");
            thread::sleep(POLL);
        }
    }

    fn run(&self, arg: &str) -> std::process::Output {
        self.command().arg(arg).output().expect("run cornercase")
    }

    fn command(&self) -> std::process::Command {
        self.command_of(std::path::Path::new(env!("CARGO_BIN_EXE_cornercase")))
    }

    fn command_of(&self, bin: &std::path::Path) -> std::process::Command {
        let mut cmd = std::process::Command::new(bin);
        cmd.env(SOCKET_ENV, self.socket()).env_remove(NESTED_ENV).env_remove(PANE_ENV).env_remove(update::LATEST_ENV);
        cmd
    }

    fn cli(&self, args: &[&str]) -> std::process::Output {
        self.command().args(args).output().expect("run cornercase")
    }

    fn says(&self, args: &[&str]) -> String {
        let out = self.cli(args);
        assert!(out.status.success(), "cornercase {args:?} failed: {out:?}");
        String::from_utf8_lossy(&out.stdout).trim_end().to_string()
    }

    fn report(&self) -> Report {
        serde_json::from_str(&self.says(&["status", "--json"])).expect("a status report")
    }

    fn spawn(&self, args: &[&str]) -> std::process::Child {
        let mut cmd = self.command();
        cmd.args(args).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
        cmd.spawn().expect("run cornercase")
    }

    fn output(child: std::process::Child) -> String {
        let out = child.wait_with_output().expect("wait for cornercase");
        assert!(out.status.success(), "cornercase failed: {out:?}");
        String::from_utf8_lossy(&out.stdout).trim_end().to_string()
    }

    fn wait_for_report(&self, what: &str, cond: impl Fn(Report) -> bool) {
        let deadline = Instant::now() + TIMEOUT;
        while !cond(self.report()) {
            assert!(Instant::now() < deadline, "timed out waiting for: {what}");
            thread::sleep(POLL);
        }
    }

    fn wait_for_working(&self) {
        self.wait_for_report("an agent at work", |report| {
            panes(report).any(|p| p.status.as_deref() == Some("working"))
        });
    }

    fn wait_for_program(&self, program: &str) {
        self.wait_for_report(program, |report| panes(report).any(|p| p.program.as_deref() == Some(program)));
    }
}

fn panes(report: Report) -> impl Iterator<Item = PaneInfo> {
    report.projects.into_iter().flat_map(|p| p.workspaces).flat_map(|w| w.tabs).flat_map(|t| t.panes)
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.run("kill-server");
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

struct Harness {
    session: Arc<Session>,
    cols: u16,
    screen: Arc<Mutex<vt100::Parser>>,
    raw: Arc<Mutex<Vec<u8>>>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn Child + Send + Sync>,
    _master: Box<dyn MasterPty + Send>,
}

impl Harness {
    fn start() -> Self {
        let mut harness = Self::open(Session::new(), ROWS, COLS);
        harness.wait_for("app starts with one terminal", |s| s.contains(&first_entry()));
        harness
    }

    fn attach(&self, rows: u16, cols: u16) -> Self {
        let mut harness = Self::open(Arc::clone(&self.session), rows, cols);
        harness.wait_for("client shows the sidebar", |s| s.contains("projects"));
        harness
    }

    fn open(session: Arc<Session>, rows: u16, cols: u16) -> Self {
        Self::open_with(session, rows, cols, &[])
    }

    fn open_with(session: Arc<Session>, rows: u16, cols: u16, env: &[(&str, &str)]) -> Self {
        Self::open_from(std::path::Path::new(env!("CARGO_BIN_EXE_cornercase")), session, rows, cols, env)
    }

    fn open_from(bin: &std::path::Path, session: Arc<Session>, rows: u16, cols: u16, env: &[(&str, &str)]) -> Self {
        Self::run(bin, &[], session, (rows, cols), env)
    }

    fn run(
        bin: &std::path::Path,
        args: &[&str],
        session: Arc<Session>,
        (rows, cols): (u16, u16),
        env: &[(&str, &str)],
    ) -> Self {
        let pair =
            native_pty_system().openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 }).expect("open pty");
        let mut cmd = CommandBuilder::new(bin);
        cmd.args(args);
        cmd.env("SHELL", "/bin/sh");
        cmd.env("PS1", "$ ");
        cmd.env(SOCKET_ENV, session.socket());
        cmd.env(CLAUDE_DIR_ENV, session.claude_dir());
        cmd.env_remove(NESTED_ENV);
        cmd.env_remove(PANE_ENV);
        for var in ["SHORTCUT_API_TOKEN", "LINEAR_API_KEY", "TMUX", "STY", "ZELLIJ", "TERM_PROGRAM", "LC_TERMINAL"] {
            cmd.env_remove(var);
        }
        for (key, value) in env {
            cmd.env(key, value);
        }
        cmd.cwd(std::env::temp_dir());
        let child = pair.slave.spawn_command(cmd).expect("spawn cornercase");
        drop(pair.slave);

        let screen = Arc::new(Mutex::new(vt100::Parser::new(rows, cols, 0)));
        let raw = Arc::new(Mutex::new(Vec::new()));
        let mut reader = pair.master.try_clone_reader().expect("pty reader");
        let (screen_in, raw_in) = (Arc::clone(&screen), Arc::clone(&raw));
        thread::spawn(move || {
            let mut buf = [0u8; 8192];
            while let Ok(n @ 1..) = reader.read(&mut buf) {
                screen_in.lock().process(&buf[..n]);
                raw_in.lock().extend_from_slice(&buf[..n]);
            }
        });

        let writer = pair.master.take_writer().expect("pty writer");
        let mut harness = Self { session, cols, screen, raw, writer, child, _master: pair.master };
        harness.wait_for_raw("app asks for the host colors", |raw| raw.contains("\x1b]4;255;?\x1b\\\x1b[>q\x1b[c"));
        harness.send(HOST_THEME_REPLY);
        harness
    }

    fn text(&self) -> String {
        self.screen.lock().screen().contents()
    }

    fn row(&self, y: u16) -> String {
        self.screen.lock().screen().contents_between(y, 0, y, self.cols)
    }

    fn wait_for(&mut self, what: &str, cond: impl Fn(&str) -> bool) {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            let text = self.text();
            if cond(&text) {
                return;
            }
            assert!(Instant::now() < deadline, "timed out waiting for: {what}\n--- screen ---\n{text}");
            thread::sleep(POLL);
        }
    }

    fn is_running(&mut self) -> bool {
        self.child.try_wait().expect("poll child").is_none()
    }

    fn wait_exit(&mut self, what: &str) {
        let deadline = Instant::now() + TIMEOUT;
        while self.is_running() {
            assert!(Instant::now() < deadline, "timed out waiting for: {what}\n--- screen ---\n{}", self.text());
            thread::sleep(POLL);
        }
    }

    fn wait_for_raw(&mut self, what: &str, cond: impl Fn(&str) -> bool) {
        let deadline = Instant::now() + TIMEOUT;
        while !cond(&String::from_utf8_lossy(&self.raw.lock())) {
            assert!(Instant::now() < deadline, "timed out waiting for: {what}");
            thread::sleep(POLL);
        }
    }

    fn send(&mut self, bytes: &[u8]) {
        self.writer.write_all(bytes).expect("write to pty");
        self.writer.flush().expect("flush pty");
    }

    fn stop(&mut self) {
        let pid = self.child.process_id().expect("client pid").to_string();
        let status = std::process::Command::new("kill").args(["-STOP", &pid]).status().expect("run kill");
        assert!(status.success(), "kill -STOP failed");
    }

    fn click(&mut self, pos: Position) {
        let (x, y) = (pos.x + 1, pos.y + 1);
        self.send(format!("\x1b[<0;{x};{y}M\x1b[<0;{x};{y}m").as_bytes());
    }

    fn right_click(&mut self, pos: Position) {
        let (x, y) = (pos.x + 1, pos.y + 1);
        self.send(format!("\x1b[<2;{x};{y}M\x1b[<2;{x};{y}m").as_bytes());
    }

    fn pick(&mut self, at: Position, items: &[&str], i: usize) {
        self.click(ui::menu_item(ui::menu_area(AREA, at, items), i).as_position());
    }

    fn open_new_menu(&mut self, entries: usize, item: usize) {
        let at = ui::new_project_button(list(), 1, &plain(entries)).as_position();
        self.click(at);
        self.wait_for("the new menu opens", |s| s.contains("open project") && s.contains("new group"));
        self.pick(at, &NEW_MENU, item);
    }

    fn open_picker(&mut self, entries: usize) {
        self.open_new_menu(entries, 0);
        self.wait_for("the folder picker opens", |s| s.contains("open project") && s.contains("cancel"));
    }

    fn open_project(&mut self, entries: usize, dir: &std::path::Path) {
        self.open_picker(entries);
        self.send(format!("{}/\r", dir.display()).as_bytes());
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn columns() -> u16 {
    areas().pane.x
}

fn entry(name: &str) -> String {
    format!(" {name} (")
}

fn first_entry() -> String {
    entry(temp().file_name().and_then(|n| n.to_str()).expect("temp dir name"))
}

fn temp() -> PathBuf {
    std::env::temp_dir().canonicalize().expect("canonicalize temp dir")
}

fn workspaces_list() -> Rect {
    areas().workspaces_list
}

fn tab_lines(counts: &[usize]) -> Vec<Vec<u16>> {
    counts.iter().map(|&n| vec![ui::Details::default().lines(); n]).collect()
}

fn workspace_row(tabs: &[usize], row: WorkspaceRow) -> Position {
    let r = ui::workspace_row(workspaces_list(), 1, &tab_lines(tabs), 0, row);
    Position::new(r.x + 3, r.y)
}

fn areas() -> ui::Areas {
    ui::layout_with(AREA, ui::Widths::default(), false, ui::Sidebar::default())
}

const NEW_MENU: [&str; 2] = ["open project", "new group"];

fn plain(projects: usize) -> Vec<SidebarRow> {
    ui::sidebar_rows(&vec![None; projects], &[])
}

fn list() -> Rect {
    areas().list
}

fn pane() -> Rect {
    areas().pane
}

fn git(dir: &std::path::Path, args: &[&str]) {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=test", "-c", "user.email=test@example.com"])
        .args(args)
        .output()
        .expect("run git");
    assert!(out.status.success(), "git {args:?} failed: {}", String::from_utf8_lossy(&out.stderr));
}

fn write_executable(path: &std::path::Path, contents: &str) {
    let mut child = std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg("cat > \"$1\" && chmod 755 \"$1\"")
        .arg("sh")
        .arg(path)
        .stdin(std::process::Stdio::piped())
        .spawn()
        .expect("run sh");
    child.stdin.take().expect("stdin").write_all(contents.as_bytes()).expect("write script");
    assert!(child.wait().expect("wait for sh").success(), "failed to write {}", path.display());
}

fn serve(files: impl FnOnce(&str) -> Vec<(String, Vec<u8>)>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind a port");
    let url = format!("http://{}", listener.local_addr().expect("its address"));
    let files = files(&url);
    thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let mut request = Vec::new();
            let mut buf = [0u8; 1024];
            while !request.ends_with(b"\r\n\r\n") {
                let Ok(n @ 1..) = stream.read(&mut buf) else { break };
                request.extend_from_slice(&buf[..n]);
            }
            let line = String::from_utf8_lossy(&request).into_owned();
            let path = line.split_whitespace().nth(1).unwrap_or_default();
            let found = files.iter().find(|(p, _)| p == path).map(|(_, body)| body.as_slice());
            let (status, body) = found.map_or(("404 Not Found", [].as_slice()), |body| ("200 OK", body));
            let head = format!("HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
            let _ = stream.write_all(head.as_bytes()).and_then(|()| stream.write_all(body));
        }
    });
    url
}

const SSH_PID: &str = "ssh.pid";

fn fake_ssh(session: &Session) -> String {
    let bin = session.dir.join("bin");
    std::fs::create_dir_all(&bin).expect("create the fake ssh's folder");
    let pid = session.dir.join(SSH_PID);
    write_executable(
        &bin.join("ssh"),
        &format!(
            "#!/bin/sh\necho $$ > '{}'\nwhile [ $# -gt 0 ]; do\n  case \"$1\" in\n    -o) shift 2 ;;\n    --) shift; break ;;\n    \
             -*) shift ;;\n    *) break ;;\n  esac\ndone\nshift\nexec /bin/sh -c \"exec $*\"\n",
            pid.display()
        ),
    );
    format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default())
}

fn ssh_pid(session: &Session) -> String {
    std::fs::read_to_string(session.dir.join(SSH_PID)).unwrap_or_default().trim().to_string()
}

fn remote_refusal(command: &str) -> std::process::Output {
    let session = Session::new();
    let path = fake_ssh(&session);
    session
        .command()
        .args(["remote", "devbox", "--command", command])
        .env("PATH", path)
        .output()
        .expect("run cornercase")
}

fn fake_release(version: &str, script: &str) -> String {
    let target = update::target().expect("a released platform");
    let name = format!("cornercase-{target}");
    let build = temp_dir(&format!("release-{version}"));
    std::fs::create_dir_all(build.join(&name)).expect("archive folder");
    write_executable(&build.join(&name).join("cornercase"), script);
    let tar = std::process::Command::new("tar")
        .current_dir(&build)
        .args(["-czf", "release.tar.gz", &name])
        .status()
        .expect("run tar");
    assert!(tar.success(), "tar failed");
    let archive = std::fs::read(build.join("release.tar.gz")).expect("read the archive");
    let _ = std::fs::remove_dir_all(&build);
    let digest = Sha256::digest(&archive).iter().fold(String::new(), |mut hex, b| {
        let _ = write!(hex, "{b:02x}");
        hex
    });
    let url = serve(|url| {
        let latest = serde_json::json!({"tag_name": format!("v{version}"), "assets": [
            {"name": format!("{name}.tar.gz"), "browser_download_url": format!("{url}/download")},
            {"name": format!("{name}.tar.gz.sha256"), "browser_download_url": format!("{url}/download.sha256")},
        ]});
        vec![
            ("/latest".to_string(), latest.to_string().into_bytes()),
            ("/download".to_string(), archive),
            ("/download.sha256".to_string(), format!("{digest} *{name}.tar.gz\n").into_bytes()),
        ]
    });
    format!("{url}/latest")
}

fn installed_copy(name: &str) -> PathBuf {
    let bin = temp_dir(name).join("cornercase");
    std::fs::copy(env!("CARGO_BIN_EXE_cornercase"), &bin).expect("copy cornercase");
    bin
}

fn temp_dir_named(name: &str) -> PathBuf {
    let dir = temp().join(name);
    std::fs::create_dir_all(&dir).expect("create dir");
    dir
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("cornercase-e2e-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir.canonicalize().expect("canonicalize temp dir")
}

#[test]
fn shell_output_shows_in_the_pane() {
    let mut app = Harness::start();

    app.send(b"echo hello-$((20+22))\r");

    app.wait_for("output in the pane", |s| s.contains("hello-42"));
}

#[test]
fn the_server_log_tells_what_happened_but_never_what_was_typed() {
    let session = Session::new();
    let mut app = Harness::open_with(Arc::clone(&session), ROWS, COLS, &[("CORNERCASE_LOG", "debug")]);
    app.wait_for("app starts with one terminal", |s| s.contains(&first_entry()));

    app.send(b"echo typed-\"\"marker-$((40+1))\r");
    app.wait_for("the typed line ran", |s| s.contains("typed-marker-41"));
    app.send(b"\x1b[200~echo pasted-\"\"marker-$((40+2))\x1b[201~\r");
    app.wait_for("the pasted line ran", |s| s.contains("pasted-marker-42"));
    session.says(&["send", "--enter", "echo sent-\"\"marker-$((40+3))"]);
    app.wait_for("the sent line ran", |s| s.contains("sent-marker-43"));
    session.wait_for_file("server.log", "the log has the answer to send", |log| log.contains("command=send"));

    let log = session.says(&["logs", "-n", "1000"]);
    for told in ["server: started", "server: client attached", "app: pane opened", "control: send", "server: input"] {
        assert!(log.contains(told), "the log lacks {told:?}:\n{log}");
    }
    for secret in ["typed-", "pasted-", "sent-", "marker"] {
        assert!(!log.contains(secret), "the log holds {secret:?}:\n{log}");
    }
}

#[test]
fn sidebar_shows_the_search_and_the_title() {
    let app = Harness::start();
    let areas = areas();

    let search = app.row(areas.search.y);
    assert!(search.contains("search projects"), "search row: {search:?}");
    let title = app.row(areas.title.y);
    assert!(title.starts_with(" projects"), "title row: {title:?}");
}

#[test]
fn the_project_name_stays_after_cd() {
    let mut app = Harness::start();
    let dir = temp_dir("cd");

    app.send(format!("cd {}; echo cd-\"\"done\r", dir.display()).as_bytes());

    app.wait_for("the shell moved", |s| s.contains("cd-done"));
    assert!(app.row(list().y).contains(&first_entry()), "entry row: {:?}", app.row(list().y));
}

#[test]
fn a_tab_running_claude_shows_what_it_is_doing() {
    let mut app = Harness::start();
    let sessions = app.session.claude_dir().join("sessions");
    std::fs::create_dir_all(&sessions).expect("create the sessions folder");
    let bin = temp_dir("claude");
    let claude = bin.join("claude");
    write_executable(
        &claude,
        "#!/bin/sh\nprintf '{\"pid\":%s,\"status\":\"waiting\"}' $$ > \"$1/$$.json\"\nread answer\nrm -f \"$1/$$.json\"\n",
    );
    let tab = usize::from(ui::workspace_row(workspaces_list(), 1, &tab_lines(&[1]), 0, WorkspaceRow::Tab(0, 0)).y);
    let tab_row = |s: &str| s.lines().nth(tab).unwrap_or_default().to_string();

    app.send(format!("{} {}\r", claude.display(), sessions.display()).as_bytes());
    app.wait_for("the tab says claude needs you", |s| tab_row(s).contains("▌ ├ ! "));
    app.send(b"\r");

    app.wait_for("the icon goes once claude exits", |s| !tab_row(s).contains('!'));
    let _ = std::fs::remove_dir_all(&bin);
}

#[test]
fn claude_asking_in_a_hidden_tab_reaches_the_desktop_through_the_outer_terminal() {
    let mut app = Harness::start();
    let sessions = app.session.claude_dir().join("sessions");
    std::fs::create_dir_all(&sessions).expect("create the sessions folder");
    let bin = temp_dir("claude-asks");
    let claude = bin.join("claude");
    write_executable(
        &claude,
        "#!/bin/sh\nprintf '{\"pid\":%s,\"status\":\"busy\"}' $$ > \"$1/$$.json\"\n\
         while [ ! -e \"$1/ask\" ]; do sleep 0.05; done\n\
         printf '{\"pid\":%s,\"status\":\"waiting\"}' $$ > \"$1/$$.json\"\nread answer\n",
    );
    app.send(format!("{} {}\r", claude.display(), sessions.display()).as_bytes());
    let tab = usize::from(ui::workspace_row(workspaces_list(), 1, &tab_lines(&[1]), 0, WorkspaceRow::Tab(0, 0)).y);
    app.wait_for("the tab says claude works", |s| s.lines().nth(tab).unwrap_or_default().contains("◐"));
    app.click(workspace_row(&[1], WorkspaceRow::NewTab(0)));
    app.wait_for("the second tab is active", |s| s.contains("$ ") && !s.contains(&claude.display().to_string()));

    std::fs::write(sessions.join("ask"), "").expect("ask");

    app.wait_for_raw("ghostty is asked for a notification", |raw| {
        raw.contains("\x1b]777;notify;cornercase;claude needs you in ")
    });
    app.wait_for("a toast says where", |s| s.contains("claude needs you in "));
    let _ = std::fs::remove_dir_all(&bin);
}

#[test]
fn new_project_opens_the_typed_folder() {
    let mut app = Harness::start();
    let name = format!("ccnw-{}", std::process::id());
    let dir = temp_dir_named(&name);

    app.open_project(1, &dir);

    app.wait_for("the project opens", |s| s.contains(&entry(&name)) && !s.contains("cancel"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_folder_picker_works_with_the_mouse() {
    let mut app = Harness::start();
    let name = format!("ccpk-{}", std::process::id());
    let dir = temp_dir_named(&name);
    for folder in ["alpha", "beta"] {
        std::fs::create_dir_all(dir.join(folder)).expect("create folder");
    }
    app.open_project(1, &dir.join("alpha"));
    app.wait_for("the first project opens", |s| s.contains(&entry("alpha")));
    app.open_picker(2);
    let picker = ui::picker_area(AREA);
    app.wait_for("the picker starts next to it", |s| s.contains(&format!("{name}/")) && s.contains("beta"));

    app.click(ui::picker_item(picker, 3, 0, 2).as_position());
    app.wait_for("the picker goes into it", |s| s.contains(&format!("{name}/beta/")));
    app.click(ui::picker_buttons(picker, "open")[0].as_position());

    app.wait_for("the second project opens", |s| s.contains(&entry("beta")));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn close_button_removes_a_project() {
    let mut app = Harness::start();
    let name = format!("cccl-{}", std::process::id());
    let dir = temp_dir_named(&name);
    app.open_project(1, &dir);
    app.wait_for("project 2 appears", |s| s.contains(&entry(&name)));

    app.click(ui::close_button(list(), 1, &plain(2), 0, SidebarRow::Project(0)).as_position());
    app.wait_for("it asks first", |s| s.contains("Close the project"));
    app.click(ui::form_buttons(ui::form_area(AREA), "close")[0].as_position());

    app.wait_for("only the second one is left", |s| s.contains(&entry(&name)) && !s.contains(&first_entry()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn opening_an_open_folder_switches_to_it() {
    let mut app = Harness::start();
    let name = format!("ccsw-{}", std::process::id());
    let dir = temp_dir_named(&name);
    app.open_project(1, &dir);
    app.wait_for("project 2 appears", |s| s.contains(&entry(&name)));

    app.open_project(2, &temp());

    app.wait_for("project 1 is active again", |s| s.contains(&format!("▌{}", first_entry())) && !s.contains("cancel"));
    let third = app.row(ui::new_project_button(list(), 1, &plain(2)).y);
    assert!(third.contains(ui::NEW_BUTTON), "a third entry opened: {third:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn clicking_an_entry_switches_project() {
    let mut app = Harness::start();
    let name = format!("ccen-{}", std::process::id());
    let dir = temp_dir_named(&name);
    app.send(b"echo in-the-first\r");
    app.wait_for("output in project 1", |s| s.contains("in-the-first"));
    app.open_project(1, &dir);
    app.wait_for("project 2 active and empty", |s| s.contains(&entry(&name)) && !s.contains("in-the-first"));

    app.click(Position::new(list().x + 2, list().y));

    app.wait_for("back to project 1", |s| s.contains("in-the-first"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_search_switches_project() {
    let mut app = Harness::start();
    let name = format!("ccse-{}", std::process::id());
    let dir = temp_dir_named(&name);
    app.send(b"echo in-the-first\r");
    app.wait_for("output in project 1", |s| s.contains("in-the-first"));
    app.open_project(1, &dir);
    app.wait_for("project 2 active and empty", |s| s.contains(&entry(&name)) && !s.contains("in-the-first"));

    app.click(areas().search.as_position());
    app.send(temp().file_name().and_then(|n| n.to_str()).expect("temp dir name").as_bytes());
    app.wait_for("the project is found", |s| s.contains("enter goes to"));
    app.send(b"\r");

    app.wait_for("back to project 1", |s| s.contains("in-the-first") && !s.contains("enter goes to"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_narrow_terminal_gets_a_menu_bar() {
    let narrow = ui::COMPACT_WIDTH - 10;
    let areas = ui::layout(Rect::new(0, 0, narrow, ROWS), ui::Widths::default());
    let mut app = Harness::open(Session::new(), ROWS, narrow);
    app.wait_for("the bar names the project", |s| s.lines().nth(1).is_some_and(|l| l.contains("≡")));

    app.send(b"stty size\r");
    app.wait_for("the shell gets the whole width", |s| s.contains(&format!("{} {narrow}", ROWS - ui::COMPACT_PITCH)));

    app.click(areas.bar.as_position());
    app.wait_for("the menu lists the workspaces", |s| s.contains("‹ projects") && s.contains("+ new workspace"));
    app.click(areas.back.as_position());
    app.wait_for("back lists the projects", |s| {
        s.contains(ui::NEW_BUTTON) && !s.contains("+ new workspace") && s.contains("quit")
    });
}

#[test]
fn ctrl_b_reaches_the_shell() {
    let mut app = Harness::start();
    app.send(b"echo cat-\"\"starts; cat -v\r");
    app.wait_for("cat runs", |s| s.contains("cat-starts"));

    app.send(b"\x02c\r");

    app.wait_for("cat echoes ctrl+b", |s| s.matches("^Bc").count() >= 2);
}

#[test]
fn ctrl_enter_reaches_a_program_that_asked_for_kitty_keys() {
    let mut app = Harness::start();
    app.wait_for_raw("the app asks the terminal for kitty keys", |raw| raw.contains("\x1b[>1u"));
    app.send(b"printf '\\033[>1u'; echo cat-\"\"starts; cat -v\r");
    app.wait_for("cat runs", |s| s.contains("cat-starts"));

    app.send(b"\x1b[13;5u\r");

    app.wait_for("cat echoes ctrl+enter", |s| s.matches("^[[13;5u").count() >= 2);
}

#[test]
fn quit_button_detaches_and_programs_keep_running() {
    let mut app = Harness::start();
    app.send(b"echo cat-\"\"starts; cat -v\r");
    app.wait_for("cat runs", |s| s.contains("cat-starts"));

    app.click(areas().quit.as_position());
    app.wait_exit("client exits");
    let mut again = app.attach(ROWS, COLS);
    again.send(b"\x02\r");

    again.wait_for("cat still echoes", |s| s.contains("cat-starts") && s.contains("^B"));
}

#[test]
fn exiting_the_last_shell_keeps_the_project() {
    let mut app = Harness::start();

    app.send(b"exit\r");
    app.wait_for("the workspace has no tab", |s| s.contains("no tab open") && s.contains(&first_entry()));
    app.click(workspace_row(&[0], WorkspaceRow::NewTab(0)));

    app.wait_for("a new tab opens", |s| !s.contains("no tab open"));
    app.send(b"echo in-the-\"\"new-tab\r");
    app.wait_for("the new tab runs a shell", |s| s.contains("in-the-new-tab"));
    assert!(app.is_running(), "client exited");
}

#[test]
fn tabs_keep_their_own_shells() {
    let mut app = Harness::start();
    app.send(b"echo in-the-first\r");
    app.wait_for("output in the first tab", |s| s.contains("in-the-first"));

    app.click(workspace_row(&[1], WorkspaceRow::NewTab(0)));
    app.wait_for("the second tab is active and empty", |s| !s.contains("in-the-first") && s.contains("$ "));
    app.click(workspace_row(&[2], WorkspaceRow::Tab(0, 0)));

    app.wait_for("back to the first tab", |s| s.contains("in-the-first"));
}

#[test]
fn clients_mirror_each_other() {
    let mut first = Harness::start();
    let mut second = first.attach(ROWS, COLS);

    second.send(b"echo from-\"\"second\r");

    first.wait_for("first client sees it", |s| s.contains("from-second"));
}

#[test]
fn the_last_used_client_sets_the_size() {
    let mut big = Harness::start();
    let mut small = big.attach(ROWS - 6, COLS - 8);

    small.send(b"stty size\r");
    small.wait_for("pane fits the small client", |s| s.contains(&format!("{} {}", ROWS - 6, COLS - 8 - columns())));
    big.send(b"stty size\r");

    big.wait_for("pane fits the big client", |s| s.contains(&format!("{ROWS} {}", COLS - columns())));
}

#[test]
fn a_hung_small_client_does_not_shrink_a_new_big_one() {
    let first = Harness::start();
    let mut small = first.attach(ROWS - 6, COLS - 8);
    small.send(b"stty size\r");
    small.wait_for("pane fits the small client", |s| s.contains(&format!("{} {}", ROWS - 6, COLS - 8 - columns())));
    small.stop();
    drop(first);

    let mut big = Harness::open(Arc::clone(&small.session), ROWS, COLS);
    big.wait_for("client shows the sidebar", |s| s.contains("projects"));
    big.send(b"stty size\r");

    big.wait_for("pane fits the new big client", |s| s.contains(&format!("{ROWS} {}", COLS - columns())));
}

#[test]
fn a_smaller_client_sees_the_frame_cut_off() {
    let mut big = Harness::start();
    let small = big.attach(ROWS - 6, COLS - 8);
    let line = format!("{}>", "=".repeat(usize::from(COLS - columns() - 1)));

    big.send(format!("clear; printf '%s\\n' '{line}'\r").as_bytes());
    big.wait_for("the line fills the big pane", |s| s.lines().any(|l| l.ends_with(&line)));

    let cut = |h: &Harness| h.screen.lock().screen().rows(0, COLS - 8).take(usize::from(ROWS - 6)).collect::<Vec<_>>();
    let deadline = Instant::now() + TIMEOUT;
    while cut(&small) != cut(&big) {
        assert!(Instant::now() < deadline, "the small client does not show the cut frame\n{}", small.text());
        thread::sleep(POLL);
    }
}

#[test]
fn a_todo_typed_in_the_panel_is_saved_next_to_the_session() {
    let mut app = Harness::start();
    app.click(areas().todo_button.as_position());
    app.wait_for("the todo panel opens", |s| s.contains("+ new todo"));
    let panel = ui::layout_with(AREA, ui::Widths::default(), true, ui::Sidebar::default()).changes;
    let view = ui::todo::View {
        items: Vec::new(),
        scroll: 0,
        adding: None,
        light: false,
        muted: ratatui::style::Color::DarkGray,
        drag: None,
    };
    let button = ui::todo::rows(panel, &view).button();

    app.click(button.as_position());
    app.send(b"buy milk\r");

    app.wait_for("the item shows", |s| s.contains("○  buy milk"));
    app.session.wait_for_file("todos.json", "the item is saved", |saved| saved.contains("buy milk"));
}

#[test]
fn dragging_a_border_resizes_the_shell_and_is_saved() {
    let mut app = Harness::start();
    let border = areas().projects_border;
    let (from, to, y) = (border.x + 1, border.x + 11, border.y + 2);

    app.send(format!("\x1b[<0;{from};{y}M\x1b[<32;{to};{y}M\x1b[<0;{to};{y}m").as_bytes());
    app.send(b"stty size\r");

    let pane = areas().pane.width - 10;
    app.wait_for("the shell sees the narrower pane", |s| s.contains(&format!("{ROWS} {pane}")));
    let widths = format!("\"projects\": {}", ui::SIDEBAR_WIDTH + 10);
    app.session.wait_for_saved("the widths are saved", |saved| saved.contains(&widths));
}

#[test]
fn dragging_over_text_copies_it_and_says_so() {
    let mut app = Harness::start();
    app.send(b"clear; echo copy-\"\"me\r");
    app.wait_for("the text shows", |s| s.lines().any(|l| l.trim_end().ends_with("copy-me")));
    let y = (0..ROWS).find(|&y| app.row(y).trim_end().ends_with("copy-me")).expect("the row of the text") + 1;
    let (from, to) = (pane().x + 1, pane().x + 7);

    app.send(format!("\x1b[<0;{from};{y}M\x1b[<32;{to};{y}M\x1b[<0;{to};{y}m").as_bytes());

    let osc52 = String::from_utf8(cornercase::clipboard::osc52("copy-me")).expect("utf-8");
    app.wait_for_raw("the text reaches the outer terminal's clipboard", |raw| raw.contains(&osc52));
    app.wait_for("a toast says it was copied", |s| s.contains("copied to clipboard"));
}

#[test]
fn a_program_inside_can_copy_to_the_clipboard() {
    let mut app = Harness::start();

    app.send(b"printf '\\033]52;c;Y29weS1tZQ==\\007'\r");

    let osc52 = String::from_utf8(cornercase::clipboard::osc52("copy-me")).expect("utf-8");
    app.wait_for_raw("the copy reaches the outer terminal's clipboard", |raw| raw.contains(&osc52));
    app.wait_for("a toast says it was copied", |s| s.contains("copied to clipboard"));
}

#[test]
fn quitting_one_client_leaves_the_others_attached() {
    let mut first = Harness::start();
    let mut second = first.attach(ROWS, COLS);

    second.click(areas().quit.as_position());
    second.wait_exit("second client exits");
    first.send(b"echo still-\"\"here\r");

    first.wait_for("first client keeps working", |s| s.contains("still-here"));
}

#[test]
fn kill_server_says_what_it_stopped() {
    let mut app = Harness::start();
    app.send(b"sleep 600\r");
    app.session.wait_for_program("sleep");

    let out = app.session.run("kill-server");

    assert!(String::from_utf8_lossy(&out.stdout).contains("stopped 1 program"), "{out:?}");
}

#[test]
fn kill_server_closes_every_client() {
    let mut first = Harness::start();
    let mut second = first.attach(ROWS, COLS);

    let out = first.session.run("kill-server");

    assert!(out.status.success(), "kill-server failed: {out:?}");
    first.wait_exit("first client exits");
    second.wait_exit("second client exits");
}

#[test]
fn clients_starting_at_once_share_one_server() {
    let session = Session::new();
    let (mut first, mut second) = thread::scope(|s| {
        let first = s.spawn(|| Harness::open(Arc::clone(&session), ROWS, COLS));
        let second = Harness::open(Arc::clone(&session), ROWS, COLS);
        (first.join().expect("first client"), second)
    });
    first.wait_for("first client has a terminal", |s| s.contains(&first_entry()));
    second.wait_for("second client has a terminal", |s| s.contains(&first_entry()));

    second.send(b"echo from-\"\"second\r");

    first.wait_for("first client sees it", |s| s.contains("from-second"));
    let entries = first.text().matches(&first_entry()).count();
    assert_eq!(entries, 1, "two servers opened a terminal each:\n{}", first.text());
    let deadline = Instant::now() + TIMEOUT;
    while session.servers() != 1 {
        assert!(Instant::now() < deadline, "{} servers are running for one socket", session.servers());
        thread::sleep(POLL);
    }
}

#[test]
fn a_new_server_reopens_the_saved_projects() {
    let mut app = Harness::start();
    let name = format!("ccrs-{}", std::process::id());
    let dir = temp_dir_named(&name);
    app.open_project(1, &dir);
    app.wait_for("project 2 appears", |s| s.contains(&entry(&name)));
    app.session.wait_for_saved("both projects are saved", |saved| saved.contains(name.as_str()));

    let out = app.session.run("kill-server");
    assert!(out.status.success(), "kill-server failed: {out:?}");
    app.wait_exit("client exits with the server");
    let mut again = app.attach(ROWS, COLS);

    again.wait_for("both projects come back", |s| s.contains(&first_entry()) && s.contains(&entry(&name)));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn dragging_a_project_reorders_the_sidebar_and_is_saved() {
    let mut app = Harness::start();
    let name = format!("ccdr-{}", std::process::id());
    let dir = temp_dir_named(&name);
    app.open_project(1, &dir);
    app.wait_for("project 2 appears", |s| s.contains(&entry(&name)));
    let (x, first, second) = (list().x + 4, list().y + 1, list().y + 2);

    app.send(format!("\x1b[<0;{x};{second}M\x1b[<32;{x};{first}M\x1b[<0;{x};{first}m").as_bytes());

    let above = |text: &str, a: &str, b: &str| matches!((text.find(a), text.find(b)), (Some(i), Some(j)) if i < j);
    app.wait_for("the new project is first", |s| above(s, &entry(&name), &first_entry()));
    let path = |dir: &std::path::Path| format!("\"path\": \"{}\"", dir.display());
    app.session.wait_for_saved("the order is saved", |saved| above(saved, &path(&dir), &path(&temp())));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn kill_server_says_when_none_is_running() {
    let session = Session::new();

    let out = session.run("kill-server");

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success() && stderr.contains("no cornercase server is running"), "{out:?}");
}

#[test]
fn restart_says_what_stops_and_brings_the_client_back_with_new_shells() {
    let mut app = Harness::start();
    app.send(b"echo old-\"\"shell\r");
    app.wait_for("the old shell answers", |s| s.contains("old-shell"));
    app.send(b"sleep 600\r");
    app.session.wait_for_program("sleep");

    let out = app.session.cli(&["restart", "--yes"]);

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{out:?}");
    assert!(stdout.contains("restarted the cornercase server") && stdout.contains("stopped 1 program"), "{stdout}");
    app.wait_for("the client comes back with a new shell", |s| !s.contains("old-shell") && s.contains(&first_entry()));
}

#[test]
fn restart_without_a_server_says_so() {
    let session = Session::new();

    let out = session.cli(&["restart", "--yes"]);

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success() && stderr.contains("no cornercase server is running"), "{out:?}");
}

#[test]
fn update_installs_the_latest_release_and_restarts_the_server() {
    let bin = installed_copy("update");
    let mut app = Harness::open_from(&bin, Session::new(), ROWS, COLS, &[]);
    app.wait_for("app starts with one terminal", |s| s.contains(&first_entry()));
    let name = format!("ccup-{}", std::process::id());
    let dir = temp_dir_named(&name);
    app.open_project(1, &dir);
    app.wait_for("project 2 appears", |s| s.contains(&entry(&name)));
    app.send(b"echo old-\"\"shell\r");
    app.wait_for("the old shell answers", |s| s.contains("old-shell"));
    app.send(b"sleep 600\r");
    app.session.wait_for_program("sleep");
    let marker = bin.with_file_name("new-client-ran");
    let new = format!(
        "#!/bin/sh\n[ \"$1\" = --version ] && exec echo 'cornercase 99.0.0'\n[ $# -eq 0 ] && touch '{}'\nexec '{}' \"$@\"\n",
        marker.display(),
        env!("CARGO_BIN_EXE_cornercase")
    );

    let out = app
        .session
        .command_of(&bin)
        .args(["update", "--yes"])
        .env(update::LATEST_ENV, fake_release("99.0.0", &new))
        .output()
        .expect("run update");

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{out:?}");
    assert!(stdout.contains(&format!("updated cornercase {} → 99.0.0", update::CURRENT)), "{stdout}");
    assert!(stdout.contains("restarted the cornercase server"), "{stdout}");
    assert!(stdout.contains("1 program (`sleep` in ") && stdout.contains("stopped 1 program"), "{stdout}");
    assert_eq!(std::fs::read_to_string(&bin).expect("the new binary"), new);
    app.wait_for("the client comes back with both projects and new shells", |s| {
        !s.contains("old-shell") && s.contains(&first_entry()) && s.contains(&entry(&name))
    });
    assert!(marker.exists(), "the client came back without running the new binary");
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(bin.parent().expect("its folder"));
}

#[test]
fn update_leaves_an_up_to_date_install_and_its_server_alone() {
    let app = Harness::start();
    let bin = installed_copy("up-to-date");

    let out = app
        .session
        .command_of(&bin)
        .arg("update")
        .env(update::LATEST_ENV, fake_release(update::CURRENT, "#!/bin/sh\n"))
        .output()
        .expect("run update");

    assert!(out.status.success(), "{out:?}");
    assert_eq!(String::from_utf8_lossy(&out.stdout), format!("cornercase {} is up to date\n", update::CURRENT));
    assert_eq!(app.session.servers(), 1);
    let _ = std::fs::remove_dir_all(bin.parent().expect("its folder"));
}

#[test]
fn a_development_build_refuses_to_update() {
    let session = Session::new();

    let out = session.run("update");

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success() && stderr.contains("this is a development build"), "{out:?}");
}

#[test]
fn running_cornercase_inside_itself_is_refused() {
    let mut app = Harness::start();

    let bin = env!("CARGO_BIN_EXE_cornercase");

    app.send(format!("{bin} 2>&1 | grep -q 'nesting it is not supported' && echo refused-\"\"ok\r").as_bytes());
    app.wait_for("nested client explains why", |s| s.contains("refused-ok"));
    app.send(format!("{bin} 2>/dev/null; echo exit-\"\"code-$?\r").as_bytes());

    app.wait_for("nested client fails", |s| s.contains("exit-code-1"));
}

#[test]
fn shells_do_not_inherit_the_claude_code_session_that_started_the_server() {
    let mut env: Vec<(&str, &str)> = CLAUDE_SESSION_ENV.iter().map(|key| (*key, "leaked")).collect();
    env.push(("CORNERCASE_E2E_KEPT", "kept"));
    let mut app = Harness::open_with(Session::new(), ROWS, COLS, &env);
    app.wait_for("app starts with one terminal", |s| s.contains(&first_entry()));

    let vars = CLAUDE_SESSION_ENV.map(|key| format!("${{{key}}}")).concat();
    app.send(format!("echo \"session-[{vars}]-$CORNERCASE_E2E_KEPT\"\r").as_bytes());

    app.wait_for("only the unrelated variable reaches the shell", |s| s.contains("session-[]-kept"));
}

#[test]
fn mouse_is_not_forwarded_when_the_program_did_not_ask() {
    let mut app = Harness::start();
    app.send(b"echo cat-\"\"starts; cat -v\r");
    app.wait_for("cat runs", |s| s.contains("cat-starts"));

    app.click(Position::new(pane().x + 7, pane().y + 4));
    app.send(b"done\r");

    app.wait_for("cat echoes the text", |s| s.matches("done").count() >= 2);
    assert!(!app.text().contains("^[[<"), "mouse leaked into the program:\n{}", app.text());
}

#[test]
fn mouse_is_forwarded_relative_to_the_pane_when_requested() {
    let mut app = Harness::start();
    app.send(b"printf '\\033[?1000h\\033[?1006h'; echo cat-\"\"starts; cat -v\r");
    app.wait_for("cat runs with mouse mode on", |s| s.contains("cat-starts"));

    app.click(Position::new(pane().x + 7, pane().y + 4));

    app.wait_for("program receives the click", |s| s.contains("^[[<0;8;5M^[[<0;8;5m"));
}

#[test]
fn programs_see_the_host_background_color() {
    let mut app = Harness::start();

    app.send(b"stty -icanon -echo; printf '\\033]11;?\\033\\\\'; head -c 23 | tr '\\033' E; stty sane\r");

    app.wait_for("program reads the host background", |s| s.contains("E]11;rgb:1212/5656/9a9a"));
}

#[test]
fn sigterm_restores_the_terminal() {
    let mut app = Harness::start();
    let pid = app.child.process_id().expect("child pid");
    app.raw.lock().clear();

    let status = std::process::Command::new("kill").args(["-TERM", &pid.to_string()]).status().expect("run kill");
    assert!(status.success(), "kill failed: {status}");

    app.wait_exit("app exits on SIGTERM");
    app.wait_for_raw("kitty keys and mouse capture off, alternate screen left", |raw| {
        raw.contains("\x1b[<1u") && raw.contains("\x1b[?1000l") && raw.contains("\x1b[?1049l")
    });
}

#[test]
fn a_workspace_with_its_own_worktree_is_created_and_removed() {
    let session = Session::new();
    let worktrees = session.dir.join("worktrees");
    let config = format!("{{\"worktrees_dir\": \"{}\"}}", worktrees.display());
    std::fs::write(session.dir.join("config.json"), config).expect("write config");
    let repo = std::env::temp_dir().join(format!("ccwt-{}", std::process::id()));
    std::fs::create_dir_all(&repo).expect("create repo dir");
    let repo_name = repo.file_name().and_then(|n| n.to_str()).expect("repo name").to_string();
    git(&repo, &["init", "--quiet"]);
    std::fs::write(repo.join(".gitignore"), ".env\n").expect("write .gitignore");
    std::fs::write(repo.join(".worktreeinclude"), ".env\n").expect("write .worktreeinclude");
    std::fs::write(repo.join(".env"), "TOKEN=1\n").expect("write .env");
    git(&repo, &["add", "--all"]);
    git(&repo, &["commit", "--quiet", "-m", "init"]);
    let mut app = Harness::open(session, ROWS, COLS);
    app.wait_for("app starts with one terminal", |s| s.contains(&first_entry()));
    app.open_project(1, &repo);
    app.wait_for("the repo opens as project 2", |s| s.contains(&entry(&repo_name)));

    app.click(ui::new_workspace_button(workspaces_list(), 1, &tab_lines(&[1])).as_position());
    app.wait_for("the form opens", |s| s.contains("with its own worktree"));
    app.send(b"e2e/login\r");

    app.wait_for("the worktree workspace opens", |s| s.contains("e2e/login") && !s.contains("cancel"));
    let list = workspaces_list();
    let y = (list.y..list.bottom()).find(|&y| app.row(y).contains("e2e/login")).expect("the workspace row");
    let label_row = Rect { y, height: 1, ..list };
    let checkout = worktrees.join(&repo_name).join("e2e-login");
    assert_eq!(std::fs::read_to_string(checkout.join(".env")).ok().as_deref(), Some("TOKEN=1\n"));

    app.click(ui::row_close_button(label_row, 1).as_position());
    app.wait_for("it asks first", |s| s.contains("remove workspace"));
    app.click(ui::form_buttons(ui::form_area(AREA), "remove")[0].as_position());

    app.wait_for("git removes it and says so", |s| s.contains("removed e2e/login") && !s.contains("remove workspace"));
    assert!(!app.row(y).contains("e2e/login") && !checkout.exists(), "the row or the checkout is still there");
    let _ = std::fs::remove_dir_all(&repo);
}

#[test]
fn an_issue_is_read_then_handed_to_its_agent_in_its_own_worktree() {
    let session = Session::new();
    let worktrees = session.dir.join("worktrees");
    let gh = session.dir.join("gh");
    let list = r#"[{"number":7,"title":"Fix the login","state":"OPEN","labels":[],"author":{"login":"ana"},"updatedAt":"2026-09-19T12:00:00Z","url":"https://github.com/acme/shop/issues/7"}]"#;
    let view = r#"{"number":7,"title":"Fix the login","state":"OPEN","labels":[],"assignees":[],"author":{"login":"ana"},"updatedAt":"","url":"u","body":"The form **breaks** after:\n\n- typing\n- waiting","comments":[{"author":{"login":"bo"},"body":"Same here","createdAt":""}]}"#;
    let script =
        format!("#!/bin/sh\nif [ \"$2\" = view ]; then cat <<'EOF'\n{view}\nEOF\nelse cat <<'EOF'\n{list}\nEOF\nfi\n");
    write_executable(&gh, &script);
    let agent = session.dir.join("agent");
    let agent_script = "#!/bin/sh\nprintf 'Do you trust the files in this folder?\\n'\nread answer\n\
        printf '\\033[2J\\033[Hagent ready> '\nread line\nprintf '%s' \"$line\" > got\n";
    write_executable(&agent, agent_script);
    let config = format!(
        r#"{{"worktrees_dir": "{}", "gh": "{}", "agent": "fake", "agent_commands": {{"fake": "{}"}}}}"#,
        worktrees.display(),
        gh.display(),
        agent.display()
    );
    std::fs::write(session.dir.join("config.json"), config).expect("write config");
    let repo = std::env::temp_dir().join(format!("ccis-{}", std::process::id()));
    std::fs::create_dir_all(&repo).expect("create repo dir");
    let repo_name = repo.file_name().and_then(|n| n.to_str()).expect("repo name").to_string();
    git(&repo, &["init", "--quiet"]);
    git(&repo, &["commit", "--quiet", "--allow-empty", "-m", "init"]);
    let mut app = Harness::open(session, ROWS, COLS);
    app.wait_for("app starts with one terminal", |s| s.contains(&first_entry()));
    app.open_project(1, &repo);
    app.wait_for("the repo opens as project 2", |s| s.contains(&entry(&repo_name)));

    app.click(areas().issues.as_position());
    app.wait_for("the issues show", |s| s.contains("#7") && s.contains("Fix the login"));
    app.send(b"\r");
    app.wait_for("the issue shows rendered", |s| {
        s.contains("The form breaks after:") && s.contains("• typing") && s.contains("@bo") && s.contains("Same here")
    });
    let detail_buttons = ["start", "agent…", "raw", "copy url", "back"];
    app.click(ui::issue_buttons(ui::issues_area(AREA), &detail_buttons)[3].as_position());
    let osc52 =
        String::from_utf8(cornercase::clipboard::osc52("https://github.com/acme/shop/issues/7")).expect("utf-8");
    app.wait_for_raw("the URL reaches the outer terminal's clipboard", |raw| raw.contains(&osc52));
    app.send(b"\r");

    app.wait_for("the agent asks whether to trust the folder", |s| {
        s.contains("#7 Fix the login") && s.contains("Do you trust the files") && !s.contains("cancel")
    });
    app.send(b"\r");
    app.wait_for("the agent starts in the issue workspace with the prompt typed", |s| {
        s.contains("#7 Fix the login") && s.contains("agent ready> https://") && !s.contains("cancel")
    });
    assert!(worktrees.join(&repo_name).join("issue-7-fix-the-login").is_dir(), "no checkout");
    let got = worktrees.join(&repo_name).join("issue-7-fix-the-login").join("got");
    assert!(!got.exists(), "the prompt was sent without Enter");
    app.send(b"\r");
    let deadline = Instant::now() + TIMEOUT;
    while std::fs::read_to_string(&got).ok().as_deref() != Some("https://github.com/acme/shop/issues/7") {
        assert!(Instant::now() < deadline, "the agent never got the prompt");
        thread::sleep(POLL);
    }
    let _ = std::fs::remove_dir_all(&repo);
}

#[test]
fn right_click_renames_a_project() {
    let mut app = Harness::start();
    let at = Position::new(list().x + 3, list().y);

    app.right_click(at);
    app.wait_for("the menu opens", |s| s.contains("rename project"));
    app.pick(at, &["rename project"], 0);
    app.wait_for("the form opens", |s| s.contains("leave it empty"));
    app.send(&[0x7f; 64]);
    app.send(b"my-api\r");

    app.wait_for("the entry shows the new name", |s| s.contains(&entry("my-api")) && !s.contains("leave it empty"));
    app.send(b"cd /\r");
    app.send(b"echo cd-\"\"done\r");
    app.wait_for("the shell moved", |s| s.contains("cd-done"));
    assert!(app.text().contains(&entry("my-api")), "the name changed after cd:\n{}", app.text());
}

#[test]
fn a_project_moves_into_a_new_group() {
    let mut app = Harness::start();
    let at = Position::new(list().x + 3, list().y);

    app.open_new_menu(1, 1);
    app.wait_for("the group form opens", |s| s.contains("new group") && s.contains("right-click a project"));
    app.send(b"work\r");
    app.wait_for("the group shows", |s| s.contains(&format!("  ▾ {} work", ui::GROUP_STYLES[0].0)));
    app.right_click(at);
    app.wait_for("the menu opens", |s| s.contains("move to group"));
    app.pick(at, &["rename project", "move to group"], 1);
    let group = format!("{} work", ui::GROUP_STYLES[0].0);
    app.wait_for("the groups are listed", |s| s.contains(&format!(" {group} ")) && !s.contains("move to group"));
    app.click(ui::menu_item(ui::menu_area(AREA, at, &[group.as_str()]), 0).as_position());

    app.wait_for("the project is inside", |s| s.contains(&format!("▌  {}", first_entry())));
    app.session
        .wait_for_saved("the group is saved", |saved| saved.contains("\"groups\"") && saved.contains("\"group\": 0"));
}

#[test]
fn a_pane_splits_from_its_menu_and_closes_on_exit() {
    let mut app = Harness::start();
    let at = Position::new(pane().x + 2, pane().y + 2);
    let items = ["split right", "split down", "send right-clicks to the pane", "close pane"];
    let mut layout = split::Node::Leaf(0);
    layout.split(0, Dir::Right, 1);
    let (right, divider) = (layout.panes(pane())[1].1, layout.dividers(pane())[0].line);

    app.right_click(at);
    app.wait_for("the pane menu opens", |s| s.contains("split right"));
    app.pick(at, &items, 0);
    app.wait_for("the divider shows", |s| screen_column(s, divider.x).iter().all(|c| *c == '│'));
    app.send(b"stty size\r");

    app.wait_for("the new shell has its half", |s| s.contains(&format!("{} {}", right.height, right.width)));
    app.session.wait_for_saved("the layout is saved", |saved| saved.contains("\"right\""));
    app.send(b"exit\r");
    app.wait_for("the divider goes", |s| screen_column(s, divider.x).iter().all(|c| *c != '│'));
    app.send(b"stty size\r");
    app.wait_for("the first shell has the whole pane again", |s| {
        s.contains(&format!("{} {}", pane().height, pane().width))
    });
}

fn screen_column(screen: &str, x: u16) -> Vec<char> {
    screen.lines().map(|l| l.chars().nth(usize::from(x)).unwrap_or(' ')).collect()
}

#[test]
fn the_changes_panel_shows_what_changed_in_the_repo() {
    let repo = std::env::temp_dir().join(format!("ccch-{}", std::process::id()));
    std::fs::create_dir_all(&repo).expect("create repo dir");
    let repo_name = repo.file_name().and_then(|n| n.to_str()).expect("repo name").to_string();
    git(&repo, &["init", "--quiet"]);
    std::fs::write(repo.join("notes.txt"), "one\ntwo\n").expect("write file");
    git(&repo, &["add", "notes.txt"]);
    git(&repo, &["commit", "--quiet", "-m", "init"]);
    std::fs::write(repo.join("notes.txt"), "one\nTWO\n").expect("edit file");
    let mut app = Harness::start();
    app.open_project(1, &repo);
    app.wait_for("the repo opens as project 2", |s| s.contains(&entry(&repo_name)));
    app.wait_for("the button counts the changed file", |s| s.contains("changes 1"));

    app.click(ui::changes_button(areas().issues, "changes 1").as_position());

    app.wait_for("the panel shows the diff", |s| {
        s.contains("uncommitted") && s.contains("notes.txt") && s.contains("TWO")
    });
    let _ = std::fs::remove_dir_all(&repo);
}

#[test]
fn a_socket_folder_other_users_can_write_is_refused() {
    let session = Session::new();
    std::fs::set_permissions(&session.dir, std::fs::Permissions::from_mode(0o777)).expect("open the folder to all");

    let outputs = [Some("server"), Some("kill-server"), None].map(|arg| {
        let mut cmd = session.command();
        cmd.args(arg).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::piped());
        finish_within(cmd.spawn().expect("run cornercase"), TIMEOUT)
    });

    let refused = outputs.iter().map(|out| refused_with(out, KEEP_THE_FOLDER));
    assert_eq!(refused.collect::<Vec<_>>(), [true, true, true]);
}

#[test]
fn a_bare_socket_name_is_checked_in_the_folder_it_runs_in() {
    let session = Session::new();
    std::fs::set_permissions(&session.dir, std::fs::Permissions::from_mode(0o777)).expect("open the folder to all");
    let mut cmd = session.command();
    cmd.arg("kill-server").env(SOCKET_ENV, "server.sock").current_dir(&session.dir);

    let out = cmd.output().expect("run cornercase");

    assert!(refused_with(&out, KEEP_THE_FOLDER), "{out:?}");
}

#[test]
fn cornercases_own_socket_folder_may_be_removed() {
    let session = Session::new();
    let own = session.dir.join(format!("cornercase-{}", cornercase::protocol::own_uid()));
    std::fs::create_dir(&own).expect("create the folder");
    std::fs::set_permissions(&own, std::fs::Permissions::from_mode(0o777)).expect("open the folder to all");
    let mut cmd = session.command();
    cmd.arg("kill-server").env_remove(SOCKET_ENV).env_remove("XDG_RUNTIME_DIR").env("TMPDIR", &session.dir);

    let out = cmd.output().expect("run cornercase");

    assert!(refused_with(&out, REMOVE_THE_FOLDER), "{out:?}");
}

const KEEP_THE_FOLDER: &str = "Point CORNERCASE_SOCKET at a socket in a folder only you can write to";
const REMOVE_THE_FOLDER: &str = "If that folder is yours, remove it and start cornercase again";

fn refused_with(out: &std::process::Output, advice: &str) -> bool {
    let stderr = String::from_utf8_lossy(&out.stderr);
    let removal = stderr.to_lowercase().contains("remove");
    !out.status.success()
        && stderr.contains("can be written by other users")
        && stderr.contains(advice)
        && (advice == REMOVE_THE_FOLDER || !removal)
}

fn finish_within(mut child: std::process::Child, timeout: Duration) -> std::process::Output {
    let deadline = Instant::now() + timeout;
    while child.try_wait().expect("check the process").is_none() && Instant::now() < deadline {
        thread::sleep(POLL);
    }
    let _ = child.kill();
    child.wait_with_output().expect("collect its output")
}

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_cornercase")
}

fn refused(out: &std::process::Output, code: i32, message: &str) -> bool {
    out.status.code() == Some(code) && String::from_utf8_lossy(&out.stderr).contains(message)
}

#[test]
fn status_lists_the_window_and_marks_the_pane_it_runs_in() {
    let mut app = Harness::start();

    let report = app.session.report();
    let inside = app.session.dir.join("inside.json");
    app.send(format!("{} status --json > '{}'; echo status-\"\"saved\r", bin(), inside.display()).as_bytes());
    app.wait_for("the status is saved", |s| s.contains("status-saved"));

    let seen: Report = serde_json::from_str(&std::fs::read_to_string(&inside).expect("read it")).expect("a report");
    assert_eq!(report.projects[0].path, temp());
    assert_eq!((report.caller, seen.caller), (None, report.shown.pane));
}

#[test]
fn open_adds_a_project_and_leaves_the_window_as_it_is() {
    let mut app = Harness::start();
    let name = format!("ccop-{}", std::process::id());
    let dir = temp_dir_named(&name);
    let path = dir.display().to_string();

    let id = app.session.says(&["open", &path]);

    app.wait_for("the project shows", |s| s.contains(&entry(&name)));
    assert!(app.text().contains(&format!("▌{}", first_entry())), "the window moved:\n{}", app.text());
    assert_eq!(app.session.says(&["open", &path]), id);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn new_workspace_adds_one_to_the_project() {
    let app = Harness::start();

    let id = app.session.says(&["new-workspace", "notes"]);

    let report = app.session.report();
    let workspace = report.projects[0].workspaces.iter().find(|w| w.id.to_string() == id).expect("the workspace");
    let shown = report.shown.workspace == Some(workspace.id);
    assert_eq!((workspace.name.as_str(), workspace.tabs.len(), shown), ("notes", 1, false));
}

#[test]
fn a_command_typed_in_a_new_tab_is_waited_for_and_read() {
    let mut app = Harness::start();

    let pane = app.session.says(&["new-tab", "--name", "build", "--", "sleep 0.5; echo built-$((40+2))"]);
    let ended = app.session.says(&["wait", "--pane", &pane, "--until", "shell", "--timeout", "10"]);

    assert_eq!(ended, "shell");
    assert!(app.session.says(&["read", "--pane", &pane, "--lines", "5"]).contains("built-42"));
    app.wait_for("the tab shows in the list", |s| s.contains("build"));
}

#[test]
fn split_opens_a_pane_beside_the_shown_one() {
    let mut app = Harness::start();

    let pane = app.session.says(&["split", "--", "echo", "split-\"\"done"]);

    app.wait_for("the new pane runs its command", |s| s.contains("split-done"));
    let tab = &app.session.report().projects[0].workspaces[0].tabs[0];
    assert_eq!(tab.panes.iter().map(|p| p.id.to_string()).collect::<Vec<_>>()[1], pane);
}

#[test]
fn an_agent_started_from_the_command_line_takes_its_prompts_and_is_waited_for() {
    let session = Session::new();
    std::fs::create_dir(session.dir.join("bin")).expect("create bin");
    let agent = session.dir.join("bin").join("claude");
    write_executable(
        &agent,
        "#!/bin/sh\nd=\"$CLAUDE_CONFIG_DIR/sessions\"; mkdir -p \"$d\"; s=\"$d/$$.json\"\n\
         printf '{\"pid\":%s,\"status\":\"idle\"}' $$ > \"$s\"\n\
         while printf 'agent> ' && IFS= read -r line; do\n\
         printf '{\"pid\":%s,\"status\":\"busy\"}' $$ > \"$s\"\n\
         while [ ! -e \"$CLAUDE_CONFIG_DIR/finish\" ]; do sleep 0.02; done; rm -f \"$CLAUDE_CONFIG_DIR/finish\"\n\
         printf 'done: %s\\n' \"$line\"; printf '{\"pid\":%s,\"status\":\"idle\"}' $$ > \"$s\"; done\n",
    );
    let config = format!("{{\"agent_commands\": {{\"claude\": \"{}\"}}}}", agent.display());
    std::fs::write(session.dir.join("config.json"), config).expect("write config");
    let mut app = Harness::open(Arc::clone(&session), ROWS, COLS);
    app.wait_for("app starts with one terminal", |s| s.contains(&first_entry()));
    let finish = || {
        session.wait_for_working();
        std::fs::write(session.claude_dir().join("finish"), "").expect("let the agent finish");
    };

    let start = session.spawn(&["start", "claude", "--prompt", "fix the login", "--wait", "--timeout", "30"]);
    finish();
    let started = Session::output(start);
    let lines: Vec<&str> = started.lines().collect();
    let [pane, ended] = lines[..] else { panic!("not a pane and an ending: {started}") };
    let send = session.spawn(&["send", "--pane", pane, "--enter", "--wait", "--timeout", "30", "add tests"]);
    finish();
    let next = Session::output(send);

    assert_eq!((ended, next.as_str()), ("done", "done"));
    let screen = session.says(&["read", "--pane", pane]);
    assert!(screen.contains("done: fix the login") && screen.contains("done: add tests"), "{screen}");
}

#[test]
fn keys_reach_the_program_in_the_pane() {
    let mut app = Harness::start();
    app.send(b"echo cat-\"\"starts; cat -v\r");
    app.wait_for("cat runs", |s| s.contains("cat-starts"));

    app.session.says(&["keys", "ctrl+b", "x", "enter"]);

    app.wait_for("cat echoes them", |s| s.matches("^Bx").count() >= 2);
}

#[test]
fn close_ends_a_tab_and_its_shell() {
    let mut app = Harness::start();
    let pane = app.session.says(&["new-tab", "--name", "doomed"]);
    app.wait_for("the tab shows", |s| s.contains("doomed"));

    app.session.says(&["close", "--pane", &pane]);

    app.wait_for("the tab goes", |s| !s.contains("doomed"));
}

#[test]
fn rename_inside_a_pane_names_its_own_tab() {
    let mut app = Harness::start();

    app.send(format!("{} rename from-\"\"inside\r", bin()).as_bytes());

    let tab = usize::from(ui::workspace_row(workspaces_list(), 1, &tab_lines(&[1]), 0, WorkspaceRow::Tab(0, 0)).y);
    app.wait_for("the tab has its name", |s| s.lines().nth(tab).is_some_and(|l| l.contains("from-inside")));
}

#[test]
fn focus_shows_another_tab() {
    let mut app = Harness::start();
    let pane = app.session.says(&["new-tab", "--", "echo", "in-the-\"\"second"]);
    assert!(!app.text().contains("in-the-second"), "the window moved:\n{}", app.text());

    app.session.says(&["focus", "--pane", &pane]);

    app.wait_for("the second tab shows", |s| s.contains("in-the-second"));
}

#[test]
fn notify_reaches_the_window_and_the_desktop() {
    let mut app = Harness::start();

    app.session.says(&["notify", "the", "build", "is", "ready"]);

    app.wait_for_raw("ghostty is asked to notify", |raw| raw.contains("\x1b]777;notify;cornercase;the build is ready"));
    app.wait_for("a toast says it", |s| s.contains("the build is ready"));
}

#[test]
fn the_todo_list_takes_commands() {
    let app = Harness::start();

    let id = app.session.says(&["todo", "add", "buy", "milk"]);
    app.session.says(&["todo", "done", &id]);

    assert_eq!(app.session.says(&["todo", "list"]), format!("{id} [x] buy milk"));
    app.session.wait_for_file("todos.json", "the item is saved", |saved| saved.contains("buy milk"));
}

#[test]
fn a_worktree_made_from_the_command_line_is_removed_by_it() {
    let session = Session::new();
    let worktrees = session.dir.join("worktrees");
    std::fs::write(session.dir.join("config.json"), format!("{{\"worktrees_dir\": \"{}\"}}", worktrees.display()))
        .expect("write config");
    let repo = temp_dir("cli-worktree");
    git(&repo, &["init", "--quiet"]);
    git(&repo, &["commit", "--quiet", "--allow-empty", "-m", "init"]);
    let mut app = Harness::open(Arc::clone(&session), ROWS, COLS);
    app.wait_for("app starts with one terminal", |s| s.contains(&first_entry()));
    let project = session.says(&["open", &repo.display().to_string()]);

    let workspace = session.says(&["new-workspace", "e2e/cli", "--worktree", "--project", &project]);
    let checkout = worktrees.join(repo.file_name().expect("repo name")).join("e2e-cli");
    assert!(checkout.is_dir(), "no checkout at {}", checkout.display());
    session.says(&["close", "--workspace", &workspace, "--remove-worktree"]);

    assert!(!checkout.exists(), "the checkout is still there");
    let _ = std::fs::remove_dir_all(&repo);
}

#[test]
fn skill_prints_the_instructions_for_agents() {
    let out = std::process::Command::new(bin()).arg("skill").output().expect("run cornercase");

    assert!(String::from_utf8_lossy(&out.stdout).starts_with("---\nname: cornercase\n"), "{out:?}");
}

#[test]
fn a_command_without_a_server_says_so_and_starts_none() {
    let session = Session::new();

    let out = session.cli(&["status"]);

    assert!(refused(&out, 1, "no cornercase server is running"), "{out:?}");
    assert_eq!(session.servers(), 0);
}

#[test]
fn wrong_usage_exits_with_2() {
    let session = Session::new();

    let out = session.cli(&["send", "--wait", "hi"]);

    assert!(refused(&out, 2, "--enter"), "{out:?}");
}

#[test]
fn a_server_without_a_window_refuses_new_terminals() {
    let session = Session::new();
    let mut server = session.command().arg("server").spawn().expect("start a server");
    let deadline = Instant::now() + TIMEOUT;
    while std::os::unix::net::UnixStream::connect(session.socket()).is_err() {
        assert!(Instant::now() < deadline, "the server never listened");
        thread::sleep(POLL);
    }

    let out = session.cli(&["new-tab"]);

    assert!(refused(&out, 1, "has not opened a window yet"), "{out:?}");
    assert_eq!(session.report().projects, []);
    drop(session);
    let _ = server.wait();
}

#[test]
fn a_remote_window_comes_back_after_the_connection_drops() {
    let session = Session::new();
    let path = fake_ssh(&session);
    let exe = env!("CARGO_BIN_EXE_cornercase");
    let args = ["remote", "devbox", "--command", exe];
    let mut app = Harness::run(exe.as_ref(), &args, Arc::clone(&session), (ROWS, COLS), &[("PATH", &path)]);
    app.wait_for("the remote window shows its shell", |s| s.contains(&first_entry()));
    app.send(b"kept=yes; echo set-\"\"done\r");
    app.wait_for("the shell ran it", |s| s.contains("set-done"));
    let first = ssh_pid(&session);

    let killed = std::process::Command::new("kill").args(["-9", &first]).status().expect("run kill");

    assert!(killed.success(), "kill failed");
    app.wait_for_raw("the window says it reconnects", |raw| raw.contains("reconnecting to devbox"));
    app.wait_for("the window comes back", |s| !s.contains("reconnecting") && s.contains(&first_entry()));
    assert_ne!(ssh_pid(&session), first, "a new ssh");
    app.send(b"echo \"$kept\"-still\r");
    app.wait_for("the same shell answers", |s| s.contains("yes-still"));
    assert_eq!(session.servers(), 1);
}

#[test]
fn a_remote_of_another_version_says_which_side_to_update() {
    let dir = temp_dir("old-remote");
    let old = dir.join("cornercase");
    write_executable(&old, "#!/bin/sh\necho 'cornercase-proxy 1 0.0.1'\ncat > /dev/null\n");

    let out = remote_refusal(&old.display().to_string());

    let _ = std::fs::remove_dir_all(&dir);
    assert!(refused(&out, 1, "`devbox` runs cornercase 0.0.1"), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stderr).contains("Run `cornercase update` there"), "{out:?}");
}

#[test]
fn a_remote_without_cornercase_says_how_to_point_at_it() {
    let out = remote_refusal("/nonexistent/cornercase");

    assert!(refused(&out, 1, "cornercase was not found on `devbox`"), "{out:?}");
}
