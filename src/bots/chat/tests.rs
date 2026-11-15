use std::path::PathBuf;

use super::*;
use crate::test_util::TempDir;

fn chat(dir: &TempDir) -> PathBuf {
    dir.path().join("chat.jsonl")
}

fn you(id: u64, held: bool) -> Line {
    Line::you(id, 10, "hi", held)
}

fn answer(id: u64) -> Line {
    let answer =
        Answer { model: Some(String::from("m")), tokens: Used::default(), conversation: Some(String::from("s")) };
    Line::answer(id, 11, "ok", answer)
}

#[test]
fn lines_are_read_back_as_they_were_written() {
    let dir = TempDir::new();
    let lines = [
        you(1, false),
        Line::step(2, 10, "Bash", "ls"),
        answer(3),
        Line::failed(4, 12, "no"),
        Line::notice(5, 13, STOPPED),
    ];
    for line in &lines {
        append(&chat(&dir), line).expect("append");
    }
    assert_eq!(read(&chat(&dir)), lines);
}

#[test]
fn only_what_is_set_is_written() {
    let text = serde_json::to_string(&you(1, false)).expect("json");
    assert_eq!(text, r#"{"id":1,"at":10,"kind":"you","text":"hi"}"#);
    let text = serde_json::to_string(&you(2, true)).expect("json");
    assert!(text.contains(r#""held":true"#), "{text}");
}

#[test]
fn a_torn_last_line_is_skipped_and_healed_before_the_next_one() {
    let dir = TempDir::new();
    append(&chat(&dir), &you(1, false)).expect("append");
    OpenOptions::new().append(true).open(chat(&dir)).expect("open").write_all(b"{\"id\":2,\"at\"").expect("tear");
    assert_eq!(read(&chat(&dir)).len(), 1);
    heal(&chat(&dir)).expect("heal");
    append(&chat(&dir), &answer(2)).expect("append");
    let lines = read(&chat(&dir));
    assert_eq!(lines.iter().map(|line| line.id).collect::<Vec<_>>(), [1, 2]);
}

#[test]
fn healing_leaves_a_whole_file_and_a_missing_one_alone() {
    let dir = TempDir::new();
    heal(&chat(&dir)).expect("nothing there");
    append(&chat(&dir), &you(1, false)).expect("append");
    let before = std::fs::read(chat(&dir)).expect("read");
    heal(&chat(&dir)).expect("heal");
    assert_eq!(std::fs::read(chat(&dir)).expect("read"), before);
}

mod replay {
    use super::*;

    fn state(lines: &[Line]) -> Replayed {
        replay(lines)
    }

    #[test]
    fn an_empty_chat_is_idle_and_starts_at_one() {
        assert_eq!(state(&[]), Replayed { next_id: 1, running: false, held: 0, ..Replayed::default() });
    }

    #[test]
    fn a_message_without_an_answer_is_a_turn_that_runs() {
        assert_eq!(
            state(&[you(1, false), Line::step(2, 1, "Bash", "ls")]),
            Replayed { next_id: 3, running: true, held: 0, ..Replayed::default() }
        );
    }

    #[test]
    fn an_answer_ends_it() {
        assert!(!state(&[you(1, false), answer(2)]).running);
        assert!(!state(&[you(1, false), Line::failed(2, 1, "x")]).running);
    }

    #[test]
    fn held_messages_are_counted_and_start_the_next_turn_after_an_answer() {
        let lines = [you(1, false), you(2, true), you(3, true)];
        assert_eq!(state(&lines), Replayed { next_id: 4, running: true, held: 2, ..Replayed::default() });
        let after = [you(1, false), you(2, true), answer(3)];
        assert_eq!(state(&after), Replayed { next_id: 4, running: true, held: 0, ..Replayed::default() });
    }

    #[test]
    fn a_stop_with_its_dropped_notice_or_a_restart_leaves_it_idle_with_nothing_held() {
        let stopped =
            [you(1, false), you(2, true), Line::notice(3, 1, STOPPED), Line::notice(4, 1, "1 held message dropped")];
        assert_eq!(state(&stopped), Replayed { next_id: 5, running: false, held: 0, ..Replayed::default() });
        let restarted = [you(1, false), you(2, true), Line::notice(3, 1, INTERRUPTED)];
        assert_eq!(state(&restarted), Replayed { next_id: 4, running: false, held: 0, ..Replayed::default() });
    }

    #[test]
    fn a_message_held_while_a_stop_was_in_flight_is_the_turn_that_runs_next() {
        let lines = [
            you(1, false),
            you(2, true),
            you(3, true),
            Line::notice(4, 1, STOPPED),
            Line::notice(5, 1, "1 held message dropped"),
        ];
        assert_eq!(state(&lines), Replayed { next_id: 6, running: true, held: 0, ..Replayed::default() });
    }

    fn after_a_stop_with_a_message_held_in_flight() -> Vec<Line> {
        vec![
            you(1, false),
            you(2, true),
            you(3, true),
            Line::notice(4, 1, STOPPED),
            Line::notice(5, 1, "1 held message dropped"),
        ]
    }

    #[test]
    fn the_turn_that_followed_a_stop_ends_with_its_answer_and_leaves_it_idle() {
        for end in [answer(6), Line::failed(6, 1, "no")] {
            let mut lines = after_a_stop_with_a_message_held_in_flight();
            lines.push(end);
            assert_eq!(state(&lines), Replayed { next_id: 7, running: false, held: 0, ..Replayed::default() });
        }
    }

    #[test]
    fn a_message_held_while_that_turn_runs_is_still_held() {
        let mut lines = after_a_stop_with_a_message_held_in_flight();
        lines.push(you(6, true));
        assert_eq!(state(&lines), Replayed { next_id: 7, running: true, held: 1, ..Replayed::default() });
        let notices: Vec<String> = restart_notices(&state(&lines), 99).into_iter().map(|line| line.text).collect();
        assert_eq!(notices, ["interrupted by a restart", "1 held message dropped"]);
    }

    #[test]
    fn other_notices_change_nothing() {
        let lines = [you(1, false), Line::notice(2, 1, NEW_CONVERSATION)];
        assert!(state(&lines).running);
    }
}

mod restart_notices {
    use super::*;

    fn texts(lines: &[Line]) -> Vec<String> {
        restart_notices(&replay(lines), 99).into_iter().map(|line| line.text).collect()
    }

    #[test]
    fn a_running_turn_is_interrupted_and_held_messages_are_dropped() {
        let lines = [you(1, false), you(2, true), you(3, true)];
        assert_eq!(texts(&lines), ["interrupted by a restart", "2 held messages dropped"]);
        assert_eq!(texts(&lines[..2]), ["interrupted by a restart", "1 held message dropped"]);
    }

    #[test]
    fn an_idle_chat_gets_nothing() {
        assert_eq!(texts(&[you(1, false), answer(2)]), Vec::<String>::new());
        assert_eq!(texts(&[]), Vec::<String>::new());
    }

    #[test]
    fn the_notices_take_the_next_ids() {
        let notices = restart_notices(&replay(&[you(4, false)]), 99);
        assert_eq!(notices.iter().map(|line| line.id).collect::<Vec<_>>(), [5]);
        assert_eq!(notices[0].at, 99);
    }
}

mod runs {
    use super::*;

    fn head(id: u64, run: u64) -> Line {
        Line::notice(id, 20, &run_head("Review PRs", "09:00")).of_run(run)
    }

    fn run_answer(id: u64, run: u64) -> Line {
        let mut line = answer(id);
        line.routine = Some(run);
        line
    }

    #[test]
    fn a_line_without_a_run_is_written_as_before_and_a_run_round_trips() {
        let text = serde_json::to_string(&you(1, false)).expect("json");
        assert!(!text.contains("routine"), "{text}");
        let dir = TempDir::new();
        let lines = [head(1, 12), Line::step(2, 21, "Bash", "gh pr list").of_run(12), run_answer(3, 12)];
        for line in &lines {
            append(&chat(&dir), line).expect("append");
        }
        assert_eq!(read(&chat(&dir)), lines);
        assert!(std::fs::read_to_string(chat(&dir)).expect("read").contains(r#""routine":12"#));
    }

    #[test]
    fn the_header_is_split_back_into_its_label_and_clock() {
        assert_eq!(head_parts("↻ Review PRs · 09:00"), Some(("Review PRs", "09:00")));
        assert_eq!(head_parts("↻ a · b · Mon 9 Nov 09:00"), Some(("a · b", "Mon 9 Nov 09:00")));
        assert_eq!(head_parts("stopped"), None);
    }

    #[test]
    fn replay_ignores_a_runs_lines() {
        let lines = [you(1, false), head(2, 7), run_answer(3, 7)];
        assert!(replay(&lines).running, "a run's answer does not end the chat's turn");
        let held =
            [you(1, false), you(2, true), Line::notice(3, 1, STOPPED), head(4, 7), Line::notice(5, 1, &dropped(1))];
        let state = replay(&held);
        assert_eq!((state.running, state.held), (false, 0));
    }

    #[test]
    fn an_unfinished_run_is_closed_once_with_a_restart_notice_of_its_own() {
        let lines = [head(1, 7), Line::step(2, 21, "Bash", "ls").of_run(7), head(3, 8), run_answer(4, 8)];
        let state = replay(&lines);
        assert_eq!(state.next_id, 5);
        let notices = restart_notices(&state, 99);
        assert_eq!(notices.len(), 1);
        assert_eq!((notices[0].text.as_str(), notices[0].routine, notices[0].id), (INTERRUPTED, Some(7), 5));
        let mut after = lines.to_vec();
        after.extend(notices);
        assert_eq!(restart_notices(&replay(&after), 100), Vec::<Line>::new());
    }

    #[test]
    fn only_the_answers_after_the_last_message_are_unseen_and_they_are_titled_by_their_header() {
        let lines =
            [head(1, 7), run_answer(2, 7), you(3, false), answer(4), head(5, 8), run_answer(6, 8), run_answer(7, 9)];
        let told = unseen(&lines);
        let titles: Vec<(&str, &str)> = told.iter().map(|one| (one.label.as_str(), one.clock.as_str())).collect();
        assert_eq!(titles, [("Review PRs", "09:00"), ("a routine", "")]);
    }
}
