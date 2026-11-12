use super::*;
use crate::cli::args::config::ConfigAction;
use crate::cli::tests::{config_args, parse};

#[test]
fn the_command_takes_one_id_as_text() {
    let config = config_args(&["config", "forum-server", "8"]);

    assert!(matches!(config.action, Some(ConfigAction::ForumServer { id }) if id == "8"));
    assert!(parse(&["config", "forum-server"]).is_err());
}

#[test]
fn json_may_come_after_the_id() {
    assert!(config_args(&["config", "forum-server", "8", "--json"]).json);
}

#[test]
fn the_line_names_the_server_and_where_to_follow_it() {
    let server = ForumServerInfo { id: "8".into(), name: "work".into() };

    assert_eq!(picked(&server), "the forum goes in work; `cornercase status --app` says when it is ready\n");
}
