#![cfg(unix)]

use super::super::worktrees::removals_wait_in_teardown;
use super::*;
use crate::app::tests::worktrees_dialog::{
    Setup, activate, alive, closed, commit_in, confirmed, dialog, opened, path_of, ready, remove, settled_text,
    shell_of_b_in_a, tick, view, with, with_a_role,
};
use crate::app::worktrees::RemoveOptions;
use crate::flows::Flow;
use crate::flows::run::{Member, RoleWorktree, Run};

impl Asks for Setup {
    fn parts(&mut self) -> (&mut App, &Receiver<AppEvent>) {
        (&mut self.app, &self.rx)
    }
}

fn with_a(names: &[&str]) -> (Setup, PathBuf) {
    let s = with(names);
    let a = path_of(&s, names[0]);
    (s, a)
}

fn workspace(s: &Setup, path: &Path) -> Option<u64> {
    s.app.projects[0].workspaces.iter().find(|w| w.path == path).map(|w| w.id)
}

fn pump_until_gone(s: &mut Setup, path: &Path) {
    let at = path.to_path_buf();
    pump_until(&mut s.app, &s.rx, "the row goes", |a| a.projects[0].workspaces.iter().all(|w| w.path != at));
}

fn left_running(s: &Setup, path: &Path) -> bool {
    path.exists() && alive(&s.app, path)
}

fn waiting_in_its_teardown(s: &mut Setup, path: &Path, args: &Value) -> (TempDir, u64) {
    let hold = TempDir::new();
    let repo = s.repo.path().to_path_buf();
    removals_wait_in_teardown(&mut s.app, &repo, hold.path());
    let id = workspace(s, path).expect("the workspace");
    request_json(&mut s.app, COMMAND, args);
    pump_until(&mut s.app, &s.rx, "the removal starts", |app| {
        app.projects.workspace_by(id).is_some_and(Workspace::removing)
    });
    (hold, id)
}

fn let_it_end(hold: &TempDir) {
    std::fs::write(hold.path().join("go"), "").expect("let the teardown end");
}

mod an_open_workspace {
    use super::*;

    #[test]
    fn stops_its_shells_goes_and_keeps_its_branch() {
        let (mut s, a) = with_a(&["a"]);

        let folder = s.removes(&json!({"paths": [a]}));

        assert!(folder["worktree"]["workspace"].is_u64(), "{folder}");
        assert!(!a.exists() && branch_exists(s.repo.path(), "a"));
        pump_until_gone(&mut s, &a);
        assert_eq!(toast(&s.app), None);
    }

    #[test]
    fn its_branch_goes_too_when_asked() {
        let (mut s, a) = with_a(&["a"]);

        let folder = s.removes(&json!({"paths": [a], "delete_branch": true}));

        assert_eq!(folder["branches"], json!(["a"]));
        assert!(!branch_exists(s.repo.path(), "a"));
    }

    #[test]
    fn with_uncommitted_files_stops_nothing_without_force() {
        let (mut s, a) = with_a(&["a"]);
        write_notes(&a);

        s.refuses(&json!({"paths": [a]}), &["force"]);

        assert!(left_running(&s, &a));
        s.removes(&json!({"paths": [a], "force": true}));
        assert!(!a.exists());
    }

    #[test]
    fn locked_stops_nothing_without_unlock() {
        let (mut s, a) = with_a(&["a"]);
        git(s.repo.path(), &["worktree", "lock", "--reason", "busy", &a.display().to_string()]);

        s.refuses(&json!({"paths": [a], "force": true}), &["unlock"]);

        assert!(left_running(&s, &a));
        s.removes(&json!({"paths": [a], "unlock": true}));
    }

    #[test]
    fn with_a_program_running_stops_nothing_without_force() {
        let (mut s, a) = with_a(&["a"]);
        activate(&mut s, &a);
        running_sleep(&mut s.app);

        let said = s.refuses(&json!({"paths": [a]}), &["force"]);

        assert!(said.contains("sleep running in it; they stop"), "{said}");
        assert_eq!(s.app.term().and_then(|t| t.program(&s.app.config)).as_deref(), Some("sleep"));
        s.removes(&json!({"paths": [a], "force": true}));
    }

    #[test]
    fn a_shell_of_another_workspace_inside_it_waits_for_force() {
        let (mut s, a) = with_a(&["a", "b"]);
        shell_of_b_in_a(&mut s);

        let said = s.refuses(&json!({"paths": [a]}), &["force"]);

        assert!(said.contains("in another cornercase tab"), "{said}");
        assert!(left_running(&s, &a));
    }

    #[test]
    fn answers_only_once_the_removal_has_ended() {
        let (mut s, a) = with_a(&["a"]);

        let (hold, _) = waiting_in_its_teardown(&mut s, &a, &json!({"paths": [a]}));

        assert_eq!(s.app.take_answers(), Vec::new(), "it answered while the teardown ran");
        let_it_end(&hold);
        let answer = reply_later(&mut s.app, &s.rx).expect("an answer");
        assert_eq!(outcomes(&answer), ["removed"]);
    }

    #[test]
    fn a_client_that_leaves_while_it_goes_leaves_it_going_and_its_branch_goes_too() {
        let (mut s, a) = with_a(&["a"]);
        let (hold, _) = waiting_in_its_teardown(&mut s, &a, &json!({"paths": [a], "delete_branch": true}));

        s.app.forget(CLIENT);
        let_it_end(&hold);

        let (repo, gone) = (s.repo.path().to_path_buf(), a.clone());
        pump_until(&mut s.app, &s.rx, "the folder and the branch go", move |_| {
            !gone.exists() && !branch_exists(&repo, "a")
        });
        assert_eq!(s.app.take_answers(), Vec::new());
    }

    #[test]
    fn a_commit_during_its_teardown_keeps_the_branch_and_its_commit() {
        let (mut s, a) = with_a(&["a"]);
        let (hold, _) = waiting_in_its_teardown(&mut s, &a, &json!({"paths": [a], "delete_branch": true}));
        git(&a, &["commit", "--quiet", "--allow-empty", "-m", "during the teardown"]);

        let_it_end(&hold);
        let answer = reply_later(&mut s.app, &s.rx).expect("an answer");

        assert_eq!(answer["folders"][0]["kept"], json!(["kept the branch a: it changed since the confirmation"]));
        assert!(!a.exists() && branch_exists(s.repo.path(), "a"));
    }

    #[test]
    fn a_link_put_in_its_place_during_its_teardown_comes_back_and_its_target_stays() {
        let (mut s, a) = with_a(&["a"]);
        let script = "mv \"$CORNERCASE_WORKTREE\" \"$CORNERCASE_WORKTREE.kept\" && ln -s \"$CORNERCASE_WORKTREE.kept\" \"$CORNERCASE_WORKTREE\"";
        let key = s.repo.path().display().to_string();
        s.app.config.projects.insert(key, ProjectSettings { teardown: script.into(), ..Default::default() });

        let folder = s.one(&json!({"paths": [a]}));

        assert_eq!(outcome(&folder), "failed");
        assert!(why(&folder).contains("moved during its teardown"), "{folder}");
        assert!(a.with_extension("kept").join("README").exists());
    }

    #[test]
    fn whose_folder_git_no_longer_knows_stops_nothing_and_says_how() {
        let (mut s, a) = with_a(&["a"]);
        std::fs::remove_dir_all(s.repo.path().join(".git/worktrees").join("a")).expect("forget it in git");

        let refused = s.one(&json!({"paths": [a], "force": true, "unlock": true}));

        assert_eq!(outcome(&refused), "refused");
        assert!(why(&refused).contains("its workspace is open"), "{refused}");
        assert!(left_running(&s, &a));
    }

    #[test]
    fn being_removed_is_refused() {
        let (mut s, a) = with_a(&["a"]);
        let hold = TempDir::new();
        let repo = s.repo.path().to_path_buf();
        removals_wait_in_teardown(&mut s.app, &repo, hold.path());
        let (project, id) = (s.app.projects[0].id, workspace(&s, &a).expect("the workspace"));
        assert!(s.app.start_removal(project, id, RemoveOptions::default(), None));

        let refused = s.one(&json!({"paths": [a], "force": true, "unlock": true}));

        let_it_end(&hold);
        assert_eq!((outcome(&refused), why(&refused)), ("refused", "it is already being removed or closed"));
    }

    #[test]
    fn whose_project_is_closing_is_refused() {
        let (mut s, a) = with_a(&["a"]);
        s.app.projects[0].closing = true;

        let refused = s.one(&json!({"paths": [a], "force": true}));

        s.app.projects[0].closing = false;
        assert_eq!((outcome(&refused), why(&refused)), ("refused", "it is already being removed or closed"));
        assert!(left_running(&s, &a));
    }

    #[test]
    fn whose_folder_git_refuses_after_the_look_comes_back_and_says_why() {
        let (mut s, a) = with_a(&["a"]);
        let (hold, id) = waiting_in_its_teardown(&mut s, &a, &json!({"paths": [a]}));
        write_notes(&a);

        let_it_end(&hold);
        let answer = reply_later(&mut s.app, &s.rx).expect("an answer");

        assert_eq!(outcomes(&answer), ["failed"]);
        assert!(why(&answer["folders"][0]).contains("use --force"), "{answer}");
        assert!(a.join("notes.txt").exists());
        assert!(s.app.projects.workspace_by(id).is_some_and(Workspace::open));
    }
}

mod a_merged_pull_request {
    use super::*;
    use crate::app::tests::pull_requests::{Repo, repo_with};
    use crate::test_util::head;

    impl Asks for Repo {
        fn parts(&mut self) -> (&mut App, &Receiver<AppEvent>) {
            (&mut self.app, &self.rx)
        }
    }

    fn with_a_commit_gh_says_was_merged(head_field: impl FnOnce(&str) -> String) -> (Repo, PathBuf) {
        let mut repo = repo_with("cat \"$0.json\"", |_| {});
        let at = repo.app.projects[0].workspaces[1].path.clone();
        commit_in(&at);
        let json = format!(r#"{{"number":482,"title":"x","url":"u","state":"MERGED"{}}}"#, head_field(&head(&at)));
        std::fs::write(repo.tmp.path().join("gh.json"), json).expect("the answer");
        repo.ask("gh answers");
        (repo, at)
    }

    fn at_its_head(head: &str) -> String {
        format!(r#","headRefOid":"{head}""#)
    }

    #[test]
    fn a_worktree_at_the_commit_it_ended_at_loses_nothing() {
        let (mut repo, at) = with_a_commit_gh_says_was_merged(at_its_head);

        let look = repo.one(&json!({"paths": [at], "delete_branch": true, "check": true}));
        assert_eq!(
            (&look["lost"], &look["needs"], &look["worktree"]["merged"]),
            (&json!([]), &json!([]), &json!(true))
        );

        let folder = repo.removes(&json!({"paths": [at], "delete_branch": true}));

        assert_eq!(folder["branches"], json!(["feat/x"]));
    }

    #[test]
    fn a_commit_after_it_waits_for_force_and_keeps_the_branch() {
        let (mut repo, at) = with_a_commit_gh_says_was_merged(at_its_head);
        git(&at, &["commit", "--quiet", "--allow-empty", "-m", "after the merge"]);

        let said = repo.refuses(&json!({"paths": [at], "delete_branch": true}), &["force"]);

        assert!(said.contains("feat/x: its commits will be lost"), "{said}");
        assert!(at.exists() && left_on_branch(&repo, "feat/x"));
    }

    #[test]
    fn gh_not_saying_where_it_ended_counts_as_a_commit_after_it() {
        let (mut repo, at) = with_a_commit_gh_says_was_merged(|_| String::new());

        let said = repo.refuses(&json!({"paths": [at], "delete_branch": true}), &["force"]);

        assert!(said.contains("feat/x: its commits will be lost"), "{said}");
    }

    fn left_on_branch(repo: &Repo, branch: &str) -> bool {
        branch_exists(&repo.app.projects[0].path, branch)
    }
}

mod a_flow_run {
    use super::*;

    fn lead_and_coder() -> (Setup, PathBuf, PathBuf) {
        let s = with_a_role();
        let (lead, coder) = (path_of(&s, "lead"), path_of(&s, "coder"));
        (s, lead, coder)
    }

    fn run_lives(s: &mut Setup) {
        let lead = &s.app.projects[0].workspaces[1];
        let mut run = Run::new(lead.id, Flow::default(), "x".into());
        run.members.push(Member { role: "lead".into(), pane: lead.terms().next().expect("a pane").id });
        s.app.runs.push(run);
    }

    #[test]
    fn a_roles_worktree_is_refused_while_the_run_lives() {
        let (mut s, _, coder) = lead_and_coder();
        run_lives(&mut s);

        let refused = s.one(&json!({"paths": [coder], "force": true, "unlock": true}));

        assert_eq!(outcome(&refused), "refused");
        assert!(why(&refused).contains("belongs to `coder`"), "{refused}");
        assert!(left_running(&s, &coder));
    }

    #[test]
    fn a_roles_worktree_goes_with_its_runs_workspace_when_both_are_named() {
        let (mut s, lead, coder) = lead_and_coder();
        run_lives(&mut s);

        let answer = s.asked(&json!({"paths": [coder, lead], "delete_branch": true}));

        assert_eq!(outcomes(&answer), ["removed", "removed"]);
        assert_eq!(answer["folders"][0]["removed_with"], json!(lead));
        assert_eq!(answer["folders"][1]["branches"].as_array().map(Vec::len), Some(2));
        assert!(!lead.exists() && !coder.exists());
        assert!(!branch_exists(s.repo.path(), "lead") && !branch_exists(s.repo.path(), "coder"));
    }

    fn the_teardown_is(s: &mut Setup, script: &str) {
        let key = s.repo.path().display().to_string();
        s.app.config.projects.insert(key, ProjectSettings { teardown: script.into(), ..Default::default() });
    }

    fn the_teardown_writes_in(s: &mut Setup, folder: &Path) {
        the_teardown_is(s, &format!("echo late > '{}/late.txt'", folder.display()));
    }

    #[test]
    fn a_workspace_that_fails_after_its_role_went_names_the_role() {
        let (mut s, lead, coder) = lead_and_coder();
        run_lives(&mut s);
        the_teardown_writes_in(&mut s, &lead);

        let folder = s.one(&json!({"paths": [lead], "delete_branch": true}));

        assert_eq!(outcome(&folder), "failed", "{folder}");
        assert!(lead.exists() && !coder.exists() && branch_exists(s.repo.path(), "coder"));
        let warning = folder["warnings"][0].as_str().expect("a warning");
        assert!(warning.contains("the worktree of `coder` went before this failed"), "{folder}");
        assert!(warning.contains(&coder.display().to_string()) && warning.contains("branch coder stays"), "{folder}");
    }

    #[test]
    fn a_workspace_that_fails_with_its_role_still_there_warns_of_nothing() {
        let (mut s, lead, coder) = lead_and_coder();
        run_lives(&mut s);
        the_teardown_is(&mut s, "echo late > late.txt");

        let folder = s.one(&json!({"paths": [lead]}));

        assert_eq!((outcome(&folder), &folder["warnings"]), ("failed", &json!([])), "{folder}");
        assert!(lead.exists() && coder.exists());
    }

    #[test]
    fn a_named_role_says_what_happened_when_its_workspace_fails() {
        let (mut s, lead, coder) = lead_and_coder();
        run_lives(&mut s);
        the_teardown_is(&mut s, "echo late > late.txt");

        let answer = s.asked(&json!({"paths": [coder, lead]}));

        assert_eq!(outcomes(&answer), ["failed", "failed"], "{answer}");
        let said = why(&answer["folders"][0]);
        assert!(said.contains("still there after") && said.ends_with(" failed"), "{answer}");
    }

    #[test]
    fn a_named_role_that_went_answers_removed_when_its_workspace_fails() {
        let (mut s, lead, coder) = lead_and_coder();
        run_lives(&mut s);
        the_teardown_writes_in(&mut s, &lead);

        let answer = s.asked(&json!({"paths": [coder, lead]}));

        assert_eq!(outcomes(&answer), ["removed", "failed"], "{answer}");
        assert!(!coder.exists() && lead.exists());
    }

    #[test]
    fn a_runs_workspace_takes_its_roles_with_it_and_names_them() {
        let (mut s, lead, coder) = lead_and_coder();
        run_lives(&mut s);

        let look = s.one(&json!({"paths": [lead], "check": true}));
        assert_eq!(look["roles"][0]["role"], "coder");
        let lines = look["lines"].to_string();
        assert!(lines.contains("ends its flow run and removes the worktrees of its 1 role"), "{look}");
        assert!(lines.contains("role coder on coder: nothing uncommitted or unpushed"), "{look}");
        assert!(left_running(&s, &coder));

        s.removes(&json!({"paths": [lead]}));

        assert!(!lead.exists() && !coder.exists() && branch_exists(s.repo.path(), "coder"));
    }

    #[test]
    fn a_roles_changes_and_lock_stop_its_runs_workspace_until_both_flags_are_given() {
        let (mut s, lead, coder) = lead_and_coder();
        write_notes(&coder);
        git(s.repo.path(), &["worktree", "lock", "--reason", "busy", &coder.display().to_string()]);

        let said = s.refuses(&json!({"paths": [lead]}), &["force", "unlock"]);

        assert!(said.contains("role coder: 1 file changed"), "{said}");
        assert!(said.contains("the worktree of `coder` is locked (busy)"), "{said}");
        assert!(left_running(&s, &lead) && left_running(&s, &coder));
        s.removes(&json!({"paths": [lead], "force": true, "unlock": true}));
        assert!(!lead.exists() && !coder.exists());
    }

    #[test]
    fn a_roles_commits_stop_its_runs_workspace_when_the_branches_would_go() {
        let (mut s, lead, coder) = lead_and_coder();
        commit_in(&coder);

        let said = s.refuses(&json!({"paths": [lead], "delete_branch": true}), &["force"]);

        assert!(said.contains("coder: its commits will be lost"), "{said}");
        assert!(left_running(&s, &lead));
    }

    #[test]
    fn a_roles_build_folders_being_freed_stop_its_runs_workspace() {
        let (mut s, lead, coder) = lead_and_coder();
        s.app.git.frees.freeing(coder.clone());

        let refused = s.one(&json!({"paths": [lead], "force": true, "unlock": true}));

        assert_eq!((outcome(&refused), why(&refused)), ("refused", "the build folders of `coder` are being freed"));
        assert!(left_running(&s, &lead) && left_running(&s, &coder));
    }

    #[test]
    fn a_role_that_could_not_be_checked_is_named_as_unchecked() {
        let (mut s, lead, coder) = lead_and_coder();
        git(&coder, &["checkout", "--quiet", "--orphan", "unborn"]);

        let look = s.one(&json!({"paths": [lead], "check": true}));

        assert!(look["unchecked"].to_string().contains("role coder:"), "{look}");
        assert_eq!(look["needs"], json!(["force"]));
    }

    #[test]
    fn a_role_of_a_role_named_with_them_still_gets_its_answer() {
        let (mut s, a, b, c) = three_in_a_chain(false);

        let answer = s.asked(&json!({"paths": [a, b, c]}));

        assert_eq!(outcomes(&answer), ["removed", "removed", "removed"]);
        assert_eq!(answer["folders"][1]["removed_with"], json!(a));
        assert_eq!(answer["folders"][2]["removed_with"], Value::Null);
        assert!(!a.exists() && !b.exists() && !c.exists());
    }

    #[test]
    fn workspaces_that_name_each_other_as_roles_still_get_their_answers() {
        let (mut s, a, b, _c) = three_in_a_chain(true);

        let answer = s.asked(&json!({"paths": [a, b], "force": true}));

        assert_eq!(answer["folders"].as_array().map(Vec::len), Some(2), "{answer}");
    }

    fn three_in_a_chain(closed_in_a_circle: bool) -> (Setup, PathBuf, PathBuf, PathBuf) {
        let mut s = with(&["a", "b", "c"]);
        let (a, b, c) = (path_of(&s, "a"), path_of(&s, "b"), path_of(&s, "c"));
        let role =
            |name: &str, path: &Path| RoleWorktree { role: name.into(), path: path.to_path_buf(), branch: name.into() };
        s.app.projects[0].workspaces[1].role_worktrees.push(role("b", &b));
        s.app.projects[0].workspaces[2].role_worktrees.push(if closed_in_a_circle {
            role("a", &a)
        } else {
            role("c", &c)
        });
        (s, a, b, c)
    }

    #[test]
    fn a_role_named_alone_goes_once_its_run_has_ended() {
        let (mut s, _, coder) = lead_and_coder();

        s.removes(&json!({"paths": [coder]}));

        assert!(!coder.exists());
    }

    #[test]
    fn a_role_stays_refused_when_its_runs_workspace_is_refused() {
        let (mut s, lead, coder) = lead_and_coder();
        run_lives(&mut s);
        write_notes(&lead);

        let answer = s.asked(&json!({"paths": [coder, lead]}));

        assert_eq!(outcomes(&answer), ["refused", "refused"]);
        assert!(why(&answer["folders"][0]).contains("belongs to `coder`"), "{answer}");
        assert!(left_running(&s, &lead) && left_running(&s, &coder));
    }
}

mod projects {
    use super::*;

    #[test]
    fn an_open_projects_main_checkout_is_refused_and_keeps_running() {
        let mut s = with(&["a"]);
        let main = s.repo.path().to_path_buf();

        let refused = s.one(&json!({"paths": [main], "force": true, "unlock": true}));

        assert_eq!(outcome(&refused), "refused");
        assert!(why(&refused).contains("main checkout"), "{refused}");
        assert!(left_running(&s, &main));
    }

    #[test]
    fn a_projects_own_folder_is_refused_even_when_it_is_a_linked_worktree() {
        let mut s = with(&["a"]);
        let (other, checkout) = closed(&s);
        s.app.open_project(checkout.clone(), AREA).expect("open the checkout as a project");

        let refused = s.one(&json!({"paths": [checkout], "force": true, "unlock": true}));

        assert_eq!((outcome(&refused), why(&refused)), ("refused", "it is a project's own folder"));
        assert!(checkout.join("README").exists() && other.path().exists());
    }
}

mod the_window {
    use super::*;

    fn held_in_the_teardown_of_a_closed_project(s: &mut Setup) -> (TempDir, TempDir, PathBuf) {
        let (other, checkout) = closed(s);
        let hold = TempDir::new();
        removals_wait_in_teardown(&mut s.app, other.path(), hold.path());
        (other, hold, checkout)
    }

    #[test]
    fn an_open_dialog_sees_the_row_go_and_stays_where_it_was() {
        let (mut s, a) = with_a(&["a", "b"]);
        opened(&mut s, 2);
        tick(&mut s, "a");
        tick(&mut s, "b");

        s.removes(&json!({"paths": [a]}));

        let shown: Vec<String> = view(&s.app).rows.into_iter().map(|row| row.branch).collect();
        assert_eq!(shown, ["b"]);
        assert_eq!(view(&s.app).ticked, 1);
        assert!(matches!(dialog(&s.app).step, crate::worktrees::Step::List));
        assert_eq!(toast(&s.app), None);
    }

    #[test]
    fn an_open_dialog_cannot_tick_a_closed_projects_row_while_a_request_removes_it() {
        let mut s = with(&[]);
        let (_other, hold, checkout) = held_in_the_teardown_of_a_closed_project(&mut s);
        s.app.open_worktrees(None);
        ready(&mut s, 1);

        request_json(&mut s.app, COMMAND, &json!({"paths": [checkout]}));
        pump_until(&mut s.app, &s.rx, "the row is removing", |a| dialog(a).list.rows[0].removing);

        assert!(!view(&s.app).rows[0].tickable);
        assert_eq!(view(&s.app).rows[0].status.as_deref(), Some("removing…"));
        let_it_end(&hold);
        pump_until(&mut s.app, &s.rx, "the row goes", |a| dialog(a).list.rows[0].gone);
    }

    #[test]
    fn the_dialogs_own_toast_comes_at_its_own_end_while_a_request_still_removes() {
        let mut s = with(&["a"]);
        let (_other, hold, checkout) = held_in_the_teardown_of_a_closed_project(&mut s);
        request_json(&mut s.app, COMMAND, &json!({"paths": [checkout]}));
        pump_until(&mut s.app, &s.rx, "the request's removal starts", |a| !a.git.cleanup.pending.is_empty());
        confirmed(&mut s, 2, &["a"]);
        remove(&mut s);

        pump_until(&mut s.app, &s.rx, "the dialog's removal ends", |a| toast(a) == Some("removed 1 worktree"));

        assert!(checkout.exists(), "the request's removal still waits");
        assert_eq!(s.app.take_answers(), Vec::new());
        let_it_end(&hold);
    }

    #[test]
    fn the_dialogs_toast_does_not_count_what_a_request_removed() {
        let (mut s, a) = with_a(&["a", "b"]);
        let b = path_of(&s, "b");
        s.removes(&json!({"paths": [a]}));
        pump_until_gone(&mut s, &a);
        confirmed(&mut s, 1, &["b"]);
        remove(&mut s);

        let toast = settled_text(&mut s, 1);

        assert_eq!(toast.as_deref(), Some("removed 1 worktree"));
        assert!(!b.exists());
    }
}

mod a_closed_projects_folder {
    use super::*;

    #[test]
    fn a_teardown_runs_first_for_a_closed_projects_worktree() {
        let (mut s, at) = with_one("feat/x");
        let marker = s.worktrees.path().join("marker");
        let script = format!("test -f README && echo \"$CORNERCASE_BRANCH\" > '{}'", marker.display());
        let key = s.repo.path().display().to_string();
        s.app.config.projects.insert(key, ProjectSettings { teardown: script, ..Default::default() });

        let folder = s.removes(&json!({"paths": [at]}));

        assert_eq!(folder["teardown"], true);
        assert_eq!(std::fs::read_to_string(&marker).expect("the teardown ran").trim(), "feat/x");
        assert!(!at.exists());
    }

    #[test]
    fn a_teardown_that_fails_still_removes_and_says_so() {
        let (mut s, at) = with_one("a");
        let key = s.repo.path().display().to_string();
        s.app
            .config
            .projects
            .insert(key, ProjectSettings { teardown: "echo nope; exit 3".into(), ..Default::default() });

        let folder = s.removes(&json!({"paths": [at]}));

        assert!(folder["warnings"].to_string().contains("nope"), "{folder}");
        assert!(!at.exists());
    }

    #[test]
    fn a_process_outside_cornercase_in_the_folder_waits_for_force() {
        let (mut s, at) = with_one("a");
        let sleeper =
            crate::test_util::Running::spawn(std::process::Command::new("/bin/sleep").arg("30").current_dir(&at));
        wait_until("sleep works in the folder", || process::cwd(sleeper.pid()).as_deref() == Some(at.as_path()));

        let said = s.refuses(&json!({"paths": [at]}), &["force"]);

        assert!(said.contains("1 process outside cornercase: sleep"), "{said}");
        assert!(at.exists());
        s.removes(&json!({"paths": [at], "force": true}));
    }
}
