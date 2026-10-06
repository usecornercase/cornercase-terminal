use std::path::PathBuf;

use crate::activity::Status;
use crate::config::Config;
use crate::context::Context;
use crate::split::{self, Dir, Node};
use crate::term::Term;
use crate::ui::GroupEntry;

pub struct Tab {
    pub id: u64,
    pub name: Option<String>,
    pub panes: Vec<Term>,
    pub active: usize,
    pub layout: Node<u64>,
    pub right_clicks: Vec<u64>,
}

impl Tab {
    pub fn new(id: u64, name: Option<String>, term: Term) -> Self {
        let layout = Node::Leaf(term.id);
        Self { id, name, panes: vec![term], active: 0, layout, right_clicks: Vec::new() }
    }

    pub fn restored(id: u64, name: Option<String>, panes: Vec<Term>, layout: Option<&Node<usize>>) -> Self {
        let ids: Vec<u64> = panes.iter().map(|t| t.id).collect();
        let mapped = layout.and_then(|l| l.map(&|i| ids.get(i).copied()));
        let layout = match mapped {
            Some(l) if is_permutation(&l.ids(), &ids) => l,
            _ => split::row(&ids).unwrap_or(Node::Leaf(0)),
        };
        Self { id, name, panes, active: 0, layout, right_clicks: Vec::new() }
    }

    pub fn pane(&self) -> Option<&Term> {
        self.panes.get(self.active)
    }

    pub fn pane_mut(&mut self) -> Option<&mut Term> {
        self.panes.get_mut(self.active)
    }

    pub fn label(&self, config: &Config) -> String {
        self.name.clone().or_else(|| self.pane().and_then(|t| t.program(config))).unwrap_or_else(|| "?".into())
    }

    pub fn status(&self) -> Option<Status> {
        self.panes.iter().filter_map(|t| t.agent.status()).max()
    }

    pub fn context(&self) -> Option<&Context> {
        self.pane().and_then(|t| t.context.context()).or_else(|| self.panes.iter().find_map(|t| t.context.context()))
    }

    pub fn memory(&self) -> Option<u64> {
        self.panes.iter().filter_map(|t| t.memory.bytes()).reduce(u64::saturating_add)
    }

    pub fn focus(&mut self, id: u64) {
        if let Some(i) = self.panes.iter().position(|t| t.id == id) {
            self.active = i;
        }
    }

    pub fn split(&mut self, target: u64, dir: Dir, term: Term) {
        if self.layout.split(target, dir, term.id) {
            self.panes.push(term);
            self.active = self.panes.len() - 1;
        }
    }

    pub fn right_clicks_to_pane(&self, id: u64) -> bool {
        self.right_clicks.contains(&id)
    }

    pub fn toggle_right_clicks(&mut self, id: u64) {
        if let Some(i) = self.right_clicks.iter().position(|p| *p == id) {
            self.right_clicks.remove(i);
        } else {
            self.right_clicks.push(id);
        }
    }

    fn remove(&mut self, id: u64) -> bool {
        let Some(p) = self.panes.iter().position(|t| t.id == id) else { return false };
        let near = self.layout.remove(id);
        self.panes.remove(p);
        self.right_clicks.retain(|r| *r != id);
        let near = near.and_then(|n| self.panes.iter().position(|t| t.id == n));
        match near {
            Some(n) if p == self.active => self.active = n,
            _ => shift_active(&mut self.active, p),
        }
        true
    }
}

fn is_permutation(found: &[u64], expected: &[u64]) -> bool {
    let mut found = found.to_vec();
    let mut expected = expected.to_vec();
    found.sort_unstable();
    expected.sort_unstable();
    found == expected
}

pub struct Workspace {
    pub id: u64,
    pub path: PathBuf,
    pub name: Option<String>,
    pub worktree: bool,
    pub tabs: Vec<Tab>,
    pub active: usize,
    pub closing: bool,
    pub behind: u32,
    pub base: Option<String>,
}

impl Workspace {
    pub fn new(id: u64, path: PathBuf, name: Option<String>, worktree: bool) -> Self {
        Self { id, path, name, worktree, tabs: Vec::new(), active: 0, closing: false, behind: 0, base: None }
    }

    pub fn tab(&self) -> Option<&Tab> {
        self.tabs.get(self.active)
    }

    pub fn tab_mut(&mut self) -> Option<&mut Tab> {
        self.tabs.get_mut(self.active)
    }

    pub fn terms(&self) -> impl Iterator<Item = &Term> {
        self.tabs.iter().flat_map(|t| &t.panes)
    }

    pub fn terms_mut(&mut self) -> impl Iterator<Item = &mut Term> {
        self.tabs.iter_mut().flat_map(|t| &mut t.panes)
    }

    pub fn label(&self) -> String {
        self.name.clone().or_else(|| crate::git::branch(&self.path)).unwrap_or_else(|| "default".into())
    }

    pub fn remove_term(&mut self, id: u64) -> bool {
        let Some(t) = self.tabs.iter().position(|tab| tab.panes.iter().any(|p| p.id == id)) else { return false };
        let tab = &mut self.tabs[t];
        tab.remove(id);
        if tab.panes.is_empty() {
            self.tabs.remove(t);
            shift_active(&mut self.active, t);
        }
        true
    }

    pub fn kill(&mut self) {
        for term in self.terms_mut() {
            term.kill();
        }
    }
}

pub struct Group {
    pub id: u64,
    pub entry: GroupEntry,
}

pub struct Project {
    pub id: u64,
    pub path: PathBuf,
    pub name: Option<String>,
    pub group: Option<u64>,
    pub workspaces: Vec<Workspace>,
    pub active: usize,
    pub closing: bool,
}

impl Project {
    pub fn new(id: u64, path: PathBuf, name: Option<String>) -> Self {
        Self { id, path, name, group: None, workspaces: Vec::new(), active: 0, closing: false }
    }

    pub fn workspace(&self) -> Option<&Workspace> {
        self.workspaces.get(self.active)
    }

    pub fn workspace_mut(&mut self) -> Option<&mut Workspace> {
        self.workspaces.get_mut(self.active)
    }

    pub fn terms_mut(&mut self) -> impl Iterator<Item = &mut Term> {
        self.workspaces.iter_mut().flat_map(Workspace::terms_mut)
    }

    pub fn has_terms(&self) -> bool {
        self.workspaces.iter().any(|w| w.terms().next().is_some())
    }

    pub fn remove_term(&mut self, id: u64) -> bool {
        let Some(w) = self.workspaces.iter_mut().position(|w| w.remove_term(id)) else { return false };
        if self.workspaces[w].closing && self.workspaces[w].tabs.is_empty() {
            self.remove_workspace(w);
        }
        true
    }

    pub fn remove_workspace(&mut self, w: usize) {
        self.workspaces.remove(w);
        shift_active(&mut self.active, w);
    }

    pub fn kill(&mut self) {
        for term in self.terms_mut() {
            term.kill();
        }
    }
}

pub fn shift_active(active: &mut usize, removed: usize) {
    if *active >= removed && *active > 0 {
        *active -= 1;
    }
}

pub fn move_before<T>(items: &mut Vec<T>, from: usize, before: usize, active: Option<&mut usize>) {
    let to = if before > from { before - 1 } else { before };
    if from >= items.len() || to >= items.len() || to == from {
        return;
    }
    let item = items.remove(from);
    items.insert(to, item);
    if let Some(active) = active {
        *active = match *active {
            a if a == from => to,
            a if from < a && a <= to => a - 1,
            a if to <= a && a < from => a + 1,
            a => a,
        };
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::before_the_active(3, 1, 2)]
    #[case::the_active_itself(2, 2, 1)]
    #[case::after_the_active(1, 2, 1)]
    #[case::the_first_when_active(0, 0, 0)]
    fn removing_keeps_the_active_one_in_place(#[case] active: usize, #[case] removed: usize, #[case] expected: usize) {
        let mut a = active;
        shift_active(&mut a, removed);
        assert_eq!(a, expected);
    }

    #[rstest]
    #[case::down(0, 3, "bcad")]
    #[case::up(3, 1, "adbc")]
    #[case::to_the_end(1, 4, "acdb")]
    #[case::to_the_start(2, 0, "cabd")]
    #[case::before_itself(1, 1, "abcd")]
    #[case::right_after_itself(1, 2, "abcd")]
    #[case::past_the_end(1, 9, "abcd")]
    fn moving_puts_the_item_before_another(#[case] from: usize, #[case] before: usize, #[case] expected: &str) {
        let mut items: Vec<char> = "abcd".chars().collect();
        move_before(&mut items, from, before, None);
        assert_eq!(items.into_iter().collect::<String>(), expected);
    }

    #[rstest]
    #[case::the_moved_one(1, 3, 1, 2)]
    #[case::one_it_passes_down(1, 3, 2, 1)]
    #[case::one_it_passes_up(3, 1, 2, 3)]
    #[case::one_it_does_not_pass(0, 2, 3, 3)]
    fn moving_keeps_the_active_one_active(
        #[case] from: usize,
        #[case] before: usize,
        #[case] active: usize,
        #[case] expected: usize,
    ) {
        let (mut items, mut a) = ((0..4).collect::<Vec<usize>>(), active);
        let id = items[active];
        move_before(&mut items, from, before, Some(&mut a));
        assert_eq!((a, items[a]), (expected, id));
    }
}
