use super::text::{agents, chat, event_text, list, when};
use crate::agents::headless::Used;
use crate::bots::chat::Line;
use crate::control::bots::{AgentInfo, BotInfo, BotLast};

const ZONE: &str = "UTC";
const NOON: i64 = 1_794_571_200;

fn line(id: u64, at: i64, make: impl FnOnce(u64, i64) -> Line) -> Line {
    make(id, at)
}

#[test]
fn a_time_today_is_the_hour_and_another_day_names_the_day() {
    assert_eq!(when(NOON - 3_600, NOON, ZONE), "11:00");
    assert_eq!(when(NOON - 86_400 * 2, NOON, ZONE), "Wed 11 Nov 12:00");
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
    assert!(text.starts_with("prs             PRs                 claude  thinking (+1 held)  2 unread"), "{text}");
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
