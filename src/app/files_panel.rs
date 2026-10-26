use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use super::input::wheel;
use super::layout::Side;
use super::{App, AppEvent};
use crate::changes::{Checkout, Tints};
use crate::error::Result;
use crate::files::find::Find;
use crate::files::search::{self, MAX_MATCHES, MAX_NAMES};
use crate::files::{self, Mode, Query, Reading, disk};
use crate::log::{self, Job, Level};
use crate::panics;
use crate::project::Hierarchy;
use crate::ui;
use crate::ui::files::{self as panel, Action, Bar, FileView, Found, Hit, Screen, SearchView, TreeRow, TreeView};

mod find;
mod selection;

fn text_rows(text: &search::Text) -> Vec<Found> {
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
    rows
}

fn text_note(text: &search::Text, waiting: bool) -> String {
    let first = if text.capped { format!("the first {MAX_MATCHES} matches") } else { plural(text.matches, "match") };
    match text.matches {
        _ if waiting => "searching…".into(),
        0 => "nothing found".into(),
        _ => format!("{first} in {}", plural(text.files.len(), "file")),
    }
}

fn read_and_send(tx: &Sender<AppEvent>, job: Job, workspace: u64, generation: u64, load: &files::Load) {
    let send = |content, done| {
        let path = load.path.clone();
        let _ = tx.send(AppEvent::Files(FilesEvent::Read { workspace, generation, path, content, done }));
    };
    let read = panics::contain(|| disk::read(&load.file, load.previous));
    job.done();
    if let Some(Some(disk::Content { body: disk::Body::Unreadable { reason, .. }, .. })) = &read {
        log::info!("files", "image unreadable", path = &load.path, reason = reason);
    }
    let Some(Some(content)) = read else {
        send(None, true);
        return;
    };
    if !matches!(content.body, disk::Body::Text { .. }) {
        send(Some(content), true);
        return;
    }
    if load.previous.is_none() {
        send(Some(content.clone()), false);
    }
    let plain = content.clone();
    let styled = panics::contain(|| disk::highlighted(content)).flatten();
    send(Some(styled.unwrap_or(plain)), true);
}

fn plural(n: usize, what: &str) -> String {
    match (n, what) {
        (1, _) => format!("1 {what}"),
        (_, "match") => format!("{n} matches"),
        _ => format!("{n} {what}s"),
    }
}

#[derive(Debug)]
pub(crate) enum FilesEvent {
    Listed { workspace: u64, generation: u64, folders: Vec<(String, Option<Vec<disk::Entry>>)> },
    Read { workspace: u64, generation: u64, path: String, content: Option<disk::Content>, done: bool },
    Indexed { workspace: u64, generation: u64, paths: Vec<String> },
    Names { workspace: u64, generation: u64, search: files::Search, found: Vec<search::Name> },
    Text { workspace: u64, generation: u64, search: files::Search, found: search::Text },
}

impl FilesEvent {
    pub(crate) fn name(&self) -> &'static str {
        match self {
            Self::Listed { .. } => "files listed",
            Self::Read { .. } => "file read",
            Self::Indexed { .. } => "files indexed",
            Self::Names { .. } => "names found",
            Self::Text { .. } => "text found",
        }
    }
}

impl App {
    pub(super) fn files_answered(&mut self, event: FilesEvent) {
        match event {
            FilesEvent::Listed { workspace, generation, folders } => {
                self.panels.files.listed(workspace, generation, folders);
            }
            FilesEvent::Read { workspace, generation, path, content, done } => {
                self.panels.files.read(Reading { workspace, generation, path: &path }, content, done);
            }
            FilesEvent::Indexed { workspace, generation, paths } => {
                self.panels.files.indexed(workspace, generation, paths);
            }
            FilesEvent::Names { workspace, generation, search, found } => {
                self.panels.files.named(workspace, generation, search, found);
                self.found_answered(workspace);
            }
            FilesEvent::Text { workspace, generation, search, found } => {
                self.panels.files.grepped(workspace, generation, search, found);
                self.found_answered(workspace);
            }
        }
    }

    fn files_target(&self) -> Option<(u64, PathBuf)> {
        let workspace = self.project()?.workspace()?;
        Some((workspace.id, workspace.path.clone()))
    }

    pub(super) fn files_shown(&self) -> bool {
        self.screen.side == Some(Side::Files) && self.files_target().is_some()
    }

    pub(super) fn toggle_files(&mut self) {
        let side = (self.screen.side != Some(Side::Files)).then_some(Side::Files);
        self.show_side(side);
        self.screen.nav = None;
    }

    pub(super) fn refresh_files(&mut self, now: Instant) {
        let shown = self.files_shown();
        self.panels.changes.set_watched(self.screen.side == Some(Side::Changes) || shown);
        let Some((workspace, root)) = self.files_target().filter(|_| shown) else { return };
        if let Some((generation, list)) = self.panels.files.list(workspace, &root, now) {
            self.spawn_listing(workspace, generation, list);
        }
        if let Some((generation, load)) = self.panels.files.load(workspace, &root, now) {
            let tx = self.tx.clone();
            let job = Job::new(Level::Debug, "files", "read").with("workspace", workspace).with("path", &load.path);
            let job = job.begin();
            std::thread::spawn(move || read_and_send(&tx, job, workspace, generation, &load));
        }
        self.refresh_search(workspace, &root, now);
    }

    fn spawn_listing(&mut self, workspace: u64, generation: u64, list: files::List) {
        let job = Job::new(Level::Debug, "files", "list").with("workspace", workspace);
        let job = job.with("folders", list.folders.len());
        let listing = move || {
            Ok(list.folders.iter().map(|folder| (folder.clone(), disk::list(&list.root, folder).ok())).collect())
        };
        self.spawn_job(job, listing, move |folders, _| {
            Some(AppEvent::Files(FilesEvent::Listed { workspace, generation, folders: folders.unwrap_or_default() }))
        });
    }

    fn refresh_search(&mut self, workspace: u64, root: &Path, now: Instant) {
        if let Some((generation, index)) = self.panels.files.index_request(workspace, root, now) {
            let job = Job::new(Level::Debug, "files", "index").with("workspace", workspace);
            self.spawn_job(
                job,
                move || Ok(search::index(&index.root, &index.cancel)),
                move |paths, job| {
                    let paths = paths.unwrap_or_default();
                    job.note("files", paths.len());
                    Some(AppEvent::Files(FilesEvent::Indexed { workspace, generation, paths }))
                },
            );
        }
        if let Some((generation, search)) = self.panels.files.names_request(workspace, now) {
            let job = Job::new(Level::Debug, "files", "name search").with("workspace", workspace);
            let (index, query) = (Arc::clone(&search.index), search.query.clone());
            self.spawn_job(
                job,
                move || Ok(search::names(&index, &query)),
                move |found, job| {
                    let found = found.unwrap_or_default();
                    job.note("found", found.len());
                    Some(AppEvent::Files(FilesEvent::Names { workspace, generation, search, found }))
                },
            );
        }
        if let Some((generation, search, cancel)) = self.panels.files.grep_request(workspace, now) {
            let root = root.to_path_buf();
            let job = Job::new(Level::Debug, "files", "text search").with("workspace", workspace);
            let (index, query) = (Arc::clone(&search.index), search.query.clone());
            self.spawn_job(
                job,
                move || Ok(search::text(&root, &index, &query, &cancel)),
                move |found, job| {
                    let found = found.unwrap_or_default();
                    job.note("capped", found.capped);
                    Some(AppEvent::Files(FilesEvent::Text { workspace, generation, search, found }))
                },
            );
        }
    }

    pub(super) fn forget_files(&mut self) {
        let alive: HashSet<u64> = self.projects.workspaces().map(|w| w.id).collect();
        self.panels.files.forget(&alive);
    }

    fn file_view(&self, workspace: u64, viewer: &files::Viewer) -> FileView {
        let lines = viewer.content.as_ref().map_or(0, |c| c.lines().len());
        let gutter = self
            .changes_target()
            .filter(|t| t.workspace == workspace)
            .and_then(|t| self.marks_diff(&t))
            .and_then(|diff| diff.files.iter().find(|f| f.path == viewer.path).cloned())
            .map(|file| files::gutter(&file, lines));
        FileView {
            path: viewer.path.clone(),
            content: viewer.content.clone(),
            gutter,
            unfolded: viewer.unfolded.clone(),
            scroll: viewer.scroll,
            selection: viewer.selection,
            stretch: viewer.stretched(),
            find: viewer.finding().cloned().map(|mut find| {
                find.focused &= self.overlay.is_none();
                find
            }),
            image: None,
            full_screen: self.file_full_screen(),
        }
    }

    pub(super) fn file_full_screen(&self) -> bool {
        self.panels.files.full_screen()
            && self.files_shown()
            && self.files_target().is_some_and(|(ws, _)| self.panels.files.viewer(ws).is_some())
    }

    fn folder_view(&self, workspace: u64) -> TreeView {
        let statuses = self
            .changes_target()
            .filter(|t| t.workspace == workspace)
            .and_then(|t| self.marks_diff(&t))
            .map(|diff| files::statuses(&diff))
            .unwrap_or_default();
        let rows = self
            .panels
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
            scroll: self.panels.files.scroll(workspace),
            last: self.panels.files.last(workspace).map(str::to_string),
            loading: !self.panels.files.listed_root(workspace),
        }
    }

    fn found(&self, workspace: u64, wanted: &str) -> (Vec<Found>, String) {
        let searching = self.panels.files.searching(workspace);
        match self.panels.files.mode(workspace) {
            Mode::Name => {
                let (answered, names) = self.panels.files.found_names(workspace).unwrap_or_default();
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
            Mode::Text => match self.panels.files.found_text(workspace) {
                Some((answered, text)) => (text_rows(text), text_note(text, answered != wanted && searching)),
                None => (Vec::new(), "searching…".into()),
            },
        }
    }

    fn found_view(&self, workspace: u64) -> Option<SearchView> {
        let query = self.panels.files.query(workspace)?;
        let (rows, note) = self.found(workspace, query.wanted()?);
        let selected = rows.iter().enumerate().filter(|(_, r)| r.selectable()).nth(query.selected).map(|(i, _)| i);
        Some(SearchView { rows, note, selected, scroll: query.scroll })
    }

    fn bar(&self, workspace: u64) -> Bar {
        let query = self.panels.files.query(workspace).cloned().unwrap_or_default();
        Bar {
            query: query.text,
            focused: query.focused && self.overlay.is_none(),
            mode: self.panels.files.mode(workspace),
        }
    }

    pub(super) fn files_view(&self) -> Option<panel::View> {
        let (workspace, root) = self.files_target()?;
        let screen = match self.panels.files.viewer(workspace) {
            Some(viewer) => Screen::File(Box::new(self.file_view(workspace, viewer))),
            None => match self.found_view(workspace) {
                Some(search) => Screen::Search(search),
                None => Screen::Tree(self.folder_view(workspace)),
            },
        };
        Some(panel::View {
            root: crate::text::display_path(&root, self.home.as_deref()),
            bar: self.bar(workspace),
            screen,
            light: self.themes.light(),
            muted: ui::muted(self.themes.host()),
            tint: Tints::of(self.themes.host()).removed,
            icons: false,
            soft: ui::soft(self.themes.host()),
        })
    }

    pub(super) fn files_mouse(&mut self, ev: MouseEvent, pos: Position, panel_area: Rect, area: Rect) -> Result<()> {
        let (Some((workspace, root)), Some(view)) = (self.files_target(), self.files_view()) else { return Ok(()) };
        if let Some(delta) = wheel(ev.kind) {
            self.files_wheel(workspace, &view, panel_area, delta);
            return Ok(());
        }
        if ev.kind == MouseEventKind::Down(MouseButton::Right) {
            self.files_right_click(workspace, &view, panel_area, pos);
            return Ok(());
        }
        if ev.kind != MouseEventKind::Down(MouseButton::Left) {
            return Ok(());
        }
        match panel::hit(panel_area, &view, pos) {
            Some(Hit::Close) => self.show_side(None),
            Some(Hit::Mode) => self.panels.files.toggle_mode(workspace),
            Some(Hit::Field) => self.panels.files.query_mut(workspace).focused = true,
            Some(Hit::Clear) => self.edit_query(workspace, String::clear),
            Some(Hit::Found(i)) => self.open_found(workspace, &view, i),
            Some(hit @ (Hit::Find | Hit::FindField | Hit::FindPrevious | Hit::FindNext | Hit::FindClose)) => {
                self.find_mouse(workspace, hit, panel_area);
            }
            Some(Hit::Back) => self.panels.files.close_file(workspace),
            Some(Hit::FullScreen) => self.panels.files.set_full_screen(!self.panels.files.full_screen()),
            Some(Hit::Row(i)) => self.click_file_row(workspace, &view, i),
            Some(Hit::Line(n)) => self.press_file_line(workspace, n, area),
            Some(Hit::Text(n)) => self.press_file_text(workspace, n, &view, (pos, panel_area, area)),
            Some(Hit::Fold(key)) => self.toggle_unfolded(workspace, key),
            Some(Hit::Action(action)) => {
                return self.file_action(&Checkout { workspace, dir: root, base: None }, action, pos, area);
            }
            None => {}
        }
        Ok(())
    }

    fn files_wheel(&mut self, workspace: u64, view: &panel::View, panel_area: Rect, delta: isize) {
        let max = panel::max_scroll(panel_area, view);
        let scrolled = |scroll: usize| scroll.min(max).saturating_add_signed(delta).min(max);
        if let Some(viewer) = self.panels.files.viewer_mut(workspace) {
            viewer.scroll = scrolled(viewer.scroll);
        } else if matches!(view.screen, Screen::Search(_)) {
            let query = self.panels.files.query_mut(workspace);
            query.scroll = scrolled(query.scroll);
        } else {
            let scroll = scrolled(self.panels.files.scroll(workspace));
            self.panels.files.set_scroll(workspace, scroll);
        }
    }

    fn files_right_click(&mut self, workspace: u64, view: &panel::View, panel_area: Rect, pos: Position) {
        if let Some(Hit::Row(i)) = panel::hit(panel_area, view, pos)
            && let Screen::Tree(tree) = &view.screen
            && let Some(row) = tree.rows.get(i)
        {
            self.open_row_menu(workspace, &row.path, pos);
        }
    }

    fn click_file_row(&mut self, workspace: u64, view: &panel::View, i: usize) {
        let Screen::Tree(tree) = &view.screen else { return };
        let Some(row) = tree.rows.get(i) else { return };
        if row.dir {
            self.panels.files.toggle(workspace, &row.path);
        } else {
            self.panels.files.open_file(workspace, &row.path);
        }
    }

    fn toggle_unfolded(&mut self, workspace: u64, key: u32) {
        if let Some(viewer) = self.panels.files.viewer_mut(workspace)
            && !viewer.unfolded.remove(&key)
        {
            viewer.unfolded.insert(key);
        }
    }

    pub(super) fn files_typing(&self) -> bool {
        self.overlay.is_none()
            && self.files_shown()
            && self.files_target().is_some_and(|(ws, _)| self.panels.files.typing(ws))
    }

    fn edit_query(&mut self, workspace: u64, edit: impl FnOnce(&mut String)) {
        let query = self.panels.files.query_mut(workspace);
        edit(&mut query.text);
        query.focused = true;
        query.selected = 0;
        query.scroll = 0;
        query.enter = false;
    }

    pub(super) fn files_key(&mut self, key: KeyEvent, area: Rect) {
        let Some((workspace, _)) = self.files_target() else { return };
        if self.panels.files.viewer(workspace).is_some() {
            self.find_key(workspace, key, self.layout(area).shown(self.screen.nav).changes);
            return;
        }
        match key.code {
            KeyCode::Esc if self.panels.files.query(workspace).is_none_or(|q| q.text.is_empty()) => self.close_panel(),
            KeyCode::Esc => *self.panels.files.query_mut(workspace) = Query::default(),
            KeyCode::Enter if !self.answers_query(workspace) => self.panels.files.query_mut(workspace).enter = true,
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
        self.panels.files.query(workspace).and_then(Query::wanted) == self.panels.files.answered(workspace)
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
        let waiting = self.panels.files.query(workspace).is_some_and(|q| q.enter) && self.answers_query(workspace);
        if !waiting {
            return;
        }
        self.panels.files.query_mut(workspace).enter = false;
        if self.files_shown() && self.files_target().is_some_and(|(ws, _)| ws == workspace) {
            self.open_selected(workspace);
        }
    }

    pub(super) fn files_paste(&mut self, text: &str, area: Rect) {
        let Some((workspace, _)) = self.files_target() else { return };
        if self.panels.files.viewer(workspace).is_some() {
            self.find_paste(workspace, text, self.layout(area).shown(self.screen.nav).changes);
            return;
        }
        self.edit_query(workspace, |q| q.extend(text.chars().filter(|c| !c.is_control())));
    }

    fn move_found(&mut self, workspace: u64, delta: isize, area: Rect) {
        self.panels.files.query_mut(workspace).enter = false;
        let Some(view) = self.files_view() else { return };
        let Screen::Search(search) = &view.screen else { return };
        let selectable: Vec<usize> =
            search.rows.iter().enumerate().filter(|(_, r)| r.selectable()).map(|(i, _)| i).collect();
        let shown = panel::shown_rows(self.layout(area).shown(self.screen.nav).changes).max(1);
        let Some(last) = selectable.len().checked_sub(1) else { return };
        let query = self.panels.files.query_mut(workspace);
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
        let query = self.panels.files.query_mut(workspace);
        let focused = query.focused;
        query.focused = false;
        query.selected = search.rows.iter().take(i).filter(|r| r.selectable()).count();
        let wanted = query.wanted().map(str::to_string);
        match line {
            Some(Found::Name { path, .. }) => self.panels.files.open_file(workspace, path),
            Some(Found::Line { path, number, .. }) => {
                let find = wanted.map(|text| Find::new(&text, *number, focused));
                self.panels.files.open_at(workspace, path, (*number, *number), find);
            }
            Some(Found::File { .. }) | None => {}
        }
    }

    fn file_action(&mut self, target: &Checkout, action: Action, pos: Position, area: Rect) -> Result<()> {
        let workspace = target.workspace;
        let Some(viewer) = self.panels.files.viewer(workspace) else { return Ok(()) };
        let path = viewer.path.clone();
        let image = viewer.content.as_ref().is_some_and(|c| c.is_image());
        let selected = viewer.selected().filter(|_| !image);
        match action {
            Action::Open => {
                let line = selected.map_or(1, |(first, _)| first);
                return self.open_in_editor(target, &path, line, area);
            }
            Action::OpenIn => self.open_file_menu(workspace, pos),
            Action::Ask => {
                let reference = match selected {
                    Some((a, b)) if a == b => format!("{path}:{a} "),
                    Some((a, b)) => format!("{path}:{a}-{b} "),
                    None => format!("{path} "),
                };
                self.ask_agent(workspace, &reference);
            }
            Action::Copy => {
                let content = viewer.content.as_ref();
                let cut = viewer.stretched().and_then(|stretch| content?.cut(stretch));
                let text = cut.or_else(|| content.and_then(|c| c.text(selected))).unwrap_or(path);
                self.copy(&text);
            }
        }
        Ok(())
    }
}
