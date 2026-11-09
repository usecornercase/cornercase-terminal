use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::*;
use crate::changes::diff::{Diff, File};
use crate::test_util::{git, git_repo};

fn target(workspace: u64, dir: &str, working: bool) -> Target {
    Target { checkout: Checkout { workspace, dir: PathBuf::from(dir), base: None }, working }
}

fn file(added: usize, removed: usize) -> std::sync::Arc<File> {
    let mut file = crate::changes::diff::untracked("a", 0, None);
    file.added = added;
    file.removed = removed;
    std::sync::Arc::new(file)
}

fn counted(files: Vec<std::sync::Arc<File>>, digest: u64) -> git::Loaded {
    git::Loaded { base: Some("origin/main".into()), digest, diff: Some(Diff { files }) }
}

fn same(digest: u64) -> git::Loaded {
    git::Loaded { base: Some("origin/main".into()), digest, diff: None }
}

fn ask(tallies: &mut Tallies, targets: &[Target], now: Instant) -> Option<(u64, u64)> {
    tallies.request(targets, now).map(|(workspace, generation, _)| (workspace, generation))
}

mod pacing {
    use super::*;

    #[test]
    fn reads_one_workspace_at_a_time() {
        let mut tallies = Tallies::default();
        let now = Instant::now();
        let targets = [target(1, "/a", false), target(2, "/b", false)];
        let (_, generation) = ask(&mut tallies, &targets, now).expect("first");
        assert!(ask(&mut tallies, &targets, now).is_none());
        tallies.loaded(generation, Ok(counted(vec![file(1, 0)], 1)), now);
        assert_eq!(ask(&mut tallies, &targets, now).map(|(w, _)| w), Some(2));
    }

    #[test]
    fn ignores_an_answer_for_an_old_generation() {
        let mut tallies = Tallies::default();
        let now = Instant::now();
        let targets = [target(1, "/a", false)];
        let (_, generation) = ask(&mut tallies, &targets, now).expect("first");
        tallies.loaded(generation + 1, Ok(counted(vec![file(1, 0)], 1)), now);
        assert!(tallies.busy());
        assert!(tallies.get(1, None).is_none());
    }

    #[test]
    fn waits_before_reading_a_workspace_again() {
        let mut tallies = Tallies::default();
        let now = Instant::now();
        let targets = [target(1, "/a", false)];
        let (_, generation) = ask(&mut tallies, &targets, now).expect("first");
        tallies.loaded(generation, Ok(counted(vec![file(1, 0)], 1)), now);
        assert!(ask(&mut tallies, &targets, now + CLOSED_EVERY.saturating_sub(Duration::from_millis(1))).is_none());
        assert!(ask(&mut tallies, &targets, now + CLOSED_EVERY).is_some());
    }

    #[test]
    fn a_slow_read_waits_ten_times_as_long() {
        let mut tallies = Tallies::default();
        let now = Instant::now();
        let targets = [target(1, "/a", false)];
        let (_, generation) = ask(&mut tallies, &targets, now).expect("first");
        let done = now + Duration::from_secs(2);
        tallies.loaded(generation, Ok(counted(vec![file(1, 0)], 1)), done);
        assert!(ask(&mut tallies, &targets, now + Duration::from_secs(19)).is_none());
        assert!(ask(&mut tallies, &targets, now + Duration::from_secs(20)).is_some());
    }

    #[test]
    fn asks_the_least_recently_asked_first() {
        let mut tallies = Tallies::default();
        let now = Instant::now();
        let targets = [target(1, "/a", false), target(2, "/b", false)];
        let (first, generation) = ask(&mut tallies, &targets, now).expect("first");
        tallies.loaded(generation, Ok(same(1)), now);
        let later = now + CLOSED_EVERY;
        let (second, _) = ask(&mut tallies, &targets, later).expect("second");
        assert_eq!((first, second), (1, 2));
    }

    #[test]
    fn a_working_agent_goes_first() {
        let mut tallies = Tallies::default();
        let targets = [target(1, "/a", false), target(2, "/b", true)];
        assert_eq!(ask(&mut tallies, &targets, Instant::now()).map(|(w, _)| w), Some(2));
    }

    #[test]
    fn workspaces_in_one_folder_are_read_once() {
        let mut tallies = Tallies::default();
        let now = Instant::now();
        let targets = [target(1, "/a", false), target(2, "/a", false), target(3, "/b", false)];
        let (_, generation) = ask(&mut tallies, &targets, now).expect("first");
        tallies.loaded(generation, Ok(counted(vec![file(5, 2)], 1)), now);
        for workspace in [1, 2] {
            let tally = tallies.get(workspace, None).expect("counted");
            assert_eq!((tally.files, tally.added, tally.removed), (1, 5, 2));
        }
        assert!(tallies.get(3, None).is_none());
        assert_eq!(ask(&mut tallies, &targets, now).map(|(w, _)| w), Some(3));
    }
}

mod answers {
    use super::*;

    fn held() -> (Tallies, Vec<Target>, Instant) {
        let mut tallies = Tallies::default();
        let now = Instant::now();
        let targets = vec![target(1, "/a", false)];
        let (_, generation) = ask(&mut tallies, &targets, now).expect("first");
        tallies.loaded(generation, Ok(counted(vec![file(4, 1)], 7)), now);
        (tallies, targets, now)
    }

    fn again(tallies: &mut Tallies, targets: &[Target], now: Instant, result: Result<git::Loaded>) {
        let (_, generation) = ask(tallies, targets, now + Duration::from_secs(60)).expect("due");
        tallies.loaded(generation, result, now + Duration::from_secs(60));
    }

    #[test]
    fn an_unchanged_answer_keeps_the_counts() {
        let (mut tallies, targets, now) = held();
        again(&mut tallies, &targets, now, Ok(same(7)));
        assert_eq!(tallies.get(1, None).map(|t| t.added), Some(4));
    }

    #[test]
    fn a_timeout_keeps_the_counts() {
        let (mut tallies, targets, now) = held();
        again(&mut tallies, &targets, now, Err(Error::Git("timed out".into())));
        assert_eq!(tallies.get(1, None).map(|t| t.added), Some(4));
    }

    #[test]
    fn an_error_drops_the_counts() {
        let (mut tallies, targets, now) = held();
        again(&mut tallies, &targets, now, Err(Error::Git("cannot compare".into())));
        assert!(tallies.get(1, None).is_none());
    }

    #[test]
    fn forgetting_drops_the_count_and_the_answer_in_flight() {
        let mut tallies = Tallies::default();
        let now = Instant::now();
        let targets = [target(1, "/a", false)];
        let (_, generation) = ask(&mut tallies, &targets, now).expect("first");
        tallies.forget(1);
        tallies.loaded(generation, Ok(counted(vec![file(1, 1)], 1)), now);
        assert!(tallies.get(1, None).is_none());
    }

    #[test]
    fn a_count_for_another_base_is_not_given_and_is_read_again_at_once() {
        let (mut tallies, _, now) = held();
        let mut other = target(1, "/a", false);
        other.checkout.base = Some("origin/dev".into());
        assert!(tallies.get(1, Some("origin/dev")).is_none());
        assert!(ask(&mut tallies, &[other], now).is_some());
    }

    #[test]
    fn a_rescan_makes_the_workspace_due_again() {
        let (mut tallies, targets, now) = held();
        tallies.rescan(1);
        assert!(ask(&mut tallies, &targets, now).is_some());
    }
}

mod reading {
    use super::*;

    fn read(dir: &std::path::Path, last: Option<u64>) -> git::Loaded {
        let request = git::Request {
            dir: dir.to_path_buf(),
            mode: Mode::All,
            base: None,
            last,
            previous: Vec::new(),
            expanded: Some(Vec::new()),
        };
        git::load(&request).expect("load")
    }

    fn full(dir: &std::path::Path) -> Diff {
        let request = git::Request {
            dir: dir.to_path_buf(),
            mode: Mode::All,
            base: None,
            last: None,
            previous: Vec::new(),
            expanded: None,
        };
        git::load(&request).expect("load").diff.expect("diff")
    }

    #[test]
    fn counts_what_all_counts() {
        let repo = git_repo(&[("a.txt", "one\ntwo\n")]);
        git(repo.path(), &["checkout", "--quiet", "-b", "topic"]);
        std::fs::write(repo.path().join("b.txt"), "x\ny\nz\n").expect("write");
        git(repo.path(), &["add", "."]);
        git(repo.path(), &["commit", "--quiet", "-m", "b"]);
        std::fs::write(repo.path().join("a.txt"), "one\nTWO\n").expect("edit");
        std::fs::write(repo.path().join("c.txt"), "new\nlines\n").expect("untracked");
        let counted = read(repo.path(), None).diff.expect("diff");
        let full = full(repo.path());
        assert_eq!(
            (counted.files.len(), counted.added(), counted.removed()),
            (full.files.len(), full.added(), full.removed())
        );
        assert_eq!(counted.files.len(), 3);
    }

    #[test]
    fn a_second_read_with_the_digest_answers_nothing_new() {
        let repo = git_repo(&[("a.txt", "one\n")]);
        std::fs::write(repo.path().join("a.txt"), "two\n").expect("edit");
        let first = read(repo.path(), None);
        assert!(first.diff.is_some());
        assert!(read(repo.path(), Some(first.digest)).diff.is_none());
    }
}

mod default_branch {
    use super::*;
    use crate::test_util::clone_of;

    #[test]
    fn counts_the_uncommitted_files_and_the_commits_not_pushed() {
        let remote = git_repo(&[("a.txt", "one\n")]);
        git(remote.path(), &["branch", "-M", "main"]);
        let (_dir, checkout) = clone_of(remote.path());
        std::fs::write(checkout.join("b.txt"), "x\ny\n").expect("write");
        git(&checkout, &["add", "."]);
        git(&checkout, &["commit", "--quiet", "-m", "unpushed"]);
        std::fs::write(checkout.join("a.txt"), "one\ntwo\n").expect("edit");
        let request = git::Request {
            dir: checkout,
            mode: Mode::All,
            base: None,
            last: None,
            previous: Vec::new(),
            expanded: Some(Vec::new()),
        };
        let loaded = git::load(&request).expect("load");
        let diff = loaded.diff.expect("diff");
        assert_eq!(loaded.base.as_deref(), Some("origin/main"));
        assert_eq!((diff.files.len(), diff.added(), diff.removed()), (2, 3, 0));
    }
}
