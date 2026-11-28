use serde_json::json;

use super::*;

fn reading(ended: Option<PairEnded>) -> Reading {
    Reading { key: Some(PairKey::Browsers), json: false, ended, reader_left: false, renewed: false }
}

fn ended(paired: bool, error: Option<&str>) -> Reading {
    reading(Some(PairEnded { paired, chat: None, error: error.map(String::from) }))
}

fn text_of(key: &str, line: &Value) -> String {
    let line = serde_json::from_value(line.clone()).expect("a line");
    Reading { key: PairKey::of(key), ..reading(None) }.text(&line)
}

mod a_person_reads {
    use super::*;

    #[test]
    fn the_address_then_the_dialogs_lines() {
        let line = json!({"url": "https://mac/pair?t=abc", "lines": ["scan the code.", "log in as ada."]});

        assert_eq!(text_of("browsers", &line), "https://mac/pair?t=abc\nscan the code.\nlog in as ada.\n\n");
    }

    #[test]
    fn only_the_lines_while_there_is_no_address() {
        let line = json!({"url": null, "lines": ["starting Tailscale…"]});

        assert_eq!(text_of("browsers", &line), "starting Tailscale…\n\n");
    }

    #[test]
    fn what_connected_at_the_end() {
        let chat = json!({"paired": true, "chat": "@ada"});

        assert_eq!(text_of("browsers", &json!({"paired": true})), "a browser connected\n");
        assert_eq!(text_of("telegram.chat", &chat), "Telegram connected with @ada\n");
        assert_eq!(text_of("a.key.from.later", &json!({"paired": true})), "paired\n");
    }

    #[test]
    fn nothing_of_a_failure_on_standard_output() {
        assert_eq!(text_of("browsers", &json!({"paired": false, "error": "no"})), "");
    }
}

mod it_ends {
    use super::*;

    fn message(result: Result<()>) -> String {
        match result {
            Err(Error::Control(message)) => message,
            other => panic!("not a refusal: {other:?}"),
        }
    }

    #[test]
    fn well_once_paired() {
        assert!(ended(true, None).finished(|| false).is_ok());
    }

    #[test]
    fn with_the_servers_words_once_it_failed() {
        assert_eq!(message(ended(false, Some("why")).finished(|| false)), "why");
        assert_eq!(message(ended(false, None).finished(|| false)), NOT_PAIRED);
    }

    #[test]
    fn in_words_when_the_server_stops_first() {
        assert_eq!(message(reading(None).finished(|| false)), SERVER_LEFT);
    }

    #[test]
    fn quietly_when_its_reader_left() {
        assert!(reading(None).finished(|| true).is_ok());
        assert!(Reading { reader_left: true, ..reading(None) }.finished(|| false).is_ok());
    }
}

mod the_arguments {
    use clap::Parser as _;

    use crate::cli::args::config::ConfigAction;
    use crate::cli::args::{Cli, Command, Control};

    #[test]
    fn name_the_key_and_take_json_after_it() {
        let cli = Cli::try_parse_from(["cornercase", "config", "pair", "telegram.chat", "--json"]).expect("parses");

        let Some(Command::Control(Control::Config(config))) = cli.command else { panic!("not config") };
        assert!(config.json);
        let pair = matches!(config.action, Some(ConfigAction::Pair { key, routes: false, new_certificate: false }) if key == "telegram.chat");
        assert!(pair);
    }

    #[test]
    fn take_a_new_certificate_but_not_with_the_routes() {
        let renewed =
            Cli::try_parse_from(["cornercase", "config", "pair", "app", "--new-certificate"]).expect("parses");
        let both = Cli::try_parse_from(["cornercase", "config", "pair", "app", "--new-certificate", "--routes"]);

        let Some(Command::Control(Control::Config(config))) = renewed.command else { panic!("not config") };
        assert!(matches!(config.action, Some(ConfigAction::Pair { routes: false, new_certificate: true, .. })));
        assert!(both.is_err());
    }

    #[test]
    fn need_the_key() {
        assert!(Cli::try_parse_from(["cornercase", "config", "pair"]).is_err());
    }
}

mod the_routes {
    use super::*;

    #[test]
    fn print_one_address_per_line_with_brackets_for_ipv6() {
        let routes: Routes = serde_json::from_value(json!({"routes": [
            {"host": "192.168.1.20", "port": 47682},
            {"host": "fd7a:115c:a1e0::1", "port": 47682},
        ]}))
        .expect("routes");

        assert_eq!(routes_text(&routes), "192.168.1.20:47682\n[fd7a:115c:a1e0::1]:47682\n");
    }

    #[test]
    fn go_only_with_the_app() {
        let refused = run("browsers".into(), true, false, false);

        assert!(matches!(refused, Err(Error::WrongUsage(message)) if message.contains("config pair app")));
    }
}

mod a_new_certificate {
    use super::*;

    #[test]
    fn goes_only_with_the_app() {
        let refused = run("browsers".into(), false, true, false);

        assert!(matches!(refused, Err(Error::WrongUsage(message)) if message.contains("--new-certificate")));
    }
}
