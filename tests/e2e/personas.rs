use serde_json::Value;

use super::*;

const REVIEWER: &str = "---\nname: Code Reviewer\ndescription: Reviews code.\n---\n\nYou review code.\n";
const README: &str = "# Agents\n";
const TESTER: &str = "---\nname: Tester\n---\n\nYou test.\n";
const DIVISIONS: &str = r#"{"divisions": {"engineering": {"label": "Engineering", "icon": "code"}}}"#;

fn json(session: &Session, args: &[&str]) -> Value {
    serde_json::from_str(&session.says(args)).expect("a JSON answer")
}

#[test]
fn a_persona_is_imported_from_a_file_listed_and_removed() {
    let app = Harness::start();
    let file = app.session.folder("agents").join("engineering-code-reviewer.md");
    std::fs::write(&file, REVIEWER).expect("write the persona");
    let path = file.to_str().expect("a path");

    let preview = app.session.says(&["persona", "import", path, "--check"]);
    let said = app.session.says(&["persona", "import", path]);
    let list = json(&app.session, &["persona", "list", "--json"]);
    let again = app.session.cli(&["persona", "import", path]);

    assert!(preview.ends_with("would import 1 persona; nothing was imported"), "{preview}");
    assert!(said.starts_with("imported Code Reviewer as engineering-code-reviewer: "), "{said}");
    assert_eq!(list["personas"][0]["id"], "engineering-code-reviewer");
    assert_eq!(list["personas"][0]["source"].as_str(), Some(path));
    assert!(!again.status.success());
    assert!(String::from_utf8_lossy(&again.stderr).contains("pass --replace"), "{again:?}");
    let removed = app.session.says(&["persona", "remove", "engineering-code-reviewer"]);
    assert!(removed.starts_with("removed the persona engineering-code-reviewer"), "{removed}");
    assert_eq!(json(&app.session, &["persona", "list", "--json"])["personas"], serde_json::json!([]));
}

#[test]
fn a_folder_imports_every_persona_in_it_and_skips_the_rest() {
    let app = Harness::start();
    let dir = app.session.folder("catalogue");
    std::fs::create_dir_all(dir.join("engineering")).expect("a division");
    std::fs::write(dir.join("engineering").join("reviewer.md"), REVIEWER).expect("write");
    std::fs::write(dir.join("README.md"), README).expect("write");

    let out = app.session.cli(&["persona", "import", dir.to_str().expect("a path"), "--json"]);

    assert!(out.status.success(), "{out:?}");
    let answer: Value = serde_json::from_slice(&out.stdout).expect("JSON");
    assert_eq!(answer["saved"], true);
    assert_eq!(answer["personas"][0]["id"], "reviewer");
    assert!(String::from_utf8_lossy(&out.stderr).contains("skipped 1 file that is not a persona"), "{out:?}");
}

#[test]
fn a_catalogue_lists_every_persona_with_its_problem_and_only_imports_the_ids_asked_for() {
    let app = Harness::start();
    let dir = app.session.folder("catalogue");
    std::fs::create_dir_all(dir.join("engineering")).expect("a division");
    std::fs::write(dir.join("engineering").join("reviewer.md"), REVIEWER).expect("write");
    std::fs::write(dir.join("engineering").join("tester.md"), TESTER).expect("write");
    std::fs::write(dir.join("divisions.json"), DIVISIONS).expect("write");
    let mine = app.session.folder("mine").join("mine.md");
    std::fs::write(&mine, REVIEWER).expect("write");
    let path = dir.to_str().expect("a path");
    app.session.says(&["persona", "import", mine.to_str().expect("a path")]);

    let catalogue = json(&app.session, &["persona", "import", path, "--catalogue", "--json"]);
    let missing = app.session.cli(&["persona", "import", path, "--only", "tester", "--only", "nobody"]);
    let imported = json(&app.session, &["persona", "import", path, "--only", "tester", "--json"]);

    assert_eq!(catalogue["divisions"][0]["icon"], "code");
    assert_eq!(catalogue["personas"][0]["file"], "engineering/reviewer.md");
    assert_eq!(catalogue["personas"][0]["division"], "engineering");
    assert_eq!(catalogue["personas"][0]["problem"], "your persona `mine` is also named `Code Reviewer`");
    assert_eq!(catalogue["personas"][1]["problem"], Value::Null);
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("no persona `nobody`"), "{missing:?}");
    assert_eq!(imported["personas"].as_array().map(Vec::len), Some(1));
    assert_eq!(imported["personas"][0]["id"], "tester");
}
