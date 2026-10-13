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
    assert_eq!(step(&mut launch, t0 + 300 * MS, &shell()), Step::Write(b"claude --permission-mode plan\r".to_vec()));
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

#[test]
fn gemini_trust_waits_for_the_user_or_accepts_the_highlighted_folder() {
    let screen = crate::test_util::fixture!("gemini/0.63.0/trust.txt");
    assert_eq!(answer_keys(screen, false), b"\r");
    let (mut launch, t) = started(false);
    assert_eq!(step(&mut launch, t + AGENT_QUIET, &asking_you(screen)), Step::Wait);
    assert!(launch.waits_for_you());
    let (mut accepted, t) = started(false);
    assert_eq!(step(&mut accepted, t + AGENT_QUIET, &agent(screen)), Step::Write(b"\r".to_vec()));
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
