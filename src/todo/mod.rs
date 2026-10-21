pub(crate) mod editor;

use std::collections::VecDeque;
use std::path::Path;

use serde::{Deserialize, Serialize};

use editor::Editor;

pub(crate) const VERSION: u32 = 1;
pub(crate) const UNDOS: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Item {
    pub(crate) id: u64,
    pub(crate) text: String,
    pub(crate) done: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Removed(Vec<(usize, Item)>);

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Todos {
    items: Vec<Item>,
    next_id: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Saved {
    pub(crate) version: u32,
    #[serde(default)]
    pub(crate) items: Vec<SavedItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SavedItem {
    pub(crate) text: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub(crate) done: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Panel {
    scroll: usize,
    field: Option<Field>,
    next_client: u64,
    sending: std::collections::HashMap<u64, String>,
    undos: Undos,
}

impl Panel {
    pub(crate) fn scroll(&self) -> usize {
        self.scroll
    }

    pub(crate) fn set_scroll(&mut self, scroll: usize) {
        self.scroll = scroll;
    }

    pub(crate) fn field(&self) -> Option<&Field> {
        self.field.as_ref()
    }

    pub(crate) fn editor_mut(&mut self) -> Option<&mut Editor> {
        self.field.as_mut().map(|field| &mut field.editor)
    }

    pub(crate) fn start_adding(&mut self) {
        self.field = Some(Field::default());
    }

    pub(crate) fn start_editing(&mut self, item: u64, editor: Editor) {
        self.field = Some(Field { item: Some(item), editor });
    }

    pub(crate) fn take_field(&mut self) -> Option<Field> {
        self.field.take()
    }

    pub(crate) fn cancel(&mut self) {
        self.field = None;
    }

    pub(crate) fn new_client(&mut self) -> u64 {
        let client = self.next_client;
        self.next_client += 1;
        client
    }

    pub(crate) fn send(&mut self, client: u64, place: String) {
        self.sending.insert(client, place);
    }

    pub(crate) fn answered(&mut self, client: u64) -> Option<String> {
        self.sending.remove(&client)
    }

    pub(crate) fn keep_removed(&mut self, removed: Removed) -> u64 {
        self.undos.keep(removed)
    }

    pub(crate) fn take_removed(&mut self, undo: u64) -> Option<Removed> {
        self.undos.take(undo)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Undos {
    next: u64,
    kept: VecDeque<(u64, Removed)>,
}

impl Undos {
    pub(crate) fn keep(&mut self, removed: Removed) -> u64 {
        self.next += 1;
        self.kept.push_back((self.next, removed));
        if self.kept.len() > UNDOS {
            self.kept.pop_front();
        }
        self.next
    }

    pub(crate) fn take(&mut self, undo: u64) -> Option<Removed> {
        let at = self.kept.iter().position(|(n, _)| *n == undo)?;
        self.kept.remove(at).map(|(_, removed)| removed)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Field {
    pub(crate) item: Option<u64>,
    pub(crate) editor: Editor,
}

pub(crate) fn clean(text: &str) -> Option<String> {
    let text: String = text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

pub(crate) fn load(path: &Path) -> Todos {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<Saved>(&text).ok())
        .filter(|saved| saved.version == VERSION)
        .map(Todos::from)
        .unwrap_or_default()
}

impl Todos {
    pub(crate) fn items(&self) -> &[Item] {
        &self.items
    }

    pub(crate) fn pending(&self) -> usize {
        self.items.iter().take_while(|i| !i.done).count()
    }

    fn find(&self, id: u64) -> Option<usize> {
        self.items.iter().position(|i| i.id == id)
    }

    pub(crate) fn item(&self, id: u64) -> Option<&Item> {
        self.items.iter().find(|i| i.id == id)
    }

    fn take_id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    pub(crate) fn add(&mut self, text: &str) -> Option<u64> {
        let text = clean(text)?;
        let id = self.take_id();
        let at = self.pending();
        self.items.insert(at, Item { id, text, done: false });
        Some(id)
    }

    pub(crate) fn edit(&mut self, id: u64, text: &str) -> bool {
        let (Some(text), Some(i)) = (clean(text), self.find(id)) else { return false };
        self.items[i].text = text;
        true
    }

    pub(crate) fn toggle(&mut self, id: u64) {
        let Some(i) = self.find(id) else { return };
        let mut item = self.items.remove(i);
        item.done = !item.done;
        let at = if item.done { self.items.len() } else { self.pending() };
        self.items.insert(at, item);
    }

    pub(crate) fn move_before(&mut self, id: u64, before: usize) {
        let Some(from) = self.find(id) else { return };
        let split = self.pending();
        let (low, high) = if self.items[from].done { (split, self.items.len()) } else { (0, split) };
        crate::project::move_before(&mut self.items, from, before.clamp(low, high), None);
    }

    pub(crate) fn move_to(&mut self, id: u64, before: Option<u64>) {
        let Some(item) = self.item(id) else { return };
        let done = item.done;
        let at = match before.and_then(|b| self.find(b)) {
            Some(at) => at,
            None if done => self.items.len(),
            None => self.pending(),
        };
        self.move_before(id, at);
    }

    pub(crate) fn remove(&mut self, id: u64) -> Option<Removed> {
        let i = self.find(id)?;
        Some(Removed(vec![(i, self.items.remove(i))]))
    }

    pub(crate) fn clear_done(&mut self) -> Option<Removed> {
        let split = self.pending();
        let done: Vec<(usize, Item)> =
            self.items.drain(split..).enumerate().map(|(i, item)| (split + i, item)).collect();
        (!done.is_empty()).then_some(Removed(done))
    }

    pub(crate) fn restore(&mut self, removed: Removed) {
        for (at, item) in removed.0 {
            if self.find(item.id).is_some() {
                continue;
            }
            let split = self.pending();
            let at = if item.done { at.clamp(split, self.items.len()) } else { at.min(split) };
            self.items.insert(at, item);
        }
    }

    pub(crate) fn saved(&self) -> Saved {
        let items = self.items.iter().map(|i| SavedItem { text: i.text.clone(), done: i.done }).collect();
        Saved { version: VERSION, items }
    }
}

impl Removed {
    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }

    pub(crate) fn items(&self) -> impl Iterator<Item = &Item> {
        self.0.iter().map(|(_, item)| item)
    }
}

impl From<Saved> for Todos {
    fn from(saved: Saved) -> Self {
        let mut todos = Self::default();
        for item in saved.items {
            if let Some(text) = clean(&item.text) {
                let id = todos.take_id();
                todos.items.push(Item { id, text, done: item.done });
            }
        }
        todos.items.sort_by_key(|i| i.done);
        todos
    }
}

#[cfg(test)]
mod tests;
