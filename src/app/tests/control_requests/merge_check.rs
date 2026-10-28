use super::*;
use crate::test_util::Checkouts;

const LINES: &str = "one\ntwo\nthree\n";

struct Two {
    app: App,
    rx: Receiver<AppEvent>,
    _project: Checkouts,
    first: u64,
    second: u64,
}

fn write(dir: &Path, name: &str, text: &str) {
    std::fs::write(dir.join(name), text).expect("write file");
}

fn scanned(app: &mut App, rx: &Receiver<AppEvent>) {
    app.scan_overlaps(Instant::now());
    pump_until(app, rx, "the scan answers", |a| a.git.overlaps.scanning.is_empty());
}

fn unscanned() -> Two {
    let mut project = Checkouts::new(&[("a.txt", LINES)]);
    let (first, second) = (project.add("w1", "main"), project.add("w2", "main"));
    write(&first, "a.txt", "one\nfirst\nthree\n");
    write(&second, "a.txt", "one\nsecond\nthree\n");
    let (mut app, rx) = app_in(project.repo.path(), no_config());
    app.config.shared_files = true;
    app.sync_worktrees();
    let ids: Vec<u64> = app.projects[0].workspaces.iter().map(|w| w.id).collect();
    Two { app, rx, _project: project, first: ids[1], second: ids[2] }
}

fn two() -> Two {
    let mut found = unscanned();
    scanned(&mut found.app, &found.rx);
    found
}

fn workspace_json(app: &App, id: u64) -> Value {
    let report = serde_json::to_value(app.report(None)).expect("json");
    let workspaces = report["projects"][0]["workspaces"].as_array().expect("workspaces").clone();
    workspaces.into_iter().find(|w| w["id"] == id).expect("workspace")
}

#[test]
fn without_arguments_it_checks_the_base_and_each_workspace_sharing_files() {
    let mut found = two();

    let answer = call_later(&mut found.app, &found.rx, "changes-merge-check", &json!({"workspace": found.first}))
        .expect("an answer");

    assert_eq!(answer["workspace"], found.first);
    let (base, pair) = (&answer["checks"][0], &answer["checks"][1]);
    assert_eq!((&base["base"], &base["clean"], &base["with"]), (&json!("main"), &json!(true), &Value::Null));
    assert_eq!((&pair["with"], &pair["clean"]), (&json!(found.second), &json!(false)));
    assert_eq!((&pair["conflicts"][0]["path"], &pair["conflicts"][0]["kind"]), (&json!("a.txt"), &json!("contents")));
}

#[test]
fn with_checks_only_that_workspace() {
    let mut found = two();
    let args = json!({"workspace": found.first, "with": found.second});

    let answer = call_later(&mut found.app, &found.rx, "changes-merge-check", &args).expect("an answer");

    assert_eq!(answer["checks"].as_array().map(Vec::len), Some(1));
    assert_eq!(answer["checks"][0]["with"], found.second);
}

#[test]
fn base_checks_only_that_branch_and_names_one_that_is_missing() {
    let mut found = two();
    let args = json!({"workspace": found.first, "base": "nope"});

    let answer = call_later(&mut found.app, &found.rx, "changes-merge-check", &args).expect("an answer");

    let check = &answer["checks"][0];
    assert_eq!(answer["checks"].as_array().map(Vec::len), Some(1));
    assert_eq!((&check["clean"], &check["error"]), (&json!(false), &json!("no branch nope")));
}

#[test]
fn with_and_base_together_are_malformed() {
    let mut found = two();
    let args = json!({"workspace": found.first, "with": found.second, "base": "main"});

    assert_eq!(refused(&mut found.app, "changes-merge-check", &args).code, "malformed");
}

#[test]
fn with_names_a_workspace_of_another_project_or_the_same_folder_as_a_conflict() {
    let mut found = two();
    let other = repo_with_edit();
    let p = found.app.add_project(other.path().to_path_buf(), AREA).expect("project");
    let elsewhere = found.app.projects[p].workspaces[0].id;

    let abroad = refused(&mut found.app, "changes-merge-check", &json!({"workspace": found.first, "with": elsewhere}));
    let itself =
        refused(&mut found.app, "changes-merge-check", &json!({"workspace": found.first, "with": found.first}));

    assert_eq!((abroad.code.as_str(), itself.code.as_str()), ("conflict", "conflict"));
    assert!(abroad.message.contains("another project"), "{}", abroad.message);
    assert!(itself.message.contains("one checkout"), "{}", itself.message);
}

#[test]
fn a_plain_folder_is_a_conflict() {
    let plain = TempDir::new();
    let (mut app, _rx) = app_in(plain.path(), no_config());

    assert_eq!(refused(&mut app, "changes-merge-check", &json!({})).code, "conflict");
}

#[test]
fn status_lists_the_shared_files_only_when_there_are_some() {
    let found = two();

    let first = workspace_json(&found.app, found.first);
    let main = workspace_json(&found.app, found.app.projects[0].workspaces[0].id);

    assert_eq!(first["shared"], json!([{"workspace": found.second, "count": 1, "files": ["a.txt"]}]));
    assert!(main.get("shared").is_none(), "{main}");
}

#[test]
fn status_lists_at_most_a_hundred_files_and_still_counts_them_all() {
    let mut found = unscanned();
    for id in [found.first, found.second] {
        let path = found.app.projects.workspace_by(id).expect("workspace").path.clone();
        (0..101).for_each(|n| write(&path, &format!("f{n:03}.txt"), "x\n"));
    }
    scanned(&mut found.app, &found.rx);

    let first = workspace_json(&found.app, found.first);

    assert_eq!(first["shared"][0]["count"], 102);
    assert_eq!(first["shared"][0]["files"].as_array().map(Vec::len), Some(100));
}

#[test]
fn a_watcher_hears_when_two_workspaces_start_sharing_a_file() {
    let mut found = unscanned();
    let before = found.app.refreshed();

    scanned(&mut found.app, &found.rx);

    assert_ne!(found.app.refreshed(), before);
}
