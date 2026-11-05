#![cfg(unix)]

use super::*;
use crate::recap::journal::{Entry, Event, files};

fn entries(dir: &Path) -> Vec<Entry> {
    let now = crate::time::now();
    files::read(dir, now - 3 * 86_400, now + 86_400)
}

#[test]
fn a_claude_pane_is_journaled_with_its_conversation_prompts_and_statuses() {
    let state = TempDir::new();
    let mut agent = Agent::new();
    agent.app.set_recap_places(&state.path().join("journal"), &state.path().join("recap"));
    let id = agent.start(Some("fix the login"));
    agent.refreshing("the prompt is journaled", |_| {
        entries(&state.path().join("journal")).iter().any(|entry| entry.event == Event::Prompt)
    });
    agent.finish();
    agent.refreshing("the turn ends", |_| {
        entries(&state.path().join("journal")).iter().filter(|e| e.event == Event::Status).count() >= 2
    });

    let found = entries(&state.path().join("journal"));
    let seen = found.iter().find(|entry| entry.event == Event::Seen).expect("a conversation was seen");
    assert_eq!((seen.agent.as_str(), seen.pane), ("claude", Some(id)));
    assert!(seen.record.is_some(), "{seen:?}");
    assert_eq!(found.iter().filter(|e| e.event == Event::Seen).count(), 1, "{found:?}");
    assert!(seen.project.is_some() && seen.workspace.is_some() && seen.tab.is_some());
    let to: Vec<&str> = found.iter().filter(|e| e.event == Event::Status).filter_map(|e| e.to.as_deref()).collect();
    assert!(to.contains(&"working"), "{to:?}");
}
