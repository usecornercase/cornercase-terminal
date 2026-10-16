use std::os::unix::net::UnixStream;
use std::sync::mpsc;

use libghostty_vt::style::RgbColor;
use ratatui::layout::Rect;

use super::super::tests::a_bug::{COLS, ROWS, half_locked, hello, one_shell_in};
use super::*;
use crate::app::App;
use crate::control::Report;
use crate::host::theme::HostTheme;
use crate::protocol::ClientMessage;
use crate::server::ServerEvent;
use crate::test_util::{Locked, TempDir};

const ASKER: u64 = 900;

struct Cold {
    server: Server,
    _dir: TempDir,
}

impl Cold {
    fn new() -> Self {
        let dir = TempDir::new();
        let saved = one_shell_in(dir.path());
        Self::with(dir, Some(&saved))
    }

    fn with(dir: TempDir, saved: Option<&state::State>) -> Self {
        let session = dir.path().join("session.json");
        if let Some(saved) = saved {
            persist::save(&session, saved).expect("save a session");
        }
        let (tx, _) = mpsc::channel();
        let (app_tx, _) = mpsc::channel();
        let app = App::new("/bin/sh".into(), HostTheme::default(), dir.path().join("config.json"), app_tx);
        let todo_saver = Saver::new(dir.path().join("todos.json"), None);
        Self { server: Server::new(app, session, todo_saver, tx), _dir: dir }
    }

    fn ask(&mut self, request: &str) -> serde_json::Value {
        let _ = self.server.handle(ServerEvent::Message(ASKER, ClientMessage::Request(request.into())));
        let answers = self.server.app.take_answers();
        let text = answers.into_iter().find(|(client, _)| *client == ASKER).expect("an answer").1;
        serde_json::from_str(&text).expect("JSON")
    }

    fn start(&mut self, size: &str) -> Started {
        let answer = self.ask(&format!(r#"{{"command":"start-server","args":{{{size}}}}}"#));
        serde_json::from_value(answer["ok"].clone()).unwrap_or_else(|_| panic!("started: {answer}"))
    }

    fn panes(&mut self) -> Vec<u64> {
        let answer = self.ask(r#"{"command":"status","args":{}}"#);
        let report: Report = serde_json::from_value(answer["ok"].clone()).expect("a report");
        report.panes().iter().map(|row| row.pane).collect()
    }

    #[must_use = "the window stays attached while its end of the socket is kept"]
    fn attach(&mut self, theme: HostTheme) -> UnixStream {
        let (ours, theirs) = UnixStream::pair().expect("a socket pair");
        let _ = self.server.handle(ServerEvent::Accepted(ours));
        let id = self.server.next_client - 1;
        let hello = Box::new(hello(theme));
        let _ = self.server.handle(ServerEvent::Message(id, ClientMessage::Hello(hello)));
        theirs
    }

    fn backup(&self) -> std::path::PathBuf {
        self.server.session.with_extension("json.bak")
    }
}

mod starting {
    use super::*;

    #[test]
    fn restores_the_session_once_and_says_so() {
        let mut cold = Cold::new();

        let started = cold.start("");

        assert_eq!(
            (started.already, started.restored, started.complete, started.cols, started.rows, started.size),
            (false, true, true, 160, 48, SizeFrom::StartServer)
        );
        assert_eq!((started.boot, started.version), (control::server_token().to_string(), version::CURRENT.into()));
        assert_eq!(cold.server.area, Some(Rect::new(0, 0, 160, 48)));
        assert_eq!(cold.panes().len(), 1);
    }

    #[test]
    fn a_second_call_opens_nothing_and_keeps_the_size() {
        let mut cold = Cold::new();
        cold.start(r#""cols":120,"rows":40"#);
        let before = cold.panes();

        let again = cold.start(r#""cols":200,"rows":60"#);

        assert_eq!((again.already, again.cols, again.rows), (true, 120, 40));
        assert_eq!(cold.server.area, Some(Rect::new(0, 0, 120, 40)));
        assert_eq!(cold.panes(), before);
    }

    #[test]
    fn with_a_window_attached_the_window_keeps_the_size() {
        let mut cold = Cold::new();
        let _window = cold.attach(HostTheme::default());

        let started = cold.start(r#""cols":200,"rows":60"#);

        assert_eq!((started.already, started.size, started.cols, started.rows), (true, SizeFrom::Window, COLS, ROWS));
        assert_eq!(cold.server.area, Some(Rect::new(0, 0, COLS, ROWS)));
    }

    #[test]
    fn without_a_session_opens_a_shell_here() {
        let mut cold = Cold::with(TempDir::new(), None);

        let started = cold.start("");

        assert_eq!((started.restored, started.complete), (false, true));
        assert!(cold.server.app.has_terms());
    }

    #[test]
    fn an_incomplete_restore_says_so_and_keeps_a_copy() {
        let (dir, locked) = (TempDir::new(), Locked::new());
        let saved = half_locked(dir.path(), locked.path());
        let mut cold = Cold::with(dir, Some(&saved));

        let started = cold.start("");

        assert_eq!((started.restored, started.complete), (true, false));
        assert_eq!(state::load(&cold.backup()), Some(saved));
    }

    #[test]
    fn a_second_call_keeps_no_new_copy() {
        let mut cold = Cold::new();
        cold.start("");

        cold.start("");

        assert!(!cold.backup().exists());
    }

    #[test]
    fn a_size_out_of_range_starts_nothing() {
        let mut cold = Cold::new();

        let answer = cold.ask(r#"{"command":"start-server","args":{"cols":89}}"#);

        assert!(answer["error"].as_str().is_some_and(|e| e.contains("between 90 and 1000")), "{answer}");
        assert_eq!((cold.server.startup, cold.server.area), (Startup::Waiting, None));
    }
}

mod other_requests {
    use super::*;

    #[rstest::rstest]
    #[case::status(r#"{"command":"status","args":{}}"#)]
    #[case::events(r#"{"command":"events","args":{}}"#)]
    #[case::contract(r#"{"command":"contract","args":{}}"#)]
    #[case::stats(r#"{"command":"stats","args":{}}"#)]
    fn start_nothing(#[case] request: &str) {
        let mut cold = Cold::new();

        let _ = cold.server.handle(ServerEvent::Message(ASKER, ClientMessage::Request(request.into())));

        assert_eq!((cold.server.startup, cold.server.area), (Startup::Waiting, None));
        assert!(cold.server.app.is_empty());
    }
}

mod the_first_window {
    use super::*;

    const BACKGROUND: RgbColor = RgbColor { r: 0x1d, g: 0x20, b: 0x22 };

    #[test]
    fn does_not_restore_again_and_takes_the_size() {
        let mut cold = Cold::new();
        cold.start("");
        let before = cold.panes();

        let _window = cold.attach(HostTheme::default());

        assert_eq!(cold.panes(), before);
        assert_eq!(cold.server.startup, Startup::Windowed);
        assert_eq!((cold.server.area, cold.server.size_from), (Some(Rect::new(0, 0, COLS, ROWS)), SizeFrom::Window));
    }

    #[test]
    fn gives_its_colours_to_the_terminals_already_open() {
        let mut cold = Cold::new();
        cold.start("");
        assert!(cold.server.app.backgrounds().iter().all(|b| *b != Some(BACKGROUND)));

        let _window = cold.attach(HostTheme { background: Some(BACKGROUND), ..HostTheme::default() });

        let backgrounds = cold.server.app.backgrounds();
        assert!(!backgrounds.is_empty() && backgrounds.iter().all(|b| *b == Some(BACKGROUND)), "{backgrounds:?}");
    }

    #[test]
    fn of_a_server_that_was_never_started_restores_it() {
        let mut cold = Cold::new();

        let _window = cold.attach(HostTheme::default());

        assert_eq!(cold.server.startup, Startup::Windowed);
        assert_eq!(cold.panes().len(), 1);
    }
}
