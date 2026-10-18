use std::collections::BTreeMap;

use super::*;
use crate::agents::Known;
use crate::control::AccountsInfo;

const LOGIN: &str = r#"#!/bin/sh
printf '%s|%s\n' "$*" "${CLAUDE_CONFIG_DIR:-system}" > OUT.tmp && mv OUT.tmp OUT
"#;

struct Setup {
    app: App,
    rx: Receiver<AppEvent>,
    config: TempDir,
    _folder: TempDir,
}

fn setup() -> Setup {
    let (config, folder) = (TempDir::new(), TempDir::new());
    let (app, rx) = app_in(folder.path(), config.path().join("config.json"));
    Setup { app, rx, config, _folder: folder }
}

impl Setup {
    fn root(&self) -> PathBuf {
        self.config.path().join("accounts")
    }

    fn info(&mut self, command: &str, args: &Value) -> AccountsInfo {
        serde_json::from_value(ok(&mut self.app, command, args)).expect("the accounts")
    }

    fn add(&mut self, provider: &str, name: &str) -> AccountsInfo {
        self.info("account-add", &json!({"provider": provider, "name": name}))
    }

    fn profile(&mut self, name: &str, accounts: &Value) -> AccountsInfo {
        self.info("profile-new", &json!({"name": name, "accounts": accounts}))
    }

    fn saved(&self) -> Value {
        let text = std::fs::read_to_string(self.config.path().join("config.json")).expect("config.json");
        serde_json::from_str(&text).expect("json")
    }
}

fn names(info: &AccountsInfo, provider: &str) -> Vec<String> {
    let found = info.providers.iter().find(|p| p.provider == provider).expect("the provider");
    found.accounts.iter().map(|a| a.name.clone()).collect()
}

fn chosen(info: &AccountsInfo, profile: &str) -> BTreeMap<String, String> {
    info.profiles.iter().find(|p| p.name == profile).map(|p| p.accounts.clone()).expect("the profile")
}

#[test]
fn an_account_is_added_listed_and_refused_twice() {
    let mut s = setup();

    let added = s.add("claude", "work");

    let dir = s.root().join("claude").join("work");
    assert!(dir.is_dir());
    assert_eq!(names(&added, "claude"), ["work"]);
    assert_eq!(added.login, None);
    let listed = s.info("account-list", &json!({}));
    assert_eq!(names(&listed, "claude"), ["work"]);
    let providers: Vec<&str> = listed.providers.iter().map(|p| p.provider.as_str()).collect();
    assert_eq!(providers, ["claude", "codex", "gemini"]);
    let claude = &listed.providers[0];
    assert_eq!(
        (claude.env.as_str(), claude.accounts[0].path.canonicalize().ok()),
        ("CLAUDE_CONFIG_DIR", dir.canonicalize().ok())
    );
    assert_eq!(refused(&mut s.app, "account-add", &json!({"provider": "claude", "name": "work"})).code, "conflict");
}

#[test]
fn a_new_account_links_the_settings_of_the_default_home_it_is_given() {
    let mut s = setup();
    let homes = TempDir::new();
    std::fs::create_dir_all(homes.path().join("claude")).expect("the default home");
    std::fs::write(homes.path().join("claude").join("settings.json"), "{}").expect("settings");
    s.app.set_default_homes(homes.path());

    let added = s.add("claude", "work");

    let link = s.root().join("claude").join("work").join("settings.json");
    let target = std::fs::read_link(&link).expect("a link");
    assert_eq!(target, homes.path().join("claude").join("settings.json"));
    assert_eq!(added.providers[0].accounts[0].unshared, Vec::<String>::new());
    std::fs::remove_file(&link).expect("unlink");
    std::fs::write(&link, "{}").expect("a file of its own");
    assert_eq!(s.info("account-list", &json!({})).providers[0].accounts[0].unshared, ["settings.json"]);
}

#[rstest::rstest]
#[case::an_agent_without_accounts(json!({"provider": "opencode", "name": "work"}), "malformed")]
#[case::an_unknown_agent(json!({"provider": "nano", "name": "work"}), "malformed")]
#[case::a_bad_name(json!({"provider": "claude", "name": "../work"}), "malformed")]
#[case::an_empty_name(json!({"provider": "claude", "name": ""}), "malformed")]
fn a_bad_account_is_refused_before_anything_is_made(#[case] args: Value, #[case] code: &str) {
    let mut s = setup();

    let refusal = refused(&mut s.app, "account-add", &args);

    assert_eq!(refusal.code, code, "{}", refusal.message);
    assert!(!s.root().exists());
}

#[test]
fn adding_with_login_opens_a_tab_of_that_account_running_the_agent_s_login() {
    let mut s = setup();
    let bin = TempDir::new();
    let out = bin.path().join("out");
    let fake = bin.path().join("claude");
    write_executable(&fake, &LOGIN.replace("OUT", &out.display().to_string()));
    s.app.config.agent_commands.insert("claude".into(), fake.display().to_string());
    let before = s.app.focus();

    let added = s.info("account-add", &json!({"provider": "claude", "name": "work", "login": true}));

    let login = added.login.expect("the login tab");
    let id = login.pane.expect("its pane");
    assert_eq!(pane(&s.app, id).accounts().name(Known::Claude), Some("work"));
    assert_eq!(s.app.focus(), before, "the window stays where it was");
    wait_until("the login command runs", || {
        drain(&mut s.app, &s.rx);
        s.app.refresh(Instant::now());
        out.exists()
    });
    let work = s.root().join("claude").join("work").canonicalize().expect("the account");
    assert_eq!(std::fs::read_to_string(&out).expect("the log").trim_end(), format!("auth login|{}", work.display()));
}

#[test]
fn removing_an_account_takes_it_out_of_every_profile() {
    let mut s = setup();
    s.add("claude", "work");
    s.add("codex", "work");
    s.profile("Work", &json!({"claude": "work", "codex": "work"}));

    let left = s.info("account-remove", &json!({"provider": "claude", "name": "work"}));

    assert!(!s.root().join("claude").join("work").exists());
    assert_eq!(names(&left, "claude"), Vec::<String>::new());
    assert_eq!(chosen(&left, "Work"), BTreeMap::from([("codex".into(), "work".into())]));
    assert_eq!(s.saved()["profiles"]["Work"], json!({"codex": "work"}));
    assert_eq!(refused(&mut s.app, "account-remove", &json!({"provider": "claude", "name": "work"})).code, "not-found");
}

#[test]
fn removing_an_account_keeps_its_folder_while_config_json_cannot_be_read() {
    let mut s = setup();
    s.add("claude", "work");
    s.profile("Work", &json!({"claude": "work"}));
    std::fs::write(s.config.path().join("config.json"), "{ not json").expect("break config.json");

    let refusal = refused(&mut s.app, "account-remove", &json!({"provider": "claude", "name": "work"}));

    assert_eq!(refusal.code, "conflict");
    assert!(refusal.message.starts_with("config.json cannot be read"), "{}", refusal.message);
    assert!(s.root().join("claude").join("work").is_dir());
}

#[test]
fn a_profile_is_refused_as_a_conflict_while_config_json_cannot_be_read() {
    let mut s = setup();
    s.add("claude", "work");
    s.profile("Work", &json!({"claude": "work"}));
    std::fs::write(s.config.path().join("config.json"), "{ not json").expect("break config.json");

    for (command, args) in [
        ("profile-new", json!({"name": "Home", "accounts": {}})),
        ("profile-set", json!({"name": "Work", "rename": "Job"})),
        ("profile-use", json!({"name": "Work"})),
        ("profile-delete", json!({"name": "Work"})),
    ] {
        let refusal = refused(&mut s.app, command, &args);
        assert_eq!(refusal.code, "conflict", "{command}");
        assert!(refusal.message.starts_with("config.json cannot be read"), "{command}: {}", refusal.message);
    }
}

#[test]
fn a_split_after_its_account_was_removed_starts_on_the_system_login() {
    let mut s = setup();
    s.add("claude", "work");
    s.profile("Work", &json!({"claude": "work"}));
    s.info("profile-use", &json!({"name": "Work"}));
    let tab: Done = serde_json::from_value(ok(&mut s.app, "new-tab", &json!({}))).expect("done");
    let first = tab.ids.pane.expect("a pane");
    assert_eq!(pane(&s.app, first).accounts().name(Known::Claude), Some("work"));
    s.info("account-remove", &json!({"provider": "claude", "name": "work"}));

    let split: Done = serde_json::from_value(ok(&mut s.app, "split", &json!({"pane": first}))).expect("done");

    let second = split.ids.pane.expect("the new pane");
    assert!(pane(&s.app, second).accounts().is_empty());
    assert!(!s.root().join("claude").join("work").exists());
}

#[test]
fn a_profile_is_made_changed_renamed_and_deleted() {
    let mut s = setup();
    s.add("claude", "work");
    s.add("codex", "work");

    let made = s.profile("Work", &json!({"claude": "work"}));
    assert_eq!(chosen(&made, "Work"), BTreeMap::from([("claude".into(), "work".into())]));
    assert_eq!(made.profile, "");

    let set = s.info("profile-set", &json!({"name": "Work", "provider": "codex", "account": "work"}));
    assert_eq!(chosen(&set, "Work").len(), 2);
    let cleared = s.info("profile-set", &json!({"name": "Work", "provider": "claude"}));
    assert_eq!(chosen(&cleared, "Work"), BTreeMap::from([("codex".into(), "work".into())]));

    s.info("profile-use", &json!({"name": "Work"}));
    let renamed = s.info("profile-set", &json!({"name": "Work", "rename": "Job"}));
    assert_eq!((renamed.profile.as_str(), renamed.profiles.len()), ("Job", 1));
    assert!(renamed.profiles[0].active);
    assert_eq!(s.saved()["profile"], json!("Job"));

    let deleted = s.info("profile-delete", &json!({"name": "Job"}));
    assert_eq!(deleted.profiles.len(), 0);
    assert_eq!(deleted.profile, "");
    assert_eq!(s.app.config.profile, "");
}

#[rstest::rstest]
#[case::a_new_profile_that_exists("profile-new", json!({"name": "Work"}), "conflict")]
#[case::a_blank_name("profile-new", json!({"name": "  "}), "malformed")]
#[case::the_system_s_name("profile-new", json!({"name": "System"}), "malformed")]
#[case::renaming_to_the_system_s_name("profile-set", json!({"name": "Work", "rename": "system"}), "malformed")]
#[case::an_account_that_does_not_exist("profile-new", json!({"name": "New", "accounts": {"claude": "home"}}), "not-found")]
#[case::an_agent_without_accounts("profile-new", json!({"name": "New", "accounts": {"opencode": "x"}}), "malformed")]
#[case::setting_an_unknown_profile("profile-set", json!({"name": "Gone", "rename": "x"}), "not-found")]
#[case::setting_nothing("profile-set", json!({"name": "Work"}), "malformed")]
#[case::an_account_without_its_agent("profile-set", json!({"name": "Work", "account": "work"}), "malformed")]
#[case::renaming_onto_another("profile-set", json!({"name": "Work", "rename": "Home"}), "conflict")]
#[case::using_an_unknown_profile("profile-use", json!({"name": "Gone"}), "not-found")]
#[case::deleting_an_unknown_profile("profile-delete", json!({"name": "Gone"}), "not-found")]
fn a_bad_profile_request_is_refused_with_its_code(#[case] command: &str, #[case] args: Value, #[case] code: &str) {
    let mut s = setup();
    s.add("claude", "work");
    s.profile("Work", &json!({"claude": "work"}));
    s.profile("Home", &json!({}));
    let saved = s.saved();

    let refusal = refused(&mut s.app, command, &args);

    assert_eq!(refusal.code, code, "{}", refusal.message);
    assert_eq!(s.saved(), saved);
}

#[test]
fn using_a_profile_raises_the_settings_revision_and_feeds_the_app() {
    let mut s = setup();
    s.add("claude", "work");
    s.profile("Work", &json!({"claude": "work"}));
    let before = s.app.config_file.revision;
    assert_eq!(s.app.app_state().profile, None);

    let used = s.info("profile-use", &json!({"name": "Work"}));

    assert_eq!(used.profile, "Work");
    assert!(s.app.config_file.revision > before);
    assert_eq!(s.app.app_state().profile.as_deref(), Some("Work"));
    let after = s.app.config_file.revision;
    s.info("profile-use", &json!({"name": ""}));
    assert!(s.app.config_file.revision > after);
    assert_eq!(s.app.app_state().profile, None);
}

#[test]
fn start_with_a_profile_uses_its_accounts_and_leaves_the_active_one() {
    let mut s = setup();
    s.add("claude", "work");
    s.profile("Work", &json!({"claude": "work"}));
    let start = |profile: &str| json!({"agent": "claude", "profile": profile});

    let text = json!({"contract": 1, "command": "start", "args": start("Work")}).to_string();
    s.app.request(CLIENT, &text, Some(AREA), Instant::now());

    let tab = s.app.projects[0].workspaces[0].tabs.last().expect("the agent's tab");
    let id = tab.pane().expect("its pane").id;
    assert_eq!(pane(&s.app, id).accounts().name(Known::Claude), Some("work"));
    assert_eq!(s.app.config.profile, "");
    assert_eq!(refused(&mut s.app, "start", &start("Gone")).code, "not-found");
}

#[test]
fn a_new_tab_with_the_system_profile_gets_no_account() {
    let mut s = setup();
    s.add("claude", "work");
    s.profile("Work", &json!({"claude": "work"}));
    s.info("profile-use", &json!({"name": "Work"}));

    let system: Done = serde_json::from_value(ok(&mut s.app, "new-tab", &json!({"profile": ""}))).expect("done");
    let active: Done = serde_json::from_value(ok(&mut s.app, "new-tab", &json!({}))).expect("done");

    assert!(pane(&s.app, system.ids.pane.expect("a pane")).accounts().is_empty());
    assert_eq!(pane(&s.app, active.ids.pane.expect("a pane")).accounts().name(Known::Claude), Some("work"));
    assert_eq!(refused(&mut s.app, "new-tab", &json!({"profile": "Gone"})).code, "not-found");
}

#[test]
fn the_contract_lists_the_accounts_feature_and_operations() {
    let mut s = setup();

    let descriptor = ok(&mut s.app, "contract", &json!({}));

    let features = descriptor["features"].as_array().expect("features");
    assert!(features.contains(&json!("accounts")));
    let operations: Vec<&str> =
        descriptor["operations"].as_array().expect("operations").iter().filter_map(|o| o["name"].as_str()).collect();
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
        assert!(operations.contains(&name), "{name}");
    }
}
