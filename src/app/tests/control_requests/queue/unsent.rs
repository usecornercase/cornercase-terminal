use super::super::unsent::quicken;
use super::results::scheduled;
use super::*;

fn launched(signal: &str) -> (Agent, u64) {
    let mut agent = Agent::new();
    agent.fake.signal(signal);
    let id = scheduled(&mut agent);
    agent.app.clients.events.subscribe(9, Vec::new(), false, true);
    agent.app.set_queue_area(AREA);
    agent.app.run_queued(id, Instant::now()).expect("starts");
    (agent, id)
}

fn entry(app: &App, id: u64) -> Entry {
    app.panels.queued.queue().entry(id).expect("entry").clone()
}

fn started(agent: &mut Agent, id: u64) -> u64 {
    agent.refreshing("the entry starts", |app| matches!(entry(app, id).state, State::Started { .. }));
    let State::Started { pane, .. } = entry(&agent.app, id).state else { panic!("started") };
    pane
}

fn read(agent: &Agent, file: &str) -> String {
    std::fs::read_to_string(agent.fake.dir().join(file)).unwrap_or_default()
}

#[test]
fn a_queued_claude_code_gets_its_prompt_once_its_input_is_ready() {
    let (mut agent, id) = launched("late-paste");

    let pane = started(&mut agent, id);

    assert!(!entry(&agent.app, id).unsent);
    assert!(pane_of(&agent.app, pane).context.prompted().is_some());
    assert_eq!((read(&agent, "pastes").as_str(), read(&agent, "lost").as_str()), ("bracketed\n", ""));
}

#[test]
fn a_queued_prompt_whose_enter_is_lost_gets_enter_again() {
    let (mut agent, id) = launched("lose");
    quicken(&mut agent);

    let pane = started(&mut agent, id);

    assert!(!entry(&agent.app, id).unsent);
    assert!(pane_of(&agent.app, pane).context.prompted().is_some());
    assert_eq!(read(&agent, "lost").lines().count(), 1);
}

fn pane_of(app: &App, id: u64) -> &Term {
    app.projects.pane_by(id).expect("the pane")
}

#[test]
fn a_queued_prompt_never_taken_says_so() {
    let (mut agent, id) = launched("deaf");
    quicken(&mut agent);

    let pane = started(&mut agent, id);

    let kept = entry(&agent.app, id);
    assert!(kept.unsent);
    let row = agent.app.queued_rows().into_iter().find(|row| row.id == id).expect("its row");
    assert!(row.meta.contains("· prompt not taken"), "{}", row.meta);
    assert_eq!(queue_panel::queued_colour(&row), Some(ratatui::style::Color::Yellow));
    let toast = agent.app.messages.toast.as_ref().map(|toast| toast.message.clone()).unwrap_or_default();
    assert!(toast.contains("did not take the prompt"), "{toast}");
    let told: Vec<_> = messages(&agent.app.take_events(), 9).into_iter().map(|(kind, ..)| kind).collect();
    assert_eq!(told, ["queue-unsent"]);
    let shown = agent.app.queue_shown(id).expect("shown");
    assert!(shown.entry.entry.unsent);
    assert!(shown.note.unwrap_or_default().contains("did not take the prompt"));

    std::fs::remove_file(agent.fake.dir().join("deaf")).expect("deaf goes");
    ask(&mut agent.app, None, send_text(pane, "", true, false));
    agent.refreshing("the agent works", |app| pane_of(app, pane).agent.status() == Some(activity::Status::Working));
    agent.refreshing("the flag clears", |app| !entry(app, id).unsent);
}
