use rstest::rstest;
use serde_json::json;

use super::*;

const SHOP: &str = "github.com/acme/web-shop";

fn shop_with(setup: &str) -> Shared {
    Shared {
        repositories: vec![Repository { remotes: vec![SHOP.into()], setup: setup.into(), teardown: String::new() }],
        ..Shared::default()
    }
}

fn with_setting(key: &str, value: Value) -> Shared {
    let mut shared = Shared::default();
    shared.settings.insert(key.into(), value);
    shared
}

#[rstest]
#[case::exported_token(
    "export NPM_TOKEN=abc123",
    Some("setup of github.com/acme/web-shop sets NPM_TOKEN: check no secret goes with it")
)]
#[case::api_key_assignment(
    "ANTHROPIC_API_KEY=sk-ant-xyz claude",
    Some("setup of github.com/acme/web-shop sets ANTHROPIC_API_KEY: check no secret goes with it")
)]
#[case::password_colon(
    "password: hunter2",
    Some("setup of github.com/acme/web-shop sets password: check no secret goes with it")
)]
#[case::flag_with_value(
    "--api-key sk-123",
    Some("setup of github.com/acme/web-shop passes --api-key: check no secret goes with it")
)]
#[case::flag_with_equals(
    "--token=abc",
    Some("setup of github.com/acme/web-shop passes --token: check no secret goes with it")
)]
#[case::bearer(
    "Authorization: Bearer abcdefghijklmnopqrstuvwxyz",
    Some("setup of github.com/acme/web-shop holds a Bearer token: check no secret goes with it")
)]
#[case::reference_not_a_value("export NPM_TOKEN=$NPM_TOKEN", None)]
#[case::empty_value("NPM_TOKEN=", None)]
#[case::quoted_empty_value("NPM_TOKEN=\"\"", None)]
#[case::flag_without_value("--token --verbose", None)]
#[case::short_bearer("Bearer abc", None)]
#[case::plain_command("npm ci && cp .env.example .env", None)]
#[case::word_in_prose("use the key from 1password", None)]
fn setup_text(#[case] setup: &str, #[case] expected: Option<&str>) {
    let found = warnings(&shop_with(setup));
    match expected {
        Some(warning) => assert_eq!(found, vec![warning.to_string()]),
        None => assert!(found.is_empty(), "{found:?}"),
    }
}

#[test]
fn agent_args_are_scanned_by_agent() {
    let shared = with_setting("agent_args", json!({"claude": ["--api-key", "x"], "codex": ["--model", "o3"]}));
    assert_eq!(warnings(&shared), vec!["agent_args of claude passes --api-key: check no secret goes with it"]);
}

#[test]
fn agent_commands_are_scanned_by_agent() {
    let shared = with_setting("agent_commands", json!({"claude": "API_KEY=abc claude"}));
    assert_eq!(warnings(&shared), vec!["agent_commands of claude sets API_KEY: check no secret goes with it"]);
}

#[test]
fn a_plain_string_setting_is_named_by_its_key() {
    let shared = with_setting("prompt", json!("Authorization: Bearer abcdefghijklmnopqrstuvwxyz"));
    assert_eq!(warnings(&shared), vec!["prompt holds a Bearer token: check no secret goes with it"]);
}

#[test]
fn teardown_is_scanned_and_named() {
    let mut shared = shop_with("");
    shared.repositories[0].teardown = "export DB_PASSWORD=x".into();
    assert_eq!(
        warnings(&shared),
        vec!["teardown of github.com/acme/web-shop sets DB_PASSWORD: check no secret goes with it"]
    );
}

#[test]
fn a_repeated_name_in_one_place_warns_once() {
    let found = warnings(&shop_with("TOKEN=a\nTOKEN=b"));
    assert_eq!(found.len(), 1, "{found:?}");
}

#[test]
fn a_warning_never_holds_the_value() {
    let found = warnings(&shop_with("export NPM_TOKEN=abc123"));
    assert_eq!(found.len(), 1);
    assert!(found.iter().all(|warning| !warning.contains("abc123")), "{found:?}");
}

#[test]
fn settings_without_text_do_not_warn() {
    let shared = with_setting("fetch_minutes", json!(9));
    assert_eq!(warnings(&shared), Vec::<String>::new());
}
