use rstest::rstest;
use serde_json::json;

use super::*;
use crate::control::{Command, Request};

#[test]
fn a_start_server_request_travels_with_its_size_and_logs_it() {
    let request = Request {
        caller: None,
        server: None,
        contract: None,
        boot: None,
        command: Command::StartServer(StartServer { cols: Some(200), rows: None }),
    };

    let sent = serde_json::to_value(&request).expect("json");

    assert_eq!(
        sent,
        json!({"caller": null, "server": null, "command": "start-server", "args": {"cols": 200, "rows": null}})
    );
    assert_eq!(serde_json::from_value::<Request>(sent).expect("a request"), request);
    assert_eq!(request.command.fields(), [("cols", "200".to_string())]);
}

#[test]
fn without_a_size_it_is_the_default() {
    assert_eq!(StartServer::default().size(), Ok((DEFAULT_COLS, DEFAULT_ROWS)));
    assert_eq!((DEFAULT_COLS, DEFAULT_ROWS), (160, 48));
}

#[rstest]
#[case::only_cols(Some(120), None, (120, 48))]
#[case::only_rows(None, Some(30), (160, 30))]
#[case::smallest(Some(90), Some(10), (90, 10))]
#[case::largest(Some(1000), Some(500), (1000, 500))]
fn a_size_in_range_is_taken(#[case] cols: Option<u16>, #[case] rows: Option<u16>, #[case] size: (u16, u16)) {
    assert_eq!(StartServer { cols, rows }.size(), Ok(size));
}

#[rstest]
#[case::too_narrow(Some(89), None, "cols must be between 90 and 1000, not 89")]
#[case::too_wide(Some(1001), None, "cols must be between 90 and 1000, not 1001")]
#[case::too_short(None, Some(9), "rows must be between 10 and 500, not 9")]
#[case::too_tall(None, Some(501), "rows must be between 10 and 500, not 501")]
fn a_size_out_of_range_names_the_range(#[case] cols: Option<u16>, #[case] rows: Option<u16>, #[case] error: &str) {
    assert_eq!(StartServer { cols, rows }.size(), Err(error.to_string()));
}

#[test]
fn the_answer_names_where_its_size_comes_from() {
    let started = Started {
        already: true,
        restored: true,
        complete: true,
        cols: 120,
        rows: 40,
        size: SizeFrom::StartServer,
        boot: "1-2".into(),
        version: "0.1.0".into(),
    };

    let sent = serde_json::to_value(&started).expect("json");

    assert_eq!(
        sent,
        json!({"already": true, "restored": true, "complete": true, "cols": 120, "rows": 40,
               "size": "start-server", "boot": "1-2", "version": "0.1.0"})
    );
    assert_eq!(serde_json::from_value::<Started>(sent).expect("an answer"), started);
    assert_eq!(serde_json::to_value(SizeFrom::Window).expect("json"), json!("window"));
}

#[test]
fn an_answer_an_older_server_lacks_fields_of_still_reads() {
    let started: Started = serde_json::from_str(r#"{"already": true}"#).expect("an answer");

    assert!(started.already);
    assert_eq!(started.size, SizeFrom::Window);
}
