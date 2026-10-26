use std::path::{Path, PathBuf};

use super::{Owner, Trees, folder, owner, trees};
use crate::test_util::{Family, Running, TempDir};

fn me() -> i32 {
    i32::try_from(std::process::id()).expect("a pid")
}

mod trees {
    use super::*;

    #[test]
    fn a_shell_owns_every_process_it_started() {
        let family = Family::new();
        let grandchild = family.grandchild().expect("a grandchild");
        let trees = trees(&[(family.pid(), 7_u64)]);
        assert_eq!(trees.get(&grandchild), Some(&7));
        assert_eq!(owner(grandchild, &trees, 0), Owner::Pane(&7));
    }

    #[test]
    fn a_process_started_by_a_pane_belongs_to_it_wherever_it_works() {
        let elsewhere = TempDir::new();
        let sleeper = Running::spawn(std::process::Command::new("/bin/sleep").arg("30").current_dir(elsewhere.path()));
        crate::test_util::wait_until("the sleeper works elsewhere", || {
            crate::process::cwd(sleeper.pid()).is_some_and(|cwd| folder(&cwd, &[elsewhere.path().into()]).is_some())
        });
        let trees = Trees::from([(me(), 3_u64)]);
        assert_eq!(owner(sleeper.pid(), &trees, 0), Owner::Pane(&3));
    }

    #[test]
    fn a_process_no_pane_started_has_no_owner() {
        let family = Family::new();
        let grandchild = family.grandchild().expect("a grandchild");
        assert_eq!(owner(grandchild, &Trees::<u64>::new(), 0), Owner::None);
    }
}

mod server {
    use super::*;

    fn git_and_sleep() -> (TempDir, Running, Running) {
        let bin = TempDir::new();
        let git = crate::test_util::sleep_named(&bin.path().join("git"));
        let sleep = Running::spawn(std::process::Command::new("/bin/sleep").arg("30"));
        crate::test_util::wait_until("git is named git", || crate::process::name(git.pid()).as_deref() == Some("git"));
        (bin, git, sleep)
    }

    #[test]
    fn the_servers_git_belongs_to_the_server() {
        let (_bin, git, _sleep) = git_and_sleep();
        assert_eq!(owner(git.pid(), &Trees::<u64>::new(), me()), Owner::Server);
    }

    #[test]
    fn another_child_of_the_server_has_no_owner_unless_the_server_is_a_pane() {
        let (_bin, _git, sleep) = git_and_sleep();
        assert_eq!(owner(sleep.pid(), &Trees::<u64>::new(), me()), Owner::None);
        assert_eq!(owner(sleep.pid(), &Trees::from([(me(), 5_u64)]), me()), Owner::Pane(&5));
    }
}

mod folder {
    use super::*;

    fn paths() -> Vec<PathBuf> {
        vec!["/repo".into(), "/repo/.worktrees/feat".into(), "/other".into()]
    }

    #[test]
    fn the_deepest_folder_holding_the_cwd_wins() {
        assert_eq!(folder(Path::new("/repo/.worktrees/feat/src"), &paths()), Some(1));
        assert_eq!(folder(Path::new("/repo/src"), &paths()), Some(0));
    }

    #[test]
    fn a_folder_matches_whole_components_only() {
        assert_eq!(folder(Path::new("/repo/.worktrees/feature"), &paths()), Some(0));
        assert_eq!(folder(Path::new("/others"), &paths()), None);
    }
}
