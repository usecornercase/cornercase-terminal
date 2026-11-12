use super::*;

struct Setup {
    app: App,
    rx: Receiver<AppEvent>,
    _dir: TempDir,
}

fn two_models() -> Vec<String> {
    vec!["deepseek/deepseek-v4-flash".to_string(), "deepseek/deepseek-v4-pro".to_string()]
}

fn setup() -> Setup {
    let dir = TempDir::new();
    let config = Config { agent_commands: [("mine".to_string(), "/bin/true".to_string())].into(), ..Config::default() };
    let (mut app, rx) = new_app(write_config(dir.path(), &config));
    app.agents.set_catalogue(two_models);
    Setup { app, rx, _dir: dir }
}

#[test]
fn every_agent_config_lists_is_answered_at_once_exactly_when_it_has_no_catalogue() {
    let mut s = setup();
    let described = ok(&mut s.app, "config", &json!({"action": "get"}));
    let listed = described["agents"].as_array().expect("the agents").clone();
    assert!(listed.iter().any(|agent| agent["name"] == "mine"), "{listed:?}");
    assert!(listed.iter().any(|agent| agent["catalogue"] == json!(true)), "{listed:?}");
    for agent in &listed {
        let name = agent["name"].as_str().expect("a name");
        request_json(&mut s.app, "agent-models", &json!({"agent": name}));
        let now = s.app.take_answers();
        if agent["catalogue"] == json!(false) {
            assert_eq!(now.len(), 1, "{name}");
            let expected = json!({"agent": name, "models": agent["models"]});
            assert_eq!(reply_of(&now[0].1).expect("answered"), expected, "{name}");
        } else {
            assert!(now.is_empty(), "{name}");
            let mut models = agent["models"].as_array().expect("models").clone();
            models.extend(two_models().into_iter().map(Value::from));
            let expected = json!({"agent": name, "models": models});
            assert_eq!(reply_later(&mut s.app, &s.rx).expect("answered"), expected, "{name}");
        }
    }
}

#[test]
fn opencode_is_answered_once_its_catalogue_is_read() {
    let mut s = setup();
    request_json(&mut s.app, "agent-models", &json!({"agent": "opencode"}));
    assert_eq!(s.app.take_answers().len(), 0);
    let reply = reply_later(&mut s.app, &s.rx).expect("answered");
    let models = ["deepseek/deepseek-v4-flash", "deepseek/deepseek-v4-pro"];
    assert_eq!(reply, json!({"agent": "opencode", "models": models}));
}

#[test]
fn claude_is_answered_with_its_aliases_and_an_agent_cornercase_only_starts_with_none() {
    let mut s = setup();
    let claude = ok(&mut s.app, "agent-models", &json!({"agent": "claude"}));
    assert_eq!(claude, json!({"agent": "claude", "models": ["opus", "sonnet", "haiku"]}));
    for name in ["amp", "mine"] {
        let other = ok(&mut s.app, "agent-models", &json!({"agent": name}));
        assert_eq!(other, json!({"agent": name, "models": []}));
    }
}

#[test]
fn an_agent_the_setting_cannot_name_is_not_found() {
    let mut s = setup();
    let error = refused(&mut s.app, "agent-models", &json!({"agent": "nope"}));
    assert_eq!(error.code, "not-found");
    assert!(error.message.contains("unknown agent `nope`") && error.message.contains("mine"), "{}", error.message);
}

#[test]
fn a_request_without_an_agent_is_malformed() {
    let mut s = setup();
    assert_eq!(refused(&mut s.app, "agent-models", &json!({})).code, "malformed");
}
