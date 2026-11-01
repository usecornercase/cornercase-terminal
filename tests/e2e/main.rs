use std::fmt::Write as _;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use cornercase::agents::claude::{CLAUDE_DIR_ENV, CLAUDE_SESSION_ENV};
use cornercase::config;
use cornercase::control::{PANE_ENV, PaneInfo, PaneRow, PaneRows, Report};
use cornercase::paths::SOCKET_ENV;
use cornercase::project::split::{self, Dir};
use cornercase::protocol::{NESTED_ENV, REMOTE_ENV};
use cornercase::state;
use cornercase::ui::{self, SidebarRow, WorkspaceRow};
use cornercase::update;
use parking_lot::Mutex;
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use ratatui::layout::{Position, Rect};
use rstest::rstest;
use sha2::{Digest, Sha256};

mod accounts;
mod agents;
mod arranging;
mod browser;
mod commands;
mod contract;
mod flows;
mod input;
mod issues;
mod machine_actions;
mod machines;
mod opening;
mod panels;
mod panes;
mod plugins;
mod ports;
mod processes;
mod queue;
mod remote;
mod server;
mod settings;
mod sidebar;
mod socket;
mod startup;
mod todos;
mod updates;
mod usage;
mod views;
mod worktrees;

const ROWS: u16 = 24;
const COLS: u16 = 100;
const TIMEOUT: Duration = Duration::from_secs(10);
const POLL: Duration = Duration::from_millis(30);
const AREA: Rect = Rect { x: 0, y: 0, width: COLS, height: ROWS };
const HOST_THEME_REPLY: &[u8] = b"\x1b]11;rgb:12/56/9a\x1b\\\x1bP>|ghostty 1.2.0\x1b\\\x1b[?62;22c";
const GHOSTTY_1_3_1: &[u8] = b"\x1b]11;rgb:20/20/20\x1b\\\x1b[6;20;10t\x1bP>|ghostty 1.3.1\x1b\\\x1b[?62;22;52c";
const UNKNOWN_TERMINAL: &[u8] = b"\x1b]11;rgb:20/20/20\x1b\\\x1b[?62;22c";
const TERMINAL_ENV: [&str; 17] = [
    "TMUX",
    "STY",
    "ZELLIJ",
    "TERM_PROGRAM",
    "TERM_PROGRAM_VERSION",
    "LC_TERMINAL",
    "CORNERCASE_IMAGES",
    "GHOSTTY_RESOURCES_DIR",
    "KITTY_WINDOW_ID",
    "ALACRITTY_WINDOW_ID",
    "KONSOLE_VERSION",
    "VTE_VERSION",
    "PTYXIS_VERSION",
    "WT_SESSION",
    "TERMINAL_EMULATOR",
    "INSIDE_EMACS",
    "XTERM_VERSION",
];

struct Session {
    dir: PathBuf,
}

impl Session {
    fn new() -> Arc<Self> {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("cornercase-e2e-sock-{}-{n}", std::process::id()));
        std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&dir).expect("create socket dir");
        std::fs::write(dir.join("tour.json"), r#"{"version":1,"offered":true,"announced":true}"#).expect("write tour");
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

    fn servers(&self) -> usize {
        servers_with(&self.socket_var())
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
        cmd.env(SOCKET_ENV, self.socket())
            .env_remove(NESTED_ENV)
            .env_remove(PANE_ENV)
            .env_remove(REMOTE_ENV)
            .env_remove(update::LATEST_ENV)
            .env_remove("CORNERCASE_SERVER");
        cmd
    }

    fn cli(&self, args: &[&str]) -> std::process::Output {
        self.command().args(args).output().expect("run cornercase")
    }

    fn fed(&self, args: &[&str], input: &str) -> std::process::Output {
        let mut cmd = self.command();
        cmd.args(args);
        feed(cmd, input)
    }

    fn cold_command(&self) -> std::process::Command {
        let mut cmd = self.command();
        cmd.env("SHELL", "/bin/sh").env("PS1", "$ ").env(CLAUDE_DIR_ENV, self.claude_dir());
        for var in ["SHORTCUT_API_TOKEN", "LINEAR_API_KEY"].into_iter().chain(TERMINAL_ENV) {
            cmd.env_remove(var);
        }
        cmd.current_dir(std::env::temp_dir());
        cmd
    }

    fn cold(&self, args: &[&str]) -> std::process::Output {
        self.cold_command().args(args).output().expect("run cornercase")
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

fn feed(mut cmd: std::process::Command, input: &str) -> std::process::Output {
    let mut child = cmd
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("run cornercase");
    child.stdin.take().expect("stdin").write_all(input.as_bytes()).expect("write to cornercase");
    child.wait_with_output().expect("wait for cornercase")
}

#[cfg(target_os = "linux")]
fn servers_with(var: &str) -> usize {
    let Ok(procs) = std::fs::read_dir("/proc") else { return 0 };
    procs
        .flatten()
        .filter(|entry| {
            let cmdline = std::fs::read(entry.path().join("cmdline")).unwrap_or_default();
            let environ = std::fs::read(entry.path().join("environ")).unwrap_or_default();
            cmdline.split(|b| *b == 0).nth(1) == Some(b"server".as_slice())
                && environ.split(|b| *b == 0).any(|word| word == var.as_bytes())
        })
        .count()
}

#[cfg(target_os = "macos")]
fn servers_with(var: &str) -> usize {
    let ps = std::process::Command::new("ps").args(["-xEww", "-o", "command="]).output().expect("run ps");
    String::from_utf8_lossy(&ps.stdout)
        .lines()
        .filter(|line| {
            line.split_whitespace().nth(1) == Some("server") && line.split_whitespace().any(|word| word == var)
        })
        .count()
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

#[derive(Clone, Copy)]
struct Terminal<'a> {
    bin: &'a std::path::Path,
    args: &'a [&'a str],
    size: (u16, u16),
    env: &'a [(&'a str, &'a str)],
    reply: &'a [u8],
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
        harness.wait_for("client shows the sidebar", |s| s.contains("Projects"));
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

    fn answering(reply: &[u8]) -> Self {
        let bin = std::path::Path::new(env!("CARGO_BIN_EXE_cornercase"));
        let env = [("TERM", "xterm-256color")];
        let terminal = Terminal { bin, args: &[], size: (ROWS, COLS), env: &env, reply };
        let mut harness = Self::run_answering(Session::new(), &terminal);
        harness.wait_for("app starts with one terminal", |s| s.contains(&first_entry()));
        harness
    }

    fn run(
        bin: &std::path::Path,
        args: &[&str],
        session: Arc<Session>,
        size: (u16, u16),
        env: &[(&str, &str)],
    ) -> Self {
        Self::run_answering(session, &Terminal { bin, args, size, env, reply: HOST_THEME_REPLY })
    }

    fn run_answering(session: Arc<Session>, terminal: &Terminal) -> Self {
        let Terminal { bin, args, size: (rows, cols), env, reply } = *terminal;
        let pair =
            native_pty_system().openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 }).expect("open pty");
        let mut cmd = CommandBuilder::new(bin);
        cmd.args(args);
        cmd.env("SHELL", "/bin/sh");
        cmd.env("PS1", "$ ");
        cmd.env(SOCKET_ENV, session.socket());
        cmd.env(CLAUDE_DIR_ENV, session.claude_dir());
        cmd.env("SSH_CONNECTION", "e2e");
        cmd.env_remove(NESTED_ENV);
        cmd.env_remove(PANE_ENV);
        cmd.env_remove(REMOTE_ENV);
        cmd.env_remove("CORNERCASE_SERVER");
        for var in ["SHORTCUT_API_TOKEN", "LINEAR_API_KEY"].into_iter().chain(TERMINAL_ENV) {
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
        harness.wait_for_raw("app asks for the host colors", |raw| raw.contains("\x1b]4;15;?\x1b\\\x1b[>q\x1b[c"));
        harness.send(reply);
        harness
    }

    fn text(&self) -> String {
        self.screen.lock().screen().contents()
    }

    fn row(&self, y: u16) -> String {
        self.screen.lock().screen().contents_between(y, 0, y, self.cols)
    }

    fn position_of(&self, text: &str) -> Position {
        let rows = self.screen.lock().screen().size().0;
        (0..rows)
            .rev()
            .find_map(|y| {
                let row = self.row(y);
                let at = row.find(text)?;
                Some(Position::new(u16::try_from(row[..at].chars().count()).ok()?, y))
            })
            .expect("the text is on screen")
    }

    fn pick_in_more(&mut self, entry: &str) {
        self.click(areas().more.as_position());
        self.wait_for("the more menu opens", |s| s.contains(entry) && s.contains("quit"));
        let at = self.position_of(entry);
        self.click(at);
    }

    fn quit(&mut self) {
        self.pick_in_more("quit");
    }

    fn restart_server(&mut self) {
        self.send(b"echo before-\"\"restart\r");
        self.wait_for("the old shell answers", |text| text.contains("before-restart"));
        self.session.says(&["restart", "--yes"]);
        self.wait_for("the new shell comes back", |text| {
            !text.contains("before-restart") && text.contains(&first_entry())
        });
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
        self.wait_for("the folder picker opens", |s| s.contains("Open project") && s.contains("cancel"));
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

fn tab_lines(counts: &[usize]) -> ui::TabLines {
    ui::TabLines::from(counts.iter().map(|&n| vec![ui::Details::default().lines(); n]).collect::<Vec<_>>())
}

fn workspace_row(tabs: &[usize], row: WorkspaceRow) -> Position {
    let r = ui::workspace_row(workspaces_list(), 1, &tab_lines(tabs), 0, row);
    Position::new(r.x + 3, r.y)
}

fn areas() -> ui::Areas {
    ui::layout_with(AREA, state::Widths::default(), false, config::Sidebar::default())
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

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().fold(String::new(), |mut hex, b| {
        let _ = write!(hex, "{b:02x}");
        hex
    })
}

fn tree_key() -> String {
    sha256_hex(env!("CARGO_MANIFEST_DIR").as_bytes())[..8].to_string()
}

fn own_folder() -> String {
    if cfg!(debug_assertions) { format!("cornercase-dev/{}", tree_key()) } else { "cornercase".into() }
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

fn repository(dir: &std::path::Path, name: &str) -> PathBuf {
    let repo = dir.join(name);
    std::fs::create_dir_all(&repo).expect("create the repository's folder");
    git(&repo, &["init", "--quiet"]);
    git(&repo, &["commit", "--quiet", "--allow-empty", "-m", "init"]);
    repo
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

fn wait_until(what: &str, mut cond: impl FnMut() -> bool) {
    let deadline = Instant::now() + TIMEOUT;
    while !cond() {
        assert!(Instant::now() < deadline, "timed out waiting for: {what}");
        thread::sleep(POLL);
    }
}

fn finish_within(mut child: std::process::Child, timeout: Duration) -> std::process::Output {
    let deadline = Instant::now() + timeout;
    while child.try_wait().expect("check the process").is_none() && Instant::now() < deadline {
        thread::sleep(POLL);
    }
    let _ = child.kill();
    child.wait_with_output().expect("collect its output")
}

fn agent_session() -> Arc<Session> {
    let session = Session::new();
    std::fs::create_dir(session.dir.join("bin")).expect("create bin");
    let agent = session.dir.join("bin").join("claude");
    write_executable(
        &agent,
        r#"#!/bin/sh
d="$CLAUDE_CONFIG_DIR"; mkdir -p "$d/sessions"; s="$d/sessions/$$.json"
t="$d/projects/$(printf %s "$PWD" | tr -c 'A-Za-z0-9' '-')"; mkdir -p "$t"; n=0
state() { printf '{"pid":%s,"sessionId":"s%s","cwd":"%s","status":"%s"}' $$ $$ "$PWD" "$1" > "$s"; }
finished() { while [ ! -e "$d/finish" ]; do sleep 0.02; done; rm -f "$d/finish"; }
said() { n=$((n+1)); printf '{"type":"assistant","timestamp":"2026-10-08T12:00:0%s.000Z","message":{"id":"m%s","model":"claude-opus-5-5","content":[{"type":"text","text":"said: %s"}]}}\n' "$n" "$n" "$1" >> "$t/s$$.jsonl"; }
: > "$t/s$$.jsonl"
state idle
while printf 'agent> ' && IFS= read -r line; do
  case "$line" in *panel*) printf 'Shell details\n'; read -r line; continue ;; esac
  printf '{"type":"user","origin":{"kind":"human"},"timestamp":"%s.999Z"}\n' "$(date -u +%Y-%m-%dT%H:%M:%S)" >> "$t/s$$.jsonl"
  state busy; finished; said "$line"
  case "$line" in *background*) state shell; finished ;; esac
  printf 'done: %s\n' "$line"; state idle
done
"#,
    );
    let config = format!("{{\"agent_commands\": {{\"claude\": \"{}\"}}}}", agent.display());
    std::fs::write(session.dir.join("config.json"), config).expect("write config");
    session
}

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_cornercase")
}

fn refused(out: &std::process::Output, code: i32, message: &str) -> bool {
    out.status.code() == Some(code) && String::from_utf8_lossy(&out.stderr).contains(message)
}

const ISSUES_LIST: &str = r#"[{"number":7,"title":"Fix the login","state":"OPEN","labels":[],"author":{"login":"ana"},"updatedAt":"2026-09-19T12:00:00Z","url":"https://github.com/acme/shop/issues/7"}]"#;

fn fake_issue_tracker(session: &Session, view: &str, trust: &str, repo: &std::path::Path) -> (PathBuf, String) {
    let worktrees = session.dir.join("worktrees");
    let gh = session.dir.join("gh");
    let script = format!(
        "#!/bin/sh\nif [ \"$2\" = view ]; then cat <<'EOF'\n{view}\nEOF\nelse cat <<'EOF'\n{ISSUES_LIST}\nEOF\nfi\n"
    );
    write_executable(&gh, &script);
    let agent = session.dir.join("agent");
    let agent_script = "#!/bin/sh\nprintf 'Do you trust the files in this folder?\\n'\nread answer\n\
        printf '\\033[2J\\033[Hagent ready> '\nread line\nprintf '%s' \"$line\" > got\n";
    write_executable(&agent, agent_script);
    let config = format!(
        r#"{{"worktrees_dir": "{}", "gh": "{}", "agent": "fake", {trust}"agent_commands": {{"fake": "{}"}}}}"#,
        worktrees.display(),
        gh.display(),
        agent.display()
    );
    std::fs::write(session.dir.join("config.json"), config).expect("write config");
    std::fs::create_dir_all(repo).expect("create repo dir");
    let repo_name = repo.file_name().and_then(|n| n.to_str()).expect("repo name").to_string();
    git(repo, &["init", "--quiet"]);
    git(repo, &["commit", "--quiet", "--allow-empty", "-m", "init"]);
    (worktrees, repo_name)
}
