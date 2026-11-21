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

fn agent() -> AgentRef {
    AgentRef { kind: String::from("claude"), pane: 4, workspace: 2, project: 1, boot: String::from("1-2") }
}

#[test]
fn the_new_kinds_write_their_fields_and_leave_out_the_rest() {
    let lines = [
        Line::started(1, 10, "▶ started claude in web", agent(), None),
        Line::finished(2, 11, "✓ claude finished in web", (agent(), String::from("done")), Some(7)),
        Line::waiting(3, 12, "! claude needs you in web", (agent(), String::from("ok?")), None),
    ];
    let dir = TempDir::new();
    for line in &lines {
        append(&chat(&dir), line).expect("append");
    }
    assert_eq!(read(&chat(&dir)), lines);
    let started = serde_json::to_string(&lines[0]).expect("json");
    assert!(
        started.contains(r#""kind":"started""#) && !started.contains("body") && !started.contains("run"),
        "{started}"
    );
    let finished = serde_json::to_string(&lines[1]).expect("json");
    assert!(finished.contains(r#""body":"done""#) && finished.contains(r#""run":7"#), "{finished}");
    let old = serde_json::to_string(&you(1, false)).expect("json");
    assert_eq!(old, r#"{"id":1,"at":10,"kind":"you","text":"hi"}"#);
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
    fn a_setup_notice_starts_a_turn_as_a_message_does() {
        let setup = Line::notice(1, 10, SETTING_UP);
        assert_eq!(state(std::slice::from_ref(&setup)), Replayed { next_id: 2, running: true, ..Replayed::default() });
        assert!(!state(&[setup.clone(), answer(2)]).running);
        let held = state(&[setup, Line::you(2, 11, "more", true)]);
        assert_eq!((held.running, held.held), (true, 1));
        let texts: Vec<String> = restart_notices(&held, 99).into_iter().map(|line| line.text).collect();
        assert_eq!(texts, ["interrupted by a restart", "1 held message dropped"]);
    }

    #[test]
    fn opens_turn_is_a_message_or_a_setup_and_never_a_runs_line() {
        assert!(opens_turn(&you(1, false)) && opens_turn(&you(1, true)));
        assert!(opens_turn(&Line::notice(1, 1, SETTING_UP)));
        assert!(!opens_turn(&Line::notice(1, 1, SETTING_UP).of_run(7)));
        assert!(!opens_turn(&Line::notice(1, 1, NEW_CONVERSATION)) && !opens_turn(&answer(1)));
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
    fn replay_reads_a_finished_line_as_a_turn_that_started() {
        let report = || Line::finished(2, 1, "✓ claude finished in web", (agent(), String::from("x")), None);
        assert!(state(&[report()]).running);
        assert!(!state(&[report(), Line { id: 3, ..answer(3) }]).running);
        let notices: Vec<String> = restart_notices(&state(&[report()]), 99).into_iter().map(|line| line.text).collect();
        assert_eq!(notices, ["interrupted by a restart"]);
        let held = Line { held: true, ..report() };
        assert_eq!(state(&[you(1, false), held]).held, 1);
    }

    #[test]
    fn started_and_waiting_lines_change_no_turn_state() {
        let lines = [
            you(1, false),
            Line::notice(2, 1, STOPPED),
            Line::started(3, 1, "▶ started claude in web", agent(), None),
            Line::waiting(4, 1, "! claude needs you in web", (agent(), String::from("?")), None),
            Line::notice(5, 1, "1 held message dropped"),
        ];
        assert_eq!(state(&lines), Replayed { next_id: 6, running: false, held: 0, ..Replayed::default() });
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
    fn the_answers_before_a_setup_are_not_unseen() {
        let lines =
            [head(1, 7), run_answer(2, 7), Line::notice(3, 1, SETTING_UP), answer(4), head(5, 8), run_answer(6, 8)];
        let told = unseen(&lines);
        assert_eq!(told.len(), 1);
        assert_eq!(told[0].text, "ok");
        assert_eq!(told[0].label, "Review PRs");
        assert_eq!(told[0].clock, "09:00");
        assert_eq!(unseen(&lines[..4]).len(), 0);
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

mod follow_ups {
    use super::*;

    fn head(run: u64) -> Line {
        Line::notice(1, 1, &run_head("Review PRs", "09:00")).of_run(run)
    }

    #[test]
    fn unseen_takes_a_run_follow_up_answer_and_stops_at_a_chat_report() {
        let follow_up = Line {
            run: Some(9),
            ..Line::answer(
                5,
                1,
                "The PR is merged.",
                Answer { model: None, tokens: Used::default(), conversation: None },
            )
        };
        let lines = [head(9), follow_up.clone()];
        let told = unseen(&lines);
        assert_eq!((told.len(), told[0].label.as_str(), told[0].text.as_str()), (1, "Review PRs", "The PR is merged."));
        let report = Line::finished(6, 1, "✓ claude finished in web", (agent(), String::from("x")), None);
        assert!(unseen(&[head(9), follow_up, report]).is_empty());
    }
}

#[test]
fn only_an_error_after_a_message_of_the_user_has_a_you_prompt() {
    let report = Line::finished(2, 1, "✓ claude finished in web", (agent(), String::from("x")), None);
    let error = |id| Line::failed(id, 1, "no");
    let kind = |lines: &[Line], id| prompt_of(lines, id).map(|line| line.kind);
    assert_eq!(kind(&[you(1, false), error(2)], 2), Some(Kind::You));
    assert_eq!(kind(&[you(1, false), report.clone(), error(3)], 3), Some(Kind::Finished));
    assert_eq!(kind(&[error(1)], 1), None);
}

mod preview {
    use rstest::rstest;

    use super::*;

    fn said(text: &str) -> Line {
        let answer = Answer { model: None, tokens: Used::default(), conversation: None };
        Line::answer(1, 10, text, answer)
    }

    #[rstest]
    #[case::list("- **Two** are blocked:\n- `#12` waits", "Two are blocked:")]
    #[case::heading("## Summary\n\nTwo are blocked.", "Summary")]
    #[case::fence("```sh\ngh pr list\n```", "gh pr list")]
    #[case::spaces("Two   are\tblocked.", "Two are blocked.")]
    #[case::html_block("<div>\nTwo are blocked.\n</div>", "Two are blocked.")]
    #[case::nothing("", "")]
    #[case::rule_only("---", "---")]
    #[case::reference_only("[a]: https://x.dev", "[a]: https://x.dev")]
    fn an_answer_previews_its_first_line_as_plain_text(#[case] text: &str, #[case] shown: &str) {
        assert_eq!(preview(&said(text)), shown);
    }

    #[test]
    fn an_error_keeps_its_text() {
        let line = Line::failed(1, 10, "there is no claude account `gone`");
        assert_eq!(preview(&line), "there is no claude account `gone`");
    }
}

mod asks {
    use super::*;

    fn notice(id: u64, text: &str, run: Option<u64>) -> Line {
        Line::notice(id, 30, text).asked(4).in_routine(run)
    }

    fn head(id: u64, run: u64) -> Line {
        Line::notice(id, 20, &run_head("Campus", "09:00")).of_run(run)
    }

    #[test]
    fn a_line_with_an_ask_round_trips_and_one_without_is_written_without_it() {
        let text = serde_json::to_string(&Line::notice(1, 1, "x")).expect("json");
        assert!(!text.contains("ask"), "{text}");
        let dir = TempDir::new();
        let line = notice(1, "asks for a secret: a", Some(7));
        append(&chat(&dir), &line).expect("append");
        assert_eq!(read(&chat(&dir)), [line]);
        assert!(std::fs::read_to_string(chat(&dir)).expect("read").contains(r#""ask":4"#));
    }

    #[test]
    fn replay_ignores_the_asks_and_the_not_given_notices() {
        let lines =
            [you(1, false), notice(2, "asks for a secret: a", None), answer(3), notice(4, "secret not given: a", None)];
        assert_eq!(replay(&lines), Replayed { next_id: 5, ..Replayed::default() });
    }

    #[test]
    fn an_ask_from_a_run_that_ended_leaves_the_run_closed() {
        let mut run_answer = answer(3);
        run_answer.routine = Some(7);
        let lines = [
            head(1, 7),
            notice(2, "asks for a secret: a", Some(7)),
            run_answer,
            notice(4, "secret not given: a", Some(7)),
        ];
        let state = replay(&lines);
        assert!(state.runs.is_empty(), "{state:?}");
        assert_eq!(restart_notices(&state, 99), Vec::<Line>::new());
    }

    #[test]
    fn a_saved_notice_of_a_chat_ask_opens_a_turn_and_counts_as_held() {
        let saved = notice(1, "secret saved: a", None);
        assert!(opens_turn(&saved) && tells_saved(&saved) && !retries(&saved));
        assert!(replay(std::slice::from_ref(&saved)).running);
        let held = Line { held: true, ..notice(3, "secret saved: a", None) };
        let state = replay(&[you(2, false), held]);
        assert_eq!((state.running, state.held), (true, 1));
        assert!(!opens_turn(&notice(1, "secret saved: a", Some(7))));
        assert!(!opens_turn(&Line::notice(1, 1, "secret saved: a")));
    }

    #[test]
    fn noted_tells_the_declined_and_the_routines_saved_ones_after_the_last_opener() {
        let lines = [
            notice(1, "secret not given: old", None),
            you(2, false),
            answer(3),
            notice(4, "asks for a secret: a", None),
            notice(5, "secret not given: a", None),
            notice(6, "secret saved: b", Some(7)),
        ];
        assert_eq!(noted(&lines), ["The user did not give: a.", "The user saved the secrets you asked for: b."]);
        let mut after = lines.to_vec();
        after.push(you(7, false));
        assert_eq!(noted(&after), Vec::<String>::new());
    }

    #[test]
    fn a_chat_asks_saved_notice_is_an_opener_so_nothing_before_it_is_noted() {
        let lines = [notice(1, "secret not given: a", None), notice(2, "secret saved: b", None)];
        assert_eq!(noted(&lines), Vec::<String>::new());
    }
}
