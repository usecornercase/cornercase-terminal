use super::*;
use crate::agents::launch::CONFIRM_EVERY;

const QUICK: Duration = Duration::from_millis(100);

fn start_with_prompt(agent: &mut Agent, wait: bool) {
    let start = wire::Start {
        agent: Some("claude".into()),
        name: Some("fixer".into()),
        prompt: Some("fix the login".into()),
        wait,
        ..wire::Start::default()
    };
    ask(&mut agent.app, None, Command::Start(start));
}

pub(super) fn quicken(agent: &mut Agent) {
    agent.refreshing("the launch exists", |app| !app.agents.launches.list().is_empty());
    let panes: Vec<u64> = agent.app.agents.launches.list().iter().map(|launch| launch.term).collect();
    for pane in panes {
        for launch in agent.app.agents.launches.of_pane_mut(pane) {
            assert_eq!(launch.confirm, Some(CONFIRM_EVERY), "a launch with a prompt confirms by default");
            launch.confirm = Some(QUICK);
        }
    }
}

#[test]
fn a_start_whose_prompt_is_never_taken_answers_unsent_with_a_warning() {
    let mut agent = Agent::new();
    agent.fake.signal("deaf");
    start_with_prompt(&mut agent, false);
    quicken(&mut agent);

    let answer = done(agent.answered("the start answers"));

    assert_eq!(answer.unsent, Some(true));
    let pane = answer.ids.pane.expect("its pane");
    let warning = answer.warning.expect("a warning");
    assert!(warning.contains(&format!("cornercase keys --pane {pane} enter")), "{warning}");
}

#[test]
fn a_waiting_start_whose_prompt_is_never_taken_fails_with_that_text() {
    let mut agent = Agent::new();
    agent.fake.signal("deaf");
    start_with_prompt(&mut agent, true);
    quicken(&mut agent);

    let message = error(agent.answered("the start fails"));

    assert!(message.contains("did not take its prompt"), "{message}");
}

#[test]
fn a_start_answers_once_its_prompt_is_taken() {
    let mut agent = Agent::new();
    start_with_prompt(&mut agent, false);

    let answer = done(agent.answered("the start answers"));

    assert_eq!(answer.unsent, None);
    let pane = answer.ids.pane.expect("its pane");
    assert!(super::pane(&agent.app, pane).context.prompted().is_some());
}

#[test]
fn a_line_typed_while_the_agent_has_bracketed_paste_on_reaches_it() {
    let mut agent = Agent::new();
    let id = agent.start(None);
    agent.app.show(Item::Pane(id)).expect("the agent's pane shows");
    agent.refreshing("the agent turns bracketed paste on", |app| pane(app, id).emulator.bracketed_paste());

    type_line(&mut agent.app, "also the footer");

    agent.refreshing("the line is taken", |app| pane(app, id).context.prompted().is_some());
    agent.finish();
}
