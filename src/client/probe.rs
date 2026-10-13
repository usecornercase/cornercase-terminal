use std::hash::{BuildHasher, RandomState};
use std::io::{Read, Write, stdin, stdout};
use std::os::fd::AsFd;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use rustix::event::{PollFd, PollFlags, Timespec, poll};

use super::server_process::stop;
use super::{Host, write_out};
use crate::graphics::detect::{self, Facts, Outer};
use crate::graphics::{self, Support};
use crate::host_theme::{HostTheme, ThemeProbe};
use crate::pointer;

fn truecolor() -> bool {
    std::env::var("COLORTERM").is_ok_and(|v| matches!(v.as_str(), "truecolor" | "24bit"))
}

pub(super) fn env(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

pub(super) fn probe_host() -> Host {
    let started = Instant::now();
    let tty = own_tty();
    let forced = detect::overridden(env);
    let asked_kitty = !forced && !detect::in_multiplexer(env) && detect::likely_kitty(env);
    let tmux_check = if forced { None } else { start_tmux_check() };
    let probe = query_terminal(HostTheme::query(asked_kitty).as_bytes(), THEME_QUERY_TIMEOUT);
    let round_trip = started.elapsed();
    let mut probe_timed_out = !probe.is_done();
    let (theme, replies) = probe.finish();
    let theme = HostTheme { truecolor: truecolor(), ..theme };
    let mut facts = Facts { replies, asked_kitty, window: window_cell(), tmux: None };
    if facts.renamed_tmux_pane() {
        write_out(detect::PANE_TITLE.as_bytes());
    }
    if !forced && detect::in_tmux(facts.replies.terminal.as_deref(), env) {
        let local = tmux_check.and_then(|child| finish_tmux_check(child, tty.as_deref()));
        facts.tmux = Some(match local {
            Some(Outer::Stale) if facts.replies.terminal.is_none() => Outer::Stale,
            Some(local @ Outer::Local { .. }) => local,
            _ => probe_outer(started, round_trip, &mut probe_timed_out),
        });
    } else if let Some(child) = tmux_check {
        stop(child);
    }
    let graphics = Support { id_hi: id_hi(), ..detect::decide(&facts, env) };
    let mut probe = format!("{} ms={}", facts.summary(), started.elapsed().as_millis());
    if let Some(value) = detect::ignored_override(env) {
        probe = format!("{probe} ignored-{}={value}", detect::OVERRIDE_ENV);
    }
    let pointer = pointer::detect(facts.replies.terminal.as_deref(), env);
    Host { theme, facts, graphics, probe, tty, probe_timed_out, pointer }
}

fn query_terminal(query: &[u8], timeout: Duration) -> ThemeProbe {
    let mut out = stdout();
    if out.write_all(query).and_then(|()| out.flush()).is_err() {
        return ThemeProbe::default();
    }
    listen(&stdin(), timeout)
}

fn listen(input: &impl AsFd, timeout: Duration) -> ThemeProbe {
    let mut probe = ThemeProbe::default();
    let deadline = Instant::now() + timeout;
    let mut buf = [0u8; 4096];
    while !probe.is_done() {
        if !readable(input, deadline.saturating_duration_since(Instant::now())) {
            break;
        }
        match rustix::io::read(input, &mut buf) {
            Ok(n @ 1..) => probe.feed(&buf[..n]),
            _ => break,
        }
    }
    if !probe.is_done() {
        drain(input);
    }
    probe
}

fn drain(input: &impl AsFd) {
    let limit = Instant::now() + DRAIN_LIMIT;
    let mut buf = [0u8; 4096];
    loop {
        let wait = DRAIN_QUIET.min(limit.saturating_duration_since(Instant::now()));
        if wait.is_zero() || !readable(input, wait) || !matches!(rustix::io::read(input, &mut buf), Ok(1..)) {
            return;
        }
    }
}

fn readable(input: &impl AsFd, wait: Duration) -> bool {
    let Ok(wait) = Timespec::try_from(wait) else { return false };
    let mut fds = [PollFd::new(input, PollFlags::IN)];
    matches!(poll(&mut fds, Some(&wait)), Ok(1..))
}

fn own_tty() -> Option<String> {
    rustix::termios::ttyname(stdin(), Vec::new()).ok().map(|name| name.to_string_lossy().into_owned())
}

pub(super) fn window_cell() -> Option<graphics::CellSize> {
    let size = crossterm::terminal::window_size().ok()?;
    detect::window_cell(size.columns, size.rows, size.width, size.height)
}

fn id_hi() -> u8 {
    let random = RandomState::new().hash_one((std::process::id(), std::time::SystemTime::now()));
    u8::try_from(random % 255).unwrap_or(0) + 1
}

pub(super) fn start_tmux_check() -> Option<Child> {
    env("TMUX").filter(|tmux| !tmux.is_empty())?;
    let mut command = Command::new("tmux");
    command.args(["display-message", "-p"]);
    if let Some(pane) = env("TMUX_PANE").filter(|pane| !pane.is_empty()) {
        command.args(["-t", &pane]);
    }
    command.arg(detect::TMUX_FORMAT).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().ok()
}

pub(super) fn finish_tmux_check(mut child: Child, tty: Option<&str>) -> Option<Outer> {
    let deadline = Instant::now() + TMUX_CHECK_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => {
                let mut output = String::new();
                child.stdout.take()?.read_to_string(&mut output).ok()?;
                return detect::local_tmux(&output, tty);
            }
            Ok(None) if Instant::now() < deadline => thread::sleep(TMUX_CHECK_POLL),
            Ok(None) => {
                stop(child);
                return None;
            }
            Ok(Some(_)) | Err(_) => return None,
        }
    }
}

fn probe_outer(started: Instant, round_trip: Duration, timed_out: &mut bool) -> Outer {
    thread::sleep(TMUX_SETTLE.saturating_sub(started.elapsed()));
    let wait = (round_trip * 2 + OUTER_WAIT).min(THEME_QUERY_TIMEOUT);
    let probe = query_terminal(detect::TMUX_PROBE.as_bytes(), wait);
    *timed_out |= !probe.is_done();
    Outer::Probed(probe.finish().1)
}

const THEME_QUERY_TIMEOUT: Duration = Duration::from_secs(1);
const TMUX_CHECK_TIMEOUT: Duration = Duration::from_millis(500);
const TMUX_CHECK_POLL: Duration = Duration::from_millis(5);
const TMUX_SETTLE: Duration = Duration::from_millis(150);
const OUTER_WAIT: Duration = Duration::from_millis(300);
const DRAIN_QUIET: Duration = Duration::from_millis(200);
const DRAIN_LIMIT: Duration = Duration::from_secs(1);

#[cfg(test)]
mod tests;
