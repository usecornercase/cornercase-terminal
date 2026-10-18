use super::*;
use crate::agents::Known;
use crate::agents::context::Context;

fn start(app: &mut App, rx: &Receiver<AppEvent>, fake: &FakeCopilot) -> usize {
    type_line(app, &fake.command_line());
    let tab = app.projects[0].workspaces[0].active;
    watch_until(app, rx, "copilot holds its session", |a| {
        tab_term(a, 0, tab).context.record_for(Known::Copilot).is_some()
    });
    tab
}

fn shown(app: &App, tab: usize) -> Option<Context> {
    tab_term(app, 0, tab).context.context().cloned()
}

fn context(model: &str, percent: Option<u16>) -> Context {
    Context { model: model.into(), percent }
}

#[test]
fn a_tab_shows_its_model_and_context_until_a_compaction() {
    let (mut app, rx, _dirs) = app_with(1);
    let fake = FakeCopilot::new(&app.projects[0].path);
    let tab = start(&mut app, &rx, &fake);
    fake.started("gpt-4.1");
    fake.prompt("fix the login");
    fake.usage(32_000, 64_000, "gpt-4.1");
    watch_until(&mut app, &rx, "the context shows", |a| shown(a, tab) == Some(context("gpt-4.1", Some(50))));

    fake.compacted();

    watch_until(&mut app, &rx, "the percentage goes", |a| shown(a, tab) == Some(context("gpt-4.1", None)));
}

#[test]
fn a_tab_shows_auto_until_a_model_is_chosen() {
    let (mut app, rx, _dirs) = app_with(1);
    let fake = FakeCopilot::new(&app.projects[0].path);
    let tab = start(&mut app, &rx, &fake);
    fake.started("auto");
    watch_until(&mut app, &rx, "auto shows", |a| shown(a, tab) == Some(context("auto", None)));

    fake.auto_resolved("claude-sonnet-4.5");
    fake.usage(16_000, 64_000, "claude-sonnet-4.5");

    watch_until(&mut app, &rx, "the chosen model shows", |a| {
        shown(a, tab) == Some(context("claude-sonnet-4.5", Some(25)))
    });
}

#[test]
fn a_second_copilot_in_its_folder_keeps_its_own_status_and_model() {
    let (mut app, rx, _dirs) = app_with(1);
    let folder = app.projects[0].path.clone();
    let (one, other) = (FakeCopilot::new(&folder), FakeCopilot::new(&folder));
    let first = start(&mut app, &rx, &one);
    app.add_tab(0, 0, AREA).expect("a second tab");
    let second = start(&mut app, &rx, &other);
    one.started("gpt-4.1");
    other.started("claude-sonnet-4.5");

    one.prompt("work");
    one.turn(true);
    one.working(true);

    watch_until(&mut app, &rx, "each keeps its own", |a| {
        tab_term(a, 0, first).context.activity("") == Some(Activity::Working)
            && tab_term(a, 0, second).context.activity("") == Some(Activity::Idle)
            && shown(a, first) == Some(context("gpt-4.1", None))
            && shown(a, second) == Some(context("claude-sonnet-4.5", None))
    });
}

#[test]
fn a_background_shell_keeps_the_tab_working_after_the_turn() {
    let (mut app, rx, _dirs) = app_with(1);
    let fake = FakeCopilot::new(&app.projects[0].path);
    let tab = start(&mut app, &rx, &fake);
    fake.started("gpt-4.1");
    fake.prompt("serve it");
    fake.turn(true);
    fake.working(true);
    fake.shell_started("0");
    fake.turn(false);

    watch_until(&mut app, &rx, "the shell shows as working", |a| {
        tab_term(a, 0, tab).context.activity("") == Some(Activity::Shell)
    });

    assert_eq!(status(&app, 0, tab), Some(Status::Working));
}

#[test]
fn a_sub_agent_keeps_the_tab_working_after_the_turn() {
    let (mut app, rx, _dirs) = app_with(1);
    let fake = FakeCopilot::new(&app.projects[0].path);
    let tab = start(&mut app, &rx, &fake);
    fake.started("gpt-4.1");
    fake.prompt("explore it");
    fake.turn(true);
    fake.working(true);
    watch_until(&mut app, &rx, "it works", |a| status(a, 0, tab) == Some(Status::Working));
    fake.subagent_started("agent-1");
    fake.turn(false);
    fake.working(false);

    watch_until(&mut app, &rx, "the sub-agent shows as working", |a| {
        tab_term(a, 0, tab).context.activity("") == Some(Activity::Subagents)
    });
    let early = app.take_notifications();
    fake.subagent_ended("agent-1");

    watch_until(&mut app, &rx, "the tab is idle", |a| status(a, 0, tab) == Some(Status::Idle));
    assert_eq!(early.len(), 0);
}
