use super::*;
use crate::test_util::{git, write_executable};

mod every_project {
    use super::*;

    fn filter_changes(app: &mut App, query: &str) {
        let mut filter = changes::filter::Filter::default();
        query.chars().for_each(|c| filter.push(c));
        app.changes.filter = Some(filter);
    }

    fn loaded_every(app: &mut App, rx: &Receiver<AppEvent>) {
        app.changes.open = true;
        app.changes.set_scope(changes::Scope::Every);
        app.refresh_changes(Instant::now());
        pump_until(app, rx, "every workspace has a summary", |a| {
            a.every_targets().iter().all(|(t, _)| a.changes.summary(t.workspace).is_some())
        });
    }

    fn row(app: &App, wanted: panel::Row) -> Position {
        let view = app.panel_view().expect("panel");
        let index = panel::rows(&view).iter().position(|r| *r == wanted).expect("row");
        let body = panel::parts(panel_area(app), &view).body;
        Position::new(body.x + 8, body.y + u16::try_from(index).expect("row index"))
    }

    fn open_file(app: &mut App, rx: &Receiver<AppEvent>, path: &str) -> usize {
        let view = app.panel_view().expect("panel");
        let panel::Body::Ready(diff) = view.body else { panic!("ready") };
        let file = diff.files.iter().position(|f| f.path == path).expect("file");
        click(app, row(app, panel::Row::File(file)));
        pump_until(app, rx, "the file patch loads", |a| {
            let Some(panel::View { body: panel::Body::Ready(d), .. }) = a.panel_view() else { return false };
            !d.files[file].hunks.is_empty()
        });
        file
    }

    #[test]
    fn groups_projects_and_changed_worktrees_and_leaves_out_clean_and_plain_folders() {
        let shop = repo_with_edit();
        let worktree = TempDir::new();
        let checkout = worktree.path().join("topic");
        git(shop.path(), &["worktree", "add", "--quiet", "-b", "topic", checkout.to_str().expect("path")]);
        std::fs::write(checkout.join("new.txt"), "new\n").expect("edit worktree");
        let api = repo_with_edit();
        let clean = git_repo(&[]);
        let plain = TempDir::new();
        let (mut app, rx) = app_in(shop.path(), no_config());
        for (path, name) in [(api.path(), "api"), (clean.path(), "clean"), (plain.path(), "notes")] {
            let p = app.add_project(path.to_path_buf(), AREA).expect("project");
            app.projects[p].name = Some(name.into());
        }
        app.projects[0].name = Some("shop".into());
        loaded_every(&mut app, &rx);
        let view = app.panel_view().expect("panel");
        assert_eq!(
            view.headers.iter().map(|h| (h.label.as_str(), h.workspace)).collect::<Vec<_>>(),
            [("shop", false), ("main", true), ("topic", true), ("api", false)]
        );
        assert_eq!(view.folded, [true; 3]);
        let panel::Body::Ready(diff) = view.body else { panic!("ready") };
        assert!(diff.files.iter().all(|f| f.hunks.is_empty()));
        let before = app.active;
        let header = row(&app, panel::Row::Header(0));
        click(&mut app, header);
        assert_eq!(app.active, before);
        assert_eq!(
            panel::rows(&app.panel_view().expect("panel")),
            [panel::Row::Header(0), panel::Row::Header(3), panel::Row::File(2)]
        );
        let header = row(&app, panel::Row::Header(0));
        click(&mut app, header);
        let sub = row(&app, panel::Row::Header(2));
        click(&mut app, sub);
        assert!(!panel::rows(&app.panel_view().expect("panel")).contains(&panel::Row::File(1)));
        filter_changes(&mut app, "new.txt");
        assert_eq!(panel::rows(&app.panel_view().expect("panel")), [panel::Row::Header(0), panel::Row::Header(2)]);
        filter_changes(&mut app, "a.txt");
        assert_eq!(
            panel::rows(&app.panel_view().expect("panel")),
            [
                panel::Row::Header(0),
                panel::Row::Header(1),
                panel::Row::File(0),
                panel::Row::Header(3),
                panel::Row::File(2)
            ]
        );
        git(shop.path(), &["checkout", "--", "a.txt"]);
        let main = app.projects[0].workspaces[0].id;
        let topic = app.projects[0].workspaces[1].id;
        app.changes.invalidate_summary(main);
        app.refresh_changes(Instant::now());
        pump_until(&mut app, &rx, "the clean workspace loses its sub-header", |a| {
            a.changes.summary(main).and_then(changes::Model::diff).is_some_and(|d| d.files.is_empty())
        });
        assert!(app.every_targets().iter().any(|(t, hidden)| t.workspace == topic && !hidden));
    }

    #[test]
    fn scope_and_mode_survive_a_restore_without_changing_the_session_version() {
        let repo = repo_with_edit();
        let (mut app, rx) = app_in(repo.path(), no_config());
        loaded_every(&mut app, &rx);
        app.changes.mode = changes::Mode::Commits;
        let saved = app.state();
        let (mut restored, _) = new_app(no_config());
        restored.restore(&saved, AREA);
        assert_eq!(
            (saved.version, restored.changes.scope, restored.changes.mode),
            (5, changes::Scope::Every, changes::Mode::Commits)
        );
        let mut old = serde_json::to_value(saved).expect("json");
        old["changes"].as_object_mut().expect("changes").remove("scope");
        let old: State = serde_json::from_value(old).expect("older session");
        restored.restore(&old, AREA);
        assert_eq!(restored.changes.scope, changes::Scope::Project);
    }

    #[test]
    fn a_clean_collection_has_no_headers_or_files() {
        let repo = git_repo(&[]);
        let (mut app, rx) = app_in(repo.path(), no_config());
        loaded_every(&mut app, &rx);
        let view = app.panel_view().expect("panel");
        assert_eq!(view.headers, Vec::new());
        assert_eq!(panel::rows(&view), Vec::new());
    }

    #[test]
    fn a_failed_scan_does_not_claim_the_collection_is_clean() {
        let repo = git_repo(&[]);
        let (mut app, _) = app_in(repo.path(), no_config());
        app.projects[0].name = Some("shop".into());
        app.changes.scope = changes::Scope::Every;
        let targets = app.every_targets();
        let (workspace, generation, request) = app.changes.request_every(&targets, Instant::now()).expect("summary");
        app.changes.loaded(workspace, generation, &request, Err(Error::Git("cannot read status".into())));
        assert_eq!(app.panel_view().expect("panel").body, panel::Body::Failed("shop: cannot read status".into()));
    }

    #[rstest::rstest]
    #[case::removing(Phase::Removing("main".into()))]
    #[case::closing(Phase::Closing)]
    fn only_open_workspaces_are_scanned_and_shown(#[case] phase: Phase) {
        let repo = repo_with_edit();
        let (mut app, rx) = app_in(repo.path(), no_config());
        loaded_every(&mut app, &rx);
        app.projects[0].workspaces[0].phase = phase;
        assert!(app.every_targets().is_empty());
        assert!(app.changes_target().is_none());
        let view = app.panel_view().expect("panel");
        assert!(view.owners.is_empty() && view.headers.is_empty());
        assert!(matches!(view.body, panel::Body::Ready(d) if d.files.is_empty()));
    }

    #[rstest::rstest]
    #[case::closed("closed")]
    #[case::removed("removed")]
    #[case::removed_outside_the_app("external")]
    fn a_departing_workspace_is_forgotten(#[case] action: &str) {
        let repo = repo_with_edit();
        let tree = TempDir::new();
        let path = tree.path().join("topic");
        git(repo.path(), &["worktree", "add", "--quiet", "-b", "topic", path.to_str().expect("path")]);
        std::fs::write(path.join("new.txt"), "new\n").expect("edit worktree");
        let (mut app, rx) = app_in(repo.path(), no_config());
        loaded_every(&mut app, &rx);
        let (project, workspace) = (app.projects[0].id, app.projects[0].workspaces[1].id);
        app.changes.headers.insert(workspace);
        match action {
            "closed" => app.drop_workspace(project, workspace),
            "removed" => {
                assert!(app.start_removal(project, workspace, true, false, None));
                pump_until(&mut app, &rx, "the worktree is removed", |a| a.workspace_position(workspace).is_none());
            }
            _ => {
                git(repo.path(), &["worktree", "remove", "--force", path.to_str().expect("path")]);
                app.sync_worktrees();
            }
        }
        assert!(app.changes.summary(workspace).is_none());
        assert!(!app.changes.headers.contains(&workspace));
        assert!(!app.panel_view().expect("panel").owners.contains(&workspace));
    }

    #[rstest::rstest]
    #[case::closed(true)]
    #[case::removed(false)]
    fn a_departing_project_and_its_workspaces_are_forgotten(#[case] close: bool) {
        let repo = repo_with_edit();
        let (mut app, rx) = app_in(repo.path(), no_config());
        loaded_every(&mut app, &rx);
        let (project, workspace) = (app.projects[0].id, app.projects[0].workspaces[0].id);
        app.changes.headers.extend([project, workspace]);
        if close {
            app.close_project(project);
        } else {
            app.remove_project(0);
        }
        assert!(app.changes.summary(workspace).is_none());
        assert!(app.changes.headers.is_empty());
        assert!(app.every_targets().is_empty());
    }

    #[test]
    fn a_view_uses_the_targets_it_already_resolved() {
        let repo = repo_with_edit();
        let (mut app, rx) = app_in(repo.path(), no_config());
        loaded_every(&mut app, &rx);
        let targets = app.every_targets();
        std::fs::remove_file(repo.path().join(".git/HEAD")).expect("head disappears after resolving targets");
        let (body, owners, headers) = app.every_body(&targets);
        assert!(matches!(body, panel::Body::Ready(d) if d.files.len() == 1));
        assert_eq!(owners, [targets[0].0.workspace]);
        assert_eq!(headers.len(), 1);
    }

    #[test]
    fn file_actions_reach_the_workspace_that_owns_the_file() {
        let first = repo_with_edit();
        let second = git_repo(&[("b.txt", "one\n")]);
        std::fs::write(second.path().join("b.txt"), "two\n").expect("edit");
        let scripts = TempDir::new();
        let agent = scripts.path().join("agent");
        let editor = scripts.path().join("editor");
        write_executable(&agent, "#!/bin/sh\nIFS= read -r line\nprintf '%s' \"$line\" > got\nIFS= read -r line\n");
        write_executable(&editor, "#!/bin/sh\nprintf '%s\n' \"$PWD\" \"$@\" > editor-got\nIFS= read -r line\n");
        let config_path = write_config(
            scripts.path(),
            &Config { agent_commands: [("fake".into(), agent.display().to_string())].into(), ..Config::default() },
        );
        let (mut app, rx) = app_in(second.path(), config_path);
        exec_agent(&mut app, &agent);
        let owner = app.projects[0].workspaces[0].id;
        let agent_pane = app.term().expect("agent pane").id;
        app.open_project(first.path().to_path_buf(), AREA).expect("other project");
        let active = app.active;
        app.projects[0].collapsed = true;
        app.projects[0].workspaces[0].collapsed = true;
        loaded_every(&mut app, &rx);
        let file = open_file(&mut app, &rx, "b.txt");
        let position = row(&app, panel::Row::Hunk(file, 0));
        let hunk = Rect::new(panel_area(&app).x, position.y, panel_area(&app).width, 1);
        let ask = panel::actions(hunk)[1].1.as_position();
        click(&mut app, ask);
        assert_eq!(app.active, 0);
        assert_eq!(app.term().expect("visible agent").id, agent_pane);
        assert!(!app.projects[0].collapsed && !app.projects[0].workspaces[0].collapsed);
        assert!(app.changes.open && app.changes.scope == changes::Scope::Every);
        app.term_mut().expect("visible agent").write(b"\r");
        wait_until("the owning agent receives the reference", || {
            std::fs::read_to_string(second.path().join("got")).is_ok_and(|s| s == "b.txt:1 ")
        });
        assert!(!first.path().join("got").exists());
        assert_eq!(app.panel_view().expect("panel").owners[file], owner);
        app.goto(Goto::Place { project: app.projects[active].id, workspace: None, tab: None });
        app.projects[0].collapsed = true;
        app.projects[0].workspaces[0].collapsed = true;
        app.editor_env = vec![("VISUAL".into(), editor.display().to_string())];
        let open = panel::actions(hunk)[0].1.as_position();
        click(&mut app, open);
        wait_until("the editor runs in the owning workspace", || second.path().join("editor-got").exists());
        let got = std::fs::read_to_string(second.path().join("editor-got")).expect("editor arguments");
        assert_eq!(got, format!("{}\n+1\n{}\n", second.path().display(), second.path().join("b.txt").display()));
        assert_eq!((app.projects[0].workspaces[0].tabs.len(), app.projects[active].workspaces[0].tabs.len()), (2, 1));
        assert_eq!(app.active, 0);
        assert_eq!(app.projects[0].workspace().expect("owning workspace").active, 1);
        assert_eq!(app.term().expect("visible editor").cwd(), Some(second.path().to_path_buf()));
        assert!(!app.projects[0].collapsed && !app.projects[0].workspaces[0].collapsed);
        assert!(app.changes.open && app.changes.scope == changes::Scope::Every);
    }
}

fn filtered_changes() -> (TempDir, App, Receiver<AppEvent>) {
    let repo = repo_with_edit();
    let (mut app, rx) = app_in(repo.path(), no_config());
    open_filter(&mut app, &rx);
    type_text(&mut app, "*.txt");
    (repo, app, rx)
}

fn first_changed_file(app: &App) -> (Checkout, std::sync::Arc<changes::diff::File>) {
    let target = app.changes_target().expect("target");
    let file = app.changed_diff(&target).expect("diff").files[0].clone();
    (target, file)
}

fn exec_agent(app: &mut App, agent: &Path) {
    app.term_mut().expect("pane").write(format!("exec {}\n", agent.display()).as_bytes());
    wait_until("the agent runs", || {
        agents::detect(&app.config, &app.term().expect("pane").foreground_args()).is_some()
    });
}

fn open_changes() -> (TempDir, App, Receiver<AppEvent>) {
    let repo = repo_with_edit();
    let (mut app, rx) = app_in(repo.path(), no_config());
    app.changes.open = true;
    refresh_until_loaded(&mut app, &rx);
    (repo, app, rx)
}

fn repo_with_edit() -> TempDir {
    let repo = git_repo(&[("a.txt", "one\ntwo\n")]);
    std::fs::write(repo.path().join("a.txt"), "one\nTWO\n").expect("edit");
    repo
}

fn loaded(app: &mut App, rx: &Receiver<AppEvent>) {
    pump_until(app, rx, "the changes load", |a| {
        let Some(target) = a.changes_target() else { return false };
        a.changes.model(target.workspace, target.base.as_deref()).is_some()
    });
}

fn refresh_until_loaded(app: &mut App, rx: &Receiver<AppEvent>) {
    app.refresh(Instant::now());
    loaded(app, rx);
}

fn button(app: &App) -> Position {
    let label = app.changes_label().expect("a changes button");
    ui::changes_button(areas().issues, &label).as_position()
}

#[test]
fn a_git_workspace_shows_the_button_with_its_count() {
    let repo = repo_with_edit();
    let (mut app, rx) = app_in(repo.path(), no_config());
    refresh_until_loaded(&mut app, &rx);
    assert_eq!(app.changes_label().as_deref(), Some("Changes 1"));
}

#[test]
fn a_folder_outside_git_has_no_button() {
    let (app, _rx, _dirs) = app_with(1);
    assert_eq!(app.changes_label(), None);
}

#[test]
fn the_button_opens_the_panel_and_narrows_the_pane() {
    let repo = repo_with_edit();
    let (mut app, rx) = app_in(repo.path(), no_config());
    refresh_until_loaded(&mut app, &rx);
    let before = app.layout(AREA).pane.width;
    let pos = button(&app);
    click(&mut app, pos);
    let shown = app.layout(AREA);
    assert_eq!((app.changes.open, shown.pane.width < before, shown.changes.is_empty()), (true, true, false));
}

#[test]
fn the_panel_shows_the_changed_file() {
    let (_repo, app, _rx) = open_changes();
    let Some(panel::View { body: panel::Body::Ready(diff), .. }) = app.panel_view() else {
        panic!("the panel has a diff")
    };
    assert_eq!((diff.files[0].path.as_str(), diff.added(), diff.removed()), ("a.txt", 1, 1));
}

#[test]
fn clicking_a_tab_changes_what_is_compared() {
    let repo = repo_with_edit();
    let (mut app, rx) = app_in(repo.path(), no_config());
    app.changes.open = true;
    let (_, commits) = panel::tabs(panel_area(&app))[1];
    click(&mut app, commits.as_position());
    refresh_until_loaded(&mut app, &rx);
    let view = app.panel_view().expect("panel");
    assert_eq!((view.mode, view.base.as_deref()), (changes::Mode::Commits, Some("main")));
}

#[test]
fn the_picked_base_is_kept_for_the_workspace() {
    let repo = repo_with_edit();
    git(repo.path(), &["branch", "release"]);
    let (mut app, rx) = app_in(repo.path(), no_config());
    app.changes.open = true;
    app.changes.set_mode(changes::Mode::All);
    refresh_until_loaded(&mut app, &rx);
    let view = app.panel_view().expect("panel");
    let pos = panel::base(panel_area(&app), &view).as_position();
    click(&mut app, pos);
    pump_until(&mut app, &rx, "the branches show", |a| matches!(a.overlay, Some(Overlay::Branches(_))));
    submit_text(&mut app, "rel");
    let saved = app.state().projects[0].workspaces[0].base.clone();
    assert_eq!((app.overlay.is_none(), saved.as_deref()), (true, Some("release")));
}

#[test]
fn branches_that_arrive_after_the_panel_closes_are_dropped() {
    let repo = repo_with_edit();
    let (mut app, rx) = app_in(repo.path(), no_config());
    app.changes.open = true;
    app.changes.set_mode(changes::Mode::Commits);
    let target = app.changes_target().expect("target");
    app.open_branches(&target);
    app.changes.open = false;
    loop {
        let ev = rx.recv_timeout(Duration::from_secs(5)).expect("the branches arrive");
        let branches = matches!(ev, AppEvent::Changes(ChangesEvent::Branches { .. }));
        app.handle_event(ev, AREA).expect("handle event");
        if branches {
            break;
        }
    }
    assert!(app.overlay.is_none());
}

#[test]
fn picking_the_default_branch_forgets_the_choice() {
    let repo = repo_with_edit();
    let (mut app, rx) = app_in(repo.path(), no_config());
    app.changes.open = true;
    app.changes.set_mode(changes::Mode::All);
    app.projects[0].workspaces[0].base = Some("old".into());
    let target = app.changes_target().expect("target");
    app.open_branches(&target);
    pump_until(&mut app, &rx, "the branches show", |a| matches!(a.overlay, Some(Overlay::Branches(_))));
    submit_text(&mut app, "main");
    assert_eq!(app.projects[0].workspaces[0].base, None);
}

#[test]
fn unchanged_lines_open_from_a_thread() {
    let lines: String = (1..=20).flat_map(|n| ["line ".to_string(), n.to_string(), "\n".to_string()]).collect();
    let repo = git_repo(&[("a.txt", lines.as_str())]);
    let edited = lines.replace("line 2\n", "LINE 2\n").replace("line 19\n", "LINE 19\n");
    std::fs::write(repo.path().join("a.txt"), edited).expect("edit");
    let (mut app, rx) = app_in(repo.path(), no_config());
    app.changes.open = true;
    refresh_until_loaded(&mut app, &rx);
    let view = app.panel_view().expect("panel");
    let gap = panel::rows(&view).iter().position(|r| matches!(r, panel::Row::Gap(0, 1))).expect("a gap");
    let body = panel::parts(panel_area(&app), &view).body;
    let y = body.y + u16::try_from(gap).expect("row");
    click(&mut app, Position::new(body.x + 12, y));
    pump_until(&mut app, &rx, "the unchanged lines open", |a| {
        let target = a.changes_target().expect("target");
        let file = a.changed_diff(&target).expect("diff").files[0].clone();
        a.changes.gap(target.workspace, &file, 1).is_some()
    });
}

#[test]
fn the_panel_is_saved_and_restored() {
    let repo = repo_with_edit();
    let (mut app, _rx) = app_in(repo.path(), no_config());
    app.changes.open = true;
    app.changes.set_mode(changes::Mode::Commits);
    let saved = app.state();
    let (mut restored, _rx) = empty_app();
    restored.restore(&saved, AREA);
    assert_eq!((restored.changes.open, restored.changes.mode), (true, changes::Mode::Commits));
}

#[test]
fn copy_puts_the_hunk_on_the_clipboard() {
    let (_repo, mut app, _rx) = open_changes();
    let (target, file) = first_changed_file(&app);
    app.hunk_action(&target, &file, 0, HunkAction::Copy, AREA).expect("copy");
    let patch = file.hunks[0].patch();
    assert_eq!((app.take_host_writes(), toast(&app)), (vec![clipboard::osc52(&patch)], Some(COPIED)));
}

fn with_an_agent() -> (TempDir, TempDir, App) {
    let repo = repo_with_edit();
    let config = TempDir::new();
    let agent = config.path().join("agent");
    write_executable(&agent, "#!/bin/sh\nIFS= read -r line\nprintf '%s' \"$line\" > got\n");
    let settings =
        Config { agent_commands: [("fake".to_string(), agent.display().to_string())].into(), ..Config::default() };
    let config_path = write_config(config.path(), &settings);
    let (mut app, rx) = app_in(repo.path(), config_path);
    exec_agent(&mut app, &agent);
    refresh_until_loaded(&mut app, &rx);
    (repo, config, app)
}

fn ask_about_the_first_hunk(app: &mut App) {
    let (target, file) = first_changed_file(app);
    app.hunk_action(&target, &file, 0, HunkAction::Ask, AREA).expect("ask");
}

#[test]
fn ask_agent_brings_a_scrolled_agent_back_to_the_bottom() {
    let (_repo, _config, mut app) = with_an_agent();
    numbered(app.term_mut().expect("pane"), 100);
    app.term_mut().expect("pane").emulator.scroll(-3);

    ask_about_the_first_hunk(&mut app);

    assert_eq!(app.term().expect("pane").emulator.below(), 0);
}

#[test]
fn ask_agent_types_the_lines_into_the_agent() {
    let (repo, _config, mut app) = with_an_agent();
    ask_about_the_first_hunk(&mut app);
    app.term_mut().expect("pane").write(b"\r");
    let got = repo.path().join("got");
    wait_until("the agent reads the reference", || std::fs::read_to_string(&got).is_ok_and(|t| !t.is_empty()));
    assert_eq!(std::fs::read_to_string(&got).expect("got"), "a.txt:2 ");
}

#[test]
fn without_an_agent_the_reference_is_copied() {
    let repo = repo_with_edit();
    let (mut app, rx) = app_in(repo.path(), no_config());
    refresh_until_loaded(&mut app, &rx);
    let (target, file) = first_changed_file(&app);
    app.hunk_action(&target, &file, 0, HunkAction::Ask, AREA).expect("ask");
    assert_eq!(app.take_host_writes(), vec![clipboard::osc52("a.txt:2")]);
}

fn open_filter(app: &mut App, rx: &Receiver<AppEvent>) {
    app.changes.open = true;
    refresh_until_loaded(app, rx);
    click(app, panel::filter_button(panel_area(app)).as_position());
}

fn query(app: &App) -> Option<&str> {
    app.changes.filter.as_ref().map(changes::filter::Filter::query)
}

#[test]
fn keys_go_to_the_field_until_a_click_in_the_pane() {
    let (_repo, mut app, rx) = filtered_changes();
    let pane = app.layout(AREA).pane;
    click(&mut app, Position::new(pane.x + 2, pane.y + 2));
    type_in_pane(&mut app, &rx, "pane");
    assert_eq!(query(&app), Some("*.txt"));
}

#[test]
fn a_click_on_the_field_takes_the_keys_again() {
    let repo = repo_with_edit();
    let (mut app, rx) = app_in(repo.path(), no_config());
    open_filter(&mut app, &rx);
    let pane = app.layout(AREA).pane;
    click(&mut app, Position::new(pane.x + 2, pane.y + 2));
    let view = app.panel_view().expect("panel");
    let field = panel::parts(panel_area(&app), &view).field;
    click(&mut app, Position::new(field.x + 5, field.y));
    type_text(&mut app, "a.txt");
    assert_eq!(query(&app), Some("a.txt"));
}

#[test]
fn a_paste_goes_to_the_field() {
    let repo = repo_with_edit();
    let (mut app, rx) = app_in(repo.path(), no_config());
    open_filter(&mut app, &rx);
    app.handle_event(AppEvent::Input(Event::Paste("*.t\nxt".into())), AREA).expect("handle paste");
    assert_eq!(query(&app), Some("*.txt"));
}

#[test]
fn the_filter_hides_the_files_that_do_not_match() {
    let repo = repo_with_edit();
    let (mut app, rx) = app_in(repo.path(), no_config());
    open_filter(&mut app, &rx);
    type_text(&mut app, "!a.txt");
    let view = app.panel_view().expect("panel");
    assert!(!panel::rows(&view).contains(&panel::Row::File(0)));
}

#[test]
fn enter_keeps_the_filter_and_gives_the_keys_back() {
    let repo = repo_with_edit();
    let (mut app, rx) = app_in(repo.path(), no_config());
    open_filter(&mut app, &rx);
    submit_text(&mut app, "*.txt");
    type_in_pane(&mut app, &rx, "enter");
    assert_eq!(query(&app), Some("*.txt"));
}

#[test]
fn esc_closes_the_field_and_clears_the_filter() {
    let (_repo, mut app, _rx) = filtered_changes();
    send_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert_eq!((query(&app), app.filtering()), (None, false));
}

#[test]
fn closing_the_panel_gives_the_keys_back() {
    let repo = repo_with_edit();
    let (mut app, rx) = app_in(repo.path(), no_config());
    open_filter(&mut app, &rx);
    let close = panel::close(panel_area(&app)).as_position();
    click(&mut app, close);
    app.changes.open = true;
    assert!(!app.filtering());
}

#[test]
fn open_starts_the_editor_in_a_new_tab() {
    let repo = repo_with_edit();
    let (mut app, rx) = app_in(repo.path(), no_config());
    app.editor_env = vec![("VISUAL".into(), "true".into())];
    refresh_until_loaded(&mut app, &rx);
    let (target, file) = first_changed_file(&app);
    let tabs = app.projects[0].workspaces[0].tabs.len();
    app.hunk_action(&target, &file, 0, HunkAction::Open, AREA).expect("open");
    let workspace = &app.projects[0].workspaces[0];
    assert_eq!((workspace.tabs.len(), workspace.active), (tabs + 1, tabs));
}
