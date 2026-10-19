use super::*;
use crate::host::open::{Failed, Open, Opener};
use crate::ui::files::{self as files_ui, Action};

const ALL: [Opener; 3] = [Opener::Files, Opener::Vscode, Opener::Default];

fn window(app: &mut App, openers: &[Opener]) {
    app.set_window(7, openers);
}

fn path_of(dir: &TempDir) -> String {
    dir.path().to_str().expect("a UTF-8 path").to_string()
}

fn order(opener: Opener, path: &str, line: Option<u32>) -> (u64, Open) {
    (7, Open { opener, path: path.into(), line })
}

fn pane_pos() -> Position {
    let pane = areas().pane;
    Position::new(pane.x + 1, pane.y + 1)
}

mod rows {
    use super::*;

    #[test]
    fn a_project_offers_open_in_and_lists_the_openers_in_order_without_the_default_app() {
        let (mut app, _rx, dirs) = app_with(1);
        window(&mut app, &ALL);
        right_click(&mut app, entry_pos());
        assert_eq!(menu_labels(&app), ["rename project", "open in…"]);
        pick(&mut app, "open in…");
        assert_eq!(menu_labels(&app), ["file manager", "VS Code"]);
        pick(&mut app, "VS Code");
        assert_eq!(app.take_opens(), [order(Opener::Vscode, &path_of(&dirs[0]), None)]);
    }

    #[test]
    fn a_workspace_opens_its_own_folder() {
        let (mut app, _rx, dirs) = app_with(1);
        window(&mut app, &ALL);
        right_click_row(&mut app, WorkspaceRow::Workspace(0));
        pick(&mut app, "open in…");
        pick(&mut app, "file manager");
        assert_eq!(app.take_opens(), [order(Opener::Files, &path_of(&dirs[0]), None)]);
    }

    #[test]
    fn a_tab_has_no_open_in() {
        let (mut app, _rx, _dirs) = app_with(1);
        window(&mut app, &ALL);
        right_click_row(&mut app, WorkspaceRow::Tab(0, 0));
        assert_eq!(menu_labels(&app), ["rename tab"]);
    }

    #[test]
    fn a_window_that_can_open_nothing_offers_copy_path() {
        let (mut app, _rx, dirs) = app_with(1);
        right_click(&mut app, entry_pos());
        assert_eq!(menu_labels(&app), ["rename project", "copy path"]);
        pick(&mut app, "copy path");
        assert_eq!(app.take_host_writes(), [clipboard::osc52(&path_of(&dirs[0]))]);
        assert_eq!(toast(&app), Some("copied the folder's path: it is on the server's machine"));
    }

    #[test]
    fn a_menu_built_for_one_window_copies_when_another_clicks_it() {
        let (mut app, _rx, dirs) = app_with(1);
        window(&mut app, &ALL);
        right_click(&mut app, entry_pos());
        pick(&mut app, "open in…");
        app.set_window(8, &[]);
        pick(&mut app, "VS Code");
        assert_eq!(app.take_opens(), []);
        assert_eq!(app.take_host_writes(), [clipboard::osc52(&path_of(&dirs[0]))]);
    }

    #[test]
    fn a_failed_open_shows_the_reason() {
        let (mut app, _rx, _dirs) = app_with(1);
        app.open_failed(7, &Failed { opener: Opener::Vscode, reason: "exit status 1".into() });
        assert_eq!(toast(&app), Some("could not open in VS Code: exit status 1"));
    }
}

mod panes {
    use super::*;

    #[test]
    fn the_pane_menu_opens_the_folder_its_program_is_in() {
        let (mut app, rx, dirs) = app_with(1);
        window(&mut app, &ALL);
        std::fs::create_dir(dirs[0].path().join("sub")).expect("a folder");
        let sub = canonical(&dirs[0]).join("sub");
        type_line(&mut app, "cd sub");
        pump_until(&mut app, &rx, "the pane moves", |a| {
            a.term().is_some_and(|t| t.cwd().as_deref() == Some(sub.as_path()))
        });
        right_click(&mut app, pane_pos());
        let labels = menu_labels(&app);
        assert_eq!(labels.iter().rev().nth(1).map(String::as_str), Some("open in…"));
        assert_eq!(labels.last().map(String::as_str), Some("close pane"));
        pick(&mut app, "open in…");
        pick(&mut app, "file manager");
        assert_eq!(app.take_opens(), [order(Opener::Files, sub.to_str().expect("UTF-8"), None)]);
    }
}

mod files {
    use super::*;
    use crate::ui::files::Screen;

    fn repo() -> TempDir {
        git_repo(&[("README.md", "# shop\n"), ("run.sh", "x\n"), ("src/main.rs", "a\nb\nc\nd\n")])
    }

    fn tree_row(app: &App, path: &str) -> Position {
        let view = app.files_view().expect("a workspace");
        let Screen::Tree(tree) = &view.screen else { panic!("the tree shows") };
        let i = tree.rows.iter().position(|r| r.path == path).expect("the row is listed");
        let body = files_ui::parts(panel_area(app)).body;
        Position::new(body.x + 4, body.y + u16::try_from(i - tree.scroll).expect("on screen"))
    }

    fn opened(repo: &TempDir) -> (App, Receiver<AppEvent>) {
        let (mut app, rx) = app_in(repo.path(), no_config());
        let button = app.layout(AREA).files_button.as_position();
        click(&mut app, button);
        wait_until("the tree is listed", || {
            app.refresh(Instant::now());
            drain(&mut app, &rx);
            app.files_view().is_some_and(|v| matches!(&v.screen, Screen::Tree(t) if !t.rows.is_empty()))
        });
        window(&mut app, &ALL);
        (app, rx)
    }

    fn read(app: &mut App, rx: &Receiver<AppEvent>, path: &str) {
        let pos = tree_row(app, path);
        click(app, pos);
        wait_until("the file is read", || {
            app.refresh(Instant::now());
            drain(app, rx);
            matches!(app.files_view().map(|v| v.screen), Some(Screen::File(f)) if f.content.is_some())
        });
    }

    fn header_button(app: &mut App) -> Position {
        app.files.set_full_screen(true);
        let Some(Screen::File(file)) = app.files_view().map(|v| v.screen) else { panic!("a file shows") };
        files_ui::action(panel_area(app), &file, Action::OpenIn).as_position()
    }

    #[test]
    fn a_right_click_on_a_folder_row_offers_the_openers() {
        let repo = repo();
        let (mut app, _rx) = opened(&repo);
        let pos = tree_row(&app, "src");
        right_click(&mut app, pos);
        assert_eq!(menu_labels(&app), ["file manager", "VS Code"]);
        pick(&mut app, "file manager");
        let expected = canonical(&repo).join("src");
        assert_eq!(app.take_opens(), [order(Opener::Files, expected.to_str().expect("UTF-8"), None)]);
    }

    #[test]
    fn a_right_click_on_a_file_row_ends_with_the_default_app_and_has_none_for_a_program() {
        let repo = repo();
        let script = repo.path().join("run.sh");
        std::fs::set_permissions(&script, std::os::unix::fs::PermissionsExt::from_mode(0o755)).expect("chmod");
        let (mut app, _rx) = opened(&repo);
        let pos = tree_row(&app, "README.md");
        right_click(&mut app, pos);
        assert_eq!(menu_labels(&app), ["file manager", "VS Code", "default app"]);
        app.overlay = None;
        let pos = tree_row(&app, "run.sh");
        right_click(&mut app, pos);
        assert_eq!(menu_labels(&app), ["file manager", "VS Code"]);
    }

    #[test]
    fn the_header_button_opens_the_file_at_its_first_selected_line() {
        let repo = repo();
        let (mut app, rx) = opened(&repo);
        let pos = tree_row(&app, "src");
        click(&mut app, pos);
        wait_until("the folder is listed", || {
            app.refresh(Instant::now());
            drain(&mut app, &rx);
            app.files_view()
                .is_some_and(|v| matches!(&v.screen, Screen::Tree(t) if t.rows.iter().any(|r| r.path == "src/main.rs")))
        });
        read(&mut app, &rx, "src/main.rs");
        app.files.viewer_mut(app.projects[0].workspaces[0].id).expect("a viewer").selection = Some((3, 2));
        let button = header_button(&mut app);
        click(&mut app, button);
        pick(&mut app, "VS Code");
        let expected = canonical(&repo).join("src/main.rs");
        assert_eq!(app.take_opens(), [order(Opener::Vscode, expected.to_str().expect("UTF-8"), Some(2))]);
    }

    #[test]
    fn the_header_button_copies_at_once_without_openers() {
        let repo = repo();
        let (mut app, rx) = opened(&repo);
        read(&mut app, &rx, "README.md");
        let button = header_button(&mut app);
        window(&mut app, &[]);
        click(&mut app, button);
        assert!(app.overlay.is_none());
        let expected = canonical(&repo).join("README.md");
        assert_eq!(app.take_host_writes(), [clipboard::osc52(expected.to_str().expect("UTF-8"))]);
    }
}
