use ratatui::style::Modifier;
use rstest::rstest;

use super::*;

fn showing(repo: &TempDir, text: &str) -> (App, Receiver<AppEvent>) {
    let (mut app, rx) = app_in(repo.path(), no_config());
    app.term_mut().expect("a pane").feed(format!("\x1b[2J\x1b[H{text}").as_bytes());
    (app, rx)
}

fn repo() -> TempDir {
    git_repo(&[("src/main.rs", "fn main() {\n    run();\n}\n")])
}

fn cell(col: u16, row: u16) -> Position {
    Position::new(areas().pane.x + col, areas().pane.y + row)
}

fn shown(app: &App) -> Option<(String, Option<(u32, u32)>)> {
    let workspace = app.project()?.workspace()?.id;
    app.panels.files.viewer(workspace).map(|v| (v.path.clone(), v.selection))
}

fn written(app: &App) -> bool {
    app.term().is_some_and(|t| t.input_at.is_some())
}

#[test]
fn a_click_on_a_path_opens_it_at_its_lines() {
    let repo = repo();
    let (mut app, _rx) = showing(&repo, "look at src/main.rs:2-3");
    click(&mut app, cell(12, 0));
    assert_eq!(app.screen.side, Some(Side::Files));
    assert_eq!(shown(&app), Some(("src/main.rs".to_string(), Some((2, 3)))));
}

#[rstest]
#[case::absolute_path(false, false, false)]
#[case::home_path(true, false, false)]
#[case::absolute_path_with_mouse_reporting(false, true, false)]
#[case::home_path_with_mouse_reporting(true, true, false)]
#[case::absolute_image(false, false, true)]
#[case::home_image(true, false, true)]
#[case::absolute_image_with_mouse_reporting(false, true, true)]
#[case::home_image_with_mouse_reporting(true, true, true)]
fn an_outside_path_is_underlined_and_opens_in_the_viewer(
    #[case] home_path: bool,
    #[case] reads_mouse: bool,
    #[case] picture_file: bool,
) {
    let (repo, other) = (repo(), TempDir::new());
    let name = if picture_file { "preview.dat" } else { "plan.md" };
    let path = other.path().join(name).display().to_string();
    if picture_file {
        image::RgbaImage::new(16, 8).save_with_format(&path, image::ImageFormat::Png).expect("write a png");
    } else {
        std::fs::write(&path, "# Plan\n\nFix the return label.\n").expect("write");
    }
    let printed = if home_path { format!("~/{name}") } else { path.clone() };
    let mode = if reads_mouse { "\x1b[?1000h\x1b[?1006h" } else { "" };
    let area = Rect { width: 200, ..AREA };
    let (mut app, rx) = app_in(repo.path(), no_config());
    app.resize(area);
    app.term_mut().expect("a pane").feed(format!("\x1b[2J\x1b[H{mode}{printed}:2-3").as_bytes());
    app.home = Some(other.path().to_path_buf());
    app.term_mut().expect("a pane").input_at = None;
    let pane = app.layout(area).pane;
    let pos = Position::new(pane.x + 2, pane.y);
    mouse_in(&mut app, MouseEventKind::Moved, pos, area);
    let terminal = render(&mut app, area);
    assert!(terminal.backend().buffer()[pos].modifier.contains(Modifier::UNDERLINED));
    click_in(&mut app, pos, area);
    assert_eq!(shown(&app), Some((path.clone(), Some((2, 3)))));
    assert_eq!(app.screen.side, Some(Side::Files));
    assert!(!written(&app), "the program never gets the click");
    wait_until("the outside file is read", || {
        app.refresh(Instant::now());
        drain_in(&mut app, &rx, area);
        matches!(app.files_view().expect("files view").screen, ui::files::Screen::File(f)
            if f.content.as_ref().is_some_and(|c| if picture_file {
                matches!(&c.body, files::disk::Body::Image(p) if (p.format, p.width, p.height) == ("PNG", 16, 8))
            } else {
                c.lines() == ["# Plan", "", "Fix the return label."]
            }) && f.gutter.is_none())
    });
    if picture_file {
        let ui::files::Screen::File(file) = app.files_view().expect("files view").screen else {
            panic!("a file shows")
        };
        let panel = app.layout(area).changes;
        assert_eq!(ui::files::action(panel, &file, ui::files::Action::Open), Rect::default());
        for action in [ui::files::Action::Ask, ui::files::Action::Copy] {
            click_in(&mut app, ui::files::action(panel, &file, action).as_position(), area);
        }
        assert_eq!(app.take_host_writes(), [clipboard::osc52(&path), clipboard::osc52(&path)]);
    }
}

#[test]
fn text_that_names_no_file_is_a_plain_click() {
    let repo = repo();
    let (mut app, _rx) = showing(&repo, "see src/gone.rs here");
    click(&mut app, cell(6, 0));
    assert_ne!(app.screen.side, Some(Side::Files));
}

#[test]
fn a_program_reading_the_mouse_does_not_get_the_click() {
    let repo = repo();
    let (mut app, _rx) = showing(&repo, "\x1b[?1000h\x1b[?1006hsrc/main.rs");
    app.term_mut().expect("a pane").input_at = None;
    click(&mut app, cell(3, 0));
    assert_eq!((shown(&app).map(|s| s.0).as_deref(), written(&app)), (Some("src/main.rs"), false));
}

#[test]
fn a_drag_from_a_path_still_reaches_the_program() {
    let repo = repo();
    let (mut app, _rx) = showing(&repo, "\x1b[?1002h\x1b[?1006hsrc/main.rs");
    app.term_mut().expect("a pane").input_at = None;
    press(&mut app, cell(3, 0));
    assert!(!written(&app), "the press waits");
    mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), cell(8, 0));
    mouse(&mut app, MouseEventKind::Up(MouseButton::Left), cell(8, 0));
    assert_eq!((app.screen.side == Some(Side::Files), written(&app)), (false, true));
}

#[test]
fn a_path_stays_underlined_while_a_press_is_held_in_a_program_reading_the_mouse() {
    let repo = repo();
    let (mut app, _rx) = showing(&repo, "\x1b[?1000h\x1b[?1006hsrc/main.rs");
    let pos = cell(3, 0);
    app.term_mut().expect("a pane").input_at = None;

    press(&mut app, pos);

    assert!(!written(&app), "the press waits");
    assert_ne!(app.screen.side, Some(Side::Files), "the path opens on release");
    let terminal = render(&mut app, AREA);
    assert!(terminal.backend().buffer()[pos].modifier.contains(Modifier::UNDERLINED));
}

#[test]
fn the_path_under_the_pointer_is_underlined() {
    let repo = repo();
    let (mut app, _rx) = showing(&repo, "open src/main.rs now");
    mouse(&mut app, MouseEventKind::Moved, cell(7, 0));
    let t = render(&mut app, AREA);
    let lined = |col: u16| t.backend().buffer()[cell(col, 0)].modifier.contains(Modifier::UNDERLINED);
    assert_eq!([lined(4), lined(5), lined(15), lined(16)], [false, true, true, false]);
}
