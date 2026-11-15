use super::follow_value;
use super::text::{agents, chat, event_text, list};
use crate::agents::headless::Used;
use crate::bots::chat::Line;
use crate::control::bots::{AgentInfo, BotInfo, BotLast};

const ZONE: &str = "UTC";
const NOON: i64 = 1_794_571_200;

fn line(id: u64, at: i64, make: impl FnOnce(u64, i64) -> Line) -> Line {
    make(id, at)
}

#[test]
fn a_chat_prints_who_said_what_with_the_steps_under_the_bot() {
    let answer = crate::bots::chat::Answer { model: None, tokens: Used::default(), conversation: None };
    let lines = [
        line(1, NOON - 600, |id, at| Line::you(id, at, "review my open PRs", false)),
        line(2, NOON - 590, |id, at| Line::step(id, at, "Bash", "cornercase status --json")),
        line(3, NOON - 580, |id, at| Line::answer(id, at, "Two are blocked.", answer)),
        line(4, NOON - 500, |id, at| Line::you(id, at, "and the drafts", true)),
        line(5, NOON - 400, |id, at| Line::failed(id, at, "Not logged in")),
        line(6, NOON - 300, |id, at| Line::notice(id, at, "new conversation")),
    ];
    let text = chat("PRs", &lines, NOON, ZONE);
    assert_eq!(
        text,
        "you · 11:50\nreview my open PRs\n\n\
         PRs · 11:50\n  ▸ cornercase status --json\nTwo are blocked.\n\n\
         you · 11:51 · held\nand the drafts\n\n\
         PRs · 11:53 · error\nNot logged in\n\n\
         — new conversation · 11:55 —\n"
    );
}

#[test]
fn the_list_has_a_row_per_bot() {
    let bot = BotInfo {
        id: "prs".into(),
        name: "PRs".into(),
        agent: "claude".into(),
        state: "thinking".into(),
        held: 1,
        unread: 2,
        last: Some(BotLast { kind: "bot".into(), text: "Two are\nblocked.".into(), at: NOON - 60 }),
        ..BotInfo::default()
    };
    let text = list(&[bot], NOON, ZONE);
    assert!(text.starts_with("prs  PRs  claude  thinking (+1 held)  2 unread  "), "{text}");
    assert!(text.trim_end().ends_with("11:59 · Two are blocked."), "{text}");
}

#[test]
fn the_agents_say_what_each_can_do() {
    let found = [
        AgentInfo {
            agent: "claude".into(),
            title: "Claude Code".into(),
            bot: true,
            accesses: vec!["cornercase".into(), "full".into()],
            ..AgentInfo::default()
        },
        AgentInfo {
            agent: "codex".into(),
            title: "Codex".into(),
            bot: true,
            accesses: vec!["full".into()],
            why: Some("its sandbox cannot reach cornercase".into()),
            ..AgentInfo::default()
        },
        AgentInfo {
            agent: "gemini".into(),
            title: "Gemini CLI".into(),
            why: Some("not yet".into()),
            ..AgentInfo::default()
        },
    ];
    assert_eq!(
        agents(&found),
        "claude    Claude Code  cornercase, full\n\
         codex     Codex        full  (its sandbox cannot reach cornercase)\n\
         gemini    Gemini CLI   not yet\n"
    );
}

#[test]
fn an_event_is_one_short_line() {
    let say = Line::you(1, 0, "line one\nline two", false);
    assert_eq!(event_text("prs", (Some(&say), None, None)), "bot prs  you: line one");
    let step = Line::step(2, 0, "Bash", "cornercase status --json");
    assert_eq!(event_text("prs", (Some(&step), None, None)), "bot prs  ▸ cornercase status --json");
    let answer = Line::answer(
        3,
        0,
        "Done.",
        crate::bots::chat::Answer { model: None, tokens: Used::default(), conversation: None },
    );
    assert_eq!(event_text("prs", (Some(&answer), None, None)), "bot prs  answered: Done.");
    assert_eq!(event_text("prs", (Some(&Line::failed(4, 0, "no")), None, None)), "bot prs  error: no");
    assert_eq!(event_text("prs", (Some(&Line::notice(5, 0, "stopped")), None, None)), "bot prs  notice: stopped");
    assert_eq!(event_text("prs", (None, Some("thinking"), None)), "bot prs  thinking");
    assert_eq!(event_text("prs", (None, None, Some("created"))), "bot prs  created");
}

#[test]
fn the_list_columns_follow_the_longest_id_and_rows_end_without_spaces() {
    let long = BotInfo {
        id: "a-very-long-bot-name-that-is".into(),
        name: "A very long name".into(),
        agent: "claude".into(),
        state: "idle".into(),
        unread: 100,
        ..BotInfo::default()
    };
    let short = BotInfo {
        id: "prs".into(),
        name: "PRs".into(),
        agent: "claude".into(),
        state: "idle".into(),
        ..BotInfo::default()
    };
    let text = list(&[long, short], NOON, ZONE);
    let rows: Vec<&str> = text.lines().collect();
    assert!(rows[0].starts_with("a-very-long-bot-name-that-is  A very long name  claude  idle  100 unread"), "{text}");
    assert!(rows[1].starts_with("prs                           PRs"), "{text}");
    assert!(rows.iter().all(|row| *row == row.trim_end()), "{text}");
}

#[test]
fn nothing_to_list_or_to_read_says_so() {
    assert_eq!(list(&[], NOON, ZONE), "no bots; cornercase bot new NAME makes one\n");
    assert_eq!(chat("PRs", &[], NOON, ZONE), "nothing said yet\n");
}

#[test]
fn an_event_of_a_routine_run_says_which_run() {
    let answer = crate::bots::chat::Answer { model: None, tokens: Used::default(), conversation: None };
    let line = Line::answer(9, NOON, "Two are blocked.", answer).of_run(12);
    assert_eq!(event_text("prs", (Some(&line), None, None)), "bot prs  routine 12  answered: Two are blocked.");
}

#[test]
fn a_chat_opens_a_header_for_each_run_and_says_its_routine() {
    let head = Line::notice(1, NOON - 600, "↻ Review PRs · 09:00").of_run(12);
    let step = Line::step(2, NOON - 590, "Bash", "gh pr list").of_run(12);
    let answer = crate::bots::chat::Answer { model: None, tokens: Used::default(), conversation: None };
    let said = Line::answer(3, NOON - 580, "Two are blocked.", answer.clone()).of_run(12);
    let chat_turn = Line::answer(4, NOON - 500, "Hi.", answer);
    let text = chat("PRs", &[head, step, said, chat_turn], NOON, ZONE);
    assert!(text.contains("PRs · 11:50 · routine 12\n  ▸ gh pr list\nTwo are blocked."), "{text}");
    assert!(text.contains("PRs · 11:51\nHi."), "{text}");
}

fn agent() -> crate::bots::chat::AgentRef {
    crate::bots::chat::AgentRef { kind: "claude".into(), pane: 4, workspace: 2, project: 1, boot: "1-2".into() }
}

#[test]
fn history_prints_started_finished_and_waiting() {
    let lines = [
        Line::started(1, NOON - 300, "▶ started claude in shop", agent(), None),
        Line::finished(2, NOON - 200, "✓ claude finished in shop", (agent(), "Fixed it.\nPushed.".into()), Some(9)),
        Line::waiting(3, NOON - 100, "! claude needs you in shop", (agent(), "Allow?".into()), None),
    ];
    let text = chat("PRs", &lines, NOON, ZONE);
    assert_eq!(
        text,
        "▶ started claude in shop · 11:55\n\n\
         ✓ claude finished in shop · 11:56 · routine 9\n  Fixed it.\n  Pushed.\n\n\
         ! claude needs you in shop · 11:58\n  Allow?\n"
    );
}

#[test]
fn event_text_names_the_new_kinds() {
    let started = Line::started(1, 0, "▶ started claude in shop", agent(), None);
    assert_eq!(event_text("prs", (Some(&started), None, None)), "bot prs  started: ▶ started claude in shop");
    let finished = Line::finished(2, 0, "✓ claude finished in shop", (agent(), "Done.".into()), None);
    assert_eq!(event_text("prs", (Some(&finished), None, None)), "bot prs  finished: ✓ claude finished in shop");
    let waiting = Line::waiting(3, 0, "! claude needs you in shop", (agent(), "?".into()), None);
    assert_eq!(event_text("prs", (Some(&waiting), None, None)), "bot prs  waiting: ! claude needs you in shop");
}

#[test]
fn edit_maps_follow_on_and_off() {
    use clap::Parser;

    use crate::cli::args::Cli;
    assert_eq!(
        (follow_value(Some("on")), follow_value(Some("off")), follow_value(None)),
        (Some(true), Some(false), None)
    );
    for given in ["on", "off"] {
        let cli = Cli::try_parse_from(["cornercase", "bot", "edit", "prs", "--follow", given]).expect("parses");
        assert!(format!("{cli:?}").contains(&format!("follow: Some(\"{given}\")")), "{cli:?}");
    }
    assert!(Cli::try_parse_from(["cornercase", "bot", "edit", "prs", "--follow", "maybe"]).is_err());
    assert!(Cli::try_parse_from(["cornercase", "bot", "new", "prs", "--follow", "off"]).is_ok());
}
