use std::path::Path;

use serde_json::{Value, json};

use super::commands::Followed;
use super::input::Typist;
use super::*;

const MANIFEST: &str = r#"id = "acme.e2e"
name = "E2E"
version = "0.1.0"

[[actions]]
id = "tally"
title = "count"
contexts = ["button"]
command = ["/bin/sh", "-c", 'echo "$CORNERCASE_CONTEXT $CORNERCASE_PROJECT $CORNERCASE_WORKSPACE $CORNERCASE_TAB $CORNERCASE_PANE $PWD" > "$CORNERCASE_PLUGIN_STATE/tally"']

[[actions]]
id = "board"
title = "show the board"
contexts = ["button"]
placement = "popup"
command = ["/bin/sh", "-c", 'read x; echo "$x $CORNERCASE_CONTEXT $CORNERCASE_WORKSPACE $(stty size) $PWD" > "$CORNERCASE_PLUGIN_STATE/said"']
"#;

struct Served {
    session: Arc<Session>,
    plugin: PathBuf,
    other: PathBuf,
    workspace: u64,
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.plugin);
        let _ = std::fs::remove_dir_all(&self.other);
    }
}

fn served(test: &str) -> Served {
    let session = Session::new();
    let started = session.cold(&["start-server", "--cols", "100", "--rows", "30"]);
    assert!(started.status.success(), "{started:?}");
    let plugin = temp_dir_named(&format!("ccviewplug-{test}-{}", std::process::id()));
    std::fs::write(plugin.join("cornercase-plugin.toml"), MANIFEST).expect("manifest");
    session.says(&["plugin", "link", &plugin.display().to_string()]);
    session.says(&["plugin", "enable", "acme.e2e"]);
    let other = temp_dir_named(&format!("ccviewother-{test}-{}", std::process::id()));
    let opened = said(&session, &["open", &other.display().to_string(), "--json"]);
    let workspace = opened["workspace"].as_u64().expect("its workspace");
    Served { session, plugin, other, workspace }
}

fn said(session: &Session, args: &[&str]) -> Value {
    serde_json::from_str(&session.says(args)).expect("a JSON answer")
}

fn ids_of(ran: &Value) -> Value {
    json!({"project": ran["project"], "workspace": ran["workspace"], "tab": ran["tab"], "pane": ran["pane"]})
}

fn tally(session: &Session) -> String {
    let file = session.dir.join("plugins/state/acme.e2e/tally");
    std::fs::read_to_string(file).expect("what the action wrote").trim_end().to_string()
}

fn wrote(shown: &Value, folder: &Path) -> String {
    let id = |key: &str| shown[key].as_u64().expect("an id");
    format!(
        "button {} {} {} {} {}",
        id("project"),
        id("workspace"),
        id("tab"),
        id("pane"),
        folder.canonicalize().expect("canonical").display()
    )
}

struct Viewed {
    process: std::process::Child,
    _lines: Followed,
    shown: Value,
    id: String,
}

impl Served {
    fn view_on_the_other_project(&self) -> Viewed {
        let mut process = self.session.spawn(&["view", "--cols", "120", "--rows", "40"]);
        let mut lines = Followed::of(&mut process);
        lines.until("the view's first line", |line| line["revision"] == 1);
        let id = lines.seen[0]["view"]["id"].as_u64().expect("an id").to_string();
        self.session.says(&["focus", "--view", &id, "--workspace", &self.workspace.to_string()]);
        lines.until("the view moves", |line| line["view"]["shown"]["workspace"] == self.workspace);
        let shown = lines.seen.last().expect("a line")["view"]["shown"].clone();
        Viewed { process, _lines: lines, shown, id }
    }

    fn popup(&self) -> Value {
        let mut popup = Value::Null;
        wait_until("the popup opens", || {
            popup = said(&self.session, &["status", "--app", "--json"])["popup"].clone();
            !popup.is_null()
        });
        popup
    }
}

fn closed(mut view: Viewed, session: &Session) {
    let _ = view.process.kill();
    let _ = view.process.wait();
    wait_until("the view closes", || session.cli(&["status", "--view", &view.id]).status.code() != Some(0));
}

#[test]
fn a_button_action_runs_on_what_the_view_shows_and_without_a_view_on_what_the_window_shows() {
    let served = served("button");
    let (session, view) = (&served.session, served.view_on_the_other_project());
    let (shown, id) = (view.shown.clone(), view.id.clone());
    let window = said(session, &["status", "--json"])["shown"].clone();
    assert_ne!(shown, window);

    let plain = said(session, &["plugin", "run", "acme.e2e", "tally", "--json"]);
    let plain_wrote = tally(session);
    let viewed = said(session, &["plugin", "run", "acme.e2e", "tally", "--view", &id, "--json"]);
    let viewed_wrote = tally(session);
    let text = session.says(&["plugin", "run", "acme.e2e", "tally", "--view", &id]);
    let gone = session.cli(&["plugin", "run", "acme.e2e", "tally", "--view", "99999"]);

    assert_eq!(ids_of(&plain), window);
    assert!(plain_wrote.starts_with(&format!("button {} ", window["project"])), "{plain_wrote}");
    assert_eq!((&viewed["context"], ids_of(&viewed)), (&json!("button"), shown.clone()));
    assert_eq!(viewed_wrote, wrote(&shown, &served.other));
    assert_eq!(text, "ran acme.e2e tally: exit status: 0");
    assert_eq!(gone.status.code(), Some(1), "{gone:?}");
    assert!(String::from_utf8_lossy(&gone.stderr).contains("there is no view 99999"), "{gone:?}");
    closed(view, session);
}

#[test]
fn a_line_of_requests_runs_the_action_in_its_view_and_the_next_line_where_the_window_is() {
    let served = served("requests");
    let (session, view) = (&served.session, served.view_on_the_other_project());
    let (shown, id) = (view.shown.clone(), view.id.clone());
    let window = said(session, &["status", "--json"])["shown"].clone();
    let mut command = session.cold_command();
    command.args(["requests", "--follow", "-"]);
    let mut typist = Typist::start(command);

    let viewed = typist.types(&json!({"args": ["plugin", "run", "acme.e2e", "tally", "--view", &id, "--json"]}));
    let plain = typist.types(&json!({"args": ["plugin", "run", "acme.e2e", "tally", "--json"]}));
    let ended = typist.ends();

    assert_eq!(ids_of(&viewed["json"]), shown, "{viewed}");
    assert_eq!(ids_of(&plain["json"]), window, "{plain}");
    assert_eq!(ended.status.code(), Some(0), "{ended:?}");
    closed(view, session);
}

#[test]
fn a_popup_action_with_a_view_opens_with_no_window_sized_from_the_view_and_without_one_is_refused() {
    let served = served("popup");
    let (session, view) = (&served.session, served.view_on_the_other_project());
    let (shown, id) = (view.shown.clone(), view.id.clone());

    let refused = session.cli(&["plugin", "run", "acme.e2e", "board"]);
    let waiting = session.spawn(&["plugin", "run", "acme.e2e", "board", "--view", &id, "--json"]);
    let popup = served.popup();
    let later = served.popup();
    session.says(&["input", "--popup", r#"{"text": "hello"}"#, r#"{"key": {"code": "enter"}}"#]);
    let out = finish_within(waiting, TIMEOUT);

    assert_eq!(refused.status.code(), Some(1), "{refused:?}");
    assert!(String::from_utf8_lossy(&refused.stderr).contains("no window is attached"), "{refused:?}");
    assert_eq!((&popup["action"], &popup["cols"], &popup["rows"]), (&json!("board"), &json!(94), &json!(30)));
    assert_eq!(later, popup, "the server's own size leaves it alone");
    let ran: Value = serde_json::from_slice(&out.stdout).expect("a JSON answer");
    assert_eq!((&ran["context"], &ran["ended"], &ran["code"]), (&json!("button"), &json!("exited"), &json!(0)));
    assert_eq!(ids_of(&ran), shown);
    let written = std::fs::read_to_string(session.dir.join("plugins/state/acme.e2e/said")).expect("what it wrote");
    let folder = served.other.canonicalize().expect("canonical");
    assert_eq!(written.trim_end(), format!("hello button {} 30 94 {}", served.workspace, folder.display()));
    closed(view, session);
}

#[test]
fn a_popup_stays_open_when_its_view_goes_and_answers_whoever_waits_on_it_once_it_closes() {
    let served = served("view-goes");
    let (session, view) = (&served.session, served.view_on_the_other_project());
    let id = view.id.clone();
    let waiting = session.spawn(&["plugin", "run", "acme.e2e", "board", "--view", &id]);
    let popup = served.popup();

    closed(view, session);
    let left = served.popup();
    let dead = session.cli(&["plugin", "run", "acme.e2e", "board", "--view", &id]);
    let plain = session.cli(&["plugin", "run", "acme.e2e", "board"]);
    session.says(&["plugin", "popup-close"]);
    let out = finish_within(waiting, TIMEOUT);

    assert_eq!(left, popup);
    assert!(String::from_utf8_lossy(&dead.stderr).contains(&format!("there is no view {id}")), "{dead:?}");
    assert!(String::from_utf8_lossy(&plain.stderr).contains("no window is attached"), "{plain:?}");
    assert!(String::from_utf8_lossy(&out.stdout).starts_with("ran acme.e2e board: the popup was closed"), "{out:?}");
}
