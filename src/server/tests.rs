use super::*;
use crate::config;
use crate::state;
use crate::test_util::TempDir;
use crate::version;

mod lock {
    use super::*;

    #[test]
    fn lets_only_one_server_hold_a_socket() {
        let tmp = TempDir::new();
        let socket = tmp.path().join("server.sock");
        let _first = lock(&socket).expect("first lock");

        assert!(matches!(lock(&socket), Err(Error::ServerRunning(_))));
    }

    #[test]
    fn is_free_again_once_the_holder_is_gone() {
        let tmp = TempDir::new();
        let socket = tmp.path().join("server.sock");
        drop(lock(&socket).expect("first lock"));

        crate::test_util::wait_until("the lock is free", || lock(&socket).is_ok());
    }
}

mod a_bug {
    use std::cell::Cell;

    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent};

    use super::*;
    use crate::graphics::Support;
    use crate::state::{PaneState, ProjectState, State, TabState, WorkspaceState};
    use crate::test_util::wait_until;

    pub(super) const COLS: u16 = 100;
    pub(super) const ROWS: u16 = 20;
    const BUG: &str = "cornercase hit a bug, see server.log";
    const PANE_MENU: &str = "split right";
    const NOBODY: u64 = u64::MAX;
    const BUGGY: ServerEvent = ServerEvent::Gone(NOBODY);
    const BUGGY_KEY: KeyCode = KeyCode::F(12);

    fn buggy(step: &Step) -> bool {
        match step {
            Step::Event(ServerEvent::Gone(NOBODY)) => true,
            Step::Event(ServerEvent::Message(_, ClientMessage::Event(Event::Key(key)))) => key.code == BUGGY_KEY,
            _ => false,
        }
    }

    fn buggy_events(server: &mut Server, step: Step) -> ControlFlow<()> {
        assert!(!buggy(&step), "a bug while handling an event");
        server.step(step)
    }

    pub(super) fn one_shell_in(dir: &Path) -> State {
        let tabs = vec![TabState {
            name: None,
            panes: vec![PaneState {
                cwd: Some(dir.to_path_buf()),
                right_clicks: false,
                agent: None,
                done: false,
                pending: false,
            }],
            active: 0,
            layout: None,
        }];
        let path = dir.to_path_buf();
        let workspace = WorkspaceState {
            path: path.clone(),
            name: None,
            worktree: false,
            tabs,
            active: 0,
            base: None,
            collapsed: false,
            run: None,
            role_worktrees: Vec::new(),
        };
        let project =
            ProjectState { path, name: None, group: None, workspaces: vec![workspace], active: 0, collapsed: false };
        State { version: state::VERSION, projects: vec![project], ..State::default() }
    }

    pub(super) struct Attached {
        pub(super) server: Server,
        rx: Receiver<ServerEvent>,
        client: Client,
        dir: TempDir,
    }

    pub(super) struct Client {
        id: u64,
        tx: Sender<ServerEvent>,
        frames: Receiver<ServerMessage>,
        screen: vt100::Parser,
        pub(super) raw: Vec<u8>,
        clears: usize,
    }

    fn connect(
        server: &mut Server,
        tx: Sender<ServerEvent>,
        id: u64,
        graphics: Support,
        pointer: Option<pointer::Terminal>,
    ) -> Client {
        let (ours, mut theirs) = UnixStream::pair().expect("a socket pair");
        let _ = server.handle(ServerEvent::Accepted(ours));
        let (version, build, theme) = (protocol::VERSION, version::build_id(), HostTheme::default());
        let (notify, probe) = (Channel::Bell, String::new());
        let hello = Hello {
            version,
            build,
            cols: COLS,
            rows: ROWS,
            theme,
            notify,
            terminal: None,
            pointer,
            graphics,
            probe,
            zone: "UTC".into(),
        };
        let _ = server.handle(ServerEvent::Message(id, ClientMessage::Hello(Box::new(hello))));
        let (frames_tx, frames) = mpsc::channel();
        thread::spawn(move || {
            while let Ok(Some(msg)) = protocol::recv::<ServerMessage>(&mut theirs) {
                if frames_tx.send(msg).is_err() {
                    return;
                }
            }
        });
        Client { id, tx, frames, screen: vt100::Parser::new(ROWS, COLS, 0), raw: Vec::new(), clears: 0 }
    }

    impl Attached {
        pub(super) fn new() -> Self {
            Self::seeing(Support::default())
        }

        pub(super) fn seeing(graphics: Support) -> Self {
            let dir = TempDir::new();
            let saved = one_shell_in(dir.path());
            Self::with_graphics(dir, &saved, graphics)
        }

        pub(super) fn with(dir: TempDir, saved: &State) -> Self {
            Self::with_graphics(dir, saved, Support::default())
        }

        fn with_graphics(dir: TempDir, saved: &State, graphics: Support) -> Self {
            let session = dir.path().join("session.json");
            persist::save(&session, saved).expect("save a session");
            let (tx, rx) = mpsc::channel();
            let (app_tx, app_rx) = mpsc::channel();
            spawn_forwarder(app_rx, tx.clone());
            let app = App::new("/bin/sh".into(), HostTheme::default(), dir.path().join("config.json"), app_tx);
            let todo_saver = Saver::new(dir.path().join("todos.json"), None);
            let mut server = Server::new(app, session, todo_saver, tx.clone());
            let client = connect(&mut server, tx, 1, graphics, None);
            Self { server, rx, client, dir }
        }

        pub(super) fn dir(&self) -> &Path {
            self.dir.path()
        }

        pub(super) fn join(&mut self, graphics: Support) -> Client {
            self.join_with(graphics, None)
        }

        pub(super) fn join_with(&mut self, graphics: Support, pointer: Option<pointer::Terminal>) -> Client {
            let id = self.server.next_client;
            connect(&mut self.server, self.client.tx.clone(), id, graphics, pointer)
        }

        pub(super) fn shows_frame(&self, bytes: &[u8]) {
            self.received(|msg| matches!(msg, ServerMessage::Frame(frame) if frame == bytes));
        }

        pub(super) fn received(&self, last: impl Fn(&ServerMessage) -> bool) -> Vec<ServerMessage> {
            let mut got = Vec::new();
            wait_until("the message arrives", || {
                got.extend(std::iter::from_fn(|| self.client.frames.try_recv().ok()));
                got.iter().any(&last)
            });
            got
        }

        pub(super) fn serve(
            self,
            run: impl Fn(&mut Server, Step) -> ControlFlow<()>,
            script: impl FnOnce(&mut Client) + Send + 'static,
        ) {
            let Self { mut server, rx, mut client, dir: _dir } = self;
            let stop = client.tx.clone();
            let script = thread::spawn(move || {
                let done = panics::contain(|| script(&mut client));
                let _ = stop.send(ServerEvent::Shutdown);
                done.expect("the script runs to its end");
            });
            server.serve_with(&rx, run);
            script.join().expect("the script passes");
        }
    }

    impl Client {
        fn send(&self, ev: ServerEvent) {
            self.tx.send(ev).expect("the server listens");
        }

        pub(super) fn input(&self, ev: Event) {
            self.send(ServerEvent::Message(self.id, ClientMessage::Event(ev)));
        }

        pub(super) fn key(&self, code: KeyCode) {
            self.input(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)));
        }

        pub(super) fn click(&self, column: u16, row: u16) {
            for kind in [MouseEventKind::Down(MouseButton::Left), MouseEventKind::Up(MouseButton::Left)] {
                self.input(Event::Mouse(MouseEvent { kind, column, row, modifiers: KeyModifiers::NONE }));
            }
        }

        pub(super) fn click_on(&mut self, text: &str) {
            self.shows(text);
            let rows = self.screen.screen().rows(0, COLS).collect::<Vec<_>>();
            let (row, column) = rows
                .iter()
                .enumerate()
                .rev()
                .find_map(|(y, line)| Some((y, line.find(text)?)))
                .expect("the text is on screen");
            let column = u16::try_from(rows[row][..column].chars().count()).expect("on screen");
            self.click(column + 1, u16::try_from(row).expect("on screen"));
        }

        pub(super) fn type_line(&self, line: &str) {
            line.chars().map(KeyCode::Char).chain([KeyCode::Enter]).for_each(|code| self.key(code));
        }

        fn right_click_in_the_pane(&self) {
            for kind in [MouseEventKind::Down(MouseButton::Right), MouseEventKind::Up(MouseButton::Right)] {
                let ev = MouseEvent { kind, column: COLS - 10, row: 3, modifiers: KeyModifiers::NONE };
                self.input(Event::Mouse(ev));
            }
        }

        pub(super) fn text(&self) -> String {
            self.screen.screen().contents()
        }

        pub(super) fn until(&mut self, what: &str, cond: impl Fn(&Self) -> bool) {
            wait_until(what, || {
                while let Ok(msg) = self.frames.try_recv() {
                    if let ServerMessage::Frame(bytes) = msg {
                        self.clears += usize::from(bytes == CLEAR_SCREEN);
                        self.screen.process(&bytes);
                        self.raw.extend_from_slice(&bytes);
                    }
                }
                cond(self)
            });
        }

        pub(super) fn count(&self, needle: &[u8]) -> usize {
            self.raw.windows(needle.len()).filter(|w| *w == needle).count()
        }

        pub(super) fn shows(&mut self, text: &str) {
            self.until(text, |client| client.text().contains(text));
        }
    }

    #[test]
    fn an_event_that_panics_is_dropped_and_the_next_ones_are_handled() {
        Attached::new().serve(buggy_events, |client| {
            client.send(BUGGY);
            client.shows(BUG);
            client.type_line("echo still-\"\"alive");
            client.shows("still-alive");
        });
    }

    #[test]
    fn a_panic_while_drawing_starts_every_screen_over() {
        let armed = Cell::new(false);
        let run = move |server: &mut Server, step: Step| {
            armed.set(armed.get() || buggy(&step));
            assert!(!(matches!(step, Step::Draw) && armed.replace(false)), "a bug while drawing");
            server.step(step)
        };
        Attached::new().serve(run, |client| {
            client.shows("Quit");
            let before = client.clears;
            client.send(BUGGY);
            client.until("the screen is cleared", |client| client.clears > before);
            client.shows(BUG);
        });
    }

    #[test]
    fn only_a_panic_in_what_the_user_does_closes_what_is_open() {
        Attached::new().serve(buggy_events, |client| {
            client.right_click_in_the_pane();
            client.shows(PANE_MENU);
            client.send(BUGGY);
            client.shows(BUG);
            assert!(client.text().contains(PANE_MENU));
            client.key(BUGGY_KEY);
            client.until("the menu closes", |client| !client.text().contains(PANE_MENU));
        });
    }
}

mod streaming {
    use super::*;
    use crate::control::{Event, Response};

    fn client() -> (Client, Receiver<ServerMessage>) {
        let (tx, rx) = mpsc::channel();
        let out = Outbox { tx, queued: Arc::new(AtomicUsize::new(0)) };
        let writer = thread::spawn(|| {});
        let graphics = Graphics::default();
        (
            Client {
                zone: "UTC".into(),
                id: 1,
                size: None,
                used: 0,
                notify: Channel::Bell,
                pointer: None,
                shape: None,
                graphics,
                screen: None,
                out,
                writer,
                dropped: 0,
            },
            rx,
        )
    }

    fn texts(rx: &Receiver<ServerMessage>) -> Vec<String> {
        rx.try_iter()
            .map(|msg| match msg {
                ServerMessage::Response(text) => text,
                _ => panic!("not an answer"),
            })
            .collect()
    }

    #[test]
    fn a_reader_that_falls_behind_loses_events_then_is_told_how_many() {
        let (mut client, rx) = client();
        for n in 0..EVENTS_QUEUED + 3 {
            client.stream(n.to_string());
        }
        let queued = texts(&rx);

        client.out.queued.store(0, Ordering::Relaxed);
        client.stream("next".into());

        let after = texts(&rx);
        let Ok(Response::Ok(notice)) = serde_json::from_str(&after[0]) else { panic!("not a notice: {after:?}") };
        let notice: Event = serde_json::from_value(notice).expect("an event");
        assert_eq!(queued.len(), EVENTS_QUEUED);
        assert_eq!((notice.what, &after[1..]), (What::Dropped { count: 3 }, &["next".to_string()][..]));
    }

    #[test]
    fn the_notice_goes_out_once_there_is_room_even_without_a_new_event() {
        let (mut client, rx) = client();
        client.out.queued.store(EVENTS_QUEUED, Ordering::Relaxed);
        client.stream("lost".into());

        client.catch_up();
        let full = texts(&rx);
        client.out.queued.store(0, Ordering::Relaxed);
        client.catch_up();

        assert_eq!((full.len(), texts(&rx).len(), client.dropped), (0, 1, 0));
    }
}

mod restarting_when_idle {
    use super::a_bug::Attached;
    use super::*;
    use crate::test_util::wait_until;

    #[test]
    fn the_command_is_answered_before_every_client_is_told() {
        let mut attached = Attached::new();
        let request = r#"{"command":"restart-when-idle","args":{}}"#.to_string();
        let _ = attached.server.handle(ServerEvent::Message(1, ClientMessage::Request(request)));

        wait_until("no agent is at work", || attached.server.step(Step::Refresh).is_break());
        attached.server.shutdown();

        let told = attached.received(|msg| matches!(msg, ServerMessage::Restart(_)));
        let last: Vec<&ServerMessage> = told.iter().filter(|msg| !matches!(msg, ServerMessage::Frame(_))).collect();
        assert!(
            matches!(last[..], [ServerMessage::Response(text), ServerMessage::Restart(_)] if text.starts_with(r#"{"ok""#)),
            "{last:?}"
        );
    }
}

mod restore {
    use super::a_bug::{Attached, one_shell_in};
    use super::*;
    use crate::test_util::Locked;

    fn backup(attached: &Attached) -> PathBuf {
        attached.server.session.with_extension("json.bak")
    }

    #[test]
    fn a_complete_restore_leaves_no_backup() {
        let attached = Attached::new();

        assert!(!backup(&attached).exists());
    }

    #[test]
    fn an_incomplete_restore_keeps_the_session_it_read() {
        let (dir, locked) = (TempDir::new(), Locked::new());
        let mut saved = one_shell_in(dir.path());
        saved.projects.extend(one_shell_in(locked.path()).projects);

        let attached = Attached::with(dir, &saved);

        assert_eq!(state::load(&backup(&attached)), Some(saved));
    }

    #[test]
    fn a_session_whose_every_tab_fails_still_opens_a_shell() {
        let locked = Locked::new();

        let attached = Attached::with(TempDir::new(), &one_shell_in(locked.path()));

        assert!(attached.server.app.has_terms(), "a shell opens in the current folder");
    }

    #[test]
    fn a_session_saved_without_tabs_comes_back_as_it_was() {
        let dir = TempDir::new();
        let mut saved = one_shell_in(dir.path());
        saved.projects[0].workspaces[0].tabs.clear();

        let attached = Attached::with(dir, &saved);

        assert_eq!(attached.server.app.state().projects.len(), 1);
    }
}

mod showing_images {
    use super::a_bug::{Attached, Client};
    use super::*;
    use crate::graphics::{CellSize, Protocol, Support, Tmux};
    use crate::ui;

    const TRANSMIT: &[u8] = b"a=t,";
    const PLACE: &[u8] = b"a=p,";
    const KITTY: &[u8] = b"\x1b_G";
    const ITERM: &[u8] = b"\x1b]1337;File=";

    fn support(protocol: Protocol) -> Support {
        Support {
            protocol: Some(protocol),
            missing: None,
            cell: Some(CellSize { width: 10, height: 20 }),
            tmux: Tmux::None,
            id_hi: 42,
        }
    }

    fn with_image(graphics: Support) -> Attached {
        let attached = Attached::seeing(graphics);
        let pixels = image::RgbaImage::from_pixel(400, 200, image::Rgba([200, 30, 30, 255]));
        pixels.save_with_format(attached.dir().join("logo.png"), image::ImageFormat::Png).expect("a png");
        attached
    }

    fn open_the_image(client: &mut Client) {
        client.type_line("echo lo\"\"go.png");
        client.click_on("logo.png");
        client.shows("400×200");
    }

    fn settings() -> Position {
        ui::full_layout(
            Rect::new(0, 0, 100, 20),
            state::Widths::default(),
            true,
            config::Sidebar::default(),
            false,
            false,
        )
        .settings
        .as_position()
    }

    #[test]
    fn kitty_gets_the_image_once_and_only_a_new_placement_when_the_window_shrinks() {
        let attached = with_image(support(Protocol::Kitty));
        attached.serve(Server::step, |client| {
            open_the_image(client);
            client.until("the image is sent", |c| c.count(TRANSMIT) == 1);
            let placed = client.count(PLACE);

            client.input(Event::Resize(100, 12));

            client.until("the image is placed again", |c| c.count(PLACE) > placed);
            assert_eq!(client.count(TRANSMIT), 1);
        });
    }

    #[test]
    fn an_inline_image_waits_while_a_dialog_is_open_and_comes_back_after() {
        let attached = with_image(support(Protocol::Iterm));
        attached.serve(Server::step, |client| {
            open_the_image(client);
            client.until("the image is sent", |c| c.count(ITERM) == 1);

            let at = settings();
            client.click(at.x, at.y);
            client.shows("Worktrees");
            assert_eq!(client.count(ITERM), 1, "nothing while the dialog is open");
            client.key(KeyCode::Esc);

            client.until("the image is sent again", |c| c.count(ITERM) == 2);
        });
    }

    #[test]
    fn a_window_without_images_gets_none_while_another_gets_its_own() {
        let mut attached = with_image(support(Protocol::Kitty));
        let mut blind = attached.join(Support::default());
        attached.serve(Server::step, move |client| {
            open_the_image(client);
            client.until("the image is sent", |c| c.count(TRANSMIT) == 1);

            blind.shows("400×200");
            blind.until("the window says why", |c| !c.text().contains("reading"));
            assert_eq!((blind.count(KITTY), blind.count(ITERM)), (0, 0));
        });
    }
}

mod pointing {
    use crossterm::event::{KeyModifiers, MouseEvent};

    use super::a_bug::{Attached, COLS, ROWS};
    use super::*;
    use crate::graphics::Support;
    use crate::ui;

    const RESET: &[u8] = b"\x1b]22;\x1b\\";
    const SIDEWAYS: &[u8] = b"\x1b]22;ew-resize\x1b\\";

    fn hover(server: &mut Server, client: u64, column: u16, row: u16) {
        let ev = MouseEvent { kind: MouseEventKind::Moved, column, row, modifiers: KeyModifiers::NONE };
        let _ = server.handle(ServerEvent::Message(client, ClientMessage::Event(Event::Mouse(ev))));
        server.flush();
    }

    #[test]
    fn a_window_is_told_the_pointer_shape_only_when_it_changes() {
        let mut attached = Attached::new();
        let mut window = attached.join_with(Support::default(), Some(pointer::Terminal::Kitty));
        let area = Rect::new(0, 0, COLS, ROWS);
        let layout = ui::full_layout(area, state::Widths::default(), false, config::Sidebar::default(), false, false);
        let border = layout.border(ui::Border::Projects);
        let server = &mut attached.server;
        server.draw();
        server.flush();

        for (column, row) in [(border.x, 1), (border.x, 2), (border.x, 3), (border.x + 3, 3), (border.x + 4, 3)] {
            hover(server, 2, column, row);
        }

        window.until("the pointer is back to the default", |c| c.count(RESET) == 2);
        assert_eq!(window.count(SIDEWAYS), 1);
    }
}

mod drawing {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::a_bug::Attached;
    use super::*;
    use crate::control::{Report, Response};
    use crate::host::clipboard;

    fn drawn_just_now(attached: &mut Attached) -> Instant {
        attached.server.draw();
        attached.server.drawn.expect("a frame was drawn")
    }

    fn shown_pane(attached: &mut Attached) -> u64 {
        let server = &mut attached.server;
        server.app.request(1, r#"{"command":"status","args":{}}"#, server.area, Instant::now());
        let (_, text) = server.app.take_answers().pop().expect("an answer");
        let Ok(Response::Ok(value)) = serde_json::from_str(&text) else { panic!("status failed: {text}") };
        let report: Report = serde_json::from_value(value).expect("a report");
        report.projects[0].workspaces[0].tabs[0].panes[0].id
    }

    #[test]
    fn output_nobody_can_see_draws_nothing() {
        let mut attached = Attached::new();
        let now = drawn_just_now(&mut attached);

        let _ = attached.server.handle(ServerEvent::App(AppEvent::Output(u64::MAX, b"hidden".to_vec())));

        assert!(!attached.server.due(now));
    }

    #[test]
    fn output_on_screen_draws_at_most_once_a_frame() {
        let mut attached = Attached::new();
        let pane = shown_pane(&mut attached);
        let now = drawn_just_now(&mut attached);
        attached.server.printed_at = Some(now);

        let _ = attached.server.handle(ServerEvent::App(AppEvent::Output(pane, b"shown".to_vec())));

        let server = &attached.server;
        assert_eq!((server.due(now), server.due(now + FRAME), server.next_frame(now)), (false, true, Some(FRAME)));
    }

    #[test]
    fn input_draws_at_once() {
        let mut attached = Attached::new();
        let now = drawn_just_now(&mut attached);
        let key = Event::Key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));

        let _ = attached.server.handle(ServerEvent::Message(1, ClientMessage::Event(key)));

        assert!(attached.server.due(now));
    }

    #[test]
    fn a_key_does_not_hold_back_its_echo() {
        let mut attached = Attached::new();
        let pane = shown_pane(&mut attached);
        drawn_just_now(&mut attached);
        let key = Event::Key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
        let _ = attached.server.handle(ServerEvent::Message(1, ClientMessage::Event(key)));
        attached.server.draw();

        let _ = attached.server.handle(ServerEvent::App(AppEvent::Output(pane, b"a".to_vec())));

        assert!(attached.server.due(Instant::now()));
    }

    #[test]
    fn a_quiet_server_still_draws_every_tick() {
        let mut attached = Attached::new();
        let now = drawn_just_now(&mut attached);

        assert_eq!((attached.server.due(now), attached.server.due(now + TICK)), (false, true));
    }

    #[test]
    fn output_nobody_can_see_still_sends_what_it_copied() {
        let mut attached = Attached::new();
        let pane = shown_pane(&mut attached);
        let server = &mut attached.server;
        server.app.request(1, r#"{"command":"new-tab","args":{"focus":true}}"#, server.area, Instant::now());
        drawn_just_now(&mut attached);
        let copy = b"\x1b]52;c;aGVsbG8=\x07".to_vec();

        let _ = attached.server.handle(ServerEvent::App(AppEvent::Output(pane, copy)));
        attached.server.flush();

        assert!(!attached.server.app.shows(pane));
        attached.shows_frame(&clipboard::osc52("hello"));
    }

    #[test]
    fn a_settled_session_is_still_saved() {
        let mut attached = Attached::new();
        let session = attached.server.session.clone();
        std::fs::remove_file(&session).expect("forget the saved session");
        attached.server.saver = Saver::new(session.clone(), None);
        let t0 = Instant::now();

        attached.server.save(t0);
        attached.server.save(t0 + SAVE_EVERY / 2);
        attached.server.save(t0 + persist::SETTLE + SAVE_EVERY);

        assert!(session.exists(), "the session settles and is written");
    }
}

mod queue_persistence {
    use super::*;

    fn restored(dir: &TempDir) -> Server {
        let session = dir.path().join("session.json");
        let (tx, _) = mpsc::channel();
        let (app_tx, _) = mpsc::channel();
        let mut app = App::new("/bin/sh".into(), HostTheme::default(), dir.path().join("config.json"), app_tx);
        app.set_queue(queue::load(&paths::queue(&session), crate::time::now()));
        Server::new(app, session, Saver::new(dir.path().join("todos.json"), None), tx)
    }

    #[rstest::rstest]
    #[case::corrupt("invalid JSON")]
    #[case::unsupported("{\"version\":2,\"next_id\":5,\"entries\":[]}")]
    fn an_unreadable_queue_is_preserved_until_the_queue_changes(#[case] contents: &str) {
        let dir = TempDir::new();
        let path = paths::queue(&dir.path().join("session.json"));
        std::fs::write(&path, contents).expect("write an unreadable queue");
        let mut server = restored(&dir);
        let now = Instant::now();
        assert!(!server.queue_saver.observe(server.app.queue_saved(), now).expect("observe"));
        assert!(!server.queue_saver.observe(server.app.queue_saved(), now + persist::SETTLE).expect("settle"));
        server.queue_saver.flush().expect("flush");
        assert_eq!(std::fs::read_to_string(&path).expect("original queue"), contents);
        let mut queued = queue::Queue::default();
        queued.add(queue::Task::default()).expect("a new entry");
        server.app.set_queue(queued);
        assert!(!server.queue_saver.observe(server.app.queue_saved(), now).expect("changed"));
        assert!(server.queue_saver.observe(server.app.queue_saved(), now + persist::SETTLE).expect("save changes"));
        assert_eq!(queue::read_saved(&path).expect("new queue").entries.len(), 1);
    }

    #[test]
    fn restored_interrupted_starts_are_saved_as_failed() {
        let dir = TempDir::new();
        let path = paths::queue(&dir.path().join("session.json"));
        let mut queued = queue::Queue::default();
        let id = queued.add(queue::Task::default()).expect("entry");
        queued.entry_mut(id).expect("entry").state = queue::State::Starting;
        persist::save(&path, &queued.saved()).expect("interrupted start");
        let mut server = restored(&dir);
        let now = Instant::now();
        assert!(!server.queue_saver.observe(server.app.queue_saved(), now).expect("observe"));
        assert!(server.queue_saver.observe(server.app.queue_saved(), now + persist::SETTLE).expect("save failure"));
        assert!(matches!(
            queue::read_saved(&path).expect("saved failure").entries[0].state,
            queue::State::Failed { .. }
        ));
    }
}
