use super::*;

mod contain {
    use super::*;

    #[test]
    fn a_panic_drops_only_the_step_that_panicked() {
        let mut handled = Vec::new();
        for event in 1..=3 {
            contain(|| {
                assert_ne!(event, 2, "a bug");
                handled.push(event);
            });
        }
        assert_eq!(handled, [1, 3]);
    }

    #[test]
    fn gives_back_what_the_step_returns() {
        assert_eq!(contain(|| 7), Some(7));
    }
}

mod job {
    use super::*;

    #[test]
    fn a_panic_becomes_a_bug_to_show() {
        let result = job::<()>(|| panic!("a bug"));
        assert_eq!(result.map_err(|e| e.to_string()), Err("cornercase hit a bug, see server.log".into()));
    }

    #[test]
    fn keeps_the_answer_of_a_job_that_ends() {
        assert_eq!(job(|| Ok(7)).ok(), Some(7));
        assert!(matches!(job::<()>(|| Err(Error::Git("no upstream".into()))), Err(Error::Git(_))));
    }
}

mod reports {
    use super::*;

    fn report(reports: &mut Reports, location: &str) -> Option<String> {
        reports.report("main", location, "min > max", || "   0: cornercase::ui::draw\n".into())
    }

    #[test]
    fn the_first_panic_at_a_place_comes_with_its_backtrace() {
        assert_eq!(
            report(&mut Reports::default(), "src/ui.rs:12:5").as_deref(),
            Some("thread 'main' panicked at src/ui.rs:12:5:\nmin > max\n   0: cornercase::ui::draw")
        );
    }

    #[test]
    fn repeats_are_told_ever_more_rarely() {
        let mut reports = Reports::default();
        let told: Vec<usize> = (1..=20).filter(|_| report(&mut reports, "src/ui.rs:12:5").is_some()).collect();
        assert_eq!(told, [1, 2, 4, 8, 16]);
    }

    #[test]
    fn a_repeat_says_how_often_it_happened() {
        let mut reports = Reports::default();
        report(&mut reports, "src/ui.rs:12:5");
        assert_eq!(
            report(&mut reports, "src/ui.rs:12:5").as_deref(),
            Some("thread 'main' panicked at src/ui.rs:12:5 again, 2 times so far:\nmin > max")
        );
    }

    #[test]
    fn each_place_is_counted_on_its_own() {
        let mut reports = Reports::default();
        report(&mut reports, "src/ui.rs:12:5");
        let other = report(&mut reports, "src/app.rs:7:9");
        assert!(other.is_some_and(|text| text.contains("src/app.rs:7:9:\n")));
    }
}
