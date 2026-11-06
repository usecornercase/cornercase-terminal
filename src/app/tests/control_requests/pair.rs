use std::net::TcpListener;

use rstest::rstest;

use super::*;
use crate::app::events::Streamed;
use crate::app::remote_access::Phase;
use crate::test_util::{FakeHttp, fake_program, free_port, http_get, wait_until_within};
use crate::web::auth::PAIRING_FOR;

const FIRST: u64 = 7;
const SECOND: u64 = 8;
const STATUS: &str = r#"{"BackendState":"Running","CertDomains":["mac.tail1234.ts.net"],"Self":{"DNSName":"mac.tail1234.ts.net.","UserID":7},"User":{"7":{"LoginName":"ada@example.com"}}}"#;
const NOT_CONNECTED: &str = "tailscale is not connected (Stopped): open it and log in";
const TIMED_OUT: &str = "no phone paired within ten minutes: run this again";
const NO_BOT: &str = "paste the bot's token first";
const A_PROGRAM_RUNS: Duration = Duration::from_secs(30);

fn pair_at(app: &mut App, client: u64, key: &str, now: Instant) -> std::result::Result<Value, Refused> {
    let text = json!({"contract": 1, "command": "config-pair", "args": {"key": key}}).to_string();
    app.request(client, &text, Some(AREA), now);
    let mut answers = app.take_answers();
    assert_eq!(answers.len(), 1, "an answer at once");
    reply_of(&answers.remove(0).1)
}

fn pair(app: &mut App, client: u64, key: &str) -> std::result::Result<Value, Refused> {
    pair_at(app, client, key, Instant::now())
}

fn shown(app: &mut App, client: u64, key: &str) -> Value {
    pair(app, client, key).unwrap_or_else(|e| panic!("refused: {} {}", e.code, e.message))
}

fn refusal(app: &mut App, key: &str) -> (String, String) {
    let refused = pair(app, FIRST, key).expect_err("refused");
    (refused.code, refused.message)
}

fn lines(app: &mut App) -> Vec<(u64, Option<Value>)> {
    let read = |text: &str| serde_json::from_str::<Value>(text).expect("a line")["ok"].clone();
    app.take_events()
        .into_iter()
        .map(|(client, streamed)| match streamed {
            Streamed::Event(text) | Streamed::Snapshot(text) => (client, Some(read(&text))),
            Streamed::End => (client, None),
        })
        .collect()
}

fn ended(client: u64, line: &Value) -> Vec<(u64, Option<Value>)> {
    vec![(client, Some(line.clone())), (client, None)]
}

fn failed(client: u64, why: &str) -> Vec<(u64, Option<Value>)> {
    ended(client, &json!({"paired": false, "error": why}))
}

fn assert_says_it_could_not_save(got: &[(u64, Option<Value>)]) {
    let [(FIRST, Some(line))] = got else { panic!("one new line: {got:?}") };
    let last = line["lines"].as_array().and_then(|lines| lines.last()).and_then(Value::as_str);
    assert!(last.is_some_and(|last| last.contains("could not save the paired chat")), "{line}");
}

fn next_lines(app: &mut App, rx: &Receiver<AppEvent>) -> Vec<(u64, Option<Value>)> {
    let mut got = Vec::new();
    wait_until_within("a new line", A_PROGRAM_RUNS, || {
        drain(app, rx);
        app.refresh(Instant::now());
        got.extend(lines(app));
        !got.is_empty()
    });
    got
}

mod browsers {
    use super::*;

    pub(super) struct Remote {
        pub(super) app: App,
        pub(super) rx: Receiver<AppEvent>,
        pub(super) dir: TempDir,
        pub(super) port: u16,
    }

    impl Remote {
        pub(super) fn takes(&self, path: &str) -> bool {
            http_get(self.port, path).starts_with("HTTP/1.1 303")
        }

        pub(super) fn refuses(&self, path: &str) -> bool {
            http_get(self.port, path).starts_with("HTTP/1.1 403")
        }

        pub(super) fn told(&mut self) {
            pump_until(&mut self.app, &self.rx, "the pairing is told", |app| toast(app).is_some());
        }

        pub(super) fn a_phone_takes(&mut self, path: &str, client: u64) {
            assert!(self.takes(path));
            self.told();
            assert_eq!(lines(&mut self.app), ended(client, &json!({"paired": true})));
            assert_eq!(self.app.remote.paired(), 1);
        }
    }

    pub(super) fn with_tailscale(status: &str) -> Remote {
        let dir = TempDir::new();
        let (mut app, rx) = new_app(dir.path().join("config.json"));
        let program = fake_program(dir.path(), "tailscale");
        std::fs::write(program.with_extension("out"), status).expect("what tailscale says");
        app.remote.set_tailscale(program);
        let port = free_port();
        app.config.remote_port = port;
        Remote { app, rx, dir, port }
    }

    pub(super) fn ready() -> Remote {
        let mut remote = with_tailscale(STATUS);
        remote.app.config.remote = true;
        remote.app.refresh(Instant::now());
        wait_until_within("tailscale answers", A_PROGRAM_RUNS, || {
            drain(&mut remote.app, &remote.rx);
            matches!(remote.app.remote.phase(), Phase::Ready(_))
        });
        remote
    }

    pub(super) fn path_of(line: &Value) -> String {
        let url = line["url"].as_str().expect("an address");
        url.split_once(":8443").map(|(_, path)| path.to_string()).expect("a path")
    }

    #[test]
    fn with_remote_access_off_it_is_refused_in_words() {
        let mut remote = with_tailscale(STATUS);

        let (code, message) = refusal(&mut remote.app, "browsers");

        assert_eq!(code, "conflict");
        assert!(message.contains("remote access is off") && message.contains("config set remote true"), "{message}");
        assert_eq!(remote.app.remote.listening(), None);
    }

    #[test]
    fn a_port_in_use_is_refused_in_words() {
        let taken = TcpListener::bind(("127.0.0.1", 0)).expect("a port");
        let mut remote = with_tailscale(STATUS);
        remote.app.config.remote_port = taken.local_addr().expect("its address").port();
        remote.app.config.remote = true;

        let (code, message) = refusal(&mut remote.app, "browsers");

        assert_eq!(code, "unavailable");
        assert!(message.contains("is taken"), "{message}");
    }

    #[test]
    fn tailscale_missing_ends_it_in_words() {
        let mut remote = with_tailscale(STATUS);
        remote.app.remote.set_tailscale(remote.dir.path().join("missing").join("tailscale"));
        remote.app.config.remote = true;

        assert_eq!(shown(&mut remote.app, FIRST, "browsers"), json!({"url": null, "lines": ["starting Tailscale…"]}));
        let got = next_lines(&mut remote.app, &remote.rx);

        assert_eq!(got, failed(FIRST, "tailscale is not installed on this computer"));
    }

    #[test]
    fn a_tailscale_that_is_not_ready_ends_it_in_words() {
        let mut remote = with_tailscale(&STATUS.replace("Running", "Stopped"));
        remote.app.config.remote = true;

        shown(&mut remote.app, FIRST, "browsers");
        let got = next_lines(&mut remote.app, &remote.rx);

        assert_eq!(got, failed(FIRST, NOT_CONNECTED));
    }

    #[test]
    fn it_prints_what_the_dialog_draws() {
        let mut remote = ready();

        let line = shown(&mut remote.app, FIRST, "browsers");
        remote.app.open_remote();

        let dialog = remote.app.remote_view();
        assert_eq!(line, json!({"url": dialog.url, "lines": dialog.lines}));
        assert_eq!(
            line["lines"],
            json!([
                "scan the code with your phone's camera.",
                "the phone needs the Tailscale app, logged in as ada@example.com."
            ])
        );
        assert!(path_of(&line).starts_with("/pair?t="), "{line}");
    }

    #[test]
    fn while_tailscale_starts_it_says_so_then_gives_the_address() {
        let mut remote = with_tailscale(STATUS);
        let hold = remote.dir.path().join("tailscale.hold");
        std::fs::write(&hold, "").expect("hold tailscale");
        remote.app.config.remote = true;

        let first = shown(&mut remote.app, FIRST, "browsers");
        std::fs::remove_file(&hold).expect("let tailscale answer");
        let mut got = next_lines(&mut remote.app, &remote.rx);

        assert_eq!(first, json!({"url": null, "lines": ["starting Tailscale…"]}));
        let (client, line) = got.pop().expect("a line");
        let line = line.expect("not the end");
        assert_eq!(client, FIRST);
        assert!(remote.takes(&path_of(&line)), "{line}");
    }

    #[test]
    fn a_browser_taking_the_code_ends_it_and_the_code_works_once() {
        let mut remote = ready();
        let path = path_of(&shown(&mut remote.app, FIRST, "browsers"));

        remote.a_phone_takes(&path, FIRST);

        assert_eq!(toast(&remote.app), Some("a browser connected"));
        assert!(remote.refuses(&path));
    }

    #[test]
    fn a_browser_that_cannot_be_saved_leaves_the_code_for_another_try() {
        let mut remote = ready();
        let path = path_of(&shown(&mut remote.app, FIRST, "browsers"));
        let file = remote.dir.path().join("browsers.json");
        std::fs::create_dir(&file).expect("a folder where the file goes");

        assert!(http_get(remote.port, &path).starts_with("HTTP/1.1 500"));
        remote.app.refresh(Instant::now());
        assert_eq!(lines(&mut remote.app), []);
        assert_eq!(remote.app.remote.paired(), 0);
        std::fs::remove_dir(&file).expect("the way cleared");

        remote.a_phone_takes(&path, FIRST);
    }

    #[test]
    fn nothing_opens_in_the_window_and_a_quiet_wait_prints_nothing() {
        let mut remote = ready();

        shown(&mut remote.app, FIRST, "browsers");
        remote.app.refresh(Instant::now());

        assert!(remote.app.overlay.is_none());
        assert_eq!(toast(&remote.app), None);
        assert_eq!(lines(&mut remote.app), []);
    }

    #[test]
    fn the_code_goes_the_moment_its_reader_goes() {
        let mut remote = ready();
        let path = path_of(&shown(&mut remote.app, FIRST, "browsers"));

        remote.app.forget(FIRST);

        assert!(remote.refuses(&path));
        assert_eq!(lines(&mut remote.app), []);
    }

    #[test]
    fn turning_remote_access_off_ends_it_in_words() {
        let mut remote = ready();
        shown(&mut remote.app, FIRST, "browsers");

        remote.app.config.remote = false;
        remote.app.refresh(Instant::now());

        let got = lines(&mut remote.app);
        let why = got[0].1.as_ref().expect("a line")["error"].as_str().expect("words").to_string();
        assert!(why.contains("remote access is off"), "{why}");
        assert_eq!(got, failed(FIRST, &why));
    }

    #[test]
    fn ten_minutes_end_it_and_drop_the_code() {
        let mut remote = ready();
        let start = Instant::now();
        let line = pair_at(&mut remote.app, FIRST, "browsers", start).expect("a line");

        assert_eq!(remote.app.clients.pairings.wake(start), Some(PAIRING_FOR));
        assert_eq!(remote.app.tick(start + PAIRING_FOR), Some(Duration::ZERO));
        remote.app.refresh(start + PAIRING_FOR.saturating_sub(Duration::from_secs(1)));
        assert_eq!(lines(&mut remote.app), []);
        remote.app.refresh(start + PAIRING_FOR);

        assert_eq!(lines(&mut remote.app), failed(FIRST, TIMED_OUT));
        assert!(remote.refuses(&path_of(&line)));
        assert_eq!(remote.app.clients.pairings.wake(start), None);
    }

    #[test]
    fn a_connection_follows_one_pairing() {
        let mut remote = ready();
        shown(&mut remote.app, FIRST, "browsers");

        let (code, message) = refusal(&mut remote.app, "browsers");

        assert_eq!(code, "conflict");
        assert!(message.contains("already follows"), "{message}");
    }

    #[test]
    fn a_key_that_cannot_be_paired_says_which_can() {
        let mut remote = with_tailscale(STATUS);

        let (code, message) = refusal(&mut remote.app, "linear");

        assert_eq!(code, "malformed");
        assert!(message.contains("browsers, telegram.chat or discord.chat"), "{message}");
    }

    #[test]
    fn an_unknown_field_is_refused_by_name() {
        let mut remote = with_tailscale(STATUS);

        let refused = refused(&mut remote.app, "config-pair", &json!({"key": "browsers", "bogus": 1}));

        assert_eq!(refused.code, "unknown-field");
        assert!(refused.message.contains("args.bogus"), "{}", refused.message);
    }
}

mod a_shared_code {
    use super::browsers::{path_of, ready};
    use super::*;

    #[test]
    fn is_what_two_commands_show_and_it_goes_with_the_last() {
        let mut remote = ready();
        let first = shown(&mut remote.app, FIRST, "browsers");
        let second = shown(&mut remote.app, SECOND, "browsers");

        assert_eq!(first, second);
        remote.app.forget(FIRST);
        remote.app.forget(SECOND);

        assert!(remote.refuses(&path_of(&first)));
    }

    #[test]
    fn stays_for_the_other_command_when_one_goes() {
        let mut remote = ready();
        let path = path_of(&shown(&mut remote.app, FIRST, "browsers"));
        shown(&mut remote.app, SECOND, "browsers");

        remote.app.forget(FIRST);
        remote.app.refresh(Instant::now());

        remote.a_phone_takes(&path, SECOND);
    }

    #[test]
    fn opens_in_the_dialog_when_a_command_shows_it_and_closing_the_dialog_keeps_it() {
        let mut remote = ready();
        let line = shown(&mut remote.app, FIRST, "browsers");

        remote.app.open_remote();
        assert_eq!(remote.app.remote_view().url, line["url"].as_str().map(String::from));
        remote.app.overlay = None;
        remote.app.refresh(Instant::now());
        assert_eq!(lines(&mut remote.app), []);

        remote.a_phone_takes(&path_of(&line), FIRST);
    }

    #[test]
    fn stays_in_the_dialog_when_the_command_that_joined_it_goes() {
        let mut remote = ready();
        remote.app.open_remote();
        let url = remote.app.remote_view().url;

        let line = shown(&mut remote.app, FIRST, "browsers");
        assert_eq!(line["url"].as_str().map(String::from), url);
        remote.app.forget(FIRST);
        remote.app.refresh(Instant::now());

        assert_eq!(remote.app.remote_view().url, url);
        assert!(matches!(remote.app.overlay, Some(Overlay::Remote)));
        assert!(remote.takes(&path_of(&line)));
        remote.told();
        assert!(remote.app.overlay.is_none());
    }

    #[test]
    fn goes_once_the_dialog_closes_after_the_command_went() {
        let mut remote = ready();
        let line = shown(&mut remote.app, FIRST, "browsers");
        remote.app.open_remote();

        remote.app.forget(FIRST);
        remote.app.overlay = None;
        remote.app.refresh(Instant::now());

        assert!(remote.refuses(&path_of(&line)));
    }

    #[test]
    fn taken_by_a_phone_ends_every_holder() {
        let mut remote = ready();
        remote.app.open_remote();
        let path = path_of(&shown(&mut remote.app, FIRST, "browsers"));
        shown(&mut remote.app, SECOND, "browsers");

        assert!(remote.takes(&path));
        remote.told();

        let done = json!({"paired": true});
        assert_eq!(lines(&mut remote.app), [ended(FIRST, &done), ended(SECOND, &done)].concat());
        assert!(remote.app.overlay.is_none());
        assert!(remote.refuses(&path));
    }

    #[test]
    fn a_new_dialog_without_a_command_makes_a_new_code_as_it_always_did() {
        let mut remote = ready();
        remote.app.open_remote();
        let before = remote.app.remote_view().url;

        remote.app.overlay = None;
        remote.app.open_remote();

        assert_ne!(remote.app.remote_view().url, before);
    }

    #[test]
    fn that_died_gives_whoever_comes_a_new_one_which_the_dialog_then_shows() {
        let mut remote = ready();
        remote.app.open_remote();
        let dead = remote.app.remote_view().url;

        let line = pair_at(&mut remote.app, FIRST, "browsers", Instant::now() + PAIRING_FOR).expect("a line");
        let url = line["url"].as_str().map(String::from);

        assert_ne!(url, dead);
        assert_eq!(remote.app.remote_view().url, url);
    }

    #[test]
    fn a_phone_that_took_it_ends_only_those_who_came_before_it() {
        let mut remote = ready();
        let taken = path_of(&shown(&mut remote.app, FIRST, "browsers"));
        assert!(remote.takes(&taken));

        let newer = path_of(&shown(&mut remote.app, SECOND, "browsers"));
        assert_ne!(newer, taken);
        remote.told();

        assert_eq!(lines(&mut remote.app), ended(FIRST, &json!({"paired": true})));
        assert!(remote.takes(&newer));
        assert_eq!(next_lines(&mut remote.app, &remote.rx), ended(SECOND, &json!({"paired": true})));
    }

    #[test]
    fn a_dialog_opened_after_a_phone_took_it_stays_open_on_a_new_one() {
        let mut remote = ready();
        let taken = path_of(&shown(&mut remote.app, FIRST, "browsers"));
        assert!(remote.takes(&taken));

        remote.app.open_remote();
        let url = remote.app.remote_view().url.expect("a new address");
        remote.told();

        assert_eq!(lines(&mut remote.app), ended(FIRST, &json!({"paired": true})));
        assert!(matches!(remote.app.overlay, Some(Overlay::Remote)));
        assert_eq!(remote.app.remote_view().url.as_deref(), Some(url.as_str()));
        assert!(remote.takes(&path_of(&json!({"url": url}))));
    }

    #[test]
    fn a_dialog_that_showed_it_closes_though_a_command_came_after_the_phone() {
        let mut remote = ready();
        remote.app.open_remote();
        let taken = path_of(&json!({"url": remote.app.remote_view().url}));
        assert!(remote.takes(&taken));

        let newer = path_of(&shown(&mut remote.app, FIRST, "browsers"));
        remote.told();

        assert!(remote.app.overlay.is_none());
        assert_eq!(lines(&mut remote.app), []);
        assert!(remote.takes(&newer));
    }

    #[test]
    fn ends_each_command_after_its_own_ten_minutes_and_lives_for_the_last() {
        let mut remote = ready();
        let start = Instant::now();
        pair_at(&mut remote.app, FIRST, "browsers", start).expect("a line");
        let later = start + PAIRING_FOR / 2;
        let line = pair_at(&mut remote.app, SECOND, "browsers", later).expect("a line");

        remote.app.refresh(start + PAIRING_FOR);

        assert_eq!(lines(&mut remote.app), failed(FIRST, TIMED_OUT));
        remote.a_phone_takes(&path_of(&line), SECOND);
    }
}

mod chats {
    use super::*;

    #[rstest]
    #[case::telegram("telegram.chat")]
    #[case::discord("discord.chat")]
    fn without_a_bot_it_is_refused_in_words(#[case] key: &str) {
        let dir = TempDir::new();
        let (mut app, _rx) = new_app(dir.path().join("config.json"));

        assert_eq!(refusal(&mut app, key), ("conflict".to_string(), NO_BOT.to_string()));
    }

    #[test]
    fn the_keys_are_the_ones_config_lists_and_remove_takes() {
        let dir = TempDir::new();
        let (mut app, _rx) = new_app(dir.path().join("config.json"));

        let described = ok(&mut app, "config", &json!({"action": "get"}));

        let listed: Vec<&str> = described["connections"]
            .as_array()
            .expect("connections")
            .iter()
            .filter_map(|c| c["key"].as_str())
            .collect();
        for key in ["browsers", "telegram.chat", "discord.chat"] {
            assert!(listed.contains(&key), "{key} in {listed:?}");
            ok(&mut app, "config", &json!({"action": "remove", "entry": key}));
        }
    }
}

mod telegram {
    use super::*;

    fn set_up() -> (App, Phone, TempDir) {
        let dir = TempDir::new();
        let (mut app, _rx) = new_app(dir.path().join("config.json"));
        let phone = Phone::set_up(&mut app, None);
        (app, phone, dir)
    }

    fn code_of(line: &Value) -> String {
        let url = line["url"].as_str().expect("a link");
        url.split_once("?start=").map(|(_, code)| code.to_string()).expect("a code")
    }

    fn start(app: &mut App, phone: &mut Phone, line: &Value) {
        phone.says(app, &format!("/start {}", code_of(line)));
    }

    fn the_code_ends_it(app: &mut App, phone: &mut Phone, line: &Value) {
        start(app, phone, line);
        assert_eq!(lines(app), ended(FIRST, &json!({"paired": true, "chat": "@ada"})));
        assert_eq!(toast(app), Some("Telegram connected with @ada"));
        assert_eq!(phone.read(app).len(), 1);
    }

    #[test]
    fn it_prints_what_the_dialog_draws() {
        let (mut app, _phone, _dir) = set_up();

        let line = shown(&mut app, FIRST, "telegram.chat");
        assert_eq!(app.pair(Via::Telegram), None);

        let dialog = app.pairing_view(Via::Telegram);
        assert_eq!(line, json!({"url": dialog.url, "lines": dialog.lines}));
        assert!(
            line["url"].as_str().is_some_and(|url| url.starts_with("https://t.me/cornercase_bot?start=")),
            "{line}"
        );
    }

    #[test]
    fn the_phone_that_sends_the_code_ends_it_with_the_chats_name() {
        let (mut app, mut phone, dir) = set_up();
        let line = shown(&mut app, FIRST, "telegram.chat");

        the_code_ends_it(&mut app, &mut phone, &line);

        let saved = std::fs::read_to_string(dir.path().join("telegram.json")).expect("the paired chat is saved");
        assert!(app.messenger(Via::Telegram).paired() && app.overlay.is_none(), "{saved}");
    }

    fn already_paired() -> (App, Phone, TempDir) {
        let dir = TempDir::new();
        let (mut app, _rx) = new_app(dir.path().join("config.json"));
        let phone = Phone::paired(&mut app);
        (app, phone, dir)
    }

    #[test]
    fn the_phone_already_paired_takes_the_code_too_and_ends_it() {
        let (mut app, mut phone, _dir) = already_paired();
        let line = shown(&mut app, FIRST, "telegram.chat");

        the_code_ends_it(&mut app, &mut phone, &line);
    }

    #[test]
    fn the_phone_already_paired_closes_the_dialog_with_the_code() {
        let (mut app, mut phone, _dir) = already_paired();
        assert_eq!(app.pair(Via::Telegram), None);
        let url = app.pairing_view(Via::Telegram).url;

        start(&mut app, &mut phone, &json!({"url": url}));

        assert!(app.overlay.is_none());
        assert_eq!(toast(&app), Some("Telegram connected with @ada"));
        assert_eq!(phone.read(&mut app).len(), 1);
    }

    #[test]
    fn a_start_without_the_code_from_the_phone_already_paired_is_only_the_command() {
        let (mut app, mut phone, _dir) = already_paired();
        shown(&mut app, FIRST, "telegram.chat");

        phone.says(&mut app, "/start");
        phone.says(&mut app, "/start 00000000000000000000000000000000");

        assert_eq!(lines(&mut app), []);
        assert_eq!(toast(&app), None);
        assert_eq!(phone.read(&mut app).len(), 2);
    }

    #[test]
    fn a_chat_that_cannot_be_saved_is_not_paired_and_says_so_and_the_code_stays() {
        let (mut app, mut phone, dir) = set_up();
        let line = shown(&mut app, FIRST, "telegram.chat");
        let file = dir.path().join("telegram.json");
        std::fs::create_dir(&file).expect("a folder where the file goes");

        start(&mut app, &mut phone, &line);
        app.refresh(Instant::now());

        assert_says_it_could_not_save(&lines(&mut app));
        assert!(!app.messenger(Via::Telegram).paired());
        assert_eq!(toast(&app), None);
        assert_eq!(phone.read(&mut app).len(), 0);
        std::fs::remove_dir(&file).expect("the way cleared");
        the_code_ends_it(&mut app, &mut phone, &line);
    }

    #[test]
    fn a_chat_that_cannot_be_saved_keeps_the_dialog_open_and_the_chat_paired_before() {
        let (mut app, mut phone, dir) = already_paired();
        assert_eq!(app.pair(Via::Telegram), None);
        let url = app.pairing_view(Via::Telegram).url;
        std::fs::create_dir(dir.path().join("telegram.json")).expect("a folder where the file goes");

        phone.message(&mut app, &format!("/start {}", code_of(&json!({"url": url}))), None, (77, 78));

        assert!(matches!(app.overlay, Some(Overlay::Pairing(Via::Telegram))));
        assert_eq!(app.messenger(Via::Telegram).chat_name(), Some("@ada"));
        let lines = app.pairing_view(Via::Telegram).lines;
        assert!(lines.last().is_some_and(|line| line.contains("could not save the paired chat")), "{lines:?}");
    }

    #[test]
    fn nothing_opens_in_the_window() {
        let (mut app, _phone, _dir) = set_up();

        shown(&mut app, FIRST, "telegram.chat");
        app.refresh(Instant::now());

        assert!(app.overlay.is_none());
        assert_eq!(lines(&mut app), []);
    }

    #[test]
    fn the_code_goes_when_its_reader_goes() {
        let (mut app, mut phone, _dir) = set_up();
        let line = shown(&mut app, FIRST, "telegram.chat");

        app.forget(FIRST);
        start(&mut app, &mut phone, &line);

        assert!(!app.messenger(Via::Telegram).paired());
        assert_eq!(lines(&mut app), []);
    }

    #[test]
    fn ten_minutes_end_it_and_drop_the_code() {
        let (mut app, mut phone, _dir) = set_up();
        let start_at = Instant::now();
        let line = pair_at(&mut app, FIRST, "telegram.chat", start_at).expect("a line");

        app.refresh(start_at + PAIRING_FOR);
        assert_eq!(lines(&mut app), failed(FIRST, TIMED_OUT));
        start(&mut app, &mut phone, &line);

        assert!(!app.messenger(Via::Telegram).paired());
    }

    #[test]
    fn two_commands_and_the_dialog_share_one_code_until_the_last_goes() {
        let (mut app, mut phone, _dir) = set_up();
        let line = shown(&mut app, FIRST, "telegram.chat");
        assert_eq!(shown(&mut app, SECOND, "telegram.chat"), line);
        assert_eq!(app.pair(Via::Telegram), None);
        assert_eq!(app.pairing_view(Via::Telegram).url, line["url"].as_str().map(String::from));

        app.forget(FIRST);
        app.overlay = None;
        app.refresh(Instant::now());
        assert_eq!(lines(&mut app), []);
        app.forget(SECOND);
        start(&mut app, &mut phone, &line);

        assert!(!app.messenger(Via::Telegram).paired());
    }

    #[test]
    fn closing_the_dialog_keeps_the_code_a_command_shows() {
        let (mut app, mut phone, _dir) = set_up();
        let line = shown(&mut app, FIRST, "telegram.chat");
        assert_eq!(app.pair(Via::Telegram), None);

        app.overlay = None;
        app.refresh(Instant::now());
        start(&mut app, &mut phone, &line);

        assert_eq!(lines(&mut app), ended(FIRST, &json!({"paired": true, "chat": "@ada"})));
    }

    #[test]
    fn a_command_going_keeps_the_code_the_dialog_shows() {
        let (mut app, mut phone, _dir) = set_up();
        assert_eq!(app.pair(Via::Telegram), None);
        let line = shown(&mut app, FIRST, "telegram.chat");

        app.forget(FIRST);
        app.refresh(Instant::now());
        start(&mut app, &mut phone, &line);

        assert!(app.messenger(Via::Telegram).paired() && app.overlay.is_none());
    }

    #[test]
    fn removing_the_bot_ends_it_in_words() {
        let (mut app, _phone, _dir) = set_up();
        shown(&mut app, FIRST, "telegram.chat");

        app.remove_messenger(Via::Telegram).expect("removed");
        app.refresh(Instant::now());

        assert_eq!(lines(&mut app), failed(FIRST, NO_BOT));
    }

    #[test]
    fn a_refused_token_ends_it_in_words_and_refuses_the_next() {
        let (mut app, _phone, _dir) = set_up();
        shown(&mut app, FIRST, "telegram.chat");

        let event = MessengerEvent::Link(Via::Telegram, LinkEvent::Rejected(1));
        app.handle_event(AppEvent::Messenger(event), AREA).expect("handled");
        app.refresh(Instant::now());

        let why = Via::Telegram.rejected();
        assert_eq!(lines(&mut app), failed(FIRST, &why));
        assert_eq!(refusal(&mut app, "telegram.chat"), ("conflict".to_string(), why));
    }

    #[test]
    fn another_bot_gives_a_new_line() {
        let (mut app, _phone, _dir) = set_up();
        let line = shown(&mut app, FIRST, "telegram.chat");

        wiring::set_up(&mut app, Via::Telegram, "2:t", "other_bot", None);
        app.refresh(Instant::now());

        let url = format!("https://t.me/other_bot?start={}", code_of(&line));
        assert_eq!(lines(&mut app), [(FIRST, Some(json!({"url": url, "lines": line["lines"]})))]);
    }
}

mod discord {
    use super::*;

    const TOKEN: &str = "MTIzNDU2Nzg5.Gx.SECRET-token";
    const OWNER: &str = r#"{"id":"123456789","owner":{"id":"9","username":"ada"}}"#;
    const PUBLIC: &str = r#"{"id":"123456789","owner":{"id":"9","username":"ada"},"bot_public":true,
        "integration_types_config":{"0":{},"1":{}}}"#;
    const CLOSED: &str = r#"{"message":"Cannot send messages to this user","code":50007}"#;
    const CHANNEL: &str = r#"{"id":"1200"}"#;

    struct Discord {
        app: App,
        rx: Receiver<AppEvent>,
        server: FakeHttp,
        _phone: Phone,
        dir: TempDir,
    }

    fn set_up(routes: Vec<(&'static str, u16, &'static str)>) -> Discord {
        let dir = TempDir::new();
        let (mut app, rx) = new_app(dir.path().join("config.json"));
        let server = FakeHttp::start(routes);
        let phone = Phone::on(&mut app, Via::Discord, None);
        wiring::set_up(&mut app, Via::Discord, TOKEN, "cornercase", None);
        wiring::point(&mut app, Via::Discord, &server.url());
        Discord { app, rx, server, _phone: phone, dir }
    }

    #[test]
    fn it_prints_what_the_dialog_draws() {
        let mut discord = set_up(Vec::new());

        let line = shown(&mut discord.app, FIRST, "discord.chat");
        assert_eq!(discord.app.pair(Via::Discord), None);

        let dialog = discord.app.pairing_view(Via::Discord);
        assert_eq!(line, json!({"url": dialog.url, "lines": dialog.lines}));
        assert!(line["url"].as_str().is_some_and(|url| url.contains("client_id=123456789")), "{line}");
    }

    #[test]
    fn its_owner_is_asked_for_only_while_it_runs() {
        let mut discord = set_up(Vec::new());
        let now = Instant::now();

        assert_eq!(discord.app.next_pairing(now), None);
        shown(&mut discord.app, FIRST, "discord.chat");
        assert_eq!(discord.app.next_pairing(now), Some(Duration::ZERO));
        discord.app.forget(FIRST);
        assert_eq!(discord.app.next_pairing(now), None);
        discord.app.refresh(now);

        assert_eq!(discord.server.requests().len(), 0);
    }

    #[test]
    fn it_ends_with_the_owners_name_once_the_app_is_installed() {
        let mut discord = set_up(vec![
            ("GET /oauth2/applications/@me", 200, OWNER),
            ("POST /users/@me/channels", 200, CHANNEL),
            ("POST /channels/1200/messages", 200, r#"{"id":"55"}"#),
        ]);
        shown(&mut discord.app, FIRST, "discord.chat");

        let got = next_lines(&mut discord.app, &discord.rx);

        assert_eq!(got, ended(FIRST, &json!({"paired": true, "chat": "ada"})));
        assert!(discord.app.messenger(Via::Discord).paired() && discord.app.overlay.is_none());
        assert_eq!(toast(&discord.app), Some("Discord connected with ada"));
    }

    #[test]
    fn a_chat_that_cannot_be_saved_says_so_and_asks_again_a_minute_later() {
        let installed = [
            ("GET /oauth2/applications/@me", 200, OWNER),
            ("POST /users/@me/channels", 200, CHANNEL),
            ("POST /channels/1200/messages", 200, r#"{"id":"55"}"#),
        ];
        let mut discord = set_up([installed, installed].concat());
        let file = discord.dir.path().join("discord.json");
        std::fs::create_dir(&file).expect("a folder where the file goes");
        shown(&mut discord.app, FIRST, "discord.chat");

        assert_says_it_could_not_save(&next_lines(&mut discord.app, &discord.rx));
        assert!(!discord.app.messenger(Via::Discord).paired());
        let wait = discord.app.next_pairing(Instant::now()).expect("it asks again");
        assert!(wait > Duration::from_secs(30), "{wait:?}");
        std::fs::remove_dir(&file).expect("the way cleared");
        discord.app.refresh(Instant::now() + wait);

        let got = next_lines(&mut discord.app, &discord.rx);
        assert_eq!(got.last(), Some(&(FIRST, None)), "{got:?}");
        assert!(got.contains(&(FIRST, Some(json!({"paired": true, "chat": "ada"})))), "{got:?}");
        assert!(discord.app.messenger(Via::Discord).paired());
    }

    #[test]
    fn its_advice_gives_a_new_line() {
        let mut discord = set_up(vec![
            ("GET /oauth2/applications/@me", 200, PUBLIC),
            ("POST /users/@me/channels", 200, CHANNEL),
            ("POST /channels/1200/messages", 403, CLOSED),
        ]);
        let first = shown(&mut discord.app, FIRST, "discord.chat");

        let got = next_lines(&mut discord.app, &discord.rx);

        let mut lines_now = first["lines"].as_array().expect("lines").clone();
        lines_now.push(json!(crate::discord::PUBLIC));
        assert_eq!(got, [(FIRST, Some(json!({"url": first["url"], "lines": lines_now})))]);
    }

    #[test]
    fn a_failure_gives_a_new_line_which_whoever_joins_keeps() {
        let mut discord =
            set_up(vec![("GET /oauth2/applications/@me", 500, r#"{"message":"500: Internal Server Error"}"#)]);
        let first = shown(&mut discord.app, FIRST, "discord.chat");

        let got = next_lines(&mut discord.app, &discord.rx);
        let (_, line) = got.last().expect("a line");
        let line = line.as_ref().expect("not the end");
        assert_eq!(line["lines"].as_array().map(Vec::len), Some(first["lines"].as_array().map_or(0, Vec::len) + 1));

        assert_eq!(&shown(&mut discord.app, SECOND, "discord.chat"), line);
        assert_eq!(discord.app.pair(Via::Discord), None);
        discord.app.refresh(Instant::now());
        assert_eq!(lines(&mut discord.app), []);
        assert_eq!(json!(discord.app.pairing_view(Via::Discord).lines), line["lines"]);
    }

    #[test]
    fn closing_the_dialog_keeps_asking_while_a_command_runs_and_the_last_going_stops_it() {
        let mut discord = set_up(Vec::new());
        let now = Instant::now();
        shown(&mut discord.app, FIRST, "discord.chat");
        assert_eq!(discord.app.pair(Via::Discord), None);

        discord.app.overlay = None;
        assert_eq!(discord.app.next_pairing(now), Some(Duration::ZERO));
        assert_eq!(discord.app.pair(Via::Discord), None);
        discord.app.forget(FIRST);
        assert_eq!(discord.app.next_pairing(now), Some(Duration::ZERO));

        discord.app.overlay = None;
        assert_eq!(discord.app.next_pairing(now), None);
    }

    #[test]
    fn a_token_that_names_no_app_is_refused_in_words() {
        let mut discord = set_up(Vec::new());
        wiring::set_up(&mut discord.app, Via::Discord, "no dots here", "cornercase", None);

        let (code, message) = refusal(&mut discord.app, "discord.chat");

        assert_eq!(code, "failed");
        assert_eq!(message, "the token does not name its app: paste it again");
    }
}
