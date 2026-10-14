pub(crate) mod emulator;
pub(crate) mod keys;
pub(crate) mod memory;
pub(crate) mod mouse;
mod status;

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Sender};
use std::thread;
use std::time::Instant;

use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};

use self::emulator::Emulator;
use crate::agents::activity;
use crate::agents::context;
use crate::agents::{self, AgentState, Known};
use crate::config::Config;
use crate::error::{Error, Result};
use crate::host::theme::HostTheme;
use crate::process;

const SCROLLBACK_BYTES: usize = 10_000_000;
const READ_BUFFER: usize = 16 * 1024;
const INTERPRETERS: [&str; 6] = ["node", "bun", "deno", "python", "python3", "ruby"];
const NODE_MAIN_THREAD: &str = "node-MainThread";
const SCRIPT_EXTENSIONS: [&str; 9] = ["js", "mjs", "cjs", "ts", "mts", "cts", "py", "rb", "sh"];
const NOT_A_SCRIPT: [&str; 6] = ["-e", "--eval", "-p", "--print", "-c", "-m"];
const PASTE_START: &str = "\x1b[200~";
const PASTE_END: &str = "\x1b[201~";

pub(crate) const MAX_QUEUED: usize = 16 * 1024 * 1024;

#[derive(Clone)]
struct Writer {
    tx: Sender<Vec<u8>>,
    queued: Arc<AtomicUsize>,
}

pub(crate) struct Term {
    pub(crate) id: u64,
    pub(crate) emulator: Emulator,
    pub(crate) agent: activity::Pane,
    pub(crate) status_agent: Option<(i32, String)>,
    pub(crate) context: context::Pane,
    pub(crate) memory: memory::Pane,
    pub(crate) resume: Option<AgentState>,
    pub(crate) resuming: Option<Instant>,
    pub(crate) output_at: Instant,
    pub(crate) input_at: Option<Instant>,
    pub(crate) submitted: Option<Instant>,
    pub(crate) input_seen: Option<i32>,
    pub(crate) job: Option<Job>,
    master: Box<dyn MasterPty + Send>,
    writer: Writer,
    child: Box<dyn Child + Send + Sync>,
    size: (u16, u16),
    interactive: bool,
}

#[derive(Debug)]
pub(crate) enum Event {
    Output(u64, Vec<u8>),
    Exited(u64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Job {
    pub(crate) pid: i32,
    pub(crate) program: String,
}

pub(crate) struct SpawnOptions<'a> {
    pub(crate) id: u64,
    pub(crate) shell: &'a str,
    pub(crate) args: &'a [String],
    pub(crate) env: &'a [(String, String)],
    pub(crate) rows: u16,
    pub(crate) cols: u16,
    pub(crate) cwd: Option<PathBuf>,
    pub(crate) theme: &'a HostTheme,
}

fn pty_size(rows: u16, cols: u16) -> PtySize {
    PtySize { rows, cols, pixel_width: 0, pixel_height: 0 }
}

impl Term {
    pub(crate) fn spawn<E: From<Event> + Send + 'static>(opts: SpawnOptions, tx: Sender<E>) -> Result<Self> {
        let SpawnOptions { id, shell, args, env, rows, cols, cwd, theme } = opts;
        let pair = native_pty_system().openpty(pty_size(rows, cols)).map_err(|e| Error::OpenPty(e.into()))?;

        let mut cmd = CommandBuilder::new(shell);
        cmd.args(args);
        cmd.env("TERM", "xterm-256color");
        for key in agents::claude::CLAUDE_SESSION_ENV {
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
            Emulator::new(rows, cols, SCROLLBACK_BYTES, theme, Box::new(move |bytes| _ = write_to(&replies, bytes)))
                .map_err(|e| Error::Emulator(e.into()))?;
        spawn_reader(id, reader, tx);

        Ok(Self {
            id,
            emulator,
            agent: activity::Pane::default(),
            status_agent: None,
            context: context::Pane::default(),
            memory: memory::Pane::default(),
            resume: None,
            resuming: None,
            output_at: Instant::now(),
            input_at: None,
            submitted: None,
            input_seen: None,
            job: None,
            master: pair.master,
            writer,
            child,
            size: (rows, cols),
            interactive: args.is_empty(),
        })
    }

    pub(crate) fn feed(&mut self, bytes: &[u8]) {
        self.output_at = Instant::now();
        let pid = self.foreground_pid();
        let reporting = if self.interactive && pid == self.shell_pid() {
            self.status_agent.as_ref().filter(|(pid, _)| process::alive(*pid)).map(|(pid, _)| *pid).or(pid)
        } else {
            pid
        };
        self.emulator.status_process(reporting);
        self.emulator.feed(bytes);
    }

    pub(crate) fn write(&mut self, bytes: &[u8]) -> bool {
        self.input_at = Some(Instant::now());
        write_to(&self.writer, bytes)
    }

    pub(crate) fn paste(&mut self, text: &str) -> bool {
        if self.emulator.bracketed_paste() {
            self.write(bracketed(text).as_bytes())
        } else {
            self.write(text.as_bytes())
        }
    }

    pub(crate) fn resize(&mut self, rows: u16, cols: u16) {
        if self.size == (rows, cols) || rows == 0 || cols == 0 {
            return;
        }
        self.size = (rows, cols);
        let _ = self.master.resize(pty_size(rows, cols));
        let _ = self.emulator.resize(rows, cols);
    }

    pub(crate) fn foreground_pid(&self) -> Option<i32> {
        self.master.process_group_leader().filter(|pid| process::alive(*pid)).or_else(|| self.shell_pid())
    }

    pub(crate) fn shell_pid(&self) -> Option<i32> {
        self.child.process_id().and_then(|pid| i32::try_from(pid).ok())
    }

    pub(crate) fn cwd(&self) -> Option<PathBuf> {
        process::cwd(self.foreground_pid()?)
    }

    pub(crate) fn program(&self, config: &Config) -> Option<String> {
        let pid = self.foreground_pid()?;
        let name = process::name(pid)?;
        Some(program(config, &name, &process::args(pid)))
    }

    pub(crate) fn current_job(&self, config: &Config) -> Option<Job> {
        if self.shell_in_foreground() {
            return None;
        }
        let pid = self.foreground_pid()?;
        let name = process::name(pid)?;
        Some(Job { pid, program: program(config, &name, &process::args(pid)) })
    }

    pub(crate) fn shell_in_foreground(&self) -> bool {
        let shell = self.shell_pid();
        self.interactive && shell.is_some() && self.foreground_pid() == shell
    }

    pub(crate) fn dialog(&self) -> bool {
        self.seen_agent_screen().is_some_and(|(agent, screen)| agent.input_box(|| screen) == Some(false))
    }

    pub(crate) fn survey(&self) -> bool {
        self.seen_agent_screen().is_some_and(|(agent, screen)| agent.survey(&screen))
    }

    fn seen_agent_screen(&self) -> Option<(Known, String)> {
        let (Some(agent), Some(seen)) = (self.agent.agent().and_then(Known::of), self.input_seen) else { return None };
        (self.foreground_pid() == Some(seen)).then(|| (agent, self.emulator.screen_text().unwrap_or_default()))
    }

    pub(crate) fn foreground_args(&self) -> Vec<String> {
        self.foreground_pid().map(process::args).unwrap_or_default()
    }

    pub(crate) fn exited(&mut self) -> bool {
        self.exit_status().is_some()
    }

    pub(crate) fn exit_status(&mut self) -> Option<String> {
        let status = self.child.try_wait().ok()??;
        Some(status.signal().map_or_else(|| status.exit_code().to_string(), str::to_owned))
    }

    pub(crate) fn kill(&mut self) {
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

pub(crate) fn bracketed(text: &str) -> String {
    format!("{PASTE_START}{}{PASTE_END}", text.replace(PASTE_END, ""))
}

pub(crate) fn program(config: &Config, name: &str, argv: &[String]) -> String {
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

fn spawn_reader<E: From<Event> + Send + 'static>(id: u64, mut reader: Box<dyn Read + Send>, tx: Sender<E>) {
    thread::spawn(move || {
        let mut buf = vec![0u8; READ_BUFFER];
        while let Ok(n @ 1..) = reader.read(&mut buf) {
            if tx.send(E::from(Event::Output(id, buf[..n].to_vec()))).is_err() {
                return;
            }
        }
        let _ = tx.send(E::from(Event::Exited(id)));
    });
}

#[cfg(test)]
mod tests;
