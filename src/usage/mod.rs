mod claude;
mod codex;

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::agents;
use crate::error::{Error, Result};
use crate::issues;
use crate::ui;

pub const TIMEOUT: Duration = Duration::from_secs(10);
const WARNING_FROM: u16 = 75;
const CRITICAL_FROM: u16 = 90;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Normal,
    Warning,
    Critical,
}

impl Severity {
    pub fn of(percent: u16) -> Self {
        if percent >= CRITICAL_FROM {
            Self::Critical
        } else if percent >= WARNING_FROM {
            Self::Warning
        } else {
            Self::Normal
        }
    }

    fn parse(text: Option<&str>, percent: u16) -> Self {
        match text {
            Some("normal") => Self::Normal,
            Some("warning") => Self::Warning,
            _ => Self::of(percent),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Window {
    pub label: String,
    pub percent: u16,
    pub severity: Severity,
    pub resets_at: Option<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    pub plan: Option<String>,
    pub limited: bool,
    pub windows: Vec<Window>,
    pub extra: Option<String>,
    pub notice: Option<&'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Agent {
    Claude,
    Codex,
}

impl Agent {
    pub const ALL: [Self; 2] = [Self::Claude, Self::Codex];

    pub fn kind(self) -> &'static str {
        match self {
            Self::Claude => agents::CLAUDE,
            Self::Codex => agents::CODEX,
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::Claude => "Claude Code",
            Self::Codex => "Codex",
        }
    }

    fn no_limits(self) -> &'static str {
        match self {
            Self::Claude => "no plan limits for this account (API key, Bedrock or Vertex)",
            Self::Codex => "no plan limits for this account",
        }
    }

    fn index(self) -> usize {
        match self {
            Self::Claude => 0,
            Self::Codex => 1,
        }
    }
}

#[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "clamped to 0..=999 first")]
fn whole(percent: Option<f64>) -> u16 {
    percent.unwrap_or(0.0).clamp(0.0, 999.0).round() as u16
}

fn window(label: String, percent: Option<f64>, severity: Option<&str>, resets_at: Option<i64>) -> Window {
    let percent = whole(percent);
    Window { label, percent, severity: Severity::parse(severity, percent), resets_at }
}

type Answer = fn(&str) -> Option<Result<Report>>;

fn read_answer(reader: impl BufRead, answer: Answer, name: &str) -> Result<Report> {
    for line in reader.lines() {
        if let Some(result) = answer(&line?) {
            return result;
        }
    }
    Err(Error::Usage(format!("{name} exited without answering")))
}

fn await_answer(
    written: Option<std::io::Result<()>>,
    stdout: Option<impl Read + Send + 'static>,
    answer: Answer,
    name: &'static str,
    timeout: Duration,
) -> Result<Report> {
    let (tx, rx) = mpsc::channel();
    if let (Some(Ok(())), Some(stdout)) = (written, stdout) {
        std::thread::spawn(move || {
            let _ = tx.send(read_answer(BufReader::new(stdout), answer, name));
        });
    } else {
        drop(tx);
    }
    match rx.recv_timeout(timeout) {
        Ok(result) => result,
        Err(mpsc::RecvTimeoutError::Timeout) => Err(Error::Usage(format!("{name} did not answer in time"))),
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(Error::Usage(format!("{name} exited without answering"))),
    }
}

fn run(agent: Agent, mut command: Command, requests: &str, answer: Answer, timeout: Duration) -> Result<Report> {
    let program = command.get_program().to_string_lossy().into_owned();
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| Error::Usage(format!("could not run {program}: {e}")))?;
    let mut stdin = child.stdin.take();
    let written = stdin.as_mut().map(|s| s.write_all(requests.as_bytes()).and_then(|()| s.flush()));
    let answer = await_answer(written, child.stdout.take(), answer, agent.kind(), timeout);
    let _ = child.kill();
    let _ = child.wait();
    drop(stdin);
    answer
}

pub fn probe(agent: Agent, program: &str, timeout: Duration) -> Result<Report> {
    match agent {
        Agent::Claude => run(agent, claude::command(program), claude::REQUESTS, claude::answer, timeout),
        Agent::Codex => run(agent, codex::command(program), &codex::requests(), codex::answer, timeout),
    }
}

fn until(secs: i64) -> String {
    let minutes = (secs.max(0) + 59) / 60;
    let (days, hours, minutes) = (minutes / 1_440, minutes / 60 % 24, minutes % 60);
    if days > 0 {
        format!("resets in {days}d {hours}h")
    } else if hours > 0 {
        format!("resets in {hours}h {minutes}m")
    } else {
        format!("resets in {minutes}m")
    }
}

fn updated(secs: u64) -> String {
    if secs < 60 {
        "updated just now".into()
    } else {
        format!("updated {} ago", issues::age(i64::try_from(secs).unwrap_or(i64::MAX)))
    }
}

#[derive(Debug, Default)]
struct Probe {
    last: Option<(Report, Instant)>,
    loading: bool,
    error: Option<String>,
}

impl Probe {
    fn start(&mut self) -> bool {
        if self.loading {
            return false;
        }
        self.loading = true;
        self.error = None;
        true
    }

    fn answered(&mut self, result: Result<Report>, now: Instant) {
        self.loading = false;
        match result {
            Ok(report) => self.last = Some((report, now)),
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    fn section(&self, agent: Agent, now: Instant, clock: i64) -> ui::UsageSection {
        let report = self.last.as_ref().map(|(r, _)| r);
        let plan = report.and_then(|r| r.plan.as_deref()).map_or_else(String::new, |p| format!(" · {p} plan"));
        let windows = report.map(|r| r.windows.as_slice()).unwrap_or_default();
        let status = if self.loading {
            "loading…".into()
        } else {
            self.last.as_ref().map(|(_, at)| updated(now.duration_since(*at).as_secs())).unwrap_or_default()
        };
        ui::UsageSection {
            title: format!("{}{plan}", agent.title()),
            status,
            error: self.error.as_ref().map(|e| format!("usage unavailable: {e}")),
            windows: windows
                .iter()
                .map(|w| ui::UsageWindow {
                    label: w.label.clone(),
                    percent: w.percent,
                    severity: w.severity,
                    resets: w.resets_at.map(|at| until(at - clock)).unwrap_or_default(),
                })
                .collect(),
            empty: report.and_then(|r| r.notice.or_else(|| (!r.limited).then(|| agent.no_limits()))),
            extra: report.and_then(|r| r.extra.clone()),
        }
    }
}

#[derive(Debug, Default)]
pub struct State {
    shown: Vec<Agent>,
    probes: [Probe; 2],
}

impl State {
    pub fn start(&mut self, shown: Vec<Agent>) -> Vec<Agent> {
        self.shown = shown;
        self.shown.iter().copied().filter(|agent| self.probes[agent.index()].start()).collect()
    }

    pub fn answered(&mut self, agent: Agent, result: Result<Report>, now: Instant) {
        self.probes[agent.index()].answered(result, now);
    }

    pub fn view(&self, now: Instant, clock: i64) -> ui::Usage {
        ui::Usage {
            sections: self.shown.iter().map(|&agent| self.probes[agent.index()].section(agent, now, clock)).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::test_util::{TempDir, write_executable};

    const CLOCK: i64 = 1_791_158_400;

    #[rstest]
    #[case::minutes(13 * 60, "resets in 13m")]
    #[case::rounds_up(30, "resets in 1m")]
    #[case::hours(2 * 3_600 + 13 * 60, "resets in 2h 13m")]
    #[case::days(3 * 86_400 + 4 * 3_600, "resets in 3d 4h")]
    #[case::past(-5, "resets in 0m")]
    fn reset_times_are_relative(#[case] secs: i64, #[case] expected: &str) {
        assert_eq!(until(secs), expected);
    }

    fn rows(section: &ui::UsageSection) -> Vec<(String, u16, String)> {
        section.windows.iter().map(|w| (w.label.clone(), w.percent, w.resets.clone())).collect()
    }

    #[test]
    fn the_view_shows_loading_then_the_windows_and_their_age() {
        let mut state = State::default();
        let now = Instant::now();
        assert_eq!(state.start(vec![Agent::Claude]), [Agent::Claude]);
        assert_eq!(state.start(vec![Agent::Claude]), []);
        let loading = state.view(now, CLOCK);
        assert_eq!((loading.sections[0].windows.len(), loading.sections[0].status.as_str()), (0, "loading…"));

        state.answered(Agent::Claude, Ok(claude::tests::report(claude::tests::RECORDED)), now);
        let clock = issues::parse_time("2026-10-04T11:46:59").expect("a time");
        let view = state.view(now + Duration::from_secs(120), clock);
        let section = &view.sections[0];
        assert_eq!(
            (section.title.as_str(), rows(section), section.status.as_str(), &section.error),
            (
                "Claude Code · team plan",
                vec![
                    ("session (5h)".into(), 7, "resets in 2h 13m".into()),
                    ("week".into(), 74, "resets in 4h 13m".into()),
                    ("week · Fable".into(), 0, "resets in 4h 14m".into()),
                ],
                "updated 2m ago",
                &None,
            )
        );
    }

    #[test]
    fn each_shown_agent_gets_its_own_section() {
        let mut state = State::default();
        let now = Instant::now();
        assert_eq!(state.start(Agent::ALL.to_vec()), Agent::ALL);
        state.answered(Agent::Codex, Ok(codex::tests::report(codex::tests::RECORDED)), now);
        let view = state.view(now, 1_791_218_970 - 3_600);
        let found: Vec<(&str, &str, usize)> =
            view.sections.iter().map(|s| (s.title.as_str(), s.status.as_str(), s.windows.len())).collect();
        assert_eq!(found, [("Claude Code", "loading…", 0), ("Codex · plus plan", "updated just now", 2)]);
        assert_eq!(rows(&view.sections[1])[0], ("session (5h)".into(), 53, "resets in 1h 0m".into()));
    }

    #[test]
    fn only_the_shown_agents_have_a_section() {
        let mut state = State::default();
        state.start(vec![Agent::Codex]);
        let titles: Vec<String> = state.view(Instant::now(), CLOCK).sections.into_iter().map(|s| s.title).collect();
        assert_eq!(titles, ["Codex"]);
    }

    #[test]
    fn a_failure_keeps_the_last_answer() {
        let mut state = State::default();
        let now = Instant::now();
        state.start(vec![Agent::Claude]);
        state.answered(Agent::Claude, Ok(claude::tests::report(claude::tests::RECORDED)), now);
        state.start(vec![Agent::Claude]);
        state.answered(Agent::Claude, Err(Error::Usage("claude did not answer in time".into())), now);
        let section = &state.view(now, CLOCK).sections[0];
        assert_eq!(
            (section.windows.len(), section.error.as_deref()),
            (3, Some("usage unavailable: claude did not answer in time"))
        );
    }

    #[rstest]
    #[case::claude(Agent::Claude, "no plan limits for this account (API key, Bedrock or Vertex)")]
    #[case::codex(Agent::Codex, "no plan limits for this account")]
    fn an_account_without_limits_says_so(#[case] agent: Agent, #[case] expected: &str) {
        let mut state = State::default();
        state.start(vec![agent]);
        state.answered(agent, Ok(Report::default()), Instant::now());
        assert_eq!(state.view(Instant::now(), CLOCK).sections[0].empty, Some(expected));
    }

    #[test]
    fn a_codex_without_a_login_says_how_to_sign_in_without_an_error() {
        let mut state = State::default();
        state.start(vec![Agent::Codex]);
        let signed_out = codex::tests::report(codex::tests::SIGNED_OUT);
        state.answered(Agent::Codex, Ok(signed_out), Instant::now());
        let section = &state.view(Instant::now(), CLOCK).sections[0];
        assert_eq!(
            (section.title.as_str(), section.empty, &section.error),
            ("Codex", Some("not signed in · run codex login"), &None)
        );
    }

    fn fake(name: &str, script: &str) -> (TempDir, String) {
        let dir = TempDir::new();
        let path = dir.path().join(name);
        write_executable(&path, &format!("#!/bin/sh\n{script}\n"));
        let command = path.display().to_string();
        (dir, command)
    }

    #[rstest]
    #[case::claude(Agent::Claude, 2, claude::tests::RECORDED, claude::tests::report)]
    #[case::codex(Agent::Codex, 3, codex::tests::RECORDED, codex::tests::report)]
    fn the_probe_answers_after_reading_the_requests(
        #[case] agent: Agent,
        #[case] requests: usize,
        #[case] recorded: &str,
        #[case] report: fn(&str) -> Report,
    ) {
        let reads = "read -r line\n".repeat(requests);
        let script = format!("{reads}echo '{{\"type\":\"system\"}}'\necho '{recorded}'\nexec sleep 30");
        let (_dir, command) = fake(agent.kind(), &script);
        assert_eq!(probe(agent, &command, TIMEOUT).expect("a report"), report(recorded));
    }

    #[test]
    fn the_codex_probe_runs_the_app_server() {
        let (_dir, command) = fake(
            "codex",
            "read -r a\nread -r b\nread -r c\necho \"{\\\"id\\\":2,\\\"error\\\":{\\\"message\\\":\\\"$1\\\"}}\"",
        );
        let error = probe(Agent::Codex, &command, TIMEOUT).expect_err("the arguments as an error");
        assert_eq!(error.to_string(), "app-server");
    }

    #[rstest]
    #[case::hangs(Agent::Claude, "exec sleep 30", Duration::from_millis(300), "claude did not answer in time")]
    #[case::exits(Agent::Claude, "exit 1", TIMEOUT, "claude exited without answering")]
    #[case::codex_hangs(Agent::Codex, "exec sleep 30", Duration::from_millis(300), "codex did not answer in time")]
    fn a_probe_that_gets_no_answer_fails(
        #[case] agent: Agent,
        #[case] script: &str,
        #[case] timeout: Duration,
        #[case] expected: &str,
    ) {
        let (_dir, command) = fake(agent.kind(), script);
        let error = probe(agent, &command, timeout).expect_err("no report");
        assert_eq!(error.to_string(), expected);
    }

    #[test]
    fn an_agent_gone_before_the_requests_fails_at_once() {
        let gone = Some(Err(std::io::ErrorKind::BrokenPipe.into()));
        let error = await_answer(gone, None::<&'static [u8]>, claude::answer, "claude", Duration::from_secs(2))
            .expect_err("no report");
        assert_eq!(error.to_string(), "claude exited without answering");
    }

    #[test]
    fn a_missing_agent_fails() {
        let error = probe(Agent::Codex, "/nonexistent/codex", TIMEOUT).expect_err("no report");
        assert!(error.to_string().starts_with("could not run /nonexistent/codex"), "{error}");
    }
}
