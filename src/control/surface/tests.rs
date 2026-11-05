use ratatui::style::{Color, Modifier};
use rstest::rstest;
use serde_json::{Value, json};

use super::*;
use crate::control::contract::SURFACE_VERSION;
use crate::control::{Command, Request, Response};
use crate::term::emulator::{Decor, ProgramColours, UnderlineStyle};
use crate::test_util::fixture;

fn cell(symbol: &str, style: Look) -> Cell {
    Cell { symbol: symbol.to_string(), style, ..Cell::default() }
}

fn text(row: &str) -> Vec<Cell> {
    row.chars().map(|c| cell(&c.to_string(), Look::default())).collect()
}

fn texts(runs: &[Run]) -> Vec<Vec<&str>> {
    runs.iter().map(|run| run.cells.iter().map(String::as_str).collect()).collect()
}

fn one(row: Vec<Cell>) -> (Vec<Run>, Vec<Style>) {
    let mut tables = Tables::default();
    let runs = lines(vec![row], &[], &mut tables).remove(0);
    (runs, tables.styles.into_entries())
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
    let mut tables = Tables::default();
    let bold = Look::default().add_modifier(Modifier::BOLD);

    let first = lines(vec![vec![cell("a", bold)]], &[], &mut tables);
    let second = lines(vec![vec![cell("b", bold), cell("c", Look::default())]], &[], &mut tables);

    assert_eq!((first[0][0].style, second[0][0].style, second[0][1].style), (1, 1, 0));
    let table = tables.styles.into_entries();
    assert_eq!(table.len(), 2);
    assert_eq!(table[0], Style::default());
}

#[test]
fn no_colour_and_reset_are_the_same_style() {
    let mut styles = Tables::default().styles;
    let of = |styles: &mut Table<Style>, look| styles.of(Style::of(look, Decor::default()));

    let none = of(&mut styles, Look::default());
    let reset = of(&mut styles, Look::default().fg(Color::Reset).bg(Color::Reset));

    assert_eq!((none, reset), (0, 0));
}

#[rstest]
#[case::palette(Color::Indexed(208), Some(Colour::Index(208)))]
#[case::rgb(Color::Rgb(1, 2, 3), Some(Colour::Rgb([1, 2, 3])))]
#[case::named(Color::Red, Some(Colour::Index(1)))]
#[case::bright(Color::White, Some(Colour::Index(15)))]
#[case::reset(Color::Reset, None)]
fn colours_are_palette_indices_or_rgb(#[case] color: Color, #[case] expected: Option<Colour>) {
    assert_eq!(Style::of(Look::default().fg(color), Decor::default()).fg, expected);
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
    let style = Style::of(Look::default().add_modifier(modifier), Decor::default());

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
    let surface = Surface { tab: Some(1), cols, rows, patches: false, popup: false };

    assert_eq!(surface.area(), expected.map_err(str::to_string));
}

#[test]
fn a_surface_request_travels_with_its_tab_and_size_and_logs_them() {
    let request = Request {
        caller: None,
        server: None,
        contract: None,
        boot: None,
        view: None,
        command: Command::Surface(Surface { tab: Some(12), cols: 120, rows: 40, patches: false, popup: false }),
    };

    let sent = serde_json::to_value(&request).expect("json");

    assert_eq!(
        sent,
        json!({"caller": null, "server": null, "command": "surface", "args": {"tab": 12, "cols": 120, "rows": 40, "patches": false, "popup": false}})
    );
    assert_eq!(serde_json::from_value::<Request>(sent).expect("a request"), request);
    assert_eq!(
        request.command.fields(),
        [
            ("tab", "12".to_string()),
            ("cols", "120".to_string()),
            ("rows", "40".to_string()),
            ("patches", "false".to_string())
        ]
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
    assert_eq!(frame.content.hyperlinks.len(), 2);
    assert_eq!(frame.content.panes[0].colours.palette["200"], [16, 32, 48]);
    assert_eq!(frame.content.panes[1].colours, Colours::default());
    assert_eq!(frame.content.styles[3].underline_style, Some(Underline::Curly));
}

fn without(value: &mut Value, keys: &[&str]) {
    match value {
        Value::Object(map) => {
            map.retain(|key, _| !keys.contains(&key.as_str()));
            map.values_mut().for_each(|inner| without(inner, keys));
        }
        Value::Array(items) => items.iter_mut().for_each(|inner| without(inner, keys)),
        _ => {}
    }
}

#[test]
fn a_frame_without_the_new_keys_reads_and_writes_back_unchanged() {
    let mut fixture: Value = serde_json::from_str(fixture!("control/surface/v1/frame.json")).expect("fixture");
    without(&mut fixture, &["hyperlinks", "hyperlink", "colours", "underline_style", "underline_colour", "overline"]);

    let frame: Frame = serde_json::from_value(fixture.clone()).expect("a frame");

    assert_eq!(serde_json::to_value(frame).expect("json"), fixture);
}

#[test]
fn a_streamed_frame_is_its_fixture_under_ok() {
    let fixture: Value = serde_json::from_str(fixture!("control/surface/v1/frame.json")).expect("fixture");

    let streamed: Value = serde_json::from_str(&frame().streamed().expect("text")).expect("json");

    assert_eq!(streamed, json!({"ok": fixture}));
    assert!(matches!(serde_json::from_value::<Response>(streamed), Ok(Response::Ok(_))));
}

#[test]
fn a_frame_names_its_stream_and_one_without_a_name_still_reads() {
    let mut fixture: Value = serde_json::from_str(fixture!("control/surface/v1/frame.json")).expect("fixture");
    assert_eq!(frame().surface, 2);

    fixture.as_object_mut().expect("an object").remove("surface");
    let older: Frame = serde_json::from_value(fixture).expect("a frame");

    assert_eq!(older.surface, 0);
}

fn decor(underline: UnderlineStyle, colour: Color, overline: bool) -> Decor {
    Decor { underline, underline_colour: colour, overline }
}

fn underlined() -> Look {
    Look::default().add_modifier(Modifier::UNDERLINED)
}

#[rstest]
#[case::curly(UnderlineStyle::Curly, json!({"underline": true, "underline_style": "curly"}))]
#[case::double(UnderlineStyle::Double, json!({"underline": true, "underline_style": "double"}))]
#[case::dotted(UnderlineStyle::Dotted, json!({"underline": true, "underline_style": "dotted"}))]
#[case::dashed(UnderlineStyle::Dashed, json!({"underline": true, "underline_style": "dashed"}))]
#[case::single(UnderlineStyle::Single, json!({"underline": true}))]
fn underline_styles_are_a_style_key_and_a_plain_underline_has_none(
    #[case] style: UnderlineStyle,
    #[case] expected: Value,
) {
    let style = Style::of(underlined(), decor(style, Color::Reset, false));

    assert_eq!(serde_json::to_value(style).expect("json"), expected);
}

#[test]
fn the_underline_colour_and_overline_are_keys_and_the_colour_needs_an_underline() {
    let coloured = Style::of(underlined(), decor(UnderlineStyle::Single, Color::Indexed(196), true));
    assert_eq!(
        serde_json::to_value(coloured).expect("json"),
        json!({"underline": true, "underline_colour": {"index": 196}, "overline": true})
    );
    let loose = Style::of(Look::default(), decor(UnderlineStyle::None, Color::Rgb(1, 2, 3), false));
    assert_eq!(loose, Style::default());
}

fn linked(symbol: &str, link: Option<usize>) -> Cell {
    Cell { link, ..cell(symbol, Look::default()) }
}

#[test]
fn a_run_is_cells_of_one_style_and_one_hyperlink() {
    let mut tables = Tables::default();
    let row =
        vec![linked("a", None), linked("b", Some(0)), linked("c", Some(0)), linked("d", Some(1)), linked("e", None)];
    let links = ["https://a.test".to_string(), "https://b.test".to_string()];

    let runs = lines(vec![row], &links, &mut tables).remove(0);

    assert_eq!(
        runs.iter().map(|run| (run.style, run.hyperlink)).collect::<Vec<_>>(),
        [(0, None), (0, Some(0)), (0, Some(1)), (0, None)]
    );
    assert_eq!(texts(&runs), [vec!["a"], vec!["b", "c"], vec!["d"], vec!["e"]]);
}

#[test]
fn the_hyperlink_table_is_shared_by_the_panes_and_deduplicated() {
    let mut tables = Tables::default();

    let first = lines(vec![vec![linked("a", Some(0))]], &["https://a.test".to_string()], &mut tables);
    let second = lines(
        vec![vec![linked("b", Some(0)), linked("c", Some(1))]],
        &["https://b.test".to_string(), "https://a.test".to_string()],
        &mut tables,
    );

    assert_eq!(first[0][0].hyperlink, Some(0));
    assert_eq!((second[0][0].hyperlink, second[0][1].hyperlink), (Some(1), Some(0)));
    let table = tables.hyperlinks.into_entries();
    assert_eq!(table, [Hyperlink { uri: "https://a.test".into() }, Hyperlink { uri: "https://b.test".into() }]);
}

#[test]
fn a_linked_blank_at_the_end_of_a_line_is_kept() {
    let row = vec![linked("a", None), linked(" ", Some(0)), linked(" ", None)];

    let runs = lines(vec![row], &["file:///x".to_string()], &mut Tables::default()).remove(0);

    assert_eq!(texts(&runs), [vec!["a"], vec![" "]]);
}

#[test]
fn colours_are_left_out_when_the_program_changed_none_and_shaped_when_it_did() {
    let pane = Pane::default();
    assert!(serde_json::to_value(&pane).expect("json").get("colours").is_none());
    let pane = Pane {
        colours: ProgramColours {
            palette: vec![(1, [255, 0, 0])],
            foreground: Some([1, 2, 3]),
            ..ProgramColours::default()
        }
        .into(),
        ..Pane::default()
    };
    assert_eq!(
        serde_json::to_value(&pane).expect("json")["colours"],
        json!({"fg": [1, 2, 3], "palette": {"1": [255, 0, 0]}})
    );
}

mod a_style_read_back {
    use super::*;

    #[rstest]
    #[case::plain(Style::default())]
    #[case::coloured(Style { fg: Some(Colour::Index(2)), bg: Some(Colour::Rgb([1, 2, 3])), ..Style::default() })]
    #[case::every_attribute(Style {
        bold: true,
        dim: true,
        italic: true,
        blink: true,
        inverse: true,
        invisible: true,
        strikethrough: true,
        overline: true,
        ..Style::default()
    })]
    #[case::single_underline(Style { underline: true, ..Style::default() })]
    #[case::curly_coloured_underline(Style {
        underline: true,
        underline_style: Some(Underline::Curly),
        underline_colour: Some(Colour::Index(5)),
        ..Style::default()
    })]
    fn is_the_style_it_came_from(#[case] style: Style) {
        let (look, decor) = style.look();

        assert_eq!(Style::of(look, decor), style);
    }

    #[test]
    fn keeps_palette_indices_as_indices() {
        let (look, _) = Style { fg: Some(Colour::Index(9)), ..Style::default() }.look();

        assert_eq!((look.fg, look.bg), (Some(Color::Indexed(9)), Some(Color::Reset)));
    }
}
