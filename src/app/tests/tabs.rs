use super::*;

fn exited_last_tab() -> (App, Receiver<AppEvent>, Vec<TempDir>) {
    let (mut app, rx, dirs) = app_with(1);
    type_line(&mut app, "exit");
    pump_until(&mut app, &rx, "the tab closes", |a| a.projects[0].workspaces[0].tabs.is_empty());
    (app, rx, dirs)
}

#[test]
fn plus_tab_opens_one_in_the_workspace_folder() {
    let (mut app, _rx, dirs) = app_with(1);

    click_row(&mut app, WorkspaceRow::NewTab(0));

    let workspace = &app.projects[0].workspaces[0];
    assert_eq!((workspace.tabs.len(), workspace.active), (2, 1));
    wait_until("tab starts in the folder", || tab_term(&app, 0, 1).cwd() == Some(canonical(&dirs[0])));
}

#[test]
fn clicking_a_tab_selects_it() {
    let (mut app, _rx, _dirs) = app_with(1);
    click_row(&mut app, WorkspaceRow::NewTab(0));

    click_row(&mut app, WorkspaceRow::Tab(0, 0));

    assert_eq!(app.projects[0].workspaces[0].active, 0);
}

#[test]
fn keys_go_to_the_active_tab() {
    let (mut app, rx, _dirs) = app_with(1);
    click_row(&mut app, WorkspaceRow::NewTab(0));

    type_line(&mut app, "cd /");

    pump_until(&mut app, &rx, "second tab moves", |a| {
        tab_term(a, 0, 1).cwd().as_deref() == Some(Path::new("/"))
    });
    assert_ne!(tab_term(&app, 0, 0).cwd().as_deref(), Some(Path::new("/")));
}

#[test]
fn close_button_closes_that_tab() {
    let (mut app, rx, _dirs) = app_with(1);
    click_row(&mut app, WorkspaceRow::NewTab(0));

    click_close(&mut app, WorkspaceRow::Tab(0, 1));

    pump_until(&mut app, &rx, "second tab closes", |a| a.projects[0].workspaces[0].tabs.len() == 1);
}

struct Orphan(PathBuf);

impl Drop for Orphan {
    fn drop(&mut self) {
        if let Ok(pid) = std::fs::read_to_string(&self.0) {
            let _ = std::process::Command::new("kill").arg(pid.trim()).status();
        }
    }
}

#[test]
fn closing_one_whose_terminal_a_detached_process_keeps_open_closes_it() {
    let (mut app, rx, dirs) = app_with(1);
    click_row(&mut app, WorkspaceRow::NewTab(0));
    let orphan = Orphan(dirs[0].path().join("pid"));
    type_line(&mut app, &format!("(trap '' HUP; exec sleep 30) & echo $! > {}", orphan.0.display()));
    wait_until("the detached process starts", || {
        std::fs::read_to_string(&orphan.0).is_ok_and(|pid| pid.ends_with('\n'))
    });

    click_close(&mut app, WorkspaceRow::Tab(0, 1));

    wait_until("the tab closes", || {
        drain(&mut app, &rx);
        app.refresh(Instant::now());
        app.projects[0].workspaces[0].tabs.len() == 1
    });
}

#[test]
fn the_last_one_exiting_keeps_the_workspace() {
    let (app, _rx, _dirs) = exited_last_tab();
    assert_eq!((app.projects.len(), app.projects[0].workspaces.len()), (1, 1));
}

#[test]
fn plus_tab_works_in_an_empty_workspace() {
    let (mut app, _rx, _dirs) = exited_last_tab();

    click_row(&mut app, WorkspaceRow::NewTab(0));

    assert_eq!(app.projects[0].workspaces[0].tabs.len(), 1);
}

#[test]
fn are_named_after_their_program() {
    let (app, _rx, _dirs) = app_with(1);
    wait_until("the shell runs", || is_sh(&app.projects[0].workspaces[0].tabs[0].label(&app.config)));
}
