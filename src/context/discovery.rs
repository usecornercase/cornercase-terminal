use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};

use crate::process;

#[derive(Debug)]
pub(super) struct Job<T>(Option<Receiver<T>>);

impl<T> Default for Job<T> {
    fn default() -> Self {
        Self(None)
    }
}

impl<T: Send + 'static> Job<T> {
    pub fn take(&mut self) -> Option<T> {
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

    pub fn start(&mut self, work: impl FnOnce() -> T + Send + 'static) {
        if self.0.is_none() {
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                tx.send(work()).ok();
            });
            self.0 = Some(rx);
        }
    }

    pub fn running(&self) -> bool {
        self.0.is_some()
    }
}

pub(super) fn folder(pid: i32) -> Option<PathBuf> {
    process::cwd(pid)?.canonicalize().ok()
}

pub(super) fn home(pid: i32, variable: &str, suffix: &str, default: &str) -> Option<PathBuf> {
    let env = process::env(pid);
    let var = |name| env.iter().find(|(key, value)| key == name && !value.is_empty()).map(|(_, v)| PathBuf::from(v));
    let path = var(variable).map(|home| home.join(suffix)).or_else(|| var("HOME").map(|p| p.join(default)))?;
    let path = if path.is_absolute() { path } else { process::cwd(pid)?.join(path) };
    path.canonicalize().ok()
}

pub(super) fn named(args: &[String], name: &str) -> bool {
    let script = args
        .first()
        .filter(|arg| Path::new(arg).file_name().is_some_and(|n| n == "node"))
        .and_then(|_| args.iter().skip(1).find(|arg| !arg.starts_with('-')));
    args.iter().take(2).chain(script).any(|arg| Path::new(arg).file_name().is_some_and(|n| n == name))
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::direct(&["/usr/local/bin/gemini"], true)]
    #[case::parent(&["node", "/usr/local/bin/gemini"], true)]
    #[case::child(&["/usr/local/bin/node", "--max-old-space-size=8192", "/usr/local/bin/gemini"], true)]
    #[case::argument(&["cat", "--", "/tmp/gemini"], false)]
    #[case::other_script(&["node", "--max-old-space-size=8192", "/tmp/other", "/tmp/gemini"], false)]
    fn a_node_child_is_recognized_past_its_runtime_flags(#[case] args: &[&str], #[case] expected: bool) {
        assert_eq!(named(&args.iter().map(|arg| (*arg).to_string()).collect::<Vec<_>>(), "gemini"), expected);
    }
}
