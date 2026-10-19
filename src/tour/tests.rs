use std::time::{Duration, SystemTime};

use rstest::rstest;

use super::*;
use crate::test_util::TempDir;

fn core() -> &'static Tour {
    find(CORE).expect("core").1
}

mod steps {
    use super::*;

    #[test]
    fn every_tour_is_short_and_has_unique_ids() {
        let mut ids: Vec<&str> = TOURS.iter().map(|t| t.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), TOURS.len());
        for tour in &TOURS {
            let mut steps: Vec<&str> = tour.steps.iter().map(|s| s.id).collect();
            steps.sort_unstable();
            steps.dedup();
            assert_eq!(steps.len(), tour.steps.len(), "{}", tour.id);
            if tour.id != CORE {
                assert!((3..=5).contains(&tour.steps.len()), "{}", tour.id);
            }
        }
    }

    #[test]
    fn the_core_tour_comes_first_in_its_order() {
        let ids: Vec<&str> = core().steps.iter().map(|s| s.id).collect();
        assert_eq!(ids, ["projects", "workspaces", "tabs", "prompt", "status", "settings", "next"]);
        assert_eq!(TOURS[0].id, CORE);
    }

    #[test]
    fn every_tour_links_its_docs() {
        for tour in &TOURS {
            assert!(tour.docs_url().starts_with(DOCS) && tour.docs.ends_with('/'), "{}", tour.id);
        }
    }

    #[test]
    fn texts_are_one_line_of_english() {
        for step in TOURS.iter().flat_map(|t| t.steps) {
            assert!(!step.text.contains('\n') && !step.text.contains("  "), "{}", step.id);
            assert!(step.text.len() < 400, "{}", step.id);
        }
    }
}

mod completion {
    use super::*;

    fn later(by: u64) -> Option<SystemTime> {
        Some(SystemTime::UNIX_EPOCH + Duration::from_secs(by))
    }

    #[rstest]
    #[case::a_project(Done::ProjectOpened, Facts { projects: 1, ..Facts::default() })]
    #[case::a_workspace(Done::WorkspaceMade, Facts { workspaces: 2, ..Facts::default() })]
    #[case::a_tab(Done::TabOpened, Facts { tabs: 1, ..Facts::default() })]
    #[case::a_split(Done::TabOpened, Facts { panes: 2, ..Facts::default() })]
    #[case::an_agent(Done::AgentStarted, Facts { agents: 1, ..Facts::default() })]
    #[case::a_prompt(Done::PromptConfirmed, Facts { prompted: later(5), ..Facts::default() })]
    #[case::a_panel(Done::Panel { panel: Panel::Todo }, Facts { panel: Some(Panel::Todo), ..Facts::default() })]
    #[case::a_dialog(Done::Opened { dialog: Dialog::Issues }, Facts { dialog: Some(Dialog::Issues), ..Facts::default() })]
    #[case::a_todo(Done::TodoAdded, Facts { todos: 1, ..Facts::default() })]
    #[case::the_section(Done::AgentsSection, Facts { agents_section: true, ..Facts::default() })]
    fn an_action_completes_its_step(#[case] done: Done, #[case] now: Facts) {
        assert!(!done.met(&Facts::default(), &Facts::default()));
        assert!(done.met(&Facts::default(), &now));
    }

    #[test]
    fn next_never_completes_by_itself() {
        let now = Facts { projects: 3, tabs: 4, agents: 2, ..Facts::default() };
        assert!(!Done::Next.met(&Facts::default(), &now));
    }

    #[test]
    fn a_prompt_before_the_step_does_not_count() {
        let start = Facts { prompted: later(10), ..Facts::default() };
        assert!(!Done::PromptConfirmed.met(&start, &Facts { prompted: later(10), ..Facts::default() }));
        assert!(Done::PromptConfirmed.met(&start, &Facts { prompted: later(11), ..Facts::default() }));
    }

    #[test]
    fn a_closed_project_does_not_complete_opening_one() {
        let start = Facts { projects: 2, ..Facts::default() };
        assert!(!Done::ProjectOpened.met(&start, &Facts { projects: 1, ..Facts::default() }));
    }

    #[rstest]
    #[case::no_project_yet(Done::ProjectOpened, Facts::default(), true)]
    #[case::a_project_already(Done::ProjectOpened, Facts { projects: 1, ..Facts::default() }, false)]
    #[case::a_repository(Done::WorkspaceMade, Facts { repository: true, ..Facts::default() }, true)]
    #[case::a_plain_folder(Done::WorkspaceMade, Facts::default(), false)]
    #[case::the_section_on(Done::AgentsSection, Facts { agents_section: true, ..Facts::default() }, false)]
    #[case::reading(Done::Next, Facts::default(), false)]
    fn a_step_asks_for_its_action_only_when_it_can_happen(#[case] done: Done, #[case] facts: Facts, #[case] asks: bool) {
        assert_eq!(done.asks(&facts), asks);
    }
}

mod progress {
    use super::*;

    #[test]
    fn a_step_completes_once() {
        let mut saved = Saved::default();
        assert!(saved.complete(core(), "projects"));
        assert!(!saved.complete(core(), "projects"));
        assert_eq!(saved.progress(CORE).done, ["projects"]);
    }

    #[test]
    fn an_unknown_step_is_not_kept() {
        let mut saved = Saved::default();
        assert!(!saved.complete(core(), "nope"));
        assert!(saved.tours.is_empty());
    }

    #[test]
    fn a_tour_resumes_at_its_first_step_not_done() {
        let mut saved = Saved::default();
        saved.complete(core(), "projects");
        saved.complete(core(), "tabs");
        assert_eq!(saved.resume_at(core()), 1);
    }

    #[test]
    fn a_finished_tour_starts_over() {
        let mut saved = Saved::default();
        saved.complete(core(), "projects");
        saved.finish(core());
        assert_eq!(saved.resume_at(core()), 0);
        assert!(saved.finished(CORE));
    }

    #[test]
    fn reset_forgets_one_tour_or_all_and_keeps_the_flags() {
        let mut saved = Saved { offered: true, announced: true, ..Saved::default() };
        saved.complete(core(), "projects");
        saved.complete(find("todo").expect("todo").1, "open");
        saved.reset(Some(CORE));
        assert!(saved.tours.contains_key("todo") && !saved.tours.contains_key(CORE));
        saved.reset(None);
        assert!(saved.tours.is_empty());
        assert!(saved.offered && saved.announced);
    }
}

mod first_run {
    use super::*;

    #[test]
    fn a_new_install_has_nothing() {
        let dir = TempDir::new();
        let (session, stats) = (dir.path().join("session.json"), dir.path().join("stats.json"));
        assert!(first_run(&session, &stats, &crate::star::Saved::default()));
    }

    #[test]
    fn a_session_stats_or_star_mean_an_old_install() {
        let dir = TempDir::new();
        let (session, stats) = (dir.path().join("session.json"), dir.path().join("stats.json"));
        let star = crate::star::Saved { days: 1, ..crate::star::Saved::default() };
        assert!(!first_run(&session, &stats, &star));
        std::fs::write(&stats, "{}").expect("stats");
        assert!(!first_run(&session, &stats, &crate::star::Saved::default()));
        std::fs::remove_file(&stats).expect("removed");
        std::fs::write(&session, "{}").expect("session");
        assert!(!first_run(&session, &stats, &crate::star::Saved::default()));
    }
}

mod file {
    use super::*;

    #[test]
    fn the_fixture_pins_the_format() {
        let text = crate::test_util::fixture!("tour/tour.json");
        let saved = serde_json::from_str::<Saved>(text).expect("fixture");
        assert!(saved.offered && !saved.announced);
        assert_eq!(saved.progress(CORE).done, ["projects", "workspaces"]);
        assert!(saved.finished("todo"));
        let dir = TempDir::new();
        let path = dir.path().join("tour.json");
        save(&path, &saved).expect("saved");
        assert_eq!(std::fs::read_to_string(path).expect("read").trim_end(), text.trim_end());
    }

    #[test]
    fn a_missing_file_is_none_and_a_broken_one_is_empty() {
        let dir = TempDir::new();
        let path = dir.path().join("tour.json");
        assert_eq!(load(&path), None);
        std::fs::write(&path, "nope").expect("written");
        assert_eq!(load(&path), Some(Saved::default()));
    }
}
