use std::path::PathBuf;

use super::super::tests::parsed;
use super::*;
use crate::cli::args::worktrees::WorktreesAction;
use crate::control::WorktreeInfo;
use crate::control::worktrees::remove::{FORCE as FORCE_FLAG, UNLOCK as UNLOCK_FLAG};

const HOME: &str = "/home/ana";

fn row(branch: Option<&str>) -> WorktreeInfo {
    WorktreeInfo {
        path: "/home/ana/wt/shop/feat-old".into(),
        project_name: "shop".into(),
        branch: branch.map(str::to_string),
        ..WorktreeInfo::default()
    }
}

fn folder(outcome: &str) -> FolderRemoved {
    FolderRemoved { path: "/home/ana/wt/shop/feat-old".into(), outcome: outcome.into(), ..FolderRemoved::default() }
}

fn removal(folder: FolderRemoved) -> String {
    text(&WorktreesRemoved { check: false, folders: vec![folder] }, false, Some(Path::new(HOME)))
}

mod its_flags {
    use super::*;

    fn remove_args(args: &[&str]) -> RemoveArgs {
        let mut all = vec!["remove"];
        all.extend(args);
        match parsed(&all).expect("it parses") {
            WorktreesAction::Remove(args) => args,
            other => panic!("not remove: {other:?}"),
        }
    }

    #[test]
    fn take_several_folders_after_one_flag_or_after_many() {
        let together = remove_args(&["--path", "/a", "/b"]);
        let apart = remove_args(&["--path", "/a", "--path", "/b", "--delete-branch", "--json"]);

        assert_eq!(together.paths, [PathBuf::from("/a"), PathBuf::from("/b")]);
        assert_eq!(apart.paths, together.paths);
        assert!(apart.delete_branch && apart.print.json && !apart.force && !apart.unlock && !apart.check);
    }

    #[test]
    fn need_a_folder() {
        assert!(parsed(&["remove"]).is_err());
        assert!(parsed(&["remove", "--force"]).is_err());
    }

    #[test]
    fn keep_a_check_from_also_asking_to_remove() {
        assert!(parsed(&["remove", "--path", "/a", "--check", "--force"]).is_err());
        assert!(parsed(&["remove", "--path", "/a", "--check", "--unlock"]).is_err());
        assert!(remove_args(&["--path", "/a", "--check", "--delete-branch"]).check);
    }
}

mod one_line_per_folder {
    use super::*;

    #[test]
    fn says_what_went_with_the_home_as_a_tilde() {
        assert_eq!(removal(folder(REMOVED)), "removed ~/wt/shop/feat-old\n");
    }

    #[test]
    fn names_the_branches_that_went_with_it() {
        let one = FolderRemoved { branches: vec!["feat/old".into()], ..folder(REMOVED) };
        let two = FolderRemoved { branches: vec!["a".into(), "b".into()], ..folder(REMOVED) };

        assert_eq!(removal(one), "removed ~/wt/shop/feat-old and the branch feat/old\n");
        assert_eq!(removal(two), "removed ~/wt/shop/feat-old and the branches a, b\n");
    }

    #[test]
    fn says_a_role_went_with_its_runs_workspace() {
        let role = FolderRemoved { removed_with: Some("/home/ana/wt/shop/lead".into()), ..folder(REMOVED) };

        assert_eq!(removal(role), "removed ~/wt/shop/feat-old with ~/wt/shop/lead\n");
    }

    #[test]
    fn keeps_what_failed_after_the_removal_on_the_same_line() {
        let partly = FolderRemoved {
            kept: vec!["kept the branch a: it changed since the confirmation".into()],
            warnings: vec!["a: the teardown failed".into()],
            ..folder(REMOVED)
        };

        assert_eq!(
            removal(partly),
            "removed ~/wt/shop/feat-old; kept the branch a: it changed since the confirmation; a: the teardown failed\n"
        );
    }

    #[test]
    fn tells_a_refusal_from_a_failure_and_says_why() {
        let refused = FolderRemoved { why: Some("1 file changed; add --force".into()), ..folder(REFUSED) };
        let failed = FolderRemoved { why: Some("git says no".into()), ..folder("failed") };

        assert_eq!(removal(refused), "did not remove ~/wt/shop/feat-old: 1 file changed; add --force\n");
        assert_eq!(removal(failed), "could not remove ~/wt/shop/feat-old: git says no\n");
    }

    #[test]
    fn keeps_the_order_it_was_given() {
        let second = FolderRemoved { path: "/srv/b".into(), ..folder(REMOVED) };
        let answer = WorktreesRemoved { check: false, folders: vec![folder(REMOVED), second] };

        assert_eq!(text(&answer, false, Some(Path::new(HOME))), "removed ~/wt/shop/feat-old\nremoved /srv/b\n");
    }
}

mod the_check {
    use super::*;

    fn look(folder: FolderRemoved, branches: bool) -> String {
        text(&WorktreesRemoved { check: true, folders: vec![folder] }, branches, Some(Path::new(HOME)))
    }

    #[test]
    fn reads_as_the_dialogs_second_look() {
        let folder = FolderRemoved {
            lines: vec!["1 file changed".into(), "2 commits not pushed".into(), "teardown runs first".into()],
            lost: vec!["feat/old: its commits will be lost".into()],
            needs: vec![FORCE_FLAG.into(), UNLOCK_FLAG.into()],
            worktree: Some(row(Some("feat/old"))),
            ..folder("removable")
        };

        assert_eq!(
            look(folder.clone(), false),
            "shop › feat/old  ~/wt/shop/feat-old\n  1 file changed\n  2 commits not pushed\n  teardown runs first\n  removing it takes --force and --unlock\n"
        );
        assert!(look(folder, true).contains("  feat/old: its commits will be lost\n"));
    }

    #[test]
    fn names_a_folder_with_no_branch_by_its_folder() {
        let folder = FolderRemoved { worktree: Some(row(None)), lines: vec!["locked".into()], ..folder("removable") };

        assert_eq!(look(folder, false), "shop › feat-old  ~/wt/shop/feat-old\n  locked\n");
    }

    #[test]
    fn says_why_a_folder_cannot_be_removed_and_not_what_it_would_take() {
        let unknown = FolderRemoved {
            why: Some("it is a repository's main checkout".into()),
            needs: vec![FORCE_FLAG.into()],
            ..folder(REFUSED)
        };

        assert_eq!(
            look(unknown, false),
            "~/wt/shop/feat-old\n  cannot be removed: it is a repository's main checkout\n"
        );
    }

    #[test]
    fn says_a_role_goes_with_its_runs_workspace() {
        let role = FolderRemoved {
            removed_with: Some("/srv/lead".into()),
            worktree: Some(row(Some("coder"))),
            ..folder("removable")
        };

        assert_eq!(look(role, false), "shop › coder  ~/wt/shop/feat-old\n  goes with /srv/lead\n");
    }
}

mod the_exit_status {
    use super::*;

    #[test]
    fn is_clean_only_when_every_folder_went_with_nothing_kept_or_failed() {
        assert!(went_well(&folder(REMOVED), false));
        assert!(!went_well(&folder(REFUSED), false));
        assert!(!went_well(&folder("failed"), false));
        assert!(!went_well(&FolderRemoved { warnings: vec!["x".into()], ..folder(REMOVED) }, false));
        assert!(!went_well(&FolderRemoved { kept: vec!["x".into()], ..folder(REMOVED) }, false));
    }

    #[test]
    fn of_a_check_says_only_what_could_not_be_read_or_removed() {
        let needs_force = FolderRemoved { needs: vec![FORCE_FLAG.into()], ..folder("removable") };
        let unread = FolderRemoved { unchecked: vec!["role coder: gave up".into()], ..folder("removable") };

        assert!(went_well(&needs_force, true));
        assert!(!went_well(&folder(REFUSED), true));
        assert!(!went_well(&unread, true));
    }
}
