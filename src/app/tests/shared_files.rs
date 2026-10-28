use super::*;
use crate::app::merge_check::MergeAnswer;
use crate::app::overlaps::SCAN_EVERY;
use crate::changes::merge::Checked;
use crate::project::Phase;
use crate::test_util::{Checkouts, git};

const LINES: &str = "one\ntwo\nthree\n";

struct Sharing {
    app: App,
    rx: Receiver<AppEvent>,
    project: Checkouts,
    first: u64,
    second: u64,
}

fn write(dir: &Path, name: &str, text: &str) {
    std::fs::write(dir.join(name), text).expect("write file");
}

fn scanned(app: &mut App, rx: &Receiver<AppEvent>, now: Instant) {
    app.scan_overlaps(now);
    pump_until(app, rx, "the scan answers", |a| a.git.overlaps.scanning.is_empty());
}

fn sharing() -> Sharing {
    let mut project = Checkouts::new(&[("a.txt", LINES), ("b.txt", LINES)]);
    let (first, second) = (project.add("w1", "main"), project.add("w2", "main"));
    write(&first, "a.txt", "one\nfirst\nthree\n");
    write(&second, "a.txt", "one\nsecond\nthree\n");
    let (mut app, rx) = app_in(project.repo.path(), no_config());
    app.config.shared_files = true;
    app.sync_worktrees();
    let ids: Vec<u64> = app.projects[0].workspaces.iter().map(|w| w.id).collect();
    assert_eq!(ids.len(), 3);
    scanned(&mut app, &rx, Instant::now());
    Sharing { app, rx, project, first: ids[1], second: ids[2] }
}

fn shared_in_row(app: &App, workspace: u64) -> usize {
    app.workspace_entry(app.projects.workspace_by(workspace).expect("workspace")).shared
}

fn badge(app: &App, w: usize) -> Position {
    let areas = app_areas(app);
    let entry = app.workspace_entry(&app.projects[0].workspaces[w]);
    let tabs = app.tab_lines();
    let layout = ui::workspace_layout(areas.workspaces_list, areas.pitch, &tabs, 0);
    ui::workspace_tag(&layout, areas.pitch, &tabs, w, (&entry, ui::Tag::Shared)).expect("the shared tag").as_position()
}

fn notes(app: &App) -> Vec<String> {
    let Some(Overlay::MergeCheck { workspace, scroll }) = app.overlay else { panic!("the dialog is not open") };
    let Some(ui::Overlay::Update(update)) = app.merge_check_view(workspace, scroll, AREA) else { panic!("no view") };
    update.notes.iter().map(ToString::to_string).collect()
}

fn checked(app: &mut App, rx: &Receiver<AppEvent>) {
    pump_until(app, rx, "the merge check answers", |a| !a.panels.merges.running);
}

mod the_scan {
    use super::*;

    #[test]
    fn counts_the_files_two_worktrees_both_change() {
        let mut found = sharing();
        write(&found.project.add("w3", "main"), "b.txt", "other\n");
        found.app.sync_worktrees();
        scanned(&mut found.app, &found.rx, Instant::now() + SCAN_EVERY);

        let third = found.app.projects[0].workspaces[3].id;
        let main = found.app.projects[0].workspaces[0].id;
        assert_eq!([found.first, found.second, third, main].map(|id| shared_in_row(&found.app, id)), [1, 1, 0, 0]);
    }

    #[test]
    fn stops_while_the_setting_is_off() {
        let mut found = sharing();
        found.app.config.shared_files = false;

        found.app.scan_overlaps(Instant::now() + SCAN_EVERY);

        assert!(found.app.git.overlaps.scanning.is_empty());
        assert_eq!(shared_in_row(&found.app, found.first), 0);
    }

    #[test]
    fn drops_a_count_at_once_when_the_other_workspace_starts_going_away() {
        let mut found = sharing();
        assert_eq!(shared_in_row(&found.app, found.first), 1);

        found.app.projects.workspace_by_mut(found.second).expect("second").phase = Phase::Removing("w2".into());

        assert_eq!(shared_in_row(&found.app, found.first), 0);
        assert_eq!(found.app.shared_with(found.first).len(), 0);
    }

    #[test]
    fn leaves_a_project_with_one_checkout_alone() {
        let repo = repo_with_edit();
        let (mut app, _rx) = app_in(repo.path(), no_config());

        app.scan_overlaps(Instant::now());

        assert_eq!((app.git.overlaps.scanning.len(), app.git.overlaps.scans.len()), (0, 0));
    }

    #[test]
    fn waits_after_an_answer_before_scanning_again() {
        let mut found = sharing();

        found.app.scan_overlaps(Instant::now());
        assert!(found.app.git.overlaps.scanning.is_empty());

        found.app.scan_overlaps(Instant::now() + SCAN_EVERY);
        assert_eq!(found.app.git.overlaps.scanning.len(), 1);
        pump_until(&mut found.app, &found.rx, "the scan answers", |a| a.git.overlaps.scanning.is_empty());
    }
}

mod the_badge {
    use super::*;

    #[test]
    fn shows_on_both_rows() {
        let mut found = sharing();

        assert!(pull_requests::row_text(&mut found.app, WorkspaceRow::Workspace(1)).contains("⇄1"));
        assert!(pull_requests::row_text(&mut found.app, WorkspaceRow::Workspace(2)).contains("⇄1"));
    }

    #[test]
    fn opens_the_dialog_without_selecting_the_workspace() {
        let mut found = sharing();
        let active = found.app.project().and_then(Project::workspace).map(|w| w.id);
        let at = badge(&found.app, 2);

        click(&mut found.app, at);

        assert!(matches!(found.app.overlay, Some(Overlay::MergeCheck { workspace, .. }) if workspace == found.second));
        assert_eq!(found.app.project().and_then(Project::workspace).map(|w| w.id), active);
    }

    #[test]
    fn closes_the_compact_menu_when_pressed() {
        let mut found = sharing();
        found.app.screen.nav = Some(ui::Nav::Workspaces);

        found.app.open_merge_check(found.first);

        assert!(found.app.screen.nav.is_none());
    }
}

mod the_menu {
    use super::*;

    #[test]
    fn offers_a_merge_check_for_a_git_workspace() {
        let mut found = sharing();

        right_click_row(&mut found.app, WorkspaceRow::Workspace(1));

        assert_eq!(&menu_labels(&found.app)[..2], ["rename workspace", "merge check"]);
    }

    #[test]
    fn offers_none_for_a_plain_folder() {
        let plain = TempDir::new();
        let (mut app, _rx) = app_in(plain.path(), no_config());

        right_click_row(&mut app, WorkspaceRow::Workspace(0));

        assert!(!menu_labels(&app).contains(&"merge check".to_string()));
    }
}

mod the_dialog {
    use super::*;

    #[test]
    fn checks_when_it_opens() {
        let mut found = sharing();

        found.app.open_merge_check(found.first);
        assert!(notes(&found.app).iter().any(|line| line.contains("checking…")));
        checked(&mut found.app, &found.rx);

        let lines = notes(&found.app);
        assert!(lines.iter().any(|line| line.contains("into main")), "{lines:?}");
        assert!(lines.iter().any(|line| line.contains("✓ merges cleanly")), "{lines:?}");
        assert!(lines.iter().any(|line| line.contains("with w2 · 1 shared file")), "{lines:?}");
        assert!(lines.iter().any(|line| line.contains("! 1 conflict")), "{lines:?}");
        assert!(lines.iter().any(|line| line.contains("a.txt · contents")), "{lines:?}");
    }

    #[test]
    fn says_the_other_workspaces_were_not_looked_at_while_the_setting_is_off() {
        let mut found = sharing();
        found.app.config.shared_files = false;

        found.app.open_merge_check(found.first);

        let lines = notes(&found.app);
        assert!(lines.iter().any(|line| line.contains("Shared files are off (Settings › Worktrees)")), "{lines:?}");
        assert!(!lines.iter().any(|line| line.contains("No other workspace")), "{lines:?}");
    }

    #[test]
    fn checks_again_and_drops_an_older_answer() {
        let mut found = sharing();
        found.app.open_merge_check(found.first);
        checked(&mut found.app, &found.rx);
        let older = found.app.panels.merges.generation;

        found.app.check_merges(found.first);
        let newer = found.app.panels.merges.generation;
        found.app.merge_checked(MergeAnswer::Dialog(older), found.first, Checked::default());

        assert_eq!(newer, older + 1);
        assert!(found.app.panels.merges.running && found.app.panels.merges.shown.is_none());
        checked(&mut found.app, &found.rx);
        assert!(found.app.panels.merges.shown.is_some());
    }

    #[test]
    fn closes_when_its_workspace_goes_away() {
        let mut found = sharing();
        found.app.open_merge_check(found.first);

        found.app.projects.workspace_by_mut(found.first).expect("first").phase = Phase::Closing;
        found.app.scan_overlaps(Instant::now());

        assert!(found.app.overlay.is_none());
    }
}

mod the_changes_panel {
    use super::*;
    use crate::changes::Scope;

    fn shared_marks(found: &mut Sharing, scope: Scope) -> Vec<(String, bool)> {
        found.app.screen.side = Some(Side::Changes);
        found.app.panels.changes.set_scope(scope);
        found.app.refresh_changes(Instant::now());
        pump_until(&mut found.app, &found.rx, "the panel has files", |a| {
            let Some(view) = a.panel_view() else { return false };
            matches!(&view.body, changes::Body::Ready(diff) if !diff.files.is_empty()) && !view.shared.is_empty()
        });
        let view = found.app.panel_view().expect("panel");
        let changes::Body::Ready(diff) = view.body else { panic!("ready") };
        diff.files.iter().map(|f| f.path.clone()).zip(view.shared).collect()
    }

    fn select(found: &mut Sharing, workspace: u64) {
        let at = found.app.projects.workspace_index(workspace).expect("workspace");
        found.app.projects[at.project()].active = at.index();
    }

    #[test]
    fn marks_the_shared_file_in_the_project_scope() {
        let mut found = sharing();
        let first = found.first;
        select(&mut found, first);
        let first_dir = found.app.projects.workspace_by(first).expect("first").path.clone();
        write(&first_dir, "only-here.txt", "x\n");

        let marks = shared_marks(&mut found, Scope::Project);

        assert_eq!(marks, [("a.txt".to_string(), true), ("only-here.txt".to_string(), false)]);
    }

    #[test]
    fn marks_a_renamed_file_when_its_old_path_is_shared() {
        let mut found = sharing();
        let first = found.first;
        select(&mut found, first);
        let first_dir = found.app.projects.workspace_by(first).expect("first").path.clone();
        git(&first_dir, &["mv", "a.txt", "c.txt"]);

        let marks = shared_marks(&mut found, Scope::Project);

        assert!(marks.iter().any(|(path, shared)| path == "c.txt" && *shared), "{marks:?}");
    }

    #[test]
    fn marks_the_shared_file_in_every_project() {
        let mut found = sharing();

        let marks = shared_marks(&mut found, Scope::Every);

        assert!(marks.iter().all(|(path, shared)| *shared == (path == "a.txt")), "{marks:?}");
        assert!(marks.iter().any(|(_, shared)| *shared));
    }
}
