use ratatui::style::{Color, Modifier, Style};

use super::*;
use crate::changes::diff::{self, File, Fold, Hunk, Kind};
use crate::changes::{Body, GapLine, Mode, Scope};
use crate::control::changes::{
    Content, FileInfo, FileStatus, FoldClass, Gap, Patch, Staging, State as Listing, Summary, WorkspaceChanges, digest,
};
use crate::control::contract::{Code, Refusal, Structured};
use crate::protocol::{self, ServerMessage};
use crate::test_util::git_repo;
use crate::ui::changes as panel;

const BASE: &str = "origin/main";

fn info(path: &str, version: u64) -> FileInfo {
    FileInfo {
        path: path.into(),
        status: FileStatus::Modified,
        added: 2,
        removed: 1,
        digest: digest(version),
        ..FileInfo::default()
    }
}

fn summary(mode: Mode, files: Vec<FileInfo>) -> Summary {
    let workspace =
        WorkspaceChanges { project: 1, workspace: SHOP, base: Some(BASE.into()), files, ..Default::default() };
    let content =
        Content { scope: Scope::Project, mode, state: Listing::Ready, workspaces: vec![workspace], error: None };
    Summary { boot: BOOT.into(), content, ..Summary::default() }
}

fn refuse(stream: &mut UnixStream, code: Code, message: &str) {
    let text = serde_json::to_string(&Structured::Error(Refusal::new(code, message))).expect("a refusal");
    protocol::send(stream, &ServerMessage::Response(text)).expect("refuse the panel");
}

fn line(kind: Kind, n: u32, text: &str) -> diff::Line {
    let old = (kind != Kind::Added).then_some(n);
    let new = (kind != Kind::Removed).then_some(n);
    diff::Line { kind, old, new, text: text.into(), raw: None, syntax: None, emphasis: Vec::new() }
}

fn highlighted() -> Style {
    Style::new().fg(Color::Rgb(200, 100, 50)).add_modifier(Modifier::BOLD)
}

fn patched(path: &str, version: u64) -> File {
    let mut changed = line(Kind::Added, 2, "fn shop() {}");
    changed.syntax = Some(vec![("fn".into(), highlighted()), (" shop() {}".into(), Style::default())]);
    changed.emphasis = vec![0..2, 3..7];
    let first = Hunk {
        old_start: 1,
        new_start: 1,
        context: String::new(),
        lines: vec![line(Kind::Context, 1, "    use std;"), changed],
    };
    let second =
        Hunk { old_start: 30, new_start: 30, context: String::new(), lines: vec![line(Kind::Removed, 30, "}")] };
    let file = info(path, version).file();
    File { hunks: vec![first, second], fold: Fold::Open, ..file }
}

fn files(app: &App) -> Vec<Arc<File>> {
    match app.panel_view().map(|view| view.body) {
        Some(Body::Ready(diff)) => diff.files.clone(),
        _ => Vec::new(),
    }
}

fn paths(app: &App) -> Vec<String> {
    files(app).iter().map(|file| file.path.clone()).collect()
}

fn hunks(app: &App, path: &str) -> usize {
    files(app).iter().find(|file| file.path == path).map_or(0, |file| file.hunks.len())
}

fn at(app: &App, wanted: panel::Row) -> Position {
    let view = app.panel_view().expect("the panel");
    let body = panel::parts(panel_area(app), &view).body;
    let row = panel::rows(&view, body.width).iter().position(|row| *row == wanted).expect("a row");
    Position::new(body.x + 12, body.y + u16::try_from(row).expect("a row"))
}

fn hunk_actions(app: &App, view: &panel::View) -> Vec<(HunkAction, Rect)> {
    let row = Rect::new(panel_area(app).x, at(app, panel::Row::Hunk(0, 0)).y, panel_area(app).width, 1);
    panel::actions(view, row)
}

fn folded(app: &App) -> Vec<bool> {
    app.panel_view().expect("the panel").folded
}

impl Panels {
    fn listing(files: Vec<FileInfo>) -> (Self, UnixStream) {
        let (mut panels, mut stream) = Self::showing(Side::Changes);
        request(&mut stream);
        let wanted: Vec<String> = files.iter().map(|file| file.path.clone()).collect();
        tell(&mut stream, &summary(Mode::Uncommitted, files));
        panels.rig.until("the machine's files show", |app| paths(app) == wanted);
        (panels, stream)
    }

    fn unfold(&mut self, path: &str) {
        let file = files(&self.rig.app).into_iter().find(|file| file.path == path).expect("a listed file");
        self.rig.app.changes_state_mut().set_fold(SHOP, &file, false);
    }

    fn asked_patch(&mut self) -> (UnixStream, Value) {
        let mut asking = self.opened();
        let asked = asked(&mut asking);
        (asking, asked)
    }

    fn with_hunks() -> (Self, UnixStream) {
        let (mut panels, stream) = Self::listing(vec![info("a.rs", 1)]);
        panels.unfold("a.rs");
        let (mut asking, _) = panels.asked_patch();
        tell(&mut asking, &Patch::of(SHOP, Mode::Uncommitted, &patched("a.rs", 1), false));
        panels.rig.until("the hunks show", |app| hunks(app, "a.rs") == 2);
        (panels, stream)
    }
}

fn unanswered(_panels: &mut Panels, asking: UnixStream) {
    drop(asking);
}

fn too_late(panels: &mut Panels, _asking: UnixStream) {
    panels.clock += 60;
    panels.sync();
}

fn patch_of(path: &str) -> Value {
    json!({ "command": "changes-file", "boot": BOOT, "args": { "workspace": SHOP, "path": path, "mode": "uncommitted" } })
}

mod the_remote_list {
    use super::*;

    #[test]
    fn shows_the_files_of_the_machine_and_not_those_of_this_computer() {
        let repo = git_repo(&[("here.txt", "one\n")]);
        std::fs::write(repo.path().join("here.txt"), "two\n").expect("edit here");
        let mut panels = Panels::linked();
        panels.rig.app.open_project(repo.path().to_path_buf(), AREA).expect("open the local copy");
        panels.pick(SHOP);
        panels.rig.app.show_side(Some(Side::Changes));
        let mut stream = panels.opened();
        request(&mut stream);

        tell(&mut stream, &summary(Mode::Uncommitted, vec![info("there.rs", 1)]));

        panels.rig.until("the machine's file shows", |app| paths(app) == ["there.rs"]);
        let view = panels.rig.app.panel_view().expect("the panel");
        assert_eq!((view.project.as_str(), view.base, view.live), ("web › web-shop · vps", None, true));
        assert_eq!(view.remote, Some(ui::RemoteNote { machine: "vps".into(), down: None }));
        local(&mut panels);
        assert_eq!(panels.rig.app.panel_view().expect("the local panel").remote, None);
    }

    #[test]
    fn keeps_what_the_machine_says_of_each_file() {
        let renamed = FileInfo { old_path: Some("old.rs".into()), status: FileStatus::Renamed, ..info("new.rs", 1) };
        let lock = FileInfo { fold: FoldClass::Lockfile, ..info("Cargo.lock", 2) };
        let binary = FileInfo { fold: FoldClass::Binary, added: 0, removed: 0, ..info("logo.png", 3) };
        let large = FileInfo { fold: FoldClass::Large, added: 9000, removed: 0, ..info("dump.sql", 4) };
        let (mut panels, _stream) = Panels::listing(vec![renamed, lock, binary, large]);

        let seen: Vec<_> = files(&panels.rig.app)
            .iter()
            .map(|f| (f.path.clone(), f.old_path.clone(), f.status, f.added, f.removed, f.fold))
            .collect();

        assert_eq!(
            seen,
            [
                ("new.rs".into(), Some("old.rs".into()), diff::Status::Renamed, 2, 1, Fold::Open),
                ("Cargo.lock".into(), None, diff::Status::Modified, 2, 1, Fold::Lockfile),
                ("logo.png".into(), None, diff::Status::Modified, 0, 0, Fold::Binary),
                ("dump.sql".into(), None, diff::Status::Modified, 9000, 0, Fold::Large),
            ]
        );
        let drawn = render(&mut panels.rig.app, Rect::new(0, 0, 200, 20)).backend().to_string();
        for shown in ["old.rs → new.rs", "lockfile", "binary", "too large"] {
            assert!(drawn.contains(shown), "{shown} in\n{drawn}");
        }
    }

    #[test]
    fn starts_folded_and_shows_the_marks_of_the_machine() {
        let viewed = FileInfo { viewed: true, ..info("seen.rs", 2) };
        let (panels, _stream) = Panels::listing(vec![info("a.rs", 1), viewed]);

        let view = panels.rig.app.panel_view().expect("the panel");

        assert_eq!((view.folded, view.viewed, view.shared), (vec![true, true], vec![false, true], vec![false, false]));
    }

    #[test]
    fn is_replaced_by_each_new_summary() {
        let (mut panels, mut stream) = Panels::listing(vec![info("a.rs", 1)]);

        tell(&mut stream, &summary(Mode::Uncommitted, vec![info("b.rs", 1), info("c.rs", 1)]));

        panels.rig.until("the new list shows", |app| paths(app) == ["b.rs", "c.rs"]);
    }

    #[test]
    fn shows_why_the_machine_could_not_read_it() {
        let (mut panels, mut stream) = Panels::listing(vec![info("a.rs", 1)]);
        let mut failed = summary(Mode::Uncommitted, Vec::new());
        failed.content.state = Listing::Failed;
        failed.content.error = Some("not a git repository".into());

        tell(&mut stream, &failed);

        panels.rig.until("the error shows", |app| {
            app.panel_view().is_some_and(|view| view.body == Body::Failed("not a git repository".into()))
        });
    }

    #[test]
    fn of_another_boot_is_not_shown_any_longer() {
        let (mut panels, mut stream) = Panels::listing(vec![info("a.rs", 1)]);

        refuse(&mut stream, Code::StaleBoot, "the server restarted");

        panels.rig.until("the old list goes", |app| paths(app).is_empty());
    }

    #[test]
    fn follows_the_mode_with_a_new_stream() {
        let (mut panels, mut stream) = Panels::listing(vec![info("a.rs", 1)]);

        panels.rig.app.changes_state_mut().set_mode(Mode::Commits);
        let mut commits = panels.opened();

        assert_eq!(asked(&mut commits)["args"]["mode"], "commits");
        assert!(ended(&mut stream));
        tell(&mut commits, &summary(Mode::Commits, vec![info("old.rs", 5)]));
        panels.rig.until("the commits show", |app| paths(app) == ["old.rs"]);
        assert_eq!(panels.rig.app.panel_view().and_then(|view| view.base).as_deref(), Some(BASE));
    }

    #[test]
    fn is_filtered_like_a_local_one() {
        let (mut panels, _stream) = Panels::listing(vec![info("src/a.rs", 1), info("Cargo.lock", 2)]);

        panels.rig.app.changes_state_mut().focus_filter();
        "!*.lock".chars().for_each(|c| panels.rig.app.changes_state_mut().push_filter(c));

        let filter = panels.rig.app.panel_view().and_then(|view| view.filter).expect("the filter");
        assert_eq!(filter.kept, [true, false]);
    }

    #[test]
    fn keeps_apart_the_folds_and_the_filter_of_the_local_workspace_with_the_same_id() {
        let (mut panels, _stream) = Panels::listing(vec![info("a.rs", 1)]);
        let file = files(&panels.rig.app)[0].clone();
        panels.rig.app.panels.changes.set_fold(SHOP, &file, false);
        panels.rig.app.panels.changes.focus_filter();
        panels.rig.app.panels.changes.push_filter('z');

        let view = panels.rig.app.panel_view().expect("the panel");

        assert_eq!((view.folded, view.filter), (vec![true], None));
    }
}

mod a_remote_patch {
    use super::*;

    #[test]
    fn is_asked_when_a_file_unfolds_and_shows_its_hunks_as_the_machine_drew_them() {
        let (mut panels, _stream) = Panels::listing(vec![info("a.rs", 1)]);
        panels.unfold("a.rs");

        let (mut asking, asked) = panels.asked_patch();
        assert_eq!(asked, patch_of("a.rs"));
        tell(&mut asking, &Patch::of(SHOP, Mode::Uncommitted, &patched("a.rs", 1), false));

        panels.rig.until("the hunks show", |app| hunks(app, "a.rs") == 2);
        let file = files(&panels.rig.app)[0].clone();
        let shown = &file.hunks[0].lines[1];
        assert_eq!(shown.emphasis, [0..2, 3..7]);
        let (text, look) = shown.syntax.as_ref().map(|syntax| syntax[0].clone()).expect("highlighted");
        assert_eq!((text.as_str(), look.fg, look.add_modifier), ("fn", highlighted().fg, Modifier::BOLD));
    }

    #[test]
    fn is_asked_one_at_a_time() {
        let (mut panels, _stream) = Panels::listing(vec![info("a.rs", 1), info("b.rs", 1)]);
        panels.unfold("a.rs");
        panels.unfold("b.rs");

        let (mut first, asked) = panels.asked_patch();
        panels.sync();
        assert_eq!((asked, panels.streams()), (patch_of("a.rs"), 2));

        tell(&mut first, &Patch::of(SHOP, Mode::Uncommitted, &patched("a.rs", 1), false));
        panels.rig.until("the first patch shows", |app| hunks(app, "a.rs") == 2);
        let (_second, asked) = panels.asked_patch();
        assert_eq!(asked, patch_of("b.rs"));
    }

    #[test]
    fn is_not_asked_for_a_folded_or_filtered_file() {
        let (mut panels, _stream) = Panels::listing(vec![info("a.rs", 1), info("Cargo.lock", 2)]);
        panels.unfold("Cargo.lock");
        panels.rig.app.changes_state_mut().focus_filter();
        "!*.lock".chars().for_each(|c| panels.rig.app.changes_state_mut().push_filter(c));

        panels.sync();

        assert_eq!(panels.streams(), 1);
    }

    #[test]
    fn is_asked_again_when_its_digest_changes() {
        let (mut panels, mut stream) = Panels::listing(vec![info("a.rs", 1)]);
        panels.unfold("a.rs");
        let (mut asking, _) = panels.asked_patch();
        tell(&mut asking, &Patch::of(SHOP, Mode::Uncommitted, &patched("a.rs", 1), false));
        panels.rig.until("the hunks show", |app| hunks(app, "a.rs") == 2);
        panels.sync();
        assert_eq!(panels.streams(), 2);

        tell(&mut stream, &summary(Mode::Uncommitted, vec![info("a.rs", 2)]));
        panels.rig.until("the new digest arrives", |app| hunks(app, "a.rs") == 0);

        let (_again, asked) = panels.asked_patch();
        assert_eq!(asked, patch_of("a.rs"));
    }

    #[rstest]
    #[case::that_ended_without_an_answer(unanswered)]
    #[case::that_took_too_long(too_late)]
    fn is_asked_again_once_its_file_is_folded_and_unfolded_by_hand(#[case] fail: fn(&mut Panels, UnixStream)) {
        let (mut panels, _stream) = Panels::listing(vec![info("a.rs", 1)]);
        let row = at(&panels.rig.app, panel::Row::File(0));
        click(&mut panels.rig.app, row);
        let (asking, _) = panels.asked_patch();
        fail(&mut panels, asking);
        panels.sync();
        assert_eq!(panels.streams(), 2);

        click(&mut panels.rig.app, row);
        click(&mut panels.rig.app, row);
        let (_again, asked) = panels.asked_patch();

        assert_eq!(asked, patch_of("a.rs"));
    }

    #[test]
    fn shown_is_not_asked_again_when_its_file_is_folded_and_unfolded() {
        let (mut panels, _stream) = Panels::with_hunks();
        let row = at(&panels.rig.app, panel::Row::File(0));

        click(&mut panels.rig.app, row);
        click(&mut panels.rig.app, row);
        panels.sync();

        assert_eq!((folded(&panels.rig.app), panels.streams()), (vec![false], 2));
    }

    #[test]
    fn takes_the_marks_of_each_new_summary_with_the_same_digest() {
        let (mut panels, mut stream) = Panels::with_hunks();

        let staged = FileInfo { staged: Some(Staging::Yes), ..info("a.rs", 1) };
        tell(&mut stream, &summary(Mode::Uncommitted, vec![staged]));

        panels.rig.until("the new mark shows", |app| files(app)[0].staged == Some(diff::Staged::Yes));
        assert_eq!(hunks(&panels.rig.app, "a.rs"), 2);
    }

    #[test]
    fn of_a_mode_left_behind_is_dropped() {
        let (mut panels, _stream) = Panels::listing(vec![info("a.rs", 1)]);
        panels.unfold("a.rs");
        let (mut asking, _) = panels.asked_patch();

        panels.rig.app.changes_state_mut().set_mode(Mode::Commits);
        panels.opened();

        assert!(ended(&mut asking));
    }

    #[test]
    fn of_another_boot_clears_the_list() {
        let (mut panels, _stream) = Panels::listing(vec![info("a.rs", 1)]);
        panels.unfold("a.rs");
        let (mut asking, _) = panels.asked_patch();

        refuse(&mut asking, Code::StaleBoot, "the server restarted");

        panels.rig.until("the old list goes", |app| paths(app).is_empty());
    }

    #[test]
    fn that_takes_too_long_is_given_up_with_a_toast() {
        let (mut panels, _stream) = Panels::listing(vec![info("a.rs", 1)]);
        panels.unfold("a.rs");
        let (mut asking, _) = panels.asked_patch();

        panels.clock += 60;
        panels.sync();

        assert!(ended(&mut asking));
        assert_eq!(toast(&panels.rig.app), Some("vps did not answer in time"));
    }
}

mod a_remote_gap {
    use super::*;

    #[test]
    fn opens_with_the_lines_the_machine_reads() {
        let (mut panels, _stream) = Panels::with_hunks();

        let gap = at(&panels.rig.app, panel::Row::Gap(0, 1));
        click(&mut panels.rig.app, gap);
        let mut gap = taken(&panels.ends);

        let args = json!({ "workspace": SHOP, "path": "a.rs", "hunk": 1, "mode": "uncommitted" });
        assert_eq!(asked(&mut gap), json!({ "command": "changes-gap", "boot": BOOT, "args": args }));
        let lines = [GapLine { old: 3, new: 3, text: "let x = 1;".into(), raw: None, syntax: None }];
        tell(&mut gap, &Gap::of(SHOP, "a.rs", 1, &lines));
        panels.rig.until("the unchanged lines show", |app| {
            app.panel_view()
                .is_some_and(|view| view.gaps.get(&(0, 1)).is_some_and(|shown| shown[0].text == "let x = 1;"))
        });
    }
}

mod the_remote_panel_by_hand {
    use super::*;

    #[test]
    fn unfolds_a_file_with_a_click_on_its_row_and_asks_its_patch() {
        let (mut panels, _stream) = Panels::listing(vec![info("a.rs", 1)]);

        let row = at(&panels.rig.app, panel::Row::File(0));
        click(&mut panels.rig.app, row);

        assert_eq!(folded(&panels.rig.app), [false]);
        let (_asking, asked) = panels.asked_patch();
        assert_eq!(asked, patch_of("a.rs"));
    }

    #[test]
    fn expands_and_collapses_every_file() {
        let (mut panels, _stream) = Panels::listing(vec![info("a.rs", 1), info("b.rs", 1)]);
        let every = |app: &App| panel::fold_all(panel_area(app), &app.panel_view().expect("the panel")).as_position();

        let expand = every(&panels.rig.app);
        click(&mut panels.rig.app, expand);
        assert_eq!(folded(&panels.rig.app), [false, false]);

        let collapse = every(&panels.rig.app);
        click(&mut panels.rig.app, collapse);
        assert_eq!(folded(&panels.rig.app), [true, true]);
    }

    #[test]
    fn copies_a_hunk_to_the_clipboard_of_this_computer() {
        let (mut panels, _stream) = Panels::with_hunks();
        let app = &mut panels.rig.app;
        let actions = hunk_actions(app, &app.panel_view().expect("the panel"));
        assert_eq!(actions.iter().map(|(action, _)| *action).collect::<Vec<_>>(), [HunkAction::Copy]);

        click(app, actions[0].1.as_position());

        let copied = clipboard::osc52("@@ -1,1 +1,2 @@\n     use std;\n+fn shop() {}\n");
        assert_eq!((app.take_host_writes(), toast(app)), (vec![copied], Some(COPIED)));
    }

    #[test]
    fn starts_no_editor_nor_pane_with_a_click_where_open_would_be() {
        let (mut panels, _stream) = Panels::with_hunks();
        let app = &mut panels.rig.app;
        app.shells.set_editor("true");
        let tabs = |app: &App| app.projects.iter().flat_map(|p| &p.workspaces).map(|w| w.tabs.len()).sum::<usize>();
        let before = tabs(app);
        let local = panel::View { remote: None, ..app.panel_view().expect("the panel") };
        let (action, open) = hunk_actions(app, &local)[0];
        assert_eq!(action, HunkAction::Open);

        click(app, open.as_position());

        assert_eq!(
            (tabs(app), app.overlay.is_none(), app.take_host_writes(), toast(app)),
            (before, true, vec![], None)
        );
    }

    #[test]
    fn takes_the_keys_in_its_filter() {
        let (mut panels, _stream) = Panels::listing(vec![info("src/a.rs", 1), info("Cargo.lock", 2)]);
        let app = &mut panels.rig.app;

        click(app, panel::filter_button(panel_area(app)).as_position());
        type_text(app, "!*.lock");

        let filter = app.panel_view().and_then(|view| view.filter).expect("the filter");
        assert_eq!((filter.query.as_str(), filter.kept), ("!*.lock", vec![true, false]));
    }

    #[test]
    fn closes_with_esc() {
        let (mut panels, mut stream) = Panels::listing(vec![info("a.rs", 1)]);

        send_key(&mut panels.rig.app, KeyCode::Esc, KeyModifiers::NONE);
        panels.sync();

        assert_eq!(panels.rig.app.screen.side, None);
        assert!(ended(&mut stream));
    }

    #[test]
    fn shows_the_projects_of_this_computer_on_the_every_project_tab() {
        let repo = git_repo(&[("here.txt", "one\n")]);
        std::fs::write(repo.path().join("here.txt"), "two\n").expect("edit here");
        let mut panels = Panels::linked();
        panels.rig.app.open_project(repo.path().to_path_buf(), AREA).expect("open a project here");
        panels.pick(SHOP);
        panels.rig.app.show_side(Some(Side::Changes));
        let mut stream = panels.opened();
        request(&mut stream);
        let tab = |app: &App, scope: Scope| {
            let view = app.panel_view().expect("the panel");
            let tabs = panel::scopes(panel_area(app), &view);
            tabs.into_iter().find(|(shown, ..)| *shown == scope).map(|(_, r, _)| r.as_position()).expect("a tab")
        };

        let every = tab(&panels.rig.app, Scope::Every);
        click(&mut panels.rig.app, every);
        panels.rig.until("this computer's file shows", |app| paths(app) == ["here.txt"]);
        let row = at(&panels.rig.app, panel::Row::File(0));
        click(&mut panels.rig.app, row);
        let view = panels.rig.app.panel_view().expect("the panel");
        assert_eq!((view.scope, view.remote, view.folded), (Scope::Every, None, vec![false]));
        panels.sync();
        assert!(ended(&mut stream));

        let project = tab(&panels.rig.app, Scope::Project);
        click(&mut panels.rig.app, project);
        let mut again = panels.opened();
        assert_eq!(asked(&mut again)["command"], "changes");
        assert!(panels.rig.app.panel_view().expect("the panel").remote.is_some());
    }
}

mod a_fallen_machine {
    use super::*;

    #[test]
    fn keeps_the_list_says_so_and_sends_nothing_for_an_unfold() {
        let (mut panels, mut stream) = Panels::listing(vec![info("a.rs", 1)]);

        panels.fall();

        assert!(ended(&mut stream));
        let down = panels.down().expect("the note");
        assert!(down.starts_with("reconnecting"), "{down}");
        let row = at(&panels.rig.app, panel::Row::File(0));
        click(&mut panels.rig.app, row);
        panels.sync();
        assert_eq!(
            (paths(&panels.rig.app), folded(&panels.rig.app), panels.streams()),
            (vec!["a.rs".into()], vec![true], 1)
        );
        assert_eq!(toast(&panels.rig.app), Some("vps unreachable; nothing was sent"));
    }

    #[test]
    fn asks_no_unchanged_lines() {
        let (mut panels, _stream) = Panels::with_hunks();
        panels.fall();

        let gap = at(&panels.rig.app, panel::Row::Gap(0, 1));
        click(&mut panels.rig.app, gap);

        assert_eq!((panels.streams(), toast(&panels.rig.app)), (2, Some("vps unreachable; nothing was sent")));
    }

    #[test]
    fn resumes_once_back_without_losing_the_list() {
        let (mut panels, _stream) = Panels::listing(vec![info("a.rs", 1)]);
        panels.fall();

        panels.back();
        let mut again = panels.opened();

        assert_eq!(asked(&mut again)["command"], "changes");
        assert_eq!((paths(&panels.rig.app), panels.down()), (vec!["a.rs".to_string()], None));
    }
}

mod the_changes_button {
    use super::*;

    fn told(panels: &mut Panels, edit: impl Fn(&mut WorkspaceInfo)) {
        let mut report = holding();
        report.projects[0].workspaces.iter_mut().for_each(edit);
        let wanted = report.projects[0].workspaces.clone();
        serve(&mut panels.link, 2, report);
        panels.rig.until("the new snapshot arrives", |app| {
            app.machines.held()[0].seen.as_ref().is_some_and(|seen| seen.projects[0].workspaces == wanted)
        });
    }

    fn counted(files: u64, mode: Mode) -> impl Fn(&mut WorkspaceInfo) {
        move |workspace| workspace.changes = Some(control::ChangesCount { files, added: 0, removed: 0, mode })
    }

    #[test]
    fn has_no_count_until_the_machine_gives_one() {
        let mut panels = Panels::linked();

        panels.pick(SHOP);

        assert_eq!(panels.rig.app.changes_label().as_deref(), Some("Changes"));
    }

    #[rstest]
    #[case::of_the_panel_mode(Mode::Uncommitted, Some("Changes 3"))]
    #[case::not_of_another_mode(Mode::All, Some("Changes"))]
    fn shows_the_count_the_machine_publishes(#[case] mode: Mode, #[case] label: Option<&str>) {
        let mut panels = Panels::linked();
        told(&mut panels, counted(3, mode));

        panels.pick(SHOP);

        assert_eq!(panels.rig.app.changes_label().as_deref(), label);
    }

    #[test]
    fn shows_the_count_of_the_open_stream_first() {
        let (mut panels, _stream) = Panels::listing(vec![info("a.rs", 1), info("b.rs", 1)]);

        told(&mut panels, counted(3, Mode::Uncommitted));

        assert_eq!(panels.rig.app.changes_label().as_deref(), Some("Changes 2"));
    }

    #[test]
    fn opens_with_its_key_in_the_keys_menu() {
        let mut panels = Panels::linked();
        panels.rig.app.config.prefix_key = "ctrl+]".into();
        panels.pick(SHOP);

        send_key(&mut panels.rig.app, KeyCode::Char(']'), KeyModifiers::CONTROL);
        send_key(&mut panels.rig.app, KeyCode::Char('g'), KeyModifiers::NONE);
        send_key(&mut panels.rig.app, KeyCode::Char('d'), KeyModifiers::NONE);

        let mut stream = panels.opened();
        assert_eq!((asked(&mut stream)["command"].as_str(), toast(&panels.rig.app)), (Some("changes"), None));
    }

    #[test]
    fn is_not_offered_outside_git_while_files_is() {
        let mut panels = Panels::linked();
        told(&mut panels, |workspace| workspace.branch = None);
        panels.pick(SHOP);

        assert_eq!(panels.rig.app.changes_label(), None);
        let files = panels.rig.app.layout(AREA).files_button.as_position();
        click(&mut panels.rig.app, files);
        assert!(panels.rig.app.files_view().is_some_and(|view| view.remote.is_some()));
    }
}
