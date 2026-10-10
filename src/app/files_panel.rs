use std::collections::HashSet;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use super::{App, AppEvent, LinkPress, copy, wheel};
use crate::changes::{Checkout, Tints};
use crate::emulator::Snapshot;
use crate::error::Result;
use crate::files::link::{self, Target};
use crate::files::search::{self, MAX_MATCHES, MAX_NAMES};
use crate::files::{self, Mode, Query, disk};
use crate::log::{self, Job, Level};
use crate::mouse::MouseMode;
use crate::panics;
use crate::project::Tab;
use crate::ui;
use crate::ui::changes::Action;
use crate::ui::files::{self as panel, Bar, FileView, Found, Hit, Screen, SearchView, TreeRow, TreeView};

impl App {
    fn files_target(&self) -> Option<(u64, PathBuf)> {
        let workspace = self.project()?.workspace()?;
        Some((workspace.id, workspace.path.clone()))
    }

    pub(super) fn files_shown(&self) -> bool {
        self.files.open && self.files_target().is_some()
    }

    pub(super) fn toggle_files(&mut self) {
        if self.files.open {
            self.files.close();
        } else {
            self.changes.close();
            self.close_todo();
            self.files.open = true;
        }
        self.nav = None;
    }

    pub(super) fn refresh_files(&mut self, now: Instant) {
        let shown = self.files_shown();
        self.changes.watched = shown;
        let Some((workspace, root)) = self.files_target().filter(|_| shown) else { return };
        if let Some((generation, list)) = self.files.list(workspace, &root, now) {
            let tx = self.tx.clone();
            let job = Job::new(Level::Debug, "files", "list").with("workspace", workspace);
            let job = job.with("folders", list.folders.len()).begin();
            std::thread::spawn(move || {
                let folders = panics::contain(|| {
                    list.folders.iter().map(|folder| (folder.clone(), disk::list(&list.root, folder).ok())).collect()
                });
                job.done();
                let folders = folders.unwrap_or_default();
                let _ = tx.send(AppEvent::FilesListed { workspace, generation, folders });
            });
        }
        if let Some((generation, load)) = self.files.load(workspace, &root, now) {
            let tx = self.tx.clone();
            let job = Job::new(Level::Debug, "files", "read").with("workspace", workspace).with("path", &load.path);
            let job = job.begin();
            std::thread::spawn(move || {
                let send = |content, done| {
                    let path = load.path.clone();
                    let _ = tx.send(AppEvent::FileRead { workspace, generation, path, content, done });
                };
                let read = panics::contain(|| disk::read(&load.file, load.previous));
                job.done();
                if let Some(Some(disk::Read {
                    content: disk::Content { body: disk::Body::Unreadable { reason, .. }, .. },
                    ..
                })) = &read
                {
                    log::info!("files", "image unreadable", path = &load.path, reason = reason);
                }
                let Some(Some(read)) = read else {
                    send(None, true);
                    return;
                };
                if read.source.is_none() {
                    send(Some(read.content), true);
                    return;
                }
                if load.previous.is_none() {
                    send(Some(read.content.clone()), false);
                }
                let plain = read.content.clone();
                let styled = panics::contain(|| disk::highlighted(read)).flatten();
                send(Some(styled.unwrap_or(plain)), true);
            });
        }
        self.refresh_search(workspace, &root, now);
    }

    fn refresh_search(&mut self, workspace: u64, root: &Path, now: Instant) {
        if let Some((generation, root)) = self.files.index_request(workspace, root, now) {
            let tx = self.tx.clone();
            let job = Job::new(Level::Debug, "files", "index").with("workspace", workspace).begin();
            std::thread::spawn(move || {
                let paths = panics::contain(|| search::index(&root)).unwrap_or_default();
                job.with("files", paths.len()).done();
                let _ = tx.send(AppEvent::FilesIndexed { workspace, generation, paths });
            });
        }
        if let Some((generation, search)) = self.files.names_request(workspace, now) {
            let tx = self.tx.clone();
            let job = Job::new(Level::Debug, "files", "name search").with("workspace", workspace).begin();
            std::thread::spawn(move || {
                let found = panics::contain(|| search::names(&search.index, &search.query)).unwrap_or_default();
                job.with("found", found.len()).done();
                let _ = tx.send(AppEvent::NamesFound { workspace, generation, search, found });
            });
        }
        if let Some((generation, search, cancel)) = self.files.grep_request(workspace, now) {
            let (tx, root) = (self.tx.clone(), root.to_path_buf());
            let job = Job::new(Level::Debug, "files", "text search").with("workspace", workspace).begin();
            std::thread::spawn(move || {
                let found =
                    panics::contain(|| search::text(&root, &search.index, &search.query, &cancel)).unwrap_or_default();
                job.with("capped", found.capped).done();
                let _ = tx.send(AppEvent::TextFound { workspace, generation, search, found });
            });
        }
    }

    pub(super) fn forget_files(&mut self) {
        let alive: HashSet<u64> = self.projects.iter().flat_map(|p| &p.workspaces).map(|w| w.id).collect();
        self.files.forget(&alive);
    }

    fn file_view(&self, workspace: u64, viewer: &files::Viewer) -> FileView {
        let lines = viewer.content.as_ref().map_or(0, |c| c.lines().len());
        let gutter = self
            .changes_target()
            .filter(|t| t.workspace == workspace)
            .and_then(|t| self.changed_diff(&t))
            .and_then(|diff| diff.files.iter().find(|f| f.path == viewer.path).cloned())
            .map(|file| files::gutter(&file, lines));
        FileView {
            path: viewer.path.clone(),
            content: viewer.content.clone(),
            gutter,
            unfolded: viewer.unfolded.clone(),
            scroll: viewer.scroll,
            selection: viewer.selection,
            find: viewer.find.clone(),
            image: None,
        }
    }

    fn folder_view(&self, workspace: u64) -> TreeView {
        let statuses = self
            .changes_target()
            .filter(|t| t.workspace == workspace)
            .and_then(|t| self.changed_diff(&t))
            .map(|diff| files::statuses(&diff))
            .unwrap_or_default();
        let rows = self
            .files
            .rows(workspace)
            .into_iter()
            .map(|row| TreeRow {
                status: statuses.get(&row.path).copied(),
                name: row.name,
                path: row.path,
                depth: row.depth,
                dir: row.dir,
                open: row.open,
            })
            .collect();
        TreeView {
            rows,
            scroll: self.files.scroll(workspace),
            last: self.files.last(workspace).map(str::to_string),
            loading: !self.files.listed_root(workspace),
        }
    }

    fn found(&self, workspace: u64, wanted: &str) -> (Vec<Found>, String) {
        let searching = self.files.searching(workspace);
        match self.files.mode(workspace) {
            Mode::Name => {
                let (answered, names) = self.files.found_names(workspace).unwrap_or_default();
                let rows: Vec<Found> =
                    names.iter().map(|n| Found::Name { path: n.path.clone(), indices: n.indices.clone() }).collect();
                let note = match rows.len() {
                    _ if answered != wanted && searching => "searching…".into(),
                    0 => "no file name matches".into(),
                    n if n >= MAX_NAMES => format!("the best {MAX_NAMES} files"),
                    n => plural(n, "file"),
                };
                (rows, note)
            }
            Mode::Text => {
                let Some((answered, text)) = self.files.found_text(workspace) else {
                    return (Vec::new(), "searching…".into());
                };
                let mut rows = Vec::new();
                for file in &text.files {
                    rows.push(Found::File { path: file.path.clone(), count: file.lines.len() });
                    rows.extend(file.lines.iter().map(|line| Found::Line {
                        path: file.path.clone(),
                        number: line.number,
                        text: line.text.clone(),
                        ranges: line.ranges.clone(),
                    }));
                }
                let first = if text.capped {
                    format!("the first {MAX_MATCHES} matches")
                } else {
                    plural(text.matches, "match")
                };
                let note = match text.matches {
                    _ if answered != wanted && searching => "searching…".into(),
                    0 => "nothing found".into(),
                    _ => format!("{first} in {}", plural(text.files.len(), "file")),
                };
                (rows, note)
            }
        }
    }

    fn found_view(&self, workspace: u64) -> Option<SearchView> {
        let query = self.files.query(workspace)?;
        let (rows, note) = self.found(workspace, query.wanted()?);
        let selected = rows.iter().enumerate().filter(|(_, r)| r.selectable()).nth(query.selected).map(|(i, _)| i);
        Some(SearchView { rows, note, selected, scroll: query.scroll })
    }

    fn bar(&self, workspace: u64) -> Bar {
        let query = self.files.query(workspace).cloned().unwrap_or_default();
        Bar { query: query.text, focused: query.focused && self.overlay.is_none(), mode: self.files.mode(workspace) }
    }

    pub(super) fn files_view(&self) -> Option<panel::View> {
        let (workspace, root) = self.files_target()?;
        let screen = match self.files.viewer(workspace) {
            Some(viewer) => Screen::File(self.file_view(workspace, viewer)),
            None => match self.found_view(workspace) {
                Some(search) => Screen::Search(search),
                None => Screen::Tree(self.folder_view(workspace)),
            },
        };
        Some(panel::View {
            root: ui::display_path(&root, self.home.as_deref()),
            bar: self.bar(workspace),
            screen,
            light: self.theme.is_light() == Some(true),
            muted: ui::muted(&self.theme),
            tint: Tints::of(&self.theme).removed,
        })
    }

    pub(super) fn files_mouse(&mut self, ev: MouseEvent, pos: Position, panel_area: Rect, area: Rect) -> Result<()> {
        let (Some((workspace, root)), Some(view)) = (self.files_target(), self.files_view()) else { return Ok(()) };
        if let Some(delta) = wheel(ev.kind) {
            let max = panel::max_scroll(panel_area, &view);
            let scrolled = |scroll: usize| scroll.min(max).saturating_add_signed(delta).min(max);
            if let Some(viewer) = self.files.viewer_mut(workspace) {
                viewer.scroll = scrolled(viewer.scroll);
            } else if matches!(view.screen, Screen::Search(_)) {
                let query = self.files.query_mut(workspace);
                query.scroll = scrolled(query.scroll);
            } else {
                let scroll = scrolled(self.files.scroll(workspace));
                self.files.set_scroll(workspace, scroll);
            }
            return Ok(());
        }
        if ev.kind != MouseEventKind::Down(MouseButton::Left) {
            return Ok(());
        }
        match panel::hit(panel_area, &view, pos) {
            Some(Hit::Close) => self.files.close(),
            Some(Hit::Mode) => self.files.toggle_mode(workspace),
            Some(Hit::Field) => self.files.query_mut(workspace).focused = true,
            Some(Hit::Clear) => self.edit_query(workspace, String::clear),
            Some(Hit::Found(i)) => self.open_found(workspace, &view, i),
            Some(Hit::Back) => self.files.close_file(workspace),
            Some(Hit::Row(i)) => {
                let Screen::Tree(tree) = &view.screen else { return Ok(()) };
                let Some(row) = tree.rows.get(i) else { return Ok(()) };
                if row.dir {
                    self.files.toggle(workspace, &row.path);
                } else {
                    self.files.open_file(workspace, &row.path);
                }
            }
            Some(Hit::Line(n)) => {
                if let Some(viewer) = self.files.viewer_mut(workspace) {
                    viewer.selection = Some((n, n));
                    self.files.selecting = Some(n);
                }
            }
            Some(Hit::Fold(key)) => {
                if let Some(viewer) = self.files.viewer_mut(workspace)
                    && !viewer.unfolded.remove(&key)
                {
                    viewer.unfolded.insert(key);
                }
            }
            Some(Hit::Action(action)) => return self.file_action(workspace, root, action, area),
            None => {}
        }
        Ok(())
    }

    pub(super) fn files_typing(&self) -> bool {
        self.overlay.is_none() && self.files_shown() && self.files_target().is_some_and(|(ws, _)| self.files.typing(ws))
    }

    fn edit_query(&mut self, workspace: u64, edit: impl FnOnce(&mut String)) {
        let query = self.files.query_mut(workspace);
        edit(&mut query.text);
        query.focused = true;
        query.selected = 0;
        query.scroll = 0;
        query.enter = false;
    }

    pub(super) fn files_key(&mut self, key: KeyEvent, area: Rect) {
        let Some((workspace, _)) = self.files_target() else { return };
        match key.code {
            KeyCode::Esc => *self.files.query_mut(workspace) = Query::default(),
            KeyCode::Enter if !self.answers_query(workspace) => self.files.query_mut(workspace).enter = true,
            KeyCode::Enter => self.open_selected(workspace),
            KeyCode::Up => self.move_found(workspace, -1, area),
            KeyCode::Down => self.move_found(workspace, 1, area),
            KeyCode::Backspace => self.edit_query(workspace, |q| _ = q.pop()),
            KeyCode::Char(c) if !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                self.edit_query(workspace, |q| q.push(c));
            }
            _ => {}
        }
    }

    fn answers_query(&self, workspace: u64) -> bool {
        self.files.query(workspace).and_then(Query::wanted) == self.files.answered(workspace)
    }

    fn open_selected(&mut self, workspace: u64) {
        if let Some(view) = self.files_view()
            && let Screen::Search(search) = &view.screen
            && let Some(i) = search.selected
        {
            self.open_found(workspace, &view, i);
        }
    }

    pub(super) fn found_answered(&mut self, workspace: u64) {
        let waiting = self.files.query(workspace).is_some_and(|q| q.enter) && self.answers_query(workspace);
        if !waiting {
            return;
        }
        self.files.query_mut(workspace).enter = false;
        if self.files_shown() && self.files_target().is_some_and(|(ws, _)| ws == workspace) {
            self.open_selected(workspace);
        }
    }

    pub(super) fn files_paste(&mut self, text: &str) {
        let Some((workspace, _)) = self.files_target() else { return };
        self.edit_query(workspace, |q| q.extend(text.chars().filter(|c| !c.is_control())));
    }

    fn move_found(&mut self, workspace: u64, delta: isize, area: Rect) {
        self.files.query_mut(workspace).enter = false;
        let Some(view) = self.files_view() else { return };
        let Screen::Search(search) = &view.screen else { return };
        let selectable: Vec<usize> =
            search.rows.iter().enumerate().filter(|(_, r)| r.selectable()).map(|(i, _)| i).collect();
        let shown = panel::shown_rows(self.layout(area).shown(self.nav).changes).max(1);
        let Some(last) = selectable.len().checked_sub(1) else { return };
        let query = self.files.query_mut(workspace);
        query.selected = query.selected.min(last).saturating_add_signed(delta).min(last);
        let row = selectable[query.selected];
        let header = usize::from(row > 0 && !search.rows[row - 1].selectable());
        if row.saturating_sub(header) < query.scroll {
            query.scroll = row.saturating_sub(header);
        } else if row >= query.scroll + shown {
            query.scroll = row + 1 - shown;
        }
    }

    fn open_found(&mut self, workspace: u64, view: &panel::View, i: usize) {
        let Screen::Search(search) = &view.screen else { return };
        let line = search.rows.get(i..).and_then(|rows| rows.iter().find(|r| r.selectable()));
        let query = self.files.query_mut(workspace);
        query.focused = false;
        query.selected = search.rows.iter().take(i).filter(|r| r.selectable()).count();
        let find = query.wanted().map(str::to_string);
        match line {
            Some(Found::Name { path, .. }) => self.files.open_file(workspace, path),
            Some(Found::Line { path, number, .. }) => {
                self.files.open_at(workspace, path, (*number, *number), find);
            }
            Some(Found::File { .. }) | None => {}
        }
    }

    pub(super) fn drag_lines(&mut self, ev: MouseEvent, panel_area: Rect) {
        let Some(anchor) = self.files.selecting else { return };
        match ev.kind {
            MouseEventKind::Drag(MouseButton::Left) => {
                let (Some((workspace, _)), Some(view)) = (self.files_target(), self.files_view()) else { return };
                let Some(line) = panel::line_near(panel_area, &view, ev.row) else { return };
                if let Some(viewer) = self.files.viewer_mut(workspace) {
                    viewer.selection = Some((anchor, line));
                }
            }
            _ => self.files.selecting = None,
        }
    }

    fn file_action(&mut self, workspace: u64, root: PathBuf, action: Action, area: Rect) -> Result<()> {
        let Some(viewer) = self.files.viewer(workspace) else { return Ok(()) };
        let path = viewer.path.clone();
        let image = viewer.content.as_ref().is_some_and(|c| c.is_image());
        let selected = viewer.selected().filter(|_| !image);
        match action {
            Action::Open => {
                let line = selected.map_or(1, |(first, _)| first);
                let target = Checkout { workspace, dir: root, base: None };
                return self.open_in_editor(&target, &path, line, area);
            }
            Action::Ask => {
                let reference = match selected {
                    Some((a, b)) if a == b => format!("{path}:{a} "),
                    Some((a, b)) => format!("{path}:{a}-{b} "),
                    None => format!("{path} "),
                };
                self.ask_agent(workspace, &reference);
            }
            Action::Copy => {
                let text = match (selected, &viewer.content) {
                    (Some((a, b)), Some(content)) => {
                        let lines = content.lines();
                        let range = usize::try_from(a).unwrap_or(1).saturating_sub(1)
                            ..usize::try_from(b).unwrap_or(usize::MAX).min(lines.len());
                        lines.get(range).map(|l| l.join("\n")).unwrap_or_default()
                    }
                    _ => path,
                };
                copy(&mut self.host_writes, &mut self.toast, &text);
            }
        }
        Ok(())
    }

    fn link_at(&mut self, at: Position) -> Option<Target> {
        let root = self.project()?.workspace()?.path.clone();
        let term = self.term_mut()?;
        let screen = term.emulator.snapshot().ok()?;
        let found = link::at(screen.rows.get(usize::from(at.y))?, at.x)?;
        link::resolve(&found, term.cwd().as_deref(), &root)
    }

    pub(super) fn link_click(&mut self, ev: MouseEvent, at: Position) -> bool {
        let Some((id, reads)) = self.term().map(|t| (t.id, t.emulator.mouse_mode() != MouseMode::None)) else {
            return false;
        };
        match ev.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                self.link_press = None;
                let Some(target) = self.link_at(at) else { return false };
                self.link_press = Some(LinkPress { term: id, at, target, held: reads.then_some(ev) });
                reads
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                let Some(press) = self.link_press.clone().filter(|p| p.term == id && p.held.is_some()) else {
                    return false;
                };
                if press.at == at {
                    return true;
                }
                self.link_press = None;
                if let Some(held) = press.held {
                    self.forward_mouse(held, press.at);
                }
                false
            }
            MouseEventKind::Up(MouseButton::Left) => {
                let Some(press) = self.link_press.take().filter(|p| p.term == id && p.held.is_some()) else {
                    return false;
                };
                if press.at == at {
                    self.open_link(&press.target);
                    return true;
                }
                if let Some(held) = press.held {
                    self.forward_mouse(held, press.at);
                }
                false
            }
            _ => false,
        }
    }

    pub(super) fn open_link(&mut self, target: &Target) {
        let Some(workspace) = self.project().and_then(crate::project::Project::workspace).map(|w| w.id) else {
            return;
        };
        if !self.files.open {
            self.changes.close();
            self.close_todo();
            self.files.open = true;
        }
        self.nav = None;
        match target.lines {
            Some(lines) => self.files.open_at(workspace, &target.path, lines, None),
            None => self.files.open_file(workspace, &target.path),
        }
    }

    pub(super) fn hovered_link(
        tab: &Tab,
        screens: &[Snapshot],
        pane_area: Rect,
        hover: Position,
        root: &Path,
    ) -> Option<(u16, Range<u16>)> {
        let term = tab.pane()?;
        let pane = tab.rect(pane_area, term.id)?;
        if !pane.contains(hover) {
            return None;
        }
        let (row, col) = (hover.y - pane.y, hover.x - pane.x);
        let found = link::at(screens.get(tab.active)?.rows.get(usize::from(row))?, col)?;
        link::resolve(&found, term.cwd().as_deref(), root)?;
        Some((row, found.start..found.end))
    }
}

fn plural(n: usize, what: &str) -> String {
    match (n, what) {
        (1, _) => format!("1 {what}"),
        (_, "match") => format!("{n} matches"),
        _ => format!("{n} {what}s"),
    }
}
