use ratatui::style::{Color, Modifier};
use rstest::rstest;
use serde_json::{Value, json};

use super::*;
use crate::control::contract::SURFACE_VERSION;
use crate::control::{Command, Request, Response};
use crate::test_util::fixture;

fn cell(symbol: &str, style: Look) -> Cell {
    Cell { symbol: symbol.to_string(), style, spacer: false }
}

fn text(row: &str) -> Vec<Cell> {
    row.chars().map(|c| cell(&c.to_string(), Look::default())).collect()
}

fn texts(runs: &[Run]) -> Vec<Vec<&str>> {
    runs.iter().map(|run| run.cells.iter().map(String::as_str).collect()).collect()
}

fn one(row: Vec<Cell>) -> (Vec<Run>, Vec<Style>) {
    let mut styles = Styles::default();
    let runs = lines(vec![row], &mut styles).remove(0);
    (runs, styles.into_table())
}

#[test]
fn a_line_is_runs_of_one_style() {
    let green = Look::default().fg(Color::Indexed(2));
    let row = vec![cell("a", Look::default()), cell("b", green), cell("c", green), cell("d", Look::default())];

    let (runs, table) = one(row);

    assert_eq!(runs.iter().map(|run| run.style).collect::<Vec<_>>(), [0, 1, 0]);
    assert_eq!(texts(&runs), [vec!["a"], vec!["b", "c"], vec!["d"]]);
    assert_eq!(table.len(), 2);
}

#[test]
fn trailing_blanks_in_the_default_style_are_left_out() {
    let (runs, _) = one(text("ab   "));

    assert_eq!(texts(&runs), [vec!["a", "b"]]);
    assert_eq!(one(text("     ")).0.len(), 0);
}

#[test]
fn a_blank_with_a_background_is_kept() {
    let mut row = text("a");
    row.push(cell(" ", Look::default().bg(Color::Indexed(4))));
    row.extend(text("  "));

    let (runs, table) = one(row);

    assert_eq!(texts(&runs), [vec!["a"], vec![" "]]);
    assert_eq!(table[runs[1].style].bg, Some(Colour::Index(4)));
}

#[test]
fn the_right_half_of_a_wide_character_is_an_empty_cell() {
    let row = vec![
        cell("中", Look::default()),
        Cell { spacer: true, ..cell(" ", Look::default()) },
        cell("x", Look::default()),
    ];

    let (runs, _) = one(row);

    assert_eq!(texts(&runs), [vec!["中", "", "x"]]);
    let trailing = vec![cell("中", Look::default()), Cell { spacer: true, ..cell(" ", Look::default()) }];
    assert_eq!(texts(&one(trailing).0), [vec!["中", ""]]);
}

#[test]
fn style_0_is_the_default_and_the_table_is_shared_by_the_panes() {
    let mut styles = Styles::default();
    let bold = Look::default().add_modifier(Modifier::BOLD);

    let first = lines(vec![vec![cell("a", bold)]], &mut styles);
    let second = lines(vec![vec![cell("b", bold), cell("c", Look::default())]], &mut styles);

    assert_eq!((first[0][0].style, second[0][0].style, second[0][1].style), (1, 1, 0));
    let table = styles.into_table();
    assert_eq!(table.len(), 2);
    assert_eq!(table[0], Style::default());
}

#[test]
fn no_colour_and_reset_are_the_same_style() {
    let mut styles = Styles::default();

    let none = styles.of(Look::default());
    let reset = styles.of(Look::default().fg(Color::Reset).bg(Color::Reset));

    assert_eq!((none, reset), (0, 0));
}

#[rstest]
#[case::palette(Color::Indexed(208), Some(Colour::Index(208)))]
#[case::rgb(Color::Rgb(1, 2, 3), Some(Colour::Rgb([1, 2, 3])))]
#[case::named(Color::Red, Some(Colour::Index(1)))]
#[case::bright(Color::White, Some(Colour::Index(15)))]
#[case::reset(Color::Reset, None)]
fn colours_are_palette_indices_or_rgb(#[case] color: Color, #[case] expected: Option<Colour>) {
    assert_eq!(Style::of(Look::default().fg(color)).fg, expected);
}

#[rstest]
#[case::bold(Modifier::BOLD, "bold")]
#[case::dim(Modifier::DIM, "dim")]
#[case::italic(Modifier::ITALIC, "italic")]
#[case::underline(Modifier::UNDERLINED, "underline")]
#[case::blink(Modifier::SLOW_BLINK, "blink")]
#[case::inverse(Modifier::REVERSED, "inverse")]
#[case::invisible(Modifier::HIDDEN, "invisible")]
#[case::strikethrough(Modifier::CROSSED_OUT, "strikethrough")]
fn attributes_map_from_the_terminal(#[case] modifier: Modifier, #[case] key: &str) {
    let style = Style::of(Look::default().add_modifier(modifier));

    assert_eq!(serde_json::to_value(style).expect("json"), json!({ key: true }));
}

#[rstest]
#[case::zero_cols(0, 10, Err("cols must be between 1 and 1000, not 0"))]
#[case::wide(1001, 10, Err("cols must be between 1 and 1000, not 1001"))]
#[case::zero_rows(10, 0, Err("rows must be between 1 and 500, not 0"))]
#[case::tall(10, 501, Err("rows must be between 1 and 500, not 501"))]
#[case::smallest(1, 1, Ok(Rect::new(0, 0, 1, 1)))]
#[case::largest(1000, 500, Ok(Rect::new(0, 0, 1000, 500)))]
fn the_size_is_checked(#[case] cols: u16, #[case] rows: u16, #[case] expected: Result<Rect, &str>) {
    let surface = Surface { tab: Some(1), cols, rows };

    assert_eq!(surface.area(), expected.map_err(str::to_string));
}

#[test]
fn a_surface_request_travels_with_its_tab_and_size_and_logs_them() {
    let request = Request {
        caller: None,
        server: None,
        contract: None,
        boot: None,
        command: Command::Surface(Surface { tab: Some(12), cols: 120, rows: 40 }),
    };

    let sent = serde_json::to_value(&request).expect("json");

    assert_eq!(
        sent,
        json!({"caller": null, "server": null, "command": "surface", "args": {"tab": 12, "cols": 120, "rows": 40}})
    );
    assert_eq!(serde_json::from_value::<Request>(sent).expect("a request"), request);
    assert_eq!(
        request.command.fields(),
        [("tab", "12".to_string()), ("cols", "120".to_string()), ("rows", "40".to_string())]
    );
}

fn frame() -> Frame {
    let fixture: Value = serde_json::from_str(fixture!("control/surface/v1/frame.json")).expect("fixture");
    serde_json::from_value(fixture).expect("a frame")
}

#[test]
fn the_fixture_is_of_the_surface_version_the_contract_names() {
    assert_eq!(SURFACE_VERSION, 1);
}

#[test]
fn a_frame_is_written_as_its_fixture() {
    let fixture: Value = serde_json::from_str(fixture!("control/surface/v1/frame.json")).expect("fixture");

    assert_eq!(serde_json::to_value(frame()).expect("json"), fixture);
}

#[test]
fn the_fixture_reads_back_as_its_frame() {
    let frame = frame();

    assert_eq!((frame.revision, frame.tab, frame.content.panes.len(), frame.content.dividers.len()), (3, 12, 2, 1));
    assert_eq!(frame.content.panes[0].lines[1][1].cells, [" ", "中", "", "x"]);
}

#[test]
fn a_streamed_frame_is_its_fixture_under_ok() {
    let fixture: Value = serde_json::from_str(fixture!("control/surface/v1/frame.json")).expect("fixture");

    let streamed: Value = serde_json::from_str(&frame().streamed().expect("text")).expect("json");

    assert_eq!(streamed, json!({"ok": fixture}));
    assert!(matches!(serde_json::from_value::<Response>(streamed), Ok(Response::Ok(_))));
}
