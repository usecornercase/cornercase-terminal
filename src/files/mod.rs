pub mod disk;
pub mod link;
pub mod search;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use self::disk::{Content, Entry, Stamp};
use crate::changes::diff::{Diff, File as Changed, Kind, Status};

pub const LIST_EVERY: Duration = Duration::from_secs(2);
pub const CHECK_EVERY: Duration = Duration::from_secs(1);
pub const INDEX_EVERY: Duration = Duration::from_secs(10);
const GIVE_UP_AFTER: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Text,
    Name,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Query {
    pub text: String,
    pub focused: bool,
    pub selected: usize,
    pub scroll: usize,
    pub enter: bool,
}

impl Query {
    pub fn wanted(&self) -> Option<&str> {
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
pub struct Search {
    pub query: String,
    pub index: Arc<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub path: String,
    pub name: String,
    pub depth: u16,
    pub dir: bool,
    pub open: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Viewer {
    pub path: String,
    pub scroll: usize,
    pub content: Option<Arc<Content>>,
    pub selection: Option<(u32, u32)>,
    pub unfolded: HashSet<u32>,
    pub find: Option<String>,
}

impl Viewer {
    pub fn selected(&self) -> Option<(u32, u32)> {
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
pub struct Panel {
    pub open: bool,
    pub selecting: Option<u32>,
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
}

pub struct List {
    pub root: PathBuf,
    pub folders: Vec<String>,
}

pub struct Load {
    pub path: String,
    pub file: PathBuf,
    pub previous: Option<Stamp>,
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
    pub fn scroll(&self, workspace: u64) -> usize {
        self.places.get(&workspace).map_or(0, |p| p.scroll)
    }

    pub fn set_scroll(&mut self, workspace: u64, scroll: usize) {
        self.places.entry(workspace).or_default().scroll = scroll;
    }

    pub fn last(&self, workspace: u64) -> Option<&str> {
        self.places.get(&workspace)?.last.as_deref()
    }

    pub fn viewer(&self, workspace: u64) -> Option<&Viewer> {
        self.places.get(&workspace)?.viewer.as_ref()
    }

    pub fn viewer_mut(&mut self, workspace: u64) -> Option<&mut Viewer> {
        self.places.get_mut(&workspace)?.viewer.as_mut()
    }

    pub fn listed_root(&self, workspace: u64) -> bool {
        self.listings.contains_key(&(workspace, String::new()))
    }

    pub fn rows(&self, workspace: u64) -> Vec<Row> {
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

    pub fn mode(&self, workspace: u64) -> Mode {
        self.places.get(&workspace).map_or_else(Mode::default, |p| p.mode)
    }

    pub fn toggle_mode(&mut self, workspace: u64) {
        let place = self.places.entry(workspace).or_default();
        place.mode = if place.mode == Mode::Name { Mode::Text } else { Mode::Name };
        place.query = Query { text: std::mem::take(&mut place.query.text), focused: true, ..Query::default() };
    }

    pub fn query(&self, workspace: u64) -> Option<&Query> {
        self.places.get(&workspace).map(|p| &p.query)
    }

    pub fn query_mut(&mut self, workspace: u64) -> &mut Query {
        &mut self.places.entry(workspace).or_default().query
    }

    pub fn searched(&self, workspace: u64) -> Option<&str> {
        self.query(workspace)?.wanted()
    }

    pub fn typing(&self, workspace: u64) -> bool {
        self.viewer(workspace).is_none() && self.query(workspace).is_some_and(|q| q.focused)
    }

    pub fn close(&mut self) {
        self.open = false;
        self.selecting = None;
        self.unfocus();
    }

    pub fn unfocus(&mut self) {
        for place in self.places.values_mut() {
            place.query.focused = false;
            place.query.enter = false;
        }
    }

    pub fn indexed_files(&self, workspace: u64) -> Option<usize> {
        self.index.get(&workspace).map(|paths| paths.len())
    }

    pub fn searching(&self, workspace: u64) -> bool {
        let running = match self.mode(workspace) {
            Mode::Name => self.naming.is_some(),
            Mode::Text => self.grepping.is_some(),
        };
        running || !self.index.contains_key(&workspace)
    }

    pub fn answered(&self, workspace: u64) -> Option<&str> {
        match self.mode(workspace) {
            Mode::Name => self.names.get(&workspace).map(|a| a.query.as_str()),
            Mode::Text => self.texts.get(&workspace).map(|a| a.query.as_str()),
        }
    }

    pub fn found_names(&self, workspace: u64) -> Option<(&str, &[search::Name])> {
        self.names.get(&workspace).map(|a| (a.query.as_str(), a.found.as_slice()))
    }

    pub fn found_text(&self, workspace: u64) -> Option<(&str, &search::Text)> {
        self.texts.get(&workspace).map(|a| (a.query.as_str(), a.found.as_ref()))
    }

    pub fn index_request(&mut self, workspace: u64, root: &Path, now: Instant) -> Option<(u64, PathBuf)> {
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

    pub fn indexed(&mut self, workspace: u64, generation: u64, paths: Vec<String>) {
        if self.indexing.is_some_and(|(g, _)| g == generation) {
            self.indexing = None;
        }
        if self.index.get(&workspace).is_none_or(|known| **known != paths) {
            self.index.insert(workspace, Arc::new(paths));
        }
    }

    pub fn names_request(&mut self, workspace: u64, now: Instant) -> Option<(u64, Search)> {
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

    pub fn named(&mut self, workspace: u64, generation: u64, search: Search, found: Vec<search::Name>) {
        if self.naming.is_none_or(|(g, _)| g != generation) {
            return;
        }
        self.naming = None;
        answer(&mut self.places, &mut self.names, workspace, search, found);
    }

    pub fn grep_request(&mut self, workspace: u64, now: Instant) -> Option<(u64, Search, Arc<AtomicBool>)> {
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

    pub fn grepped(&mut self, workspace: u64, generation: u64, search: Search, found: search::Text) {
        if self.grepping.as_ref().is_none_or(|(g, ..)| *g != generation) {
            return;
        }
        self.grepping = None;
        answer(&mut self.places, &mut self.texts, workspace, search, found);
    }

    pub fn open_at(&mut self, workspace: u64, path: &str, lines: (u32, u32), find: Option<String>) {
        self.open_file(workspace, path);
        if let Some(viewer) = self.viewer_mut(workspace) {
            viewer.selection = Some(lines);
            viewer.scroll = usize::try_from(lines.0).unwrap_or(usize::MAX).saturating_sub(4);
            viewer.find = find;
        }
    }

    pub fn toggle(&mut self, workspace: u64, folder: &str) {
        let expanded = &mut self.places.entry(workspace).or_default().expanded;
        if !expanded.remove(folder) {
            expanded.insert(folder.to_string());
        }
    }

    pub fn open_file(&mut self, workspace: u64, path: &str) {
        let place = self.places.entry(workspace).or_default();
        place.last = Some(path.to_string());
        place.viewer = Some(Viewer { path: path.to_string(), ..Viewer::default() });
        self.reading = None;
        self.selecting = None;
    }

    pub fn close_file(&mut self, workspace: u64) {
        if let Some(place) = self.places.get_mut(&workspace) {
            place.viewer = None;
        }
        self.reading = None;
        self.selecting = None;
    }

    pub fn list(&mut self, workspace: u64, root: &Path, now: Instant) -> Option<(u64, List)> {
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

    pub fn listed(&mut self, workspace: u64, generation: u64, folders: Vec<(String, Option<Vec<Entry>>)>) {
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

    pub fn load(&mut self, workspace: u64, root: &Path, now: Instant) -> Option<(u64, Load)> {
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

    pub fn read(&mut self, workspace: u64, generation: u64, path: &str, content: Option<Content>, done: bool) {
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

    pub fn forget(&mut self, alive: &HashSet<u64>) {
        self.places.retain(|id, _| alive.contains(id));
        self.listings.retain(|(id, _), _| alive.contains(id));
        self.listed.retain(|id, _| alive.contains(id));
        self.index.retain(|id, _| alive.contains(id));
        self.indexed.retain(|id, _| alive.contains(id));
        self.names.retain(|id, _| alive.contains(id));
        self.texts.retain(|id, _| alive.contains(id));
    }
}

pub fn statuses(diff: &Diff) -> HashMap<String, Status> {
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
pub enum Mark {
    Added,
    Modified,
    Deleted,
    DeletedAbove,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Gutter {
    marks: Vec<Option<(Mark, u32)>>,
    pub removed: BTreeMap<u32, Vec<String>>,
}

impl Gutter {
    fn index(line: u32) -> usize {
        usize::try_from(line).unwrap_or(usize::MAX).wrapping_sub(1)
    }

    pub fn mark(&self, line: u32) -> Option<Mark> {
        self.marks.get(Self::index(line)).copied().flatten().map(|(mark, _)| mark)
    }

    pub fn block(&self, line: u32) -> Option<u32> {
        let (_, key) = self.marks.get(Self::index(line)).copied().flatten()?;
        self.removed.contains_key(&key).then_some(key)
    }

    fn set(&mut self, line: u32, mark: Mark, key: u32) {
        if let Some(slot) = self.marks.get_mut(Self::index(line)) {
            *slot = Some((mark, key));
        }
    }
}

pub fn gutter(file: &Changed, lines: usize) -> Gutter {
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
pub enum Line {
    Code(u32),
    Removed(u32, usize),
}

pub struct Lines<'a> {
    count: usize,
    blocks: Vec<(u32, &'a [String])>,
}

impl<'a> Lines<'a> {
    pub fn new(count: usize, gutter: Option<&'a Gutter>, unfolded: &HashSet<u32>) -> Self {
        let blocks = gutter
            .map(|g| g.removed.iter().filter(|(key, _)| unfolded.contains(key)).map(|(k, v)| (*k, v.as_slice())))
            .into_iter()
            .flatten()
            .collect();
        Self { count, blocks }
    }

    pub fn len(&self) -> usize {
        self.count + self.blocks.iter().map(|(_, lines)| lines.len()).sum::<usize>()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn at(&self, row: usize) -> Option<Line> {
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

    pub fn removed(&self, key: u32, k: usize) -> Option<&str> {
        self.blocks.iter().find(|(b, _)| *b == key).and_then(|(_, lines)| lines.get(k)).map(String::as_str)
    }

    pub fn row_of(&self, line: u32) -> usize {
        let before: usize = self.blocks.iter().filter(|(key, _)| *key <= line).map(|(_, l)| l.len()).sum();
        usize::try_from(line).unwrap_or(usize::MAX).saturating_sub(1) + before
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::changes::diff;

    fn entries(names: &[(&str, bool)]) -> Vec<Entry> {
        names.iter().map(|(name, dir)| Entry { name: (*name).to_string(), dir: *dir }).collect()
    }

    fn panel_with_listings() -> Panel {
        let mut panel = Panel::default();
        let now = Instant::now();
        let (generation, _) = panel.list(1, Path::new("/repo"), now).expect("a first listing");
        panel.listed(1, generation, vec![(String::new(), Some(entries(&[("src", true), ("README.md", false)])))]);
        panel.listed(1, 0, vec![("src".into(), Some(entries(&[("ui", true), ("main.rs", false)])))]);
        panel
    }

    fn paths(rows: &[Row]) -> Vec<(&str, u16)> {
        rows.iter().map(|r| (r.path.as_str(), r.depth)).collect()
    }

    mod tree {
        use super::*;

        #[test]
        fn shows_the_root_until_a_folder_opens() {
            let mut panel = panel_with_listings();
            assert_eq!(paths(&panel.rows(1)), [("src", 0), ("README.md", 0)]);
            panel.toggle(1, "src");
            assert_eq!(paths(&panel.rows(1)), [("src", 0), ("src/ui", 1), ("src/main.rs", 1), ("README.md", 0)]);
            panel.toggle(1, "src");
            assert_eq!(paths(&panel.rows(1)).len(), 2);
        }

        #[test]
        fn an_opened_folder_is_listed_at_once() {
            let mut panel = panel_with_listings();
            let now = Instant::now();
            assert!(panel.list(1, Path::new("/repo"), now).is_none());
            panel.toggle(1, "src/ui");
            let (_, list) = panel.list(1, Path::new("/repo"), now).expect("the new folder is listed");
            assert_eq!(list.folders, ["", "src/ui"]);
        }

        #[test]
        fn lists_again_after_a_while() {
            let mut panel = Panel::default();
            let now = Instant::now();
            let (generation, _) = panel.list(1, Path::new("/repo"), now).expect("listing");
            assert!(panel.list(1, Path::new("/repo"), now).is_none(), "one listing at a time");
            panel.listed(1, generation, vec![(String::new(), Some(Vec::new()))]);
            assert!(panel.list(1, Path::new("/repo"), now + LIST_EVERY / 2).is_none());
            assert!(panel.list(1, Path::new("/repo"), now + LIST_EVERY).is_some());
        }

        #[test]
        fn a_root_that_went_away_shows_no_files() {
            let mut panel = panel_with_listings();
            panel.listed(1, 0, vec![(String::new(), None)]);
            assert_eq!((panel.rows(1).len(), panel.listed_root(1)), (0, true));
        }

        #[test]
        fn a_folder_that_went_away_closes() {
            let mut panel = panel_with_listings();
            panel.toggle(1, "src");
            panel.listed(1, 0, vec![("src".into(), None)]);
            assert_eq!(paths(&panel.rows(1)), [("src", 0), ("README.md", 0)]);
            assert!(!panel.rows(1)[0].open);
        }
    }

    mod viewer {
        use super::*;

        fn content(lines: &[&str]) -> Content {
            let lines = lines.iter().map(|l| (*l).to_string()).collect();
            Content { stamp: Stamp::default(), language: None, body: disk::Body::Text { lines, styles: None } }
        }

        #[test]
        fn reads_the_open_file_and_drops_answers_for_another() {
            let mut panel = Panel::default();
            let now = Instant::now();
            panel.open_file(1, "a.rs");
            let (first, load) = panel.load(1, Path::new("/repo"), now).expect("a read");
            assert_eq!(load.file, Path::new("/repo/a.rs"));
            panel.open_file(1, "b.rs");
            panel.read(1, first, "a.rs", Some(content(&["old"])), true);
            assert!(panel.viewer(1).and_then(|v| v.content.as_ref()).is_none());
            let (second, _) = panel.load(1, Path::new("/repo"), now).expect("b is read");
            panel.read(1, second, "b.rs", Some(content(&["new"])), true);
            assert_eq!(
                panel.viewer(1).and_then(|v| v.content.as_ref()).map(|c| c.lines().to_vec()),
                Some(vec!["new".to_string()])
            );
        }

        #[test]
        fn checks_the_file_again_every_second() {
            let mut panel = Panel::default();
            let now = Instant::now();
            panel.open_file(1, "a.rs");
            let (generation, _) = panel.load(1, Path::new("/repo"), now).expect("a read");
            panel.read(1, generation, "a.rs", Some(content(&["a"])), false);
            assert!(
                panel.load(1, Path::new("/repo"), now + CHECK_EVERY).is_none(),
                "the highlighting is still on its way"
            );
            panel.read(1, generation, "a.rs", None, true);
            assert!(panel.load(1, Path::new("/repo"), now + CHECK_EVERY / 2).is_none());
            let (_, load) = panel.load(1, Path::new("/repo"), now + CHECK_EVERY).expect("checked again");
            assert!(load.previous.is_some());
        }

        #[test]
        fn going_back_keeps_the_last_file() {
            let mut panel = Panel::default();
            panel.open_file(1, "src/a.rs");
            panel.close_file(1);
            assert_eq!((panel.viewer(1).is_none(), panel.last(1)), (true, Some("src/a.rs")));
        }
    }

    mod searching {
        use super::*;

        fn indexed(panel: &mut Panel, mode: Mode, query: &str) {
            if mode == Mode::Name {
                panel.toggle_mode(1);
            }
            panel.query_mut(1).text = query.into();
            let (generation, _) = panel.index_request(1, Path::new("/repo"), Instant::now()).expect("indexing");
            panel.indexed(1, generation, vec!["src/main.rs".into(), "README.md".into()]);
        }

        #[test]
        fn indexes_once_the_search_bar_is_used() {
            let mut panel = Panel::default();
            assert!(panel.index_request(1, Path::new("/repo"), Instant::now()).is_none());
            panel.query_mut(1).focused = true;
            assert!(panel.index_request(1, Path::new("/repo"), Instant::now()).is_some());
        }

        #[test]
        fn the_focused_bar_takes_the_keys() {
            let mut panel = Panel::default();
            panel.query_mut(1).focused = true;
            assert!(panel.typing(1));
            panel.unfocus();
            assert!(!panel.typing(1));
        }

        #[test]
        fn the_file_icon_switches_to_names_and_keeps_the_query() {
            let mut panel = Panel::default();
            panel.query_mut(1).text = "main".into();
            panel.toggle_mode(1);
            assert_eq!((panel.mode(1), panel.searched(1), panel.typing(1)), (Mode::Name, Some("main"), true));
            panel.toggle_mode(1);
            assert_eq!(panel.mode(1), Mode::Text);
        }

        #[test]
        fn looks_for_names_again_when_the_query_changes() {
            let mut panel = Panel::default();
            indexed(&mut panel, Mode::Name, "main");
            let now = Instant::now();
            let (generation, search) = panel.names_request(1, now).expect("a search");
            panel.named(1, generation, search, Vec::new());
            assert!(panel.names_request(1, now).is_none());
            panel.query_mut(1).text = "read".into();
            assert_eq!(panel.names_request(1, now).map(|(_, s)| s.query), Some("read".to_string()));
        }

        #[test]
        fn a_new_text_search_cancels_the_running_one() {
            let mut panel = Panel::default();
            indexed(&mut panel, Mode::Text, "fn");
            let now = Instant::now();
            let (_, _, first) = panel.grep_request(1, now).expect("a search");
            assert!(panel.grep_request(1, now).is_none(), "the same query is already running");
            panel.query_mut(1).text = "fn main".into();
            let (generation, search, _) = panel.grep_request(1, now).expect("a new search");
            assert!(first.load(Ordering::Relaxed));
            panel.grepped(1, generation, search, search::Text::default());
            assert_eq!(panel.found_text(1).map(|(q, _)| q), Some("fn main"));
        }

        #[test]
        fn opening_a_line_selects_it_and_remembers_the_text() {
            let mut panel = Panel::default();
            panel.open_at(1, "src/main.rs", (40, 40), Some("run".into()));
            let viewer = panel.viewer(1).expect("a viewer");
            assert_eq!((viewer.selection, viewer.scroll, viewer.find.as_deref()), (Some((40, 40)), 36, Some("run")));
        }
    }

    mod marks {
        use super::*;

        const PATCH: &str = "diff --git a/src/a.rs b/src/a.rs
--- a/src/a.rs
+++ b/src/a.rs
@@ -1,7 +1,6 @@
-gone at the top
 one
-two
+TWO
 three
+new
 four
-five
 six
";

        fn file() -> Changed {
            diff::parse(PATCH).remove(0)
        }

        #[test]
        fn marks_added_modified_and_deleted_lines() {
            let gutter = gutter(&file(), 6);
            let marks: Vec<Option<Mark>> = (1..=6).map(|n| gutter.mark(n)).collect();
            assert_eq!(
                marks,
                [Some(Mark::DeletedAbove), Some(Mark::Modified), None, Some(Mark::Added), Some(Mark::Deleted), None]
            );
        }

        #[test]
        fn keeps_the_removed_lines_where_they_were() {
            let gutter = gutter(&file(), 6);
            let removed: Vec<(u32, Vec<&str>)> =
                gutter.removed.iter().map(|(k, v)| (*k, v.iter().map(String::as_str).collect())).collect();
            assert_eq!(removed, [(1, vec!["gone at the top"]), (2, vec!["two"]), (6, vec!["five"])]);
            assert_eq!((gutter.block(2), gutter.block(4), gutter.block(5)), (Some(2), None, Some(6)));
        }

        #[test]
        fn a_new_file_is_all_added() {
            let file = diff::untracked("new.rs", 4, Some(b"a\nb\n"));
            let gutter = gutter(&file, 2);
            assert_eq!((gutter.mark(1), gutter.mark(2), gutter.mark(3)), (Some(Mark::Added), Some(Mark::Added), None));
        }

        #[test]
        fn folders_show_what_changed_inside() {
            let added = diff::untracked("src/ui/new.rs", 1, Some(b"a"));
            let diff = Diff { files: vec![Arc::new(added), Arc::new(file())] };
            let marks = statuses(&diff);
            let mark = |path: &str| marks.get(path).copied();
            assert_eq!(
                [mark("src/a.rs"), mark("src/ui/new.rs"), mark("src"), mark("src/ui"), mark("README.md")],
                [Some(Status::Modified), Some(Status::Untracked), Some(Status::Modified), Some(Status::Added), None]
            );
        }

        #[test]
        fn unfolded_removals_take_rows_before_their_line() {
            let gutter = gutter(&file(), 6);
            let unfolded = HashSet::from([2, 6]);
            let lines = Lines::new(6, Some(&gutter), &unfolded);
            let rows: Vec<Line> = (0..lines.len()).filter_map(|r| lines.at(r)).collect();
            assert_eq!(
                rows,
                [
                    Line::Code(1),
                    Line::Removed(2, 0),
                    Line::Code(2),
                    Line::Code(3),
                    Line::Code(4),
                    Line::Code(5),
                    Line::Removed(6, 0),
                    Line::Code(6)
                ]
            );
            assert_eq!((lines.row_of(2), lines.row_of(6), lines.removed(6, 0)), (2, 7, Some("five")));
        }
    }
}
