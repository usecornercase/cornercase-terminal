use super::*;
use crate::changes::diff::Status;
use crate::files::Mark;
use crate::ui::changes::Action;
use crate::ui::files::{self as panel, Screen};

fn pressed_lines() -> (TempDir, App, Receiver<AppEvent>, Position) {
    let repo = repo();
    let (mut app, rx) = opened(&repo);
    show(&mut app, &rx, "src/main.rs");
    let (first, last) = (line_pos(&app, 1), line_pos(&app, 3));
    press(&mut app, first);
    (repo, app, rx, last)
}

fn repo() -> TempDir {
    git_repo(&[("README.md", "# shop\n"), ("src/main.rs", "fn main() {\n    run();\n}\n")])
}

fn view(app: &App) -> panel::View {
    app.files_view().expect("the panel has a workspace")
}

fn open(app: &mut App) {
    let pos = app.layout(AREA).files_button.as_position();
    click(app, pos);
}

fn settle(app: &mut App, rx: &Receiver<AppEvent>, what: &str, cond: impl Fn(&panel::View) -> bool) {
    wait_until(what, || {
        app.refresh(Instant::now());
        drain(app, rx);
        cond(&view(app))
    });
}

fn tree_rows(view: &panel::View) -> Vec<String> {
    match &view.screen {
        Screen::Tree(tree) => tree.rows.iter().map(|r| r.path.clone()).collect(),
        Screen::Search(_) | Screen::File(_) => Vec::new(),
    }
}

fn opened(repo: &TempDir) -> (App, Receiver<AppEvent>) {
    let (mut app, rx) = app_in(repo.path(), no_config());
    open(&mut app);
    settle(&mut app, &rx, "the workspace is listed", |v| !tree_rows(v).is_empty());
    (app, rx)
}

fn row_pos(app: &App, path: &str) -> Position {
    let view = view(app);
    let Screen::Tree(tree) = &view.screen else { panic!("the tree shows") };
    let i = tree.rows.iter().position(|r| r.path == path).expect("the row is listed");
    let body = panel::parts(panel_area(app)).body;
    Position::new(body.x + 4, body.y + u16::try_from(i - tree.scroll).expect("the row is on screen"))
}

fn file(app: &App) -> panel::FileView {
    match view(app).screen {
        Screen::File(file) => file,
        Screen::Tree(_) | Screen::Search(_) => panic!("a file shows"),
    }
}

fn lines(view: &panel::View) -> Vec<String> {
    match &view.screen {
        Screen::File(file) => file.content.as_ref().map(|c| c.lines().to_vec()).unwrap_or_default(),
        Screen::Tree(_) | Screen::Search(_) => Vec::new(),
    }
}

fn line_pos(app: &App, line: u32) -> Position {
    let view = view(app);
    let body = panel::parts(panel_area(app)).body;
    let row = panel::row_of(&view, line) - file(app).scroll;
    Position::new(body.x + 2, body.y + u16::try_from(row).expect("the line is on screen"))
}

fn action_pos(app: &App, action: Action) -> Position {
    panel::action(panel_area(app), &file(app), action).as_position()
}

fn show(app: &mut App, rx: &Receiver<AppEvent>, path: &str) {
    if let Some((folder, _)) = path.rsplit_once('/') {
        let pos = row_pos(app, folder);
        click(app, pos);
        settle(app, rx, "the folder is listed", |v| tree_rows(v).iter().any(|r| r == path));
    }
    let pos = row_pos(app, path);
    click(app, pos);
    settle(
        app,
        rx,
        "the file is read",
        |v| matches!(&v.screen, Screen::File(f) if f.content.as_ref().is_some_and(|c| c.styles().is_some())),
    );
}

#[test]
fn the_button_opens_the_tree_beside_the_pane() {
    let repo = repo();
    let (mut app, _rx) = app_in(repo.path(), no_config());
    let before = app.layout(AREA).pane.width;
    open(&mut app);
    assert_eq!((app.files.open, app.layout(AREA).pane.width < before), (true, true));
}

#[test]
fn the_files_todo_and_changes_panels_take_turns() {
    let repo = repo();
    let (mut app, _rx) = app_in(repo.path(), no_config());
    open(&mut app);
    let todo = app.layout(AREA).todo_button.as_position();
    click(&mut app, todo);
    assert_eq!((app.files.open, app.todo.open), (false, true));
    open(&mut app);
    assert_eq!((app.files.open, app.todo.open), (true, false));
    let changes = ui::changes_button(app.layout(AREA).issues, &app.changes_label().expect("a changes button"));
    click(&mut app, changes.as_position());
    assert_eq!((app.files.open, app.changes.open), (false, true));
}

#[test]
fn the_compact_bar_has_a_button_too() {
    let small = Rect { width: 80, ..AREA };
    let repo = repo();
    let (mut app, _rx) = app_in(repo.path(), no_config());
    click_in(&mut app, ui::layout(small, ui::Widths::default()).files_button.as_position(), small);
    assert!(app.files.open);
}

#[test]
fn lists_folders_first_and_opens_them() {
    let repo = repo();
    let (mut app, rx) = opened(&repo);
    assert_eq!(tree_rows(&view(&app)), ["src", "README.md"]);
    let pos = row_pos(&app, "src");
    click(&mut app, pos);
    settle(&mut app, &rx, "the folder is listed", |v| tree_rows(v).len() == 3);
    assert_eq!(tree_rows(&view(&app)), ["src", "src/main.rs", "README.md"]);
}

#[test]
fn a_file_shows_with_colours_and_back_returns_to_it() {
    let repo = repo();
    let (mut app, rx) = opened(&repo);
    show(&mut app, &rx, "README.md");
    assert_eq!(lines(&view(&app)), ["# shop"]);
    let back = panel::back(panel_area(&app)).as_position();
    click(&mut app, back);
    assert!(matches!(&view(&app).screen, Screen::Tree(t) if t.last.as_deref() == Some("README.md")));
}

#[test]
fn marks_what_changed_like_the_changes_panel() {
    let repo = repo();
    std::fs::write(repo.path().join("src/main.rs"), "fn main() {\n    start();\n}\n").expect("edit");
    let (mut app, rx) = opened(&repo);
    settle(
        &mut app,
        &rx,
        "the folder shows the change",
        |v| matches!(&v.screen, Screen::Tree(t) if t.rows.first().and_then(|r| r.status) == Some(Status::Modified)),
    );
    show(&mut app, &rx, "src/main.rs");
    settle(
        &mut app,
        &rx,
        "the edited line is marked",
        |v| matches!(&v.screen, Screen::File(f) if f.gutter.as_ref().and_then(|g| g.mark(2)) == Some(Mark::Modified)),
    );
    let mark = Position::new(panel_area(&app).x + 2 + 3, line_pos(&app, 2).y);
    click(&mut app, mark);
    assert_eq!(file(&app).unfolded, HashSet::from([2]));
}

#[test]
fn dragging_over_the_numbers_selects_lines_for_the_agent() {
    let (_repo, mut app, _rx, last) = pressed_lines();
    mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), last);
    mouse(&mut app, MouseEventKind::Up(MouseButton::Left), last);
    assert_eq!(file(&app).selection, Some((1, 3)));
    let ask = action_pos(&app, Action::Ask);
    click(&mut app, ask);
    assert_eq!(app.outbox.host_writes.last(), Some(&clipboard::osc52("src/main.rs:1-3")));
    assert_eq!(toast(&app), Some(NO_AGENT));
}

#[test]
fn a_selection_whose_release_was_lost_ends_at_the_next_event() {
    let (_repo, mut app, _rx, last) = pressed_lines();
    mouse(&mut app, MouseEventKind::Moved, last);
    click(&mut app, last);
    assert_eq!(file(&app).selection, Some((3, 3)));
}

#[test]
fn copy_takes_the_selected_lines() {
    let repo = repo();
    let (mut app, rx) = opened(&repo);
    show(&mut app, &rx, "src/main.rs");
    let pos = line_pos(&app, 2);
    click(&mut app, pos);
    let copy = action_pos(&app, Action::Copy);
    click(&mut app, copy);
    assert_eq!(app.outbox.host_writes.last(), Some(&clipboard::osc52("    run();")));
}

#[test]
fn an_outside_file_keeps_its_absolute_path_for_actions_and_the_previous_search() {
    let (repo, other) = (repo(), TempDir::new());
    let path = other.path().join("note.md").display().to_string();
    std::fs::write(&path, "# Note\nRead this.\n").expect("write");
    let (mut app, rx) = opened(&repo);
    search(&mut app, &rx, files::Mode::Text, "shop");
    app.open_link(&files::link::Target { path: path.clone(), lines: Some((1, 2)) });
    settle(&mut app, &rx, "the outside file is read", |v| lines(v) == ["# Note", "Read this."]);
    assert!(file(&app).gutter.is_none());
    let ask = action_pos(&app, Action::Ask);
    click(&mut app, ask);
    assert_eq!(app.outbox.host_writes.last(), Some(&clipboard::osc52(&format!("{path}:1-2"))));
    let copy = action_pos(&app, Action::Copy);
    click(&mut app, copy);
    assert_eq!(app.outbox.host_writes.last(), Some(&clipboard::osc52("# Note\nRead this.")));
    let workspace = app.project().expect("project").workspace().expect("workspace").id;
    app.files.viewer_mut(workspace).expect("viewer").selection = None;
    click(&mut app, copy);
    assert_eq!(app.outbox.host_writes.last(), Some(&clipboard::osc52(&path)));
    click(&mut app, ask);
    assert_eq!(app.outbox.host_writes.last(), Some(&clipboard::osc52(&path)));
    open(&mut app);
    open(&mut app);
    assert_eq!(file(&app).path, path, "reopening the panel keeps the outside file");
    let back = panel::back(panel_area(&app)).as_position();
    click(&mut app, back);
    assert!(!results(&view(&app)).is_empty(), "back returns to the previous search");
    assert_eq!(app.files.last(workspace), Some(path.as_str()));
    let field = panel::field(panel_area(&app)).as_position();
    click(&mut app, field);
    send_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(matches!(view(&app).screen, Screen::Tree(t) if t.rows.iter().all(|r| r.path != path)));
}

#[test]
fn the_viewer_follows_edits() {
    let repo = repo();
    let (mut app, rx) = opened(&repo);
    show(&mut app, &rx, "README.md");
    std::fs::write(repo.path().join("README.md"), "# shop\nsells socks\n").expect("edit");
    settle(&mut app, &rx, "the edit shows", |v| lines(v).len() == 2);
}

fn results(view: &panel::View) -> Vec<panel::Found> {
    match &view.screen {
        Screen::Search(search) => search.rows.clone(),
        Screen::Tree(_) | Screen::File(_) => Vec::new(),
    }
}

fn search(app: &mut App, rx: &Receiver<AppEvent>, mode: files::Mode, query: &str) {
    let button = if mode == files::Mode::Name { panel::mode_button } else { panel::field };
    let pos = button(panel_area(app)).as_position();
    click(app, pos);
    type_text(app, query);
    settle(app, rx, "the search answers", |v| !results(v).is_empty());
}

#[test]
fn a_name_search_finds_the_file_and_enter_opens_it() {
    let repo = repo();
    let (mut app, rx) = opened(&repo);
    search(&mut app, &rx, files::Mode::Name, "main");
    let paths: Vec<panel::Found> = results(&view(&app));
    assert!(matches!(&paths[..], [panel::Found::Name { path, .. }] if path == "src/main.rs"), "{paths:?}");
    send_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    settle(&mut app, &rx, "the file opens", |v| !lines(v).is_empty());
    assert_eq!(file(&app).path, "src/main.rs");
}

#[test]
fn a_text_search_opens_the_file_at_the_line() {
    let repo = repo();
    let (mut app, rx) = opened(&repo);
    search(&mut app, &rx, files::Mode::Text, "run");
    let found = results(&view(&app));
    let row = found.iter().position(|r| matches!(r, panel::Found::Line { number: 2, .. })).expect("line 2");
    let body = panel::parts(panel_area(&app)).body;
    click(&mut app, Position::new(body.x + 4, body.y + u16::try_from(row).expect("on screen")));
    let opened = file(&app);
    assert_eq!(
        (opened.path.as_str(), opened.selection, opened.find.as_deref()),
        ("src/main.rs", Some((2, 2)), Some("run"))
    );
    let back = panel::back(panel_area(&app)).as_position();
    click(&mut app, back);
    assert!(!results(&view(&app)).is_empty(), "back returns to the results");
}

#[test]
fn enter_before_the_answer_waits_for_it() {
    let repo = repo();
    let (mut app, rx) = opened(&repo);
    search(&mut app, &rx, files::Mode::Text, "shop");
    for _ in "shop".chars() {
        send_key(&mut app, KeyCode::Backspace, KeyModifiers::NONE);
    }
    type_text(&mut app, "run");
    send_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    settle(&mut app, &rx, "a file opens", |v| matches!(v.screen, Screen::File(_)));
    let opened = file(&app);
    assert_eq!((opened.path.as_str(), opened.selection), ("src/main.rs", Some((2, 2))));
}

#[test]
fn the_search_field_takes_the_keys_until_a_press_outside() {
    let repo = repo();
    let (mut app, rx) = opened(&repo);
    search(&mut app, &rx, files::Mode::Name, "ma");
    assert_eq!(
        app.files.query(app.project().and_then(Project::workspace).map(|w| w.id).expect("ws")).map(|q| q.text.as_str()),
        Some("ma")
    );
    click(&mut app, areas().pane.as_position());
    type_text(&mut app, "x");
    let query = app.files.query(app.project().and_then(Project::workspace).map(|w| w.id).expect("ws"));
    assert_eq!(query.map(|q| (q.text.as_str(), q.focused)), Some(("ma", false)));
}

#[test]
fn escape_goes_back_to_the_tree() {
    let repo = repo();
    let (mut app, rx) = opened(&repo);
    search(&mut app, &rx, files::Mode::Text, "shop");
    send_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(matches!(view(&app).screen, Screen::Tree(_)));
}

#[test]
fn the_open_panel_is_saved() {
    let repo = repo();
    let (mut app, _rx) = app_in(repo.path(), no_config());
    open(&mut app);
    let saved = app.state();
    assert!(saved.files);
    let (mut restored, _rx) = empty_app();
    restored.restore(&saved, AREA);
    assert_eq!((restored.files.open, restored.todo.open, restored.changes.open), (true, false, false));
}

mod images {
    use ratatui::buffer::CellDiffOption;

    use super::*;
    use crate::files::disk::Body;
    use crate::graphics::{CellSize, Missing, Protocol, Support, Tmux};
    use crate::ui::files::MARKER;

    const PLACEHOLDER: char = '\u{10EEEE}';

    fn png(dir: &Path, name: &str, width: u32, height: u32) {
        let pixels = image::RgbaImage::from_pixel(width, height, image::Rgba([200, 30, 30, 255]));
        pixels.save_with_format(dir.join(name), image::ImageFormat::Png).expect("write a png");
    }

    fn sight(protocol: Option<Protocol>) -> Sight {
        let support = Support {
            protocol,
            missing: protocol.is_none().then(|| Missing::Cannot { name: "st".into() }),
            cell: Some(CellSize { width: 10, height: 20 }),
            tmux: Tmux::None,
            id_hi: 42,
        };
        Sight { support, lo: 0xF0, ..Sight::default() }
    }

    fn shown(dir: &TempDir) -> (App, Receiver<AppEvent>) {
        png(dir.path(), "logo.png", 400, 200);
        let (mut app, rx) = opened(dir);
        let pos = row_pos(&app, "logo.png");
        click(&mut app, pos);
        settle(
            &mut app,
            &rx,
            "the image is read",
            |v| matches!(&v.screen, Screen::File(f) if f.content.as_ref().is_some_and(|c| c.is_image())),
        );
        (app, rx)
    }

    fn ready(app: &mut App, rx: &Receiver<AppEvent>, sight: &Sight) -> (Buffer, Placed) {
        let mut last = None;
        wait_until("the payload is ready", || {
            drain(app, rx);
            last = Some(render_frame(app, AREA, sight));
            last.as_ref().is_some_and(|(_, p)| p.as_ref().is_some_and(|p| p.payload.is_some()))
        });
        let (buffer, placed) = last.expect("drawn");
        (buffer, placed.expect("placed"))
    }

    fn cells(buffer: &Buffer, rect: Rect, what: impl Fn(&str) -> bool) -> usize {
        rect.positions().filter(|&p| what(buffer[p].symbol())).count()
    }

    fn text(buffer: &Buffer) -> String {
        buffer.content().iter().map(ratatui::buffer::Cell::symbol).collect()
    }

    #[test]
    fn an_image_says_what_it_is_and_offers_only_its_path() {
        let dir = repo();
        let (mut app, _rx) = shown(&dir);
        let Some(Body::Image(picture)) = file(&app).content.map(|c| c.body.clone()) else { panic!("an image") };

        assert_eq!((picture.width, picture.height), (400, 200));
        assert_eq!(panel::action(panel_area(&app), &file(&app), Action::Open), Rect::default());
        let ask = action_pos(&app, Action::Ask);
        click(&mut app, ask);
        let copy = action_pos(&app, Action::Copy);
        click(&mut app, copy);
        assert_eq!(app.take_host_writes(), [clipboard::osc52("logo.png"), clipboard::osc52("logo.png")]);
    }

    #[test]
    fn kitty_gets_placeholder_cells_once_its_payload_is_ready() {
        let dir = repo();
        let (mut app, rx) = shown(&dir);
        let kitty = sight(Some(Protocol::Kitty));

        let (first, placed) = render_frame(&mut app, AREA, &kitty);
        assert!(placed.is_some_and(|p| !p.drawn), "blank until the payload comes");
        assert_eq!(cells(&first, panel_area(&app), |s| s.starts_with(PLACEHOLDER)), 0);
        let (buffer, placed) = ready(&mut app, &rx, &kitty);

        assert_eq!(cells(&buffer, placed.rect, |s| s.starts_with(PLACEHOLDER)), placed.rect.area() as usize);
        assert_eq!(placed.key.kitty_id, crate::graphics::kitty::id(42, 0xF0));
    }

    #[test]
    fn iterm_and_sixel_get_markers_the_diff_skips() {
        let dir = repo();
        let (mut app, rx) = shown(&dir);

        let (buffer, placed) = ready(&mut app, &rx, &sight(Some(Protocol::Iterm)));

        assert!(placed.drawn);
        assert!(placed.rect.positions().all(|p| buffer[p].diff_option == CellDiffOption::Skip));
        assert_eq!(cells(&buffer, placed.rect, |s| s == MARKER), placed.rect.area() as usize);
    }

    #[test]
    fn a_terminal_without_images_says_why() {
        let dir = repo();
        let (mut app, _rx) = shown(&dir);
        let blind = sight(None);

        let (buffer, placed) = render_frame(&mut app, AREA, &blind);

        assert!(placed.is_none());
        let missing = blind.support.missing.expect("missing");
        assert!(text(&buffer).contains(&missing.lines()[0][..12]), "{}", text(&buffer));
    }

    #[test]
    fn a_dialog_hides_the_picture_until_it_closes() {
        let dir = repo();
        let (mut app, rx) = shown(&dir);
        let kitty = sight(Some(Protocol::Kitty));
        ready(&mut app, &rx, &kitty);
        let settings = app.layout(AREA).settings.as_position();
        click(&mut app, settings);

        let (buffer, placed) = render_frame(&mut app, AREA, &kitty);
        let placed = placed.expect("placed");
        assert!(!placed.drawn);
        assert_eq!(cells(&buffer, placed.rect, |s| s.starts_with(PLACEHOLDER)), 0);

        send_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        assert!(render_frame(&mut app, AREA, &kitty).1.is_some_and(|p| p.drawn));
    }

    #[test]
    fn a_menu_over_the_markers_is_drawn_and_the_picture_waits() {
        let dir = repo();
        let (mut app, rx) = shown(&dir);
        let iterm = sight(Some(Protocol::Iterm));
        let (_, placed) = ready(&mut app, &rx, &iterm);
        let pane = app.layout(AREA).pane;
        right_click(&mut app, Position::new(pane.right() - 1, placed.rect.y));

        let (buffer, placed) = render_frame(&mut app, AREA, &iterm);
        let placed = placed.expect("placed");
        let covered: Vec<Position> = placed.rect.positions().filter(|&p| buffer[p].symbol() != MARKER).collect();

        assert!(!covered.is_empty(), "the menu reaches the picture");
        assert!(!placed.drawn);
        assert!(covered.iter().all(|&p| buffer[p].diff_option == CellDiffOption::None));
    }

    #[test]
    fn a_window_that_sees_less_gets_a_smaller_picture() {
        let dir = repo();
        let (mut app, rx) = shown(&dir);
        let kitty = sight(Some(Protocol::Kitty));
        let (_, wide) = ready(&mut app, &rx, &kitty);
        let narrow = Sight { visible: Rect::new(0, 0, wide.rect.x + 6, AREA.height), ..kitty };

        let (_, placed) = render_frame(&mut app, AREA, &narrow);

        let placed = placed.expect("placed");
        assert!(placed.rect.right() <= narrow.visible.right() && placed.rect.width < wide.rect.width);
    }
}
