use super::*;
use crate::agents::context::Said;
use crate::agents::context::message::tests::Written;
use crate::test_util::fixture;

fn numbered(count: u64) -> Vec<Spoken> {
    (1..=count)
        .map(|n| Spoken {
            role: if n % 2 == 1 { Role::Prompt } else { Role::Reply },
            said: Said { text: format!("message {n}"), written: None },
        })
        .collect()
}

fn ids(page: &Page) -> Vec<u64> {
    page.messages.iter().map(|told| told.id).collect()
}

fn slice(last: Option<usize>, before: Option<u64>, after: Option<u64>) -> Slice {
    Slice { last, before, after }
}

mod paging {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::the_newest_twenty_by_default(slice(None, None, None), 31..=50, true)]
    #[case::last_keeps_the_newest_n(slice(Some(5), None, None), 46..=50, true)]
    #[case::last_over_the_whole_conversation_is_all_of_it(slice(Some(80), None, None), 1..=50, false)]
    #[case::before_gives_the_n_just_before_an_id(slice(Some(5), Some(41), None), 36..=40, true)]
    #[case::before_takes_twenty_by_default(slice(None, Some(41), None), 21..=40, true)]
    #[case::before_near_the_start_reaches_the_first(slice(Some(5), Some(4), None), 1..=3, false)]
    #[case::before_an_id_past_the_end_ends_at_the_newest(slice(Some(3), Some(90), None), 48..=50, true)]
    #[case::after_gives_everything_after_an_id(slice(None, None, Some(20)), 21..=50, true)]
    #[case::after_with_last_keeps_the_newest_n_of_them(slice(Some(4), None, Some(20)), 47..=50, true)]
    #[case::after_with_a_last_wider_than_what_follows(slice(Some(40), None, Some(45)), 46..=50, true)]
    #[case::after_zero_is_the_whole_conversation(slice(None, None, Some(0)), 1..=50, false)]
    fn a_slice_of_fifty_messages(
        #[case] slice: Slice,
        #[case] expected: std::ops::RangeInclusive<u64>,
        #[case] more_before: bool,
    ) {
        let page = paged(numbered(50), slice);

        assert_eq!(ids(&page), expected.collect::<Vec<_>>());
        assert_eq!(page.more_before, more_before);
    }

    #[rstest]
    #[case::before_the_first_message(slice(None, Some(1), None))]
    #[case::after_the_newest(slice(None, None, Some(50)))]
    #[case::after_past_the_end(slice(Some(3), None, Some(70)))]
    fn an_empty_page(#[case] slice: Slice) {
        let page = paged(numbered(50), slice);

        assert_eq!(ids(&page), Vec::<u64>::new());
        assert_eq!(page.more_before, slice.after.is_some());
    }

    #[test]
    fn a_short_conversation_comes_whole_without_more_before() {
        let page = paged(numbered(3), Slice::default());

        assert_eq!(ids(&page), [1, 2, 3]);
        assert!(!page.more_before);
    }

    #[test]
    fn an_empty_conversation_has_no_messages() {
        assert_eq!(paged(Vec::new(), Slice::default()), Page::default());
    }

    #[test]
    fn each_message_keeps_its_role_text_and_time() {
        let spoken = Spoken { role: Role::Reply, said: Said { text: "Done.".into(), written: Some("now".into()) } };

        let page = paged(vec![spoken], Slice::default());

        let told = Told { id: 1, role: Role::Reply, text: "Done.".into(), written: Some("now".into()) };
        assert_eq!(page.messages, [told]);
    }

    #[test]
    fn pages_walked_backwards_cover_every_message_once() {
        let mut seen = Vec::new();
        let mut page = paged(numbered(50), slice(Some(7), None, None));
        loop {
            seen.splice(0..0, ids(&page));
            let Some(first) = page.messages.first().filter(|_| page.more_before) else { break };
            page = paged(numbered(50), slice(Some(7), Some(first.id), None));
        }

        assert_eq!(seen, (1..=50).collect::<Vec<_>>());
    }

    #[test]
    fn roles_are_named_as_a_chat_names_them() {
        assert_eq!((Role::Prompt.speaker(), Role::Reply.speaker()), ("user", "assistant"));
    }
}

mod records {
    use serde_json::json;

    use super::*;
    use crate::agents::vibe::fake::FakeVibe;

    const THREAD: &str = fixture!("claude/2.1.332/thread.jsonl");

    fn said(page: &Page) -> Vec<(u64, Role, &str)> {
        page.messages.iter().map(|told| (told.id, told.role, told.text.as_str())).collect()
    }

    #[test]
    fn claudes_thread_leaves_tool_calls_out() {
        let file = Written::new(THREAD);

        let page = Record::Claude(file.path.clone()).page(Slice::default()).expect("read the transcript");

        assert_eq!(
            said(&page),
            [
                (1, Role::Prompt, "Add a login page"),
                (2, Role::Reply, "I will read the routes first."),
                (3, Role::Reply, "The page is in src/login.rs."),
                (4, Role::Prompt, "Add a logout button too"),
                (5, Role::Prompt, "And a test for both"),
                (6, Role::Reply, "Both are done, with a test."),
            ]
        );
        assert_eq!(page.messages[0].written.as_deref(), Some("2026-11-27T10:00:00.000Z"));
        assert!(!page.more_before);
    }

    #[test]
    fn ids_stay_when_the_record_grows() {
        let file = Written::new(THREAD);
        let record = Record::Claude(file.path.clone());
        let before = record.page(Slice::default()).expect("read the transcript");
        let last = before.messages.last().expect("a message").id;
        let grown = |line: &str| {
            let mut line: serde_json::Value = serde_json::from_str(line).expect("a line");
            line["message"]["content"] = json!("And the docs");
            format!("{line}\n")
        };
        file.append(&grown(THREAD.lines().next().expect("a prompt")));

        let after = record.page(slice(None, None, Some(last))).expect("read the transcript");

        assert_eq!(said(&after), [(7, Role::Prompt, "And the docs")]);
        assert!(after.more_before);
    }

    #[test]
    fn vibes_oldest_first_record_is_numbered_the_same_way() {
        let fake = FakeVibe::new();
        fake.answered();

        let page = Record::Vibe(fake.session()).page(slice(Some(2), None, None)).expect("read the journal");

        assert_eq!(said(&page), [(2, Role::Reply, "first"), (3, Role::Reply, "done")]);
        assert!(page.more_before);
    }

    #[test]
    fn a_codex_rollout_leaves_its_tool_call_out() {
        let user = fixture!("codex/0.160.0/user-message.jsonl");
        let reply = fixture!("codex/0.160.0/reply.jsonl");
        let file = Written::new(&format!("{user}{reply}"));

        let page = Record::Codex(file.path.clone()).page(Slice::default()).expect("read the rollout");

        assert_eq!(
            said(&page),
            [
                (1, Role::Prompt, "Add tests for the login form"),
                (2, Role::Reply, "Checking the form first."),
                (3, Role::Reply, "The login form now checks the password.\n\nTests pass."),
            ]
        );
    }
}
