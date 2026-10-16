use super::*;
use crate::test_util::TempDir;

mod via {
    use super::*;

    #[test]
    fn every_platform_is_indexed_by_its_place_in_the_list() {
        assert!(Via::ALL.iter().enumerate().all(|(i, via)| via.index() == i));
    }

    #[test]
    fn discord_pairs_with_the_app_owner_through_its_install_link() {
        let (url, _) = Via::Discord.pairing_link("cornercase", "MTIzNDU2Nzg5.Gx.secret", "unused");

        assert!(url.starts_with("https://discord.com/oauth2/authorize?client_id=123456789&integration_type=1"));
        assert_eq!((Via::Telegram.pairs_by_owner(), Via::Discord.pairs_by_owner()), (false, true));
        assert_eq!(Via::Discord.pairing_code("/start abc"), None);
        assert_eq!(
            Via::Telegram.pair("http://127.0.0.1:1", "t", "hello"),
            (Advice::Read(None), Ok(Owned::Later(Duration::ZERO)))
        );
    }

    #[test]
    fn a_discord_owner_who_has_not_added_the_app_is_waited_for() {
        let server = crate::test_util::FakeHttp::start(vec![
            ("GET /oauth2/applications/@me", 200, r#"{"owner":{"id":"9","username":"ada"},"bot_public":true}"#),
            ("POST /users/@me/channels", 200, r#"{"id":"1200"}"#),
            ("POST /channels/1200/messages", 403, r#"{"message":"Cannot send messages to this user","code":50007}"#),
        ]);

        assert_eq!(
            Via::Discord.pair(&server.url(), "MTIzNDU2Nzg5.Gx.secret", "hello"),
            (Advice::Read(Some(discord::PUBLIC)), Ok(Owned::Later(Duration::ZERO)))
        );
    }

    #[test]
    fn a_discord_rate_limit_while_pairing_waits_as_long_as_discord_asks() {
        let server = crate::test_util::FakeHttp::start(vec![(
            "GET /oauth2/applications/@me",
            429,
            r#"{"message":"You are being rate limited.","retry_after":4.2}"#,
        )]);

        let owned = Via::Discord.pair(&server.url(), "MTIzNDU2Nzg5.Gx.secret", "hello");

        assert_eq!(owned, (Advice::Unread, Ok(Owned::Later(Duration::from_secs(5)))));
    }

    #[test]
    fn only_telegram_forces_a_reply() {
        assert_eq!((Via::Telegram.forces_reply(), Via::Discord.forces_reply()), (true, false));
    }
}

mod taps {
    use super::*;

    #[rstest::rstest]
    #[case::keys(Tap::Key(5, "esc".into()))]
    #[case::screen(Tap::Screen(5))]
    #[case::write(Tap::Write(12))]
    #[case::deliver(Tap::Deliver(5, 3))]
    #[case::new(Tap::New)]
    #[case::project(Tap::Draft(2, Choice::Project(7)))]
    #[case::workspace(Tap::Draft(2, Choice::Workspace(8)))]
    #[case::worktree(Tap::Draft(2, Choice::Worktree))]
    #[case::agent(Tap::Draft(2, Choice::Agent("claude".into())))]
    fn read_what_they_wrote(#[case] tap: Tap) {
        assert_eq!(Tap::read(&tap.data("ab12"), "ab12"), Ok(tap));
    }

    #[test]
    fn a_button_from_another_run_is_stale_and_garbage_is_unknown() {
        assert_eq!(Tap::read(&Tap::Screen(5).data("ab12"), "cd34"), Err(Unread::Stale));
        assert_eq!(Tap::read("ab12:zz:1", "ab12"), Err(Unread::Unknown));
        assert_eq!(Tap::read("ab12:s:x", "ab12"), Err(Unread::Unknown));
    }

    #[test]
    fn a_long_agent_name_does_not_fit_a_button() {
        assert!(Tap::Draft(1, Choice::Agent("claude".into())).fits("ab12", Via::Telegram));
        assert!(!Tap::Draft(1, Choice::Agent("x".repeat(60))).fits("ab12", Via::Telegram));
    }
}

mod choices {
    use super::*;

    #[test]
    fn the_options_of_a_permission_prompt_become_buttons() {
        let screen = "● I'll run the tests.\n\n  1. Fixed the parser\n  2. Added a test\n\n\
            ╭──────────────────────────────╮\n│ Bash command                 │\n│   cargo test                 │\n\
            │ Do you want to proceed?      │\n│ ❯ 1. Yes                     │\n│   2. Yes, and don't ask again │\n\
            │   3. No, and tell Claude what to do differently (esc) │\n╰──────────────────────────────╯\n";

        let found = choices(screen);

        assert_eq!(
            found,
            vec![
                ('1', "Yes".to_string()),
                ('2', "Yes, and don't ask again".to_string()),
                ('3', "No, and tell Claude what to do differently (esc)".to_string()),
            ]
        );
    }

    #[test]
    fn a_screen_without_options_has_none() {
        assert_eq!(choices("$ cargo test\nok\n"), Vec::new());
    }
}

mod saved {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    #[test]
    fn the_paired_chat_comes_back_from_a_private_file() {
        let dir = TempDir::new();
        let path = dir.path().join("telegram.json");
        let chat = Chat { id: 42, user: 9, name: "@ada".into(), paired: 1_760_000_000 };

        Saved::new("cornercase_bot".into(), Some(chat.clone())).save(&path).expect("save");

        assert_eq!(Saved::load(&path), Saved::new("cornercase_bot".into(), Some(chat)));
        let mode = std::fs::metadata(&path).expect("the file").permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn another_version_or_a_broken_file_is_nothing() {
        let dir = TempDir::new();
        let path = dir.path().join("telegram.json");
        std::fs::write(&path, r#"{"version":9,"bot":"x"}"#).expect("write");

        assert_eq!(Saved::load(&path), Saved::default());
        std::fs::write(&path, "{").expect("write");
        assert_eq!(Saved::load(&path), Saved::default());
    }
}
