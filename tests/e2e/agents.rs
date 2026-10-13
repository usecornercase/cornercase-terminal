use super::*;

#[test]
fn a_tab_running_claude_shows_what_it_is_doing() {
    let mut app = Harness::start();
    let sessions = app.session.claude_dir().join("sessions");
    std::fs::create_dir_all(&sessions).expect("create the sessions folder");
    let bin = temp_dir("claude");
    let claude = bin.join("claude");
    write_executable(
        &claude,
        "#!/bin/sh\nprintf '{\"pid\":%s,\"status\":\"waiting\"}' $$ > \"$1/$$.json\"\nread answer\nrm -f \"$1/$$.json\"\n",
    );
    let tab = usize::from(ui::workspace_row(workspaces_list(), 1, &tab_lines(&[1]), 0, WorkspaceRow::Tab(0, 0)).y);
    let tab_row = |s: &str| s.lines().nth(tab).unwrap_or_default().to_string();

    app.send(format!("{} {}\r", claude.display(), sessions.display()).as_bytes());
    app.wait_for("the tab says claude needs you", |s| tab_row(s).contains("▌ ├ ! "));
    app.send(b"\r");

    app.wait_for("the icon goes once claude exits", |s| !tab_row(s).contains('!'));
    let _ = std::fs::remove_dir_all(&bin);
}

#[test]
fn claude_asking_in_a_hidden_tab_reaches_the_desktop_through_the_outer_terminal() {
    let mut app = Harness::start();
    let sessions = app.session.claude_dir().join("sessions");
    std::fs::create_dir_all(&sessions).expect("create the sessions folder");
    let bin = temp_dir("claude-asks");
    let claude = bin.join("claude");
    write_executable(
        &claude,
        "#!/bin/sh\nprintf '{\"pid\":%s,\"status\":\"busy\"}' $$ > \"$1/$$.json\"\n\
         while [ ! -e \"$1/ask\" ]; do sleep 0.05; done\n\
         printf '{\"pid\":%s,\"status\":\"waiting\"}' $$ > \"$1/$$.json\"\nread answer\n",
    );
    app.send(format!("{} {}\r", claude.display(), sessions.display()).as_bytes());
    let tab = usize::from(ui::workspace_row(workspaces_list(), 1, &tab_lines(&[1]), 0, WorkspaceRow::Tab(0, 0)).y);
    app.wait_for("the tab says claude works", |s| s.lines().nth(tab).unwrap_or_default().contains("◐"));
    app.click(workspace_row(&[1], WorkspaceRow::NewTab(0)));
    app.wait_for("the second tab is active", |s| s.contains("$ ") && !s.contains(&claude.display().to_string()));

    std::fs::write(sessions.join("ask"), "").expect("ask");

    app.wait_for_raw("ghostty is asked for a notification", |raw| {
        raw.contains("\x1b]777;notify;cornercase;claude needs you in ")
    });
    app.wait_for("a toast says where", |s| s.contains("claude needs you in "));
    let _ = std::fs::remove_dir_all(&bin);
}

#[test]
fn an_issue_is_read_then_handed_to_its_agent_in_its_own_worktree() {
    let session = Session::new();
    let worktrees = session.dir.join("worktrees");
    let gh = session.dir.join("gh");
    let list = r#"[{"number":7,"title":"Fix the login","state":"OPEN","labels":[],"author":{"login":"ana"},"updatedAt":"2026-09-19T12:00:00Z","url":"https://github.com/acme/shop/issues/7"}]"#;
    let view = r#"{"number":7,"title":"Fix the login","state":"OPEN","labels":[],"assignees":[],"author":{"login":"ana"},"updatedAt":"","url":"u","body":"The form **breaks** after:\n\n- typing\n- waiting","comments":[{"author":{"login":"bo"},"body":"Same here","createdAt":""}]}"#;
    let script =
        format!("#!/bin/sh\nif [ \"$2\" = view ]; then cat <<'EOF'\n{view}\nEOF\nelse cat <<'EOF'\n{list}\nEOF\nfi\n");
    write_executable(&gh, &script);
    let agent = session.dir.join("agent");
    let agent_script = "#!/bin/sh\nprintf 'Do you trust the files in this folder?\\n'\nread answer\n\
        printf '\\033[2J\\033[Hagent ready> '\nread line\nprintf '%s' \"$line\" > got\n";
    write_executable(&agent, agent_script);
    let config = format!(
        r#"{{"worktrees_dir": "{}", "gh": "{}", "agent": "fake", "agent_commands": {{"fake": "{}"}}}}"#,
        worktrees.display(),
        gh.display(),
        agent.display()
    );
    std::fs::write(session.dir.join("config.json"), config).expect("write config");
    let repo = std::env::temp_dir().join(format!("ccis-{}", std::process::id()));
    std::fs::create_dir_all(&repo).expect("create repo dir");
    let repo_name = repo.file_name().and_then(|n| n.to_str()).expect("repo name").to_string();
    git(&repo, &["init", "--quiet"]);
    git(&repo, &["commit", "--quiet", "--allow-empty", "-m", "init"]);
    let mut app = Harness::open(session, ROWS, COLS);
    app.wait_for("app starts with one terminal", |s| s.contains(&first_entry()));
    app.open_project(1, &repo);
    app.wait_for("the repo opens as project 2", |s| s.contains(&entry(&repo_name)));

    app.click(areas().issues.as_position());
    app.wait_for("the issues show", |s| s.contains("#7") && s.contains("Fix the login"));
    app.send(b"\r");
    app.wait_for("the issue shows rendered", |s| {
        s.contains("The form breaks after:") && s.contains("• typing") && s.contains("@bo") && s.contains("Same here")
    });
    let detail_buttons = ["start", "agent…", "raw", "copy url", "back"];
    app.click(ui::issue_buttons(ui::issues_area(AREA), &detail_buttons)[3].as_position());
    let osc52 =
        String::from_utf8(cornercase::host::clipboard::osc52("https://github.com/acme/shop/issues/7")).expect("utf-8");
    app.wait_for_raw("the URL reaches the outer terminal's clipboard", |raw| raw.contains(&osc52));
    app.send(b"\r");

    app.wait_for("the agent asks whether to trust the folder", |s| {
        s.contains("#7 Fix the login") && s.contains("Do you trust the files") && !s.contains("cancel")
    });
    app.send(b"\r");
    app.wait_for("the agent starts in the issue workspace with the prompt typed", |s| {
        s.contains("#7 Fix the login") && s.contains("agent ready> https://") && !s.contains("cancel")
    });
    assert!(worktrees.join(&repo_name).join("issue-7-fix-the-login").is_dir(), "no checkout");
    let got = worktrees.join(&repo_name).join("issue-7-fix-the-login").join("got");
    assert!(!got.exists(), "the prompt was sent without Enter");
    app.send(b"\r");
    let deadline = Instant::now() + TIMEOUT;
    while std::fs::read_to_string(&got).ok().as_deref() != Some("https://github.com/acme/shop/issues/7") {
        assert!(Instant::now() < deadline, "the agent never got the prompt");
        thread::sleep(POLL);
    }
    let _ = std::fs::remove_dir_all(&repo);
}
