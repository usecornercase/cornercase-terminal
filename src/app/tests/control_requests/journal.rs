#![cfg(unix)]

use super::*;
use crate::recap::journal::{Entry, Event, files};
use crate::recap::{Choice, Recap};

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

fn place(app: &mut App, rx: &Receiver<AppEvent>) -> Value {
    let written = crate::time::now();
    wait_until("the second the journal last wrote in is over, since a range ends before now", || {
        crate::time::now() > written
    });
    let dossier = json!({"range": "now", "dossier": true});
    asked(app, rx, "recap", &dossier).expect("a dossier")["places"]["c1"].take()
}

#[test]
fn the_places_of_a_recap_carry_the_id_a_pane_names_while_it_is_open_and_once_it_closed() {
    let state = TempDir::new();
    let journal = state.path().join("journal");
    let mut agent = Agent::new();
    agent.app.set_recap_places(&journal, &state.path().join("recap"));
    let id = agent.start(None);
    agent.refreshing("the conversation is seen", |_| entries(&journal).iter().any(|e| e.event == Event::Seen));
    let unnamed = agents::in_status(&mut agent.app, id).conversation;

    agent.transcript(id, &[r#"{"type":"user","uuid":"p1","origin":{"kind":"human"}}"#]);
    agent.refreshing("the agent has written", |a| pane(a, id).context.conversation().is_some());
    let named = agents::in_status(&mut agent.app, id).conversation.expect("the status names the conversation");
    let open = place(&mut agent.app, &agent.rx);
    let tab = agent.app.projects[0].workspaces[0].tabs.iter().position(|t| t.panes.iter().any(|p| p.id == id));
    agent.app.close_tab(tab_at(&agent.app, 0, 0, tab.expect("the agent's tab")));
    agent.refreshing("the close is journaled", |_| entries(&journal).iter().any(|e| e.event == Event::Closed));
    let closed = place(&mut agent.app, &agent.rx);

    assert_eq!(unnamed, None);
    assert_eq!(entries(&journal)[0].conversation, None);
    assert_eq!(open["agent"], "claude");
    assert_eq!((&open["conversation"], &open["pane"]), (&json!(named), &json!(id)));
    assert_eq!(open["boot"], json!(wire::server_token()));
    assert_eq!(closed["conversation"], json!(named));
}

fn followed_by_a_client(report: &Report, place: &Value) -> Option<u64> {
    let tabs = report.projects.iter().flat_map(|p| &p.workspaces).flat_map(|w| &w.tabs);
    let mut panes = tabs.flat_map(|t| &t.panes);
    let id = place["conversation"].as_str()?;
    let same = |pane: &&wire::PaneInfo| pane.conversation.as_deref() == Some(id) && json!(pane.agent) == place["agent"];
    panes.find(same).map(|pane| pane.id)
}

fn pane_in(app: &App, conversation: &str) -> Option<u64> {
    app.projects.terms().find(|term| term.context.conversation() == Some(conversation)).map(|term| term.id)
}

#[test]
fn a_client_and_a_link_of_the_panel_find_the_pane_a_conversation_moved_to() {
    let state = TempDir::new();
    let (mut app, rx, _dirs) = app_with(1);
    app.set_recap_places(&state.path().join("journal"), &state.path().join("recap"));
    let fake = FakeClaude::running(FakeClaude::ANSWERING);
    app.config.agent_commands.insert("claude".into(), fake.script().display().to_string());
    app.config.agent_args.insert("claude".into(), vec![fake.dir().display().to_string()]);
    app.agents.set_claude_dir(fake.dir());
    type_line(&mut app, &fake.command_line(""));
    refreshing(&mut app, &rx, "the conversation is known", |a| pane_in(a, "s1").is_some());
    let from = pane_in(&app, "s1").expect("the agent's pane");
    let workspace = app.projects[0].workspaces[0].id;

    ok(&mut app, "agents-close", &json!({"workspace": workspace}));
    refreshing(&mut app, &rx, "the conversation closes", |a| pane_in(a, "s1").is_none());
    let place = place(&mut app, &rx);
    ok(&mut app, "agents-reopen", &json!({"workspace": workspace}));
    refreshing(&mut app, &rx, "the conversation is back", |a| pane_in(a, "s1").is_some());
    let to = pane_in(&app, "s1").expect("the agent's new pane");
    refreshing(&mut app, &rx, "the status says what it runs", |a| {
        followed_by_a_client(&a.report(None), &place) == Some(to)
    });
    app.add_tab(workspace_at(&app, 0, 0), AREA).expect("another tab to look at");
    app.panels.recap.run_mut(Choice::Now).last = Some(Recap {
        places: [("c1".to_string(), serde_json::from_value(place.clone()).expect("a place"))].into(),
        ..Recap::default()
    });
    app.recap_go("c1");

    assert_ne!(to, from);
    assert_eq!((&place["conversation"], &place["pane"]), (&json!("s1"), &json!(from)));
    assert_eq!(app.term().map(|term| term.id), Some(to));
}
