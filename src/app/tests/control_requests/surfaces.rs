use rstest::rstest;

use super::*;
use crate::app::Surfaced;
use crate::app::surfaces::{FRAME_EVERY, REFRESH_EVERY};
use crate::control::contract::Code;
use crate::control::surface::{Area, Colour, Frame, Shape, Surface};
use crate::control::{Layout, Watch};

fn command(tab: u64, cols: u16, rows: u16) -> Command {
    Command::Surface(Surface { tab: Some(tab), cols, rows, patches: false })
}

fn text_of(command: &Command, boot: Option<&str>, structured: bool) -> String {
    let mut request = serde_json::to_value(wire::Request {
        caller: None,
        server: None,
        contract: structured.then_some(1),
        boot: boot.map(str::to_string),
        view: None,
        command: command.clone(),
    })
    .expect("json");
    if !structured {
        request.as_object_mut().expect("an object").remove("contract");
    }
    request.to_string()
}

fn subscribe_as(app: &mut App, client: u64, at: Instant, command: &Command) -> Response {
    app.request(client, &text_of(command, None, false), Some(AREA), at);
    let mut answers = app.take_answers();
    assert_eq!(answers.len(), 1, "an answer at once");
    let (who, text) = answers.remove(0);
    assert_eq!(who, client);
    serde_json::from_str(&text).expect("a response")
}

fn subscribe(app: &mut App, at: Instant, tab: u64) -> Frame {
    match subscribe_as(app, CLIENT, at, &command(tab, 80, 24)) {
        Response::Ok(value) => serde_json::from_value(value).expect("a frame"),
        Response::Error(message) => panic!("the request failed: {message}"),
    }
}

fn output(app: &mut App, pane: u64, bytes: &str) {
    app.handle_event(AppEvent::Output(pane, bytes.as_bytes().to_vec()), AREA).expect("output");
}

fn frames(rendered: Vec<(u64, Surfaced)>) -> Vec<Frame> {
    rendered
        .into_iter()
        .filter_map(|(client, surfaced)| {
            assert_eq!(client, CLIENT);
            match surfaced {
                Surfaced::Frame(text) => match serde_json::from_str(&text) {
                    Ok(Response::Ok(value)) => Some(serde_json::from_value(value).expect("a frame")),
                    other => panic!("not a frame: {other:?}"),
                },
                Surfaced::End => None,
            }
        })
        .collect()
}

fn ready(app: &mut App, at: Instant) -> Vec<Frame> {
    frames(app.render_surfaces(&[CLIENT], at))
}

fn line(frame: &Frame, pane: usize, row: usize) -> String {
    frame.content.panes[pane].lines[row].iter().flat_map(|run| run.cells.iter().map(String::as_str)).collect()
}

fn tab_of(app: &App, index: usize) -> u64 {
    app.projects[0].workspaces[0].tabs[index].id
}

#[test]
fn the_first_answer_is_the_whole_tab_at_the_asked_size() {
    let (mut app, _rx, _dirs) = app_with(1);
    let (pane, tab) = (term(&app, 0).id, tab_of(&app, 0));

    let frame = subscribe(&mut app, Instant::now(), tab);

    let shown = &frame.content.panes[0];
    assert_eq!((frame.revision, frame.tab, frame.cols, frame.rows), (1, tab, 80, 24));
    assert_eq!(frame.boot, wire::server_token());
    assert_eq!(frame.content.layout, Some(Layout::Leaf(pane)));
    assert_eq!((shown.pane, shown.active, shown.rect), (pane, true, Area { x: 0, y: 0, cols: 80, rows: 24 }));
    let (rows, cols) = term(&app, 0).emulator.size().expect("a size");
    assert_eq!((shown.size.cols, shown.size.rows, shown.lines.len()), (cols, rows, usize::from(rows)));
    assert_eq!(frame.content.dividers.len(), 0);
}

#[test]
fn styled_output_comes_as_cells_with_their_style() {
    let (mut app, _rx, _dirs) = app_with(1);
    let (pane, tab, t0) = (term(&app, 0).id, tab_of(&app, 0), Instant::now());
    subscribe(&mut app, t0, tab);

    output(&mut app, pane, "\x1b[1;32mgreen\x1b[0m");
    let got = ready(&mut app, t0 + FRAME_EVERY);

    let run = &got[0].content.panes[0].lines[0][0];
    assert_eq!(run.cells, ["g", "r", "e", "e", "n"]);
    let style = got[0].content.styles[run.style];
    assert!(style.bold);
    assert_eq!(style.fg, Some(Colour::Index(2)));
}

#[test]
fn the_cursor_moves_and_changes_shape() {
    let (mut app, _rx, _dirs) = app_with(1);
    let (pane, tab, t0) = (term(&app, 0).id, tab_of(&app, 0), Instant::now());
    subscribe(&mut app, t0, tab);

    output(&mut app, pane, "\x1b[3;5H\x1b[6 q");
    let moved = ready(&mut app, t0 + FRAME_EVERY);
    output(&mut app, pane, "\x1b[?25l");
    let hidden = ready(&mut app, t0 + FRAME_EVERY * 2);

    let cursor = moved[0].content.panes[0].cursor.expect("a cursor");
    assert_eq!((cursor.x, cursor.y, cursor.shape, cursor.blinking), (4, 2, Shape::Bar, false));
    assert_eq!(hidden[0].content.panes[0].cursor, None);
}

#[test]
fn a_split_adds_a_pane_and_its_divider() {
    let (mut app, rx, _dirs) = app_with(1);
    let (pane, tab, t0) = (term(&app, 0).id, tab_of(&app, 0), Instant::now());
    subscribe(&mut app, t0, tab);

    done(now(&mut app, None, Command::Split(wire::Split { pane: Some(pane), ..wire::Split::default() })));
    drop(rx);
    app.touch_surfaces();
    let got = ready(&mut app, t0 + FRAME_EVERY);

    let area = Rect::new(0, 0, 80, 24);
    let layout = &app.projects[0].workspaces[0].tabs[0].layout;
    let expected: Vec<(u64, Area)> = layout.panes(area).into_iter().map(|(id, rect)| (id, rect.into())).collect();
    let rects: Vec<(u64, Area)> = got[0].content.panes.iter().map(|p| (p.pane, p.rect)).collect();
    assert_eq!(rects, expected);
    assert_eq!(got[0].content.dividers.len(), 1);
    assert_eq!(got[0].content.dividers[0].line, Area::from(layout.dividers(area)[0].line));
    assert_eq!(got[0].revision, 2);
}

#[test]
fn a_tab_the_window_does_not_show_streams_without_switching_to_it() {
    let (mut app, _rx, _dirs) = app_with(1);
    let hidden = done(now(&mut app, None, new_tab(None))).ids;
    let (tab, pane, t0) = (hidden.tab.expect("a tab"), hidden.pane.expect("a pane"), Instant::now());
    let before = app.report(None).shown;
    subscribe(&mut app, t0, tab);

    output(&mut app, pane, "hidden-text");
    let got = ready(&mut app, t0 + FRAME_EVERY);

    assert!(!app.shows(pane));
    assert_eq!(line(&got[0], 0, 0), "hidden-text");
    assert_eq!(app.report(None).shown, before);
    assert_ne!(Some(tab), before.tab);
}

#[test]
fn output_in_a_pane_the_surface_does_not_show_renders_nothing() {
    let (mut app, _rx, _dirs) = app_with(1);
    let other = done(now(&mut app, None, new_tab(None))).ids;
    let (watched, t0) = (tab_of(&app, 0), Instant::now());
    subscribe(&mut app, t0, watched);

    output(&mut app, other.pane.expect("a pane"), "elsewhere");

    assert_eq!(app.surface_wake(t0), None);
    assert_eq!(ready(&mut app, t0 + FRAME_EVERY), []);
}

#[test]
fn nothing_is_sent_while_nothing_changes() {
    let (mut app, _rx, _dirs) = app_with(1);
    let (tab, t0) = (tab_of(&app, 0), Instant::now());
    subscribe(&mut app, t0, tab);

    app.touch_surfaces();
    let touched = ready(&mut app, t0 + FRAME_EVERY);
    let refreshed = ready(&mut app, t0 + REFRESH_EVERY * 3);

    assert_eq!((touched, refreshed), (vec![], vec![]));
}

#[test]
fn frames_come_at_most_once_a_frame() {
    let (mut app, _rx, _dirs) = app_with(1);
    let (pane, tab, t0) = (term(&app, 0).id, tab_of(&app, 0), Instant::now());
    subscribe(&mut app, t0, tab);
    let at = t0 + FRAME_EVERY;

    output(&mut app, pane, "one");
    let first = ready(&mut app, at);
    output(&mut app, pane, "two");
    let soon = at + Duration::from_millis(5);
    let early = ready(&mut app, soon);
    let wake = app.surface_wake(soon);
    let later = ready(&mut app, at + FRAME_EVERY);

    assert_eq!((first.len(), early.len(), later.len()), (1, 0, 1));
    assert_eq!(wake, Some(FRAME_EVERY.saturating_sub(Duration::from_millis(5))));
    assert_eq!((first[0].revision, later[0].revision), (2, 3));
}

#[test]
fn a_reader_without_room_gets_the_newest_frame_once_it_has_room() {
    let (mut app, _rx, _dirs) = app_with(1);
    let (pane, tab, t0) = (term(&app, 0).id, tab_of(&app, 0), Instant::now());
    subscribe(&mut app, t0, tab);

    output(&mut app, pane, "one");
    let none = frames(app.render_surfaces(&[], t0 + FRAME_EVERY));
    output(&mut app, pane, "two");
    let got = ready(&mut app, t0 + FRAME_EVERY * 2);

    assert_eq!(none, []);
    assert_eq!(got.len(), 1);
    assert_eq!((got[0].revision, line(&got[0], 0, 0)), (2, "onetwo".to_string()));
}

#[test]
fn the_stream_ends_when_its_tab_closes() {
    let (mut app, rx, _dirs) = app_with(1);
    let hidden = done(now(&mut app, None, new_tab(None))).ids;
    let (tab, t0) = (hidden.tab.expect("a tab"), Instant::now());
    subscribe(&mut app, t0, tab);

    done(now(
        &mut app,
        None,
        Command::Close(wire::Close { item: Item::Tab(tab), remove_worktree: false, force: false, unlock: false }),
    ));
    refreshing(&mut app, &rx, "the tab closes", |app| app.projects.tab_index(tab).is_none());
    let ended = app.render_surfaces(&[CLIENT], t0 + FRAME_EVERY);
    let after = app.render_surfaces(&[CLIENT], t0 + FRAME_EVERY * 2);

    assert_eq!(ended, [(CLIENT, Surfaced::End)]);
    assert_eq!(after, []);
    assert!(!app.streams_surfaces());
}

#[test]
fn a_client_that_leaves_stops_its_stream_and_the_shells_keep_running() {
    let (mut app, _rx, _dirs) = app_with(1);
    let (pane, tab, t0) = (term(&app, 0).id, tab_of(&app, 0), Instant::now());
    subscribe(&mut app, t0, tab);

    app.forget(CLIENT);
    output(&mut app, pane, "unseen");

    assert!(!app.streams_surfaces());
    assert_eq!(ready(&mut app, t0 + FRAME_EVERY), []);
    assert!(app.projects.pane_by(pane).is_some());
    ask(&mut app, None, send_text(pane, "x", false, false));
    done(answers(&mut app).remove(0));
}

#[test]
fn reconnecting_starts_again_from_a_whole_frame() {
    let (mut app, _rx, _dirs) = app_with(1);
    let (pane, tab, t0) = (term(&app, 0).id, tab_of(&app, 0), Instant::now());
    subscribe(&mut app, t0, tab);
    output(&mut app, pane, "kept");
    ready(&mut app, t0 + FRAME_EVERY);

    app.forget(CLIENT);
    let again = subscribe(&mut app, t0 + FRAME_EVERY * 2, tab);

    assert_eq!((again.revision, line(&again, 0, 0)), (1, "kept".to_string()));
}

#[rstest]
#[case::unknown_tab(command(9999, 80, 24), None, Code::NotFound)]
#[case::bad_size(command(1, 0, 24), None, Code::Malformed)]
#[case::no_tab(Command::Surface(Surface { tab: None, cols: 80, rows: 24, patches: false }), None, Code::Malformed)]
#[case::stale_boot(command(1, 80, 24), Some("1-2"), Code::StaleBoot)]
fn a_surface_is_refused(#[case] command: Command, #[case] boot: Option<&str>, #[case] code: Code) {
    let (mut app, _rx, _dirs) = app_with(1);
    let command = match command {
        Command::Surface(surface) if surface.tab == Some(1) => command_for(tab_of(&app, 0), &surface),
        other => other,
    };

    app.request(CLIENT, &text_of(&command, boot, true), Some(AREA), Instant::now());

    let answers = app.take_answers();
    let value: Value = serde_json::from_str(&answers[0].1).expect("json");
    assert_eq!(value["error"]["code"], serde_json::to_value(code).expect("a code"), "{value}");
    assert!(!app.streams_surfaces());
}

fn command_for(tab: u64, surface: &Surface) -> Command {
    Command::Surface(Surface { tab: Some(tab), ..surface.clone() })
}

#[test]
fn a_second_surface_on_the_connection_is_refused() {
    let (mut app, _rx, _dirs) = app_with(1);
    let (tab, t0) = (tab_of(&app, 0), Instant::now());
    subscribe(&mut app, t0, tab);

    let second = subscribe_as(&mut app, CLIENT, t0, &command(tab, 80, 24));

    assert!(error(second).contains("already streams tab"));
}

#[test]
fn a_frame_names_the_session_revision() {
    let (mut app, _rx, _dirs) = app_with(1);
    let (pane, tab, t0) = (term(&app, 0).id, tab_of(&app, 0), Instant::now());
    let watching = subscribe_as(&mut app, 77, t0, &Command::Watch(Watch::default()));
    let Response::Ok(snapshot) = watching else { panic!("not a snapshot") };
    let revision = snapshot["revision"].as_u64().expect("a revision");
    let first = subscribe(&mut app, t0, tab);

    output(&mut app, pane, "x");
    let got = ready(&mut app, t0 + FRAME_EVERY);

    assert_eq!((first.session, got[0].session), (revision, revision));
}

mod resizing {
    use super::*;
    use crate::control::terminal::Resize;

    fn resize(app: &mut App, client: u64, surface: Option<u64>, cols: u16, rows: u16) -> Response {
        let command = Command::Resize(Resize { surface, cols, rows });
        subscribe_as(app, client, Instant::now(), &command)
    }

    fn refusal(response: Response) -> String {
        match response {
            Response::Error(message) => message,
            Response::Ok(value) => panic!("it was taken: {value}"),
        }
    }

    #[test]
    fn a_surface_is_laid_out_again_in_place() {
        let (mut app, _rx, _dirs) = app_with(1);
        let (pane, tab, t0) = (term(&app, 0).id, tab_of(&app, 0), Instant::now());
        let first = subscribe(&mut app, t0, tab);
        let size_before = term(&app, 0).emulator.size().expect("a size");
        assert_ne!(first.surface, 0);

        let Response::Ok(answer) = resize(&mut app, CLIENT + 1, Some(first.surface), 100, 30) else {
            panic!("refused")
        };
        let got = ready(&mut app, t0 + FRAME_EVERY);

        assert_eq!((answer["surface"].as_u64(), answer["tab"].as_u64()), (Some(first.surface), Some(tab)));
        let frame = &got[0];
        assert_eq!((frame.cols, frame.rows, frame.revision, frame.surface), (100, 30, 2, first.surface));
        assert_eq!(frame.content.panes[0].rect, Area { x: 0, y: 0, cols: 100, rows: 30 });
        assert_eq!(frame.content.panes[0].pane, pane);
        assert_eq!(term(&app, 0).emulator.size().expect("a size"), size_before, "no terminal was resized");
    }

    #[test]
    fn a_resize_names_a_live_surface_and_a_size_it_can_lay_out() {
        let (mut app, _rx, _dirs) = app_with(1);
        let (tab, t0) = (tab_of(&app, 0), Instant::now());
        let surface = subscribe(&mut app, t0, tab).surface;

        let missing = refusal(resize(&mut app, CLIENT + 1, None, 80, 24));
        let unknown = refusal(resize(&mut app, CLIENT + 1, Some(surface + 50), 80, 24));
        let empty = refusal(resize(&mut app, CLIENT + 1, Some(surface), 0, 0));
        let wide = refusal(resize(&mut app, CLIENT + 1, Some(surface), 1001, 24));
        app.forget(CLIENT);
        let ended = refusal(resize(&mut app, CLIENT + 1, Some(surface), 80, 24));

        assert!(missing.contains("--surface"), "{missing}");
        assert!(unknown.contains("no surface") && ended.contains("no surface"), "{unknown} / {ended}");
        assert!(empty.contains("cols") && wide.contains("cols"), "{empty} / {wide}");
    }

    #[test]
    fn two_surfaces_have_their_own_ids() {
        let (mut app, _rx, _dirs) = app_with(1);
        let (tab, t0) = (tab_of(&app, 0), Instant::now());
        let one = subscribe(&mut app, t0, tab).surface;
        let two = match subscribe_as(&mut app, CLIENT + 1, t0, &command(tab, 80, 24)) {
            Response::Ok(value) => serde_json::from_value::<Frame>(value).expect("a frame").surface,
            Response::Error(message) => panic!("{message}"),
        };

        assert_ne!(one, two);
    }
}

mod cells {
    use super::*;
    use crate::control::surface::{Colours, Hyperlink, Underline};

    const CLEAR: &str = "\x1b[2J\x1b[H";

    fn frame_after(bytes: &str) -> Frame {
        let (mut app, _rx, _dirs) = app_with(1);
        let (pane, tab, t0) = (term(&app, 0).id, tab_of(&app, 0), Instant::now());
        subscribe(&mut app, t0, tab);
        output(&mut app, pane, &format!("{CLEAR}{bytes}"));
        ready(&mut app, t0 + FRAME_EVERY).remove(0)
    }

    #[test]
    fn hyperlinks_come_as_a_table_the_runs_index() {
        let got = frame_after("\x1b]8;;https://example.com\x1b\\site\x1b]8;;\x1b\\ plain");

        assert_eq!(got.content.hyperlinks, [Hyperlink { uri: "https://example.com".into() }]);
        let runs = &got.content.panes[0].lines[0];
        assert_eq!((runs[0].hyperlink, runs[0].cells.len()), (Some(0), 4));
        assert_eq!(runs[1].hyperlink, None);
    }

    #[test]
    fn a_hyperlink_that_wraps_is_one_entry_on_both_rows() {
        let (mut app, _rx, _dirs) = app_with(1);
        let (pane, tab, t0) = (term(&app, 0).id, tab_of(&app, 0), Instant::now());
        let cols = usize::from(subscribe(&mut app, t0, tab).content.panes[0].size.cols);
        let linked = "x".repeat(cols + 5);

        output(&mut app, pane, &format!("{CLEAR}\x1b]8;;https://example.com/long\x1b\\{linked}\x1b]8;;\x1b\\"));
        let got = ready(&mut app, t0 + FRAME_EVERY).remove(0);

        assert_eq!(got.content.hyperlinks.len(), 1);
        let lines = &got.content.panes[0].lines;
        assert_eq!((lines[0][0].hyperlink, lines[1][0].hyperlink), (Some(0), Some(0)));
        assert_eq!(lines[1][0].cells.len(), 5);
    }

    #[test]
    fn undercurl_and_its_colour_reach_the_styles() {
        let got = frame_after("\x1b[4:3;58:5:196mx");

        let style = got.content.styles[got.content.panes[0].lines[0][0].style];
        assert!(style.underline);
        assert_eq!(style.underline_style, Some(Underline::Curly));
        assert_eq!(style.underline_colour, Some(Colour::Index(196)));
    }

    #[test]
    fn a_line_cleared_with_a_background_streams_it() {
        let got = frame_after("\x1b[44mab\x1b[K");

        let runs = &got.content.panes[0].lines[0];
        assert_eq!(runs.len(), 1);
        assert_eq!(got.content.styles[runs[0].style].bg, Some(Colour::Index(4)));
        assert!(runs[0].cells.len() > 2);
    }

    #[test]
    fn colours_a_program_changes_reach_its_pane_until_it_resets_them() {
        let (mut app, _rx, _dirs) = app_with(1);
        let (pane, tab, t0) = (term(&app, 0).id, tab_of(&app, 0), Instant::now());
        subscribe(&mut app, t0, tab);

        output(&mut app, pane, "\x1b]4;1;rgb:ff/00/00\x07\x1b]11;rgb:44/55/66\x07");
        let set = ready(&mut app, t0 + FRAME_EVERY).remove(0);
        let colours = &set.content.panes[0].colours;
        assert_eq!((colours.palette["1"], colours.bg), ([255, 0, 0], Some([0x44, 0x55, 0x66])));

        output(&mut app, pane, "\x1b]104\x07\x1b]111\x07");
        let reset = ready(&mut app, t0 + FRAME_EVERY * 2).remove(0);
        assert_eq!(reset.content.panes[0].colours, Colours::default());
    }
}

mod patches;
