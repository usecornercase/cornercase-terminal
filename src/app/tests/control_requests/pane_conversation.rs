use super::*;
use crate::agents::context;

fn conversation(app: &mut App) -> Value {
    let status = value(app, None, Command::Status(wire::Status {}));
    status["projects"][0]["workspaces"][0]["tabs"][0]["panes"][0]["conversation"].clone()
}

#[test]
fn a_pane_says_null_until_its_reader_knows_the_conversation_and_then_gives_its_id() {
    let dir = TempDir::new();
    let (mut app, _rx) = new_app_with_shell(if cfg!(windows) { "cmd.exe" } else { "/bin/sh" }, no_config());
    app.open_project(dir.path().to_path_buf(), AREA).expect("open a project");

    let before = conversation(&mut app);
    app.term_mut().expect("a pane").context = context::Pane::claude_in(dir.path(), "a01c");

    assert_eq!((before, conversation(&mut app)), (Value::Null, json!("a01c")));
}
