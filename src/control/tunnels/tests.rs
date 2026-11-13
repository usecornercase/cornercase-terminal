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
