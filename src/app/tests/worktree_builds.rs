use super::worktrees_dialog::{Setup, at, dialog, opened, path_of, shown, tick, view, with};
use super::*;
use crate::config::ProjectSettings;

fn write(path: &Path, bytes: usize) {
    std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    std::fs::write(path, vec![7_u8; bytes]).expect("write");
}

fn with_builds(s: &Setup, branch: &str, folder: &str) -> PathBuf {
    let at = path_of(s, branch);
    std::fs::write(at.join(".gitignore"), format!("{folder}/\n")).expect("ignore");
    write(&at.join(folder).join("x.js"), 300_000);
    at
}

fn sized(s: &mut Setup, rows: usize) {
    opened(s, rows);
    pump_until(&mut s.app, &s.rx, "every size is measured", |a| dialog(a).list.sizes_pending() == 0);
}

fn free_row(s: &mut Setup, branch: &str) {
    let pos = at(&s.app, ui::WorktreesHit::Free(shown(&s.app, branch)));
    click(&mut s.app, pos);
    pump_until(&mut s.app, &s.rx, "the processes are checked", |a| {
        view(a).confirm.as_ref().is_some_and(|c| !c.checking)
    });
}

fn confirm_free(s: &mut Setup) {
    let pos = at(&s.app, ui::WorktreesHit::Remove);
    click(&mut s.app, pos);
}

fn lines(app: &App) -> String {
    let view = view(app);
    view.confirm.as_ref().expect("a confirmation").lines.iter().map(ToString::to_string).collect::<Vec<_>>().join("\n")
}

fn freed(s: &mut Setup) -> String {
    pump_until(&mut s.app, &s.rx, "the toast says what was freed", |a| {
        toast(a).is_some_and(|t| t.contains("freed") || t.contains("nothing was freed"))
    });
    toast(&s.app).expect("a toast").to_string()
}

#[test]
fn a_row_shows_its_size_and_the_part_in_builds_once_measured() {
    let mut s = with(&["a"]);
    with_builds(&s, "a", "node_modules");
    sized(&mut s, 1);
    let row = view(&s.app).rows.into_iter().next().expect("a row");
    assert!(row.size.ends_with("KB") || row.size.ends_with("MB"), "{}", row.size);
    assert!(row.builds.is_some_and(|b| b.ends_with(" builds")));
    assert!(row.free);
}

#[test]
fn the_row_button_asks_first_and_deletes_nothing_before_the_confirmation() {
    let mut s = with(&["a"]);
    let at = with_builds(&s, "a", "node_modules");
    sized(&mut s, 1);
    free_row(&mut s, "a");
    let text = lines(&s.app);
    assert!(text.contains("frees ") && text.contains("node_modules"), "{text}");
    assert!(at.join("node_modules/x.js").exists());
    assert!(matches!(dialog(&s.app).step, crate::worktrees::Step::Free { .. }));
}

#[test]
fn confirming_frees_the_folder_and_keeps_everything_else() {
    let mut s = with(&["a"]);
    let at = with_builds(&s, "a", "node_modules");
    write(&at.join("mine.txt"), 10);
    std::fs::write(at.join("README"), "edited").expect("edit");
    sized(&mut s, 1);
    let before = dialog(&s.app).list.rows[0].size.clone();
    free_row(&mut s, "a");
    confirm_free(&mut s);
    let text = freed(&mut s);
    assert!(text.starts_with("freed ") && text.ends_with("in 1 worktree"), "{text}");
    assert!(!at.join("node_modules").exists());
    assert!(at.join("mine.txt").exists());
    assert_eq!(std::fs::read_to_string(at.join("README")).expect("read"), "edited");
    let branches = std::process::Command::new("git")
        .arg("-C")
        .arg(s.repo.path())
        .args(["branch", "--list", "a"])
        .output()
        .expect("git");
    assert!(String::from_utf8_lossy(&branches.stdout).contains('a'));
    let after = dialog(&s.app).list.rows[0].size.clone();
    assert_ne!(before, after);
    assert!(!dialog(&s.app).list.rows[0].freeing);
    assert!(!view(&s.app).rows[0].free);
}

#[test]
fn a_program_in_the_worktree_is_warned_about_and_confirming_still_frees() {
    let mut s = with(&["a"]);
    let at = with_builds(&s, "a", "node_modules");
    let sleeper = crate::test_util::Running::spawn(std::process::Command::new("/bin/sleep").arg("30").current_dir(&at));
    wait_until("sleep works in the folder", || process::cwd(sleeper.pid()).as_deref() == Some(at.as_path()));
    sized(&mut s, 1);
    pump_until(&mut s.app, &s.rx, "the first look ends", |a| !a.worktree_outside_running());
    free_row(&mut s, "a");
    let text = lines(&s.app);
    assert!(text.contains("1 process outside cornercase: sleep"), "{text}");
    assert!(text.contains("it may still use these folders") || text.contains("outside cornercase"));
    confirm_free(&mut s);
    freed(&mut s);
    assert!(!at.join("node_modules").exists());
}

#[test]
fn ticked_rows_free_together_with_one_toast() {
    let mut s = with(&["a", "b"]);
    let (a, b) = (with_builds(&s, "a", "node_modules"), with_builds(&s, "b", "node_modules"));
    sized(&mut s, 2);
    tick(&mut s, "a");
    tick(&mut s, "b");
    let pos = at(&s.app, ui::WorktreesHit::FreeTicked);
    click(&mut s.app, pos);
    pump_until(&mut s.app, &s.rx, "the processes are checked", |a| {
        view(a).confirm.as_ref().is_some_and(|c| !c.checking)
    });
    confirm_free(&mut s);
    let text = freed(&mut s);
    assert!(text.ends_with("in 2 worktrees"), "{text}");
    assert!(!a.join("node_modules").exists() && !b.join("node_modules").exists());
}

#[test]
fn the_projects_own_list_decides_which_folders_go() {
    let mut s = with(&["a"]);
    let at = with_builds(&s, "a", "node_modules");
    std::fs::write(at.join(".gitignore"), "node_modules/\nout/\n").expect("ignore");
    write(&at.join("out/y.js"), 300_000);
    let key = s.repo.path().display().to_string();
    s.app.config.projects.insert(key, ProjectSettings { builds: Some(vec!["out".into()]), ..Default::default() });
    sized(&mut s, 1);
    free_row(&mut s, "a");
    let text = lines(&s.app);
    assert!(text.contains("out") && !text.contains("node_modules"), "{text}");
    confirm_free(&mut s);
    freed(&mut s);
    assert!(!at.join("out").exists() && at.join("node_modules").exists());
}

#[test]
fn the_size_order_and_the_total_show_once_every_size_is_known() {
    let mut s = with(&["a", "b"]);
    with_builds(&s, "b", "node_modules");
    sized(&mut s, 2);
    let pos = at(&s.app, ui::WorktreesHit::Order(crate::worktrees::Order::Size));
    click(&mut s.app, pos);
    let view = view(&s.app);
    assert_eq!(view.rows[0].branch, "b");
    let (long, short) = view.total.expect("a total");
    assert!(long.contains("in 2 worktrees") && !short.contains("worktrees"), "{long} {short}");
}
