use ratatui::style::Color;

use super::*;

fn dim() -> Style {
    Style::default().fg(Color::DarkGray)
}

fn target(name: &str, why: Option<&str>) -> Target {
    Target { name: name.into(), icon: '☁', link: 1, why: why.map(str::to_string) }
}

fn run_on(why: Option<&str>) -> RunOn {
    RunOn { targets: vec![target("vps", None), target("box", why)], ..RunOn::default() }
}

fn drawn(spans: &[Span]) -> Vec<(String, bool)> {
    spans.iter().map(|span| (span.content.to_string(), span.style == dim())).collect()
}

fn text(spans: &[Span]) -> String {
    spans.iter().map(|span| span.content.as_ref()).collect()
}

mod choosing {
    use super::*;

    #[test]
    fn this_computer_is_chosen_until_a_machine_is() {
        let mut run_on = run_on(None);

        assert_eq!(run_on.machine(), None);
        assert!(run_on.choose(Some(1)));

        assert_eq!(run_on.machine().map(|target| target.name.as_str()), Some("box"));
        assert!(run_on.choose(None));
        assert_eq!(run_on.chosen, None);
    }

    #[test]
    fn a_machine_with_a_reason_cannot_be_chosen() {
        let mut run_on = run_on(Some("unreachable"));

        assert!(!run_on.choose(Some(1)));

        assert_eq!(run_on.chosen, None);
        assert!(!run_on.can_choose(1));
        assert!(run_on.can_choose(0));
    }

    #[test]
    fn no_machine_can_be_chosen_without_a_git_remote() {
        let mut run_on = RunOn { blocked: Some(NEEDS_REMOTE), ..run_on(None) };

        assert!(!run_on.choose(Some(0)));

        assert_eq!(run_on.chosen, None);
        assert!(run_on.choose(None));
    }

    #[test]
    fn a_place_past_the_machines_is_refused() {
        let mut run_on = run_on(None);

        assert!(!run_on.choose(Some(2)));

        assert_eq!(run_on.chosen, None);
    }
}

mod spans {
    use super::*;

    #[test]
    fn mark_this_computer_by_default() {
        let spans = run_on(None).spans(dim());

        assert_eq!(text(&spans), "● this computer   ○ ☁ vps   ○ ☁ box");
        assert!(drawn(&spans).iter().all(|(_, dimmed)| !dimmed));
    }

    #[test]
    fn mark_the_chosen_machine() {
        let run_on = RunOn { chosen: Some(0), ..run_on(None) };

        assert_eq!(text(&run_on.spans(dim())), "○ this computer   ● ☁ vps   ○ ☁ box");
    }

    #[test]
    fn dim_a_machine_that_is_not_linked_with_its_reason() {
        let spans = run_on(Some("unreachable")).spans(dim());

        assert_eq!(
            drawn(&spans),
            [
                ("● this computer".to_string(), false),
                ("   ○ ☁ vps".to_string(), false),
                ("   ○ ☁ box · unreachable".to_string(), true),
            ]
        );
    }

    #[test]
    fn dim_every_machine_and_say_why_without_a_git_remote() {
        let spans = RunOn { blocked: Some(NEEDS_REMOTE), ..run_on(Some("unreachable")) }.spans(dim());

        assert_eq!(
            drawn(&spans),
            [
                ("● this computer".to_string(), false),
                ("   ○ ☁ vps".to_string(), true),
                ("   ○ ☁ box".to_string(), true),
                (" — needs a git remote".to_string(), true),
            ]
        );
    }
}

mod its_lists {
    use super::*;

    fn items(count: usize) -> Vec<String> {
        (1..=count).map(|n| format!("file{n}")).collect()
    }

    #[rstest::rstest]
    #[case::few(3, "file1, file2, file3")]
    #[case::many(5, "file1, file2, file3 … and 2 more")]
    fn show_a_few_and_count_the_rest(#[case] count: usize, #[case] said: &str) {
        assert_eq!(capped(items(count).into_iter()).as_deref(), Some(said));
    }

    #[test]
    fn cut_each_long_text() {
        let said = capped(vec!["a".repeat(500), "b".into()].into_iter());

        assert_eq!(said, Some(format!("{}…, b", "a".repeat(59))));
    }

    #[test]
    fn leave_out_and_never_count_the_empty_ones() {
        let mut all = ["", " ", "\t"].map(String::from).to_vec();
        all.extend(items(4));
        let said = capped(all.into_iter());

        assert_eq!(said.as_deref(), Some("file1, file2, file3 … and 1 more"));
    }

    #[test]
    fn say_nothing_when_every_one_is_blank() {
        assert_eq!(capped(["", " ", "\u{1b}\t"].into_iter()), None);
    }

    struct Read<'a>(&'a std::cell::Cell<usize>, String);

    impl AsRef<str> for Read<'_> {
        fn as_ref(&self) -> &str {
            self.0.set(self.0.get() + 1);
            &self.1
        }
    }

    #[test]
    fn read_each_once_and_cut_only_the_ones_shown() {
        let reads = std::cell::Cell::new(0);
        let all = items(1_000).into_iter().map(|item| Read(&reads, item));

        assert_eq!(capped(all).as_deref(), Some("file1, file2, file3 … and 997 more"));
        assert_eq!(reads.get(), 1_000 + 3);
    }

    #[rstest::rstest]
    #[case::empty("")]
    #[case::spaces("  \n ")]
    #[case::control("\u{1b}\u{7}")]
    fn say_the_machine_gave_no_reason_for_a_blank_one(#[case] text: &str) {
        assert_eq!(reason("vps", text), "vps gave no reason");
        assert_eq!(clipped_or(text, "the project"), "the project");
    }

    #[test]
    fn keep_a_reason_that_has_text() {
        assert_eq!(reason("vps", " disk full\n"), "disk full");
    }

    #[rstest::rstest]
    #[case::lines("no such\nfolder\r\n", "no such folder")]
    #[case::escapes("red \u{1b}[31mtext\u{7}", "red [31mtext")]
    #[case::tabs("\tindented\t", "indented")]
    fn keep_each_text_on_one_line_without_control_characters(#[case] text: &str, #[case] said: &str) {
        assert_eq!(clipped(text), said);
    }
}

mod a_chosen_machine {
    use super::*;

    fn on_vps() -> RunOn {
        RunOn { chosen: Some(0), ..run_on(None) }
    }

    fn linked(name: &str, link: u64) -> Target {
        Target { link, ..target(name, None) }
    }

    #[test]
    fn stays_chosen_while_it_is_listed() {
        let kept = RunOn::kept(Some(run_on(Some("unreachable"))), Some(&on_vps()));

        assert_eq!(kept.as_ref().and_then(RunOn::machine).map(|target| target.name.as_str()), Some("vps"));
        assert_eq!(kept.and_then(|kept| kept.lost), None);
    }

    #[rstest::rstest]
    #[case::removed(Some(RunOn { targets: vec![linked("box", 2)], ..RunOn::default() }), "vps is no longer listed")]
    #[case::the_last_one(None, "vps is no longer listed")]
    #[case::edited(
        Some(RunOn { targets: vec![linked("vps", 3)], ..RunOn::default() }),
        "vps changed in Settings; choose it again"
    )]
    fn that_is_gone_is_not_swapped_for_this_computer(#[case] fresh: Option<RunOn>, #[case] why: &str) {
        let kept = RunOn::kept(fresh, Some(&on_vps())).expect("the row");

        assert_eq!((kept.chosen, kept.lost.as_deref()), (None, Some(why)));
        assert!(text(&kept.spans(dim())).starts_with("○ this computer"), "{}", text(&kept.spans(dim())));
        assert!(text(&kept.spans(dim())).ends_with(&format!(" · {why}")));
    }

    #[test]
    fn that_is_gone_stays_gone_until_one_is_chosen() {
        let lost = RunOn::kept(None, Some(&on_vps()));
        let mut kept = RunOn::kept(None, lost.as_ref()).expect("the row");

        assert!(!kept.is_chosen(None));
        assert!(kept.choose(None));

        assert_eq!((kept.lost, RunOn::kept(None, Some(&RunOn::default()))), (None, None));
    }
}
