use super::*;

struct Setup {
    app: App,
    _rx: Receiver<AppEvent>,
    root: PathBuf,
    _tmp: TempDir,
}

fn open_picker() -> Setup {
    let tmp = TempDir::new();
    let root = canonical(&tmp);
    for folder in ["active", "api", "web/src"] {
        std::fs::create_dir_all(root.join(folder)).expect("create folder");
    }
    let (mut app, rx) = app_in(&root.join("active"), no_config());
    click_new_project(&mut app);
    Setup { app, _rx: rx, root, _tmp: tmp }
}

fn picker(app: &App) -> &Picker {
    let Some(Overlay::Picker { picker, .. }) = &app.overlay else { panic!("the picker is not open") };
    picker
}

fn item_pos(app: &App, name: &str) -> Position {
    let items = picker(app).items();
    let i = items.iter().position(|item| item.name == name).expect("item listed");
    let area = ui::picker_area(AREA);
    ui::picker_item(area, items.len(), picker(app).scroll(), i).as_position()
}

fn button(which: usize) -> Position {
    ui::picker_buttons(ui::picker_area(AREA), PICKER_SUBMIT)[which].as_position()
}

fn import(s: &mut Setup, name: &str, text: &str) {
    std::fs::write(s.root.join(name), text).expect("write workspace");
    s.app.overlay = None;
    click_new_project(&mut s.app);
    let file = item_pos(&s.app, name);
    click(&mut s.app, file);
}

fn grouped(app: &App, group: &str) -> Vec<PathBuf> {
    let id = app.groups.iter().find(|g| g.entry.name == group).map(|g| g.id);
    app.projects.iter().filter(|p| id.is_some() && p.group == id).map(|p| p.path.clone()).collect()
}

#[test]
fn a_vscode_workspace_becomes_a_group_of_its_folders() {
    let mut s = open_picker();

    import(&mut s, "work.code-workspace", r#"{"folders": [{"path": "api"}, {"path": "web"}]}"#);

    let active = s.app.project().map(|p| p.path.clone());
    assert_eq!(
        (grouped(&s.app, "work"), active, s.app.overlay.is_none()),
        (vec![s.root.join("api"), s.root.join("web")], Some(s.root.join("api")), true)
    );
    assert_eq!(toast(&s.app), Some("2 projects imported into work"));
}

#[test]
fn importing_moves_an_open_project_and_skips_missing_folders() {
    let mut s = open_picker();

    import(&mut s, "w.code-workspace", r#"{"folders": [{"path": "active"}, {"path": "gone"}]}"#);

    assert_eq!(
        (s.app.projects.len(), grouped(&s.app, "w"), toast(&s.app)),
        (1, vec![s.root.join("active")], Some("1 project imported into w, 1 not found"))
    );
}

#[test]
fn importing_into_a_group_with_the_same_name_reuses_it() {
    let mut s = open_picker();
    s.app.add_group("work".into());

    import(&mut s, "work.code-workspace", r#"{"folders": [{"path": "api"}]}"#);

    assert_eq!((s.app.groups.len(), grouped(&s.app, "work")), (1, vec![s.root.join("api")]));
}

#[test]
fn a_folder_name_becomes_the_project_name() {
    let mut s = open_picker();

    import(&mut s, "w.code-workspace", r#"{"folders": [{"path": "api", "name": "Backend"}]}"#);

    assert_eq!(s.app.project().and_then(|p| p.name.as_deref()), Some("Backend"));
}

#[test]
fn an_open_project_gets_the_folder_name_unless_it_has_its_own() {
    let mut s = open_picker();
    s.app.projects[0].name = Some("mine".into());
    let api = item_pos(&s.app, "api");
    click(&mut s.app, api);
    click(&mut s.app, button(0));

    let text = r#"{"folders": [{"path": "active", "name": "Active"}, {"path": "api", "name": "Backend"}]}"#;
    import(&mut s, "w.code-workspace", text);

    let names: Vec<Option<&str>> = s.app.projects.iter().map(|p| p.name.as_deref()).collect();
    assert_eq!(names, [Some("mine"), Some("Backend")]);
}

#[test]
fn a_folder_whose_shell_cannot_start_is_counted_and_the_rest_still_opens() {
    let mut s = open_picker();
    let locked = Locked::new();
    let text = format!(
        r#"{{"folders": [{{"path": "api"}}, {{"path": "{}"}}, {{"path": "web"}}]}}"#,
        locked.path().display()
    );

    import(&mut s, "w.code-workspace", &text);

    assert_eq!(grouped(&s.app, "w"), [s.root.join("api"), s.root.join("web")]);
    assert_eq!(toast(&s.app), Some("2 projects imported into w, 1 could not open, see server.log"));
}

#[test]
fn a_folder_listed_twice_counts_once() {
    let mut s = open_picker();

    import(&mut s, "w.code-workspace", r#"{"folders": [{"path": "api"}, {"path": "./api"}]}"#);

    assert_eq!(
        (grouped(&s.app, "w"), toast(&s.app)),
        (vec![s.root.join("api")], Some("1 project imported into w"))
    );
}

#[test]
fn a_workspace_without_folders_found_makes_no_group() {
    let mut s = open_picker();

    import(&mut s, "w.code-workspace", r#"{"folders": [{"path": "gone"}]}"#);

    assert_eq!((s.app.groups.len(), toast(&s.app)), (0, Some("nothing imported from w, 1 not found")));
}

#[test]
fn a_broken_workspace_keeps_the_picker_open_with_the_error() {
    let mut s = open_picker();

    import(&mut s, "w.code-workspace", "{ nope");

    let error = picker(&s.app).error().map(str::to_string).unwrap_or_default();
    assert!(error.starts_with("cannot read") && s.app.groups.is_empty(), "{error}");
}

#[test]
fn starts_next_to_the_active_project() {
    let s = open_picker();
    assert_eq!(picker(&s.app).dir(), s.root);
}

#[test]
fn starts_at_home_with_no_projects() {
    let (mut app, _rx) = empty_app();
    let home = TempDir::new();
    app.home = Some(home.path().to_path_buf());

    click_new_project(&mut app);

    assert_eq!(picker(&app).dir(), home.path());
}

#[test]
fn enter_opens_a_project_in_the_current_folder() {
    let mut s = open_picker();

    send_key(&mut s.app, KeyCode::Enter, KeyModifiers::NONE);

    assert_eq!((s.app.overlay.is_none(), s.app.projects.len(), s.app.active), (true, 2, 1));
    wait_until("its tab starts in the folder", || term(&s.app, 1).cwd().as_ref() == Some(&s.root));
}

#[test]
fn clicking_a_folder_goes_into_it() {
    let mut s = open_picker();

    let web = item_pos(&s.app, "web");
    click(&mut s.app, web);

    assert_eq!(picker(&s.app).dir(), s.root.join("web"));
}

#[test]
fn open_button_opens_the_current_folder() {
    let mut s = open_picker();
    let web = item_pos(&s.app, "web");
    click(&mut s.app, web);

    click(&mut s.app, button(0));

    assert_eq!(s.app.projects.get(1).map(|p| p.path.clone()), Some(s.root.join("web")));
}

#[test]
fn typing_filters_and_enter_goes_into_the_match() {
    let mut s = open_picker();

    type_text(&mut s.app, "we");
    send_key(&mut s.app, KeyCode::Enter, KeyModifiers::NONE);

    assert_eq!((picker(&s.app).dir(), s.app.projects.len()), (s.root.join("web").as_path(), 1));
}

#[test]
fn arrows_select_and_tab_goes_into_the_selection() {
    let mut s = open_picker();

    send_key(&mut s.app, KeyCode::Down, KeyModifiers::NONE);
    send_key(&mut s.app, KeyCode::Down, KeyModifiers::NONE);
    send_key(&mut s.app, KeyCode::Tab, KeyModifiers::NONE);

    assert_eq!(picker(&s.app).dir(), s.root.join("active"));
}

#[test]
fn left_goes_up() {
    let mut s = open_picker();

    send_key(&mut s.app, KeyCode::Left, KeyModifiers::NONE);

    assert_eq!(Some(picker(&s.app).dir()), s.root.parent());
}

#[test]
fn a_pasted_path_is_walked() {
    let mut s = open_picker();

    s.app
        .handle_event(AppEvent::Input(Event::Paste(format!("{}/web/src/", s.root.display()))), AREA)
        .expect("handle paste");

    assert_eq!(picker(&s.app).dir(), s.root.join("web/src"));
}

#[test]
fn esc_cancels() {
    let mut s = open_picker();

    send_key(&mut s.app, KeyCode::Esc, KeyModifiers::NONE);

    assert_eq!((s.app.overlay.is_none(), s.app.projects.len()), (true, 1));
}

#[test]
fn cancel_button_cancels() {
    let mut s = open_picker();

    click(&mut s.app, button(1));

    assert_eq!((s.app.overlay.is_none(), s.app.projects.len()), (true, 1));
}

#[test]
fn the_wheel_scrolls_the_list() {
    let tmp = TempDir::new();
    for i in 0..40 {
        std::fs::create_dir(tmp.path().join(format!("f{i:02}"))).expect("create folder");
    }
    let picker = Picker::open(tmp.path(), None).expect("open picker");
    let (mut app, _rx) = app();
    app.overlay = Some(Overlay::Picker { picker, group: None });
    let ev =
        MouseEvent { kind: MouseEventKind::ScrollDown, column: 50, row: 10, modifiers: KeyModifiers::NONE };

    app.handle_event(AppEvent::Input(Event::Mouse(ev)), AREA).expect("handle wheel");

    assert_eq!(self::picker(&app).scroll(), usize::try_from(WHEEL_ROWS).expect("rows"));
}
