use std::path::PathBuf;

use super::*;
use crate::recap::journal::{Entry, Event, files};

const NO_CONVERSATION_YET: [agents::Known; 3] = [agents::Known::Claude, agents::Known::Gemini, agents::Known::Vibe];

fn entries(dir: &Path) -> Vec<Entry> {
    let now = crate::time::now();
    files::read(dir, now - 3 * 86_400, now + 86_400)
}

fn journal(w: &Watched) -> PathBuf {
    w.app.panels.recap.folders.as_ref().expect("recap folders").journal.clone()
}

fn kinds(found: &[Entry]) -> Vec<(Event, Option<&str>)> {
    found.iter().map(|entry| (entry.event, entry.to.as_deref())).collect()
}

#[rstest]
fn the_journal_follows_every_agent_from_its_first_conversation_to_its_close(
    #[values(
        agents::Known::Claude,
        agents::Known::Codex,
        agents::Known::Gemini,
        agents::Known::Opencode,
        agents::Known::Grok,
        agents::Known::Cursor,
        agents::Known::Copilot,
        agents::Known::Vibe
    )]
    agent: agents::Known,
) {
    let state = TempDir::new();
    let mut w = Watched::start(agent, false);
    w.app.set_recap_places(&state.path().join("journal"), &state.path().join("recap"));
    w.report("busy", 3);
    w.report("waiting", 3);
    w.report("idle", 3);
    w.app.close_tab(tab_at(&w.app, 0, 0, 0));
    tick(&mut w.app);

    let found = entries(&journal(&w));
    if NO_CONVERSATION_YET.contains(&agent) {
        assert_eq!(kinds(&found), []);
        return;
    }
    let seen: Vec<&Entry> = found.iter().filter(|entry| entry.event == Event::Seen).collect();
    assert!(!seen.is_empty(), "{:?}", kinds(&found));
    assert_eq!(seen[0].agent, agent.name());
    assert!(seen[0].project.is_some() && seen[0].workspace.is_some() && seen[0].tab.is_some());
    let statuses: Vec<&str> =
        found.iter().filter(|entry| entry.event == Event::Status).filter_map(|entry| entry.to.as_deref()).collect();
    let waits = agent != agents::Known::Opencode;
    assert!(!statuses.is_empty() && (!waits || statuses.contains(&"waiting")), "{statuses:?}");
    assert_eq!(found.last().map(|entry| entry.event), Some(Event::Closed), "{:?}", kinds(&found));
}

#[test]
fn a_conversation_is_recorded_with_the_place_it_had_and_keeps_it_when_its_folder_is_gone() {
    let state = TempDir::new();
    let mut w = Watched::start(agents::Known::Codex, false);
    w.app.set_recap_places(&state.path().join("journal"), &state.path().join("recap"));
    w.report("busy", 3);
    w.app.close_tab(tab_at(&w.app, 0, 0, 0));
    tick(&mut w.app);
    let found = entries(&journal(&w));
    let place = found.iter().find_map(|entry| entry.workspace.clone()).expect("a place");
    std::fs::remove_dir_all(&place.path).ok();
    let again = entries(&journal(&w));
    assert_eq!(again.iter().find_map(|entry| entry.workspace.clone()).map(|w| w.path), Some(place.path));
}

#[test]
fn without_a_server_folder_nothing_is_written() {
    let mut w = Watched::start(agents::Known::Claude, false);
    w.report("busy", 3);
    assert!(w.app.panels.recap.folders.is_none());
}
