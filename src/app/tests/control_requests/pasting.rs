use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use rstest::rstest;
use serde_json::{Value, json};

use super::driving::{hidden_tab, raw_cat};
use super::*;
use crate::app::files_panel::FilesEvent;
use crate::project::Hierarchy;

fn encoded(bytes: &[u8]) -> String {
    STANDARD.encode(bytes)
}

fn keeping(app: &mut App) -> TempDir {
    let temp = TempDir::new();
    app.set_pasted_dir(&temp.path().join("pasted"));
    temp
}

fn folders(temp: &TempDir) -> Vec<PathBuf> {
    std::fs::read_dir(temp.path().join("pasted"))
        .map(|entries| entries.flatten().map(|e| e.path()).collect())
        .unwrap_or_default()
}

#[test]
fn a_pasted_file_is_kept_and_its_path_pasted() {
    let (mut app, rx) = app();
    let temp = keeping(&mut app);
    let id = first(&app);
    raw_cat(&mut app, &rx, id, "printf '\\033[?2004h'; ");

    let reply =
        asked(&mut app, &rx, "paste-file", &json!({"pane": id, "name": "My shot.png", "data": encoded(b"hello")}))
            .expect("pasted");

    let path = PathBuf::from(reply["path"].as_str().expect("path"));
    assert_eq!((reply["pane"].as_u64(), reply["bytes"].as_u64()), (Some(id), Some(5)));
    assert_eq!(path.file_name().and_then(|n| n.to_str()), Some("My-shot.png"));
    assert!(path.starts_with(temp.path().join("pasted")), "{path:?}");
    assert_eq!(std::fs::read(&path).expect("file"), b"hello");
    let mode = |p: &Path| std::fs::metadata(p).expect("metadata").permissions().mode() & 0o777;
    assert_eq!((mode(&path), mode(path.parent().expect("folder"))), (0o600, 0o700));
    let shows = format!("^[[200~{} ^[[201~", path.display());
    pump_until(&mut app, &rx, "the path is pasted", |a| pane_screen(a, id).contains("^[[200~"));
    let screen = pane_screen(&app, id);
    let flat: String = screen.split_whitespace().collect();
    let expected: String = shows.split_whitespace().collect();
    assert!(flat.contains(&expected), "{screen}");
}

#[rstest]
#[case::no_pane(json!({"data": "aGk="}), "malformed")]
#[case::unknown_pane(json!({"pane": 99999, "data": "aGk="}), "not-found")]
#[case::empty_data(json!({"pane": "FIRST", "name": "a", "data": ""}), "malformed")]
fn what_cannot_be_pasted_is_refused_at_once(#[case] args: Value, #[case] code: &str) {
    let (mut app, _rx) = app();
    let _temp = keeping(&mut app);
    let args = if args["pane"] == "FIRST" { json!({"pane": first(&app), "name": "a", "data": ""}) } else { args };

    let refusal = refused(&mut app, "paste-file", &args);

    assert_eq!(refusal.code, code, "{}", refusal.message);
}

#[test]
fn a_server_without_a_folder_says_so() {
    let (mut app, _rx) = app();
    let id = first(&app);

    let refusal = refused(&mut app, "paste-file", &json!({"pane": id, "data": "aGk="}));

    assert_eq!(refusal.code, "failed");
    assert!(refusal.message.contains("keeps no pasted files"), "{}", refusal.message);
}

#[test]
fn bytes_that_are_not_base64_are_refused_from_the_job_and_leave_nothing() {
    let (mut app, rx) = app();
    let temp = keeping(&mut app);
    let id = first(&app);

    let refusal = asked(&mut app, &rx, "paste-file", &json!({"pane": id, "data": "%%%"})).expect_err("refused");

    assert_eq!(refusal.code, "malformed", "{}", refusal.message);
    assert_eq!(folders(&temp), Vec::<PathBuf>::new());
}

#[test]
fn the_folder_goes_when_its_pane_closes() {
    let (mut app, rx) = app();
    let temp = keeping(&mut app);
    let hidden = hidden_tab(&mut app);
    let reply = asked(&mut app, &rx, "paste-file", &json!({"pane": hidden, "name": "a.txt", "data": encoded(b"x")}))
        .expect("pasted");
    let path = PathBuf::from(reply["path"].as_str().expect("path"));
    assert!(path.exists());
    let tab = app.projects.tab_of_pane(hidden).map(|at| app.projects.tab(at).id).expect("its tab");

    ok(&mut app, "close", &json!({"item": {"tab": tab}}));

    wait_until("the folder is removed", || {
        app.refresh(Instant::now());
        drain(&mut app, &rx);
        folders(&temp).is_empty()
    });
}

#[test]
fn nothing_is_pasted_for_a_client_that_left() {
    let (mut app, rx) = app();
    let temp = keeping(&mut app);
    let id = first(&app);
    let text = json!({"contract": 1, "command": "paste-file", "args": {"pane": id, "data": encoded(b"x")}}).to_string();

    app.request(CLIENT, &text, Some(AREA), Instant::now());
    app.forget(CLIENT);

    loop {
        let event = rx.recv_timeout(Duration::from_secs(30)).expect("the job's answer");
        let stored = matches!(event, AppEvent::Files(FilesEvent::Stored { .. }));
        app.handle_event(event, AREA).expect("handle event");
        if stored {
            break;
        }
    }
    wait_until("the folder is removed", || folders(&temp).is_empty());
    assert!(pane(&app, id).input_at.is_none());
}
