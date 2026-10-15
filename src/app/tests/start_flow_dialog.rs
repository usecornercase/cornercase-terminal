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
    _config: TempDir,
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
    Setup { app, rx, _config: config, project }
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
        s.app.open_issues(WIDE).expect("open the issues");
        let Some(Overlay::Issues(b)) = &mut s.app.overlay else { panic!("no issues") };
        let issue = issues::issue(Source::Github, 7, "Fix the login");
        b.screen = Screen::Detail { issue: Box::new(issue), detail: None, scroll: 0, raw: false };
        let labels = b.labels();
        let i = labels.iter().position(|l| *l == "start a flow").expect("the button");
        click_in(&mut s.app, ui::issue_buttons(ui::issues_area(WIDE), &labels)[i].as_position(), WIDE);
        let start = start(&s.app);
        assert_eq!(start.prompt.text(), "https://github.com/acme/shop/issues/7");
        assert_eq!(
            (start.branch.as_str(), start.title()),
            ("issue-7-fix-the-login", "Start a flow › #7 Fix the login".into())
        );
        button(&mut s.app, WIDE, dialog::CANCEL);
        assert!(matches!(s.app.overlay, Some(Overlay::Issues(_))));
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
        let list = ui::start_list(WIDE, view(&s.app, WIDE).shape());
        click_in(&mut s.app, Position::new(list.x + 4, list.y + 3), WIDE);
        assert_eq!(selected(&s.app), "Quick");
    }

    #[test]
    fn the_list_takes_its_rows_and_the_prompt_the_rest() {
        let s = opened(true);
        let view = view(&s.app, WIDE);
        let (list, prompt) = (ui::start_list(WIDE, view.shape()), ui::start_prompt(WIDE, view.shape()));
        assert_eq!((list.height, prompt.y - list.bottom()), (5, 2));
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
    fn edit_opens_the_flows_dialog_on_the_selected_flow_and_done_comes_back() {
        let mut s = opened(true);
        type_in(&mut s.app, "Add dark mode", WIDE);
        button(&mut s.app, WIDE, dialog::EDIT);
        let Some(Overlay::Flows { dialog: flows, .. }) = &s.app.overlay else { panic!("no flows dialog") };
        assert_eq!(flows.cursor, 0);
        flows_button(&mut s.app, dialog::DONE);
        assert_eq!((selected(&s.app), start(&s.app).prompt.text()), ("Full issue".into(), "Add dark mode".into()));
    }

    #[test]
    fn a_flow_duplicated_while_editing_is_selected_on_coming_back() {
        let mut s = opened(true);
        type_in(&mut s.app, "Add dark mode", WIDE);
        button(&mut s.app, WIDE, dialog::EDIT);
        flows_button(&mut s.app, dialog::DUPLICATE);
        flows_button(&mut s.app, dialog::DONE);
        flows_button(&mut s.app, dialog::DONE);
        assert_eq!((selected(&s.app), start(&s.app).prompt.text()), ("Full issue copy".into(), "Add dark mode".into()));
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
