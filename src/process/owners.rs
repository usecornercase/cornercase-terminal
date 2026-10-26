use std::collections::HashMap;
use std::path::{Path, PathBuf};

const MAX_ANCESTORS: usize = 64;

pub(crate) type Trees<T> = HashMap<i32, T>;

pub(crate) fn trees<T: Clone>(shells: &[(i32, T)]) -> Trees<T> {
    let mut trees = Trees::new();
    for (pid, owner) in shells {
        trees.insert(*pid, owner.clone());
        for child in super::descendants(*pid) {
            trees.entry(child).or_insert_with(|| owner.clone());
        }
    }
    trees
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Owner<'a, T> {
    Pane(&'a T),
    Server,
    None,
}

pub(crate) fn owner<T>(pid: i32, trees: &Trees<T>, server: i32) -> Owner<'_, T> {
    let mut at = pid;
    for _ in 0..MAX_ANCESTORS {
        if let Some(owner) = trees.get(&at) {
            return Owner::Pane(owner);
        }
        let Some(up) = super::parent(at) else { return Owner::None };
        if up == server && super::name(at).as_deref() == Some("git") {
            return Owner::Server;
        }
        if up == server {
            return trees.get(&up).map_or(Owner::None, Owner::Pane);
        }
        if up <= 1 || up == at {
            return Owner::None;
        }
        at = up;
    }
    Owner::None
}

pub(crate) fn folder(cwd: &Path, paths: &[PathBuf]) -> Option<usize> {
    paths
        .iter()
        .enumerate()
        .filter(|(_, path)| cwd.starts_with(path))
        .max_by_key(|(_, path)| path.components().count())
        .map(|(index, _)| index)
}

#[cfg(test)]
mod tests;
