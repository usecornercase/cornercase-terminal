use serde_json::json;

use super::*;
use crate::control::{Command, Request};
use crate::test_util::fixture;

fn shown() -> PairLine {
    PairLine::Shown(PairShown {
        url: Some("https://mac.tail1234.ts.net:8443/pair?t=0123456789abcdef0123456789abcdef".into()),
        lines: vec![
            "scan the code with your phone's camera.".into(),
            "the phone needs the Tailscale app, logged in as ada@example.com.".into(),
        ],
    })
}

fn waiting() -> PairLine {
    PairLine::Shown(PairShown { url: None, lines: vec!["starting Tailscale…".into()] })
}

fn paired() -> PairLine {
    PairLine::Ended(PairEnded { paired: true, chat: None, error: None })
}

fn chat() -> PairLine {
    PairLine::Ended(PairEnded { paired: true, chat: Some("@ada".into()), error: None })
}

fn failed() -> PairLine {
    let error = "tailscale is not connected (Stopped): open it and log in";
    PairLine::Ended(PairEnded { paired: false, chat: None, error: Some(error.into()) })
}

mod the_stream {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::the_address(shown(), fixture!("control/config-pair/v1/shown.json"))]
    #[case::no_address_yet(waiting(), fixture!("control/config-pair/v1/waiting.json"))]
    #[case::a_browser(paired(), fixture!("control/config-pair/v1/paired.json"))]
    #[case::a_chat(chat(), fixture!("control/config-pair/v1/chat.json"))]
    #[case::a_failure(failed(), fixture!("control/config-pair/v1/failed.json"))]
    fn is_pinned_by_its_fixtures(#[case] line: PairLine, #[case] fixture: &str) {
        let pinned: serde_json::Value = serde_json::from_str(fixture).expect("a fixture");

        assert_eq!(serde_json::to_value(&line).expect("json"), pinned);
        assert_eq!(serde_json::from_str::<PairLine>(fixture).expect("read back"), line);
    }

    #[test]
    fn the_address_is_null_while_there_is_none() {
        let written = serde_json::to_value(waiting()).expect("json");

        assert_eq!(written, json!({"url": null, "lines": ["starting Tailscale…"]}));
    }

    #[test]
    fn a_line_from_a_newer_server_still_reads() {
        let shown: PairLine = serde_json::from_str(r#"{"url": "u", "lines": [], "later": 1}"#).expect("a line");
        let ended: PairLine = serde_json::from_str(r#"{"paired": true, "later": 1}"#).expect("a line");

        assert!(matches!(shown, PairLine::Shown(_)));
        assert!(matches!(ended, PairLine::Ended(PairEnded { paired: true, .. })));
    }
}

mod the_request {
    use super::*;

    #[test]
    fn takes_the_key_and_logs_only_it() {
        let text = r#"{"command": "config-pair", "args": {"key": "telegram.chat"}}"#;

        let request: Request = serde_json::from_str(text).expect("a request");

        let pair = ConfigPair { key: "telegram.chat".into() };
        assert_eq!(request.command, Command::ConfigPair(pair));
        assert_eq!(request.command.name(), "config-pair");
        assert_eq!(request.command.fields(), [("key", "telegram.chat".to_string())]);
        assert!(!Command::chatty("config-pair"));
    }
}

mod a_key {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::browsers("browsers", PairKey::Browsers)]
    #[case::telegram("telegram.chat", PairKey::Chat(Via::Telegram))]
    #[case::discord("discord.chat", PairKey::Chat(Via::Discord))]
    fn is_named_as_config_remove_names_it(#[case] name: &str, #[case] key: PairKey) {
        assert_eq!(PairKey::of(name), Some(key));
        assert_eq!(key.name(), name);
    }

    #[rstest]
    #[case::a_token("telegram")]
    #[case::a_tracker("linear")]
    #[case::nothing("")]
    fn cannot_be_anything_else(#[case] name: &str) {
        assert_eq!(PairKey::of(name), None);
    }

    #[test]
    fn says_what_connected_as_the_window_does() {
        let telegram = PairKey::Chat(Via::Telegram);

        assert_eq!(PairKey::Browsers.connected(None), "a browser connected");
        assert_eq!(telegram.connected(Some("@ada")), "Telegram connected with @ada");
        assert_eq!(telegram.connected(Some("")), "Telegram connected");
        assert_eq!(PairKey::Chat(Via::Discord).connected(None), "Discord connected");
    }
}
