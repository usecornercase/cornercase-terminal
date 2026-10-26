use super::*;

const MARKER: &str = "zq-body-marker";

#[test]
fn the_issues_are_read_and_started_through_the_server_and_never_logged() {
    let session = Session::new();
    let view = format!(
        r#"{{"number":7,"title":"Fix the login","state":"OPEN","labels":[],"assignees":[],"author":{{"login":"ana"}},"updatedAt":"","url":"https://github.com/acme/shop/issues/7","body":"It breaks {MARKER}","comments":[]}}"#
    );
    let repo = session.dir.join("repo");
    let (worktrees, repo_name) = fake_issue_tracker(&session, &view, r#""accept_trust_prompts": true, "#, &repo);
    let mut app = Harness::open(Arc::clone(&session), ROWS, COLS);
    app.wait_for("app starts with one terminal", |s| s.contains(&first_entry()));
    let project = session.says(&["open", &repo.display().to_string()]);

    let sources = session.says(&["issue", "sources", "--project", &project]);
    assert!(
        sources.lines().next().is_some_and(|line| line.starts_with("github") && line.contains("connected")),
        "{sources}"
    );
    let listed = session.says(&["issue", "list", "--source", "github", "--project", &project, "--json"]);
    assert!(listed.contains("issue-7-fix-the-login"), "{listed}");
    let read = session.says(&["issue", "read", "7", "--project", &project]);
    assert!(read.contains("Fix the login") && read.contains(MARKER), "{read}");

    let pane = session.says(&["start", "--issue", "7", "--project", &project]);
    assert!(pane.parse::<u64>().is_ok(), "{pane}");
    let checkout = worktrees.join(&repo_name).join("issue-7-fix-the-login");
    let got = checkout.join("got");
    session.wait_for_file(&got.display().to_string(), "the agent got the prompt", |text| {
        text == "https://github.com/acme/shop/issues/7"
    });

    let log = session.says(&["logs", "-n", "1000"]);
    assert!(log.contains("issues: "), "the log tells nothing of the issues:\n{log}");
    assert!(!log.contains(MARKER), "the log holds the issue's text:\n{log}");
    let own = |line: &&str| line.contains(" issues: ") || line.contains(" control: ");
    for line in log.lines().filter(own) {
        assert!(!line.contains("Fix the login"), "a request or a job logs the title: {line}");
    }
}

#[test]
fn a_tracker_that_fails_keeps_its_cached_list_and_exits_3() {
    let session = Session::new();
    let repo = session.dir.join("repo");
    fake_issue_tracker(&session, "{}", "", &repo);
    let started = session.cold(&["start-server"]);
    assert!(started.status.success(), "start-server failed: {started:?}");
    let project = session.says(&["open", &repo.display().to_string()]);
    let list = ["issue", "list", "--source", "github", "--project", &project, "--json"];
    session.says(&list);
    write_executable(&session.dir.join("gh"), "#!/bin/sh\necho boom >&2\nexit 1\n");

    let out = session.cli(&list);

    assert_eq!(out.status.code(), Some(3), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stderr).contains("GitHub: "), "{out:?}");
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    assert_eq!(json["issues"][0]["key"], "#7", "{json}");
    assert_eq!(json["sources"][0]["cached"], true, "{json}");
    assert!(json["sources"][0]["error"].as_str().is_some_and(|e| e.contains("boom")), "{json}");
}
