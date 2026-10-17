use rstest::rstest;
use serde_json::{Value, json};

use super::super::mirror::Mirror;
use super::*;
use crate::control::Layout;
use crate::control::contract::SURFACE_VERSION;
use crate::control::surface::{Area, Divider, Frame, Size};
use crate::test_util::fixture;

fn plain(text: &str) -> Vec<Run> {
    if text.is_empty() {
        return Vec::new();
    }
    vec![Run { style: 0, hyperlink: None, cells: text.chars().map(|c| c.to_string()).collect() }]
}

fn pane(id: u64, rows: &[&str]) -> Pane {
    let rows: Vec<Vec<Run>> = rows.iter().map(|row| plain(row)).collect();
    let height = u16::try_from(rows.len()).expect("rows");
    Pane {
        pane: id,
        active: id == 1,
        rect: Area { x: 0, y: 0, cols: 20, rows: height },
        size: Size { cols: 20, rows: height },
        lines: rows,
        ..Pane::default()
    }
}

fn content(panes: Vec<Pane>) -> Content {
    Content { layout: Some(Layout::Leaf(1)), styles: vec![Style::default()], panes, ..Content::default() }
}

fn one(rows: &[&str]) -> Content {
    content(vec![pane(1, rows)])
}

fn frame<B: Default>(revision: u64, content: B) -> Frame<B> {
    Frame { revision, surface: 1, tab: 1, cols: 20, rows: 10, content, ..Frame::default() }
}

fn patch(base: &Content, next: &Content) -> PaneChanges {
    let mut changes = changes(base, next).expect("a patch");
    assert_eq!(changes.panes.len(), 1, "{changes:?}");
    changes.panes.remove(0)
}

fn rows_of(changed: &PaneChanges) -> Vec<u16> {
    changed.lines.keys().copied().collect()
}

#[test]
fn nothing_changed_lists_no_pane() {
    let held = one(&["a", "b"]);

    assert_eq!(changes(&held, &held.clone()).expect("a patch").panes, []);
}

#[test]
fn a_changed_row_comes_whole_with_the_patchs_own_tables() {
    let green = Style { fg: Some(crate::control::surface::Colour::Index(2)), ..Style::default() };
    let was = one(&["abc", "def"]);
    let mut now = one(&["abc", ""]);
    now.styles.push(green);
    now.panes[0].lines[1] = vec![Run { style: 1, hyperlink: None, cells: vec!["x".into(), "y".into()] }];

    let all = changes(&was, &now).expect("a patch");

    assert_eq!(all.styles, [Style::default(), green]);
    assert_eq!(rows_of(&all.panes[0]), [1]);
    assert_eq!(all.panes[0].lines[&1][0].style, 1);
}

#[test]
fn rows_are_compared_by_their_look_not_by_their_style_numbers() {
    let red = Style { bold: true, ..Style::default() };
    let green = Style { italic: true, ..Style::default() };
    let styled = |style| vec![Run { style, hyperlink: None, cells: vec!["a".into()] }];
    let mut was = one(&["", ""]);
    was.styles = vec![Style::default(), red, green];
    was.panes[0].lines = vec![styled(1), styled(2)];
    let mut now = was.clone();
    now.styles = vec![Style::default(), green, red];
    now.panes[0].lines = vec![styled(2), styled(1)];

    assert_eq!(changes(&was, &now).expect("a patch").panes, []);
}

#[test]
fn a_hyperlink_is_compared_by_its_address() {
    let linked = |uri: &str| {
        let mut held = one(&[""]);
        held.hyperlinks = vec![Hyperlink { uri: uri.to_string() }];
        held.panes[0].lines = vec![vec![Run { style: 0, hyperlink: Some(0), cells: vec!["a".into()] }]];
        held
    };

    let changed = patch(&linked("https://a"), &linked("https://b"));
    let all = changes(&linked("https://a"), &linked("https://b")).expect("a patch");

    assert_eq!(rows_of(&changed), [0]);
    assert_eq!(all.hyperlinks, [Hyperlink { uri: "https://b".into() }]);
    assert_eq!(all.panes[0].lines[&0][0].hyperlink, Some(0));
}

#[test]
fn output_that_scrolls_up_is_a_copy_and_the_new_rows() {
    let was = one(&["one", "two", "three", "four", "five", "six"]);
    let now = one(&["two", "three", "four", "five", "six", "seven"]);

    let changed = patch(&was, &now);

    assert_eq!(changed.copy, Some(Copied { from: 1, to: 0, count: 5 }));
    assert_eq!(rows_of(&changed), [5]);
}

#[test]
fn a_footer_that_stays_put_is_not_sent_again() {
    let was = one(&["a1", "b2", "c3", "d4", "e5", "f6", "---", "> ", "status"]);
    let now = one(&["b2", "c3", "d4", "e5", "f6", "g7", "---", "> ", "status"]);

    let changed = patch(&was, &now);

    assert_eq!(changed.copy, Some(Copied { from: 1, to: 0, count: 5 }));
    assert_eq!(rows_of(&changed), [5]);
}

#[test]
fn scrolling_down_copies_the_other_way() {
    let was = one(&["two", "three", "four", "five", "six", "seven"]);
    let now = one(&["one", "two", "three", "four", "five", "six"]);

    let changed = patch(&was, &now);

    assert_eq!(changed.copy, Some(Copied { from: 0, to: 1, count: 5 }));
    assert_eq!(rows_of(&changed), [0]);
}

#[test]
fn a_copy_is_used_only_when_it_sends_fewer_rows() {
    let was = one(&["aa", "bb", "cc", "dd"]);
    let now = one(&["ee", "ff", "xx", "bb"]);

    let changed = patch(&was, &now);

    assert_eq!(changed.copy, Some(Copied { from: 1, to: 3, count: 1 }));
    assert_eq!(rows_of(&changed), [0, 1, 2]);
    let unrelated = patch(&was, &one(&["ee", "ff", "gg", "hh"]));
    assert_eq!(unrelated.copy, None);
    assert_eq!(rows_of(&unrelated), [0, 1, 2, 3]);
}

#[test]
fn many_identical_rows_still_copy_the_rows_that_moved() {
    let mut was: Vec<&str> = vec!["same"; 12];
    was.extend(["a1", "b2", "c3", "d4"]);
    let mut now: Vec<&str> = vec!["same"; 11];
    now.extend(["a1", "b2", "c3", "d4", "e5"]);

    let changed = patch(&one(&was), &one(&now));

    assert!(changed.lines.len() <= 2, "{changed:?}");
    let rebuilt = changes(&one(&was), &one(&now)).expect("a patch");
    assert_eq!(rebuilt.panes[0].lines.len(), changed.lines.len());
}

#[test]
fn identical_rows_alone_never_make_a_copy() {
    let was = one(&["same", "same", "same", "same"]);
    let now = one(&["same", "same", "same", "other"]);

    let changed = patch(&was, &now);

    assert_eq!(changed.copy, None);
    assert_eq!(rows_of(&changed), [3]);
}

#[test]
fn blank_rows_do_not_vote_for_a_copy() {
    let was = one(&["", "", "", "", "x"]);
    let now = one(&["", "", "", "y", ""]);

    let changed = patch(&was, &now);

    assert_eq!(changed.copy, None);
    assert_eq!(rows_of(&changed), [3, 4]);
}

#[test]
fn every_changed_pane_carries_its_cursor_and_what_else_changed() {
    let mut was = one(&["a"]);
    was.panes[0].cursor = Some(Cursor { x: 1, y: 0, ..Cursor::default() });
    was.panes[0].colours = Colours { fg: Some([1, 2, 3]), ..Colours::default() };
    let mut now = was.clone();
    now.panes[0].cursor = None;
    now.panes[0].colours = Colours::default();
    now.panes[0].active = false;
    now.panes[0].modes.bracketed_paste = true;
    now.panes[0].scroll = Scroll { total: 5, offset: 4, rows: 1 };

    let changed = patch(&was, &now);

    assert_eq!(changed.cursor, None);
    assert_eq!(changed.active, Some(false));
    assert_eq!(changed.colours, Some(Colours::default()));
    assert_eq!(changed.modes.map(|modes| modes.bracketed_paste), Some(true));
    assert_eq!(changed.scroll, Some(Scroll { total: 5, offset: 4, rows: 1 }));
    assert_eq!(changed.lines.len(), 0);
}

#[test]
fn a_pane_that_did_not_change_is_absent() {
    let was = content(vec![pane(1, &["a"]), pane(2, &["b"])]);
    let mut now = was.clone();
    now.panes[1].lines = vec![plain("c")];

    let all = changes(&was, &now).expect("a patch");

    assert_eq!(all.panes.iter().map(|changed| changed.pane).collect::<Vec<_>>(), [2]);
}

fn moved(edit: impl FnOnce(&mut Content)) -> Content {
    let mut held = content(vec![pane(1, &["a"]), pane(2, &["b"])]);
    edit(&mut held);
    held
}

#[rstest]
#[case::layout(|c: &mut Content| c.layout = Some(Layout::Leaf(2)))]
#[case::divider(|c: &mut Content| c.dividers.push(Divider::default()))]
#[case::pane_added(|c: &mut Content| c.panes.push(pane(3, &["c"])))]
#[case::panes_reordered(|c: &mut Content| c.panes.reverse())]
#[case::rect(|c: &mut Content| c.panes[0].rect.x = 3)]
#[case::terminal_size(|c: &mut Content| c.panes[1].size.cols = 7)]
#[case::rows(|c: &mut Content| c.panes[0].lines.push(Vec::new()))]
fn a_new_shape_needs_a_complete_frame(#[case] edit: fn(&mut Content)) {
    let was = moved(|_| {});

    assert_eq!(changes(&was, &moved(edit)), None);
}

fn styled_rows(rows: &[(&str, usize)]) -> Content {
    let mut held = one(&[]);
    held.styles = vec![Style::default(), Style { bold: true, ..Style::default() }];
    held.hyperlinks = vec![Hyperlink { uri: "https://x".into() }];
    held.panes[0].lines = rows
        .iter()
        .map(|(text, kind)| {
            if text.is_empty() {
                return Vec::new();
            }
            let hyperlink = (*kind == 2).then_some(0);
            let style = usize::from(*kind == 1);
            vec![Run { style, hyperlink, cells: text.chars().map(|c| c.to_string()).collect() }]
        })
        .collect();
    let height = u16::try_from(held.panes[0].lines.len()).expect("rows");
    held.panes[0].rect.rows = height;
    held.panes[0].size.rows = height;
    held
}

#[test]
fn applying_a_patch_gives_the_next_frame() {
    let steps = [
        styled_rows(&[("one", 0), ("two", 1), ("three", 2), ("four", 0), ("", 0), ("six", 0)]),
        styled_rows(&[("two", 1), ("three", 2), ("four", 0), ("", 0), ("six", 0), ("seven", 1)]),
        styled_rows(&[("two", 1), ("three", 2), ("four", 0), ("", 0), ("six", 0), ("seven", 1)]),
        styled_rows(&[("new", 2), ("two", 1), ("three", 2), ("four", 0), ("", 0), ("six", 0)]),
        styled_rows(&[("", 0), ("", 0), ("", 0), ("", 0), ("", 0), ("", 0)]),
        styled_rows(&[("a", 1), ("", 0), ("c", 2), ("", 0), ("e", 0), ("f", 1)]),
    ];
    let mut client = Mirror::of(&frame(1, steps[0].clone()));
    for (at, pair) in steps.windows(2).enumerate() {
        let revision = u64::try_from(at).expect("a revision") + 2;
        let Some(all) = changes(&pair[0], &pair[1]) else { panic!("a patch for step {at}") };
        let shipped = Frame { base: Some(revision - 1), ..frame(revision, all) };
        let text = shipped.streamed().expect("text");

        client.apply(&text).expect("applies");

        assert_eq!(client.shown, Mirror::of(&frame(revision, pair[1].clone())).shown, "step {at}");
    }
}

#[test]
fn a_patch_for_another_revision_is_refused() {
    let held = one(&["a"]);
    let mut client = Mirror::of(&frame(1, held.clone()));
    let all = changes(&held, &one(&["b"])).expect("a patch");
    let late = Frame { base: Some(5), ..frame(6, all) };

    assert!(client.apply(&late.streamed().expect("text")).is_err());
}

fn patch_frame() -> Frame<Changes> {
    serde_json::from_str(fixture!("control/surface/v1/patch.json")).expect("a patch")
}

#[test]
fn a_patch_is_written_as_its_fixture() {
    let fixture: Value = serde_json::from_str(fixture!("control/surface/v1/patch.json")).expect("fixture");

    assert_eq!(serde_json::to_value(patch_frame()).expect("json"), fixture);
}

#[test]
fn the_patch_fixture_reads_back_as_its_patch() {
    let patch = patch_frame();

    assert_eq!((SURFACE_VERSION, patch.base, patch.revision), (1, Some(3), 4));
    assert_eq!(patch.content.panes.len(), 2);
    assert_eq!(patch.content.panes[0].copy, Some(Copied { from: 1, to: 0, count: 3 }));
    assert_eq!(patch.content.panes[0].lines.keys().copied().collect::<Vec<_>>(), [3, 4]);
    assert_eq!(patch.content.panes[1].cursor, None);
    assert_eq!(patch.content.panes[1].colours, Some(Colours::default()));
}

#[test]
fn a_streamed_patch_is_its_fixture_under_ok() {
    let fixture: Value = serde_json::from_str(fixture!("control/surface/v1/patch.json")).expect("fixture");

    let streamed: Value = serde_json::from_str(&patch_frame().streamed().expect("text")).expect("json");

    assert_eq!(streamed, json!({"ok": fixture}));
}

#[test]
fn a_complete_frame_has_no_base() {
    let fixture: Value = serde_json::from_str(fixture!("control/surface/v1/frame.json")).expect("fixture");
    let frame: Frame = serde_json::from_value(fixture).expect("a frame");

    assert_eq!(frame.base, None);
    assert!(serde_json::to_value(frame).expect("json").get("base").is_none());
}
