use super::agents::in_status;
use super::*;

fn compacting(agent: &mut Agent, text: &str) -> u64 {
    let id = agent.start(None);
    agent.refreshing("the pane knows its conversation", |a| {
        pane(a, id).context.record_for(crate::agents::Known::Claude).is_some()
    });
    agent.app.clients.set_confirm_within(Duration::from_secs(1));
    ask(&mut agent.app, None, send_text(id, text, true, false));
    agent.refreshing("the agent compacts", |a| pane(a, id).agent.status() == Some(activity::Status::Working));
    id
}

#[test]
fn a_compact_command_is_confirmed_by_the_compaction_it_records_long_after_the_enter() {
    let mut agent = Agent::new();
    let id = compacting(&mut agent, "/compact keep: the branches");
    let started = Instant::now();
    agent.refreshing("longer than the confirmation window goes by", move |_| {
        started.elapsed() > Duration::from_millis(2500)
    });
    let early = answers(&mut agent.app);
    assert!(early.is_empty(), "{early:?}");

    agent.finish();
    done(agent.answered("the compaction is recorded"));
    agent.refreshing("the agent is idle", |a| pane(a, id).agent.status() != Some(activity::Status::Working));
    let compacted = in_status(&mut agent.app, id);
    let shown = agent.app.tab_details(&agent.app.projects[0].workspaces[0].tabs[1]).compacted;

    ask(&mut agent.app, None, send_text(id, "go on", true, false));
    done(agent.answered("the prompt is recorded"));
    agent.finish();
    agent.refreshing("the next reply sets the context", |a| pane(a, id).context.context().is_some());
    let answered = in_status(&mut agent.app, id);

    assert!(early.is_empty(), "{early:?}");
    assert_eq!((compacted.compacted, compacted.context, shown), (true, None, true));
    assert_eq!((answered.compacted, answered.context.is_some()), (false, true));
}

#[test]
fn a_compact_command_the_agent_never_records_fails_once_it_stops_working() {
    let mut agent = Agent::new();
    compacting(&mut agent, "/compact nothing");

    agent.finish();
    let message = error(agent.answered("the confirmation gives up"));

    assert!(message.starts_with("not confirmed: the prompt may not have been submitted"), "{message}");
}
