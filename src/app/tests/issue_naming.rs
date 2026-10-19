use super::issue_list::{self, browser, checkout, enter, listing, open_loaded, start};
use super::*;
use crate::issues::browser::Naming;

const TALL: Rect = Rect { x: 0, y: 0, width: 120, height: 32 };

fn to_naming(s: &mut issue_list::Setup) {
    open_loaded(s, Source::Github);
    enter(s);
    enter(s);
}

fn naming(app: &App) -> &Naming {
    browser(app).naming.as_ref().expect("the names are asked")
}

fn erase(app: &mut App, count: usize) {
    for _ in 0..count {
        send_key(app, KeyCode::Backspace, KeyModifiers::NONE);
    }
}

#[test]
fn start_asks_for_the_branch_and_the_workspace_s_name_prefilled() {
    let mut s = listing();
    to_naming(&mut s);

    let asked = naming(&s.app);
    assert_eq!((asked.branch.as_str(), asked.name.as_str()), ("issue-7-fix-the-login", "#7 Fix the login"));
    let shown = format!("{:?}", render(&mut s.app, TALL).backend());
    assert!(shown.contains("› branch    issue-7-fix-the-login"), "{shown}");
    assert!(shown.contains("› workspace #7 Fix the login"), "{shown}");
}

#[test]
fn enter_starts_with_what_was_filled_in() {
    let mut s = listing();
    start(&mut s);

    assert!(checkout(&s).is_dir());
    assert_eq!(s.app.projects[0].workspaces[1].name.as_deref(), Some("#7 Fix the login"));
}

#[test]
fn the_template_names_the_branch() {
    let mut s = listing();
    s.app.config.issue_branch = "feat/{key}-{slug}".into();
    to_naming(&mut s);

    assert_eq!(naming(&s.app).branch, "feat/7-fix-the-login");
}

#[test]
fn an_edited_branch_and_name_are_the_ones_used() {
    let mut s = listing();
    to_naming(&mut s);
    erase(&mut s.app, "issue-7-fix-the-login".len());
    type_text(&mut s.app, "ana/7");
    send_key(&mut s.app, KeyCode::Tab, KeyModifiers::NONE);
    erase(&mut s.app, "#7 Fix the login".len());
    type_text(&mut s.app, "login");
    enter(&mut s);
    pump_until(&mut s.app, &s.rx, "the issue workspace opens", |a| a.projects[0].workspaces.len() == 2);

    let repo = s.app.projects[0].path.file_name().expect("repo name").to_owned();
    let workspace = &s.app.projects[0].workspaces[1];
    assert_eq!(git::branch(&workspace.path).as_deref(), Some("ana/7"));
    assert_eq!(workspace.path, s.worktrees.path().join(repo).join(git::worktree::slug("ana/7")));
    assert_eq!(workspace.name.as_deref(), Some("login"));
}
