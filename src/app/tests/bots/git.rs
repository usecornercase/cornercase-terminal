use super::*;

fn rule(project: &Path) -> String {
    let quoted = format!("{:?}", format!("Bash(git -C '{}' log:*)", project.display()));
    quoted.trim_matches('"').to_string()
}

#[test]
fn a_turn_may_read_the_git_of_each_open_project_and_none_under_the_bots_folder() {
    let shell = if cfg!(windows) { "cmd.exe" } else { "/bin/sh" };
    let (mut app, rx) = new_app_with_shell(shell, no_config());
    if cfg!(windows) {
        app.config.shell.args = vec![String::from("/d")];
    }
    let dirs = [TempDir::new(), TempDir::new()];
    for dir in &dirs {
        app.open_project(dir.path().to_path_buf(), AREA).expect("open project");
    }
    let (dir, _bin, claude) = install_bot(&mut app, ANSWER, Access::Cornercase);
    let inside = folder::of(dir.path(), "prs").join("x");
    std::fs::create_dir_all(&inside).expect("a folder in the bot's");
    app.open_project(inside.clone(), AREA).expect("open project");

    app.bot_say("prs", "hi", false).expect("say");
    settle(&mut app, &rx);

    let argv = std::fs::read_to_string(claude.with_extension("argv")).expect("the argv");
    for project in &dirs {
        let wanted = rule(project.path());
        assert!(argv.contains(&wanted), "{wanted} in {argv}");
    }
    assert!(!argv.contains(&rule(&inside)), "{argv}");
    assert!(!argv.contains("Bash(git log:*)"), "{argv}");
}
