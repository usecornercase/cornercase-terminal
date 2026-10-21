use super::*;

fn told(app: &mut App) -> Vec<String> {
    app.observe()
        .0
        .into_iter()
        .map(|note| {
            let fields: Vec<String> = note.fields.iter().map(|(k, v)| format!(" {k}={v}")).collect();
            format!("{}: {}{}", note.target, note.message, fields.concat())
        })
        .collect()
}

fn ids(app: &App) -> (u64, u64, u64, u64) {
    let project = &app.projects[0];
    let workspace = project.workspace().expect("a workspace");
    let tab = workspace.tab().expect("a tab");
    (project.id, workspace.id, tab.id, tab.pane().expect("a pane").id)
}

#[test]
fn tells_when_the_side_panel_changes_once_per_switch() {
    let (mut app, _rx) = empty_app();
    assert_eq!(told(&mut app), Vec::<String>::new());

    for (switch, shown) in [
        (App::toggle_changes as fn(&mut App), "changes"),
        (App::toggle_todo, "todo"),
        (App::toggle_files, "files"),
        (App::close_panel, "none"),
    ] {
        switch(&mut app);

        assert_eq!(told(&mut app), [format!("ui: panel shown={shown}")]);
        assert_eq!(told(&mut app), Vec::<String>::new(), "an unchanged panel is not logged twice");
    }
}

#[test]
fn tells_each_new_item_once_with_where_it_sits() {
    let (mut app, _rx, dirs) = app_with(1);
    let (project, workspace, tab, pane) = ids(&app);
    let path = canonical(&dirs[0]);

    assert_eq!(
        told(&mut app),
        [
            format!("app: project opened id={project} path={}", path.display()),
            format!(
                "app: workspace opened id={workspace} project={project} path={} label=default worktree=false",
                path.display()
            ),
            format!("app: tab opened id={tab} workspace={workspace}"),
            format!("app: pane opened id={pane} tab={tab}"),
            format!("ui: focus project={project} workspace={workspace} tab={tab} pane={pane}"),
        ]
    );
    assert_eq!(told(&mut app), Vec::<String>::new());
}

#[test]
fn tells_what_closed_from_the_inside_out() {
    let (mut app, _rx, _dirs) = app_with(1);
    app.add_tab(workspace_at(&app, 0, 0), AREA).expect("a second tab");
    told(&mut app);
    let (_, _, tab, pane) = ids(&app);

    app.remove(pane);

    let told = told(&mut app);
    assert_eq!(told[..2], [format!("app: pane closed id={pane}"), format!("app: tab closed id={tab}")]);
    assert!(told[2].starts_with("ui: focus "), "{told:?}");
}

#[test]
fn tells_when_a_dialog_opens_and_closes() {
    let (mut app, _rx, _dirs) = app_with(1);
    told(&mut app);

    app.open_settings();
    let opened = told(&mut app);
    app.overlay = None;

    assert_eq!(opened, ["ui: overlay opened kind=settings"]);
    assert_eq!(told(&mut app), ["ui: overlay closed kind=settings"]);
}

#[test]
fn tells_the_errors_it_shows() {
    let (mut app, _rx, _dirs) = app_with(1);
    told(&mut app);

    app.report_bug();

    assert_eq!(told(&mut app), ["ui: error shown text=cornercase hit a bug, see server.log"]);
}

#[test]
fn tells_which_settings_changed_but_not_to_what() {
    let (app, _rx, _dirs) = app_with(1);
    let mut config = app.config.clone();
    config.memory = !config.memory;

    assert_eq!(config::Changes::between(&app.config, &config).keys(), ["memory"]);
}
