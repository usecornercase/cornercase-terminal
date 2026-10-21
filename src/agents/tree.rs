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
    let mut walker = Walker {
        children: &children,
        folded: &folded,
        cap,
        visited: vec![false; rows.len()],
        nodes: Vec::with_capacity(rows.len()),
    };
    for index in (0..rows.len()).filter(|&index| parent[index].is_none()) {
        walker.walk(index);
    }
    while let Some(index) = (0..rows.len()).find(|&index| !walker.visited[index]) {
        walker.walk(index);
    }
    let mut nodes = walker.nodes;
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

struct Walker<'a> {
    children: &'a [Vec<usize>],
    folded: &'a [bool],
    cap: usize,
    visited: Vec<bool>,
    nodes: Vec<Node>,
}

impl Walker<'_> {
    fn walk(&mut self, root: usize) {
        self.visited[root] = true;
        let mut stack = vec![(root, 0)];
        while let Some((index, depth)) = stack.pop() {
            let next: Vec<usize> = self.children[index].iter().copied().filter(|&child| !self.visited[child]).collect();
            for &child in &next {
                self.visited[child] = true;
            }
            let mut node = Node {
                index,
                depth: depth.min(self.cap),
                last: true,
                rails: Vec::new(),
                has_children: !next.is_empty(),
                hidden: Vec::new(),
            };
            if self.folded[index] {
                node.hidden = hide(next, self.children, &mut self.visited);
            } else {
                stack.extend(next.iter().rev().map(|&child| (child, depth + 1)));
            }
            self.nodes.push(node);
        }
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
