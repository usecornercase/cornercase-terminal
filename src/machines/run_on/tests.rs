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
        assert_eq!(capped(items(count).into_iter()), said);
    }

    #[test]
    fn cut_each_long_text() {
        let said = capped(vec!["a".repeat(500), String::new()].into_iter());

        assert_eq!(said, format!("{}…, ", "a".repeat(59)));
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
