use serde_json::Value;

use super::*;

fn fake_gh(session: &Session) {
    let fixture = format!("{}/tests/fixtures/gh/2.101.0/pr-view.json", env!("CARGO_MANIFEST_DIR"));
    let script = format!(
        "#!/bin/sh\nif [ \"$1\" = api ]; then echo '{{\"data\":{{\"resource\":{{\"stack\":null}}}}}}'; exit 0; fi\n\
         if [ -f \"$(dirname \"$0\")/logged-out\" ]; then\n\
         echo 'To get started with GitHub CLI, please run:  gh auth login' >&2; exit 1; fi\ncat '{fixture}'\n"
    );
    write_executable(&session.dir.join("gh"), &script);
    let config = format!("{{\"gh\": \"{}\"}}", session.dir.join("gh").display());
    std::fs::write(session.dir.join("config.json"), config).expect("write config");
}

fn repo_on_a_branch(name: &str) -> PathBuf {
    let repo = temp_dir(name);
    git(&repo, &["init", "--quiet"]);
    git(&repo, &["commit", "--quiet", "--allow-empty", "-m", "init"]);
    git(&repo, &["checkout", "--quiet", "-b", "e2e/pr"]);
    git(&repo, &["remote", "add", "origin", "https://github.com/acme/web-shop.git"]);
    repo
}

#[test]
fn a_workspaces_pull_request_is_read_with_every_check_and_a_failed_read_says_why() {
    let session = Session::new();
    fake_gh(&session);
    let repo = repo_on_a_branch("pull-request");
    let mut app = Harness::open(Arc::clone(&session), ROWS, COLS);
    app.wait_for("app starts with one terminal", |s| s.contains(&first_entry()));
    session.says(&["open", &repo.display().to_string(), "--focus"]);

    let json = session.says(&["pull-request", "--json"]);
    let json: Value = serde_json::from_str(&json).expect("the answer");
    assert_eq!(json["pull_request"]["number"], 482, "{json}");
    let first = &json["pull_request"]["all_checks"][0];
    assert_eq!((&first["name"], &first["outcome"]), (&Value::from("tests (ubuntu-24.04)"), &Value::from("failed")));
    assert!(first["url"].as_str().is_some_and(|url| url.ends_with("/job/2001")), "{first}");

    let text = session.says(&["pull-request"]);
    assert!(text.contains("pull request #482 open, checks failing"), "{text}");
    assert!(
        text.contains("failed   tests (ubuntu-24.04)  CI  https://github.com/acme/web-shop/actions/runs/1001/job/2001"),
        "{text}"
    );

    std::fs::write(session.dir.join("logged-out"), "").expect("log gh out");
    let out = session.cli(&["pull-request"]);
    assert!(refused(&out, 1, "gh is not logged in: run gh auth login"), "{out:?}");
    assert!(out.stdout.is_empty(), "{out:?}");
    let _ = std::fs::remove_dir_all(&repo);
}
