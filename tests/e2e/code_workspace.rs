use serde_json::{Value, json};

use super::remote::remotely;
use super::requests::requesting;
use super::*;

const TEAM: &str = r#"{"folders":[{"path":"one"},{"path":"two","name":"Second"}]}"#;

struct Team {
    session: Arc<Session>,
    folder: PathBuf,
    file: String,
}

fn team_of(text: &str) -> Team {
    let session = Session::new();
    assert!(session.cold(&["start-server"]).status.success());
    let folder = session.dir.join("code").join("work");
    for name in ["one", "two"] {
        std::fs::create_dir_all(folder.join(name)).expect("create the folder");
    }
    let folder = folder.canonicalize().expect("canonical");
    let file = folder.join("team.code-workspace");
    std::fs::write(&file, text).expect("write the workspace");
    Team { session, folder, file: file.display().to_string() }
}

fn parsed(text: &[u8]) -> Value {
    serde_json::from_slice(text).expect("a JSON answer")
}

fn status(session: &Session) -> Value {
    parsed(session.says(&["status", "--json"]).as_bytes())
}

fn group_named(status: &Value, name: &str) -> Option<Value> {
    status["groups"].as_array()?.iter().find(|group| group["name"] == name).map(|group| group["id"].clone())
}

fn project_at(status: &Value, path: &std::path::Path) -> Value {
    let path = path.display().to_string();
    let projects = status["projects"].as_array().expect("projects");
    projects.iter().find(|project| project["path"] == path.as_str()).cloned().unwrap_or(Value::Null)
}

#[test]
fn open_imports_a_code_workspace_as_the_issue_asks() {
    let team = team_of(TEAM);

    let out = team.session.cli(&["open", &team.file, "--json"]);

    assert!(out.status.success(), "{out:?}");
    let answer = parsed(&out.stdout);
    let report = status(&team.session);
    let (one, two) = (project_at(&report, &team.folder.join("one")), project_at(&report, &team.folder.join("two")));
    let group = group_named(&report, "team").expect("a group named after the file");
    assert_eq!(answer["group"], group);
    assert_eq!(answer["name"], "team");
    assert_eq!(answer["projects"], json!([one["id"], two["id"]]));
    assert_eq!((answer["missing"].as_u64(), answer["failed"].as_u64()), (Some(0), Some(0)));
    assert_eq!((&one["group"], &two["group"]), (&group, &group));
    assert_eq!((&one["custom_name"], &two["custom_name"]), (&Value::Null, &json!("Second")));
}

#[test]
fn open_prints_the_groups_id_and_a_second_import_opens_nothing_twice() {
    let team = team_of(TEAM);

    let first = team.session.says(&["open", &team.file]);
    let projects = status(&team.session)["projects"].as_array().map(Vec::len);
    let second = team.session.says(&["open", &team.file]);

    assert_eq!(first, group_named(&status(&team.session), "team").expect("the group").to_string());
    assert_eq!(second, first);
    assert_eq!(status(&team.session)["projects"].as_array().map(Vec::len), projects);
}

#[test]
fn open_reads_a_relative_file_from_the_folder_it_runs_in() {
    let team = team_of(TEAM);

    let out = team.session.command().current_dir(&team.folder).args(["open", "team.code-workspace"]).output();

    let out = out.expect("run cornercase");
    assert!(out.status.success(), "{out:?}");
    assert!(project_at(&status(&team.session), &team.folder.join("two")).is_object());
}

#[test]
fn open_leaves_the_window_as_it_is_until_focus_shows_the_first_project() {
    let mut app = Harness::start();
    let folder = temp_dir("code-workspace-window");
    for name in ["ccimport-one", "ccimport-two"] {
        std::fs::create_dir_all(folder.join(name)).expect("create the folder");
    }
    let file = folder.join("team.code-workspace");
    let text = r#"{"folders": [{"path": "ccimport-one"}, {"path": "ccimport-two"}]}"#;
    std::fs::write(&file, text).expect("write the workspace");
    let file = file.display().to_string();

    app.session.says(&["open", &file]);

    app.wait_for("the group shows", |s| s.contains(" team"));
    assert!(app.text().contains(&format!("▌{}", first_entry())), "the window moved:\n{}", app.text());

    let answer = parsed(app.session.says(&["open", &file, "--focus", "--json"]).as_bytes());

    let marked = |line: &str| line.contains('▌') && line.contains(&entry("ccimport-one"));
    app.wait_for("the first project shows", |s| s.lines().any(marked));
    assert_eq!(status(&app.session)["shown"]["project"], answer["projects"][0]);
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn open_says_what_it_could_not_import_and_exits_with_3() {
    let team = team_of(r#"{"folders": [{"path": "one"}, {"path": "gone"}]}"#);

    let plain = team.session.cli(&["open", &team.file]);
    let json = team.session.cli(&["open", &team.file, "--json"]);

    let group = group_named(&status(&team.session), "team").expect("the group");
    assert_eq!(plain.status.code(), Some(3), "{plain:?}");
    assert_eq!(String::from_utf8_lossy(&plain.stdout).trim_end(), group.to_string());
    assert!(
        String::from_utf8_lossy(&plain.stderr).contains("warning: 1 project imported into team, 1 not found"),
        "{plain:?}"
    );
    assert_eq!(json.status.code(), Some(3), "{json:?}");
    let answer = parsed(&json.stdout);
    assert_eq!((&answer["group"], answer["missing"].as_u64()), (&group, Some(1)));
}

#[test]
fn open_with_nothing_to_import_makes_no_group_and_prints_no_id() {
    let team = team_of(r#"{"folders": [{"path": "gone"}]}"#);

    let plain = team.session.cli(&["open", &team.file]);
    let json = team.session.cli(&["open", &team.file, "--json"]);

    assert_eq!((plain.status.code(), plain.stdout.as_slice()), (Some(3), &b""[..]), "{plain:?}");
    assert!(String::from_utf8_lossy(&plain.stderr).contains("nothing imported from team, 1 not found"), "{plain:?}");
    assert_eq!(json.status.code(), Some(3), "{json:?}");
    assert_eq!(parsed(&json.stdout)["group"], Value::Null);
    assert_eq!(status(&team.session)["groups"], json!([]));
}

#[test]
fn open_refuses_a_broken_file_with_its_reason() {
    let team = team_of("{ nope");

    let out = team.session.cli(&["open", &team.file]);

    assert!(refused(&out, 1, "cannot read"), "{out:?}");
    assert_eq!(status(&team.session)["groups"], json!([]));
}

#[test]
fn open_into_puts_a_file_or_a_folder_in_that_group() {
    let team = team_of(r#"{"folders": [{"path": "one"}]}"#);
    let other = team.session.says(&["group", "new", "other"]);
    let two = team.folder.join("two");

    let imported = team.session.says(&["open", &team.file, "--into", &other]);
    let project = team.session.says(&["open", &two.display().to_string(), "--into", &other]);
    let missing = team.session.cli(&["open", &team.file, "--into", "99999"]);

    let report = status(&team.session);
    let want: u64 = other.parse().expect("an id");
    assert_eq!(imported, other);
    assert_eq!(group_named(&report, "team"), None);
    assert_eq!(project_at(&report, &team.folder.join("one"))["group"], want);
    let opened = project_at(&report, &two);
    assert_eq!((opened["group"].as_u64(), opened["id"].to_string()), (Some(want), project));
    assert!(refused(&missing, 1, "there is no group 99999"), "{missing:?}");
}

#[test]
fn open_through_ssh_reads_the_file_on_the_other_machine() {
    let team = team_of(r#"{"folders": [{"path": "one", "name": "First"}]}"#);
    let other = team.session.says(&["group", "new", "other"]);
    let two = team.folder.join("two").display().to_string();

    let there = remotely(&team.session, &["open", &team.file, "--json"]).output().expect("run cornercase");
    let into = remotely(&team.session, &["open", &two, "--into", &other]).output().expect("run cornercase");
    let relative = remotely(&team.session, &["open", "team.code-workspace"]).output().expect("run cornercase");

    assert!(there.status.success(), "{there:?}");
    assert_eq!(parsed(&there.stdout)["name"], "team");
    let report = status(&team.session);
    assert_eq!(project_at(&report, &team.folder.join("one"))["custom_name"], "First");
    assert!(into.status.success(), "{into:?}");
    assert_eq!(project_at(&report, &team.folder.join("two"))["group"].to_string(), other);
    assert!(refused(&relative, 2, "is relative: with --remote"), "{relative:?}");
}

#[test]
fn open_answers_on_a_line_of_requests_with_the_status_it_would_have() {
    let team = team_of(TEAM);
    let partial = team.folder.join("partial.code-workspace");
    std::fs::write(&partial, r#"{"folders": [{"path": "gone"}]}"#).expect("write the workspace");
    let mut typist = requesting(&team.session);

    let whole = typist.types(&json!({"args": ["open", team.file, "--json"]}));
    let none = typist.types(&json!({"args": ["open", partial, "--json"]}));

    assert_eq!((&whole["status"], &whole["json"]["name"]), (&json!(0), &json!("team")));
    assert_eq!((&none["status"], &none["json"]["missing"]), (&json!(3), &json!(1)));
    assert_eq!(typist.ends().status.code(), Some(3));
}
