use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use super::{Kind, List, Row, SCAN_GIVE_UP, SCANS_AT_ONCE, SIZE_GIVE_UP, SIZES_AT_ONCE, builds, scan};
use crate::error::Result;
use crate::panics;

pub(crate) struct Asks {
    pub(crate) bases: Vec<Option<String>>,
    pub(crate) names: Option<Vec<Vec<String>>>,
    pub(crate) paths: Vec<PathBuf>,
    pub(crate) shells: Vec<(i32, Vec<usize>)>,
}

type Answers<R> = Vec<Option<Result<R>>>;

pub(crate) fn fill(mut rows: Vec<Row>, asks: Asks) -> Vec<Row> {
    scans(&mut rows, &asks.bases);
    if let Some(names) = asks.names {
        sizes(&mut rows, &names);
    }
    let found = scan::processes(&asks.paths, &asks.shells);
    for row in &mut rows {
        row.set_processes(&found);
    }
    List::new(rows).rows
}

fn scans(rows: &mut [Row], bases: &[Option<String>]) {
    let jobs: Vec<(usize, ScanJob)> = rows
        .iter()
        .enumerate()
        .filter_map(|(i, row)| {
            let branch = match row.found.kind {
                Kind::Linked => None,
                Kind::Prunable => Some(row.found.branch.clone()?),
                Kind::Orphan | Kind::Unknown => return None,
            };
            let job = ScanJob {
                path: row.found.path.clone(),
                repo: row.found.repo.clone(),
                base: bases.get(i).cloned().flatten(),
                branch,
            };
            Some((i, job))
        })
        .collect();
    let (at, jobs): (Vec<usize>, Vec<ScanJob>) = jobs.into_iter().unzip();
    let work = |job: ScanJob| scan::scan_or_count(&job.path, &job.repo, job.base, job.branch.as_deref());
    let answers = at_most(jobs, SCANS_AT_ONCE, SCAN_GIVE_UP, work);
    for (i, answer) in at.into_iter().zip(answers) {
        rows[i].scan = Some(settle(answer, "a scan", &rows[i].found.path));
    }
}

struct ScanJob {
    path: PathBuf,
    repo: PathBuf,
    base: Option<String>,
    branch: Option<String>,
}

fn sizes(rows: &mut [Row], names: &[Vec<String>]) {
    let at: Vec<usize> = (0..rows.len()).filter(|&i| rows[i].has_folder() && !rows[i].gone).collect();
    let jobs: Vec<(PathBuf, Kind, Vec<String>)> = at
        .iter()
        .map(|&i| (rows[i].found.path.clone(), rows[i].found.kind, names.get(i).cloned().unwrap_or_default()))
        .collect();
    let work = |(path, kind, names): (PathBuf, Kind, Vec<String>)| builds::measure(&path, kind, &names);
    let answers = at_most(jobs, SIZES_AT_ONCE, SIZE_GIVE_UP, work);
    for (i, answer) in at.into_iter().zip(answers) {
        rows[i].size = Some(settle(answer, "a size", &rows[i].found.path));
    }
}

fn settle<R>(answer: Option<Result<R>>, what: &str, path: &std::path::Path) -> std::result::Result<R, String> {
    let Some(result) = answer else {
        crate::log::warning!("worktrees", "gave up", what = what, path = path.display());
        return Err("gave up".to_string());
    };
    result.map_err(|e| e.to_string())
}

pub(super) fn at_most<T, R>(
    items: Vec<T>,
    at_once: usize,
    give_up: Duration,
    work: impl Fn(T) -> Result<R> + Clone + Send + 'static,
) -> Answers<R>
where
    T: Send + 'static,
    R: Send + 'static,
{
    let total = items.len();
    let mut waiting = items.into_iter().enumerate();
    let mut answers: Answers<R> = (0..total).map(|_| None).collect();
    let mut running: Vec<(usize, Instant)> = Vec::new();
    let (tx, rx) = mpsc::channel::<(usize, Result<R>)>();
    let mut left = total;
    while left > 0 {
        while running.len() < at_once {
            let Some((i, item)) = waiting.next() else { break };
            start(i, item, &work, &tx);
            running.push((i, Instant::now()));
        }
        let oldest = running.iter().map(|(_, since)| since.elapsed()).max().unwrap_or_default();
        match rx.recv_timeout(give_up.saturating_sub(oldest)) {
            Ok((i, result)) => {
                if let Some(at) = running.iter().position(|(j, _)| *j == i) {
                    running.remove(at);
                    answers[i] = Some(result);
                    left -= 1;
                }
            }
            Err(_) => left -= drop_late(&mut running, give_up),
        }
    }
    answers
}

fn start<T, R>(
    i: usize,
    item: T,
    work: &(impl Fn(T) -> Result<R> + Clone + Send + 'static),
    tx: &mpsc::Sender<(usize, Result<R>)>,
) where
    T: Send + 'static,
    R: Send + 'static,
{
    let (work, tx) = (work.clone(), tx.clone());
    thread::spawn(move || {
        let _ = tx.send((i, panics::job(|| work(item))));
    });
}

fn drop_late(running: &mut Vec<(usize, Instant)>, give_up: Duration) -> usize {
    let before = running.len();
    running.retain(|(_, since)| since.elapsed() < give_up);
    before - running.len()
}
