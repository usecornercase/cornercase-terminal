use ratatui::style::Color;

use super::*;

fn dim() -> Style {
    Style::default().fg(Color::DarkGray)
}

fn target(name: &str, why: Option<&str>) -> Target {
    Target { name: name.into(), icon: '☁', link: 1, why: why.map(str::to_string) }
}

fn run_on(why: Option<&str>) -> RunOn {
    RunOn { targets: vec![target("vps", None), target("box", why)], chosen: None, blocked: None, awaiting: None }
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
