use rstest::rstest;

use super::*;

fn feed(status: &mut Status, bytes: &[u8]) -> usize {
    bytes.iter().filter(|&&byte| status.advance(byte)).count()
}

fn report(status: &mut Status, body: &str) {
    feed(status, format!("\x1b]7501;{body}\x1b\\").as_bytes());
}

mod reports {
    use super::*;

    #[rstest]
    #[case::idle("idle", Activity::Ready)]
    #[case::working("working", Activity::Working)]
    #[case::done("done", Activity::Done)]
    #[case::blocked("blocked", Activity::Waiting)]
    #[case::error("error", Activity::Failed)]
    fn a_report_sets_the_state(#[case] state: &str, #[case] activity: Activity) {
        let mut status = Status::default();
        report(&mut status, &format!("state={state}"));
        assert_eq!(status.activity(), Some(activity));
    }

    #[rstest]
    #[case::bel("\x07")]
    #[case::st("\x1b\\")]
    fn a_query_is_answered_without_changing_the_records(#[case] end: &str) {
        let mut status = Status::default();
        report(&mut status, "state=working");
        assert_eq!(feed(&mut status, format!("\x1b]7501;?{end}").as_bytes()), 1);
        assert_eq!(status.activity(), Some(Activity::Working));
    }

    #[rstest]
    #[case::whitespace(" state = working : app = codex ")]
    #[case::unknown_key("future=1:state=working")]
    #[case::malformed_pair("bad:State=idle:=idle:state=working")]
    #[case::bad_value("state=working:state=not valid")]
    #[case::last_value("state=idle:state=working")]
    #[case::optional_data("state=working:progress=30:msg=d29ya2luZw:title=dGFzaw==")]
    fn valid_report(#[case] body: &str) {
        let mut status = Status::default();
        report(&mut status, body);
        assert_eq!(status.activity(), Some(Activity::Working));
    }

    #[rstest]
    #[case::missing_state("app=codex")]
    #[case::unknown_state("state=failed")]
    #[case::last_state("state=idle:state=future")]
    #[case::bad_base64("state=idle:msg=A")]
    #[case::newline("state=idle:msg=Cg==")]
    #[case::escape("state=idle:title=Gw==")]
    #[case::c1_control("state=idle:msg=wpw=")]
    #[case::invalid_utf8("state=idle:title=/w==")]
    #[case::bad_duplicate_text("state=idle:msg=A:msg=dGV4dA==")]
    #[case::empty_id("state=idle:id=")]
    #[case::leading_slash("state=idle:id=/task")]
    #[case::trailing_slash("state=idle:id=task/")]
    #[case::empty_segment("state=idle:id=task//child")]
    #[case::bad_segment("state=idle:id=task,child")]
    fn an_invalid_report_leaves_the_previous_state(#[case] body: &str) {
        let mut status = Status::default();
        report(&mut status, "state=working");
        report(&mut status, body);
        assert_eq!(status.activity(), Some(Activity::Working));
    }

    #[rstest]
    #[case::long_key(format!("state=idle:{}=1", "k".repeat(17)))]
    #[case::long_app(format!("state=idle:app={}", "a".repeat(33)))]
    #[case::long_id(format!("state=idle:id={}", "a".repeat(129)))]
    #[case::long_segment(format!("state=idle:id={}", "a".repeat(33)))]
    #[case::deep_id(format!("state=idle:id={}", ["a"; 9].join("/")))]
    #[case::long_message(format!("state=idle:msg={}", STANDARD.encode("a".repeat(2049))))]
    #[case::long_title(format!("state=idle:title={}", STANDARD.encode("a".repeat(193))))]
    #[case::long_sequence(format!("state=idle:future={}", "a".repeat(4096)))]
    fn limits_discard_the_whole_report(#[case] body: String) {
        let mut status = Status::default();
        report(&mut status, "state=working");
        report(&mut status, &body);
        assert_eq!(status.activity(), Some(Activity::Working));
    }

    #[test]
    fn the_largest_message_and_title_are_accepted() {
        let mut status = Status::default();
        report(
            &mut status,
            &format!(
                "state=working:msg={}:title={}",
                STANDARD.encode("a".repeat(2048)),
                STANDARD.encode("a".repeat(192))
            ),
        );
        assert_eq!(status.activity(), Some(Activity::Working));
    }
}

mod records {
    use super::*;

    #[test]
    fn the_most_urgent_record_wins_and_clearing_a_parent_removes_its_children() {
        let mut status = Status::default();
        report(&mut status, "state=working:app=codex");
        report(&mut status, "state=done:id=task");
        report(&mut status, "state=error:id=task/fail");
        report(&mut status, "state=blocked:id=task/ask:kind=permission");
        assert_eq!(status.activity(), Some(Activity::Waiting));
        report(&mut status, "state=clear:id=task/ask");
        assert_eq!(status.activity(), Some(Activity::Failed));
        report(&mut status, "state=clear:id=task/fail");
        assert_eq!(status.activity(), Some(Activity::Done));
        report(&mut status, "state=clear:id=task");
        assert_eq!(status.activity(), Some(Activity::Working));
        report(&mut status, "state=clear");
        assert_eq!(status.activity(), None);
    }

    #[test]
    fn clearing_a_path_does_not_clear_a_sibling_with_the_same_prefix() {
        let mut status = Status::default();
        report(&mut status, "state=working:id=task");
        report(&mut status, "state=blocked:id=task2");
        report(&mut status, "state=clear:id=task");
        assert_eq!(status.activity(), Some(Activity::Waiting));
    }

    #[test]
    fn the_oldest_updated_record_is_evicted_at_the_cap() {
        let mut status = Status::default();
        report(&mut status, "state=blocked:id=first");
        report(&mut status, "state=error:id=second");
        for id in 2..MAX_RECORDS {
            report(&mut status, &format!("state=working:id={id}"));
        }
        report(&mut status, "state=blocked:id=first");
        report(&mut status, "state=working:id=new");
        assert_eq!(status.records.len(), MAX_RECORDS);
        assert!(!status.records.iter().any(|record| record.id == "second"));
        assert_eq!(status.activity(), Some(Activity::Waiting));
    }
}

mod lifetimes {
    use super::*;

    #[rstest]
    #[case::alternate("\x1b[?1049h\x1b[?1049l", Some(Activity::Working))]
    #[case::soft_reset("\x1b[!p", Some(Activity::Working))]
    #[case::full_reset("\x1bc", None)]
    #[case::prompt("\x1b]133;A\x07", None)]
    fn records_have_terminal_lifetimes(#[case] bytes: &str, #[case] expected: Option<Activity>) {
        let mut status = Status::default();
        report(&mut status, "state=working");
        feed(&mut status, bytes.as_bytes());
        assert_eq!(status.activity(), expected);
    }

    #[rstest]
    #[case::done("done", Activity::Done)]
    #[case::error("error", Activity::Failed)]
    fn finished_records_survive_a_prompt_and_process_exit_but_do_not_follow_a_new_process(
        #[case] state: &str,
        #[case] activity: Activity,
    ) {
        let mut status = Status::default();
        status.process(Some(10));
        report(&mut status, &format!("state={state}"));
        feed(&mut status, b"\x1b]133;A\x07");
        assert_eq!(status.activity(), Some(activity));
        status.process(Some(11));
        assert_eq!(status.records.len(), 1);
        assert_eq!(status.activity(), None);
        report(&mut status, "state=working");
        assert_eq!(status.activity(), Some(Activity::Working));
    }

    #[rstest]
    #[case::working("working")]
    #[case::blocked("blocked")]
    fn active_records_end_with_the_process(#[case] state: &str) {
        let mut status = Status::default();
        status.process(Some(10));
        report(&mut status, &format!("state={state}"));
        status.process(Some(11));
        assert!(status.records.is_empty());
    }
}

mod control_strings {
    use super::*;

    #[rstest]
    #[case::dcs("\x1bP")]
    #[case::apc("\x1b_")]
    #[case::pm("\x1b^")]
    #[case::sos("\x1bX")]
    fn reports_inside_another_control_string_are_ignored(#[case] start: &str) {
        let mut status = Status::default();
        feed(&mut status, format!("{start}\x1b]7501;state=blocked\x07\x1b\\").as_bytes());
        assert_eq!(status.activity(), None);
        report(&mut status, "state=working");
        assert_eq!(status.activity(), Some(Activity::Working));
    }

    #[test]
    fn an_oversized_unterminated_string_keeps_a_bounded_buffer_and_recovers() {
        let mut status = Status::default();
        feed(&mut status, b"\x1b]7501;state=blocked:future=");
        feed(&mut status, &vec![b'a'; 100_000]);
        feed(&mut status, b"\x07");
        assert_eq!(status.activity(), None);
        report(&mut status, "state=working");
        assert_eq!(status.activity(), Some(Activity::Working));
    }

    #[rstest]
    #[case::cancel("\x18")]
    #[case::substitute("\x1a")]
    fn a_cancelled_sequence_never_sets_a_status(#[case] cancel: &str) {
        let mut status = Status::default();
        feed(&mut status, format!("\x1b]7501;state=blocked{cancel}\x07").as_bytes());
        assert_eq!(status.activity(), None);
    }
}
