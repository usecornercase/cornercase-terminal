use super::*;
use crate::agents::Known;
use crate::flows::dialog;
use crate::flows::start::{Focus, START, Start};
use crate::issues::Source;

const WIDE: Rect = Rect { x: 0, y: 0, width: 150, height: 44 };
const NARROW: Rect = Rect { x: 0, y: 0, width: 90, height: 30 };
const START_FLOW: &str = "start a flow…";

struct Setup {
    app: App,
    rx: Receiver<AppEvent>,
    config: TempDir,
    project: TempDir,
}

fn setup(repo: bool) -> Setup {
    let project = if repo { git_repo(&[("README", "hi")]) } else { TempDir::new() };
    let config = TempDir::new();
    let (mut app, rx) = app_in(project.path(), config.path().join("config.json"));
    app.projects[0].name = Some("web-shop".into());
    app.issues.env_tokens.clear();
    for kind in Known::ALL {
        app.config.agent_commands.insert(kind.name().into(), "/nonexistent/agent".into());
    }
    Setup { app, rx, config, project }
}

fn opened(repo: bool) -> Setup {
    let mut s = setup(repo);
    let new = new_workspace_pos(&s.app);
    click(&mut s.app, new);
    pick(&mut s.app, START_FLOW);
    s
}

fn start(app: &App) -> &Start {
    let Some(Overlay::StartFlow { start, .. }) = &app.overlay else { panic!("the start dialog is not open") };
    start
}

fn view(app: &App, area: Rect) -> ui::StartFlow {
    let ui::Overlay::StartFlow(view) = overlay_shown(app, area) else { panic!("the start dialog is not shown") };
    *view
}

fn button(app: &mut App, area: Rect, label: &str) {
    let view = view(app, area);
    click_button(app, area, &view.buttons, &ui::start_buttons(area, view.shape(), &view.buttons), label);
}

fn type_in(app: &mut App, text: &str, area: Rect) {
    for c in text.chars() {
        app.handle_event(AppEvent::Input(Event::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))), area)
            .expect("key");
    }
}

fn from_issue(s: &mut Setup) {
    s.app.open_issues(WIDE).expect("open the issues");
    let Some(Overlay::Issues(b)) = &mut s.app.overlay else { panic!("no issues") };
    let issue = issues::issue(Source::Github, 7, "Fix the login");
    b.screen = Screen::Detail { issue: Box::new(issue), detail: None, scroll: 0, raw: false };
    let labels = b.labels(usize::MAX);
    let i = labels.iter().position(|l| *l == "start a flow").expect("the button");
    click_in(&mut s.app, ui::issue_buttons(ui::issues_area(WIDE), &labels)[i].as_position(), WIDE);
}

fn click_flow(app: &mut App, name: &str, line: u16) {
    let i = start(app).flows.iter().position(|saved| saved.flow.name == name).expect("the flow is listed");
    let list = ui::start_list(WIDE, view(app, WIDE).shape());
    let y = list.y + 1 + u16::try_from(i).expect("few flows") * 2 + line;
    click_in(app, Position::new(list.x + 4, y), WIDE);
}

fn selected(app: &App) -> String {
    start(app).selected().map(|saved| saved.flow.name.clone()).unwrap_or_default()
}

mod entries {
    use super::*;

    #[test]
    fn the_workspaces_new_menu_starts_a_flow_in_their_project() {
        let s = opened(true);
        let start = start(&s.app);
        assert_eq!((start.project, start.focus, start.repo), (s.app.projects[0].id, Focus::Prompt, true));
    }

    #[test]
    fn the_workspaces_new_offers_a_workspace_or_a_flow_and_the_projects_new_no_flow() {
        let mut s = setup(true);
        let new = new_workspace_pos(&s.app);
        click(&mut s.app, new);
        assert_eq!(menu_labels(&s.app), ["new workspace", START_FLOW]);
        pick(&mut s.app, "new workspace");
        assert!(matches!(s.app.overlay, Some(Overlay::NewWorkspace { .. })));
        s.app.overlay = None;
        let new = new_project_pos(&s.app);
        click(&mut s.app, new);
        assert_eq!(menu_labels(&s.app), ["open project", "new group"]);
    }

    #[test]
    fn the_keys_menu_starts_one_for_the_project_on_screen() {
        let mut s = setup(true);
        s.app.config.prefix_key = "ctrl+]".into();
        send_key(&mut s.app, KeyCode::Char(']'), KeyModifiers::CONTROL);
        type_text(&mut s.app, "ws");
        assert_eq!(start(&s.app).project, s.app.projects[0].id);
    }

    #[test]
    fn an_issue_starts_one_with_its_prompt_and_branch_and_cancel_goes_back_to_it() {
        let mut s = setup(true);
        from_issue(&mut s);
        let start = start(&s.app);
        assert_eq!(start.prompt.text(), "https://github.com/acme/shop/issues/7");
        assert_eq!(
            (start.branch.as_str(), start.title()),
            ("issue-7-fix-the-login", "Start a flow › #7 Fix the login".into())
        );
        button(&mut s.app, WIDE, dialog::CANCEL);
        assert!(matches!(s.app.overlay, Some(Overlay::Issues(_))));
    }

    #[test]
    fn the_issues_list_starts_one_for_the_selected_issue_and_cancel_starts_nothing() {
        let mut s = setup(true);
        s.app.open_issues(WIDE).expect("open the issues");
        let Some(Overlay::Issues(b)) = &mut s.app.overlay else { panic!("no issues") };
        let query = b.query(Source::Github);
        b.loaded(Source::Github, &query, Ok(vec![issues::issue(Source::Github, 7, "Fix the login")]));
        assert_eq!(b.shown().iter().map(|i| i.key.as_str()).collect::<Vec<_>>(), ["#7"]);
        let labels = b.labels(usize::MAX);
        let i = labels.iter().position(|l| *l == "start a flow").expect("the list offers start a flow");
        click_in(&mut s.app, ui::issue_buttons(ui::issues_area(WIDE), &labels)[i].as_position(), WIDE);
        let start = start(&s.app);
        assert_eq!(start.prompt.text(), "https://github.com/acme/shop/issues/7");
        assert_eq!(
            (start.branch.as_str(), start.title()),
            ("issue-7-fix-the-login", "Start a flow › #7 Fix the login".into())
        );
        let workspaces = s.app.projects[0].workspaces.len();
        button(&mut s.app, WIDE, dialog::CANCEL);
        assert!(matches!(&s.app.overlay, Some(Overlay::Issues(b)) if matches!(b.screen, Screen::List)));
        assert_eq!(s.app.projects[0].workspaces.len(), workspaces);
    }
}

mod the_dialog {
    use super::*;

    #[test]
    fn typing_writes_the_prompt_and_the_branch_follows_it() {
        let mut s = opened(true);
        type_in(&mut s.app, "Add dark mode", WIDE);
        assert_eq!(start(&s.app).branch, "add-dark-mode");
    }

    #[test]
    fn a_click_on_a_flow_selects_it() {
        let mut s = opened(true);
        click_flow(&mut s.app, "Quick", 0);
        assert_eq!(selected(&s.app), "Quick");
        click_flow(&mut s.app, "Cross review", 1);
        assert_eq!(selected(&s.app), "Cross review");
    }

    #[test]
    fn the_list_takes_its_rows_and_the_prompt_the_rest() {
        let s = opened(true);
        let view = view(&s.app, WIDE);
        let (list, prompt) = (ui::start_list(WIDE, view.shape()), ui::start_prompt(WIDE, view.shape()));
        assert_eq!((list.height, prompt.y - list.bottom()), (11, 2));
        assert!(prompt.height > list.height);
    }

    #[test]
    fn outside_a_repository_it_says_so_and_does_not_start() {
        let mut s = opened(false);
        type_in(&mut s.app, "Add dark mode", WIDE);
        button(&mut s.app, WIDE, START);
        let refused = start(&s.app).notice.as_deref();
        assert_eq!(refused, Some("a flow needs a project that is the root of a git repository"));
    }

    #[test]
    fn a_press_outside_closes_it() {
        let mut s = opened(true);
        click_in(&mut s.app, Position::new(0, 0), WIDE);
        assert!(s.app.overlay.is_none());
    }

    #[test]
    fn edit_opens_the_selected_flow_on_its_edit_page_and_done_comes_back() {
        let mut s = opened(true);
        type_in(&mut s.app, "Add dark mode", WIDE);
        button(&mut s.app, WIDE, dialog::EDIT);
        let Some(Overlay::Flows { dialog: flows, .. }) = &s.app.overlay else { panic!("no flows dialog") };
        assert_eq!(flows.edit.as_ref().map(|edit| edit.flow.name.as_str()), Some("Full issue"));
        flows_button(&mut s.app, dialog::DONE);
        flows_button(&mut s.app, dialog::DONE);
        assert_eq!((selected(&s.app), start(&s.app).prompt.text()), ("Full issue".into(), "Add dark mode".into()));
    }

    #[test]
    fn a_flow_duplicated_while_editing_is_selected_on_coming_back() {
        let mut s = opened(true);
        type_in(&mut s.app, "Add dark mode", WIDE);
        button(&mut s.app, WIDE, dialog::EDIT);
        flows_button(&mut s.app, dialog::DONE);
        flows_button(&mut s.app, dialog::DUPLICATE);
        flows_button(&mut s.app, dialog::DONE);
        flows_button(&mut s.app, dialog::DONE);
        let expected = ("Full issue copy".to_string(), "Add dark mode".to_string());
        assert_eq!((selected(&s.app), start(&s.app).prompt.text()), expected);
    }
}

mod preview {
    use super::*;

    const SHORT: Rect = Rect { x: 0, y: 0, width: 150, height: 30 };

    #[test]
    fn the_wheel_over_it_scrolls_it_and_choosing_another_flow_brings_it_back_to_the_top() {
        let mut s = opened(true);
        let body = ui::start_preview(SHORT, view(&s.app, SHORT).shape()).map(ui::preview_body).expect("a preview");
        assert!(format!("{}", render(&mut s.app, SHORT).backend()).contains(" ↓ more "));
        mouse_in(&mut s.app, MouseEventKind::ScrollDown, body.as_position(), SHORT);
        assert!(start(&s.app).preview > 0);
        assert!(format!("{}", render(&mut s.app, SHORT).backend()).contains(" ↑ more "));
        click_flow(&mut s.app, "Quick", 0);
        assert_eq!(start(&s.app).preview, 0);
    }
}

mod running {
    use super::*;

    fn with_claude(s: &mut Setup) -> [TempDir; 2] {
        let (worktrees, bin) = (TempDir::new(), TempDir::new());
        let fake = bin.path().join("claude");
        crate::test_util::write_executable(&fake, "#!/bin/sh\nexec sleep 60\n");
        s.app.config.agent_commands.insert("claude".into(), fake.display().to_string());
        s.app.config.worktrees_dir = worktrees.path().display().to_string();
        [worktrees, bin]
    }

    #[test]
    fn start_opens_the_team_in_its_worktree_and_shows_the_orchestrator() {
        let mut s = opened(true);
        let _dirs = with_claude(&mut s);
        click_flow(&mut s.app, "Full issue", 0);
        let prompt = ui::start_prompt(WIDE, view(&s.app, WIDE).shape());
        click_in(&mut s.app, prompt.as_position(), WIDE);
        type_in(&mut s.app, "Fix the login form", WIDE);
        button(&mut s.app, WIDE, START);
        click_in(&mut s.app, Position::new(0, 0), WIDE);
        let ui::Overlay::StartFlow(view) = overlay_shown(&s.app, WIDE) else { panic!("the dialog closed at once") };
        assert_eq!(view.note, Some(ui::Note::Busy("creating the worktree and starting the team…")));
        pump_until(&mut s.app, &s.rx, "the team's tabs open", |a| !a.runs.is_empty());
        assert!(s.app.overlay.is_none());
        let workspace = s.app.project().and_then(Project::workspace).expect("the run's workspace");
        let tabs: Vec<String> = workspace.tabs.iter().map(|t| t.label(&s.app.config)).collect();
        assert_eq!(git::branch(&workspace.path).as_deref(), Some("fix-the-login-form"));
        assert_eq!(tabs, ["orchestrator", "coder", "reviewer", "security", "tester"]);
        assert_eq!((workspace.active, s.app.runs[0].task.as_str()), (0, "Fix the login form"));
        assert_eq!(s.app.side, Some(Side::Flow));
    }

    #[test]
    fn a_flow_from_an_issue_takes_the_template_and_names_its_workspace() {
        let mut s = setup(true);
        let _dirs = with_claude(&mut s);
        s.app.config.issue_branch = "feat/{key}-{slug}".into();
        from_issue(&mut s);
        assert_eq!(start(&s.app).branch, "feat/7-fix-the-login");
        assert_eq!(view(&s.app, WIDE).workspace.as_deref(), Some("#7 Fix the login"));
        let field = ui::start_workspace(WIDE, view(&s.app, WIDE).shape());
        click_in(&mut s.app, field.as_position(), WIDE);
        for _ in 0.."#7 Fix the login".len() {
            send_key(&mut s.app, KeyCode::Backspace, KeyModifiers::NONE);
        }
        type_in(&mut s.app, "Login", WIDE);
        button(&mut s.app, WIDE, START);
        pump_until(&mut s.app, &s.rx, "the team's tabs open", |a| !a.runs.is_empty());
        let workspace = s.app.project().and_then(Project::workspace).expect("the run's workspace");
        assert_eq!(git::branch(&workspace.path).as_deref(), Some("feat/7-fix-the-login"));
        assert_eq!(workspace.name.as_deref(), Some("Login"));
    }

    #[test]
    fn a_flow_from_an_issue_keeps_its_branch_when_the_prompt_would_name_the_same_one() {
        let mut s = setup(true);
        let _dirs = with_claude(&mut s);
        s.app.config.prompt = "{branch}".into();
        crate::test_util::git(&s.app.projects[0].path, &["branch", "issue-7-fix-the-login"]);
        from_issue(&mut s);
        button(&mut s.app, WIDE, START);
        pump_until(&mut s.app, &s.rx, "the team's tabs open", |a| !a.runs.is_empty());
        let workspace = s.app.project().and_then(Project::workspace).expect("the run's workspace");
        assert_eq!(git::branch(&workspace.path).as_deref(), Some("issue-7-fix-the-login"));
    }

    #[test]
    fn a_flow_from_an_issue_refuses_a_branch_git_would_refuse() {
        let mut s = setup(true);
        from_issue(&mut s);
        let field = ui::start_branch(WIDE, view(&s.app, WIDE).shape());
        click_in(&mut s.app, field.as_position(), WIDE);
        type_in(&mut s.app, " x", WIDE);
        button(&mut s.app, WIDE, START);
        let note = start(&s.app).notice.clone().unwrap_or_default();
        assert!(note.contains("cannot be a branch"), "{note}");
        assert_eq!(s.app.projects[0].workspaces.len(), 1);
    }

    #[test]
    fn a_worktree_that_cannot_be_made_is_said_in_the_dialog_with_the_task_kept() {
        let mut s = opened(true);
        let dirs = with_claude(&mut s);
        let blocked = dirs[0].path().join("a file");
        std::fs::write(&blocked, "").expect("write a file where the worktrees go");
        s.app.config.worktrees_dir = blocked.display().to_string();
        type_in(&mut s.app, "Fix the login form", WIDE);
        button(&mut s.app, WIDE, START);
        pump_until(
            &mut s.app,
            &s.rx,
            "the worktree fails",
            |a| matches!(&a.overlay, Some(Overlay::StartFlow { start, .. }) if !start.waiting),
        );
        let start = start(&s.app);
        assert!(
            start.notice.as_deref().is_some_and(|n| n.starts_with("the flow did not start: ")),
            "{:?}",
            start.notice
        );
        assert_eq!((start.prompt.text(), s.app.toast.is_none()), ("Fix the login form".into(), true));
    }

    #[test]
    fn a_refusal_stays_in_the_dialog_in_red() {
        let mut s = opened(true);
        type_in(&mut s.app, "Fix the login form", WIDE);
        button(&mut s.app, WIDE, START);
        let refused = start(&s.app).notice.clone().expect("the refusal");
        assert!(refused.contains("cannot find on its PATH"), "{refused}");
        let ui::Overlay::StartFlow(view) = overlay_shown(&s.app, WIDE) else { panic!("the dialog closed") };
        assert_eq!((view.note, s.app.runs.len()), (Some(ui::Note::Error(refused)), 0));
    }
}

mod projects {
    use super::*;
    use crate::flows::{builtin, store};

    fn project_flow(dir: &Path, name: &str) {
        let flow = crate::flows::Flow { name: name.into(), ..builtin::starter(&[], None) };
        store::save(&store::project_dir(dir), &flow, None).expect("save the project's flow");
    }

    #[test]
    fn edit_lists_the_flows_of_the_project_the_dialog_is_for_not_the_shown_one() {
        let mut s = setup(true);
        let other = git_repo(&[("README", "hi")]);
        project_flow(s.project.path(), "Shown project's flow");
        project_flow(other.path(), "Other project's flow");
        s.app.open_project(other.path().to_path_buf(), AREA).expect("open the other project");
        let id = s.app.projects[1].id;
        s.app.active = 0;
        s.app.open_start_flow(s.app.start_flow_in(id));
        button(&mut s.app, WIDE, dialog::EDIT);
        let Some(Overlay::Flows { dialog: flows, .. }) = &s.app.overlay else { panic!("no flows dialog") };
        let names: Vec<&str> = flows.flows.iter().map(|saved| saved.flow.name.as_str()).collect();
        assert!(names.contains(&"Other project's flow") && !names.contains(&"Shown project's flow"), "{names:?}");
        assert_eq!(flows.project, Some(id));
    }
}

mod accounts {
    use std::collections::BTreeMap;

    use super::*;
    use crate::accounts;

    fn with_accounts(s: &mut Setup) {
        let dir = s.config.path().join("accounts");
        for name in ["home", "work"] {
            accounts::add(&dir, Known::Claude, name, None).expect("the account");
        }
        s.app.config.active_accounts = BTreeMap::from([("claude".to_string(), "work".to_string())]);
        s.app.config.account_names =
            BTreeMap::from([("claude".to_string(), BTreeMap::from([("home".to_string(), "Personal".to_string())]))]);
        s.app.set_accounts_dir(&dir);
        s.app.relist_accounts();
    }

    fn opened_with_accounts() -> Setup {
        let mut s = setup(true);
        with_accounts(&mut s);
        let new = new_workspace_pos(&s.app);
        click(&mut s.app, new);
        pick(&mut s.app, START_FLOW);
        s
    }

    fn claude(app: &App) -> String {
        start(app).accounts.iter().find(|agent| agent.kind == "claude").expect("claude's combo").chosen().into()
    }

    fn choose(app: &mut App, option: u16) {
        let field = ui::start_account(WIDE, view(app, WIDE).shape(), 0);
        click_in(app, field.as_position(), WIDE);
        let view = view(app, WIDE);
        let list = ui::start_account_list(WIDE, view.shape(), 0, &view.accounts[0].combo).expect("the list is open");
        click_in(app, Position::new(list.x + 2, list.y + 1 + option), WIDE);
    }

    fn started(s: &mut Setup) -> Vec<Option<String>> {
        let (worktrees, bin) = (TempDir::new(), TempDir::new());
        let fake = bin.path().join("claude");
        crate::test_util::write_executable(&fake, "#!/bin/sh\nexec sleep 60\n");
        s.app.config.agent_commands.insert("claude".into(), fake.display().to_string());
        s.app.config.worktrees_dir = worktrees.path().display().to_string();
        let prompt = ui::start_prompt(WIDE, view(&s.app, WIDE).shape());
        click_in(&mut s.app, prompt.as_position(), WIDE);
        type_in(&mut s.app, "Fix the login form", WIDE);
        button(&mut s.app, WIDE, START);
        pump_until(&mut s.app, &s.rx, "the team's tabs open", |a| !a.runs.is_empty());
        let workspace = s.app.project().and_then(Project::workspace).expect("the run's workspace");
        workspace.terms().map(|t| t.accounts().name(Known::Claude).map(String::from)).collect()
    }

    #[test]
    fn without_accounts_or_with_the_picker_off_there_is_no_combo() {
        assert_eq!(start(&opened(true).app).accounts.len(), 0);
        assert!(view(&opened(true).app, WIDE).accounts.is_empty());
        let mut s = setup(true);
        with_accounts(&mut s);
        s.app.config.account_picker = false;
        s.app.open_start_flow(s.app.start_flow_here());
        assert_eq!(start(&s.app).accounts.len(), 0);
    }

    #[test]
    fn the_combo_lists_the_accounts_by_their_names_and_starts_on_the_one_in_use() {
        let s = opened_with_accounts();
        let view = view(&s.app, WIDE);
        let combo = &view.accounts[0].combo;
        assert_eq!(view.accounts.len(), 1);
        assert_eq!(
            (combo.options(), combo.label()),
            (&["System".into(), "Personal".into(), "work".into()][..], Some("work"))
        );
    }

    #[test]
    fn a_click_on_an_option_chooses_it() {
        let mut s = opened_with_accounts();
        choose(&mut s.app, 0);
        assert_eq!(
            (claude(&s.app), start(&s.app).accounts_open(), s.app.overlay.is_some()),
            (String::new(), false, true)
        );
    }

    #[test]
    fn without_a_choice_the_team_starts_with_the_account_in_use() {
        let mut s = opened_with_accounts();
        assert_eq!(started(&mut s), vec![Some("work".to_string()); 5]);
    }

    #[test]
    fn the_team_starts_with_the_chosen_account_and_new_tabs_keep_the_one_in_use() {
        let mut s = opened_with_accounts();
        choose(&mut s.app, 1);
        assert_eq!(started(&mut s), vec![Some("home".to_string()); 5]);
        assert_eq!(s.app.config.active_accounts.get("claude").map(String::as_str), Some("work"));
    }

    #[test]
    fn the_system_login_chosen_starts_the_team_without_an_account() {
        let mut s = opened_with_accounts();
        choose(&mut s.app, 0);
        assert_eq!(started(&mut s), vec![None; 5]);
    }

    #[test]
    fn an_account_deleted_meanwhile_is_said_in_the_dialog_and_nothing_starts() {
        let mut s = opened_with_accounts();
        choose(&mut s.app, 1);
        let dir = s.config.path().join("accounts");
        std::fs::remove_dir_all(accounts::path(&dir, Known::Claude, "home")).expect("delete the account");
        s.app.relist_accounts();
        let prompt = ui::start_prompt(WIDE, view(&s.app, WIDE).shape());
        click_in(&mut s.app, prompt.as_position(), WIDE);
        type_in(&mut s.app, "Fix the login form", WIDE);
        button(&mut s.app, WIDE, START);
        let notice = start(&s.app).notice.clone().expect("the refusal");
        assert!(notice.contains("home"), "{notice}");
        assert!(s.app.runs.is_empty());
        assert_eq!(s.app.projects[0].workspaces.len(), 1);
    }

    #[test]
    fn with_an_account() {
        let mut s = opened_with_accounts();
        type_in(&mut s.app, "Add a dark mode to the settings page", WIDE);
        insta::assert_snapshot!(format!("{}", render(&mut s.app, WIDE).backend()));
    }

    #[test]
    fn with_the_account_list_open() {
        let mut s = opened_with_accounts();
        let field = ui::start_account(WIDE, view(&s.app, WIDE).shape(), 0);
        click_in(&mut s.app, field.as_position(), WIDE);
        insta::assert_snapshot!(format!("{}", render(&mut s.app, WIDE).backend()));
    }
}

fn flows_button(app: &mut App, label: &str) {
    flows_dialog::button(app, WIDE, label);
}

mod pages {
    use super::*;

    fn written(area: Rect) -> Setup {
        let mut s = opened(true);
        type_in(&mut s.app, "Add a dark mode to the settings page", area);
        s
    }

    #[test]
    fn wide() {
        let mut s = written(WIDE);
        insta::assert_snapshot!(format!("{}", render(&mut s.app, WIDE).backend()));
    }

    #[test]
    fn narrow() {
        let mut s = written(NARROW);
        insta::assert_snapshot!(format!("{}", render(&mut s.app, NARROW).backend()));
    }

    #[test]
    fn outside_a_repository() {
        let mut s = opened(false);
        insta::assert_snapshot!(format!("{}", render(&mut s.app, WIDE).backend()));
    }
}
