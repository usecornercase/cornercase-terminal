use super::*;

fn started() -> Started {
    Started {
        already: false,
        restored: true,
        complete: true,
        cols: 160,
        rows: 48,
        size: SizeFrom::StartServer,
        boot: "1-2".into(),
        version: version::CURRENT.into(),
        theme: None,
    }
}

mod summary {
    use super::*;

    #[test]
    fn a_fresh_start_says_it_restored_the_session_and_its_size() {
        assert_eq!(summary(&started()), "started: restored the session (160×48)\n");
    }

    #[test]
    fn a_fresh_start_without_a_session_says_so() {
        assert_eq!(summary(&Started { restored: false, ..started() }), "started: no session to restore (160×48)\n");
    }

    #[test]
    fn a_running_server_says_where_its_size_comes_from() {
        let window = Started { already: true, cols: 120, rows: 40, size: SizeFrom::Window, ..started() };

        assert_eq!(summary(&window), "already running (window size 120×40)\n");
    }
}

mod warnings {
    use super::*;

    #[test]
    fn a_complete_start_of_this_version_warns_of_nothing() {
        assert_eq!(warnings(&started()), Vec::<String>::new());
    }

    #[test]
    fn an_incomplete_restore_points_at_the_log() {
        let warned = warnings(&Started { complete: false, ..started() });

        assert_eq!(warned.len(), 1);
        assert!(warned[0].contains("see server.log"), "{warned:?}");
    }

    #[test]
    fn a_server_of_another_version_asks_for_a_restart() {
        let warned = warnings(&Started { version: "0.0.1".into(), ..started() });

        assert_eq!(
            warned,
            [format!(
                "the running server is cornercase 0.0.1 and this is {}; run `cornercase restart`",
                version::CURRENT
            )]
        );
    }
}

mod older_servers {
    use super::*;
    use crate::control::ThemeInfo;

    #[test]
    fn a_theme_sent_to_a_server_that_does_not_answer_with_one_is_an_error_naming_its_version() {
        let error = ignored_theme(true, &started()).expect_err("refused").to_string();
        assert!(error.contains(version::CURRENT) && error.contains("takes no theme"), "{error}");
    }

    #[test]
    fn a_theme_the_server_answered_for_is_fine() {
        let answered = Started { theme: Some(ThemeInfo::default()), ..started() };
        assert!(ignored_theme(true, &answered).is_ok());
    }

    #[test]
    fn nothing_sent_nothing_to_check() {
        assert!(ignored_theme(false, &started()).is_ok());
    }
}
