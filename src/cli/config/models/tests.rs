use super::*;
use crate::cli::Control;
use crate::cli::args::Command as Cli;
use crate::cli::args::config::ConfigAction;
use crate::cli::tests::parse;

#[test]
fn the_command_takes_one_agent() {
    let Some(Cli::Control(Control::Config(config))) = parse(&["config", "models", "opencode"]).expect("parse").command
    else {
        panic!("not config")
    };

    assert!(matches!(config.action, Some(ConfigAction::Models { agent }) if agent == "opencode"));
    assert!(parse(&["config", "models"]).is_err());
}

#[test]
fn json_may_come_after_the_agent() {
    let Some(Cli::Control(Control::Config(config))) =
        parse(&["config", "models", "opencode", "--json"]).expect("parse").command
    else {
        panic!("not config")
    };

    assert!(config.json);
}

#[test]
fn the_text_is_one_model_per_line_and_nothing_for_none() {
    let some = AgentModelsInfo { agent: "opencode".into(), models: vec!["a/b".into(), "c/d".into()] };
    let none = AgentModelsInfo { agent: "amp".into(), models: Vec::new() };

    assert_eq!(lines(&some), "a/b\nc/d\n");
    assert_eq!(lines(&none), "");
}
