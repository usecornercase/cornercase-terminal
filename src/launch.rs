use std::time::{Duration, Instant};

use crate::issues::one_line;
use crate::term::bracketed;

const SHELL_QUIET: Duration = Duration::from_millis(300);
const SHELL_LATEST: Duration = Duration::from_secs(5);
const AGENT_QUIET: Duration = Duration::from_secs(1);
const AGENT_GONE: Duration = Duration::from_secs(3);
const AGENT_LATEST: Duration = Duration::from_secs(30);
const SUBMIT_QUIET: Duration = Duration::from_millis(300);
const SUBMIT_LATEST: Duration = Duration::from_secs(3);
const MAX_TRUSTS: u8 = 4;
const MARKERS: [&str; 5] = ["❯", "›", ">", "▸", "→"];
const MENU_REACH: usize = 6;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spec {
    pub command: String,
    pub prompt: Option<String>,
    pub submit: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    Shell,
    Agent,
    Submit,
}

#[derive(Debug)]
pub struct Launch {
    pub term: u64,
    pub key: Option<u64>,
    spec: Spec,
    agent: bool,
    stage: Stage,
    since: Instant,
    output: Option<Instant>,
    trusts: u8,
    trusted_screen: Option<String>,
    echo: String,
    undrawn: bool,
    asked: bool,
}

pub struct Seen<'a> {
    pub shell_in_foreground: bool,
    pub bracketed_paste: bool,
    pub application_cursor: bool,
    pub screen: &'a mut dyn FnMut() -> String,
    pub trust_prompt: &'a dyn Fn(&str) -> bool,
    pub trust: Trust,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trust {
    LeftToYou,
    Accepted,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Step {
    Wait,
    Write(Vec<u8>),
    Done(Vec<u8>),
    Abandon,
}

impl Launch {
    pub fn new(term: u64, spec: Spec, now: Instant) -> Self {
        Self {
            term,
            key: None,
            spec,
            agent: true,
            stage: Stage::Shell,
            since: now,
            output: None,
            trusts: 0,
            trusted_screen: None,
            echo: String::new(),
            undrawn: false,
            asked: false,
        }
    }

    pub fn command(term: u64, command: String, now: Instant) -> Self {
        Self { agent: false, ..Self::new(term, Spec { command, prompt: None, submit: false }, now) }
    }

    pub fn enter(term: u64, now: Instant) -> Self {
        Self { stage: Stage::Submit, ..Self::command(term, String::new(), now) }
    }

    pub fn output(&mut self, now: Instant) {
        if self.asked {
            self.since = now;
        }
        self.output = Some(now);
        self.undrawn = false;
        self.asked = false;
    }

    pub fn waits_for_you(&self) -> bool {
        self.asked
    }

    pub fn submits(&self) -> bool {
        self.stage == Stage::Submit
    }

    fn quiet(&self, now: Instant) -> Duration {
        now.duration_since(self.output.unwrap_or(self.since))
    }

    fn next(&mut self, stage: Stage, now: Instant) {
        self.stage = stage;
        self.since = now;
        self.output = None;
        self.undrawn = false;
    }

    pub fn step(&mut self, now: Instant, seen: &mut Seen) -> Step {
        match self.stage {
            Stage::Shell => {
                let ready = self.output.is_some_and(|at| now.duration_since(at) >= SHELL_QUIET)
                    || now.duration_since(self.since) >= SHELL_LATEST;
                if !ready {
                    return Step::Wait;
                }
                let typed = format!("{}\r", self.spec.command).into_bytes();
                if !self.agent {
                    return Step::Done(typed);
                }
                self.echo = compact(&(seen.screen)()) + &compact(&self.spec.command);
                self.next(Stage::Agent, now);
                Step::Write(typed)
            }
            Stage::Agent if seen.shell_in_foreground => {
                if now.duration_since(self.since) >= AGENT_GONE && self.quiet(now) >= AGENT_GONE {
                    Step::Abandon
                } else {
                    Step::Wait
                }
            }
            Stage::Agent if self.asked => Step::Wait,
            Stage::Agent => {
                let latest = now.duration_since(self.since) >= AGENT_LATEST;
                if (self.quiet(now) < AGENT_QUIET || self.undrawn) && !latest {
                    return Step::Wait;
                }
                let screen = (seen.screen)();
                if !latest && only_echo(&screen, &self.echo) {
                    self.undrawn = true;
                    return Step::Wait;
                }
                if (seen.trust_prompt)(&screen) {
                    if seen.trust == Trust::LeftToYou {
                        self.next(Stage::Agent, now);
                        self.asked = true;
                        return Step::Wait;
                    }
                    if self.trusts < MAX_TRUSTS && self.trusted_screen.as_ref() != Some(&screen) {
                        self.trusts += 1;
                        let keys = answer_keys(&screen, seen.application_cursor);
                        self.trusted_screen = Some(screen);
                        self.next(Stage::Agent, now);
                        return Step::Write(keys);
                    }
                }
                let Some(prompt) = &self.spec.prompt else { return Step::Done(Vec::new()) };
                let text = if self.spec.submit && seen.bracketed_paste { prompt.clone() } else { one_line(prompt) };
                let bytes = if seen.bracketed_paste { bracketed(&text) } else { text };
                if self.spec.submit {
                    self.next(Stage::Submit, now);
                    Step::Write(bytes.into_bytes())
                } else {
                    Step::Done(bytes.into_bytes())
                }
            }
            Stage::Submit => {
                if self.quiet(now) >= SUBMIT_QUIET || now.duration_since(self.since) >= SUBMIT_LATEST {
                    Step::Done(b"\r".to_vec())
                } else {
                    Step::Wait
                }
            }
        }
    }
}

fn compact(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

fn only_echo(screen: &str, echo: &str) -> bool {
    let mut echo = echo.chars();
    screen.chars().filter(|c| !c.is_whitespace()).all(|c| echo.next() == Some(c))
}

fn option_text(line: &str) -> &str {
    line.trim_start_matches(|c: char| c.is_whitespace() || "│┃║|".contains(c))
}

pub fn answer_keys(screen: &str, application_cursor: bool) -> Vec<u8> {
    let lines: Vec<&str> = screen.lines().map(option_text).collect();
    let Some(marked) = lines.iter().position(|l| MARKERS.iter().any(|m| l.starts_with(m))) else {
        return b"\r".to_vec();
    };
    let near = marked.saturating_sub(MENU_REACH)..(marked + MENU_REACH + 1).min(lines.len());
    let yes = near
        .filter(|&i| lines[i].to_lowercase().split(|c: char| !c.is_alphanumeric()).any(|word| word == "yes"))
        .min_by_key(|&i| i.abs_diff(marked));
    match yes {
        Some(target) if target != marked => {
            let key = match (target > marked, application_cursor) {
                (true, false) => "\x1b[B",
                (true, true) => "\x1bOB",
                (false, false) => "\x1b[A",
                (false, true) => "\x1bOA",
            };
            key.repeat(target.abs_diff(marked)).into_bytes()
        }
        _ => b"\r".to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    const MS: Duration = Duration::from_millis(1);

    fn spec(submit: bool) -> Spec {
        Spec { command: "claude --permission-mode plan".into(), prompt: Some("https://x.dev/7\nplease".into()), submit }
    }

    struct World {
        shell: bool,
        bracketed: bool,
        trust: Trust,
        screen: String,
    }

    fn step(launch: &mut Launch, now: Instant, world: &World) -> Step {
        let screen = world.screen.clone();
        let mut read = move || screen.clone();
        let trust = |s: &str| s.contains("trust");
        let mut seen = Seen {
            shell_in_foreground: world.shell,
            bracketed_paste: world.bracketed,
            application_cursor: false,
            screen: &mut read,
            trust_prompt: &trust,
            trust: world.trust,
        };
        launch.step(now, &mut seen)
    }

    fn shell() -> World {
        World { shell: true, bracketed: false, trust: Trust::Accepted, screen: "$ ".into() }
    }

    fn agent(screen: &str) -> World {
        World { shell: false, bracketed: true, trust: Trust::Accepted, screen: screen.into() }
    }

    fn asking_you(screen: &str) -> World {
        World { trust: Trust::LeftToYou, ..agent(screen) }
    }

    fn started(submit: bool) -> (Launch, Instant) {
        let t0 = Instant::now();
        let mut launch = Launch::new(1, spec(submit), t0);
        launch.output(t0);
        assert_eq!(
            step(&mut launch, t0 + 300 * MS, &shell()),
            Step::Write(b"claude --permission-mode plan\r".to_vec())
        );
        (launch, t0 + 300 * MS)
    }

    #[test]
    fn waits_for_the_shell_prompt_before_typing_the_agent() {
        let t0 = Instant::now();
        let mut launch = Launch::new(1, spec(false), t0);
        launch.output(t0);
        assert_eq!(step(&mut launch, t0 + 100 * MS, &shell()), Step::Wait);
    }

    #[test]
    fn types_the_agent_anyway_after_a_while() {
        let t0 = Instant::now();
        let mut launch = Launch::new(1, spec(false), t0);
        assert!(matches!(step(&mut launch, t0 + SHELL_LATEST, &shell()), Step::Write(_)));
    }

    #[test]
    fn waits_until_the_agent_has_been_quiet_for_a_second() {
        let (mut launch, t) = started(false);
        launch.output(t + 500 * MS);
        assert_eq!(step(&mut launch, t + 1200 * MS, &agent("> ")), Step::Wait);
    }

    #[rstest]
    #[case::the_typed_command("$ claude --permission-mode plan")]
    #[case::part_of_its_echo("$ claude --perm")]
    #[case::nothing("")]
    fn waits_while_the_screen_shows_only(#[case] screen: &str) {
        let (mut launch, t) = started(false);
        assert_eq!(step(&mut launch, t + AGENT_QUIET, &agent(screen)), Step::Wait);
    }

    #[test]
    fn answers_the_trust_question_once_the_agent_draws_it() {
        let (mut launch, t) = started(false);
        assert_eq!(step(&mut launch, t + AGENT_QUIET, &agent("$ claude --permission-mode plan")), Step::Wait);
        launch.output(t + 2 * AGENT_QUIET);
        let asked = agent("$ claude --permission-mode plan\nDo you trust the files?");
        assert_eq!(step(&mut launch, t + 3 * AGENT_QUIET, &asked), Step::Write(b"\r".to_vec()));
    }

    #[test]
    fn a_silent_agent_gets_the_prompt_after_the_longest_wait() {
        let (mut launch, t) = started(false);
        let silent = agent("$ claude --permission-mode plan");
        assert!(matches!(step(&mut launch, t + AGENT_LATEST, &silent), Step::Done(_)));
    }

    #[test]
    fn accepts_the_trust_prompt_with_enter() {
        let (mut launch, t) = started(false);
        assert_eq!(step(&mut launch, t + AGENT_QUIET, &agent("Do you trust the files?")), Step::Write(b"\r".to_vec()));
    }

    #[test]
    fn stops_accepting_after_a_few_tries() {
        let (mut launch, mut t) = started(false);
        for n in 0..MAX_TRUSTS {
            t += AGENT_QUIET;
            step(&mut launch, t, &agent(&format!("trust {n}")));
        }
        assert!(matches!(step(&mut launch, t + AGENT_QUIET, &agent("trust again")), Step::Done(_)));
    }

    const CLAUDE_TRUST: &str = "│ Quick safety check: Is this a project you created or one you trust?\n\
        │ Claude Code'll be able to read, edit, and execute files here.\n│\n\
        │ ❯ No, exit\n│   Yes, I trust this folder\n│\n│ Enter to confirm · Esc to cancel";

    #[test]
    fn a_menu_that_defaults_to_no_is_moved_to_yes_first() {
        assert_eq!(answer_keys(CLAUDE_TRUST, false), b"\x1b[B");
    }

    #[test]
    fn once_yes_is_marked_enter_answers() {
        let marked = CLAUDE_TRUST.replace("❯ No, exit", "  No, exit").replace("  Yes, I trust", "❯ Yes, I trust");
        assert_eq!(answer_keys(&marked, false), b"\r");
    }

    #[test]
    fn arrows_follow_the_application_cursor_mode() {
        let above = "  1. Yes, proceed\n> 2. No, exit";
        assert_eq!(answer_keys(above, true), b"\x1bOA");
    }

    #[test]
    fn without_a_menu_enter_answers() {
        assert_eq!(answer_keys("Do you trust the files in this folder? (press enter)", false), b"\r");
    }

    #[test]
    fn a_menu_is_answered_in_two_steps() {
        let (mut launch, t) = started(false);
        let first = step(&mut launch, t + AGENT_QUIET, &agent(CLAUDE_TRUST));
        let moved = CLAUDE_TRUST.replace("❯ No, exit", "  No, exit").replace("  Yes, I trust", "❯ Yes, I trust");
        let second = step(&mut launch, t + 2 * AGENT_QUIET, &agent(&moved));
        assert_eq!((first, second), (Step::Write(b"\x1b[B".to_vec()), Step::Write(b"\r".to_vec())));
    }

    #[test]
    fn a_screen_already_answered_is_not_answered_again() {
        let (mut launch, t) = started(false);
        step(&mut launch, t + AGENT_QUIET, &agent("Do you trust the files?"));
        assert!(matches!(step(&mut launch, t + 2 * AGENT_QUIET, &agent("Do you trust the files?")), Step::Done(_)));
    }

    #[test]
    fn leaves_the_trust_question_to_you_unless_told_to_accept_it() {
        let (mut launch, t) = started(false);
        assert_eq!(step(&mut launch, t + AGENT_QUIET, &asking_you("Do you trust the files?")), Step::Wait);
    }

    #[test]
    fn never_types_into_a_trust_question_left_to_you() {
        let (mut launch, t) = started(false);
        let asked = asking_you("Do you trust the files?");
        step(&mut launch, t + AGENT_QUIET, &asked);
        assert_eq!(step(&mut launch, t + AGENT_QUIET + AGENT_LATEST, &asked), Step::Wait);
    }

    #[test]
    fn a_launch_waiting_for_you_needs_no_polling_until_the_agent_prints() {
        let (mut launch, t) = started(false);
        step(&mut launch, t + AGENT_QUIET, &asking_you("Do you trust the files?"));
        let parked = launch.waits_for_you();

        launch.output(t + 2 * AGENT_QUIET);

        assert_eq!((parked, launch.waits_for_you()), (true, false));
    }

    #[test]
    fn an_answer_given_much_later_still_gets_its_quiet_second() {
        let (mut launch, t) = started(false);
        step(&mut launch, t + AGENT_QUIET, &asking_you("Do you trust the files?"));
        let answered = t + 100 * AGENT_LATEST;

        launch.output(answered);

        assert_eq!(step(&mut launch, answered + 100 * MS, &asking_you("> ")), Step::Wait);
    }

    #[test]
    fn pastes_the_prompt_once_you_have_answered_the_trust_question() {
        let (mut launch, t) = started(false);
        step(&mut launch, t + AGENT_QUIET, &asking_you("Do you trust the files?"));
        launch.output(t + 5 * AGENT_QUIET);
        assert!(matches!(step(&mut launch, t + 6 * AGENT_QUIET, &asking_you("> ")), Step::Done(_)));
    }

    #[test]
    fn pastes_the_prompt_on_one_line_and_leaves_it_there() {
        let (mut launch, t) = started(false);
        assert_eq!(
            step(&mut launch, t + AGENT_QUIET, &agent("> ")),
            Step::Done(b"\x1b[200~https://x.dev/7 please\x1b[201~".to_vec())
        );
    }

    #[test]
    fn without_bracketed_paste_the_prompt_is_typed() {
        let (mut launch, t) = started(false);
        let world = World { bracketed: false, ..agent("> ") };
        assert_eq!(step(&mut launch, t + AGENT_QUIET, &world), Step::Done(b"https://x.dev/7 please".to_vec()));
    }

    #[test]
    fn submit_pastes_as_written_then_presses_enter() {
        let (mut launch, t) = started(true);
        let pasted = step(&mut launch, t + AGENT_QUIET, &agent("> "));
        let enter = step(&mut launch, t + AGENT_QUIET + SUBMIT_QUIET, &agent("> "));
        assert_eq!(
            (pasted, enter),
            (Step::Write(b"\x1b[200~https://x.dev/7\nplease\x1b[201~".to_vec()), Step::Done(b"\r".to_vec()))
        );
    }

    #[test]
    fn gives_up_when_the_agent_never_leaves_the_shell() {
        let (mut launch, t) = started(false);
        assert_eq!(step(&mut launch, t + AGENT_GONE, &shell()), Step::Abandon);
    }

    #[test]
    fn a_command_is_typed_once_the_shell_is_quiet_and_that_is_all() {
        let t0 = Instant::now();
        let mut launch = Launch::command(1, "cargo test".into(), t0);
        launch.output(t0);

        let early = step(&mut launch, t0 + 100 * MS, &shell());
        let typed = step(&mut launch, t0 + SHELL_QUIET, &shell());

        assert_eq!((early, typed), (Step::Wait, Step::Done(b"cargo test\r".to_vec())));
    }

    #[test]
    fn an_agent_without_a_prompt_is_done_once_it_is_ready() {
        let t0 = Instant::now();
        let mut launch = Launch::new(1, Spec { prompt: None, ..spec(true) }, t0);
        launch.output(t0);
        step(&mut launch, t0 + SHELL_QUIET, &shell());

        assert_eq!(step(&mut launch, t0 + SHELL_QUIET + AGENT_QUIET, &agent("> ")), Step::Done(Vec::new()));
    }

    #[test]
    fn enter_waits_for_the_screen_to_settle() {
        let t0 = Instant::now();
        let mut launch = Launch::enter(1, t0);
        launch.output(t0 + 200 * MS);

        let early = step(&mut launch, t0 + 400 * MS, &agent("> fix it"));
        let pressed = step(&mut launch, t0 + 200 * MS + SUBMIT_QUIET, &agent("> fix it"));

        assert_eq!((early, pressed, launch.submits()), (Step::Wait, Step::Done(b"\r".to_vec()), true));
    }

    #[test]
    fn enter_is_pressed_anyway_while_the_screen_keeps_changing() {
        let t0 = Instant::now();
        let mut launch = Launch::enter(1, t0);
        launch.output(t0 + SUBMIT_LATEST);

        assert_eq!(step(&mut launch, t0 + SUBMIT_LATEST, &agent("⠋ thinking")), Step::Done(b"\r".to_vec()));
    }

    #[test]
    fn a_busy_agent_gets_the_prompt_after_the_longest_wait() {
        let (mut launch, t) = started(false);
        launch.output(t + AGENT_LATEST);
        assert!(matches!(step(&mut launch, t + AGENT_LATEST, &agent("⠋ thinking")), Step::Done(_)));
    }
}
