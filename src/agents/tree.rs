use std::collections::{HashMap, HashSet};

pub(crate) const MAX_DEPTH: usize = 3;
pub(crate) const NARROW_DEPTH: usize = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Node {
    pub(crate) index: usize,
    pub(crate) depth: usize,
    pub(crate) last: bool,
    pub(crate) rails: Vec<bool>,
    pub(crate) has_children: bool,
    pub(crate) hidden: Vec<usize>,
}

pub(crate) fn order(rows: &[(u64, Option<u64>)], folded: &HashSet<u64>, narrow: bool) -> Vec<Node> {
    let cap = if narrow { NARROW_DEPTH } else { MAX_DEPTH };
    let mut first = HashMap::new();
    for (index, (id, _)) in rows.iter().enumerate() {
        first.entry(*id).or_insert(index);
    }
    let parent: Vec<Option<usize>> = rows
        .iter()
        .enumerate()
        .map(|(index, (_, parent))| parent.and_then(|id| first.get(&id).copied()).filter(|&p| p != index))
        .collect();
    let mut children = vec![Vec::new(); rows.len()];
    for (index, parent) in parent.iter().enumerate() {
        if let Some(parent) = parent {
            children[*parent].push(index);
        }
    }
    let folded: Vec<bool> = rows.iter().map(|(id, _)| folded.contains(id)).collect();
    let mut visited = vec![false; rows.len()];
    let mut nodes = Vec::with_capacity(rows.len());
    for index in (0..rows.len()).filter(|&index| parent[index].is_none()) {
        walk(index, &children, &folded, &mut visited, cap, &mut nodes);
    }
    while let Some(index) = (0..rows.len()).find(|&index| !visited[index]) {
        walk(index, &children, &folded, &mut visited, cap, &mut nodes);
    }
    shape(&mut nodes);
    nodes
}

fn shape(nodes: &mut [Node]) {
    let mut seen: Vec<bool> = Vec::new();
    for node in nodes.iter_mut().rev() {
        let depth = node.depth;
        seen.resize(seen.len().max(depth + 1), false);
        node.last = !seen[depth];
        node.rails = seen[1..depth.max(1)].to_vec();
        seen[depth] = true;
        seen.truncate(depth + 1);
    }
}

fn walk(
    root: usize,
    children: &[Vec<usize>],
    folded: &[bool],
    visited: &mut [bool],
    cap: usize,
    nodes: &mut Vec<Node>,
) {
    visited[root] = true;
    let mut stack = vec![(root, 0)];
    while let Some((index, depth)) = stack.pop() {
        let next: Vec<usize> = children[index].iter().copied().filter(|&child| !visited[child]).collect();
        for &child in &next {
            visited[child] = true;
        }
        let mut node = Node {
            index,
            depth: depth.min(cap),
            last: true,
            rails: Vec::new(),
            has_children: !next.is_empty(),
            hidden: Vec::new(),
        };
        if folded[index] {
            node.hidden = hide(next, children, visited);
        } else {
            for &child in next.iter().rev() {
                stack.push((child, depth + 1));
            }
        }
        nodes.push(node);
    }
}

fn hide(mut stack: Vec<usize>, children: &[Vec<usize>], visited: &mut [bool]) -> Vec<usize> {
    let mut hidden = Vec::new();
    while let Some(index) = stack.pop() {
        hidden.push(index);
        for &child in &children[index] {
            if !visited[child] {
                visited[child] = true;
                stack.push(child);
            }
        }
    }
    hidden.sort_unstable();
    hidden
}

#[cfg(test)]
mod tests;
