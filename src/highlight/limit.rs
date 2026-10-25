use std::collections::HashSet;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::{self, Read, Write};
use std::os::unix::process::CommandExt;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use rustix::event::{PollFd, PollFlags, Timespec, poll};

use super::syntax::{self, Segments};
use super::worker;
use crate::process::{self, kill_group};

const PAINT_BUDGET: Duration = Duration::from_secs(5);

enum Fail {
    Timeout,
    Broken,
}

#[derive(Default)]
struct Session {
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    stdout: Option<ChildStdout>,
    failed: HashSet<u64>,
    last_pid: Option<i32>,
}

pub(super) fn paint(source: &str, language: &str) -> Option<Vec<Segments>> {
    if cfg!(test) {
        return syntax::parse(source, language);
    }
    let budget = process::time_left(PAINT_BUDGET).min(PAINT_BUDGET);
    session().lock().ask(source, language, budget, &mut spawn_worker)
}

fn session() -> &'static Mutex<Session> {
    static SESSION: OnceLock<Mutex<Session>> = OnceLock::new();
    SESSION.get_or_init(|| Mutex::new(Session::default()))
}

fn spawn_worker() -> io::Result<Child> {
    let exe = std::env::current_exe()?;
    Command::new(exe)
        .env("CORNERCASE_HIGHLIGHT_WORKER", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
}

impl Session {
    fn ask(
        &mut self,
        source: &str,
        language: &str,
        budget: Duration,
        spawn: &mut dyn FnMut() -> io::Result<Child>,
    ) -> Option<Vec<Segments>> {
        let key = hash_of(language, source);
        if self.failed.contains(&key) || budget.is_zero() {
            self.failed.insert(key);
            return None;
        }
        match self.exchange(language, source, budget, spawn) {
            Ok(lines) => lines,
            Err(Fail::Timeout) => {
                self.failed.insert(key);
                self.stop();
                None
            }
            Err(Fail::Broken) => {
                self.stop();
                None
            }
        }
    }

    fn exchange(
        &mut self,
        language: &str,
        source: &str,
        budget: Duration,
        spawn: &mut dyn FnMut() -> io::Result<Child>,
    ) -> Result<Option<Vec<Segments>>, Fail> {
        self.ensure(spawn)?;
        let deadline = Instant::now() + budget;
        write_request(self.stdin.as_mut().ok_or(Fail::Broken)?, language, source).map_err(|_| Fail::Broken)?;
        let stdout = self.stdout.as_mut().ok_or(Fail::Broken)?;
        read_response(stdout, deadline)
    }

    fn ensure(&mut self, spawn: &mut dyn FnMut() -> io::Result<Child>) -> Result<(), Fail> {
        if self.child.is_some() {
            return Ok(());
        }
        let mut child = spawn().map_err(|_| Fail::Broken)?;
        self.stdin = child.stdin.take();
        self.stdout = child.stdout.take();
        self.last_pid = i32::try_from(child.id()).ok();
        self.child = Some(child);
        Ok(())
    }

    fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            kill_group(&mut child);
        }
        self.stdin = None;
        self.stdout = None;
    }
}

fn hash_of(language: &str, source: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    (language, source).hash(&mut hasher);
    hasher.finish()
}

fn write_request(stdin: &mut ChildStdin, language: &str, source: &str) -> io::Result<()> {
    worker::write_string(stdin, language)?;
    worker::write_string(stdin, source)?;
    stdin.flush()
}

fn read_response(stdout: &mut ChildStdout, deadline: Instant) -> Result<Option<Vec<Segments>>, Fail> {
    let mut tag = [0];
    read_exact(stdout, &mut tag, deadline)?;
    if tag[0] == 0 {
        return Ok(None);
    }
    if tag[0] != 1 {
        return Err(Fail::Broken);
    }
    let count = read_u32(stdout, deadline)?;
    if count > worker::LINE_CAP {
        return Err(Fail::Broken);
    }
    let mut lines = Vec::with_capacity(count);
    for _ in 0..count {
        lines.push(read_line(stdout, deadline)?);
    }
    Ok(Some(lines))
}

fn read_line(stdout: &mut ChildStdout, deadline: Instant) -> Result<Segments, Fail> {
    let count = read_u32(stdout, deadline)?;
    if count > worker::SEGMENT_CAP {
        return Err(Fail::Broken);
    }
    let mut line = Vec::with_capacity(count);
    for _ in 0..count {
        line.push(read_segment(stdout, deadline)?);
    }
    Ok(line)
}

fn read_segment(stdout: &mut ChildStdout, deadline: Instant) -> Result<(String, ratatui::style::Style), Fail> {
    let mut bits = [0; 2];
    read_exact(stdout, &mut bits, deadline)?;
    let len = read_u32(stdout, deadline)?;
    if len > syntax::MAX_BYTES {
        return Err(Fail::Broken);
    }
    let mut text = vec![0; len];
    read_exact(stdout, &mut text, deadline)?;
    let text = String::from_utf8(text).map_err(|_| Fail::Broken)?;
    Ok((text, worker::unpack(u16::from_le_bytes(bits))))
}

fn read_u32(stdout: &mut ChildStdout, deadline: Instant) -> Result<usize, Fail> {
    let mut buf = [0; 4];
    read_exact(stdout, &mut buf, deadline)?;
    Ok(u32::from_le_bytes(buf) as usize)
}

fn read_exact(stdout: &mut ChildStdout, buf: &mut [u8], deadline: Instant) -> Result<(), Fail> {
    let mut got = 0;
    while got < buf.len() {
        wait_in(stdout, deadline)?;
        match stdout.read(&mut buf[got..]) {
            Ok(0) => return Err(Fail::Broken),
            Ok(n) => got += n,
            Err(err) if err.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => return Err(Fail::Broken),
        }
    }
    Ok(())
}

fn wait_in(stdout: &ChildStdout, deadline: Instant) -> Result<(), Fail> {
    let left = deadline.saturating_duration_since(Instant::now());
    if left.is_zero() {
        return Err(Fail::Timeout);
    }
    let timeout = Timespec::try_from(left).map_err(|_| Fail::Broken)?;
    let mut fds = [PollFd::new(stdout, PollFlags::IN)];
    match poll(&mut fds, Some(&timeout)) {
        Ok(_) => ready(&fds[0]),
        Err(err) if err.kind() == io::ErrorKind::Interrupted => Ok(()),
        Err(_) => Err(Fail::Broken),
    }
}

fn ready(fd: &PollFd<'_>) -> Result<(), Fail> {
    let flags = fd.revents();
    if flags.contains(PollFlags::IN) {
        return Ok(());
    }
    if flags.contains(PollFlags::ERR) || flags.contains(PollFlags::HUP) {
        return Err(Fail::Broken);
    }
    Err(Fail::Timeout)
}

#[cfg(test)]
mod tests;
