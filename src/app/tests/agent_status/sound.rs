use super::*;
use crate::host::sound::{Cue, Kind};

fn loud(w: &mut Watched) {
    w.app.config.sound = true;
    w.app.config.sound_waiting = "~/needs.wav".into();
    w.app.config.sound_done = "~/done.wav".into();
}

fn kinds(w: &mut Watched) -> Vec<Kind> {
    w.app.take_sounds().into_iter().map(|cue| cue.kind).collect()
}

fn with_a_subagent(subagents: bool) -> Watched {
    let mut w = Watched::start(agents::Known::Claude, false);
    loud(&mut w);
    w.app.config.sound_subagents = subagents;
    let parent = tab_term(&w.app, 0, 0).id;
    w.app.add_tab(workspace_at(&w.app, 0, 0), AREA).expect("a tab for cursor");
    w.run(agents::Known::Cursor);
    w.report("busy", 1);
    let cursor = tab_term(&w.app, 0, 1).id;
    w.app.projects.pane_by_mut(cursor).expect("the cursor pane").parent = Some(parent);
    w.app.add_tab(workspace_at(&w.app, 0, 0), AREA).expect("hide both agents");
    w
}

#[test]
fn an_agent_that_needs_you_in_a_hidden_tab_plays_the_sound_for_it() {
    let mut w = Watched::start(agents::Known::Claude, true);
    loud(&mut w);

    w.report("waiting", 6);

    assert_eq!(w.app.take_sounds(), [Cue { kind: Kind::NeedsYou, path: "~/needs.wav".into() }]);
}

#[test]
fn an_agent_back_at_work_within_a_second_plays_nothing() {
    let mut w = Watched::start(agents::Known::Claude, true);
    loud(&mut w);

    w.report("waiting", 1);
    w.report("busy", 6);

    assert_eq!(kinds(&mut w), []);
}

#[test]
fn a_finished_agent_plays_the_other_sound() {
    let mut w = Watched::start(agents::Known::Claude, true);
    loud(&mut w);

    w.report("idle", 6);

    assert_eq!(w.app.take_sounds(), [Cue { kind: Kind::Finished, path: "~/done.wav".into() }]);
}

#[test]
fn an_agent_in_the_tab_on_screen_plays_nothing() {
    let mut w = Watched::start(agents::Known::Claude, false);
    loud(&mut w);

    w.report("waiting", 6);

    assert_eq!(kinds(&mut w), []);
}

#[test]
fn with_the_sound_off_nothing_plays() {
    let mut w = Watched::start(agents::Known::Claude, true);

    w.report("waiting", 6);

    assert_eq!(kinds(&mut w), []);
}

#[test]
fn desktop_notifications_off_keep_the_sound_and_the_toast() {
    let mut w = Watched::start(agents::Known::Claude, true);
    loud(&mut w);
    w.app.config.desktop_notifications = notify::OFF.into();

    w.report("waiting", 6);

    let (shown, sent) = w.told();
    assert_eq!((shown.is_some(), sent), (true, Vec::new()));
    assert_eq!(kinds(&mut w), [Kind::NeedsYou]);
}

#[test]
fn a_collapsed_parent_hides_no_subagent_from_the_sound_rule() {
    let mut w = with_a_subagent(false);
    let parent = tab_term(&w.app, 0, 0).id;
    w.app.projects.pane_by_mut(parent).expect("the parent").collapsed = true;

    w.report("waiting", 6);

    assert_eq!(w.told().1.len(), 2);
    assert_eq!(w.app.take_sounds().len(), 1);
}

#[rstest]
#[case::only_the_agent_that_started_it(false, 1)]
#[case::both_with_subagents_on(true, 2)]
fn an_agent_started_by_another_agent_is_quiet_unless_asked(#[case] subagents: bool, #[case] sounds: usize) {
    let mut w = with_a_subagent(subagents);

    w.report("waiting", 6);

    assert_eq!(w.told().1.len(), 2);
    assert_eq!(w.app.take_sounds().len(), sounds);
}

#[test]
fn agents_that_name_each_other_as_parent_still_play_for_the_first_one() {
    let mut w = with_a_subagent(false);
    let claude = tab_term(&w.app, 0, 0).id;
    let cursor = tab_term(&w.app, 0, 1).id;
    w.app.projects.pane_by_mut(claude).expect("the claude pane").parent = Some(cursor);

    w.report("waiting", 6);

    assert_eq!(w.told().1.len(), 2);
    assert_eq!(w.app.take_sounds().len(), 1);
}

#[test]
fn an_agent_whose_parent_is_a_shell_is_a_main_agent() {
    let mut w = Watched::start(agents::Known::Claude, true);
    loud(&mut w);
    let shell = tab_term(&w.app, 0, 1).id;
    let claude = tab_term(&w.app, 0, 0).id;
    w.app.projects.pane_by_mut(claude).expect("the claude pane").parent = Some(shell);

    w.report("waiting", 6);

    assert_eq!(kinds(&mut w), [Kind::NeedsYou]);
}

fn program_reports(w: &mut Watched, tabs: &[usize], state: &str) {
    for &tab in tabs {
        let pane = tab_term(&w.app, 0, tab).id;
        let osc = format!("\x1b]7501;state={state}\x1b\\");
        w.app.projects.pane_by_mut(pane).expect("the agent pane").feed(osc.as_bytes());
    }
    for _ in 0..6 {
        tick(&mut w.app);
    }
}

#[test]
fn an_agent_that_reports_a_failure_plays_the_sound_that_needs_you() {
    let mut w = Watched::start(agents::Known::Claude, true);
    loud(&mut w);

    program_reports(&mut w, &[0], "error");

    assert_eq!(status(&w.app, 0, 0), Some(Status::Failed));
    assert_eq!(kinds(&mut w), [Kind::NeedsYou]);
}

#[test]
fn an_agent_and_its_subagent_needing_you_together_play_one_sound() {
    let mut w = Watched::start(agents::Known::Claude, false);
    loud(&mut w);
    let parent = tab_term(&w.app, 0, 0).id;
    w.app.add_tab(workspace_at(&w.app, 0, 0), AREA).expect("a tab for the subagent");
    w.run(agents::Known::Claude);
    let child = tab_term(&w.app, 0, 1).id;
    w.app.projects.pane_by_mut(child).expect("the subagent pane").parent = Some(parent);
    w.app.add_tab(workspace_at(&w.app, 0, 0), AREA).expect("hide both agents");

    program_reports(&mut w, &[0, 1], "blocked");

    assert_eq!(w.told().1.len(), 1);
    assert_eq!(kinds(&mut w), [Kind::NeedsYou]);
}
