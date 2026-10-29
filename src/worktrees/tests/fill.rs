use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use super::*;
use crate::error::Error;
use crate::test_util::wait_until;
use crate::worktrees::fill::{Asks, at_most, fill};
use crate::worktrees::{Found, List};

fn row(found: Found) -> Row {
    Row { found, ..list::row("p", "b", Kind::Linked) }
}

fn asks(rows: &[Row], sizes: bool) -> Asks {
    Asks {
        bases: vec![Some("main".to_string()); rows.len()],
        names: sizes.then(|| vec![vec!["node_modules".to_string()]; rows.len()]),
        paths: crate::worktrees::folders_of(rows),
        shells: Vec::new(),
    }
}

#[test]
fn at_most_answers_in_order_and_runs_no_more_than_asked_at_once() {
    let started = Arc::new(AtomicUsize::new(0));
    let open = Arc::new(AtomicBool::new(false));
    let work = {
        let (started, open) = (started.clone(), open.clone());
        move |item: usize| {
            started.fetch_add(1, Ordering::SeqCst);
            while !open.load(Ordering::SeqCst) {
                std::thread::yield_now();
            }
            Ok(item)
        }
    };
    let opener = {
        let (started, open) = (started.clone(), open.clone());
        std::thread::spawn(move || {
            wait_until("two are running", || started.load(Ordering::SeqCst) == 2);
            let enough = started.load(Ordering::SeqCst);
            open.store(true, Ordering::SeqCst);
            enough
        })
    };

    let answers = at_most((0..8).collect(), 2, Duration::from_secs(60), work);

    assert_eq!(opener.join().expect("opener"), 2);
    let answers: Vec<usize> = answers.into_iter().map(|a| a.expect("answered").expect("ok")).collect();
    assert_eq!(answers, (0..8).collect::<Vec<_>>());
}

#[test]
fn a_late_item_is_given_up_while_the_others_answer() {
    let release = Arc::new(AtomicBool::new(false));
    let work = {
        let release = release.clone();
        move |item: usize| {
            while item == 1 && !release.load(Ordering::SeqCst) {
                std::thread::yield_now();
            }
            Ok(item)
        }
    };

    let answers = at_most((0..3).collect(), 2, Duration::from_millis(50), work);
    release.store(true, Ordering::SeqCst);

    assert!(answers[1].is_none());
    assert!(matches!(answers[0], Some(Ok(0))) && matches!(answers[2], Some(Ok(2))));
}

#[test]
fn a_panicking_item_answers_an_error_without_waiting_for_the_give_up() {
    let work = |item: usize| -> crate::error::Result<usize> {
        assert_ne!(item, 0, "boom");
        Ok(item)
    };
    let before = Instant::now();

    let answers = at_most(vec![0, 1], 2, Duration::from_secs(60), work);

    assert!(matches!(answers[0], Some(Err(Error::Bug))));
    assert!(matches!(answers[1], Some(Ok(1))));
    assert!(before.elapsed() < Duration::from_secs(30));
}

fn rows_of_a_repository() -> (Vec<Row>, Vec<TempDir>) {
    let repo = main_repo();
    let dirs = TempDir::new();
    for branch in ["dirty", "clean", "lost"] {
        add(repo.path(), branch, &dirs.path().join(branch));
    }
    commit_at(&dirs.path().join("lost"), "2020-01-01T00:00:00Z");
    std::fs::write(dirs.path().join("dirty/notes.txt"), "draft").expect("write");
    std::fs::create_dir_all(dirs.path().join("clean/node_modules")).expect("mkdir");
    std::fs::write(dirs.path().join("clean/.gitignore"), "node_modules/\n").expect("ignore");
    std::fs::write(dirs.path().join("clean/node_modules/x.js"), vec![1_u8; 100_000]).expect("write");
    std::fs::remove_dir_all(dirs.path().join("lost")).expect("remove");
    let none = TempDir::new();
    let rows = discover(&[repo.path().to_path_buf()], none.path()).into_iter().map(row).collect();
    (rows, vec![repo, dirs, none])
}

fn of<'a>(rows: &'a [Row], branch: &str) -> &'a Row {
    rows.iter().find(|r| r.found.branch.as_deref() == Some(branch)).expect("the row")
}

fn counts(row: &Row) -> (usize, usize) {
    let Some(Ok(scan)) = &row.scan else { panic!("{:?} was not scanned", row.found.path) };
    (scan.changed, scan.unpushed)
}

fn builds_of(row: &Row) -> u64 {
    let Some(Ok(size)) = &row.size else { panic!("{:?} was not measured", row.found.path) };
    size.builds_bytes()
}

#[test]
fn fill_scans_counts_measures_and_sorts_like_the_dialog() {
    let (rows, _keep) = rows_of_a_repository();

    let filled = fill(rows.clone(), asks(&rows, true));

    assert_eq!((filled[0].found.kind, counts(&filled[0]), filled[0].size.is_none()), (Kind::Prunable, (0, 1), true));
    assert_eq!(counts(of(&filled, "dirty")).0, 1);
    assert!(builds_of(of(&filled, "clean")) >= 100_000);
    assert_eq!(List::new(filled).pending(), 0);
}

#[test]
fn fill_leaves_the_sizes_out_when_it_has_no_names() {
    let (rows, _keep) = rows_of_a_repository();

    let filled = fill(rows.clone(), asks(&rows, false));

    assert!(filled.iter().all(|r| r.size.is_none()));
    assert_eq!(counts(of(&filled, "dirty")).0, 1);
}
