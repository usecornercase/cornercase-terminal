use super::*;

#[test]
fn a_shell_pane_has_no_context_to_read() {
    let app = Harness::start();
    let session = &app.session;
    let shell = session.report().projects[0].workspaces[0].tabs[0].panes[0].id.to_string();

    let out = session.cli(&["context", "--pane", &shell]);

    assert!(refused(&out, 1, &format!("pane {shell} runs no agent cornercase reads")), "{out:?}");
    assert!(out.stdout.is_empty(), "{out:?}");
}
