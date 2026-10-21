#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Pane {
    pub(crate) id: u64,
    pub(crate) parent: Option<u64>,
    pub(crate) workspace: u64,
    pub(crate) tab: u64,
    pub(crate) agent: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Closing {
    pub(crate) tabs: Vec<u64>,
    pub(crate) inside: Vec<u64>,
    pub(crate) elsewhere: Vec<u64>,
}

pub(crate) fn closing(panes: &[Pane], agent: u64, whole_workspace: bool) -> Option<Closing> {
    let origin = panes.iter().find(|pane| pane.id == agent)?;
    let (mut inside, mut elsewhere) = (Vec::new(), Vec::new());
    for pane in panes.iter().filter(|pane| pane.id != agent && descends(panes, pane, agent)) {
        if pane.workspace == origin.workspace {
            inside.push(pane.id);
        } else {
            elsewhere.push(pane.id);
        }
    }
    let mut tabs = Vec::new();
    for pane in panes {
        let wanted = if whole_workspace {
            pane.workspace == origin.workspace
        } else {
            pane.id == agent || inside.contains(&pane.id)
        };
        if wanted && !tabs.contains(&pane.tab) {
            tabs.push(pane.tab);
        }
    }
    Some(Closing { tabs, inside, elsewhere })
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
