use super::issue_list::{
    Setup, accounts_with_linear, assert_only_linear, browser, enter, loaded, open_list, secrets_file, setup, shown,
    to_tab,
};
use super::*;
use crate::issues::browser::Step;
use crate::issues::secrets;
use crate::test_util::FakeHttp;

const ONE: &str = r#"{"data":{"gid":"u-me","name":"Ana","workspaces":[{"gid":"w-1","name":"Acme"}]}}"#;
const TWO: &str = r#"{"data":{"gid":"u-me","name":"Ana",
    "workspaces":[{"gid":"w-1","name":"Acme"},{"gid":"w-2","name":"Beta"}]}}"#;
const USERS: &str = r#"{"data":[{"gid":"u-me","name":"Ana","email":"ana@acme.dev"}],"next_page":null}"#;
const TASKS: &str = r#"{"data":[{"gid":"1","name":"Login redirect loops","completed":false,
    "modified_at":"2026-09-30T00:00:00.000Z","resource_subtype":"default_task","created_by":{"gid":"u-me"},
    "custom_fields":[{"representation_type":"custom_id","display_value":"ENG-42"}]}],"next_page":null}"#;

fn asana(s: &mut Setup, me: &'static str) -> FakeHttp {
    let server = FakeHttp::start(vec![
        ("GET /users/me", 200, me),
        ("GET /workspaces/w-1/users", 200, USERS),
        ("GET /workspaces/w-2/users", 200, USERS),
        ("GET /tasks", 200, TASKS),
    ]);
    s.app.issues.api_at(Source::Asana, &server.url());
    server
}

fn type_token(s: &mut Setup, token: &str) {
    open_list(s);
    to_tab(s, IssueTab::One(Source::Asana));
    type_text(&mut s.app, token);
    enter(s);
}

fn at_workspace_step(app: &App) -> bool {
    matches!(&app.overlay, Some(Overlay::Issues(b)) if b.form_step(Source::Asana) == Step::Workspace)
}

#[test]
fn a_token_for_one_workspace_connects_at_once_and_lists() {
    let mut s = setup(false, "echo '[]'");
    let server = asana(&mut s, ONE);

    type_token(&mut s, "t0k");

    pump_until(&mut s.app, &s.rx, "the issues load", |a| loaded(a, Source::Asana));
    assert_eq!(shown(&s.app), ["ENG-42"]);
    assert_eq!(secrets::read(&secrets_file(&s), "asana_token").as_deref(), Some("t0k"));
    assert!(server.request(0).to_lowercase().contains("authorization: bearer t0k"), "{}", server.request(0));
}

#[test]
fn a_token_for_several_workspaces_asks_which_and_saves_the_choice() {
    let mut s = setup(false, "echo '[]'");
    let _server = asana(&mut s, TWO);

    type_token(&mut s, "t0k");
    pump_until(&mut s.app, &s.rx, "the workspace step", at_workspace_step);
    assert_eq!(secrets::read(&secrets_file(&s), "asana_token"), None);
    type_text(&mut s.app, "beta");
    enter(&mut s);

    pump_until(&mut s.app, &s.rx, "the issues load", |a| loaded(a, Source::Asana));
    assert_eq!(shown(&s.app), ["ENG-42"]);
    assert_eq!(config::load(&s.config.path().join("config.json")).asana_workspace, "w-2");
    assert_eq!(secrets::read(&secrets_file(&s), "asana_token").as_deref(), Some("t0k"));
}

#[test]
fn a_workspace_that_is_not_the_tokens_stays_in_the_step() {
    let mut s = setup(false, "echo '[]'");
    let _server = asana(&mut s, TWO);
    type_token(&mut s, "t0k");
    pump_until(&mut s.app, &s.rx, "the workspace step", at_workspace_step);

    type_text(&mut s.app, "gamma");
    enter(&mut s);

    let error = browser(&s.app).forms.get(&Source::Asana).and_then(|f| f.error.clone());
    assert_eq!(error.as_deref(), Some("no Asana workspace named gamma; yours are Acme, Beta"));
    assert!(at_workspace_step(&s.app));
    assert_eq!(secrets::read(&secrets_file(&s), "asana_token"), None);
}

#[test]
fn a_token_from_the_environment_lists_without_a_form() {
    let mut s = setup(false, "echo '[]'");
    let _server = asana(&mut s, ONE);
    s.app.issues.set_env_token(Source::Asana, "env-token");

    open_list(&mut s);
    to_tab(&mut s, IssueTab::One(Source::Asana));

    pump_until(&mut s.app, &s.rx, "the issues load", |a| loaded(a, Source::Asana));
    assert_eq!(shown(&s.app), ["ENG-42"]);
}

fn forgets_the_account_when(change: impl Fn(Config) -> Config) {
    let mut s = setup(false, "echo '[]'");
    accounts_with_linear(&mut s, Source::Asana, "Acme");

    s.app.set_config(change(s.app.config.clone()));

    assert_only_linear(&s);
}

#[test]
fn another_workspace_forgets_the_account_of_the_old_one() {
    forgets_the_account_when(|config| Config { asana_workspace: "other".into(), ..config });
}

#[test]
fn other_projects_forget_the_account_of_the_old_ones() {
    forgets_the_account_when(|config| Config { asana_projects: "123".into(), ..config });
}
