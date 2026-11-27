use super::commands::{Followed, agent_app, send_and_finish, start_and_finish, told};
use super::*;

#[test]
fn events_tell_when_an_agents_last_message_changes_and_pane_filters_them() {
    let app = agent_app();
    let session = &app.session;
    let started = start_and_finish(session, "fix the login");
    let pane = started.lines().next().expect("the pane").to_string();
    let shell = session.report().projects[0].workspaces[0].tabs[0].panes[0].id.to_string();
    let mut all = session.spawn(&["events", "--json"]);
    let agents = session.spawn(&["events", "--pane", &pane]);
    let shells = session.spawn(&["events", "--pane", &shell]);
    session.wait_for_file("server.log", "all follow", |log| log.matches("command=events").count() == 3);
    let mut followed = Followed::of(&mut all);
    let waited = session.says(&["new-tab", "--", "sleep 1"]);
    followed.until("a read step passed", |e| told(e, "exited", "program", "sleep", &waited));

    send_and_finish(session, &pane, &["--timeout", "30"], "add tests");
    followed.until("the new message", |e| told(e, "last-message", "agent", "claude", &pane));
    let stopped = session.run("kill-server");

    assert!(stopped.status.success(), "{stopped:?}");
    let (agents, shells) = (finish_within(agents, TIMEOUT), finish_within(shells, TIMEOUT));
    let agents = String::from_utf8_lossy(&agents.stdout);
    let changed = format!("pane {pane}  claude  last message changed");
    assert_eq!(agents.lines().filter(|line| *line == changed).count(), 1, "{agents}");
    assert!(!String::from_utf8_lossy(&shells.stdout).contains("last message"), "{shells:?}");
    drop(all);
}
