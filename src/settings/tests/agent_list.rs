use std::path::PathBuf;

use super::*;
use crate::config::Config;
use crate::control::config::AgentEntry;
use crate::flows::{Flow, Role};

fn listed() -> Settings {
    Settings::new(
        Config { agent_commands: [("mine".to_string(), "/bin/true".to_string())].into(), ..Config::default() },
        Some(PathBuf::from("/home/ana")),
        Vec::new(),
        Page::default(),
    )
}

fn entry(name: &str) -> AgentEntry {
    listed().agents().into_iter().find(|agent| agent.name == name).expect("the agent is listed")
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_string()).collect()
}

#[test]
fn the_agents_are_the_choices_of_the_agent_setting_without_auto() {
    let s = listed();
    let setting = s.describe().into_iter().find(|setting| setting.key == "agent").expect("the agent setting");
    let choices: Vec<String> = setting.choices.into_iter().map(|choice| choice.value).filter(|v| v != "auto").collect();
    let names: Vec<String> = s.agents().into_iter().map(|agent| agent.name).collect();
    assert_eq!(names, choices);
    assert_eq!(names.last().map(String::as_str), Some("mine"));
}

#[test]
fn a_known_agent_says_its_title_models_and_efforts() {
    assert_eq!(
        entry("claude"),
        AgentEntry {
            name: "claude".into(),
            title: Some("Claude Code".into()),
            reads_status: true,
            flows: true,
            models: strings(&["opus", "sonnet", "haiku"]),
            efforts: strings(&["low", "medium", "high", "xhigh", "max"]),
        }
    );
    assert_eq!(entry("codex").efforts.first().map(String::as_str), Some("minimal"));
    assert_eq!(entry("copilot").models, strings(&["auto"]));
}

#[test]
fn an_agent_cornercase_does_not_know_has_no_title_and_nothing_to_offer() {
    for name in ["amp", "mine"] {
        let expected = AgentEntry { name: name.into(), ..AgentEntry::default() };
        assert_eq!(entry(name), expected);
    }
}

#[test]
fn a_flow_refuses_a_role_exactly_when_its_agent_is_listed_without_flows() {
    for agent in listed().agents() {
        let role = Role { name: "a".into(), agent: agent.name.clone(), ..Role::default() };
        let flow = Flow { roles: vec![role], ..Flow::default() };
        let prefix = format!("`a` runs `{}`", agent.name);
        let refused = flow.problems().iter().any(|problem| problem.starts_with(&prefix));
        assert_eq!(refused, !agent.flows, "{}", agent.name);
    }
}
