use std::collections::HashMap;

use super::*;
use crate::app::issues_dialog;
use crate::app::machines::checks::Checked;
use crate::control::TabInfo;
use crate::issues::browser::{Accounts, AgentNote, Browser, Screen};
use crate::issues::{Issue, Source};

const SHOP: &str = "github.com/acme/web-shop";
const WIDE: Rect = Rect { x: 0, y: 0, width: 120, height: 30 };
const TAB: u64 = 9;

fn there(command: &str) -> Value {
    match command {
        "account-list" => json!({
            "providers": [{ "provider": "claude", "active": "work",
                            "accounts": [{ "name": "work", "login": { "state": "signed-out" } }, { "name": "home" }] }],
            "agents": [{ "agent": "claude", "command": "claude", "installed": true }],
        }),
        "issue-sources" => json!({
            "project": 1,
            "sources": [{ "source": "github", "name": "GitHub", "state": "missing", "missing": ["token"] }],
        }),
        _ => json!({}),
    }
}

fn ready(command: &str) -> Structured {
    let answer = match command {
        "account-list" => json!({
            "providers": [{ "provider": "claude", "active": "work", "accounts": [{ "name": "work" }, { "name": "home" }] }],
            "agents": [{ "agent": "claude", "command": "claude", "installed": true }],
        }),
        "issue-sources" => {
            json!({ "project": 1, "sources": [{ "source": "github", "name": "GitHub", "state": "connected" }] })
        }
        "start" => json!({ "workspace": WORKSPACE, "tab": TAB }),
        _ => json!({}),
    };
    Structured::Ok(answer)
}

fn refusing(command: &str) -> Structured {
    match command {
        "start" => Structured::Error(Refusal::new(Code::Failed, "the worktree on vps.lan is in use")),
        command => ready(command),
    }
}

fn rambling_start(command: &str) -> Structured {
    match command {
        "start" => Structured::Error(Refusal::new(
            Code::Failed,
            "the worktree is in use\nby the run that started on Monday morning and is still at it",
        )),
        command => ready(command),
    }
}

fn rambling_tracker(command: &str) -> Structured {
    match command {
        "issue-sources" => Structured::Error(Refusal::new(
            Code::Failed,
            "gh failed:\r\nexit status 1 because the token expired on Monday and nobody renewed it",
        )),
        command => ready(command),
    }
}

fn no_accounts(command: &str) -> Structured {
    match command {
        "account-list" => Structured::Error(Refusal::new(Code::Failed, "cannot read the accounts")),
        command => ready(command),
    }
}

fn restarted(command: &str) -> Structured {
    match command {
        "start" => Structured::Error(Refusal::new(Code::StaleBoot, format!("this request is for {BOOT}"))),
        command => ready(command),
    }
}

fn opened(command: &str) -> Value {
    match command {
        "new-tab" => json!({ "tab": TAB }),
        command => there(command),
    }
}

fn shop(projects: Vec<ProjectInfo>) -> control::Report {
    control::Report { boot: BOOT.into(), home: Some("/home/ana".into()), projects, ..control::Report::default() }
}

fn web_shop() -> ProjectInfo {
    ProjectInfo {
        id: 1,
        name: "web-shop".into(),
        path: PathBuf::from("/home/ana/code/web-shop"),
        repo: Some(SHOP.into()),
        workspaces: vec![workspace()],
        ..ProjectInfo::default()
    }
}

fn issue() -> Issue {
    Issue {
        source: Source::Github,
        number: 7,
        key: "#7".into(),
        title: "Fix the login".into(),
        url: "https://github.com/acme/web-shop/issues/7".into(),
        state: "open".into(),
        closed: false,
        labels: Vec::new(),
        assignees: Vec::new(),
        author: "ana".into(),
        updated_at: String::new(),
    }
}

struct Reading {
    remote: Remote,
    _dir: TempDir,
}

fn reading(projects: Vec<ProjectInfo>) -> Reading {
    reading_with(Reply::Each(there), projects)
}

fn reading_with(reply: Reply, projects: Vec<ProjectInfo>) -> Reading {
    let mut remote = seeing_in(reply, shop(projects));
    let dir = TempDir::new();
    let path = dir.path().join("web-shop");
    std::fs::create_dir(&path).expect("the project's folder");
    let app = &mut remote.rig.app;
    app.open_project(path, AREA).expect("open the project");
    app.projects[0].repo.identity = Some(SHOP.into());
    app.config.agent = "claude".into();
    app.config.account_picker = true;
    app.open_issues(AREA).expect("open the issues");
    let Some(Overlay::Issues(b)) = &mut app.overlay else { panic!("the issues are not open") };
    b.screen = Screen::Detail { issue: Box::new(issue()), detail: None, scroll: 0, raw: false };
    Reading { remote, _dir: dir }
}

fn dialog(app: &mut App, area: Rect) -> String {
    let shown = overlay_shown(app, area).area(area).expect("the dialog has an area");
    let terminal = render(app, area);
    let cell = |x, y| terminal.backend().buffer()[(x, y)].symbol().to_string();
    let row = |y| (shown.x..shown.right()).map(|x| cell(x, y)).collect::<String>();
    (shown.y..shown.bottom()).map(row).collect::<Vec<_>>().join("\n")
}

fn browser(app: &App) -> &Browser {
    let Some(Overlay::Issues(b)) = &app.overlay else { panic!("the issues are not open") };
    b
}

fn row(app: &App, label: &str) -> Option<String> {
    let lines = browser(app).detail_lines(issues_dialog::room(AREA));
    let found = lines.into_iter().find(|line| line.spans.first().is_some_and(|span| span.content.trim() == label));
    found.map(|line| line.spans.iter().map(|span| span.content.as_ref()).collect())
}

impl Reading {
    fn press(&mut self, label: &str) {
        let app = &mut self.remote.rig.app;
        let labels = browser(app).labels(issues_dialog::room(AREA).buttons);
        let at = labels.iter().position(|shown| *shown == label).expect("the button is shown");
        click(app, ui::issue_buttons(ui::issues_area(AREA), &labels)[at].as_position());
    }

    fn pick(&mut self, button: &str, choice: &str) {
        self.press(button);
        let app = &mut self.remote.rig.app;
        choice.chars().for_each(|c| send_key(app, KeyCode::Char(c), KeyModifiers::NONE));
        send_key(app, KeyCode::Enter, KeyModifiers::NONE);
    }

    fn on_vps(&mut self) {
        self.pick("run on…", "vps");
        self.remote.rig.until("the machine answers the checks", |app| {
            browser(app).there().is_some_and(|there| there.agent != AgentNote::Ready && there.tracker.is_some())
        });
    }

    fn checked(&mut self) {
        self.pick("run on…", "vps");
        self.remote.rig.until("the machine answers the checks", |app| {
            app.accounts_there("vps") != Checked::Waiting && app.tracker_there("vps", 1, "github") != Checked::Waiting
        });
    }

    fn sent(&self, command: &str) -> Option<Value> {
        let asked = self.remote.asked().into_iter();
        asked.filter(|asked| asked["command"] == command).map(|asked| asked["args"].clone()).next_back()
    }

    fn answered(&mut self) {
        self.remote.rig.until("the machine answers", |app| !browser_open(app) || !browser(app).starting);
    }

    fn lists_the_tab(&mut self) {
        let workspace = WorkspaceInfo { tabs: vec![TabInfo { id: TAB, ..TabInfo::default() }], ..workspace() };
        serve(&mut self.remote.theirs, 2, shop(vec![ProjectInfo { workspaces: vec![workspace], ..web_shop() }]));
        self.remote.rig.until("the machine lists the tab", |app| {
            let seen = app.machines.held()[0].seen.as_deref();
            seen.is_some_and(|seen| seen.projects[0].workspaces[0].tabs.iter().any(|tab| tab.id == TAB))
        });
    }

    fn shows_the_tab(&mut self) {
        self.lists_the_tab();
        self.remote
            .rig
            .until("the new tab is selected", |app| app.shown_remote().and_then(|shown| shown.tab) == Some(TAB));
    }
}

fn browser_open(app: &App) -> bool {
    matches!(app.overlay, Some(Overlay::Issues(_)))
}

fn error(app: &App) -> Option<&str> {
    browser(app).error.as_deref()
}

mod the_issues_dialog {
    use super::*;

    #[test]
    fn asks_the_machine_for_its_trackers_and_not_its_flows() {
        let mut reading = reading(vec![web_shop()]);

        reading.checked();

        assert_eq!((reading.sent("issue-sources"), reading.sent("flow-list")), (Some(json!({ "project": 1 })), None));
    }

    #[test]
    fn lists_the_machine_s_accounts_once_it_is_chosen() {
        let mut reading = reading(vec![web_shop()]);

        reading.on_vps();
        reading.press("account…");

        let app = &reading.remote.rig.app;
        let keys: Vec<String> = browser(app).choices().into_iter().map(|choice| choice.key).collect();
        assert_eq!(keys, ["System", "work", "home"]);
        assert_eq!(row(app, "account").as_deref(), Some("account work  (in use)  (vps's accounts)"));
    }

    #[test]
    fn says_the_agent_has_no_login_there_and_what_the_tracker_lacks() {
        let mut reading = reading(vec![web_shop()]);

        reading.on_vps();

        let app = &reading.remote.rig.app;
        let agent = row(app, "agent").expect("the agent row");
        assert!(agent.ends_with("  not logged in on vps"), "{agent}");
        let run_on = row(app, "run on").expect("the run on row");
        assert!(run_on.ends_with(" · vps has no GitHub token for web-shop"), "{run_on}");
        assert_eq!(row(app, "start").as_deref(), Some("start   in a tab on vps, in ~/code/web-shop"));
    }

    #[test]
    fn keeps_the_account_picked_there_while_the_machine_is_asked_again() {
        let mut reading = reading(vec![web_shop()]);
        reading.on_vps();
        reading.pick("account…", "home");

        reading.pick("run on…", "vps");

        let chosen = |app: &App| browser(app).accounts.as_ref().and_then(|accounts| accounts.chosen.clone());
        let home = Some(("claude".to_string(), "home".to_string()));
        assert_eq!(chosen(&reading.remote.rig.app), home);
        reading.remote.rig.until("the machine answers again", |app| app.accounts_there("vps") != Checked::Waiting);
        assert_eq!(chosen(&reading.remote.rig.app), home);
    }

    #[test]
    fn drops_an_account_picked_here_once_the_machine_is_chosen() {
        let mut reading = reading(vec![web_shop()]);
        let Some(Overlay::Issues(b)) = &mut reading.remote.rig.app.overlay else { panic!("the issues are not open") };
        let here = Some(("claude".to_string(), "home".to_string()));
        b.accounts = Some(Accounts { agents: HashMap::new(), chosen: here });

        reading.pick("run on…", "vps");

        let chosen = |app: &App| browser(app).accounts.as_ref().and_then(|accounts| accounts.chosen.clone());
        assert_eq!(chosen(&reading.remote.rig.app), None);
        reading.remote.rig.until("the machine answers again", |app| app.accounts_there("vps") != Checked::Waiting);
        assert_eq!(chosen(&reading.remote.rig.app), None);
    }

    #[test]
    fn shows_no_accounts_of_this_computer_when_the_machine_cannot_list_its_own() {
        let mut reading = reading_with(Reply::Told(no_accounts), vec![web_shop()]);
        let Some(Overlay::Issues(b)) = &mut reading.remote.rig.app.overlay else { panic!("the issues are not open") };
        b.accounts = Some(Accounts { agents: HashMap::new(), chosen: Some(("claude".into(), "home".into())) });

        reading.pick("run on…", "vps");
        reading.remote.rig.until("the machine refuses", |app| app.accounts_there("vps") != Checked::Waiting);

        assert!(browser(&reading.remote.rig.app).accounts.is_none());
    }

    #[test]
    fn says_briefly_why_the_machine_cannot_read_the_tracker() {
        let mut reading = reading_with(Reply::Told(rambling_tracker), vec![web_shop()]);

        reading.checked();

        let run_on = row(&reading.remote.rig.app, "run on").expect("the run on row");
        assert!(run_on.ends_with(" · gh failed: exit status 1 because the token expired on Monda…"), "{run_on:?}");
    }

    #[test]
    fn says_an_account_with_a_login_there_is_ready() {
        let mut reading = reading(vec![web_shop()]);
        reading.on_vps();

        reading.pick("account…", "home");

        let app = &reading.remote.rig.app;
        assert_eq!(browser(app).there().map(|there| there.agent), Some(AgentNote::Ready));
    }

    #[test]
    fn goes_back_to_this_computer_s_accounts() {
        let mut reading = reading(vec![web_shop()]);
        reading.on_vps();

        reading.pick("run on…", "this");

        let app = &reading.remote.rig.app;
        assert_eq!((browser(app).machine(), browser(app).there()), (None, None));
        assert_eq!(row(app, "account"), None);
        assert!(!row(app, "start").is_some_and(|start| start.contains("vps")));
    }

    #[test]
    fn offers_to_clone_a_project_that_is_not_there() {
        let mut reading = reading(Vec::new());

        reading.pick("run on…", "vps");
        reading
            .remote
            .rig
            .until("the machine answers its accounts", |app| app.accounts_there("vps") != Checked::Waiting);

        let app = &reading.remote.rig.app;
        assert_eq!(row(app, "start").as_deref(), Some("start   not on vps · clone…"));
        let asked: Vec<Value> = reading.remote.asked().into_iter().map(|asked| asked["command"].clone()).collect();
        assert_eq!(asked, [json!("account-list")]);
    }

    #[test]
    fn shows_where_it_runs_on_the_machine() {
        let mut reading = reading(vec![web_shop()]);

        reading.on_vps();

        insta::assert_snapshot!(dialog(&mut reading.remote.rig.app, WIDE));
    }
}

mod starting_there {
    use super::*;

    #[test]
    fn sends_the_issue_with_the_machine_s_ids_and_the_account_picked() {
        let mut reading = reading_with(Reply::Told(ready), vec![web_shop()]);
        reading.checked();
        reading.pick("account…", "home");

        reading.press("start");

        assert_eq!(browser(&reading.remote.rig.app).starting_there().as_deref(), Some("starting on vps…"));
        reading.answered();
        let sent = reading.sent("start").expect("a start");
        assert_eq!(
            (&sent["project"], &sent["issue"], &sent["source"], &sent["agent"]),
            (&json!(1), &json!("#7"), &json!("github"), &json!("claude"))
        );
        assert_eq!(
            (&sent["accounts"], &sent["focus"], sent.get("machine")),
            (&json!({ "claude": "home" }), &json!(false), None)
        );
    }

    #[test]
    fn closes_the_dialog_and_selects_what_it_started_once_the_machine_shows_it() {
        let mut reading = reading_with(Reply::Told(ready), vec![web_shop()]);
        reading.checked();
        reading.pick("account…", "home");

        reading.press("start");
        reading.answered();

        assert!(!browser_open(&reading.remote.rig.app));
        reading.shows_the_tab();
    }

    #[test]
    fn leaves_alone_what_was_picked_before_the_machine_answered() {
        let mut reading = reading_with(Reply::Told(ready), vec![web_shop()]);
        reading.checked();
        reading.pick("account…", "home");

        reading.press("start");
        reading.remote.rig.app.leave_remote();
        reading.answered();
        reading.lists_the_tab();

        assert!(reading.remote.rig.app.shown_remote().is_none());
    }

    #[rstest]
    #[case::refused(refusing, "the worktree on vps is in use")]
    #[case::restarted(restarted, "vps restarted; nothing was started there")]
    #[case::rambling(rambling_start, "the worktree is in use by the run that started on Monday mo…")]
    fn says_why_the_machine_did_not_start_it(#[case] told: fn(&str) -> Structured, #[case] said: &str) {
        let mut reading = reading_with(Reply::Told(told), vec![web_shop()]);
        reading.checked();
        reading.pick("account…", "home");

        reading.press("start");
        reading.answered();

        let app = &reading.remote.rig.app;
        assert_eq!((error(app), browser(app).starting), (Some(said), false));
    }

    #[test]
    fn sends_nothing_while_the_machine_cannot_read_the_tracker() {
        let mut reading = reading(vec![web_shop()]);
        reading.on_vps();
        reading.pick("account…", "home");

        reading.press("start");

        let app = &reading.remote.rig.app;
        assert_eq!((error(app), browser(app).starting), (Some("vps has no GitHub token for web-shop"), false));
        assert_eq!(reading.sent("start"), None);
    }
}

mod the_login_tab {
    use super::*;

    #[test]
    fn opens_the_agent_s_login_in_the_project_there_and_shows_it() {
        let mut reading = reading_with(Reply::Each(opened), vec![web_shop()]);
        reading.on_vps();

        reading.press("open login tab");
        reading.answered();

        let sent = reading.sent("new-tab").expect("a new tab");
        assert_eq!(
            (&sent["workspace"], &sent["command"], &sent["name"]),
            (&json!(WORKSPACE), &json!("claude auth login"), &json!("claude login"))
        );
        assert!(!browser_open(&reading.remote.rig.app));
        reading.shows_the_tab();
    }
}

mod cloning;
mod new_workspace;
mod start_flow;
