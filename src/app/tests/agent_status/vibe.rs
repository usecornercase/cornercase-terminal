use std::path::PathBuf;

use crate::agents::vibe::fake::{FakeVibe, Usage};
use crate::test_util::TempDir;

use super::*;

fn script(fake: &FakeVibe, dir: &TempDir) -> PathBuf {
    let script = dir.path().join("vibe-rs");
    let active = fake.path().join("logs/session/active");
    write_executable(
        &script,
        &format!(
            "#!/bin/sh\nmkdir -p {active}\n\
             printf '{{\"process_id\":%s,\"session_id\":\"{id}\"}}' \"$$\" > {active}/{id}.lock.json\n\
             while :; do sleep 1; done\n",
            active = agents::quote(&active.to_string_lossy()),
            id = fake.id,
        ),
    );
    script
}

fn after_a_turn(fake: &FakeVibe, dir: &TempDir) -> (App, Receiver<AppEvent>, Vec<TempDir>) {
    fake.turn("completed", "glm-5-3", Usage { input: 130_300, cached: 100_000, output: 240 }, 1_700_000_000_000);
    let script = script(fake, dir);
    let (mut app, rx, dirs) = app_with(1);
    app.agents.set_vibe_dir(fake.path());
    type_line(&mut app, &agents::quote(&script.to_string_lossy()));
    pump_until(&mut app, &rx, "vibe runs", |a| runs(a, agents::Known::Vibe.name()));
    (app, rx, dirs)
}

#[test]
fn a_vibe_tab_shows_the_model_and_the_context_percentage() {
    let (fake, dir) = (FakeVibe::new(), TempDir::new());
    let (mut app, rx, _dirs) = after_a_turn(&fake, &dir);

    watch_until(&mut app, &rx, "vibe's context shows", |a| a.tab().is_some_and(|tab| tab.context().is_some()));

    let row = second_row(&mut app).1;
    assert!(row.contains("glm-5-3") && row.contains("16%"), "{row}");
}

#[test]
fn a_vibe_pane_names_its_conversation_once_it_has_had_a_turn() {
    let (fake, dir) = (FakeVibe::new(), TempDir::new());
    let (mut app, rx, _dirs) = after_a_turn(&fake, &dir);

    let named = |a: &App| a.report(None).projects[0].workspaces[0].tabs[0].panes[0].conversation.clone();
    watch_until(&mut app, &rx, "the status names it", |a| named(a).as_deref() == Some(fake.id.as_str()));
}
