use super::*;
use crate::git::worktree::setup::Pane as Setting;

fn home(setup: &Setup, kind: Known, name: &str) -> String {
    let home = setup.config.path().join("accounts").join(kind.name()).join(name);
    home.canonicalize().expect("canonical home").display().to_string()
}

#[test]
fn a_shell_at_its_prompt_starts_again_on_the_account_in_the_same_place() {
    let mut setup = Setup::new(FakeClaude::SILENT);
    accounts::add(&setup.config.path().join("accounts"), Known::Codex, "work", None).expect("codex account");
    setup.app.config.active_accounts.insert("codex".into(), "work".into());
    let pane = setup.shell_on("work");
    setup.add("personal");
    let old_shell = setup.app.term().and_then(Term::shell_pid);

    setup.app.respawn_shell(pane, Known::Claude, "personal").expect("the shell starts again");

    let (claude, codex) = (home(&setup, Known::Claude, "personal"), home(&setup, Known::Codex, "work"));
    let line = format!("[ \"$CLAUDE_CONFIG_DIR $CODEX_HOME\" = '{claude} {codex}' ] && echo on-\"\"personal");
    type_line(&mut setup.app, &line);
    setup.typed("on-personal");
    let term = setup.app.projects.pane_by(pane).expect("the same pane");
    assert_ne!(term.shell_pid(), old_shell);
    assert_eq!(term.generation(), 1);
    assert_eq!(term.cwd(), Some(setup.project.path().to_path_buf()));
    assert_eq!(term.accounts().name(Known::Claude), Some("personal"));
    assert_eq!(term.accounts().name(Known::Codex), Some("work"));
    assert!(setup.app.agents.launches.of_pane(pane).next().is_none());
}

#[test]
fn the_agent_s_own_login_drops_only_its_account() {
    let mut setup = Setup::new(FakeClaude::SILENT);
    let pane = setup.shell_on("work");

    setup.app.respawn_shell(pane, Known::Claude, "").expect("the shell starts again");

    let term = setup.app.projects.pane_by(pane).expect("the same pane");
    assert_eq!((term.generation(), term.accounts().name(Known::Claude)), (1, None));
}

#[test]
fn the_same_account_or_a_missing_one_leaves_the_shell_as_it_is() {
    let mut setup = Setup::new(FakeClaude::SILENT);
    let pane = setup.shell_on("work");

    let same = setup.app.respawn_shell(pane, Known::Claude, "work").expect_err("already there");
    let missing = setup.app.respawn_shell(pane, Known::Claude, "nobody").expect_err("no such account");

    assert_eq!(same, format!("pane {pane} already uses that account"));
    assert_eq!(missing, "the claude account `nobody` is gone");
    assert_eq!(setup.app.projects.pane_by(pane).map(Term::generation), Some(0));
}

#[test]
fn a_program_in_front_of_the_shell_keeps_it() {
    let (mut setup, pane) = Setup::shell_beside_personal();
    setup.runs_sleep(pane);
    let old_shell = setup.app.term().and_then(Term::shell_pid);

    let refused = setup.app.respawn_shell(pane, Known::Claude, "personal").expect_err("sleep runs");

    assert_eq!(refused, "stop sleep first");
    assert_eq!(setup.app.shell_blocker(pane, Known::Claude), Some(refused));
    assert_eq!(setup.app.term().and_then(Term::shell_pid), old_shell);
}

#[test]
fn a_flow_seat_a_setup_or_a_resume_keeps_the_shell_and_says_why() {
    let (mut setup, pane) = Setup::shell_beside_personal();
    let refused = |setup: &mut Setup| setup.app.respawn_shell(pane, Known::Claude, "personal").expect_err("kept");

    setup.seat_coder(pane);
    assert_eq!(refused(&mut setup), "it is the coder of a flow run");
    setup.app.runs.clear();

    let term = setup.app.projects.pane_by_mut(pane).expect("the pane");
    term.setup = Some(Setting::Running { status: PathBuf::from("/nonexistent"), since: Instant::now() });
    assert_eq!(refused(&mut setup), "wait for its setup");
    let term = setup.app.projects.pane_by_mut(pane).expect("the pane");
    term.setup = None;

    term.resuming = Some(Instant::now());
    assert_eq!(refused(&mut setup), "wait for it to resume");
    assert_eq!(setup.app.projects.pane_by(pane).map(Term::generation), Some(0));
}

#[test]
fn a_line_the_person_is_typing_keeps_the_shell() {
    let (mut setup, pane) = Setup::shell_beside_personal();
    type_text(&mut setup.app, "echo half");
    assert!(setup.app.projects.pane_by(pane).is_some_and(|term| term.draft(Instant::now())));

    let refused = setup.app.respawn_shell(pane, Known::Claude, "personal").expect_err("a draft");

    assert_eq!(refused, "wait for what is typed in it to be sent or cleared");
    assert_eq!(setup.app.projects.pane_by(pane).map(Term::generation), Some(0));
}

#[test]
fn a_shell_that_cannot_start_again_keeps_its_requests() {
    let (mut setup, pane) = Setup::shell_beside_personal();
    setup.wait_on(pane, Until::Text("never-shown".into()));
    setup.app.shells = crate::app::tabs::Shells::new("/nonexistent/sh".into());

    setup.app.respawn_shell(pane, Known::Claude, "personal").expect_err("no shell to start");

    assert_eq!(setup.app.take_answers(), Vec::new());
    assert_eq!(setup.app.projects.pane_by(pane).map(Term::generation), Some(0));
}

fn shell_with_variable(folder: fn(&Setup) -> PathBuf) -> (Setup, u64) {
    let mut setup = Setup::new(FakeClaude::SILENT);
    setup.add("personal");
    let dir = folder(&setup);
    std::fs::create_dir_all(&dir).expect("the folder");
    let shell = setup.config.path().join("shell");
    write_executable(&shell, &format!("#!/bin/sh\nCLAUDE_CONFIG_DIR='{}' exec /bin/sh \"$@\"\n", dir.display()));
    setup.app.shells = crate::app::tabs::Shells::new(shell.display().to_string());
    setup.app.open_project(setup.project.path().to_path_buf(), AREA).expect("open the project");
    wait_until("the shell is at its prompt", || setup.app.term().is_some_and(Term::shell_in_foreground));
    let pane = setup.app.term().expect("the pane").id;
    (setup, pane)
}

#[cfg(target_os = "linux")]
#[test]
fn a_variable_set_outside_cornercase_keeps_the_shell() {
    let dir = |setup: &Setup| setup.project.path().join("elsewhere");
    let (mut setup, pane) = shell_with_variable(dir);
    let shell = setup.app.term().and_then(Term::shell_pid).expect("the shell's pid");
    let elsewhere = dir(&setup).display().to_string();
    wait_until("the shell runs with the variable", || {
        process::env(shell).iter().any(|(name, value)| name == "CLAUDE_CONFIG_DIR" && *value == elsewhere)
    });

    let refused = setup.app.respawn_shell(pane, Known::Claude, "personal").expect_err("set by hand");

    assert_eq!(refused, "its account was not set by cornercase");
    assert_eq!(setup.app.projects.pane_by(pane).map(Term::generation), Some(0));
}

#[rstest::rstest]
#[case::the_agent_s_own_login(|setup: &Setup| setup.config.path().join("homes/claude"))]
#[case::an_account_of_cornercase(|setup: &Setup| setup.config.path().join("accounts/claude/personal"))]
fn a_variable_naming_a_known_folder_lets_the_shell_start_again(#[case] folder: fn(&Setup) -> PathBuf) {
    let (mut setup, pane) = shell_with_variable(folder);

    setup.app.respawn_shell(pane, Known::Claude, "personal").expect("the shell starts again");

    assert_eq!(setup.app.projects.pane_by(pane).map(Term::generation), Some(1));
}
