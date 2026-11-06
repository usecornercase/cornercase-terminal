use rstest::rstest;

use super::*;
use crate::test_util::TempDir;
use crate::worktrees::Live;

fn role(name: &str, path: &Path) -> RoleTree {
    RoleTree { role: name.to_string(), path: path.to_path_buf(), branch: name.to_string() }
}

mod a_folder_that_failed {
    use super::*;

    fn failed_with(roles: Vec<RoleTree>) -> FolderRemoved {
        let mut answer = FolderRemoved { roles, ..FolderRemoved::default() };
        let mut tally = Tally::default();
        tally.error = Some("git says no".to_string());
        ended(&mut answer, tally);
        answer
    }

    #[test]
    fn names_the_roles_no_longer_on_disk_and_the_branch_that_stays() {
        let dir = TempDir::new();
        let gone = dir.path().join("gone");

        let answer = failed_with(vec![role("coder", &gone)]);

        assert_eq!((answer.outcome.as_str(), answer.why.as_deref()), (FAILED, Some("git says no")));
        let said = format!(
            "the worktree of `coder` went before this failed: {} is gone, its branch coder stays",
            gone.display()
        );
        assert_eq!(answer.warnings, [said]);
    }

    #[test]
    fn says_nothing_of_a_role_still_on_disk() {
        let dir = TempDir::new();

        let answer = failed_with(vec![role("coder", dir.path())]);

        assert_eq!(answer.warnings, Vec::<String>::new());
    }
}

mod a_role_named_with_its_workspace {
    use super::*;

    fn following(path: &Path, outcome: &str) -> FolderRemoved {
        let lead =
            FolderRemoved { path: PathBuf::from("lead"), outcome: outcome.to_string(), ..FolderRemoved::default() };
        let mut answer = FolderRemoved { path: path.to_path_buf(), ..FolderRemoved::default() };
        follow(&mut answer, &lead);
        answer
    }

    #[rstest]
    #[case::its_workspace_failed(FAILED, "it is still there after lead failed")]
    #[case::its_workspace_went(REMOVED, "it is still there after lead went")]
    fn fails_while_on_disk_and_says_what_happened(#[case] lead: &str, #[case] said: &str) {
        let dir = TempDir::new();

        let answer = following(dir.path(), lead);

        assert_eq!((answer.outcome.as_str(), answer.why.as_deref()), (FAILED, Some(said)));
    }

    #[test]
    fn answers_removed_once_gone_even_when_its_workspace_failed() {
        let dir = TempDir::new();

        let answer = following(&dir.path().join("gone"), FAILED);

        assert_eq!((answer.outcome.as_str(), answer.why), (REMOVED, None));
    }
}

mod folders_that_carry_each_other {
    use super::*;

    fn named(dir: &Path, folders: &[(&str, Option<&str>)]) -> Removing {
        let folder = |(name, carries): &(&str, Option<&str>)| {
            let roles = carries.iter().map(|carried| role(carried, &dir.join(carried))).collect();
            let worktree = Some(WorktreeInfo::default());
            let answer = FolderRemoved { path: canonical(dir.join(name)), roles, worktree, ..FolderRemoved::default() };
            Folder { follows: None, answer }
        };
        let folders = folders.iter().map(folder).collect();
        let mut removing = Removing { request: WorktreesRemove::default(), folders };
        removing.link_followers();
        removing
    }

    fn following(removing: &Removing) -> Vec<Option<usize>> {
        removing.folders.iter().map(|folder| folder.follows).collect()
    }

    #[test]
    fn a_role_follows_its_workspace_and_ends_with_it() {
        let dir = TempDir::new();
        let mut removing = named(dir.path(), &[("role", None), ("lead", Some("role"))]);
        assert_eq!(following(&removing), [Some(1), None]);

        removing.settle(1, Tally::default());

        assert!(removing.ended());
        assert_eq!(removing.folders[0].answer.outcome, REMOVED);
    }

    #[test]
    fn a_chain_ends_once_the_folders_that_started_end() {
        let dir = TempDir::new();
        let mut removing = named(dir.path(), &[("a", Some("b")), ("b", Some("c")), ("c", None)]);
        assert_eq!(following(&removing), [None, Some(0), None]);

        removing.settle(0, Tally::default());
        assert!(!removing.ended());
        removing.settle(2, Tally::default());

        assert!(removing.ended());
    }

    #[test]
    fn a_circle_starts_every_folder_so_each_gets_its_answer() {
        let dir = TempDir::new();
        let mut removing = named(dir.path(), &[("a", Some("b")), ("b", Some("a"))]);
        assert_eq!(following(&removing), [None, None]);

        removing.settle(0, Tally::default());
        removing.settle(1, Tally::default());

        assert!(removing.ended());
    }
}

mod an_open_project {
    use super::*;

    #[test]
    fn inside_a_folder_that_would_go_stops_it_and_is_named() {
        let dir = TempDir::new();
        let (going, project) = (dir.path().join("wt"), dir.path().join("wt").join("api"));
        std::fs::create_dir_all(&project).expect("mkdir");

        let said = project_inside(std::slice::from_ref(&project), &[going]).expect("refused");

        assert!(said.contains("is open inside it") && said.contains(&project.display().to_string()), "{said}");
    }

    #[test]
    fn at_a_role_that_would_go_stops_it() {
        let dir = TempDir::new();

        let said = project_inside(&[dir.path().to_path_buf()], &[PathBuf::from("lead"), dir.path().to_path_buf()]);

        assert!(said.is_some());
    }

    #[test]
    fn around_or_beside_a_folder_that_would_go_stops_nothing() {
        let dir = TempDir::new();
        let going = dir.path().join("wt");
        let beside = dir.path().join("wt-2");

        assert_eq!(project_inside(&[dir.path().to_path_buf(), beside], &[going]), None);
    }
}

mod a_role_still_busy {
    use super::*;

    fn row_at(path: &Path) -> Row {
        let found = Found {
            repo: PathBuf::from("repo"),
            project: "shop".to_string(),
            path: path.to_path_buf(),
            branch: None,
            admin: None,
            kind: Kind::Linked,
        };
        Row {
            gone: false,
            ticked: false,
            removing: false,
            freeing: false,
            elsewhere: Vec::new(),
            outside: Vec::new(),
            size: None,
            scan: None,
            live: None,
            found,
        }
    }

    #[test]
    fn stops_its_lead_while_its_build_folders_are_freed() {
        let dir = TempDir::new();
        let row = Row { freeing: true, ..row_at(dir.path()) };

        let busy = busy_role(&[row], &[role("coder", dir.path())]);

        assert_eq!(busy.as_deref(), Some("the build folders of `coder` are being freed"));
    }

    #[test]
    fn stops_its_lead_while_it_is_being_removed() {
        let dir = TempDir::new();
        let row = Row { removing: true, ..row_at(dir.path()) };

        let busy = busy_role(&[row], &[role("coder", dir.path())]);

        assert_eq!(busy.as_deref(), Some("the worktree of `coder` is already being removed or closed"));
    }

    #[test]
    fn stops_nothing_for_only_belonging_to_its_leads_run() {
        let dir = TempDir::new();
        let live = Live {
            role_of_run: true,
            project: 1,
            workspace: 2,
            running: Vec::new(),
            removing: false,
            run_roles: 0,
            pull_merged: false,
            pull_head: None,
        };
        let row = Row { live: Some(live), ..row_at(dir.path()) };

        assert_eq!(busy_role(&[row], &[role("coder", dir.path())]), None);
    }
}
