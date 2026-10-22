use super::{Action, Dialog};
use crate::flows::store;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Dragging {
    pub(crate) from: usize,
    pub(crate) landing: Option<usize>,
    pub(crate) moved: bool,
    open: bool,
}

impl Dialog {
    pub(crate) fn grabbable(&self, i: usize) -> bool {
        self.edit.is_none() && self.importing.is_none() && self.confirm.is_none() && i < self.flows.len()
    }

    pub(crate) fn grab(&mut self, i: usize) {
        self.notice = None;
        let open = self.cursor == i;
        self.select(i);
        self.drag = Some(Dragging { from: i, landing: None, moved: false, open });
    }

    pub(crate) fn drag_over(&mut self, at: Option<usize>) {
        let len = self.flows.len();
        let Some(drag) = &mut self.drag else { return };
        match at {
            None => {
                drag.moved = true;
                drag.landing = None;
            }
            Some(row) if row == drag.from => drag.landing = None,
            Some(row) => {
                drag.moved = true;
                let landing = if row >= len {
                    len
                } else if row > drag.from {
                    row + 1
                } else {
                    row
                };
                drag.landing = (landing != drag.from && landing != drag.from + 1).then_some(landing);
            }
        }
    }

    pub(crate) fn release(&mut self) -> Action {
        let Some(drag) = self.drag.take() else { return Action::None };
        match drag.landing {
            Some(landing) if drag.moved => Action::Order(self.reordered(drag.from, landing)),
            _ if !drag.moved && drag.open => self.activate(),
            _ => Action::None,
        }
    }

    pub(crate) fn cancel_drag(&mut self) -> bool {
        self.drag.take().is_some()
    }

    pub(crate) fn drag_hint(&self) -> Option<String> {
        let drag = self.drag.filter(|drag| drag.moved)?;
        let name = self.flows.get(drag.from)?.flow.name.trim();
        Some(format!("release to move {name} there · esc puts it back"))
    }

    fn reordered(&self, from: usize, landing: usize) -> Vec<String> {
        let mut names: Vec<String> = self.flows.iter().map(|saved| saved.flow.name.trim().to_string()).collect();
        let moved = names.remove(from);
        names.insert(if landing > from { landing - 1 } else { landing }, moved);
        store::dedup_names(names)
    }
}
