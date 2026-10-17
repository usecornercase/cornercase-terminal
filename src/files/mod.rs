pub(crate) mod disk;
pub(crate) mod link;
pub(crate) mod search;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use self::disk::{Content, Entry, Stamp};
use crate::changes::diff::{Diff, File as Changed, Kind, Status};

pub(crate) const LIST_EVERY: Duration = Duration::from_secs(2);
pub(crate) const CHECK_EVERY: Duration = Duration::from_secs(1);
pub(crate) const INDEX_EVERY: Duration = Duration::from_secs(10);
const GIVE_UP_AFTER: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Mode {
    #[default]
    Text,
    Name,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Query {
    pub(crate) text: String,
    pub(crate) focused: bool,
    pub(crate) selected: usize,
    pub(crate) scroll: usize,
    pub(crate) enter: bool,
}

impl Query {
    pub(crate) fn wanted(&self) -> Option<&str> {
        Some(self.text.trim()).filter(|q| !q.is_empty())
    }
}

#[derive(Debug)]
struct Answer<T> {
    query: String,
    index: Arc<Vec<String>>,
    found: Arc<T>,
}

impl<T> Answer<T> {
    fn answers(&self, query: &str, index: &Arc<Vec<String>>) -> bool {
        self.query == query && Arc::ptr_eq(&self.index, index)
    }
}

#[derive(Debug)]
pub(crate) struct Search {
    pub(crate) query: String,
    pub(crate) index: Arc<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Row {
    pub(crate) path: String,
    pub(crate) name: String,
    pub(crate) depth: u16,
    pub(crate) dir: bool,
    pub(crate) open: bool,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Viewer {
    pub(crate) path: String,
    pub(crate) scroll: usize,
    pub(crate) content: Option<Arc<Content>>,
    pub(crate) selection: Option<(u32, u32)>,
    pub(crate) unfolded: HashSet<u32>,
    pub(crate) find: Option<String>,
}

impl Viewer {
    pub(crate) fn selected(&self) -> Option<(u32, u32)> {
        self.selection.map(|(a, b)| (a.min(b), a.max(b)))
    }
}

#[derive(Debug, Default)]
struct Place {
    mode: Mode,
    query: Query,
    expanded: HashSet<String>,
    scroll: usize,
    viewer: Option<Viewer>,
    last: Option<String>,
}

#[derive(Debug, Default)]
pub(crate) struct Panel {
    places: HashMap<u64, Place>,
    listings: HashMap<(u64, String), Arc<Vec<Entry>>>,
    generation: u64,
    listing: Option<(u64, Instant)>,
    listed: HashMap<u64, Instant>,
    reading: Option<(u64, Instant)>,
    checked: Option<Instant>,
    index: HashMap<u64, Arc<Vec<String>>>,
    indexing: Option<(u64, Instant)>,
    indexed: HashMap<u64, Instant>,
    names: HashMap<u64, Answer<Vec<search::Name>>>,
    naming: Option<(u64, Instant)>,
    texts: HashMap<u64, Answer<search::Text>>,
    grepping: Option<(u64, String, Arc<AtomicBool>, Instant)>,
    full_screen: bool,
}

pub(crate) struct List {
    pub(crate) root: PathBuf,
    pub(crate) folders: Vec<String>,
}

pub(crate) struct Load {
    pub(crate) path: String,
    pub(crate) file: PathBuf,
    pub(crate) previous: Option<Stamp>,
}

fn answer<T>(
    places: &mut HashMap<u64, Place>,
    answers: &mut HashMap<u64, Answer<T>>,
    workspace: u64,
    search: Search,
    found: T,
) {
    let fresh = answers.get(&workspace).is_none_or(|a| a.query != search.query);
    if fresh && let Some(place) = places.get_mut(&workspace) {
        place.query.selected = 0;
        place.query.scroll = 0;
    }
    answers.insert(workspace, Answer { query: search.query, index: search.index, found: Arc::new(found) });
}

fn child(folder: &str, name: &str) -> String {
    if folder.is_empty() { name.to_string() } else { format!("{folder}/{name}") }
}

impl Panel {
    pub(crate) fn scroll(&self, workspace: u64) -> usize {
        self.places.get(&workspace).map_or(0, |p| p.scroll)
    }

    pub(crate) fn set_scroll(&mut self, workspace: u64, scroll: usize) {
        self.places.entry(workspace).or_default().scroll = scroll;
    }

    pub(crate) fn last(&self, workspace: u64) -> Option<&str> {
        self.places.get(&workspace)?.last.as_deref()
    }

    pub(crate) fn viewer(&self, workspace: u64) -> Option<&Viewer> {
        self.places.get(&workspace)?.viewer.as_ref()
    }

    pub(crate) fn viewer_mut(&mut self, workspace: u64) -> Option<&mut Viewer> {
        self.places.get_mut(&workspace)?.viewer.as_mut()
    }

    pub(crate) fn listed_root(&self, workspace: u64) -> bool {
        self.listings.contains_key(&(workspace, String::new()))
    }

    pub(crate) fn rows(&self, workspace: u64) -> Vec<Row> {
        let mut rows = Vec::new();
        let expanded = self.places.get(&workspace).map(|p| &p.expanded);
        self.walk(workspace, "", 0, expanded, &mut rows);
        rows
    }

    fn walk(&self, workspace: u64, folder: &str, depth: u16, expanded: Option<&HashSet<String>>, rows: &mut Vec<Row>) {
        let Some(entries) = self.listings.get(&(workspace, folder.to_string())) else { return };
        for entry in entries.iter() {
            let path = child(folder, &entry.name);
            let open = entry.dir && expanded.is_some_and(|e| e.contains(&path));
            rows.push(Row { path: path.clone(), name: entry.name.clone(), depth, dir: entry.dir, open });
            if open {
                self.walk(workspace, &path, depth + 1, expanded, rows);
            }
        }
    }

    pub(crate) fn mode(&self, workspace: u64) -> Mode {
        self.places.get(&workspace).map_or_else(Mode::default, |p| p.mode)
    }

    pub(crate) fn toggle_mode(&mut self, workspace: u64) {
        let place = self.places.entry(workspace).or_default();
        place.mode = if place.mode == Mode::Name { Mode::Text } else { Mode::Name };
        place.query = Query { text: std::mem::take(&mut place.query.text), focused: true, ..Query::default() };
    }

    pub(crate) fn search(&mut self, workspace: u64, mode: Mode) {
        self.close_file(workspace);
        let place = self.places.entry(workspace).or_default();
        if place.mode != mode {
            place.mode = mode;
            place.query = Query::default();
        }
        place.query.focused = true;
    }

    pub(crate) fn query(&self, workspace: u64) -> Option<&Query> {
        self.places.get(&workspace).map(|p| &p.query)
    }

    pub(crate) fn query_mut(&mut self, workspace: u64) -> &mut Query {
        &mut self.places.entry(workspace).or_default().query
    }

    pub(crate) fn searched(&self, workspace: u64) -> Option<&str> {
        self.query(workspace)?.wanted()
    }

    pub(crate) fn typing(&self, workspace: u64) -> bool {
        self.viewer(workspace).is_none() && self.query(workspace).is_some_and(|q| q.focused)
    }

    pub(crate) fn full_screen(&self) -> bool {
        self.full_screen
    }

    pub(crate) fn set_full_screen(&mut self, on: bool) {
        self.full_screen = on;
    }

    pub(crate) fn close(&mut self) {
        self.full_screen = false;
        self.unfocus();
    }

    pub(crate) fn unfocus(&mut self) {
        for place in self.places.values_mut() {
            place.query.focused = false;
            place.query.enter = false;
        }
    }

    pub(crate) fn searching(&self, workspace: u64) -> bool {
        let running = match self.mode(workspace) {
            Mode::Name => self.naming.is_some(),
            Mode::Text => self.grepping.is_some(),
        };
        running || !self.index.contains_key(&workspace)
    }

    pub(crate) fn answered(&self, workspace: u64) -> Option<&str> {
        match self.mode(workspace) {
            Mode::Name => self.names.get(&workspace).map(|a| a.query.as_str()),
            Mode::Text => self.texts.get(&workspace).map(|a| a.query.as_str()),
        }
    }

    pub(crate) fn found_names(&self, workspace: u64) -> Option<(&str, &[search::Name])> {
        self.names.get(&workspace).map(|a| (a.query.as_str(), a.found.as_slice()))
    }

    pub(crate) fn found_text(&self, workspace: u64) -> Option<(&str, &search::Text)> {
        self.texts.get(&workspace).map(|a| (a.query.as_str(), a.found.as_ref()))
    }

    pub(crate) fn index_request(&mut self, workspace: u64, root: &Path, now: Instant) -> Option<(u64, PathBuf)> {
        let wanted = self.query(workspace).is_some_and(|q| q.focused || q.wanted().is_some());
        if !wanted || self.indexing.is_some_and(|(_, at)| now.duration_since(at) < GIVE_UP_AFTER) {
            return None;
        }
        let fresh = self.indexed.get(&workspace).is_some_and(|at| now.duration_since(*at) < INDEX_EVERY);
        if self.index.contains_key(&workspace) && fresh {
            return None;
        }
        self.generation += 1;
        self.indexing = Some((self.generation, now));
        self.indexed.insert(workspace, now);
        Some((self.generation, root.to_path_buf()))
    }

    pub(crate) fn indexed(&mut self, workspace: u64, generation: u64, paths: Vec<String>) {
        if self.indexing.is_some_and(|(g, _)| g == generation) {
            self.indexing = None;
        }
        if self.index.get(&workspace).is_none_or(|known| **known != paths) {
            self.index.insert(workspace, Arc::new(paths));
        }
    }

    pub(crate) fn names_request(&mut self, workspace: u64, now: Instant) -> Option<(u64, Search)> {
        if self.mode(workspace) != Mode::Name
            || self.naming.is_some_and(|(_, at)| now.duration_since(at) < GIVE_UP_AFTER)
        {
            return None;
        }
        let query = self.searched(workspace)?.to_string();
        let index = Arc::clone(self.index.get(&workspace)?);
        if self.names.get(&workspace).is_some_and(|a| a.answers(&query, &index)) {
            return None;
        }
        self.generation += 1;
        self.naming = Some((self.generation, now));
        Some((self.generation, Search { query, index }))
    }

    pub(crate) fn named(&mut self, workspace: u64, generation: u64, search: Search, found: Vec<search::Name>) {
        if self.naming.is_none_or(|(g, _)| g != generation) {
            return;
        }
        self.naming = None;
        answer(&mut self.places, &mut self.names, workspace, search, found);
    }

    pub(crate) fn grep_request(&mut self, workspace: u64, now: Instant) -> Option<(u64, Search, Arc<AtomicBool>)> {
        if self.mode(workspace) != Mode::Text {
            return None;
        }
        let query = self.searched(workspace)?.to_string();
        let index = Arc::clone(self.index.get(&workspace)?);
        if self.texts.get(&workspace).is_some_and(|a| a.answers(&query, &index)) {
            return None;
        }
        if let Some((_, running, cancel, at)) = &self.grepping {
            if *running == query && now.duration_since(*at) < GIVE_UP_AFTER {
                return None;
            }
            cancel.store(true, Ordering::Relaxed);
        }
        let cancel = Arc::new(AtomicBool::new(false));
        self.generation += 1;
        self.grepping = Some((self.generation, query.clone(), Arc::clone(&cancel), now));
        Some((self.generation, Search { query, index }, cancel))
    }

    pub(crate) fn grepped(&mut self, workspace: u64, generation: u64, search: Search, found: search::Text) {
        if self.grepping.as_ref().is_none_or(|(g, ..)| *g != generation) {
            return;
        }
        self.grepping = None;
        answer(&mut self.places, &mut self.texts, workspace, search, found);
    }

    pub(crate) fn open_at(&mut self, workspace: u64, path: &str, lines: (u32, u32), find: Option<String>) {
        self.open_file(workspace, path);
        if let Some(viewer) = self.viewer_mut(workspace) {
            viewer.selection = Some(lines);
            viewer.scroll = usize::try_from(lines.0).unwrap_or(usize::MAX).saturating_sub(4);
            viewer.find = find;
        }
    }

    pub(crate) fn toggle(&mut self, workspace: u64, folder: &str) {
        let expanded = &mut self.places.entry(workspace).or_default().expanded;
        if !expanded.remove(folder) {
            expanded.insert(folder.to_string());
        }
    }

    pub(crate) fn open_file(&mut self, workspace: u64, path: &str) {
        self.full_screen = false;
        let place = self.places.entry(workspace).or_default();
        place.last = Some(path.to_string());
        place.viewer = Some(Viewer { path: path.to_string(), ..Viewer::default() });
        self.reading = None;
    }

    pub(crate) fn close_file(&mut self, workspace: u64) {
        self.full_screen = false;
        if let Some(place) = self.places.get_mut(&workspace) {
            place.viewer = None;
        }
        self.reading = None;
    }

    pub(crate) fn list(&mut self, workspace: u64, root: &Path, now: Instant) -> Option<(u64, List)> {
        if self.listing.is_some_and(|(_, at)| now.duration_since(at) < GIVE_UP_AFTER) {
            return None;
        }
        let place = self.places.entry(workspace).or_default();
        let mut folders: Vec<String> = std::iter::once(String::new()).chain(place.expanded.iter().cloned()).collect();
        folders.sort();
        let missing = folders.iter().any(|f| !self.listings.contains_key(&(workspace, f.clone())));
        let stale = self.listed.get(&workspace).is_none_or(|at| now.duration_since(*at) >= LIST_EVERY);
        if !missing && !stale {
            return None;
        }
        self.generation += 1;
        self.listing = Some((self.generation, now));
        self.listed.insert(workspace, now);
        Some((self.generation, List { root: root.to_path_buf(), folders }))
    }

    pub(crate) fn listed(&mut self, workspace: u64, generation: u64, folders: Vec<(String, Option<Vec<Entry>>)>) {
        if self.listing.is_some_and(|(g, _)| g == generation) {
            self.listing = None;
        }
        for (folder, entries) in folders {
            let key = (workspace, folder);
            match entries {
                Some(entries) if self.listings.get(&key).is_some_and(|known| **known == entries) => {}
                Some(entries) => _ = self.listings.insert(key, Arc::new(entries)),
                None if key.1.is_empty() => _ = self.listings.insert(key, Arc::default()),
                None => {
                    if let Some(place) = self.places.get_mut(&workspace) {
                        place.expanded.remove(&key.1);
                    }
                    self.listings.remove(&key);
                }
            }
        }
    }

    pub(crate) fn load(&mut self, workspace: u64, root: &Path, now: Instant) -> Option<(u64, Load)> {
        let viewer = self.places.get(&workspace)?.viewer.as_ref()?;
        if self.reading.is_some_and(|(_, at)| now.duration_since(at) < GIVE_UP_AFTER) {
            return None;
        }
        if viewer.content.is_some() && self.checked.is_some_and(|at| now.duration_since(at) < CHECK_EVERY) {
            return None;
        }
        let load = Load {
            path: viewer.path.clone(),
            file: root.join(&viewer.path),
            previous: viewer.content.as_ref().map(|c| c.stamp),
        };
        self.generation += 1;
        self.reading = Some((self.generation, now));
        self.checked = Some(now);
        Some((self.generation, load))
    }

    pub(crate) fn read(&mut self, workspace: u64, generation: u64, path: &str, content: Option<Content>, done: bool) {
        if self.reading.is_none_or(|(g, _)| g != generation) {
            return;
        }
        if done {
            self.reading = None;
        }
        let viewer = self.places.get_mut(&workspace).and_then(|p| p.viewer.as_mut()).filter(|v| v.path == path);
        if let (Some(viewer), Some(content)) = (viewer, content) {
            viewer.content = Some(Arc::new(content));
        }
    }

    pub(crate) fn forget(&mut self, alive: &HashSet<u64>) {
        self.places.retain(|id, _| alive.contains(id));
        self.listings.retain(|(id, _), _| alive.contains(id));
        self.listed.retain(|id, _| alive.contains(id));
        self.index.retain(|id, _| alive.contains(id));
        self.indexed.retain(|id, _| alive.contains(id));
        self.names.retain(|id, _| alive.contains(id));
        self.texts.retain(|id, _| alive.contains(id));
    }
}

pub(crate) fn statuses(diff: &Diff) -> HashMap<String, Status> {
    let mut marks = HashMap::new();
    for file in &diff.files {
        marks.insert(file.path.clone(), file.status);
        let added = matches!(file.status, Status::Added | Status::Untracked);
        let mut folder = file.path.as_str();
        while let Some((parent, _)) = folder.rsplit_once('/') {
            let mark = marks.entry(parent.to_string()).or_insert(Status::Added);
            if !added {
                *mark = Status::Modified;
            }
            folder = parent;
        }
    }
    marks
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mark {
    Added,
    Modified,
    Deleted,
    DeletedAbove,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Gutter {
    marks: Vec<Option<(Mark, u32)>>,
    pub(crate) removed: BTreeMap<u32, Vec<String>>,
}

impl Gutter {
    fn index(line: u32) -> usize {
        usize::try_from(line).unwrap_or(usize::MAX).wrapping_sub(1)
    }

    pub(crate) fn mark(&self, line: u32) -> Option<Mark> {
        self.marks.get(Self::index(line)).copied().flatten().map(|(mark, _)| mark)
    }

    pub(crate) fn block(&self, line: u32) -> Option<u32> {
        let (_, key) = self.marks.get(Self::index(line)).copied().flatten()?;
        self.removed.contains_key(&key).then_some(key)
    }

    fn set(&mut self, line: u32, mark: Mark, key: u32) {
        if let Some(slot) = self.marks.get_mut(Self::index(line)) {
            *slot = Some((mark, key));
        }
    }
}

pub(crate) fn gutter(file: &Changed, lines: usize) -> Gutter {
    let mut gutter = Gutter { marks: vec![None; lines], removed: BTreeMap::new() };
    if matches!(file.status, Status::Added | Status::Untracked) {
        gutter.marks.iter_mut().zip(1..).for_each(|(m, n)| *m = Some((Mark::Added, n)));
        return gutter;
    }
    for hunk in &file.hunks {
        let mut next = hunk.new_start.max(1);
        let mut i = 0;
        while let Some(line) = hunk.lines.get(i) {
            if line.kind == Kind::Context {
                next = line.new.map_or(next, |n| n + 1);
                i += 1;
                continue;
            }
            let mut gone = Vec::new();
            while let Some(l) = hunk.lines.get(i).filter(|l| l.kind == Kind::Removed) {
                gone.push(l.text.clone());
                i += 1;
            }
            let mut added = Vec::new();
            while let Some(l) = hunk.lines.get(i).filter(|l| l.kind == Kind::Added) {
                added.extend(l.new);
                i += 1;
            }
            let key = added.first().copied().unwrap_or(next);
            match (gone.is_empty(), added.is_empty()) {
                (true, _) => added.iter().for_each(|&n| gutter.set(n, Mark::Added, key)),
                (false, false) => added.iter().for_each(|&n| gutter.set(n, Mark::Modified, key)),
                (false, true) if next > 1 => gutter.set(next - 1, Mark::Deleted, key),
                (false, true) => gutter.set(1, Mark::DeletedAbove, key),
            }
            if !gone.is_empty() {
                gutter.removed.insert(key, gone);
            }
            next = added.last().map_or(next, |n| n + 1);
        }
    }
    gutter
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Line {
    Code(u32),
    Removed(u32, usize),
}

pub(crate) struct Lines<'a> {
    count: usize,
    blocks: Vec<(u32, &'a [String])>,
}

impl<'a> Lines<'a> {
    pub(crate) fn new(count: usize, gutter: Option<&'a Gutter>, unfolded: &HashSet<u32>) -> Self {
        let blocks = gutter
            .map(|g| g.removed.iter().filter(|(key, _)| unfolded.contains(key)).map(|(k, v)| (*k, v.as_slice())))
            .into_iter()
            .flatten()
            .collect();
        Self { count, blocks }
    }

    pub(crate) fn len(&self) -> usize {
        self.count + self.blocks.iter().map(|(_, lines)| lines.len()).sum::<usize>()
    }

    pub(crate) fn at(&self, row: usize) -> Option<Line> {
        let mut extra = 0;
        for (key, lines) in &self.blocks {
            let start = usize::try_from(*key).unwrap_or(usize::MAX).saturating_sub(1) + extra;
            if row < start {
                break;
            }
            if row < start + lines.len() {
                return Some(Line::Removed(*key, row - start));
            }
            extra += lines.len();
        }
        let line = row - extra;
        (line < self.count).then(|| Line::Code(u32::try_from(line + 1).unwrap_or(u32::MAX)))
    }

    pub(crate) fn removed(&self, key: u32, k: usize) -> Option<&str> {
        self.blocks.iter().find(|(b, _)| *b == key).and_then(|(_, lines)| lines.get(k)).map(String::as_str)
    }

    #[cfg(test)]
    pub(crate) fn row_of(&self, line: u32) -> usize {
        let before: usize = self.blocks.iter().filter(|(key, _)| *key <= line).map(|(_, l)| l.len()).sum();
        usize::try_from(line).unwrap_or(usize::MAX).saturating_sub(1) + before
    }
}

#[cfg(test)]
mod tests;
