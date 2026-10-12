use super::*;

#[test]
fn the_first_one_opens_in_the_server_folder() {
    let (app, _rx) = app();
    let here = std::env::current_dir().and_then(|d| d.canonicalize()).expect("current dir");
    assert_eq!(app.projects[0].path, here);
}

#[test]
fn a_new_one_has_one_workspace_with_one_tab_in_its_folder() {
    let (app, _rx, dirs) = app_with(1);
    let workspace = &app.projects[0].workspaces[0];
    assert_eq!((workspace.path.clone(), workspace.worktree, workspace.tabs.len()), (canonical(&dirs[0]), false, 1));
}

#[test]
fn opening_an_open_folder_switches_to_it() {
    let (mut app, _rx, dirs) = app_with(2);

    app.open_project(dirs[0].path().to_path_buf(), AREA).expect("open project");

    assert_eq!((app.projects.len(), app.active), (2, 0));
}

#[test]
fn the_name_does_not_follow_cd() {
    let (mut app, rx, dirs) = app_with(1);

    type_line(&mut app, "cd /");
    pump_until(&mut app, &rx, "shell changes dir", |a| term(a, 0).cwd().as_deref() == Some(Path::new("/")));

    let folder = dirs[0].path().file_name().and_then(|n| n.to_str()).expect("folder name");
    assert_eq!(app.project_label(&app.projects[0]), folder);
}
