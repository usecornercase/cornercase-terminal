use super::*;

fn recording(session: &Session, name: &str, status: i32) {
    let args = session.dir.join(format!("{name}.args"));
    write_executable(
        &session.dir.join("bin").join(name),
        &format!("#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\nexit {status}\n", args.display()),
    );
}

fn local_window(session: &Arc<Session>, code_status: i32) -> Harness {
    std::fs::create_dir_all(session.dir.join("bin")).expect("bin folder");
    for name in ["xdg-open", "open"] {
        recording(session, name, 0);
    }
    recording(session, "code", code_status);
    let path = format!("{}:{}", session.dir.join("bin").display(), std::env::var("PATH").unwrap_or_default());
    let env = [("PATH", path.as_str()), ("SSH_CONNECTION", ""), ("SSH_TTY", ""), ("DISPLAY", ":0")];
    let mut ui = Harness::open_with(Arc::clone(session), ROWS, COLS, &env);
    ui.wait_for("the project row shows", |s| s.contains(&first_entry()));
    ui
}

fn choose_vscode(ui: &mut Harness) {
    ui.right_click(ui.position_of(&first_entry()));
    ui.wait_for("the menu offers open in…", |s| s.contains("open in…"));
    ui.click(ui.position_of("open in…"));
    ui.wait_for("the openers are listed", |s| s.contains("VS Code"));
    ui.click(ui.position_of("VS Code"));
}

#[test]
fn open_in_runs_the_editor_of_a_local_window_on_the_project_folder() {
    let session = Session::new();
    let mut ui = local_window(&session, 0);

    choose_vscode(&mut ui);

    let folder = temp().display().to_string();
    session.wait_for_file("code.args", "the editor ran on the folder", |args| args.trim() == folder);
}

#[test]
fn a_failing_opener_shows_a_toast_with_its_exit_status() {
    let session = Session::new();
    let mut ui = local_window(&session, 3);

    choose_vscode(&mut ui);

    ui.wait_for("the failure shows", |s| s.contains("could not open in VS Code: exit status 3"));
}

#[test]
fn a_window_over_ssh_is_offered_copy_path() {
    let mut ui = Harness::open(Session::new(), ROWS, COLS);
    ui.wait_for("the project row shows", |s| s.contains(&first_entry()));

    ui.right_click(ui.position_of(&first_entry()));

    ui.wait_for("the menu offers copy path", |s| s.contains("copy path") && !s.contains("open in…"));
}
