use std::ffi::OsString;
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::Deserialize;

pub const CLAUDE_DIR_ENV: &str = "CLAUDE_CONFIG_DIR";
pub const CLAUDE_SESSION_ENV: [&str; 10] = [
    "CLAUDECODE",
    "CLAUDE_CODE_CHILD_SESSION",
    "CLAUDE_CODE_SESSION_ID",
    "CLAUDE_CODE_MESSAGING_SOCKET",
    "CLAUDE_CODE_MESSAGING_TOKEN",
    "CLAUDE_CODE_ENTRYPOINT",
    "CLAUDE_CODE_SESSION_ATTENDED",
    "CLAUDE_CODE_EXECPATH",
    "CLAUDE_PID",
    "CLAUDE_EFFORT",
];
const SPINNER: [char; 4] = ['◐', '◓', '◑', '◒'];
const BRAILLE: RangeInclusive<char> = '\u{2800}'..='\u{28ff}';
const IDLE: char = '✳';
const ACTION_REQUIRED: [&str; 2] = ["[ ! ] Action Required", "[ . ] Action Required"];
const NOTIFY_AFTER: Duration = Duration::from_secs(1);
pub const SHELL: &str = "shell";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Activity {
    Working,
    Shell,
    Waiting,
    Idle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Status {
    Idle,
    Working,
    Done,
    Waiting,
}

impl Status {
    pub fn needs_you(self) -> bool {
        matches!(self, Self::Done | Self::Waiting)
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Working => "working",
            Self::Done => "done",
            Self::Waiting => "waiting",
        }
    }
}

pub fn attention(statuses: impl IntoIterator<Item = Option<Status>>) -> Option<Status> {
    statuses.into_iter().flatten().filter(|s| s.needs_you()).max()
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Pane {
    agent: Option<String>,
    activity: Option<Activity>,
    unseen: bool,
    since: Option<Instant>,
    notified: bool,
    worked: Option<Instant>,
    moved: Option<Instant>,
}

impl Pane {
    pub fn follow(&mut self, agent: Option<&str>) {
        if self.agent.as_deref() != agent {
            *self = Self { agent: agent.map(str::to_string), ..Self::default() };
        }
    }

    pub fn update(&mut self, activity: Option<Activity>, seen: bool, now: Instant) -> Option<Status> {
        let before = self.status();
        let finished = matches!(self.activity, Some(Activity::Working | Activity::Shell | Activity::Waiting));
        if activity == Some(Activity::Working) {
            self.worked = Some(now);
        }
        if activity.is_some() && self.activity.is_some() && activity != self.activity {
            self.moved = Some(now);
        }
        self.unseen = activity == Some(Activity::Idle) && !seen && (self.unseen || finished);
        self.activity = activity;
        let status = self.status();
        if status != before {
            self.since = Some(now);
            self.notified = false;
        }
        self.notified |= seen;
        let settled = self.since.is_some_and(|since| now.duration_since(since) >= NOTIFY_AFTER);
        if !settled || self.notified || !status.is_some_and(Status::needs_you) {
            return None;
        }
        self.notified = true;
        status
    }

    pub fn see(&mut self) {
        self.unseen = false;
        self.notified = true;
    }

    pub fn agent(&self) -> Option<&str> {
        self.agent.as_deref()
    }

    pub fn reacted(&self, since: Instant) -> bool {
        [self.worked, self.moved].into_iter().flatten().any(|at| at >= since)
    }

    pub fn background_shell(&self) -> bool {
        self.activity == Some(Activity::Shell)
    }

    pub fn state(&self) -> Option<&'static str> {
        if self.background_shell() { Some(SHELL) } else { self.status().map(Status::name) }
    }

    pub fn status(&self) -> Option<Status> {
        Some(match self.activity? {
            Activity::Working | Activity::Shell => Status::Working,
            Activity::Waiting => Status::Waiting,
            Activity::Idle if self.unseen => Status::Done,
            Activity::Idle => Status::Idle,
        })
    }
}

#[derive(Debug, Deserialize)]
pub struct Session {
    pid: Option<i64>,
    status: Option<String>,
    #[serde(rename = "sessionId")]
    pub id: Option<String>,
    pub cwd: Option<PathBuf>,
}

impl Session {
    pub fn read(dir: Option<&Path>, pid: i32) -> Option<Self> {
        let text = std::fs::read_to_string(dir?.join("sessions").join(format!("{pid}.json"))).ok()?;
        Self::parse(&text, pid)
    }

    pub fn parse(text: &str, pid: i32) -> Option<Self> {
        let session: Self = serde_json::from_str(text).ok()?;
        session.pid.is_none_or(|p| p == i64::from(pid)).then_some(session)
    }

    fn activity(&self) -> Option<Activity> {
        match self.status.as_deref()? {
            "busy" => Some(Activity::Working),
            "shell" => Some(Activity::Shell),
            "waiting" => Some(Activity::Waiting),
            "idle" => Some(Activity::Idle),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub struct Claude {
    pub pid: i32,
    pub args: Vec<String>,
    pub session: Option<Session>,
}

impl Claude {
    pub fn activity(&self, title: &str) -> Activity {
        self.session.as_ref().and_then(Session::activity).or_else(|| from_title(title)).unwrap_or(Activity::Idle)
    }
}

pub fn claude_dir(home: Option<&Path>) -> Option<PathBuf> {
    claude_dir_from(std::env::var_os(CLAUDE_DIR_ENV), home)
}

fn claude_dir_from(env: Option<OsString>, home: Option<&Path>) -> Option<PathBuf> {
    match env {
        Some(dir) if !dir.is_empty() => Some(PathBuf::from(dir)),
        _ => home.map(|home| home.join(".claude")),
    }
}

pub fn codex(title: &str, turn: bool) -> Activity {
    if ACTION_REQUIRED.iter().any(|prefix| title.starts_with(prefix)) {
        Activity::Waiting
    } else if turn || glyph(title).is_some_and(|g| BRAILLE.contains(&g)) {
        Activity::Working
    } else {
        Activity::Idle
    }
}

pub fn opencode(turn: bool) -> Activity {
    if turn { Activity::Working } else { Activity::Idle }
}

fn glyph(title: &str) -> Option<char> {
    let mut chars = title.chars();
    let glyph = chars.next()?;
    (chars.next() == Some(' ')).then_some(glyph)
}

fn from_title(title: &str) -> Option<Activity> {
    let glyph = glyph(title)?;
    if SPINNER.contains(&glyph) || BRAILLE.contains(&glyph) {
        Some(Activity::Working)
    } else if glyph == IDLE {
        Some(Activity::Idle)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::test_util::TempDir;

    fn session(text: &str, pid: i32) -> Option<Activity> {
        Session::parse(text, pid).and_then(|s| s.activity())
    }

    fn claude(dir: Option<&Path>, pid: i32, title: &str) -> Activity {
        Claude { pid, args: Vec::new(), session: Session::read(dir, pid) }.activity(title)
    }

    mod session_file {
        use super::*;

        #[rstest]
        #[case::busy(r#"{"pid":7,"status":"busy"}"#, Some(Activity::Working))]
        #[case::permission(r#"{"pid":7,"status":"waiting","waitingFor":"permission prompt"}"#, Some(Activity::Waiting))]
        #[case::question(r#"{"pid":7,"status":"waiting","waitingFor":"input needed"}"#, Some(Activity::Waiting))]
        #[case::idle(r#"{"pid":7,"status":"idle"}"#, Some(Activity::Idle))]
        #[case::background_shell(r#"{"pid":7,"status":"shell"}"#, Some(Activity::Shell))]
        #[case::no_status_yet(r#"{"pid":7,"sessionId":"a"}"#, None)]
        #[case::unknown_status(r#"{"pid":7,"status":"dreaming"}"#, None)]
        #[case::another_process(r#"{"pid":8,"status":"busy"}"#, None)]
        #[case::half_written(r#"{"pid":7,"sta"#, None)]
        fn says_what_claude_is_doing(#[case] text: &str, #[case] expected: Option<Activity>) {
            assert_eq!(session(text, 7), expected);
        }

        #[test]
        fn names_the_conversation_and_where_it_started() {
            let text = r#"{"pid":7,"sessionId":"a01c","cwd":"/home/a/shop","status":"idle","version":"2.1.289"}"#;

            let found = Session::parse(text, 7).map(|s| (s.id, s.cwd));

            assert_eq!(found, Some((Some("a01c".into()), Some(PathBuf::from("/home/a/shop")))));
        }

        #[test]
        fn is_read_from_the_sessions_folder() {
            let dir = TempDir::new();
            std::fs::create_dir(dir.path().join("sessions")).expect("create sessions");
            std::fs::write(dir.path().join("sessions/42.json"), r#"{"pid":42,"status":"waiting"}"#).expect("write");

            assert_eq!(claude(Some(dir.path()), 42, "✳ Claude Code"), Activity::Waiting);
        }

        #[test]
        fn falls_back_to_the_title_without_one() {
            let dir = TempDir::new();

            assert_eq!(claude(Some(dir.path()), 42, "◑ fix the login"), Activity::Working);
        }

        #[test]
        fn counts_as_idle_without_a_file_or_a_title() {
            assert_eq!(claude(None, 42, ""), Activity::Idle);
        }
    }

    mod title {
        use super::*;

        #[rstest]
        #[case::half_circle("◐ fix the login", Some(Activity::Working))]
        #[case::other_half("◑ fix the login", Some(Activity::Working))]
        #[case::braille("⠂ fix the login", Some(Activity::Working))]
        #[case::idle("✳ Claude Code", Some(Activity::Idle))]
        #[case::shell_title("zsh", None)]
        #[case::glyph_without_space("◐x", None)]
        #[case::empty("", None)]
        fn shows_whether_claude_works(#[case] title: &str, #[case] expected: Option<Activity>) {
            assert_eq!(from_title(title), expected);
        }
    }

    mod codex_title {
        use super::*;

        #[rstest]
        #[case::starting("⠏ ⠏ | shop", false, Activity::Working)]
        #[case::working("⠴ Check example.com status | shop", false, Activity::Working)]
        #[case::approval("[ ! ] Action Required | Check example.com status | shop", true, Activity::Waiting)]
        #[case::approval_blinking("[ . ] Action Required | Check example.com status | shop", true, Activity::Waiting)]
        #[case::finished("Check example.com status | shop", false, Activity::Idle)]
        #[case::fresh("shop", false, Activity::Idle)]
        #[case::no_activity_in_the_title("shop", true, Activity::Working)]
        #[case::approval_without_a_turn("[ ! ] Action Required | shop", false, Activity::Waiting)]
        #[case::empty("", false, Activity::Idle)]
        fn shows_what_codex_is_doing(#[case] title: &str, #[case] turn: bool, #[case] expected: Activity) {
            assert_eq!(codex(title, turn), expected);
        }
    }

    mod pane {
        use super::*;

        fn after(steps: &[(Option<Activity>, bool)]) -> Option<Status> {
            let mut pane = Pane::default();
            let now = Instant::now();
            for (activity, seen) in steps {
                pane.update(*activity, *seen, now);
            }
            pane.status()
        }

        #[rstest]
        #[case::nothing_runs(&[(None, false)], None)]
        #[case::fresh_claude(&[(Some(Activity::Idle), false)], Some(Status::Idle))]
        #[case::working(&[(Some(Activity::Working), false)], Some(Status::Working))]
        #[case::asking(&[(Some(Activity::Working), false), (Some(Activity::Waiting), false)], Some(Status::Waiting))]
        #[case::finished_out_of_sight(&[(Some(Activity::Working), false), (Some(Activity::Idle), false)], Some(Status::Done))]
        #[case::finished_in_sight(&[(Some(Activity::Working), true), (Some(Activity::Idle), true)], Some(Status::Idle))]
        #[case::stays_done(
            &[(Some(Activity::Working), false), (Some(Activity::Idle), false), (Some(Activity::Idle), false)],
            Some(Status::Done)
        )]
        #[case::seen_later(
            &[(Some(Activity::Working), false), (Some(Activity::Idle), false), (Some(Activity::Idle), true)],
            Some(Status::Idle)
        )]
        #[case::works_again(
            &[(Some(Activity::Working), false), (Some(Activity::Idle), false), (Some(Activity::Working), false)],
            Some(Status::Working)
        )]
        #[case::answered_out_of_sight(&[(Some(Activity::Waiting), false), (Some(Activity::Idle), false)], Some(Status::Done))]
        #[case::exited(&[(Some(Activity::Working), false), (Some(Activity::Idle), false), (None, false)], None)]
        #[case::left_a_background_shell(&[(Some(Activity::Working), false), (Some(Activity::Shell), false)], Some(Status::Working))]
        #[case::its_background_shell_ended(
            &[(Some(Activity::Shell), false), (Some(Activity::Idle), false)],
            Some(Status::Done)
        )]
        fn follows_the_agent(#[case] steps: &[(Option<Activity>, bool)], #[case] expected: Option<Status>) {
            assert_eq!(after(steps), expected);
        }

        fn reacted(first: Option<Activity>, between: Option<Activity>, last: Option<Activity>) -> bool {
            let mut pane = Pane::default();
            let t0 = Instant::now();
            pane.update(first, false, t0);
            let sent = t0 + Duration::from_millis(500);
            pane.update(between, false, sent + Duration::from_millis(500));
            pane.update(last, false, sent + Duration::from_secs(1));
            pane.reacted(sent)
        }

        #[rstest]
        #[case::it_worked(Some(Activity::Idle), Some(Activity::Idle), Some(Activity::Working), true)]
        #[case::its_question_was_answered(Some(Activity::Waiting), Some(Activity::Waiting), Some(Activity::Idle), true)]
        #[case::it_stayed_idle(Some(Activity::Idle), Some(Activity::Idle), Some(Activity::Idle), false)]
        #[case::it_only_lost_and_found_its_agent(Some(Activity::Idle), None, Some(Activity::Idle), false)]
        #[case::its_background_shell_still_runs(
            Some(Activity::Shell),
            Some(Activity::Shell),
            Some(Activity::Shell),
            false
        )]
        #[case::it_took_the_prompt(Some(Activity::Shell), Some(Activity::Shell), Some(Activity::Working), true)]
        #[case::it_left_a_background_shell(Some(Activity::Idle), Some(Activity::Idle), Some(Activity::Shell), true)]
        fn reacting_is_working_or_changing_after_the_moment(
            #[case] first: Option<Activity>,
            #[case] between: Option<Activity>,
            #[case] last: Option<Activity>,
            #[case] expected: bool,
        ) {
            assert_eq!(reacted(first, between, last), expected);
        }

        #[rstest]
        #[case::nothing_runs(None, None)]
        #[case::working(Some(Activity::Working), Some("working"))]
        #[case::background_shell(Some(Activity::Shell), Some("shell"))]
        #[case::waiting(Some(Activity::Waiting), Some("waiting"))]
        #[case::idle(Some(Activity::Idle), Some("idle"))]
        fn tells_scripts_about_a_background_shell(#[case] activity: Option<Activity>, #[case] expected: Option<&str>) {
            let mut pane = Pane::default();
            pane.update(activity, true, Instant::now());

            assert_eq!(pane.state(), expected);
        }

        #[test]
        fn seeing_it_clears_done() {
            let mut pane = Pane::default();
            let now = Instant::now();
            pane.update(Some(Activity::Working), false, now);
            pane.update(Some(Activity::Idle), false, now);

            pane.see();

            assert_eq!(pane.status(), Some(Status::Idle));
        }
    }

    mod notice {
        use super::*;

        const WORKING: (Option<Activity>, bool) = (Some(Activity::Working), false);
        const WAITING: (Option<Activity>, bool) = (Some(Activity::Waiting), false);
        const IDLE: (Option<Activity>, bool) = (Some(Activity::Idle), false);
        const WAITING_IN_SIGHT: (Option<Activity>, bool) = (Some(Activity::Waiting), true);
        const TICK: Duration = Duration::from_millis(500);

        fn notices(steps: &[(Option<Activity>, bool)]) -> Vec<(usize, Status)> {
            let mut pane = Pane::default();
            let start = Instant::now();
            let mut now = start;
            let mut notices = Vec::new();
            for (i, (activity, seen)) in steps.iter().enumerate() {
                if let Some(status) = pane.update(*activity, *seen, now) {
                    notices.push((i, status));
                }
                now += TICK;
            }
            notices
        }

        #[rstest]
        #[case::asks_out_of_sight(&[WORKING, WAITING, WAITING, WAITING, WAITING], &[(3, Status::Waiting)])]
        #[case::finishes_out_of_sight(&[WORKING, IDLE, IDLE, IDLE, IDLE], &[(3, Status::Done)])]
        #[case::asks_in_sight(&[WORKING, WAITING_IN_SIGHT, WAITING_IN_SIGHT, WAITING_IN_SIGHT], &[])]
        #[case::flips_straight_back(&[WORKING, WAITING, WORKING, WORKING, WORKING], &[])]
        #[case::seen_before_it_settled(&[WORKING, WAITING_IN_SIGHT, WAITING, WAITING, WAITING], &[])]
        #[case::asks_then_finishes(&[WORKING, WAITING, WAITING, WAITING, IDLE, IDLE, IDLE], &[(3, Status::Waiting), (6, Status::Done)])]
        #[case::finishes_twice(&[WORKING, IDLE, IDLE, IDLE, WORKING, IDLE, IDLE, IDLE], &[(3, Status::Done), (7, Status::Done)])]
        #[case::only_works(&[WORKING, WORKING, WORKING, IDLE], &[])]
        fn comes_once_the_change_settles(
            #[case] steps: &[(Option<Activity>, bool)],
            #[case] expected: &[(usize, Status)],
        ) {
            assert_eq!(notices(steps), expected);
        }

        #[test]
        fn a_background_shell_finishes_once_it_ends() {
            let steps: Vec<_> = ["busy", "shell", "shell", "shell", "shell", "idle", "idle", "idle"]
                .iter()
                .map(|status| (session(&format!(r#"{{"pid":7,"status":"{status}"}}"#), 7), false))
                .collect();

            assert_eq!(notices(&steps), [(7, Status::Done)]);
        }

        #[test]
        fn another_agent_taking_over_the_pane_tells_you_again() {
            let mut pane = Pane::default();
            let start = Instant::now();
            pane.follow(Some("claude"));
            pane.update(Some(Activity::Waiting), false, start);
            let first = pane.update(Some(Activity::Waiting), false, start + NOTIFY_AFTER);

            pane.follow(Some("codex"));
            pane.update(Some(Activity::Waiting), false, start + NOTIFY_AFTER);
            let second = pane.update(Some(Activity::Waiting), false, start + NOTIFY_AFTER * 2);

            assert_eq!((first, second), (Some(Status::Waiting), Some(Status::Waiting)));
        }

        #[test]
        fn the_same_agent_keeps_its_state() {
            let mut pane = Pane::default();
            let start = Instant::now();
            pane.follow(Some("codex"));
            pane.update(Some(Activity::Waiting), false, start);
            pane.update(Some(Activity::Waiting), false, start + NOTIFY_AFTER);

            pane.follow(Some("codex"));

            assert_eq!(pane.update(Some(Activity::Waiting), false, start + NOTIFY_AFTER * 2), None);
        }

        #[test]
        fn none_comes_once_the_tab_was_seen() {
            let mut pane = Pane::default();
            let now = Instant::now();
            pane.update(Some(Activity::Working), false, now);
            pane.update(Some(Activity::Waiting), false, now);

            pane.see();

            assert_eq!(pane.update(Some(Activity::Waiting), false, now + NOTIFY_AFTER), None);
        }
    }

    mod rollup {
        use super::*;

        #[rstest]
        #[case::nothing(&[], None)]
        #[case::only_work(&[Some(Status::Working), Some(Status::Idle), None], None)]
        #[case::done(&[Some(Status::Working), Some(Status::Done)], Some(Status::Done))]
        #[case::waiting_wins(&[Some(Status::Done), Some(Status::Waiting), Some(Status::Working)], Some(Status::Waiting))]
        fn keeps_what_needs_you(#[case] statuses: &[Option<Status>], #[case] expected: Option<Status>) {
            assert_eq!(attention(statuses.iter().copied()), expected);
        }
    }

    mod config_dir {
        use super::*;

        #[rstest]
        #[case::default(None, Some("/home/a/.claude"))]
        #[case::from_the_environment(Some("/opt/claude"), Some("/opt/claude"))]
        #[case::empty_variable(Some(""), Some("/home/a/.claude"))]
        fn is_where_claude_keeps_its_sessions(#[case] env: Option<&str>, #[case] expected: Option<&str>) {
            let found = claude_dir_from(env.map(OsString::from), Some(Path::new("/home/a")));
            assert_eq!(found, expected.map(PathBuf::from));
        }
    }
}
