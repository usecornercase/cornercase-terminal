pub(crate) mod emulator;
pub(crate) mod keys;
pub(crate) mod memory;
pub(crate) mod mouse;
pub(crate) mod reader;
pub(crate) mod shell;
mod status;
pub(crate) mod typing;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Sender};
use std::thread;
use std::time::Instant;

use crossterm::event::KeyEvent;
use portable_pty::{Child, CommandBuilder, PtySize, native_pty_system};

use self::emulator::Emulator;
use self::typing::Typing;
use crate::accounts::{self, Accounts};
use crate::agents::activity;
use crate::agents::context;
use crate::agents::parent::{By, Parent, Running};
use crate::agents::{self, AgentState, Known};
use crate::config::Config;
use crate::error::{Error, Result};
use crate::git::worktree::setup;
use crate::host::theme::HostTheme;
use crate::os::process::launch_folder;
use crate::process;

const SCROLLBACK_BYTES: usize = 10_000_000;
const INTERPRETERS: [&str; 6] = ["node", "bun", "deno", "python", "python3", "ruby"];
const NODE_MAIN_THREAD: &str = "node-MainThread";
const MAIN_THREAD: &str = "MainThread";
const SCRIPT_EXTENSIONS: [&str; 9] = ["js", "mjs", "cjs", "ts", "mts", "cts", "py", "rb", "sh"];
const NOT_A_SCRIPT: [&str; 6] = ["-e", "--eval", "-p", "--print", "-c", "-m"];
const PASTE_START: &str = "\x1b[200~";
const PASTE_END: &str = "\x1b[201~";

pub(crate) const MAX_QUEUED: usize = 16 * 1024 * 1024;

type Started = (imp::Master, Box<dyn Child + Send + Sync>);

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
    pub(crate) exited_agent: Option<AgentState>,
    pub(crate) output_at: Instant,
    pub(crate) input_at: Option<Instant>,
    pub(crate) typing: Typing,
    pub(crate) submitted: Option<Instant>,
    pub(crate) input_seen: Option<i32>,
    pub(crate) job: Option<Job>,
    pub(crate) setup: Option<setup::Pane>,
    pub(crate) parent: Option<Parent>,
    pub(crate) agent_pid: Option<i32>,
    pub(crate) fold: Option<By>,
    generation: u32,
    accounts: Accounts,
    account: accounts::Seen,
    master: imp::Master,
    writer: Writer,
    child: Box<dyn Child + Send + Sync>,
    size: (u16, u16),
    interactive: bool,
}

#[derive(Debug)]
pub(crate) enum Event {
    Output(u64, u32, reader::Output),
    Exited(u64, u32),
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
    pub(crate) remove: &'a [&'a str],
    pub(crate) rows: u16,
    pub(crate) cols: u16,
    pub(crate) cwd: Option<PathBuf>,
    pub(crate) theme: &'a HostTheme,
}

struct Wired {
    id: u64,
    generation: u32,
    emulator: Emulator,
    master: imp::Master,
    writer: Writer,
    child: Box<dyn Child + Send + Sync>,
    size: (u16, u16),
    interactive: bool,
}

impl From<Wired> for Term {
    fn from(wired: Wired) -> Self {
        let Wired { id, generation, emulator, master, writer, child, size, interactive } = wired;
        Self {
            id,
            emulator,
            agent: activity::Pane::default(),
            status_agent: None,
            context: context::Pane::default(),
            memory: memory::Pane::default(),
            resume: None,
            resuming: None,
            exited_agent: None,
            output_at: Instant::now(),
            input_at: None,
            typing: Typing::default(),
            submitted: None,
            input_seen: None,
            job: None,
            setup: None,
            parent: None,
            agent_pid: None,
            fold: None,
            generation,
            accounts: Accounts::default(),
            account: accounts::Seen::default(),
            master,
            writer,
            child,
            size,
            interactive,
        }
    }
}

fn command(
    shell: &str,
    args: &[String],
    env: &[(String, String)],
    remove: &[&str],
    dir: Option<&Path>,
) -> CommandBuilder {
    let mut cmd = CommandBuilder::new(shell);
    imp::inherit(&mut cmd);
    cmd.args(args);
    cmd.env("TERM", "xterm-256color");
    for key in agents::claude::CLAUDE_SESSION_ENV.iter().chain(&agents::copilot::SESSION_ENV) {
        cmd.env_remove(key);
    }
    for (key, value) in env {
        cmd.env(key, value);
    }
    for key in remove {
        cmd.env_remove(key);
    }
    if let Some(dir) = dir {
        cmd.cwd(dir);
    }
    cmd
}

fn pty_size(rows: u16, cols: u16) -> PtySize {
    PtySize { rows, cols, pixel_width: 0, pixel_height: 0 }
}

impl Term {
    pub(crate) fn spawn<E: From<Event> + Send + 'static>(opts: SpawnOptions, tx: Sender<E>) -> Result<Self> {
        Self::spawn_as(opts, 0, tx)
    }

    pub(crate) fn spawn_as<E: From<Event> + Send + 'static>(
        opts: SpawnOptions,
        generation: u32,
        tx: Sender<E>,
    ) -> Result<Self> {
        let SpawnOptions { id, shell, args, env, remove, rows, cols, cwd, theme } = opts;
        let pair = native_pty_system().openpty(pty_size(rows, cols)).map_err(|e| Error::OpenPty(e.into()))?;
        let reader = pair.master.try_clone_reader().map_err(|e| Error::AttachPty(e.into()))?;
        let writer = spawn_writer(pair.master.take_writer().map_err(|e| Error::AttachPty(e.into()))?);

        let dir = cwd.or_else(launch_folder);
        let cmd = command(shell, args, env, remove, dir.as_deref());
        let (master, child) = imp::Master::start(pair, cmd, &writer).map_err(|cause| Error::SpawnShell {
            shell: shell.to_string(),
            dir: dir.unwrap_or_default(),
            cause,
        })?;

        let replies = writer.clone();
        let emulator =
            Emulator::new(rows, cols, SCROLLBACK_BYTES, theme, Box::new(move |bytes| _ = write_to(&replies, bytes)))
                .map_err(|e| Error::Emulator(e.into()))?;
        reader::spawn(id, generation, reader, tx);

        Ok(Wired { id, generation, emulator, master, writer, child, size: (rows, cols), interactive: args.is_empty() }
            .into())
    }

    pub(crate) fn generation(&self) -> u32 {
        self.generation
    }

    pub(crate) fn size(&self) -> (u16, u16) {
        self.size
    }

    pub(crate) fn accounts(&self) -> &Accounts {
        &self.accounts
    }

    pub(crate) fn account(&self) -> Option<&str> {
        self.account.name()
    }

    pub(crate) fn agent_account(&self) -> Option<(Known, Option<&str>)> {
        self.account.of()
    }

    pub(crate) fn see_account(
        &mut self,
        root: &Path,
        agent: Option<(i32, Known, &[String])>,
        env: impl FnOnce(i32) -> Vec<(String, String)>,
    ) {
        self.account.update(root, &self.accounts, agent, env);
    }

    pub(crate) fn set_accounts(&mut self, accounts: Accounts) {
        self.accounts = accounts;
    }

    pub(crate) fn feed(&mut self, bytes: &[u8]) {
        self.output_at = Instant::now();
        let pid = self.foreground_pid();
        let reporting = if self.interactive && imp::at_prompt(pid, self.shell_pid()) {
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
        let bytes = self.pasted(text);
        self.write(&bytes)
    }

    pub(crate) fn pasted(&self, text: &str) -> Vec<u8> {
        if self.emulator.bracketed_paste() { bracketed(text).into_bytes() } else { text.as_bytes().to_vec() }
    }

    pub(crate) fn queue_full(&self) -> bool {
        self.writer.queued.load(Ordering::Relaxed) >= MAX_QUEUED
    }

    pub(crate) fn set_theme(&mut self, theme: &HostTheme) {
        if let Err(error) = self.emulator.set_theme(theme) {
            crate::log::warning!("app", "cannot apply the colours to a pane", pane = self.id, error = error);
        }
    }

    pub(crate) fn resize(&mut self, rows: u16, cols: u16) {
        if self.size == (rows, cols) || rows == 0 || cols == 0 {
            return;
        }
        self.size = (rows, cols);
        self.master.resize(pty_size(rows, cols));
        let _ = self.emulator.resize(rows, cols);
    }

    pub(crate) fn foreground_pid(&self) -> Option<i32> {
        imp::group_leader(&self.master, self.shell_pid())
            .filter(|pid| process::alive(*pid))
            .or_else(|| self.shell_pid())
    }

    pub(crate) fn running(&self) -> Running<'_> {
        Running {
            conversation: self.resume.as_ref(),
            read: self.context.agent().map(|(pid, _)| pid),
            agent: self.agent_pid,
            shell: self.shell_pid(),
        }
    }

    pub(crate) fn collapsed(&self) -> bool {
        self.fold.as_ref().is_some_and(|by| by.holds(self.running()))
    }

    pub(crate) fn set_collapsed(&mut self, collapsed: bool) {
        self.fold = collapsed.then(|| self.running().by()).flatten();
    }

    pub(crate) fn kept_fold(&self) -> bool {
        self.fold.as_ref().is_some_and(|by| by.kept(self.running()))
    }

    pub(crate) fn see_agent(&mut self, config: &Config) -> (Option<i32>, Vec<String>, Option<String>) {
        let pid = self.foreground_pid();
        let args = pid.map(process::args).unwrap_or_default();
        let name = agents::detect(config, &args);
        self.agent_pid = pid.filter(|_| name.is_some());
        (pid, args, name)
    }

    pub(crate) fn shell_pid(&self) -> Option<i32> {
        self.child.process_id().and_then(|pid| i32::try_from(pid).ok())
    }

    pub(crate) fn cwd(&self) -> Option<PathBuf> {
        let pid = self.foreground_pid()?;
        imp::reported_folder(pid, || self.emulator.pwd()).or_else(|| process::cwd(pid))
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
        self.interactive && shell.is_some() && imp::at_prompt(self.foreground_pid(), shell)
    }

    pub(crate) fn became_shell(&mut self) {
        self.interactive = true;
    }

    pub(crate) fn typed_key(&mut self, key: KeyEvent) {
        let pid = self.foreground_pid();
        self.typing.key(key, pid, Instant::now());
    }

    pub(crate) fn typed_text(&mut self, text: &str) {
        let pid = self.foreground_pid();
        self.typing.text(text, pid, Instant::now());
    }

    pub(crate) fn typed_paste(&mut self, text: &str) {
        let pid = self.foreground_pid();
        self.typing.paste(text, pid, Instant::now());
    }

    pub(crate) fn draft(&self, now: Instant) -> bool {
        self.typing.drafting() && self.typing.draft(self.foreground_pid(), now)
    }

    pub(crate) fn entered(&mut self, now: Instant) {
        self.submitted = Some(now);
        self.typing.clear();
    }

    pub(crate) fn seen_draft(&mut self) -> Option<bool> {
        let pid = self.foreground_pid();
        let seen =
            self.agent.agent().and_then(Known::of).filter(|_| self.input_seen.is_some() && self.input_seen == pid)?;
        let emulator = &mut self.emulator;
        seen.draft(|| emulator.active_snapshot().map(|s| s.lit_lines()).unwrap_or_default())
    }

    pub(crate) fn settle_draft(&mut self, now: Instant) {
        if !self.typing.drafting() {
            return;
        }
        let pid = self.foreground_pid();
        let empty = self.seen_draft() == Some(false);
        if empty || !self.typing.draft(pid, now) {
            self.typing.clear();
        }
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
            let pid = self.shell_pid();
            imp::end(self.child.as_mut(), pid);
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
    let node = name == NODE_MAIN_THREAD
        || (name == MAIN_THREAD
            && (argv.first().is_some_and(|a| Path::new(a).file_name().is_some_and(|n| n == "node"))
                || agents::detect(config, argv).is_some()
                || script(argv).is_some()));
    let name = if node { "node" } else { name };
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

#[cfg(unix)]
mod imp {
    use std::path::PathBuf;
    use std::time::Duration;

    use portable_pty::{Child, CommandBuilder, MasterPty, PtyPair, PtySize};

    use super::{Started, Writer};
    use crate::error::BoxError;
    use crate::os::process::{Pid, Signal, kill_process};

    const GRACE: Duration = Duration::from_millis(200);
    const FIRST_NAP: Duration = Duration::from_millis(1);
    const LONGEST_NAP: Duration = Duration::from_millis(50);

    pub(super) struct Master(Box<dyn MasterPty + Send>);

    impl Master {
        pub(super) fn start(pair: PtyPair, cmd: CommandBuilder, _replies: &Writer) -> Result<Started, BoxError> {
            let child = pair.slave.spawn_command(cmd)?;
            drop(pair.slave);
            Ok((Self(pair.master), child))
        }

        pub(super) fn resize(&self, size: PtySize) {
            let _ = self.0.resize(size);
        }
    }

    pub(super) fn group_leader(master: &Master, _shell: Option<i32>) -> Option<i32> {
        master.0.process_group_leader()
    }

    pub(super) fn end(child: &mut (dyn Child + Send + Sync), pid: Option<i32>) {
        let Some(pid) = pid.and_then(Pid::from_raw) else {
            let _ = child.kill();
            return;
        };
        let hung_up = kill_process(pid, Signal::HUP).is_ok();
        if !hung_up || !ends_within_grace(|| child.try_wait().is_ok_and(|status| status.is_some()), std::thread::sleep)
        {
            let _ = kill_process(pid, Signal::KILL);
            let _ = child.try_wait();
        }
    }

    pub(super) fn ends_within_grace(mut ended: impl FnMut() -> bool, mut nap: impl FnMut(Duration)) -> bool {
        let mut waited = Duration::ZERO;
        let mut next = FIRST_NAP;
        while !ended() {
            if waited >= GRACE {
                return false;
            }
            nap(next);
            waited += next;
            next = (next * 2).min(LONGEST_NAP);
        }
        true
    }

    pub(super) fn at_prompt(foreground: Option<i32>, shell: Option<i32>) -> bool {
        foreground == shell
    }

    pub(super) fn reported_folder(_foreground: i32, _reported: impl FnOnce() -> String) -> Option<PathBuf> {
        None
    }

    pub(super) fn inherit(_cmd: &mut CommandBuilder) {}
}

#[cfg(windows)]
mod imp {
    use std::os::windows::io::{AsRawHandle, BorrowedHandle, OwnedHandle};
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::thread;

    use parking_lot::Mutex;
    use portable_pty::{Child, CommandBuilder, MasterPty, PtyPair, PtySize};
    use windows_sys::Win32::System::Threading::{INFINITE, WaitForSingleObject};

    use super::{Started, Writer, write_to};
    use crate::error::BoxError;
    use crate::{process, text};

    const CURSOR_REPORT: &[u8] = b"\x1b[1;1R";
    const MAX_SHELLS: usize = 16;
    const FILE_URL: &str = "file://";

    type Shared = Arc<Mutex<Option<Box<dyn MasterPty + Send>>>>;

    pub(super) struct Master {
        shared: Shared,
        replies: Writer,
    }

    impl Master {
        pub(super) fn start(pair: PtyPair, cmd: CommandBuilder, replies: &Writer) -> Result<Started, BoxError> {
            let PtyPair { slave, master } = pair;
            let master = Self { shared: Arc::new(Mutex::new(Some(master))), replies: replies.clone() };
            let spawned = slave.spawn_command(cmd);
            drop(slave);
            let child = spawned?;
            if let Some(exit) = exit_handle(child.as_ref()) {
                let (shared, replies) = (Arc::clone(&master.shared), master.replies.clone());
                thread::spawn(move || close_after(&exit, &shared, &replies));
            }
            Ok((master, child))
        }

        pub(super) fn resize(&self, size: PtySize) {
            if let Some(master) = self.shared.lock().as_ref() {
                let _ = master.resize(size);
            }
        }
    }

    impl Drop for Master {
        fn drop(&mut self) {
            let (shared, replies) = (Arc::clone(&self.shared), self.replies.clone());
            thread::spawn(move || close(&shared, &replies));
        }
    }

    fn exit_handle(child: &(dyn Child + Send + Sync)) -> Option<OwnedHandle> {
        let raw = child.as_raw_handle()?;
        unsafe { BorrowedHandle::borrow_raw(raw) }.try_clone_to_owned().ok()
    }

    fn close_after(exit: &OwnedHandle, shared: &Shared, replies: &Writer) {
        unsafe { WaitForSingleObject(exit.as_raw_handle(), INFINITE) };
        close(shared, replies);
    }

    fn close(shared: &Shared, replies: &Writer) {
        let Some(master) = shared.lock().take() else { return };
        let _ = write_to(replies, CURSOR_REPORT);
        drop(master);
    }

    pub(super) fn end(child: &mut (dyn Child + Send + Sync), _pid: Option<i32>) {
        let _ = child.kill();
    }

    pub(super) fn group_leader(_master: &Master, shell: Option<i32>) -> Option<i32> {
        shell.map(foreground)
    }

    fn foreground(shell: i32) -> i32 {
        let mut at = shell;
        for _ in 0..MAX_SHELLS {
            let Some(child) = newest_child(at) else { break };
            if !is_shell(child) {
                return child;
            }
            at = child;
        }
        at
    }

    fn newest_child(pid: i32) -> Option<i32> {
        process::children(pid).into_iter().max_by_key(|child| process::start_stamp(*child))
    }

    fn is_shell(pid: i32) -> bool {
        process::name(pid).is_some_and(|name| process::is_shell(&name))
    }

    pub(super) fn at_prompt(foreground: Option<i32>, shell: Option<i32>) -> bool {
        foreground == shell || (shell.is_some() && foreground.is_some_and(is_shell))
    }

    pub(super) fn reported_folder(foreground: i32, reported: impl FnOnce() -> String) -> Option<PathBuf> {
        let powershell = process::name(foreground).is_some_and(|name| process::program::is_powershell(&name));
        powershell.then(|| folder_from(&reported())).flatten().filter(|folder| folder.is_dir())
    }

    pub(super) fn folder_from(reported: &str) -> Option<PathBuf> {
        let reported = reported.trim().trim_matches('"');
        let path = match reported.strip_prefix(FILE_URL) {
            Some(url) => text::percent_decoded(url.get(url.find('/')?..)?)?,
            None => reported.to_string(),
        };
        let path = PathBuf::from(without_slash_before_drive(&path));
        path.is_absolute().then_some(path)
    }

    fn without_slash_before_drive(path: &str) -> &str {
        match path.as_bytes() {
            [b'/', drive, b':', ..] if drive.is_ascii_alphabetic() => &path[1..],
            _ => path,
        }
    }

    pub(super) fn inherit(cmd: &mut CommandBuilder) {
        for (key, value) in std::env::vars_os() {
            cmd.env(key, value);
        }
    }
}

#[cfg(test)]
mod tests;
