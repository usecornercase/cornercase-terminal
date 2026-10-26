use serde_json::Value;

use super::*;
use crate::app::plugin_popup::Opening;
use crate::control::{self as wire, Command, Response};
use crate::plugins::manifest::FILE;
use crate::plugins::{Context, Places, Registry};
use crate::process;
use crate::term::mouse;
use crate::test_util::write_executable;

const ID: &str = "acme.pop";

const MANIFEST: &str = r#"id = "acme.pop"
name = "Pop"
version = "1.0"

[[actions]]
id = "watch"
title = "watch it"
contexts = ["button"]
placement = "popup"
width = 60
height = 10
command = ["./watch"]

[[actions]]
id = "stubborn"
title = "ignore hang-ups"
contexts = ["button"]
placement = "popup"
command = ["./stubborn"]

[[actions]]
id = "ok"
title = "say ok"
contexts = ["button"]
placement = "popup"
command = ["./ok"]

[[actions]]
id = "line"
title = "read a line"
contexts = ["button"]
placement = "popup"
width = 60
height = 10
command = ["./line"]

[[actions]]
id = "fail"
title = "break"
contexts = ["button"]
placement = "popup"
command = ["./fail"]
"#;

struct Popped {
    app: App,
    rx: Receiver<AppEvent>,
    root: TempDir,
    project: TempDir,
    places: Places,
    _config: TempDir,
}

fn popped() -> Popped {
    let (config, root, project) = (TempDir::new(), TempDir::new(), TempDir::new());
    std::fs::write(root.path().join(FILE), MANIFEST).expect("manifest");
    let seen =
        "pid=$$ tty=$(tty >/dev/null && echo yes) ctx=$CORNERCASE_CONTEXT plugin=$CORNERCASE_PLUGIN_ID pwd=$(pwd)";
    let watch = format!(
        "#!/bin/sh\nsleep 30 &\necho \"kid=$! {seen}\" > seen.tmp && mv seen.tmp '{}'\nwait\n",
        seen_file(&root)
    );
    write_executable(&root.path().join("watch"), &watch);
    write_executable(&root.path().join("stubborn"), &watch.replacen("#!/bin/sh\n", "#!/bin/sh\ntrap '' HUP\n", 1));
    let line = format!(
        "#!/bin/sh\nprintf '\\033[?1000h\\033[?1006h'\nread line\n{{ stty size; printf '%s' \"$line\" | cat -v; }} > line.tmp && mv line.tmp '{}'\nexec sleep 30\n",
        line_file(&root)
    );
    write_executable(&root.path().join("line"), &line);
    write_executable(&root.path().join("ok"), "#!/bin/sh\nexit 0\n");
    write_executable(&root.path().join("fail"), "#!/bin/sh\nexit 3\n");
    let config_path = config.path().join("config.json");
    let mut registry = Registry::load(&paths::plugins(&config_path));
    registry.link(root.path()).expect("link");
    registry.set_enabled(ID, true).expect("switch on");
    registry.save().expect("save");
    let (app, rx) = app_in(project.path(), config_path.clone());
    let places = Places::new(&config_path, &config_path);
    let mut popped = Popped { app, rx, root, project, places, _config: config };
    popped.app.refresh(Instant::now());
    popped
}

fn line_file(root: &TempDir) -> String {
    root.path().join("said").display().to_string()
}

fn seen_file(root: &TempDir) -> String {
    root.path().join("seen").display().to_string()
}

impl Popped {
    fn open(&mut self, action: &str, request: Option<u64>) {
        let (plugin, action) = self.app.plugin_action(ID, action).expect("the action");
        let env = vec![("CORNERCASE_CONTEXT".to_string(), "button".to_string())];
        let cwd = self.project.path().to_path_buf();
        let on = (wire::Ids::default(), Context::Button);
        let opening = Opening { plugin, action, places: self.places.clone(), cwd, env, on, request };
        self.app.open_plugin_popup(opening).expect("the popup opens");
    }

    fn seen(&self) -> String {
        let mut seen = String::new();
        wait_until("the popup's program runs", || {
            seen = std::fs::read_to_string(seen_file(&self.root)).unwrap_or_default();
            !seen.is_empty()
        });
        seen
    }

    fn pid(&self) -> i32 {
        self.seen_number("pid=")
    }

    fn kid(&self) -> i32 {
        self.seen_number("kid=")
    }

    fn seen_number(&self, key: &str) -> i32 {
        let seen = self.seen();
        let number = seen.split_whitespace().find_map(|word| word.strip_prefix(key)).expect("a pid");
        number.parse().expect("a number")
    }

    fn wait_closed(&mut self) {
        wait_until("the popup closes", || {
            drain(&mut self.app, &self.rx);
            self.app.refresh(Instant::now());
            self.app.popup().is_none()
        });
    }

    fn wait_gone(&mut self, pid: i32) {
        wait_until("the popup's program is gone", || {
            drain(&mut self.app, &self.rx);
            self.app.refresh(Instant::now());
            !process::alive(pid)
        });
    }

    fn line(&self) -> String {
        let mut line = String::new();
        wait_until("the popup's program reads a line", || {
            line = std::fs::read_to_string(line_file(&self.root)).unwrap_or_default();
            !line.is_empty()
        });
        line
    }

    fn wait_mouse(&mut self) {
        wait_until("the popup's program asks for the mouse", || {
            drain(&mut self.app, &self.rx);
            self.app.popup_mut().is_some_and(|p| p.term.emulator.mouse_mode() != mouse::MouseMode::None)
        });
    }

    fn log(&self) -> String {
        std::fs::read_to_string(self.places.log(ID)).unwrap_or_default()
    }

    fn toast(&self) -> Option<String> {
        self.app.messages.toast.as_ref().map(|toast| toast.message.clone())
    }
}

fn ids(value: &Value, found: &mut Vec<u64>) {
    match value {
        Value::Object(map) => {
            found.extend(map.get("id").and_then(Value::as_u64));
            map.values().for_each(|v| ids(v, found));
        }
        Value::Array(items) => items.iter().for_each(|v| ids(v, found)),
        _ => {}
    }
}

fn status_ids(app: &mut App) -> Vec<u64> {
    let request = wire::Request {
        caller: None,
        server: None,
        contract: None,
        boot: None,
        view: None,
        command: Command::Status(wire::Status {}),
    };
    app.request(9, &serde_json::to_string(&request).expect("json"), Some(AREA), Instant::now());
    let answers = app.take_answers();
    let Ok(Response::Ok(value)) = serde_json::from_str(&answers[0].1) else { panic!("a status: {answers:?}") };
    let mut found = Vec::new();
    ids(&value, &mut found);
    found
}

mod opening {
    use super::*;

    #[test]
    fn runs_the_program_in_a_terminal_with_the_plugins_environment() {
        let mut p = popped();
        p.open("watch", None);
        let seen = p.seen();
        let project = p.project.path().display().to_string();
        assert!(seen.contains("tty=yes ctx=button plugin=acme.pop"), "{seen}");
        assert!(seen.ends_with(&format!("pwd={project}\n")), "{seen}");
        assert_eq!(p.app.popup().map(|popup| popup.term.size()), Some((10, 60)));
        assert!(p.log().contains("popup opened"), "{}", p.log());
    }

    #[test]
    fn a_second_popup_is_refused_while_one_is_open() {
        let mut p = popped();
        p.open("watch", None);
        let (plugin, action) = p.app.plugin_action(ID, "ok").expect("the action");
        let cwd = p.project.path().to_path_buf();
        let on = (wire::Ids::default(), Context::Button);
        let opening = Opening { plugin, action, places: p.places.clone(), cwd, env: Vec::new(), on, request: None };
        assert!(p.app.open_plugin_popup(opening).is_err());
        assert_eq!(p.app.popup().map(|popup| popup.action.id.as_str()), Some("watch"));
    }

    #[test]
    fn is_not_a_pane_of_status() {
        let mut p = popped();
        p.open("watch", None);
        p.seen();
        let id = p.app.popup().expect("the popup").term.id;
        assert!(!status_ids(&mut p.app).contains(&id));
    }
}

mod closing {
    use super::*;

    #[test]
    fn closing_it_stops_its_program_and_what_it_started() {
        let mut p = popped();
        p.open("watch", None);
        let (pid, kid) = (p.pid(), p.kid());
        assert!(p.app.close_plugin_popup());
        assert!(p.app.popup().is_none());
        p.wait_gone(pid);
        p.wait_gone(kid);
        assert!(p.log().contains("popup closed"), "{}", p.log());
        assert_eq!(p.toast(), None);
    }

    #[test]
    fn a_program_that_ignores_the_hang_up_is_killed_with_what_it_started() {
        let mut p = popped();
        p.open("stubborn", None);
        let (pid, kid) = (p.pid(), p.kid());
        let closed = Instant::now();
        assert!(p.app.close_plugin_popup());
        p.app.refresh(closed);
        assert!(process::alive(pid) && process::alive(kid));
        p.app.refresh(closed + Duration::from_secs(3));
        p.wait_gone(pid);
        p.wait_gone(kid);
    }

    #[test]
    fn stopping_the_server_stops_its_program_and_what_it_started() {
        let mut p = popped();
        p.open("watch", None);
        let (pid, kid) = (p.pid(), p.kid());
        p.app.stop_plugins();
        assert!(p.app.popup().is_none());
        p.wait_gone(pid);
        p.wait_gone(kid);
    }

    #[test]
    fn stopping_the_server_kills_a_program_that_ignores_the_hang_up_with_what_it_started() {
        let mut p = popped();
        p.open("stubborn", None);
        let (pid, kid) = (p.pid(), p.kid());
        p.app.stop_plugins();
        wait_until("the stubborn program and its child are gone", || !process::alive(pid) && !process::alive(kid));
    }

    #[test]
    fn closing_with_no_popup_does_nothing() {
        let mut p = popped();
        assert!(!p.app.close_plugin_popup());
    }

    #[test]
    fn a_program_that_exits_well_closes_it_quietly() {
        let mut p = popped();
        p.open("ok", None);
        p.wait_closed();
        assert_eq!(p.toast(), None);
    }

    #[test]
    fn a_program_that_fails_closes_it_with_a_toast() {
        let mut p = popped();
        p.open("fail", None);
        p.wait_closed();
        let toast = p.toast().expect("a toast");
        assert!(toast.starts_with("Pop: break failed: exit status: 3"), "{toast}");
        assert!(toast.contains("`cornercase plugin logs acme.pop`"), "{toast}");
    }

    #[test]
    fn a_failure_asked_over_the_socket_leaves_the_answer_to_the_request() {
        let mut p = popped();
        p.open("fail", Some(4));
        p.wait_closed();
        assert_eq!(p.toast(), None);
    }
}

mod sizing {
    use super::*;

    #[test]
    fn a_new_window_size_reaches_its_program() {
        let mut p = popped();
        p.open("line", None);
        p.app.resize(Rect::new(0, 0, 50, 12));
        send_key(&mut p.app, KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(p.line().lines().next(), Some("7 38"));
    }

    #[test]
    fn opens_at_the_size_that_fits_the_window() {
        let mut p = popped();
        p.app.resize(Rect::new(0, 0, 50, 12));
        p.open("watch", None);
        assert_eq!(p.app.popup().map(|popup| popup.term.size()), Some((7, 38)));
    }
}

mod input {
    use super::*;

    fn wanted() -> ui::PopupWanted {
        ui::PopupWanted { cols: Some(60), rows: Some(10) }
    }

    #[test]
    fn esc_reaches_its_program_and_keeps_it_open() {
        let mut p = popped();
        p.open("line", None);
        send_key(&mut p.app, KeyCode::Esc, KeyModifiers::NONE);
        send_key(&mut p.app, KeyCode::Enter, KeyModifiers::NONE);
        assert!(p.line().ends_with("\n^["), "{}", p.line());
        assert!(p.app.popup().is_some());
    }

    #[test]
    fn the_prefix_key_reaches_its_program_instead_of_the_keys_menu() {
        let mut p = popped();
        p.app.config.prefix_key = "ctrl+]".into();
        p.open("line", None);
        send_key(&mut p.app, KeyCode::Char(']'), KeyModifiers::CONTROL);
        send_key(&mut p.app, KeyCode::Enter, KeyModifiers::NONE);
        assert!(p.line().ends_with("\n^]"), "{}", p.line());
        assert!(p.app.popup().is_some());
    }

    #[test]
    fn a_paste_reaches_its_program() {
        let mut p = popped();
        p.open("line", None);
        type_line(&mut p.app, "pasted");
        assert!(p.line().ends_with("\npasted"), "{}", p.line());
    }

    #[test]
    fn a_click_on_a_project_row_does_nothing() {
        let mut p = popped();
        let other = TempDir::new();
        p.app.open_project(other.path().to_path_buf(), AREA).expect("open project");
        p.open("watch", None);
        let first = sidebar_pos(&p.app, SidebarRow::Project(0));
        click(&mut p.app, first);
        assert_eq!((p.app.active, p.app.popup().is_some()), (1, true));
    }

    #[test]
    fn its_close_button_closes_it_and_stops_its_program() {
        let mut p = popped();
        p.open("watch", None);
        let pid = p.pid();
        click(&mut p.app, ui::popup_close(AREA, wanted()).as_position());
        assert!(p.app.popup().is_none());
        p.wait_gone(pid);
    }

    #[test]
    fn a_click_inside_reaches_a_program_that_asked_for_the_mouse() {
        let mut p = popped();
        p.open("line", None);
        p.wait_mouse();
        let inner = ui::popup_inner(AREA, wanted());
        click(&mut p.app, Position::new(inner.x + 2, inner.y + 1));
        send_key(&mut p.app, KeyCode::Enter, KeyModifiers::NONE);
        assert!(p.line().contains("^[[<0;3;2M"), "{}", p.line());
        assert!(p.app.popup().is_some());
    }
}

mod guard {
    use super::*;
    use crate::app::toast::Toast;

    fn call(p: &mut Popped, command: &str, args: &Value) -> Value {
        let text = serde_json::json!({"contract": 1, "command": command, "args": args}).to_string();
        p.app.request(9, &text, Some(AREA), Instant::now());
        let mut answers = Vec::new();
        wait_until("an answer", || {
            drain(&mut p.app, &p.rx);
            answers = p.app.take_answers();
            !answers.is_empty()
        });
        serde_json::from_str(&answers[0].1).expect("a reply")
    }

    fn run_over_the_socket(p: &mut Popped, action: &str) -> Value {
        p.app.window_attached();
        let args = serde_json::json!({"plugin": ID, "action": action, "revision": p.app.plugin_revision()});
        call(p, "plugin-run", &args)
    }

    fn still_open(p: &Popped) -> bool {
        p.app.popup().is_some_and(|popup| popup.action.id == "watch")
    }

    #[test]
    fn a_tour_that_shows_a_dialog_does_not_replace_it() {
        let mut p = popped();
        p.open("watch", None);
        let args = serde_json::json!({"action": "start", "tour": "search", "step": "said"});
        let answer = call(&mut p, "tour", &args);
        assert!(answer["error"].is_null(), "{answer}");
        assert!(still_open(&p));
    }

    #[test]
    fn a_config_request_does_not_replace_it() {
        let mut p = popped();
        p.open("watch", None);
        let answer = call(&mut p, "config", &serde_json::json!({"action": "set", "entry": {"tour": false}}));
        assert!(answer["error"].is_null(), "{answer}");
        assert!(still_open(&p));
    }

    #[test]
    fn running_one_over_the_socket_leaves_an_open_dialog_alone() {
        let mut p = popped();
        p.app.open_settings();
        let answer = run_over_the_socket(&mut p, "watch");
        assert_eq!(answer["error"]["code"], "conflict", "{answer}");
        assert_eq!(answer["error"]["message"], "a dialog is open in the window", "{answer}");
        assert!(matches!(p.app.overlay, Some(Overlay::Settings(_))));
    }

    #[test]
    fn running_another_over_the_socket_is_refused_at_once() {
        let mut p = popped();
        p.open("watch", None);
        let answer = run_over_the_socket(&mut p, "line");
        assert_eq!(answer["error"]["code"], "conflict", "{answer}");
        assert!(still_open(&p));
    }

    #[test]
    fn running_one_over_the_socket_replaces_a_menu() {
        let mut p = popped();
        p.app.overlay = Some(Overlay::Menu { at: Position::new(1, 1), actions: Vec::new() });
        p.app.window_attached();
        let args = serde_json::json!({"plugin": ID, "action": "watch", "revision": p.app.plugin_revision()});
        let text = serde_json::json!({"contract": 1, "command": "plugin-run", "args": args}).to_string();
        p.app.request(9, &text, Some(AREA), Instant::now());
        assert!(still_open(&p));
    }

    #[test]
    fn cancelling_a_form_leaves_it_open() {
        let mut p = popped();
        p.open("watch", None);
        p.app.cancel_form();
        assert!(still_open(&p));
    }

    #[test]
    fn a_click_on_a_toasts_button_does_nothing() {
        let mut p = popped();
        p.open("watch", None);
        let toast = Toast { go: Some("gone".into()), ..Toast::new("an agent needs you", ui::ToastIcon::Check) };
        let button = ui::toast_button(AREA, toast.view());
        p.app.messages.toast = Some(toast);
        click(&mut p.app, button.as_position());
        assert!(still_open(&p));
        assert_eq!(p.toast().as_deref(), Some("an agent needs you"));
    }

    #[test]
    fn recovering_from_a_panic_closes_it_and_stops_its_program() {
        let mut p = popped();
        p.open("watch", None);
        let pid = p.pid();
        p.app.reset_interaction();
        assert!(p.app.popup().is_none());
        p.wait_gone(pid);
        assert!(p.log().contains("popup closed"), "{}", p.log());
    }
}

mod external;
