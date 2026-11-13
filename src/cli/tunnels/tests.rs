use clap::Parser;

use super::*;
use crate::cli::Cli;
use crate::cli::args::{Command as Cmd, Control};

fn info(port: u16, local: Option<u16>, state: &str, error: Option<&str>, holder: u64) -> TunnelInfo {
    TunnelInfo {
        port,
        local,
        state: state.into(),
        error: error.map(str::to_string),
        holder,
        kind: "window".into(),
        direction: None,
    }
}

fn hold(port: u16, local: u16, holder: u64) -> TunnelInfo {
    TunnelInfo { kind: "hold".into(), ..info(port, Some(local), "up", None, holder) }
}

fn reverse_hold(port: u16, local: u16, holder: u64) -> TunnelInfo {
    TunnelInfo { direction: Some(Direction::Reverse), ..hold(port, local, holder) }
}

mod the_list {
    use super::*;

    #[test]
    fn says_each_tunnels_port_and_state() {
        let list = TunnelList {
            tunnels: vec![
                info(3000, None, "down", Some("Could not request local forwarding."), 3),
                info(4000, None, "opening", None, 3),
                info(5173, Some(5174), "up", None, 3),
            ],
            windows: 1,
            ..TunnelList::default()
        };

        assert_eq!(
            text(&list),
            "3000  down: Could not request local forwarding.\n4000  opening\n5173 → localhost:5174  up\n"
        );
    }

    #[test]
    fn names_the_window_when_several_hold_tunnels() {
        let list =
            TunnelList { tunnels: vec![info(5173, Some(5173), "up", None, 3)], windows: 2, ..TunnelList::default() };

        assert_eq!(text(&list), "5173 → localhost:5173  up (window 3)\n");
    }

    #[test]
    fn says_how_to_get_a_window_when_none_is_attached() {
        let said = text(&TunnelList::default());

        assert!(said.starts_with("no window holds tunnels for this server") && said.contains("cornercase remote"));
    }

    #[test]
    fn says_no_tunnels_when_only_a_program_takes_orders() {
        assert_eq!(text(&TunnelList { takers: 1, ..TunnelList::default() }), "no tunnels\n");
    }

    #[test]
    fn says_a_closed_tunnel_is_closed() {
        let list =
            TunnelList { tunnels: vec![info(3000, None, "closed", None, 3)], windows: 1, ..TunnelList::default() };

        assert_eq!(text(&list), "3000  closed\n");
    }

    #[test]
    fn marks_a_hold_and_prints_it_when_no_window_is_attached() {
        let list = TunnelList { tunnels: vec![hold(5173, 5174, 9)], windows: 0, ..TunnelList::default() };

        assert_eq!(text(&list), "5173 → localhost:5174  up (hold 9)\n");
    }

    #[test]
    fn names_the_window_when_a_hold_is_listed_beside_it() {
        let list = TunnelList {
            tunnels: vec![info(3000, Some(3000), "up", None, 3), hold(5173, 5174, 9)],
            windows: 1,
            ..TunnelList::default()
        };

        assert_eq!(text(&list), "3000 → localhost:3000  up (window 3)\n5173 → localhost:5174  up (hold 9)\n");
    }

    #[test]
    fn marks_a_reverse_hold_with_its_arrow_and_kind() {
        let list =
            TunnelList { tunnels: vec![hold(5173, 5174, 3), reverse_hold(9223, 9222, 4)], ..TunnelList::default() };

        assert_eq!(text(&list), "5173 → localhost:5174  up (hold 3)\n9223 ← localhost:9222  up (reverse hold 4)\n");
    }

    #[test]
    fn says_there_are_none_when_windows_hold_nothing() {
        assert_eq!(text(&TunnelList { tunnels: vec![], windows: 1, ..TunnelList::default() }), "no tunnels\n");
    }
}

mod opening {
    use super::*;

    #[test]
    fn says_where_each_window_listens() {
        let list =
            TunnelList { tunnels: vec![info(5173, Some(5174), "up", None, 3)], windows: 1, ..TunnelList::default() };

        assert_eq!(opened_text(5173, &list), "port 5173 is tunnelled to localhost:5174\n");
    }
}

mod opening_with_a_program {
    use super::*;

    #[test]
    fn names_a_hold_that_answered_an_open() {
        let list = TunnelList { tunnels: vec![hold(5173, 5174, 9)], takers: 1, ..TunnelList::default() };

        assert_eq!(opened_text(5173, &list), "port 5173 is tunnelled to localhost:5174 (hold 9)\n");
    }
}

mod opening_with_several_windows {
    use super::*;

    #[test]
    fn says_why_a_window_could_not() {
        let list = TunnelList {
            tunnels: vec![info(5173, Some(5173), "up", None, 3), info(5173, None, "down", Some("denied"), 4)],
            windows: 2,
            ..TunnelList::default()
        };

        assert_eq!(
            opened_text(5173, &list),
            "port 5173 is tunnelled to localhost:5173 (window 3)\nport 5173 is not tunnelled: denied (window 4)\n"
        );
    }
}

mod parsing {
    use super::*;
    use crate::cli::args::tunnels::HoldArg;

    #[test]
    fn a_port_is_one_to_65535() {
        assert!(Cli::try_parse_from(["cornercase", "tunnel", "open", "0"]).is_err());
        assert!(Cli::try_parse_from(["cornercase", "tunnel", "open", "x"]).is_err());
        assert!(Cli::try_parse_from(["cornercase", "tunnel", "close", "70000"]).is_err());
        assert!(Cli::try_parse_from(["cornercase", "tunnel", "hold", "0"]).is_err());
    }

    fn action_of(args: &[&str]) -> TunnelAction {
        let words = ["cornercase", "tunnel"].into_iter().chain(args.iter().copied());
        match Cli::try_parse_from(words).expect("parse").command {
            Some(Cmd::Control(Control::Tunnel(args))) => args.action,
            other => panic!("not tunnel: {other:?}"),
        }
    }

    #[test]
    fn forget_and_hold_take_a_port() {
        assert!(matches!(action_of(&["forget", "5173"]), TunnelAction::Forget(arg) if arg.port == 5173));
        assert!(matches!(action_of(&["hold", "5173"]), TunnelAction::Hold(arg) if arg.port == Some(5173)));
    }

    fn hold_of(args: &[&str]) -> HoldArg {
        let all = ["hold"].into_iter().chain(args.iter().copied()).collect::<Vec<_>>();
        match action_of(&all) {
            TunnelAction::Hold(arg) => arg,
            other => panic!("not hold: {other:?}"),
        }
    }

    #[test]
    fn hold_takes_the_local_port_it_had_before() {
        let arg = hold_of(&["5173", "--local", "5174"]);

        assert_eq!((arg.port, arg.local, arg.reverse, arg.there), (Some(5173), Some(5174), None, None));
    }

    #[test]
    fn hold_can_be_reverse_with_a_port_there() {
        let arg = hold_of(&["--reverse", "9222"]);
        assert_eq!((arg.port, arg.reverse, arg.there), (None, Some(9222), None));

        let arg = hold_of(&["--reverse", "9222", "--there", "9223"]);
        assert_eq!((arg.reverse, arg.there), (Some(9222), Some(9223)));
    }

    #[rstest::rstest]
    #[case::there_needs_reverse(&["hold", "--there", "9223"])]
    #[case::reverse_and_a_port(&["hold", "5173", "--reverse", "9222"])]
    #[case::reverse_and_local(&["hold", "--reverse", "9222", "--local", "1"])]
    #[case::reverse_port_zero(&["hold", "--reverse", "0"])]
    #[case::local_port_zero(&["hold", "--local", "0", "5173"])]
    #[case::nothing_to_hold(&["hold"])]
    #[case::open_has_no_reverse(&["open", "--reverse", "5173"])]
    fn a_hold_or_an_open_that_asks_for_too_much_is_refused(#[case] args: &[&str]) {
        let words = ["cornercase", "tunnel"].into_iter().chain(args.iter().copied());

        assert!(Cli::try_parse_from(words).is_err());
    }

    #[test]
    fn close_and_forget_take_reverse() {
        assert!(matches!(action_of(&["close", "--reverse", "9223"]), TunnelAction::Close(arg) if arg.reverse));
        assert!(matches!(action_of(&["forget", "--reverse", "9223"]), TunnelAction::Forget(arg) if arg.reverse));
        assert!(matches!(action_of(&["close", "9223"]), TunnelAction::Close(arg) if !arg.reverse));
    }

    #[test]
    fn closing_and_forgetting_a_reverse_tunnel_say_so() {
        let list = TunnelList::default();

        assert_eq!(
            done_text(&action_of(&["close", "--reverse", "9223"]), &list),
            "closed the reverse tunnel of port 9223\n"
        );
        assert_eq!(
            done_text(&action_of(&["forget", "--reverse", "9223"]), &list),
            "forgot the reverse tunnel of port 9223\n"
        );
    }

    #[test]
    fn orders_takes_no_port_and_no_json() {
        assert!(matches!(action_of(&["orders"]), TunnelAction::Orders));
        assert!(Cli::try_parse_from(["cornercase", "tunnel", "orders", "1"]).is_err());
        assert!(Cli::try_parse_from(["cornercase", "tunnel", "orders", "--json"]).is_err());
    }

    #[test]
    fn failed_takes_a_port_and_a_reason() {
        let TunnelAction::Failed(arg) = action_of(&["failed", "3000", "no", "--json"]) else { panic!("not failed") };

        assert_eq!((arg.port, arg.reason.as_str(), arg.print.json), (3000, "no", true));
        assert!(Cli::try_parse_from(["cornercase", "tunnel", "failed", "0", "no"]).is_err());
        assert!(Cli::try_parse_from(["cornercase", "tunnel", "failed", "3000"]).is_err());
    }

    #[test]
    fn failing_says_which_port() {
        assert_eq!(failed_text(3000), "reported that port 3000 could not be tunnelled\n");
        assert_eq!(done_text(&action_of(&["failed", "3000", "no"]), &TunnelList::default()), "");
    }

    #[test]
    fn hold_prints_json_always_so_it_takes_no_flag() {
        assert!(Cli::try_parse_from(["cornercase", "tunnel", "hold", "5173", "--json"]).is_err());
    }

    #[test]
    fn forgetting_says_which_port() {
        let said = done_text(&action_of(&["forget", "5173"]), &TunnelList::default());

        assert_eq!(said, "forgot the tunnel of port 5173\n");
    }

    #[test]
    fn the_commands_take_json() {
        let cli = Cli::try_parse_from(["cornercase", "tunnel", "open", "5173", "--json"]).expect("parse");
        match cli.command {
            Some(Cmd::Control(Control::Tunnel(args))) => match args.action {
                TunnelAction::Open(arg) => assert_eq!((arg.port, arg.print.json), (5173, true)),
                TunnelAction::Close(_)
                | TunnelAction::Forget(_)
                | TunnelAction::Hold(_)
                | TunnelAction::Orders
                | TunnelAction::Failed(_) => panic!("not open"),
            },
            other => panic!("not tunnel: {other:?}"),
        }
        let cli = Cli::try_parse_from(["cornercase", "tunnels", "--json"]).expect("parse");
        assert!(matches!(cli.command, Some(Cmd::Control(Control::Tunnels(args))) if args.print.json));
    }

    #[test]
    fn both_commands_run_there_so_remote_reaches_another_machine() {
        let all = [
            &["tunnels"][..],
            &["tunnel", "open", "1"],
            &["tunnel", "close", "1"],
            &["tunnel", "forget", "1"],
            &["tunnel", "hold", "1"],
            &["tunnel", "hold", "--reverse", "1"],
            &["tunnel", "close", "--reverse", "1"],
            &["tunnel", "orders"],
            &["tunnel", "failed", "1", "x"],
        ];
        for args in all {
            let words = ["cornercase", "--remote", "vps"].into_iter().chain(args.iter().copied());
            let cli = Cli::try_parse_from(words).expect("parse");

            assert_eq!(crate::cli::reach::kind(cli.command.as_ref()), crate::cli::reach::Kind::There);
        }
    }
}
