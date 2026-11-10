use super::*;
use crate::app::events::Streamed;

pub(super) fn with_a_subagent() -> Watched {
    let mut w = Watched::start(agents::Known::Claude, false);
    let parent = tab_term(&w.app, 0, 0).id;
    w.app.add_tab(workspace_at(&w.app, 0, 0), AREA).expect("a tab for cursor");
    w.run(agents::Known::Cursor);
    w.report("busy", 1);
    let cursor = tab_term(&w.app, 0, 1).id;
    w.app.projects.pane_by_mut(cursor).expect("the cursor pane").parent = Some(Parent::pane(parent));
    w.app.add_tab(workspace_at(&w.app, 0, 0), AREA).expect("hide both agents");
    w
}

pub(super) fn with_a_shell_parent() -> Watched {
    let mut w = Watched::start(agents::Known::Claude, true);
    let shell = tab_term(&w.app, 0, 1).id;
    let claude = tab_term(&w.app, 0, 0).id;
    w.app.projects.pane_by_mut(claude).expect("the claude pane").parent = Some(Parent::pane(shell));
    w
}

const READER: u64 = 1;

fn listened(on: bool) -> Watched {
    let mut w = with_a_subagent();
    w.app.config.notify_subagents = on;
    w.app.config.sound = true;
    w.app.config.sound_subagents = true;
    w.app.clients.events.subscribe(READER, Vec::new(), false, true);
    let _ = w.app.take_events();
    w
}

fn notice_events(w: &mut Watched) -> Vec<String> {
    w.app
        .take_events()
        .into_iter()
        .filter_map(|(_, streamed)| match streamed {
            Streamed::Event(text) if text.contains("\"event\":\"notice\"") => Some(text),
            _ => None,
        })
        .collect()
}

mod turned_off {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::needing_you("blocked", Status::Waiting)]
    #[case::failing("error", Status::Failed)]
    #[case::finishing("done", Status::Done)]
    fn a_hidden_subagent_tells_no_one_and_keeps_its_mark(#[case] state: &str, #[case] shown: Status) {
        let mut w = listened(false);

        program_reports(&mut w, &[1], state);

        assert_eq!(w.told(), (None, Vec::new()));
        assert_eq!((w.app.take_sounds(), notice_events(&mut w)), (Vec::new(), Vec::<String>::new()));
        assert_eq!(status(&w.app, 0, 1), Some(shown));
    }

    #[test]
    fn the_agent_that_started_it_still_tells_you_on_every_channel() {
        let mut w = listened(false);

        program_reports(&mut w, &[0], "blocked");

        let (shown, sent) = w.told();
        assert_eq!((shown.is_some(), sent.len(), w.app.take_sounds().len()), (true, 1, 1));
        let events = notice_events(&mut w);
        assert!(matches!(&events[..], [only] if !only.contains("subagent")), "{events:?}");
    }

    #[test]
    fn a_subagent_whose_parent_closed_tells_you() {
        let mut w = listened(false);
        w.app.close_tab(tab_at(&w.app, 0, 0, 0));
        wait_until("the parent's tab closes", || {
            drain(&mut w.app, &w.rx);
            w.app.projects[0].workspaces[0].tabs.len() == 2
        });

        program_reports(&mut w, &[0], "blocked");

        assert_eq!(w.told().1.len(), 1);
    }

    #[test]
    fn an_agent_started_from_a_shell_tells_you() {
        let mut w = with_a_shell_parent();
        w.app.config.notify_subagents = false;

        w.report("waiting", 6);

        assert_eq!(w.told().1.len(), 1);
    }
}

mod turned_on {
    use super::*;

    #[test]
    fn a_hidden_subagent_tells_you_and_its_notice_says_it_is_one() {
        let mut w = listened(true);

        program_reports(&mut w, &[1], "blocked");

        assert_eq!((w.told().1.len(), w.app.take_sounds().len()), (1, 1));
        let events = notice_events(&mut w);
        assert!(matches!(&events[..], [only] if only.contains("\"subagent\":true")), "{events:?}");
    }
}
