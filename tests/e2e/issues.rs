use super::*;

const MARKER: &str = "zq-body-marker";

#[test]
fn the_issues_are_read_and_started_through_the_server_and_never_logged() {
    let session = Session::new();
    let worktrees = session.dir.join("worktrees");
    let gh = session.dir.join("gh");
    let list = r#"[{"number":7,"title":"Fix the login","state":"OPEN","labels":[],"author":{"login":"ana"},"updatedAt":"2026-09-19T12:00:00Z","url":"https://github.com/acme/shop/issues/7"}]"#;
    let view = format!(
        r#"{{"number":7,"title":"Fix the login","state":"OPEN","labels":[],"assignees":[],"author":{{"login":"ana"}},"updatedAt":"","url":"https://github.com/acme/shop/issues/7","body":"It breaks {MARKER}","comments":[]}}"#
    );
    let script =
        format!("#!/bin/sh\nif [ \"$2\" = view ]; then cat <<'EOF'\n{view}\nEOF\nelse cat <<'EOF'\n{list}\nEOF\nfi\n");
    write_executable(&gh, &script);
    let agent = session.dir.join("agent");
    write_executable(
        &agent,
        "#!/bin/sh\nprintf 'Do you trust the files in this folder?\\n'\nread answer\n\
         printf '\\033[2J\\033[Hagent ready> '\nread line\nprintf '%s' \"$line\" > got\n",
    );
    let config = format!(
        r#"{{"worktrees_dir": "{}", "gh": "{}", "agent": "fake", "accept_trust_prompts": true, "agent_commands": {{"fake": "{}"}}}}"#,
        worktrees.display(),
        gh.display(),
        agent.display()
    );
    std::fs::write(session.dir.join("config.json"), config).expect("write config");
    let repo = session.dir.join("repo");
    std::fs::create_dir_all(&repo).expect("create repo dir");
    let repo_name = repo.file_name().and_then(|n| n.to_str()).expect("repo name").to_string();
    git(&repo, &["init", "--quiet"]);
    git(&repo, &["commit", "--quiet", "--allow-empty", "-m", "init"]);
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
