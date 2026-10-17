use rstest::rstest;

use super::*;
use crate::control::contract::{FEATURES, VIEW_VERSION};
use crate::control::{Request, Status};

#[rstest]
#[case::none(None, None, Ok(None))]
#[case::both(Some(100), Some(30), Ok(Some((100, 30))))]
#[case::half(Some(100), None, Err("both"))]
#[case::too_wide(Some(1001), Some(30), Err("cols"))]
#[case::too_tall(Some(100), Some(501), Err("rows"))]
#[case::zero(Some(0), Some(30), Err("cols"))]
fn a_size_is_both_or_neither(
    #[case] cols: Option<u16>,
    #[case] rows: Option<u16>,
    #[case] expected: Result<Option<(u16, u16)>, &str>,
) {
    let got = size(cols, rows);

    match expected {
        Ok(size) => assert_eq!(got, Ok(size)),
        Err(word) => assert!(got.expect_err("refused").contains(word)),
    }
}

#[test]
fn the_view_in_a_request_is_left_out_when_absent() {
    let request = Request {
        caller: None,
        server: None,
        contract: None,
        boot: None,
        view: None,
        command: Command::Status(Status {}),
    };

    let text = serde_json::to_string(&request).expect("json");

    assert!(!text.contains("view"), "{text}");
    assert!(serde_json::to_string(&Request { view: Some(4), ..request }).expect("json").contains("\"view\":4"));
}

#[test]
fn the_commands_that_take_a_view_are_the_ones_that_act_on_a_place() {
    assert!(takes_view(&Command::Status(Status {})));
    assert!(takes_view(&Command::ViewSet(SetView::default())));
    assert!(takes_view(&Command::Go(crate::control::Go::default())));
    assert!(!takes_view(&Command::Stats(crate::control::Stats {})));
    assert!(!takes_view(&Command::View(OpenView::default())));
}

#[test]
fn views_are_a_feature_and_a_stream_of_the_contract() {
    assert!(FEATURES.contains(&"views"));
    assert_eq!(VIEW_VERSION, 1);
}

#[test]
fn a_view_that_is_skipped_by_a_reader_reads_back_whole() {
    let info = ViewInfo {
        id: 41,
        projection: 3,
        active: true,
        cols: Some(120),
        rows: Some(40),
        shown: Ids { project: Some(1), workspace: Some(2), tab: Some(3), pane: Some(4) },
        controls: true,
        visible: false,
    };

    let written = serde_json::to_string_pretty(&info).expect("json");

    assert_eq!(serde_json::from_str::<ViewInfo>(&written).expect("a view"), info);
}

#[test]
fn a_view_read_without_visible_is_visible() {
    let older: ViewInfo = serde_json::from_str(r#"{"id": 4, "active": true}"#).expect("a view");
    let hidden: ViewInfo = serde_json::from_str(r#"{"id": 4, "visible": false}"#).expect("a view");

    assert!(older.visible);
    assert!(!hidden.visible);
}
