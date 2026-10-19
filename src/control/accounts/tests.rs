use serde_json::{Value, json};

use super::*;
use crate::accounts::login::State;
use crate::control::contract::{FEATURES, unknown_fields};
use crate::control::{Command, Request};
use crate::test_util::fixture;

fn account(name: &str, unshared: &[&str]) -> AccountInfo {
    AccountInfo {
        name: name.into(),
        path: PathBuf::from(format!("/home/me/.local/state/cornercase/accounts/claude/{name}")),
        unshared: unshared.iter().map(ToString::to_string).collect(),
        login: None,
        ..AccountInfo::default()
    }
}

fn profile(name: &str, claude: &str, active: bool) -> ProfileInfo {
    ProfileInfo { name: name.into(), accounts: BTreeMap::from([("claude".into(), claude.into())]), active }
}

fn answer() -> AccountsInfo {
    AccountsInfo {
        providers: vec![
            ProviderInfo {
                provider: "claude".into(),
                title: "Claude Code".into(),
                env: "CLAUDE_CONFIG_DIR".into(),
                accounts: vec![account("personal", &[]), account("work", &["settings.json"])],
                ..ProviderInfo::default()
            },
            ProviderInfo {
                provider: "codex".into(),
                title: "Codex".into(),
                env: "CODEX_HOME".into(),
                accounts: Vec::new(),
                ..ProviderInfo::default()
            },
        ],
        profiles: vec![profile("Personal", "personal", false), profile("Work", "work", true)],
        profile: "Work".into(),
        login: Some(Ids { project: Some(1), workspace: Some(2), tab: Some(7), pane: Some(8) }),
        tokens: Vec::new(),
    }
}

fn checked() -> AccountsInfo {
    let mut info = AccountsInfo { login: None, tokens: vec!["ANTHROPIC_API_KEY".into()], ..answer() };
    info.providers[0].accounts[0].login = Some(LoginInfo {
        state: State::SignedIn,
        email: Some("me@home.dev".into()),
        plan: Some("pro".into()),
        checked: "2026-10-19T08:00:00.000Z".into(),
    });
    info.providers[0].accounts[1].login =
        Some(LoginInfo { state: State::SignedOut, checked: "2026-10-19T08:00:01.250Z".into(), ..LoginInfo::default() });
    info
}

#[test]
fn an_answer_with_logins_is_written_as_its_fixture_and_reads_back() {
    let written = serde_json::to_string_pretty(&checked()).expect("json");
    let read: AccountsInfo = serde_json::from_str(fixture!("control/accounts/v1/answer-login.json")).expect("answer");

    assert_eq!(written, fixture!("control/accounts/v1/answer-login.json").trim_end());
    assert_eq!(read, checked());
}

#[test]
fn the_check_field_is_always_written_and_logged() {
    let list = Command::AccountList(AccountList { check: true });

    assert_eq!(serde_json::to_value(&list).expect("json")["args"], json!({"check": true}));
    assert!(list.fields().contains(&("check", "true".into())));
    assert!(FEATURES.contains(&"account-login"));
}

#[test]
fn the_answer_is_written_as_its_fixture() {
    let written = serde_json::to_string_pretty(&answer()).expect("json");

    assert_eq!(written, fixture!("control/accounts/v1/answer.json").trim_end());
}

#[test]
fn the_fixture_reads_back_as_its_answer() {
    let read: AccountsInfo = serde_json::from_str(fixture!("control/accounts/v1/answer.json")).expect("answer");

    assert_eq!(read, answer());
}

#[test]
fn an_answer_without_a_login_tab_has_no_login_key() {
    let value = serde_json::to_value(AccountsInfo { login: None, ..answer() }).expect("json");

    assert!(value.get("login").is_none());
}

#[test]
fn every_documented_request_reads_with_no_stray_field() {
    let requests: Vec<Value> = serde_json::from_str(fixture!("control/accounts/v1/requests.json")).expect("requests");

    for raw in requests {
        let parsed: Request = serde_json::from_value(raw.clone()).expect("a request");
        assert_eq!(unknown_fields(&raw, &parsed), None, "{raw}");
    }
}

#[test]
fn the_requests_carry_what_they_say() {
    let parsed = |raw: Value| serde_json::from_value::<Command>(raw).expect("a command");
    let Command::AccountAdd(add) =
        parsed(json!({"command": "account-add", "args": {"provider": "claude", "name": "work", "login": true}}))
    else {
        panic!("not an add")
    };
    assert_eq!((add.provider.as_str(), add.name.as_str(), add.login, add.workspace), ("claude", "work", true, None));
    let Command::Start(start) = parsed(json!({"command": "start", "args": {"profile": "Work"}})) else {
        panic!("not a start")
    };
    assert_eq!(start.profile.as_deref(), Some("Work"));
}

#[test]
fn the_profile_field_is_always_written() {
    for command in [
        Command::Start(crate::control::Start::default()),
        Command::NewTab(crate::control::NewTab::default()),
        Command::FlowRun(crate::control::FlowRun::default()),
    ] {
        let value = serde_json::to_value(&command).expect("json");
        assert_eq!(value["args"].get("profile"), Some(&Value::Null), "{value}");
    }
}

#[test]
fn the_contract_lists_the_feature_and_the_operations() {
    assert!(FEATURES.contains(&"accounts"));
    for name in [
        "account-list",
        "account-add",
        "account-remove",
        "profile-list",
        "profile-new",
        "profile-set",
        "profile-use",
        "profile-delete",
    ] {
        assert!(Command::NAMES.contains(&name), "{name}");
    }
}

#[test]
fn the_logged_fields_name_the_account_and_the_profile() {
    let add =
        Command::AccountAdd(AccountAdd { provider: "claude".into(), name: "work".into(), ..AccountAdd::default() });
    let fields = add.fields();

    assert!(fields.contains(&("provider", "claude".into())) && fields.contains(&("name", "work".into())));
}

fn switched() -> AccountsInfo {
    let mut info = AccountsInfo { login: None, ..answer() };
    info.providers[0].system_label = "Office".into();
    info.providers[0].active = "personal".into();
    info.providers[0].accounts[0].label = "Personal".into();
    info.profiles.iter_mut().for_each(|profile| profile.active = false);
    info.profile = String::new();
    info
}

#[test]
fn an_answer_with_names_and_the_account_in_use_is_written_as_its_fixture_and_reads_back() {
    let written = serde_json::to_string_pretty(&switched()).expect("json");
    let read: AccountsInfo = serde_json::from_str(fixture!("control/accounts/v1/answer-switch.json")).expect("answer");

    assert_eq!(written, fixture!("control/accounts/v1/answer-switch.json").trim_end());
    assert_eq!(read, switched());
}

#[test]
fn renaming_and_using_an_account_read_with_no_stray_field_and_log_what_they_name() {
    for raw in [
        json!({"command": "account-rename", "args": {"provider": "claude", "account": "", "label": "work"}}),
        json!({"command": "account-use", "args": {"provider": "claude", "account": "home"}}),
    ] {
        let parsed: Request = serde_json::from_value(raw.clone()).expect("a request");
        assert_eq!(unknown_fields(&raw, &parsed), None, "{raw}");
        assert!(parsed.command.fields().contains(&("provider", "claude".into())), "{raw}");
    }
    assert!(FEATURES.contains(&"account-switch"));
    assert!(Command::NAMES.contains(&"account-rename") && Command::NAMES.contains(&"account-use"));
}
