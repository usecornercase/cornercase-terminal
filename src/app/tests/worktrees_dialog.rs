use std::cell::RefCell;

use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::*;
use crate::app::worktrees::RemoveOptions;
use crate::app::worktrees_dialog::wrap;
use crate::flows::Flow;
use crate::flows::run::{RoleWorktree, Run};
use crate::test_util::git;
use crate::worktrees::Dialog;

struct Setup {
    app: App,
    rx: Receiver<AppEvent>,
    repo: TempDir,
    worktrees: TempDir,
    _config: TempDir,
}

fn with(names: &[&str]) -> Setup {
    let repo = git_repo(&[("README", "hi")]);
    let (worktrees, config, config_path) = with_worktrees_config();
    let (mut app, rx) = app_in(repo.path(), config_path);
    for (i, name) in names.iter().enumerate() {
        click_new_workspace(&mut app);
        submit_text(&mut app, name);
        pump_until(&mut app, &rx, "the workspace opens", |a| a.projects[0].workspaces.len() == i + 2);
    }
    Setup { app, rx, repo, worktrees, _config: config }
}

fn path_of(s: &Setup, branch: &str) -> PathBuf {
    let found = s.app.projects[0].workspaces.iter().find(|w| git::branch(&w.path).as_deref() == Some(branch));
    found.expect("the workspace exists").path.clone()
}

fn closed(s: &Setup) -> (TempDir, PathBuf) {
    let other = git_repo(&[("README", "other")]);
    let name = other.path().file_name().expect("a name").to_string_lossy().into_owned();
    let checkout = s.worktrees.path().join(&name).join("side");
    git(other.path(), &["worktree", "add", "-b", "side", checkout.to_str().expect("utf-8")]);
    (other, checkout)
}

fn dialog(app: &App) -> &Dialog {
    match &app.overlay {
        Some(Overlay::Worktrees(dialog)) => dialog,
        _ => panic!("the worktrees dialog is open"),
    }
}

fn opened(s: &mut Setup, rows: usize) {
    s.app.open_worktrees(None);
    ready(s, rows);
}

fn ready(s: &mut Setup, rows: usize) {
    pump_until(&mut s.app, &s.rx, "every worktree is listed and scanned", |a| {
        let list = &dialog(a).list;
        list.rows.len() == rows && list.pending() == 0
    });
}

fn view(app: &App) -> Box<ui::Worktrees> {
    match overlay_shown(app, AREA) {
        ui::Overlay::Worktrees(view) => view,
        _ => panic!("the worktrees dialog is shown"),
    }
}

fn at(app: &App, hit: ui::WorktreesHit) -> Position {
    let view = view(app);
    (0..AREA.height)
        .flat_map(|y| (0..AREA.width).map(move |x| Position::new(x, y)))
        .find(|&pos| ui::worktrees_hit(AREA, &view, pos) == Some(hit))
        .unwrap_or_else(|| panic!("no {hit:?} on screen"))
}

fn shown(app: &App, branch: &str) -> usize {
    view(app).rows.iter().position(|r| r.branch == branch).expect("the row is shown")
}

fn tick(s: &mut Setup, branch: &str) {
    let pos = at(&s.app, ui::WorktreesHit::Tick(shown(&s.app, branch)));
    click(&mut s.app, pos);
}

fn confirm(s: &mut Setup) {
    let pos = at(&s.app, ui::WorktreesHit::Remove);
    click(&mut s.app, pos);
    pump_until(&mut s.app, &s.rx, "the confirmation is checked", |a| {
        view(a).confirm.as_ref().is_some_and(|c| !c.checking)
    });
}

fn confirmed(s: &mut Setup, rows: usize, ticked: &[&str]) {
    opened(s, rows);
    for branch in ticked {
        tick(s, branch);
    }
    confirm(s);
}

fn ask_branches(s: &mut Setup) {
    let pos = at(&s.app, ui::WorktreesHit::Branches);
    click(&mut s.app, pos);
}

fn confirmation(app: &App) -> String {
    let view = view(app);
    let lines = &view.confirm.as_ref().expect("the confirmation is shown").lines;
    lines.iter().map(ToString::to_string).collect::<Vec<_>>().join("\n")
}

fn remove(s: &mut Setup) {
    let pos = at(&s.app, ui::WorktreesHit::Remove);
    click(&mut s.app, pos);
}

fn settle(s: &mut Setup, left: usize) -> Vec<String> {
    let seen = RefCell::new(Vec::new());
    pump_until(&mut s.app, &s.rx, "the removals settle", |a| {
        if let Some(text) = toast(a)
            && seen.borrow().last().is_none_or(|last: &String| last != text)
        {
            seen.borrow_mut().push(text.to_string());
        }
        a.projects[0].workspaces.len() == left && a.git.cleanup.pending.is_empty()
    });
    seen.into_inner()
}

fn alive(app: &App, path: &Path) -> bool {
    let ws = app.projects[0].workspaces.iter().find(|w| w.path == path).expect("the workspace is there");
    ws.open() && ws.terms().all(|t| t.shell_pid().is_some_and(process::alive))
}

#[test]
fn the_project_menu_opens_its_worktrees() {
    let mut s = with(&["a"]);
    let row = sidebar_pos(&s.app, SidebarRow::Project(0));
    right_click(&mut s.app, row);

    pick(&mut s.app, "worktrees");
    ready(&mut s, 1);

    let label = s.app.project_label(&s.app.projects[0]);
    assert_eq!((view(&s.app).title, dialog(&s.app).project.is_some()), (format!("Worktrees › {label}"), true));
}

#[test]
fn settings_opens_every_projects_worktrees() {
    let mut s = with(&["a"]);
    s.app.open_settings_on(&crate::settings::Row::Worktrees);

    send_key(&mut s.app, KeyCode::Enter, KeyModifiers::NONE);
    ready(&mut s, 1);

    assert_eq!((view(&s.app).title, dialog(&s.app).project.is_none()), ("Worktrees".to_string(), true));
}

#[test]
fn lists_a_closed_projects_worktree() {
    let mut s = with(&["a"]);
    let (other, _checkout) = closed(&s);
    let name = other.path().file_name().expect("a name").to_string_lossy().into_owned();

    opened(&mut s, 2);

    let row = view(&s.app).rows.into_iter().find(|r| r.branch == "side").expect("the closed project's worktree");
    assert_eq!((row.project, row.changes, row.tickable), (name, false, true));
}

#[test]
fn confirming_names_changes_and_a_lock_and_stops_nothing() {
    let mut s = with(&["a", "b"]);
    let (a, b) = (path_of(&s, "a"), path_of(&s, "b"));
    std::fs::write(a.join("new.txt"), "x").expect("write");
    git(s.repo.path(), &["worktree", "lock", "--reason", "busy", b.to_str().expect("utf-8")]);
    opened(&mut s, 2);
    tick(&mut s, "a");
    tick(&mut s, "b");

    confirm(&mut s);

    let text = confirmation(&s.app);
    assert!(text.contains("1 file changed") && text.contains("locked: busy"), "{text}");
    assert!(alive(&s.app, &a) && alive(&s.app, &b));
}

#[test]
fn a_file_written_after_the_list_is_named_in_the_confirmation() {
    let mut s = with(&["a"]);
    opened(&mut s, 1);
    std::fs::write(path_of(&s, "a").join("late.txt"), "x").expect("write");
    tick(&mut s, "a");

    confirm(&mut s);

    assert!(confirmation(&s.app).contains("1 file changed"), "{}", confirmation(&s.app));
}

#[test]
fn removing_two_runs_in_the_background_and_toasts_once() {
    let mut s = with(&["a", "b"]);
    confirmed(&mut s, 2, &["a", "b"]);

    remove(&mut s);
    let both = (s.app.removing(0, 1), s.app.removing(0, 2));

    let seen = settle(&mut s, 1);
    assert_eq!((both, seen), ((true, true), vec!["removed 2 worktrees".to_string()]));
}

#[test]
fn branches_stay_unless_asked() {
    let mut s = with(&["a"]);
    confirmed(&mut s, 1, &["a"]);

    remove(&mut s);

    settle(&mut s, 1);
    assert!(branch_exists(s.repo.path(), "a"));
}

#[test]
fn asked_branches_go_with_their_worktrees() {
    let mut s = with(&["a", "b"]);
    confirmed(&mut s, 2, &["a"]);
    ask_branches(&mut s);

    remove(&mut s);

    let seen = settle(&mut s, 2);
    let gone = (branch_exists(s.repo.path(), "a"), branch_exists(s.repo.path(), "b"));
    assert_eq!((gone, seen.last().cloned()), ((false, true), Some("removed 1 worktree and 1 branch".to_string())));
}

fn with_a_role() -> Setup {
    let mut s = with(&["lead", "coder"]);
    let coder = path_of(&s, "coder");
    let lead = &mut s.app.projects[0].workspaces[1];
    lead.role_worktrees.push(RoleWorktree { role: "coder".into(), path: coder, branch: "coder".into() });
    s
}

#[test]
fn a_role_worktree_of_a_live_run_cannot_be_ticked() {
    let mut s = with_a_role();
    let id = s.app.projects[0].workspaces[1].id;
    s.app.runs.list.push(Run::new(id, Flow::default(), "x".into()));
    opened(&mut s, 2);
    let pos = at(&s.app, ui::WorktreesHit::Row(shown(&s.app, "coder")));

    click(&mut s.app, pos);

    let row = view(&s.app).rows.into_iter().find(|r| r.branch == "coder").expect("the role's row");
    assert_eq!((row.ticked, row.tickable, row.status.as_deref()), (false, false, Some("owned by a flow run")));
}

#[test]
fn a_workspace_removed_from_the_sidebar_meanwhile_is_skipped() {
    let mut s = with(&["a", "b"]);
    confirmed(&mut s, 2, &["a", "b"]);
    let (project, workspace) = (s.app.projects[0].id, s.app.projects[0].workspaces[1].id);
    assert!(s.app.start_removal(project, workspace, RemoveOptions { force: true, unlock: false }, None));

    remove(&mut s);

    let seen = settle(&mut s, 1);
    assert!(seen.contains(&"removed 1 worktree".to_string()), "{seen:?}");
}

#[test]
fn changes_opens_the_panel_on_that_workspace() {
    let mut s = with(&["a", "b"]);
    s.app.projects[0].active = 0;
    opened(&mut s, 2);
    let pos = at(&s.app, ui::WorktreesHit::Changes(shown(&s.app, "a")));

    click(&mut s.app, pos);

    let active = s.app.project().and_then(Project::workspace).map(|w| w.path.clone());
    assert_eq!(
        (s.app.overlay.is_none(), active, s.app.screen.side),
        (true, Some(path_of(&s, "a")), Some(Side::Changes))
    );
}

#[test]
fn a_scan_that_fails_counts_as_changed() {
    let mut s = with(&["a"]);
    opened(&mut s, 1);
    std::fs::remove_file(path_of(&s, "a").join(".git")).expect("remove .git");
    tick(&mut s, "a");

    confirm(&mut s);

    assert!(confirmation(&s.app).contains("may have changes"), "{}", confirmation(&s.app));
}

#[test]
fn a_closed_projects_worktree_goes_with_its_branch_when_asked() {
    let mut s = with(&[]);
    let (other, checkout) = closed(&s);
    confirmed(&mut s, 1, &["side"]);
    ask_branches(&mut s);

    remove(&mut s);

    let seen = settle(&mut s, 1);
    let left = (checkout.exists(), branch_exists(other.path(), "side"));
    assert_eq!((left, seen), ((false, false), vec!["removed 1 worktree and 1 branch".to_string()]));
}

#[test]
fn a_removal_that_fails_is_named_in_the_toast() {
    let mut s = with(&[]);
    let (_other, checkout) = closed(&s);
    opened(&mut s, 1);
    std::fs::remove_file(checkout.join(".git")).expect("remove .git");
    tick(&mut s, "side");
    confirm(&mut s);

    remove(&mut s);

    let seen = settle(&mut s, 1);
    let text = seen.last().cloned().unwrap_or_default();
    let bug = s.app.messages.toast.as_ref().is_some_and(|t| t.icon == ui::ToastIcon::Bug);
    assert!(text.starts_with("removed 0 worktrees; could not remove side: "), "{text}");
    assert!(text.ends_with(", see server.log") && bug && checkout.exists(), "{text}");
}

#[test]
fn every_wrapped_line_of_the_confirmation_can_be_scrolled_to() {
    let red = Style::default().fg(ratatui::style::Color::Red);
    let lines = vec![
        Line::from("web-shop › feat/gift-cards"),
        Line::from("  2 commits not pushed, locked: claude session 日本語 (pid 4 start 9)"),
        Line::from("  role  coder:  12 files changed,  3 processes outside cornercase"),
        Line::from(Span::styled("  feat/gift-cards: its commits will be lost", red)),
    ];
    let text: String = lines.iter().map(ToString::to_string).collect();
    let (width, height) = (30, 4);

    let wrapped = wrap(lines, width);
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
    let most = u16::try_from(wrapped.len()).expect("rows") - height;
    terminal.draw(|f| f.render_widget(Paragraph::new(wrapped.clone()).scroll((most, 0)), f.area())).expect("draw");
    let bottom: String =
        (0..width).map(|x| terminal.backend().buffer()[(x, height - 1)].symbol().to_string()).collect();

    let cells = |line: &Line<'_>| unicode_width::UnicodeWidthStr::width(line.to_string().trim_end());
    assert!(wrapped.iter().all(|line| cells(line) <= usize::from(width)));
    assert_eq!(wrapped.iter().map(ToString::to_string).collect::<String>(), text);
    assert_eq!(bottom.trim_end(), "will be lost");
    assert_eq!(wrapped.last().map(|l| l.spans[0].style), Some(red));
}

#[test]
fn remove_counts_ticked_rows_the_filter_hides() {
    let mut s = with(&["a", "b"]);
    opened(&mut s, 2);
    tick(&mut s, "a");
    tick(&mut s, "b");

    type_text(&mut s.app, "b");

    assert_eq!((view(&s.app).rows.len(), view(&s.app).ticked), (1, 2));
}

fn commit_in(dir: &Path) {
    std::fs::write(dir.join("work.txt"), "work").expect("write");
    git(dir, &["add", "--all"]);
    git(dir, &["-c", "user.name=t", "-c", "user.email=t@e.com", "commit", "--quiet", "-m", "work"]);
}

fn settled_text(s: &mut Setup, left: usize) -> Option<String> {
    settle(s, left).last().cloned()
}

fn activate(s: &mut Setup, path: &Path) {
    let at = s.app.projects[0].workspaces.iter().position(|w| w.path == path).expect("the workspace");
    s.app.projects[0].active = at;
}

#[test]
fn a_branch_switched_after_the_confirmation_is_not_deleted() {
    let mut s = with(&["a"]);
    let a = path_of(&s, "a");
    confirmed(&mut s, 1, &["a"]);
    ask_branches(&mut s);
    git(&a, &["checkout", "--quiet", "-b", "other"]);

    remove(&mut s);

    let text = settled_text(&mut s, 1);
    let left = (branch_exists(s.repo.path(), "a"), branch_exists(s.repo.path(), "other"));
    let kept = "removed 1 worktree; kept the branch a: it changed since the confirmation";
    assert_eq!((left, text.as_deref()), ((true, true), Some(kept)));
}

#[test]
fn a_detached_worktree_deletes_no_branch() {
    let mut s = with(&["a"]);
    let a = path_of(&s, "a");
    git(&a, &["checkout", "--quiet", "--detach"]);
    let folder = a.file_name().expect("a name").to_string_lossy().into_owned();
    confirmed(&mut s, 1, &[&folder]);
    ask_branches(&mut s);
    assert!(confirmation(&s.app).contains("no branch is checked out, so none is deleted"), "{}", confirmation(&s.app));

    remove(&mut s);

    let text = settled_text(&mut s, 1);
    assert_eq!((branch_exists(s.repo.path(), "a"), text.as_deref()), (true, Some("removed 1 worktree")));
}

#[test]
fn a_prunable_rows_branch_with_local_commits_is_named() {
    let mut s = with(&[]);
    let lost = s.worktrees.path().join("lost");
    git(s.repo.path(), &["worktree", "add", "--quiet", "-b", "lost", lost.to_str().expect("utf-8")]);
    commit_in(&lost);
    std::fs::remove_dir_all(&lost).expect("remove the folder");
    confirmed(&mut s, 1, &["lost"]);

    ask_branches(&mut s);

    let text = confirmation(&s.app);
    assert!(text.contains("1 commit not pushed") && text.contains("lost: its commits will be lost"), "{text}");
}

#[test]
fn an_orphan_only_confirmation_still_checks_outside_processes() {
    let mut s = with(&[]);
    let (other, checkout) = closed(&s);
    std::fs::remove_dir_all(other.path()).expect("remove the repository");
    opened(&mut s, 1);
    pump_until(&mut s.app, &s.rx, "the first look ends", |a| !a.worktree_outside_running());
    let sleeper =
        crate::test_util::Running::spawn(std::process::Command::new("/bin/sleep").arg("30").current_dir(&checkout));
    wait_until("sleep works in the folder", || process::cwd(sleeper.pid()).as_deref() == Some(checkout.as_path()));
    tick(&mut s, &checkout.file_name().expect("a name").to_string_lossy());

    let pos = at(&s.app, ui::WorktreesHit::Remove);
    click(&mut s.app, pos);
    let waited = view(&s.app).confirm.as_ref().is_some_and(|c| c.checking);
    pump_until(&mut s.app, &s.rx, "the look ends", |a| view(a).confirm.as_ref().is_some_and(|c| !c.checking));

    let text = confirmation(&s.app);
    assert!(waited && text.contains("1 process outside cornercase: sleep"), "{waited} {text}");
}

#[test]
fn a_shell_of_another_workspace_inside_the_worktree_is_named() {
    let mut s = with(&["a", "b"]);
    let (a, b) = (path_of(&s, "a"), path_of(&s, "b"));
    activate(&mut s, &b);
    type_line(&mut s.app, &format!("cd {}", a.display()));
    wait_until("the shell of b is in a", || s.app.term().and_then(Term::cwd).as_deref() == Some(a.as_path()));

    confirmed(&mut s, 2, &["a"]);

    let text = confirmation(&s.app);
    assert!(text.contains(" in another cornercase tab") && !text.contains("outside cornercase"), "{text}");
}

#[test]
fn ticking_a_run_and_its_role_removes_the_role_once() {
    let mut s = with_a_role();
    confirmed(&mut s, 2, &["lead", "coder"]);
    ask_branches(&mut s);

    remove(&mut s);

    let text = settled_text(&mut s, 2);
    let left = (branch_exists(s.repo.path(), "lead"), branch_exists(s.repo.path(), "coder"));
    assert_eq!((left, text.as_deref()), ((false, false), Some("removed 2 worktrees and 2 branches")));
}

#[test]
fn rescans_never_run_more_than_four_scans_at_once() {
    let mut s = with(&["a", "b", "c", "d"]);
    s.app.open_worktrees(None);
    loop {
        let ev = s.rx.recv_timeout(Duration::from_secs(30)).expect("an answer");
        let listed = matches!(ev, AppEvent::Worktree(WorktreeEvent::Listed { .. }));
        s.app.handle_event(ev, AREA).expect("handle event");
        if listed {
            break;
        }
    }
    let started = s.app.worktree_scans();
    tick(&mut s, "a");

    let pos = at(&s.app, ui::WorktreesHit::Remove);
    click(&mut s.app, pos);

    assert_eq!((started, s.app.worktree_scans()), ((4, 0), (4, 0)));
    pump_until(&mut s.app, &s.rx, "the rescan answers", |a| view(a).confirm.as_ref().is_some_and(|c| !c.checking));
}

#[test]
fn the_list_marks_running_programs_and_processes_elsewhere() {
    let mut s = with(&["a", "b"]);
    let (a, b) = (path_of(&s, "a"), path_of(&s, "b"));
    activate(&mut s, &b);
    running_sleep(&mut s.app);
    let sleeper = crate::test_util::Running::spawn(std::process::Command::new("/bin/sleep").arg("30").current_dir(&a));
    wait_until("sleep works in a", || process::cwd(sleeper.pid()).as_deref() == Some(a.as_path()));

    opened(&mut s, 2);
    pump_until(&mut s.app, &s.rx, "the look ends", |a| !a.worktree_outside_running());

    let marks = |branch: &str| -> Vec<String> {
        let rows = view(&s.app).rows;
        rows.into_iter().find(|r| r.branch == branch).expect("the row").marks.into_iter().map(|m| m.0).collect()
    };
    let (of_a, of_b) = (marks("a"), marks("b"));
    assert!(of_a.contains(&"1 outside".to_string()) && of_b.contains(&"sleep".to_string()), "{of_a:?} {of_b:?}");
}
