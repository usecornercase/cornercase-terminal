use rstest::rstest;

use super::*;
use crate::test_util::fixture;

mod the_orders_stream {
    use super::*;

    #[rstest]
    #[case::open("open", fixture!("control/tunnel-orders/v1/open.json"))]
    #[case::close("close", fixture!("control/tunnel-orders/v1/close.json"))]
    #[case::forget("forget", fixture!("control/tunnel-orders/v1/forget.json"))]
    fn is_pinned_by_its_fixtures(#[case] order: &str, #[case] fixture: &str) {
        let line = TunnelOrderLine { order: order.into(), port: 5173 };

        let written = serde_json::to_string_pretty(&line).expect("json");
        let read: TunnelOrderLine = serde_json::from_str(fixture).expect("line");

        assert_eq!(written, fixture.trim_end());
        assert_eq!(read, line);
    }

    #[test]
    fn the_first_line_is_the_list_with_takers() {
        let list = TunnelList { takers: 1, ..TunnelList::default() };

        let written = serde_json::to_string_pretty(&list).expect("json");

        assert_eq!(written, fixture!("control/tunnel-orders/v1/taking.json").trim_end());
    }

    #[test]
    fn a_list_from_an_older_server_counts_no_takers() {
        let list: TunnelList = serde_json::from_str(r#"{"tunnels":[],"windows":1}"#).expect("list");

        assert_eq!(list.takers, 0);
        assert_eq!(list.windows, 1);
    }
}

mod the_direction {
    use super::*;

    fn info(port: u16, local: u16, holder: u64, kind: &str, direction: Option<Direction>) -> TunnelInfo {
        TunnelInfo { port, local: Some(local), state: "up".into(), error: None, holder, kind: kind.into(), direction }
    }

    #[test]
    fn a_reverse_entry_says_its_direction_and_a_forward_one_leaves_it_out() {
        let list = TunnelList {
            tunnels: vec![info(5173, 5174, 7, "window", None), info(9223, 9222, 33, "hold", Some(Direction::Reverse))],
            windows: 1,
            takers: 0,
        };

        let written = serde_json::to_string_pretty(&list).expect("json");
        let read: TunnelList = serde_json::from_str(fixture!("control/tunnels/list.json")).expect("list");

        assert_eq!(written, fixture!("control/tunnels/list.json").trim_end());
        assert_eq!(read, list);
    }

    #[test]
    fn a_forward_request_keeps_the_bytes_older_servers_get() {
        let hold = TunnelHold { port: 5173, local: 5174, direction: None };
        let port = TunnelPort { port: 5173, direction: None };

        assert_eq!(serde_json::to_string(&hold).expect("json"), r#"{"port":5173,"local":5174}"#);
        assert_eq!(serde_json::to_string(&port).expect("json"), r#"{"port":5173}"#);
    }

    #[test]
    fn a_reverse_request_says_so() {
        let hold = TunnelHold { port: 9223, local: 9222, direction: Some(Direction::Reverse) };
        let port = TunnelPort { port: 9223, direction: Some(Direction::Reverse) };

        assert_eq!(serde_json::to_string(&hold).expect("json"), r#"{"port":9223,"local":9222,"direction":"reverse"}"#);
        assert_eq!(serde_json::to_string(&port).expect("json"), r#"{"port":9223,"direction":"reverse"}"#);
    }

    #[test]
    fn a_list_from_an_older_server_has_no_direction() {
        let text =
            r#"{"tunnels":[{"port":1,"local":2,"state":"up","error":null,"holder":3,"kind":"hold"}],"windows":0}"#;

        let list: TunnelList = serde_json::from_str(text).expect("list");

        assert_eq!(list.tunnels[0].direction, None);
    }
}

mod the_end_of_a_hold {
    use super::*;

    #[rstest]
    #[case::closed(HoldEnd::Closed, "closed")]
    #[case::forgotten(HoldEnd::Forgotten, "forgotten")]
    #[case::server_stopped(HoldEnd::ServerStopped, "server-stopped")]
    #[case::server_restarted(HoldEnd::ServerRestarted, "server-restarted")]
    fn is_written_in_kebab_case_and_read_back(#[case] end: HoldEnd, #[case] name: &str) {
        let written = serde_json::to_string(&HoldEnded { ended: end }).expect("json");
        let read: HoldEnded = serde_json::from_str(&written).expect("line");

        assert_eq!(written, format!(r#"{{"ended":"{name}"}}"#));
        assert_eq!(read.ended, end);
    }

    #[test]
    fn is_pinned_by_its_fixture() {
        let fixture = fixture!("control/tunnels/ended.json");

        let written = serde_json::to_string_pretty(&HoldEnded { ended: HoldEnd::Forgotten }).expect("json");
        let read: HoldEnded = serde_json::from_str(fixture).expect("line");

        assert_eq!(written, fixture.trim_end());
        assert_eq!(read.ended, HoldEnd::Forgotten);
    }

    #[test]
    fn the_first_answer_is_not_an_end() {
        let first = r#"{"tunnels":[],"windows":0,"takers":0}"#;

        assert!(serde_json::from_str::<HoldEnded>(first).is_err());
    }

    #[rstest]
    #[case::closed(HoldEnd::Closed, false)]
    #[case::forgotten(HoldEnd::Forgotten, false)]
    #[case::server_stopped(HoldEnd::ServerStopped, true)]
    #[case::server_restarted(HoldEnd::ServerRestarted, true)]
    fn only_the_server_stopping_is_told_apart(#[case] end: HoldEnd, #[case] stopped: bool) {
        assert_eq!(end.server_stopped(), stopped);
    }
}
