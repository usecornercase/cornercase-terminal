use super::*;

fn open_rename(app: &mut App, at: Position) {
    right_click(app, at);
    let item = menu_item_at(app, 0);
    click(app, item);
}

fn folder(dir: &TempDir) -> String {
    dir.path().file_name().and_then(|n| n.to_str()).expect("folder name").to_string()
}

#[test]
fn opens_with_the_current_name() {
    let (mut app, _rx, dirs) = app_with(1);
    open_rename(&mut app, entry_pos());
    assert_eq!(form_value(&app), Some(folder(&dirs[0])));
}

#[test]
fn renames_the_project() {
    let (mut app, _rx, _dirs) = app_with(1);
    open_rename(&mut app, entry_pos());
    clear_input(&mut app);
    type_text(&mut app, "  my api  ");

    click(&mut app, form_button(RENAME_SUBMIT, 0));

    assert_eq!((app.overlay.is_none(), app.project_label(&app.projects[0])), (true, "my api".into()));
}

#[test]
fn an_empty_name_goes_back_to_the_folder_name() {
    let (mut app, _rx, dirs) = app_with(1);
    open_rename(&mut app, entry_pos());
    submit_text(&mut app, "-x");
    open_rename(&mut app, entry_pos());

    clear_input(&mut app);
    send_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);

    assert_eq!(app.project_label(&app.projects[0]), folder(&dirs[0]));
}

#[test]
fn esc_keeps_the_old_name() {
    let (mut app, _rx, dirs) = app_with(1);
    open_rename(&mut app, entry_pos());
    type_text(&mut app, "-changed");

    send_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);

    assert_eq!((app.overlay.is_none(), app.project_label(&app.projects[0])), (true, folder(&dirs[0])));
}

#[test]
fn renames_a_workspace() {
    let (mut app, _rx, _dirs) = app_with(1);
    let at = row_pos(&app, WorkspaceRow::Workspace(0));
    open_rename(&mut app, at);
    clear_input(&mut app);

    submit_text(&mut app, "main line");

    assert_eq!(workspace_labels(&app), ["main line"]);
}

#[test]
fn renames_a_tab_until_it_is_cleared() {
    let (mut app, _rx, _dirs) = app_with(1);
    let at = row_pos(&app, WorkspaceRow::Tab(0, 0));
    open_rename(&mut app, at);
    clear_input(&mut app);
    submit_text(&mut app, "server");
    assert_eq!(app.projects[0].workspaces[0].tabs[0].label(&app.config), "server");

    let at = row_pos(&app, WorkspaceRow::Tab(0, 0));
    open_rename(&mut app, at);
    clear_input(&mut app);
    send_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);

    wait_until("back to the program name", || is_sh(&app.projects[0].workspaces[0].tabs[0].label(&app.config)));
}

mod editing_the_name {
    use rstest::rstest;

    use super::*;

    fn renaming(name: &str) -> (App, Receiver<AppEvent>, Vec<TempDir>) {
        let (mut app, rx, dirs) = app_with(1);
        open_rename(&mut app, entry_pos());
        clear_input(&mut app);
        type_text(&mut app, name);
        (app, rx, dirs)
    }

    #[rstest]
    #[case::home(KeyCode::Home, KeyModifiers::NONE, "xfix the login")]
    #[case::ctrl_a(KeyCode::Char('a'), KeyModifiers::CONTROL, "xfix the login")]
    #[case::left(KeyCode::Left, KeyModifiers::NONE, "fix the logixn")]
    #[case::alt_left(KeyCode::Left, KeyModifiers::ALT, "fix the xlogin")]
    #[case::esc_b(KeyCode::Char('b'), KeyModifiers::ALT, "fix the xlogin")]
    fn a_key_moves_the_cursor_and_typing_goes_there(
        #[case] code: KeyCode,
        #[case] mods: KeyModifiers,
        #[case] expected: &str,
    ) {
        let (mut app, _rx, _dirs) = renaming("fix the login");
        send_key(&mut app, code, mods);
        type_text(&mut app, "x");
        assert_eq!(form_value(&app).as_deref(), Some(expected));
    }

    #[test]
    fn ctrl_e_goes_back_to_the_end() {
        let (mut app, _rx, _dirs) = renaming("fix");
        send_key(&mut app, KeyCode::Home, KeyModifiers::NONE);
        send_key(&mut app, KeyCode::Char('e'), KeyModifiers::CONTROL);
        type_text(&mut app, "ed");
        assert_eq!(form_value(&app).as_deref(), Some("fixed"));
    }

    #[test]
    fn a_click_places_the_cursor() {
        let (mut app, _rx, _dirs) = renaming("fix the login");
        let text = ui::form_text(AREA);
        click(&mut app, Position::new(text.x, text.y));
        type_text(&mut app, "x");
        assert_eq!(form_value(&app).as_deref(), Some("xfix the login"));
    }

    #[test]
    fn a_paste_goes_where_the_cursor_is() {
        let (mut app, _rx, _dirs) = renaming("fix login");
        send_key(&mut app, KeyCode::Left, KeyModifiers::ALT);
        app.handle_paste("the ", AREA);
        assert_eq!(form_value(&app).as_deref(), Some("fix the login"));
    }

    #[test]
    fn shift_selects_and_typing_replaces_the_selection() {
        let (mut app, _rx, _dirs) = renaming("fix the login");
        send_key(&mut app, KeyCode::Left, KeyModifiers::ALT | KeyModifiers::SHIFT);
        type_text(&mut app, "bug");
        assert_eq!(form_value(&app).as_deref(), Some("fix the bug"));
    }

    #[test]
    fn a_long_name_opens_on_the_line_with_the_cursor() {
        let (mut app, _rx, _dirs) = app_with(1);
        let target = Target::Project(app.projects[0].id);
        let name = format!("{}end", "a ".repeat(60));
        app.overlay = Some(Overlay::Rename { target, input: Editor::new(&name) });
        let mut t = render(&mut app, AREA);
        let text = ui::form_text(AREA);
        let row: String =
            (text.x..text.right()).map(|x| t.backend().buffer()[(x, text.y)].symbol().to_string()).collect();
        assert!(row.trim_end().ends_with(" end"), "{row}");
        assert_eq!(t.get_cursor_position().expect("cursor").y, text.y);
    }

    #[test]
    fn the_cursor_is_drawn_where_it_is() {
        let (mut app, _rx, _dirs) = renaming("fix");
        send_key(&mut app, KeyCode::Home, KeyModifiers::NONE);
        let mut t = render(&mut app, AREA);
        let text = ui::form_text(AREA);
        assert_eq!(t.get_cursor_position().expect("cursor"), Position::new(text.x, text.y));
    }
}
