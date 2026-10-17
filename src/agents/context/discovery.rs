use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};

use crate::process;

#[derive(Debug)]
pub(in crate::agents) struct Job<T>(Option<Receiver<T>>);

impl<T> Default for Job<T> {
    fn default() -> Self {
        Self(None)
    }
}

impl<T: Send + 'static> Job<T> {
    pub(in crate::agents) fn take(&mut self) -> Option<T> {
        let receiver = self.0.take()?;
        match receiver.try_recv() {
            Ok(answer) => Some(answer),
            Err(mpsc::TryRecvError::Empty) => {
                self.0 = Some(receiver);
                None
            }
            Err(mpsc::TryRecvError::Disconnected) => None,
        }
    }

    pub(in crate::agents) fn start(&mut self, work: impl FnOnce() -> T + Send + 'static) {
        if self.0.is_none() {
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                tx.send(work()).ok();
            });
            self.0 = Some(rx);
        }
    }

    pub(in crate::agents) fn running(&self) -> bool {
        self.0.is_some()
    }
}

#[derive(Debug)]
pub(in crate::agents) struct Follower<S> {
    kept: Option<Box<S>>,
    reading: Job<Option<S>>,
}

impl<S> Default for Follower<S> {
    fn default() -> Self {
        Self { kept: None, reading: Job::default() }
    }
}

impl<S: Send + 'static> Follower<S> {
    pub(in crate::agents) fn poll(
        &mut self,
        work: impl FnOnce(Option<S>) -> Option<S> + Send + 'static,
        mut answered: impl FnMut(Option<&mut S>),
    ) {
        if let Some(found) = self.reading.take() {
            self.kept = found.map(Box::new);
            answered(self.kept.as_deref_mut());
        }
        if !self.reading.running() {
            let previous = self.kept.take().map(|session| *session);
            self.reading.start(move || work(previous));
        }
    }
}

pub(in crate::agents) fn folder(pid: i32) -> Option<PathBuf> {
    process::cwd(pid)?.canonicalize().ok()
}

pub(in crate::agents) fn home(pid: i32, variable: &str, suffix: &str, default: &str) -> Option<PathBuf> {
    let env = process::env(pid);
    let var = |name| env.iter().find(|(key, value)| key == name && !value.is_empty()).map(|(_, v)| PathBuf::from(v));
    let path = var(variable).map(|home| home.join(suffix)).or_else(|| var("HOME").map(|p| p.join(default)))?;
    let path = if path.is_absolute() { path } else { process::cwd(pid)?.join(path) };
    path.canonicalize().ok()
}

pub(in crate::agents) fn named(args: &[String], name: &str) -> bool {
    let script = args
        .first()
        .filter(|arg| Path::new(arg).file_name().is_some_and(|n| n == "node"))
        .and_then(|_| args.iter().skip(1).find(|arg| !arg.starts_with('-')));
    args.iter().take(2).chain(script).any(|arg| Path::new(arg).file_name().is_some_and(|n| n == name))
}

#[cfg(test)]
mod tests;
