use std::path::PathBuf;

use rstest::rstest;
use serde_json::{Value, json};

use super::*;
use crate::test_util::{FakeHttp, TempDir};
use crate::update::tests::replace::{NEW, installed, publish_for, sums_for};
use crate::update::{self, Install};

const LATEST: &str = r#"{"tag_name": "v9.0.0", "assets": []}"#;

fn target() -> &'static str {
    update::target().expect("a known target")
}

fn looking(server: &FakeHttp) -> (App, Receiver<AppEvent>) {
    let (mut app, rx) = empty_app();
    app.update.updates.enabled = true;
    app.update.updates.url = format!("{}/releases/latest", server.url());
    (app, rx)
}

fn send_as(app: &mut App, client: u64, command: &str) {
    let text = json!({"contract": 1, "command": command, "args": {}}).to_string();
    app.request(client, &text, Some(AREA), Instant::now());
}

fn replies(app: &mut App, rx: &Receiver<AppEvent>, clients: &[u64]) -> Vec<(u64, Value)> {
    let mut got = Vec::new();
    wait_until("every client is answered", || {
        drain(app, rx);
        app.refresh(Instant::now());
        got.extend(app.take_answers());
        clients.iter().all(|client| got.iter().any(|(c, _)| c == client))
    });
    got.into_iter().map(|(client, text)| (client, serde_json::from_str(&text).expect("a reply"))).collect()
}

fn reply(app: &mut App, rx: &Receiver<AppEvent>, command: &str) -> Value {
    send_as(app, CLIENT, command);
    replies(app, rx, &[CLIENT]).remove(0).1
}

fn info(reply: &Value) -> &Value {
    assert!(reply.get("error").is_none(), "{reply}");
    &reply["ok"]
}

fn code(reply: &Value) -> &str {
    reply["error"]["code"].as_str().unwrap_or_else(|| panic!("no error in {reply}"))
}

fn found(app: &mut App) {
    let release = update::release(&json!({"tag_name": "v9.0.0", "assets": []})).expect("a release");
    app.update.updates.available = Some(release);
}

mod unavailable {
    use super::*;

    #[test]
    fn a_development_build_has_no_update_requests() {
        let (mut app, _rx) = empty_app();
        for command in ["update-check", "update-install"] {
            let refusal = refused(&mut app, command, &json!({}));
            assert_eq!(refusal.code, "unavailable", "{command}");
            assert!(refusal.message.contains("development build"), "{}", refusal.message);
        }
        let descriptor = ok(&mut app, "contract", &json!({}));
        let off: Vec<&str> = descriptor["operations"]
            .as_array()
            .expect("operations")
            .iter()
            .filter(|op| op["available"] == false)
            .filter_map(|op| op["name"].as_str())
            .collect();
        assert_eq!(off, ["update-check", "update-install"]);
    }
}

mod checking {
    use super::*;

    #[test]
    fn a_check_answers_the_release_it_found() {
        let server = FakeHttp::start(vec![("GET /releases/latest", 200, LATEST)]);
        let (mut app, rx) = looking(&server);

        let answer = reply(&mut app, &rx, "update-check");

        assert_eq!(info(&answer)["release"]["version"], "9.0.0");
        assert_eq!(app.update_label().as_deref(), Some("↑ 9.0.0"));
        assert!(app.messages.toast.is_some());
    }

    #[test]
    fn a_check_runs_with_checks_off() {
        let server = FakeHttp::start(vec![("GET /releases/latest", 200, LATEST)]);
        let (mut app, rx) = looking(&server);
        app.config.check_updates = false;

        let answer = reply(&mut app, &rx, "update-check");

        assert_eq!(info(&answer)["release"]["version"], "9.0.0");
    }

    #[test]
    fn two_checks_at_once_ask_github_once() {
        let server = FakeHttp::start(vec![("GET /releases/latest", 200, LATEST)]);
        let (mut app, rx) = looking(&server);

        send_as(&mut app, 7, "update-check");
        send_as(&mut app, 8, "update-check");
        let got = replies(&mut app, &rx, &[7, 8]);

        assert_eq!(server.requests().len(), 1);
        assert!(got.iter().all(|(_, answer)| info(answer)["release"]["version"] == "9.0.0"));
    }

    #[test]
    fn a_failed_check_says_why() {
        let server = FakeHttp::start(vec![("GET /releases/latest", 503, "{}")]);
        let (mut app, rx) = looking(&server);

        let answer = reply(&mut app, &rx, "update-check");

        assert_eq!(code(&answer), "failed");
        let message = answer["error"]["message"].as_str().expect("a message");
        assert!(message.contains("GitHub answered 503"), "{message}");
        let state = app.update_info();
        assert_eq!(serde_json::to_value(state.none).expect("none"), "failed");
        assert!(state.error.is_some_and(|error| error.contains("503")));
    }

    #[rstest]
    #[case::disabled(false, true, false, false, "disabled")]
    #[case::off(true, false, false, false, "off")]
    #[case::unchecked(true, true, false, false, "unchecked")]
    #[case::up_to_date(true, true, true, false, "up-to-date")]
    #[case::failed(true, true, true, true, "failed")]
    fn the_state_says_why_there_is_no_update(
        #[case] enabled: bool,
        #[case] looks: bool,
        #[case] answered: bool,
        #[case] failed: bool,
        #[case] expected: &str,
    ) {
        let (mut app, _rx) = empty_app();
        app.update.updates.enabled = enabled;
        app.config.check_updates = looks;
        if answered {
            app.update.updates.answered = Some(SystemTime::now());
        }
        if failed {
            app.update.updates.error = Some("offline".into());
        }

        assert_eq!(serde_json::to_value(app.update_info().none).expect("none"), expected);
    }

    #[test]
    fn a_check_during_an_install_keeps_the_release_being_installed() {
        let (mut app, _rx) = empty_app();
        found(&mut app);
        app.update.updates.installing = true;
        let later = update::release(&json!({"tag_name": "v9.1.0", "assets": []})).expect("a release");

        app.update_checked(Ok(Some(later)));

        assert_eq!(app.update_label().as_deref(), Some("↑ 9.0.0"));
    }
}

mod installing {
    use super::*;

    struct Replacing {
        app: App,
        rx: Receiver<AppEvent>,
        exe: PathBuf,
        files: FakeHttp,
        _releases: FakeHttp,
    }

    fn replacing(tmp: &TempDir, sums: impl FnOnce(&[u8]) -> String) -> Replacing {
        let (releases, files) = publish_for(tmp, target(), sums);
        let exe = installed(tmp);
        let (mut app, rx) = looking(&releases);
        app.update.updates.install = Install::Replace(exe.clone());
        Replacing { app, rx, exe, files, _releases: releases }
    }

    #[test]
    fn install_needs_a_release() {
        let (mut app, _rx) = empty_app();
        app.update.updates.enabled = true;
        assert_eq!(refused(&mut app, "update-install", &json!({})).code, "conflict");
    }

    #[test]
    fn an_install_by_command_is_refused_naming_it() {
        let (mut app, _rx) = empty_app();
        app.update.updates.enabled = true;
        app.update.updates.install = Install::Command(update::BREW);
        found(&mut app);

        let refusal = refused(&mut app, "update-install", &json!({}));

        assert_eq!(refusal.code, "conflict");
        assert!(refusal.message.contains("brew"), "{}", refusal.message);
    }

    #[test]
    fn install_replaces_the_binary_and_a_restart_names_it() {
        let tmp = TempDir::new();
        let Replacing { mut app, rx, exe, files: _files, _releases } = replacing(&tmp, |b| sums_for(target(), b));
        assert_eq!(code(&reply(&mut app, &rx, "update-install")), "conflict");
        reply(&mut app, &rx, "update-check");

        let answer = reply(&mut app, &rx, "update-install");

        assert_eq!(info(&answer)["installed"], true);
        assert_eq!(std::fs::read_to_string(&exe).expect("binary"), NEW);
        assert_eq!(app.update_label().as_deref(), Some(RESTART_LABEL));
        app.outbox.restart();
        assert_eq!(app.take_restart(), Some(exe));
    }

    #[test]
    fn an_install_running_is_joined() {
        let tmp = TempDir::new();
        let Replacing { mut app, rx, exe: _exe, files, _releases } = replacing(&tmp, |b| sums_for(target(), b));
        reply(&mut app, &rx, "update-check");

        send_as(&mut app, 7, "update-install");
        send_as(&mut app, 8, "update-install");
        let got = replies(&mut app, &rx, &[7, 8]);

        assert_eq!(files.requests().len(), 2);
        assert!(got.iter().all(|(_, answer)| info(answer)["installed"] == true));
    }

    #[test]
    fn the_dialog_shows_an_install_a_request_started() {
        let tmp = TempDir::new();
        let Replacing { mut app, rx, exe: _exe, files, _releases } = replacing(&tmp, |b| sums_for(target(), b));
        reply(&mut app, &rx, "update-check");

        send_as(&mut app, 7, "update-install");
        app.open_update();
        assert!(matches!(&app.overlay, Some(Overlay::Update(UpdateStep::Updating))));
        app.submit_form(AREA).expect("submit");
        replies(&mut app, &rx, &[7]);

        assert_eq!(files.requests().len(), 2);
    }

    #[test]
    fn a_failed_install_says_why_and_can_be_tried_again() {
        let tmp = TempDir::new();
        let Replacing { mut app, rx, exe: _exe, files: _files, _releases } =
            replacing(&tmp, |_| sums_for(target(), b"other"));
        reply(&mut app, &rx, "update-check");

        let first = reply(&mut app, &rx, "update-install");
        let again = reply(&mut app, &rx, "update-install");

        assert_eq!(code(&first), "failed", "{first}");
        assert!(first["error"]["message"].as_str().expect("message").contains("checksum"));
        assert_eq!(code(&again), "failed");
        assert!(app.update_info().install_error.is_some());
    }

    #[test]
    fn an_installed_update_answers_at_once() {
        let (mut app, _rx) = empty_app();
        app.update.updates.enabled = true;
        app.update.updates.installed = true;

        let answer = ok(&mut app, "update-install", &json!({}));

        assert_eq!(answer["installed"], true);
    }
}
