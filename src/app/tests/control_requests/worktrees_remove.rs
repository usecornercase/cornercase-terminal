use std::path::PathBuf;

use super::*;
use crate::config::ProjectSettings;
use crate::test_util::{git, git_repo};

mod links;
mod open;
mod second_look;

const COMMAND: &str = "worktrees-remove";

trait Asks {
    fn parts(&mut self) -> (&mut App, &Receiver<AppEvent>);

    fn asked(&mut self, args: &Value) -> Value {
        let (app, rx) = self.parts();
        call_later(app, rx, COMMAND, args).expect("an answer")
    }

    fn one(&mut self, args: &Value) -> Value {
        let mut answer = self.asked(args);
        assert_eq!(answer["folders"].as_array().map(Vec::len), Some(1), "{answer}");
        answer["folders"][0].take()
    }

    fn refuses(&mut self, args: &Value, needs: &[&str]) -> String {
        let folder = self.one(args);
        assert_eq!((outcome(&folder), &folder["needs"]), ("refused", &json!(needs)), "{folder}");
        why(&folder).to_string()
    }

    fn removes(&mut self, args: &Value) -> Value {
        let folder = self.one(args);
        assert_eq!(outcome(&folder), "removed", "{folder}");
        folder
    }
}

struct Closed {
    app: App,
    rx: Receiver<AppEvent>,
    repo: TempDir,
    worktrees: TempDir,
}

impl Asks for Closed {
    fn parts(&mut self) -> (&mut App, &Receiver<AppEvent>) {
        (&mut self.app, &self.rx)
    }
}

fn closed_repo() -> Closed {
    let (mut app, rx) = empty_app();
    let worktrees = TempDir::new();
    app.config.worktrees_dir = worktrees.path().display().to_string();
    Closed { app, rx, repo: git_repo(&[("README", "hi")]), worktrees }
}

fn with_one(branch: &str) -> (Closed, PathBuf) {
    let s = closed_repo();
    let at = s.add(branch);
    (s, at)
}

impl Closed {
    fn folder(&self, slug: &str) -> PathBuf {
        self.worktrees.path().join(self.repo.path().file_name().expect("a name")).join(slug)
    }

    fn add(&self, branch: &str) -> PathBuf {
        let at = self.folder(&branch.replace('/', "-"));
        git(self.repo.path(), &["worktree", "add", "--quiet", "-b", branch, &at.display().to_string()]);
        at
    }

    fn has_branch(&self, branch: &str) -> bool {
        branch_exists(self.repo.path(), branch)
    }

    fn leftover_folder(&self, gitdir: &str) -> PathBuf {
        let at = self.worktrees.path().join("gone").join("x");
        std::fs::create_dir_all(&at).expect("mkdir");
        std::fs::write(at.join(".git"), format!("gitdir: {gitdir}\n")).expect("dot git");
        std::fs::write(at.join("notes.txt"), "mine").expect("write");
        at
    }

    fn main_on_side_and_a_worktree_on(&self, branch: &str) -> PathBuf {
        let at = self.folder("the-other");
        git(self.repo.path(), &["checkout", "--quiet", "-b", "side"]);
        git(self.repo.path(), &["worktree", "add", "--quiet", "--force", &at.display().to_string(), branch]);
        at
    }
}

fn outcome(folder: &Value) -> &str {
    folder["outcome"].as_str().unwrap_or_default()
}

fn why(folder: &Value) -> &str {
    folder["why"].as_str().unwrap_or_default()
}

fn outcomes(answer: &Value) -> Vec<&str> {
    answer["folders"].as_array().expect("a list").iter().map(outcome).collect()
}

fn commit_in(dir: &Path) {
    std::fs::write(dir.join("work.txt"), "work").expect("write");
    git(dir, &["add", "--all"]);
    git(dir, &["commit", "--quiet", "-m", "work"]);
}

fn write_notes(dir: &Path) {
    std::fs::write(dir.join("notes.txt"), "draft").expect("write");
}

fn force_takes_it(s: &mut Closed, at: &Path, left: &str) {
    assert!(at.join(left).exists());
    s.removes(&json!({"paths": [at], "force": true}));
    assert!(!at.exists());
}

mod closed_projects {
    use super::*;

    #[test]
    fn a_worktree_goes_its_branch_stays_and_the_window_says_nothing() {
        let (mut s, at) = with_one("feat/old");

        let answer = s.asked(&json!({"paths": [at]}));

        let folder = &answer["folders"][0];
        assert_eq!((outcome(folder), &folder["path"], &answer["check"]), ("removed", &json!(at), &json!(false)));
        assert_eq!(folder["worktree"]["branch"], "feat/old");
        assert!(!at.exists());
        assert!(s.has_branch("feat/old"));
        assert_eq!(toast(&s.app), None);
        assert!(s.app.overlay.is_none());
    }

    #[test]
    fn its_branch_goes_too_when_asked() {
        let (mut s, at) = with_one("feat/old");

        let folder = s.removes(&json!({"paths": [at], "delete_branch": true}));

        assert_eq!(folder["branches"], json!(["feat/old"]));
        assert!(!at.exists() && !s.has_branch("feat/old"));
    }

    #[test]
    fn each_folder_answers_in_order_and_one_refusal_stops_nothing_else() {
        let mut s = closed_repo();
        let (a, b) = (s.add("a"), s.add("b"));
        let nothing = s.folder("nothing");
        let main = s.repo.path().to_path_buf();

        let answer = s.asked(&json!({"paths": [nothing, a, main, b]}));

        assert_eq!(outcomes(&answer), ["refused", "removed", "refused", "removed"]);
        assert!(!a.exists() && !b.exists());
    }

    #[test]
    fn a_folder_named_twice_answers_twice_and_goes_once() {
        let (mut s, at) = with_one("a");

        let answer = s.asked(&json!({"paths": [at, at], "delete_branch": true}));

        let folders = answer["folders"].as_array().expect("a list");
        assert_eq!(folders.len(), 2);
        assert_eq!(folders[0], folders[1]);
        assert_eq!((outcome(&folders[0]), &folders[0]["branches"]), ("removed", &json!(["a"])));
        assert_eq!(folders[0]["warnings"], json!([]));
    }

    #[test]
    fn a_folder_with_files_nobody_committed_waits_for_force() {
        let (mut s, at) = with_one("a");
        write_notes(&at);

        let said = s.refuses(&json!({"paths": [at]}), &["force"]);

        assert!(said.contains("1 file changed") && said.contains("--force"), "{said}");
        force_takes_it(&mut s, &at, "notes.txt");
    }

    #[test]
    fn a_locked_worktree_waits_for_unlock_even_with_force() {
        let (mut s, at) = with_one("a");
        git(s.repo.path(), &["worktree", "lock", "--reason", "on a usb disk", &at.display().to_string()]);

        let said = s.refuses(&json!({"paths": [at], "force": true}), &["unlock"]);

        assert!(said.contains("is locked (on a usb disk)") && said.contains("--unlock"), "{said}");
        assert!(said.contains("git worktree unlock") && at.exists(), "{said}");
        s.removes(&json!({"paths": [at], "unlock": true}));
        assert!(!at.exists());
    }

    #[test]
    fn a_locked_worktree_with_changes_needs_both_flags() {
        let (mut s, at) = with_one("a");
        write_notes(&at);
        git(s.repo.path(), &["worktree", "lock", &at.display().to_string()]);
        let both = ["force", "unlock"];

        let unlocked = s.refuses(&json!({"paths": [at], "unlock": true}), &both);
        let forced = s.refuses(&json!({"paths": [at], "force": true}), &both);

        assert!(unlocked.contains("--force") && !unlocked.contains("--unlock"), "{unlocked}");
        assert!(forced.contains("--unlock") && !forced.contains("--force"), "{forced}");
        assert!(at.join("notes.txt").exists());
        s.removes(&json!({"paths": [at], "force": true, "unlock": true}));
        assert!(!at.exists());
    }

    #[test]
    fn commits_not_pushed_stay_with_their_branch_so_they_refuse_only_when_it_would_go() {
        let mut s = closed_repo();
        let (keeps, loses) = (s.add("keeps"), s.add("loses"));
        commit_in(&keeps);
        commit_in(&loses);

        s.removes(&json!({"paths": [keeps]}));
        assert!(s.has_branch("keeps"));

        let said = s.refuses(&json!({"paths": [loses], "delete_branch": true}), &["force"]);
        assert!(said.contains("loses: its commits will be lost"), "{said}");
        assert!(loses.exists() && s.has_branch("loses"));

        let forced = s.removes(&json!({"paths": [loses], "delete_branch": true, "force": true}));
        assert_eq!(forced["branches"], json!(["loses"]));
        assert!(!s.has_branch("loses"));
    }

    #[test]
    fn commits_on_no_branch_wait_for_force_since_removing_the_folder_loses_them() {
        let (mut s, at) = with_one("a");
        git(&at, &["checkout", "--quiet", "--detach"]);
        commit_in(&at);

        let said = s.refuses(&json!({"paths": [at]}), &["force"]);

        assert!(said.contains("1 commit not pushed, on no branch"), "{said}");
        s.removes(&json!({"paths": [at], "force": true}));
    }

    #[test]
    fn a_folder_that_could_not_be_checked_counts_as_having_changes() {
        let (mut s, at) = with_one("a");
        git(&at, &["checkout", "--quiet", "--orphan", "unborn"]);

        let said = s.refuses(&json!({"paths": [at]}), &["force"]);

        assert!(said.contains("so it may have changes"), "{said}");
        force_takes_it(&mut s, &at, "README");
    }

    #[test]
    fn a_folder_whose_build_folders_are_being_freed_cannot_be_ticked() {
        let (mut s, at) = with_one("a");
        s.app.git.frees.freeing(at.clone());

        let refused = s.one(&json!({"paths": [at], "force": true, "unlock": true}));

        assert_eq!((outcome(&refused), why(&refused)), ("refused", "its build folders are being freed"));
        assert!(at.exists());
    }
}

mod leftovers {
    use super::*;

    fn listed(s: &mut Closed) -> Vec<Value> {
        let listed = call_later(&mut s.app, &s.rx, "worktrees", &json!({"sizes": false})).expect("the list");
        listed["worktrees"].as_array().expect("a list").iter().map(|row| row["path"].clone()).collect()
    }

    #[test]
    fn a_prunable_entry_leaves_gits_list_and_the_others_stay() {
        let mut s = closed_repo();
        let (a, b, stays) = (s.add("a"), s.add("b"), s.add("stays"));
        std::fs::remove_dir_all(&a).expect("remove");
        std::fs::remove_dir_all(&b).expect("remove");
        let found = s.one(&json!({"paths": [a], "check": true}));
        assert_eq!(found["worktree"]["kind"], "prunable");

        s.removes(&json!({"paths": [a]}));

        let paths = listed(&mut s);
        assert!(s.has_branch("a"));
        assert!(!paths.contains(&json!(a)) && paths.contains(&json!(b)) && paths.contains(&json!(stays)), "{paths:?}");
    }

    #[test]
    fn a_prunable_entrys_branch_goes_when_asked_and_its_commits_wait_for_force() {
        let (mut s, at) = with_one("lost");
        s.add("stays");
        commit_in(&at);
        std::fs::remove_dir_all(&at).expect("remove");

        let said = s.refuses(&json!({"paths": [at], "delete_branch": true}), &["force"]);
        assert!(said.contains("lost: its commits will be lost"), "{said}");
        assert!(s.has_branch("lost"));

        let forced = s.removes(&json!({"paths": [at], "delete_branch": true, "force": true}));
        assert_eq!(forced["branches"], json!(["lost"]));
        assert!(!s.has_branch("lost"));
    }

    #[test]
    fn a_folder_whose_repository_is_gone_waits_for_force_and_then_goes() {
        let mut s = closed_repo();
        let at = s.leftover_folder("/nonexistent/repo/.git/worktrees/x");
        assert_eq!(s.one(&json!({"paths": [at], "check": true}))["worktree"]["kind"], "repository-gone");

        let said = s.refuses(&json!({"paths": [at]}), &["force"]);

        assert!(said.contains("its repository is gone; deletes the folder and its files"), "{said}");
        force_takes_it(&mut s, &at, "notes.txt");
    }

    #[test]
    fn a_folder_git_no_longer_knows_waits_for_force_and_then_goes() {
        let mut s = closed_repo();
        let admin = TempDir::new();
        let at = s.leftover_folder(&admin.path().display().to_string());
        assert_eq!(s.one(&json!({"paths": [at], "check": true}))["worktree"]["kind"], "unknown-to-git");

        let said = s.refuses(&json!({"paths": [at]}), &["force"]);

        assert!(said.contains("git no longer knows this folder"), "{said}");
        force_takes_it(&mut s, &at, "notes.txt");
    }
}

mod refusals {
    use super::*;

    fn left_alone(folder: &Path) -> Vec<String> {
        let names = std::fs::read_dir(folder).expect("the folder is there").flatten();
        let mut names: Vec<String> = names.map(|entry| entry.file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        names
    }

    #[test]
    fn a_main_checkout_is_never_a_target_whatever_the_flags() {
        let (mut s, _) = with_one("a");
        let main = s.repo.path().to_path_buf();
        let before = left_alone(&main);

        let refused = s.one(&json!({"paths": [main], "force": true, "unlock": true, "delete_branch": true}));

        assert_eq!(outcome(&refused), "refused");
        assert!(why(&refused).contains("main checkout"), "{refused}");
        assert_eq!(left_alone(&main), before);
        assert!(s.has_branch("main"));
    }

    #[test]
    fn a_plain_folder_is_refused_in_words_and_nothing_happens_to_it() {
        let mut s = closed_repo();
        let plain = TempDir::new();
        std::fs::write(plain.path().join("precious"), "x").expect("write");

        let refused = s.one(&json!({"paths": [plain.path()], "force": true, "unlock": true}));

        assert_eq!(outcome(&refused), "refused");
        assert_eq!(why(&refused), "it is not a worktree that `cornercase worktrees list` knows");
        assert_eq!(left_alone(plain.path()), ["precious"]);
    }

    #[test]
    fn a_folder_inside_a_worktree_and_the_folder_above_one_are_not_that_worktree() {
        let (mut s, at) = with_one("a");
        std::fs::create_dir(at.join("src")).expect("mkdir");
        let (inside, above) = (at.join("src"), at.parent().expect("a parent").to_path_buf());

        let answer = s.asked(&json!({"paths": [inside, above], "force": true, "unlock": true}));

        assert_eq!(outcomes(&answer), ["refused", "refused"]);
        assert!(inside.is_dir() && at.join("README").exists());
    }

    #[test]
    fn a_path_with_nothing_there_is_refused() {
        let mut s = closed_repo();
        let nothing = s.folder("nothing");

        let refused = s.one(&json!({"paths": [nothing], "force": true}));

        assert_eq!(why(&refused), "nothing is there, and no repository lists a worktree there");
    }

    #[test]
    fn a_relative_path_is_refused_whatever_folder_the_server_runs_in() {
        let mut s = closed_repo();

        let answer = s.asked(&json!({"paths": [".", "..", "feat-old"], "force": true, "unlock": true}));

        for folder in answer["folders"].as_array().expect("a list") {
            assert_eq!((outcome(folder), why(folder)), ("refused", "it is not an absolute path"));
        }
    }

    #[test]
    fn a_request_with_no_folders_or_with_a_stray_field_is_refused_as_a_whole() {
        let mut s = closed_repo();

        assert_eq!(refused(&mut s.app, COMMAND, &json!({})).code, "malformed");
        assert_eq!(refused(&mut s.app, COMMAND, &json!({"paths": []})).code, "malformed");
        let stray = refused(&mut s.app, COMMAND, &json!({"paths": ["/x"], "branches": true}));
        assert_eq!((stray.code.as_str(), stray.message.contains("args.branches")), ("unknown-field", true));
    }
}

mod branches {
    use super::*;

    #[test]
    fn the_default_branch_is_never_deleted_and_the_folder_still_goes() {
        let mut s = closed_repo();
        let at = s.main_on_side_and_a_worktree_on("main");

        let folder = s.removes(&json!({"paths": [at], "delete_branch": true}));

        assert_eq!(folder["branches"], json!([]));
        assert!(folder["warnings"].to_string().contains("refusing to delete the branch main"), "{folder}");
        assert!(!at.exists() && s.has_branch("main"));
    }

    #[test]
    fn the_main_checkouts_branch_is_never_deleted() {
        let mut s = closed_repo();
        let at = s.main_on_side_and_a_worktree_on("side");

        let folder = s.removes(&json!({"paths": [at], "delete_branch": true}));

        assert!(folder["warnings"].to_string().contains("refusing to delete the branch side"), "{folder}");
        assert!(s.has_branch("side"));
    }

    #[test]
    fn a_detached_worktree_deletes_no_branch_and_says_so() {
        let (mut s, at) = with_one("a");
        git(&at, &["checkout", "--quiet", "--detach"]);
        let look = s.one(&json!({"paths": [at], "delete_branch": true, "check": true}));
        assert!(look["lines"].to_string().contains("no branch is checked out, so none is deleted"), "{look}");

        let folder = s.removes(&json!({"paths": [at], "delete_branch": true}));

        assert_eq!((&folder["branches"], &folder["kept"]), (&json!([]), &json!([])));
        assert!(s.has_branch("a"));
    }

    #[test]
    fn a_branch_with_no_commits_of_its_own_goes_without_force() {
        let (mut s, at) = with_one("fresh");

        let look = s.one(&json!({"paths": [at], "delete_branch": true, "check": true}));

        assert_eq!((outcome(&look), &look["lost"], &look["needs"]), ("removable", &json!([]), &json!([])));
    }
}

mod the_check {
    use super::*;

    #[test]
    fn names_what_the_second_look_names_and_stops_nothing() {
        let (mut s, at) = with_one("feat/x");
        commit_in(&at);
        write_notes(&at);
        git(s.repo.path(), &["worktree", "lock", "--reason", "busy", &at.display().to_string()]);
        let key = s.repo.path().display().to_string();
        s.app.config.projects.insert(key, ProjectSettings { teardown: "exit 1".into(), ..Default::default() });

        let answer = s.asked(&json!({"paths": [at], "check": true}));

        assert_eq!(answer["check"], true);
        let folder = &answer["folders"][0];
        assert_eq!(outcome(folder), "removable");
        assert_eq!((&folder["needs"], &folder["teardown"]), (&json!(["force", "unlock"]), &json!(true)));
        assert_eq!(
            folder["lines"],
            json!(["1 file changed", "1 commit not pushed", "locked: busy", "teardown runs first"])
        );
        assert_eq!(folder["lost"], json!(["feat/x: its commits will be lost"]));
        assert_eq!((&folder["worktree"]["changed"], &folder["worktree"]["unpushed"]), (&json!(1), &json!(1)));
        assert_eq!(folder["worktree"]["lock"]["reason"], "busy");
        assert!(at.join("notes.txt").exists() && s.has_branch("feat/x"));
    }

    #[test]
    fn says_nothing_uncommitted_or_unpushed_when_there_is_nothing() {
        let (mut s, at) = with_one("clean");

        let folder = s.one(&json!({"paths": [at], "check": true}));

        assert_eq!(folder["lines"], json!(["nothing uncommitted or unpushed"]));
        assert_eq!((&folder["needs"], &folder["lost"]), (&json!([]), &json!([])));
    }

    #[test]
    fn removes_nothing_even_when_it_is_also_told_to_force_and_unlock() {
        let (mut s, at) = with_one("a");
        write_notes(&at);
        let prunable = s.add("b");
        std::fs::remove_dir_all(&prunable).expect("remove");
        let orphan = s.leftover_folder("/nonexistent/repo/.git/worktrees/x");
        let args = json!({"paths": [at, prunable, orphan], "check": true, "force": true, "unlock": true, "delete_branch": true});

        let answer = s.asked(&args);

        assert_eq!(outcomes(&answer), ["removable", "removable", "removable"]);
        assert!(at.join("notes.txt").exists() && orphan.join("notes.txt").exists());
        assert!(s.has_branch("a") && s.has_branch("b"));
        let again = s.asked(&args);
        assert_eq!(outcomes(&again), ["removable", "removable", "removable"], "it is all still listed");
    }

    #[test]
    fn says_why_a_folder_cannot_be_removed() {
        let mut s = closed_repo();
        let main = s.repo.path().to_path_buf();

        let folder = s.one(&json!({"paths": [main], "check": true}));

        assert_eq!(outcome(&folder), "refused");
        assert!(why(&folder).contains("main checkout"), "{folder}");
    }
}
