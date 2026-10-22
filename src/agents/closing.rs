#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Pane {
    pub(crate) id: u64,
    pub(crate) parent: Option<u64>,
    pub(crate) workspace: u64,
    pub(crate) tab: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Closing {
    pub(crate) tabs: Vec<u64>,
    pub(crate) inside: Vec<u64>,
    pub(crate) elsewhere: Vec<u64>,
}

pub(crate) fn closing(panes: &[Pane], agent: u64, team: &[u64]) -> Option<Closing> {
    let origin = panes.iter().find(|pane| pane.id == agent)?;
    let alone = gather(panes, origin.workspace, &[agent]);
    let reached = panes.iter().any(|pane| team.contains(&pane.id) && alone.tabs.contains(&pane.tab));
    if !reached {
        return Some(alone);
    }
    let roots: Vec<u64> =
        panes.iter().filter(|pane| pane.id == agent || team.contains(&pane.id)).map(|pane| pane.id).collect();
    Some(gather(panes, origin.workspace, &roots))
}

fn gather(panes: &[Pane], workspace: u64, roots: &[u64]) -> Closing {
    let (mut inside, mut elsewhere) = (Vec::new(), Vec::new());
    let below = |pane: &&Pane| !roots.contains(&pane.id) && roots.iter().any(|&root| descends(panes, pane, root));
    for pane in panes.iter().filter(below) {
        if pane.workspace == workspace {
            inside.push(pane.id);
        } else {
            elsewhere.push(pane.id);
        }
    }
    let mut tabs = Vec::new();
    for pane in panes.iter().filter(|pane| roots.contains(&pane.id) || inside.contains(&pane.id)) {
        if !tabs.contains(&pane.tab) {
            tabs.push(pane.tab);
        }
    }
    Closing { tabs, inside, elsewhere }
}

fn descends(panes: &[Pane], pane: &Pane, ancestor: u64) -> bool {
    let mut seen = vec![pane.id];
    let mut parent = pane.parent;
    while let Some(id) = parent {
        if id == ancestor {
            return true;
        }
        if seen.len() > panes.len() || seen.contains(&id) {
            return false;
        }
        seen.push(id);
        parent = panes.iter().find(|other| other.id == id).and_then(|other| other.parent);
    }
    false
}

#[cfg(test)]
mod tests;
