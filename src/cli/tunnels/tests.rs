use clap::Parser;

use super::*;
use crate::cli::Cli;
use crate::cli::args::{Command as Cmd, Control};

fn info(port: u16, local: Option<u16>, state: &str, error: Option<&str>, holder: u64) -> TunnelInfo {
    TunnelInfo { port, local, state: state.into(), error: error.map(str::to_string), holder }
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
        };

        assert_eq!(
            text(&list),
            "3000  down: Could not request local forwarding.\n4000  opening\n5173 → localhost:5174  up\n"
        );
    }

    #[test]
    fn names_the_window_when_several_hold_tunnels() {
        let list = TunnelList { tunnels: vec![info(5173, Some(5173), "up", None, 3)], windows: 2 };

        assert_eq!(text(&list), "5173 → localhost:5173  up (window 3)\n");
    }

    #[test]
    fn says_how_to_get_a_window_when_none_is_attached() {
        let said = text(&TunnelList::default());

        assert!(said.starts_with("no window holds tunnels for this server") && said.contains("cornercase remote"));
    }

    #[test]
    fn says_there_are_none_when_windows_hold_nothing() {
        assert_eq!(text(&TunnelList { tunnels: vec![], windows: 1 }), "no tunnels\n");
    }
}

mod opening {
    use super::*;

    #[test]
    fn says_where_each_window_listens() {
        let list = TunnelList { tunnels: vec![info(5173, Some(5174), "up", None, 3)], windows: 1 };

        assert_eq!(opened_text(5173, &list), "port 5173 is tunnelled to localhost:5174\n");
    }
}

mod parsing {
    use super::*;

    #[test]
    fn a_port_is_one_to_65535() {
        assert!(Cli::try_parse_from(["cornercase", "tunnel", "open", "0"]).is_err());
        assert!(Cli::try_parse_from(["cornercase", "tunnel", "open", "x"]).is_err());
        assert!(Cli::try_parse_from(["cornercase", "tunnel", "close", "70000"]).is_err());
    }

    #[test]
    fn the_commands_take_json() {
        let cli = Cli::try_parse_from(["cornercase", "tunnel", "open", "5173", "--json"]).expect("parse");
        match cli.command {
            Some(Cmd::Control(Control::Tunnel(args))) => match args.action {
                TunnelAction::Open(arg) => assert_eq!((arg.port, arg.print.json), (5173, true)),
                TunnelAction::Close(_) => panic!("not open"),
            },
            other => panic!("not tunnel: {other:?}"),
        }
        let cli = Cli::try_parse_from(["cornercase", "tunnels", "--json"]).expect("parse");
        assert!(matches!(cli.command, Some(Cmd::Control(Control::Tunnels(args))) if args.print.json));
    }

    #[test]
    fn both_commands_run_there_so_remote_reaches_another_machine() {
        for args in [&["tunnels"][..], &["tunnel", "open", "1"], &["tunnel", "close", "1"]] {
            let words = ["cornercase", "--remote", "vps"].into_iter().chain(args.iter().copied());
            let cli = Cli::try_parse_from(words).expect("parse");

            assert_eq!(crate::cli::reach::kind(cli.command.as_ref()), crate::cli::reach::Kind::There);
        }
    }
}
