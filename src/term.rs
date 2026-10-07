use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Sender};
use std::thread;
use std::time::Instant;

use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};

use crate::activity;
use crate::agents;
use crate::app::AppEvent;
use crate::config::Config;
use crate::context;
use crate::control;
use crate::emulator::Emulator;
use crate::error::{Error, Result};
use crate::host_theme::HostTheme;
use crate::memory;
use crate::process;
use crate::protocol;

const SCROLLBACK: usize = 5_000;
const READ_BUFFER: usize = 16 * 1024;
const INTERPRETERS: [&str; 6] = ["node", "bun", "deno", "python", "python3", "ruby"];
const NODE_MAIN_THREAD: &str = "node-MainThread";
const SCRIPT_EXTENSIONS: [&str; 9] = ["js", "mjs", "cjs", "ts", "mts", "cts", "py", "rb", "sh"];
const NOT_A_SCRIPT: [&str; 6] = ["-e", "--eval", "-p", "--print", "-c", "-m"];
const PASTE_START: &str = "\x1b[200~";
const PASTE_END: &str = "\x1b[201~";

pub const MAX_QUEUED: usize = 16 * 1024 * 1024;

#[derive(Clone)]
struct Writer {
    tx: Sender<Vec<u8>>,
    queued: Arc<AtomicUsize>,
}

pub struct Term {
    pub id: u64,
    pub emulator: Emulator,
    pub agent: activity::Pane,
    pub context: context::Pane,
    pub memory: memory::Pane,
    pub output_at: Instant,
    pub input_at: Option<Instant>,
    pub submitted: Option<Instant>,
    master: Box<dyn MasterPty + Send>,
    writer: Writer,
    child: Box<dyn Child + Send + Sync>,
    size: (u16, u16),
}

pub struct SpawnOptions<'a> {
    pub id: u64,
    pub shell: &'a str,
    pub args: &'a [String],
    pub env: &'a [(String, String)],
    pub rows: u16,
    pub cols: u16,
    pub cwd: Option<PathBuf>,
    pub theme: &'a HostTheme,
}

fn pty_size(rows: u16, cols: u16) -> PtySize {
    PtySize { rows, cols, pixel_width: 0, pixel_height: 0 }
}

impl Term {
    pub fn spawn(opts: SpawnOptions, tx: Sender<AppEvent>) -> Result<Self> {
        let SpawnOptions { id, shell, args, env, rows, cols, cwd, theme } = opts;
        let pair = native_pty_system().openpty(pty_size(rows, cols)).map_err(|e| Error::OpenPty(e.into()))?;

        let mut cmd = CommandBuilder::new(shell);
        cmd.args(args);
        cmd.env("TERM", "xterm-256color");
        cmd.env(protocol::NESTED_ENV, "1");
        cmd.env(control::PANE_ENV, id.to_string());
        cmd.env(control::SERVER_ENV, control::server_token());
        for key in activity::CLAUDE_SESSION_ENV {
            cmd.env_remove(key);
        }
        for (key, value) in env {
            cmd.env(key, value);
        }
        let dir = cwd.or_else(|| std::env::current_dir().ok());
        if let Some(dir) = &dir {
            cmd.cwd(dir);
        }

        let child = pair.slave.spawn_command(cmd).map_err(|e| Error::SpawnShell {
            shell: shell.to_string(),
            dir: dir.unwrap_or_default(),
            cause: e.into(),
        })?;
        drop(pair.slave);

        let reader = pair.master.try_clone_reader().map_err(|e| Error::AttachPty(e.into()))?;
        let writer = spawn_writer(pair.master.take_writer().map_err(|e| Error::AttachPty(e.into()))?);
        let replies = writer.clone();
        let emulator =
            Emulator::new(rows, cols, SCROLLBACK, theme, Box::new(move |bytes| _ = write_to(&replies, bytes)))
                .map_err(|e| Error::Emulator(e.into()))?;
        spawn_reader(id, reader, tx);

        Ok(Self {
            id,
            emulator,
            agent: activity::Pane::default(),
            context: context::Pane::default(),
            memory: memory::Pane::default(),
            output_at: Instant::now(),
            input_at: None,
            submitted: None,
            master: pair.master,
            writer,
            child,
            size: (rows, cols),
        })
    }

    pub fn feed(&mut self, bytes: &[u8]) {
        self.output_at = Instant::now();
        self.emulator.feed(bytes);
    }

    pub fn write(&mut self, bytes: &[u8]) -> bool {
        self.input_at = Some(Instant::now());
        write_to(&self.writer, bytes)
    }

    pub fn paste(&mut self, text: &str) -> bool {
        if self.emulator.bracketed_paste() {
            self.write(bracketed(text).as_bytes())
        } else {
            self.write(text.as_bytes())
        }
    }

    pub fn resize(&mut self, rows: u16, cols: u16) {
        if self.size == (rows, cols) || rows == 0 || cols == 0 {
            return;
        }
        self.size = (rows, cols);
        let _ = self.master.resize(pty_size(rows, cols));
        let _ = self.emulator.resize(rows, cols);
    }

    pub fn foreground_pid(&self) -> Option<i32> {
        self.master.process_group_leader().filter(|pid| process::alive(*pid)).or_else(|| self.shell_pid())
    }

    pub fn shell_pid(&self) -> Option<i32> {
        self.child.process_id().and_then(|pid| i32::try_from(pid).ok())
    }

    pub fn cwd(&self) -> Option<PathBuf> {
        process::cwd(self.foreground_pid()?)
    }

    pub fn program(&self, config: &Config) -> Option<String> {
        let pid = self.foreground_pid()?;
        let name = process::name(pid)?;
        Some(program(config, &name, &process::args(pid)))
    }

    pub fn shell_in_foreground(&self) -> bool {
        let shell = self.shell_pid();
        shell.is_some() && self.foreground_pid() == shell
    }

    pub fn foreground_args(&self) -> Vec<String> {
        self.foreground_pid().map(process::args).unwrap_or_default()
    }

    pub fn exited(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(Some(_)))
    }

    pub fn kill(&mut self) {
        if !self.exited() {
            let _ = self.child.kill();
        }
    }
}

impl std::fmt::Debug for Term {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Term").field("id", &self.id).field("size", &self.size).finish_non_exhaustive()
    }
}

impl Drop for Term {
    fn drop(&mut self) {
        self.kill();
    }
}

pub fn bracketed(text: &str) -> String {
    format!("{PASTE_START}{}{PASTE_END}", text.replace(PASTE_END, ""))
}

pub fn program(config: &Config, name: &str, argv: &[String]) -> String {
    let name = if name == NODE_MAIN_THREAD { "node" } else { name };
    let interpreter = INTERPRETERS.contains(&name);
    let considered = if interpreter { argv } else { argv.get(..1).unwrap_or_default() };
    agents::detect(config, considered)
        .or_else(|| interpreter.then(|| script(argv)).flatten())
        .unwrap_or_else(|| name.to_string())
}

fn script(argv: &[String]) -> Option<String> {
    let rest = argv.get(1..)?;
    let at = rest.iter().position(|a| !a.starts_with('-'))?;
    if rest[..at].iter().any(|flag| NOT_A_SCRIPT.contains(&flag.as_str())) {
        return None;
    }
    let arg = &rest[at];
    let path = Path::new(arg);
    let named =
        arg.contains('/') || path.extension().and_then(|e| e.to_str()).is_some_and(|e| SCRIPT_EXTENSIONS.contains(&e));
    if !named || arg.chars().any(char::is_whitespace) {
        return None;
    }
    path.file_stem().and_then(|s| s.to_str()).filter(|s| !s.is_empty()).map(str::to_string)
}

fn write_to(writer: &Writer, bytes: &[u8]) -> bool {
    if writer.queued.load(Ordering::Relaxed) >= MAX_QUEUED {
        return false;
    }
    writer.queued.fetch_add(bytes.len(), Ordering::Relaxed);
    writer.tx.send(bytes.to_vec()).is_ok()
}

fn spawn_writer(mut pty: Box<dyn Write + Send>) -> Writer {
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    let queued = Arc::new(AtomicUsize::new(0));
    let written = Arc::clone(&queued);
    thread::spawn(move || {
        for bytes in rx {
            if pty.write_all(&bytes).and_then(|()| pty.flush()).is_err() {
                return;
            }
            written.fetch_sub(bytes.len(), Ordering::Relaxed);
        }
    });
    Writer { tx, queued }
}

fn spawn_reader(id: u64, mut reader: Box<dyn Read + Send>, tx: Sender<AppEvent>) {
    thread::spawn(move || {
        let mut buf = vec![0u8; READ_BUFFER];
        while let Ok(n @ 1..) = reader.read(&mut buf) {
            if tx.send(AppEvent::Output(id, buf[..n].to_vec())).is_err() {
                return;
            }
        }
        let _ = tx.send(AppEvent::Exited(id));
    });
}

#[cfg(test)]
mod tests {
    use std::assert_matches;
    use std::sync::mpsc::{self, Receiver};
    use std::time::Duration;

    use rstest::rstest;

    use super::*;
    use crate::test_util::{TempDir, is_sh, wait_until};

    const RECV_TIMEOUT: Duration = Duration::from_secs(5);

    fn spawn_sh_in(cwd: Option<PathBuf>) -> (Term, Receiver<AppEvent>) {
        let (tx, rx) = mpsc::channel();
        let opts = SpawnOptions {
            id: 1,
            shell: "/bin/sh",
            args: &[],
            env: &[],
            rows: 24,
            cols: 80,
            cwd,
            theme: &HostTheme::default(),
        };
        (Term::spawn(opts, tx).expect("spawn /bin/sh"), rx)
    }

    fn spawn_sh() -> (Term, Receiver<AppEvent>) {
        spawn_sh_in(None)
    }

    fn contents(term: &mut Term, rx: &Receiver<AppEvent>) -> String {
        while let Ok(ev) = rx.try_recv() {
            if let AppEvent::Output(_, bytes) = ev {
                term.feed(&bytes);
            }
        }
        term.emulator.snapshot().expect("snapshot").contents()
    }

    mod spawn {
        use super::*;

        #[test]
        fn returns_error_when_shell_does_not_exist() {
            let (tx, _rx) = mpsc::channel();
            let opts = SpawnOptions {
                id: 1,
                shell: "/nonexistent/shell",
                args: &[],
                env: &[],
                rows: 24,
                cols: 80,
                cwd: None,
                theme: &HostTheme::default(),
            };

            let result = Term::spawn(opts, tx);

            assert_matches!(result, Err(Error::SpawnShell { .. }));
        }

        #[test]
        fn starts_in_requested_dir() {
            let tmp = std::env::temp_dir().canonicalize().expect("temp dir");
            let (term, _rx) = spawn_sh_in(Some(tmp.clone()));

            wait_until("shell starts in requested dir", || term.cwd().as_ref() == Some(&tmp));
        }
    }

    mod shell_pid {
        use super::*;

        #[test]
        fn is_the_parent_of_the_program_in_the_foreground() {
            let (mut term, _rx) = spawn_sh();

            term.write(b"/bin/sleep 30\r");

            wait_until("sleep runs under the shell", || {
                term.shell_pid()
                    .zip(term.foreground_pid())
                    .is_some_and(|(shell, pid)| process::children(shell).contains(&pid))
            });
            term.write(b"\x03");
        }
    }

    mod output {
        use super::*;

        #[test]
        fn is_parsed_into_the_screen() {
            let (mut term, rx) = spawn_sh();

            term.write(b"echo hello-$((1+1))\r");

            wait_until("output appears on screen", || contents(&mut term, &rx).contains("hello-2"));
        }

        #[test]
        fn sends_output_event() {
            let (mut term, rx) = spawn_sh();

            term.write(b"echo x\r");

            assert_matches!(rx.recv_timeout(RECV_TIMEOUT), Ok(AppEvent::Output(1, _)));
        }
    }

    mod writing {
        use super::*;

        struct Stuck(Receiver<()>);

        impl Write for Stuck {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                let _ = self.0.recv();
                Err(std::io::Error::other("released"))
            }

            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }

        #[test]
        fn a_terminal_that_stops_taking_input_gets_no_more_than_its_queue() {
            let (release, stuck) = mpsc::channel();
            let writer = spawn_writer(Box::new(Stuck(stuck)));

            let taken = [write_to(&writer, &vec![b'x'; MAX_QUEUED]), write_to(&writer, b"y")];
            drop(release);

            assert_eq!(taken, [true, false]);
        }

        #[test]
        fn a_paste_cannot_end_itself_early() {
            assert_eq!(bracketed("ls\x1b[201~\rrm -rf x"), "\x1b[200~ls\rrm -rf x\x1b[201~");
        }

        #[test]
        fn a_program_that_does_not_read_never_blocks_the_writer() {
            let (mut term, _rx) = spawn_sh();
            term.write(b"stty -echo; sleep 2; cat > /dev/null\r");
            wait_until("sleep runs", || term.program(&Config::default()).as_deref() == Some("sleep"));
            let lines = format!("{}\n", "x".repeat(63)).repeat(16 * 1024);

            let started = Instant::now();
            term.write(lines.as_bytes());

            assert!(started.elapsed() < Duration::from_millis(500), "the write took {:?}", started.elapsed());
        }
    }

    mod replies {
        use super::*;

        #[test]
        fn reach_the_program() {
            let (mut term, rx) = spawn_sh();

            term.write(b"stty -icanon -echo; printf '\\033[5n'; head -c 4 | tr '\\033' E; stty sane\r");

            wait_until("program reads the status reply", || contents(&mut term, &rx).contains("E[0n"));
        }
    }

    mod cwd {
        use super::*;

        #[test]
        fn follows_cd() {
            let (mut term, _rx) = spawn_sh();

            term.write(b"cd /\r");

            wait_until("cwd follows cd", || term.cwd() == Some(PathBuf::from("/")));
        }
    }

    mod program {
        use super::*;

        fn label(name: &str, argv: &[&str]) -> String {
            let argv: Vec<String> = argv.iter().map(|a| (*a).to_string()).collect();
            program(&Config::default(), name, &argv)
        }

        #[rstest]
        #[case::codex_through_npm("node-MainThread", &["node", "/usr/local/bin/codex"], "codex")]
        #[case::codex_by_its_script("node", &["node", "/usr/lib/node_modules/@openai/codex/bin/codex.js"], "codex")]
        #[case::a_native_agent("claude", &["/home/a/.local/bin/claude", "--resume"], "claude")]
        #[case::a_renamed_agent("cursor-agent", &["cursor-agent"], "cursor")]
        #[case::a_script_without_extension("node", &["node", "/usr/bin/vite", "--port", "3000"], "vite")]
        #[case::a_flag_before_the_script("node", &["node", "--inspect", "server.mjs"], "server")]
        #[case::a_python_script("python3", &["python3", "-u", "./tools/sync.py"], "sync")]
        #[case::a_python_module("python3", &["python3", "-m", "http.server"], "python3")]
        #[case::inline_code("ruby", &["ruby", "-e", "puts 'a/b'"], "ruby")]
        #[case::inline_code_naming_a_file("node", &["node", "-e", "require('./lib/foo.js')"], "node")]
        #[case::python_code_naming_a_file("python3", &["python3", "-c", "import./tools/sync.py"], "python3")]
        #[case::a_bare_interpreter("node", &["node"], "node")]
        #[case::the_node_thread_name("node-MainThread", &["node"], "node")]
        #[case::another_program("nvim", &["nvim", "src/main.rs"], "nvim")]
        #[case::a_program_with_an_agent_argument("less", &["less", "claude"], "less")]
        fn names_the_program(#[case] name: &str, #[case] argv: &[&str], #[case] expected: &str) {
            assert_eq!(label(name, argv), expected);
        }

        #[test]
        fn is_the_shell_when_idle() {
            let (term, _rx) = spawn_sh();

            wait_until("shows the shell", || term.program(&Config::default()).as_deref().is_some_and(is_sh));
        }

        #[test]
        fn is_the_foreground_program_while_it_runs() {
            let (mut term, _rx) = spawn_sh();

            term.write(b"sleep 30\r");

            wait_until("shows the foreground program", || term.program(&Config::default()).as_deref() == Some("sleep"));
        }

        #[test]
        fn falls_back_to_shell_when_program_ends() {
            let (mut term, _rx) = spawn_sh();
            term.write(b"sleep 30\r");
            wait_until("sleep starts", || term.program(&Config::default()).as_deref() == Some("sleep"));

            term.write(&[0x03]);

            wait_until("back to the shell", || term.program(&Config::default()).as_deref().is_some_and(is_sh));
        }

        #[test]
        fn is_the_script_an_interpreter_runs() {
            let dir = TempDir::new();
            let node = dir.path().join("node");
            let status = std::process::Command::new("/bin/sh")
                .arg("-c")
                .arg("cp /bin/bash \"$1\"")
                .arg("sh")
                .arg(&node)
                .status()
                .expect("run cp");
            assert!(status.success(), "copy bash");
            std::fs::create_dir(dir.path().join("bin")).expect("create bin");
            std::fs::write(dir.path().join("bin/tool.js"), "read line\n").expect("write script");
            let (mut term, _rx) = spawn_sh_in(Some(dir.path().to_path_buf()));

            term.write(b"./node bin/tool.js\r");

            wait_until("shows the script", || term.program(&Config::default()).as_deref() == Some("tool"));
        }
    }

    mod resize {
        use super::*;

        #[test]
        fn updates_the_emulator_size() {
            let (mut term, _rx) = spawn_sh();

            term.resize(10, 50);

            assert_eq!(term.emulator.size().expect("size"), (10, 50));
        }

        #[test]
        fn reaches_the_program() {
            let (mut term, rx) = spawn_sh();

            term.resize(10, 50);
            term.write(b"stty size\r");

            wait_until("stty sees the new size", || contents(&mut term, &rx).contains("10 50"));
        }
    }

    mod kill {
        use super::*;

        #[test]
        fn sends_exited_event() {
            let (mut term, rx) = spawn_sh();

            term.kill();

            let exited =
                std::iter::from_fn(|| rx.recv_timeout(RECV_TIMEOUT).ok()).any(|ev| matches!(ev, AppEvent::Exited(1)));
            assert!(exited, "no Exited(1) event received");
        }
    }
}
