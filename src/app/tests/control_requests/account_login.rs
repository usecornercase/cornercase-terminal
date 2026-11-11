use super::*;
use crate::agents::Known;
use crate::control::AccountsInfo;
use crate::project::Hierarchy;

const SHELL: &str = if cfg!(windows) { "cmd.exe" } else { "/bin/sh" };
const VIEWER: u64 = 31;

struct Logins {
    app: App,
    _rx: Receiver<AppEvent>,
    folders: [TempDir; 3],
}

fn logins() -> Logins {
    let folders = [TempDir::new(), TempDir::new(), TempDir::new()];
    let [config, viewed, windowed] = &folders;
    let (mut app, rx) = new_app_with_shell(SHELL, config.path().join("config.json"));
    app.open_project(viewed.path().to_path_buf(), AREA).expect("the project a view shows");
    app.open_project(windowed.path().to_path_buf(), AREA).expect("the project the window shows");
    ok(&mut app, "account-add", &json!({"provider": "claude", "name": "work"}));
    Logins { app, _rx: rx, folders }
}

fn panes(app: &App) -> usize {
    app.projects.iter().flat_map(|p| &p.workspaces).flat_map(Workspace::terms).count()
}

fn name_of(app: &App, pane: u64) -> Option<String> {
    app.projects.tab_with_pane(pane).and_then(|(_, tab)| tab.name.clone())
}

fn in_view(app: &mut App, view: u64, command: &str, args: &Value) -> std::result::Result<Value, Refused> {
    let text = json!({"contract": 1, "view": view, "command": command, "args": args}).to_string();
    app.request(CLIENT, &text, Some(AREA), Instant::now());
    let mut answers = app.take_answers();
    assert_eq!(answers.len(), 1, "an answer at once");
    reply_of(&answers.remove(0).1)
}

impl Logins {
    fn views_the_first_project(&mut self) -> u64 {
        let project = self.app.projects[0].id;
        open_view(&mut self.app, VIEWER, &json!({"cols": 100, "rows": 30, "project": project}))
    }

    fn login(&mut self, args: &Value) -> Value {
        ok(&mut self.app, "account-login", args)
    }
}

fn info(value: Value) -> AccountsInfo {
    serde_json::from_value(value).expect("the accounts")
}

#[test]
fn an_account_that_exists_opens_its_login_tab_and_moves_nothing() {
    let mut s = logins();
    let before = (s.app.focus(), s.app.active);

    let answer = s.login(&json!({"provider": "claude", "account": "work"}));

    assert!(answer.get("projection").is_none());
    let id = info(answer).login.expect("the login tab").pane.expect("its pane");
    assert_eq!(name_of(&s.app, id).as_deref(), Some("work login"));
    assert_eq!(pane(&s.app, id).accounts().name(Known::Claude), Some("work"));
    assert_eq!(s.app.agents.launches.list().iter().filter(|launch| launch.term == id).count(), 1);
    assert_eq!((s.app.focus(), s.app.active), before);
}

#[test]
fn the_agent_s_own_login_opens_its_tab_without_an_account() {
    let mut s = logins();

    let answer = s.login(&json!({"provider": "claude", "account": ""}));

    let id = info(answer).login.expect("the login tab").pane.expect("its pane");
    assert_eq!(name_of(&s.app, id).as_deref(), Some("claude login"));
    assert!(pane(&s.app, id).accounts().is_empty());
    assert_eq!(s.app.agents.launches.list().iter().filter(|launch| launch.term == id).count(), 1);
}

#[rstest::rstest]
#[case::an_account_that_does_not_exist(json!({"provider": "claude", "account": "nope"}), "not-found", "no claude account")]
#[case::an_unknown_agent(json!({"provider": "nano", "account": "work"}), "malformed", "no accounts")]
#[case::an_agent_without_accounts(json!({"provider": "opencode", "account": ""}), "malformed", "no accounts")]
#[case::an_agent_without_a_login_command(json!({"provider": "gemini", "account": ""}), "malformed", "no login command")]
#[case::an_unknown_workspace(json!({"provider": "claude", "account": "work", "workspace": 9999}), "not-found", "")]
fn a_login_that_cannot_run_is_refused_and_opens_nothing(#[case] args: Value, #[case] code: &str, #[case] said: &str) {
    let mut s = logins();
    let before = (panes(&s.app), s.app.agents.launches.list().len());

    let refusal = refused(&mut s.app, "account-login", &args);

    assert_eq!(refusal.code, code, "{}", refusal.message);
    assert!(refusal.message.contains(said), "{}", refusal.message);
    assert_eq!((panes(&s.app), s.app.agents.launches.list().len()), before);
}

#[test]
fn focused_in_a_view_it_shows_there_lands_in_the_view_s_workspace_and_leaves_the_window() {
    let mut s = logins();
    let view = s.views_the_first_project();
    let before = (s.app.focus(), s.app.active);

    let answer =
        in_view(&mut s.app, view, "account-login", &json!({"provider": "claude", "account": "work", "focus": true}))
            .expect("the login");

    assert_eq!(answer["projection"], 2);
    let login = info(answer).login.expect("the login tab");
    assert_eq!(s.app.view_ids(view).tab, login.tab);
    assert_eq!(login.project, Some(s.app.projects[0].id));
    assert_eq!((s.app.focus(), s.app.active), before);
}

#[test]
fn focused_without_a_view_it_shows_the_window() {
    let mut s = logins();

    let answer = s.login(&json!({"provider": "claude", "account": "work", "focus": true}));

    assert!(answer.get("projection").is_none());
    assert_eq!(s.app.shown_ids(s.app.active).tab, info(answer).login.expect("the login tab").tab);
}

#[test]
fn adding_with_login_focused_in_a_view_shows_the_new_tab_there() {
    let mut s = logins();
    let view = s.views_the_first_project();
    let before = (s.app.focus(), s.app.active);
    let args = json!({"provider": "codex", "name": "home", "login": true, "focus": true});

    let answer = in_view(&mut s.app, view, "account-add", &args).expect("the account");

    assert_eq!(answer["projection"], 2);
    assert_eq!(s.app.view_ids(view).tab, info(answer).login.expect("the login tab").tab);
    assert_eq!((s.app.focus(), s.app.active), before);
    assert!(s.folders[0].path().join("accounts").join("codex").join("home").is_dir());
}

#[test]
fn a_view_that_closed_is_not_found_and_opens_nothing() {
    let mut s = logins();
    let view = s.views_the_first_project();
    s.app.forget(VIEWER);
    let before = (panes(&s.app), s.app.agents.launches.list().len());

    let refusal = in_view(&mut s.app, view, "account-login", &json!({"provider": "claude", "account": "work"}))
        .expect_err("refused");

    assert_eq!(refusal.code, "not-found", "{}", refusal.message);
    assert_eq!((panes(&s.app), s.app.agents.launches.list().len()), before);
}
