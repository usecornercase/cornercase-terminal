use super::build_folders::with_builds;
use super::*;
use crate::app::tests::worktrees_dialog::{Setup, closed, dialog, path_of, ready, with};
use crate::test_util::git;

fn listed(s: &mut Setup, args: &Value) -> Vec<Value> {
    let answer = asked(&mut s.app, &s.rx, "worktrees", args).expect("an answer");
    answer["worktrees"].as_array().expect("a list").clone()
}

fn named<'a>(rows: &'a [Value], branch: &str) -> &'a Value {
    rows.iter().find(|row| row["branch"] == branch).unwrap_or_else(|| panic!("no row for {branch}: {rows:?}"))
}

#[test]
fn every_worktree_is_listed_with_what_makes_it_safe_to_remove() {
    let mut s = with(&["dirty", "clean"]);
    let dirty = path_of(&s, "dirty");
    std::fs::write(dirty.join("notes.txt"), "draft").expect("write");
    let workspace = s.app.projects[0].workspaces.iter().find(|w| w.path == dirty).expect("it").id;

    let rows = listed(&mut s, &json!({}));

    let dirty = named(&rows, "dirty");
    assert_eq!(
        (dirty["changed"].clone(), dirty["safe"].clone(), dirty["workspace"].clone()),
        (json!(1), json!(false), json!(workspace))
    );
    assert!(dirty["risks"].to_string().contains("1 file changed"), "{dirty}");
    let clean = named(&rows, "clean");
    assert_eq!(
        (clean["no_commits"].clone(), clean["safe"].clone(), clean["risks"].clone()),
        (json!(true), json!(true), json!([]))
    );
}

#[test]
fn a_closed_repositorys_worktree_and_a_leftover_folder_are_listed_and_the_leftover_first() {
    let mut s = with(&["a"]);
    let (_other, _checkout) = closed(&s);
    let worktrees = s.app.config.worktrees_dir(s.app.home.as_deref());
    let leftover = worktrees.join("gone").join("x");
    std::fs::create_dir_all(&leftover).expect("mkdir");
    std::fs::write(leftover.join(".git"), "gitdir: /nonexistent/repo/.git/worktrees/x\n").expect("dot git");

    let rows = listed(&mut s, &json!({"sizes": false}));

    let side = named(&rows, "side");
    assert_eq!((side["project"].clone(), side["kind"].clone()), (Value::Null, json!("linked")));
    let first = &rows[0];
    assert_eq!(first["kind"], "repository-gone");
    assert_eq!((first["repository"].clone(), first["safe"].clone()), (Value::Null, json!(false)));
}

#[test]
fn sizes_come_with_their_build_folders_unless_left_out() {
    let mut s = with(&["big"]);
    with_builds(&path_of(&s, "big"));

    let rows = listed(&mut s, &json!({}));
    let big = named(&rows, "big");
    assert_eq!(big["size"]["builds"][0]["path"], "node_modules");
    assert!(big["size"]["builds_bytes"].as_u64() >= Some(300_000));

    let rows = listed(&mut s, &json!({"sizes": false}));
    assert_eq!(named(&rows, "big")["size"], Value::Null);
}

#[test]
fn a_project_narrows_the_list_and_a_project_that_is_no_repository_is_refused() {
    let mut s = with(&["a"]);
    let (_other, _checkout) = closed(&s);
    let project = s.app.projects[0].id;

    let rows = listed(&mut s, &json!({"project": project, "sizes": false}));
    assert!(rows.iter().all(|row| row["branch"] != "side") && rows.iter().any(|row| row["branch"] == "a"), "{rows:?}");

    assert_eq!(refused(&mut s.app, "worktrees", &json!({"project": 9999})).code, "not-found");
    let plain = TempDir::new();
    s.app.open_project(plain.path().to_path_buf(), AREA).expect("open project");
    let opened = s.app.projects.iter().find(|p| p.path != s.app.projects[0].path).expect("the plain folder").id;
    assert_eq!(refused(&mut s.app, "worktrees", &json!({"project": opened})).code, "conflict");
}

#[test]
fn a_prunable_entry_has_its_branch_commits_counted() {
    let mut s = with(&[]);
    let lost = TempDir::new().path().join("lost");
    git(s.repo.path(), &["worktree", "add", "--quiet", "-b", "lost", lost.to_str().expect("utf-8")]);
    git(&lost, &["-c", "user.name=t", "-c", "user.email=t@e.c", "commit", "--allow-empty", "-m", "work"]);
    std::fs::remove_dir_all(&lost).expect("remove");

    let rows = listed(&mut s, &json!({"sizes": false}));

    let lost = named(&rows, "lost");
    assert_eq!(
        (lost["kind"].clone(), lost["unpushed"].clone(), lost["size"].clone()),
        (json!("prunable"), json!(1), Value::Null)
    );
}

#[test]
fn listing_leaves_the_window_and_an_open_dialog_alone() {
    let mut s = with(&["a"]);
    listed(&mut s, &json!({"sizes": false}));
    assert!(s.app.overlay.is_none());

    s.app.open_worktrees(None);
    ready(&mut s, 1);
    let before = (dialog(&s.app).generation, dialog(&s.app).list.rows.len());

    let rows = listed(&mut s, &json!({}));

    assert_eq!(rows.len(), 1);
    assert_eq!(
        (dialog(&s.app).generation, dialog(&s.app).list.rows.len(), dialog(&s.app).list.pending()),
        (before.0, before.1, 0)
    );
}
