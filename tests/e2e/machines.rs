use cornercase::version;
use serde_json::{Value, json};

use super::agents::claude_asking_on_cue;
use super::commands::Followed;
use super::remote::{boot_of, cornercase_in, fake_ssh_to, gone, hold_ssh_to, let_ssh_to, ssh_pid_to, ssh_pids_to};
use super::views::size_of;
use super::*;

pub(super) const VPS: &str = "vps";

pub(super) fn reach(machine: &Session) -> String {
    format!("env {}", machine.socket_var())
}

fn folder(session: &Session, name: &str) -> PathBuf {
    let dir = session.dir.join(name);
    std::fs::create_dir_all(&dir).expect("create a folder");
    dir
}

fn here_with(listed: &[&str], reached: &[(&str, &str)]) -> Arc<Session> {
    let named: Vec<(&str, &str)> = listed.iter().map(|name| (*name, *name)).collect();
    here_listing(&named, reached)
}

fn here_listing(listed: &[(&str, &str)], reached: &[(&str, &str)]) -> Arc<Session> {
    let here = Session::new();
    start_listing(&here, listed, reached, json!({}));
    here
}

pub(super) fn start_listing(here: &Session, listed: &[(&str, &str)], reached: &[(&str, &str)], mut config: Value) {
    let machines: Vec<Value> = listed
        .iter()
        .map(|(name, destination)| json!({"name": name, "icon": "☁", "destination": destination, "folder": "~/code"}))
        .collect();
    config["machines"] = Value::Array(machines);
    std::fs::write(here.dir.join("config.json"), config.to_string()).expect("write the config");
    let path = fake_ssh_to(&here.dir, reached);
    let started = here
        .cold_command()
        .env("PATH", path)
        .env("CORNERCASE_LOG", "info")
        .arg("start-server")
        .output()
        .expect("run cornercase");
    assert!(started.status.success(), "{started:?}");
}

fn linked_to_one() -> (Arc<Session>, Arc<Session>) {
    let machine = Session::new();
    let here = here_with(&[VPS], &[(VPS, &reach(&machine))]);
    wait_for_machine(&here, VPS, "the machine is linked", in_state("linked"));
    (machine, here)
}

fn machines(here: &Session) -> Vec<Value> {
    let list: Value = serde_json::from_str(&here.says(&["machines", "--json"])).expect("a machine list");
    list["machines"].as_array().cloned().unwrap_or_default()
}

pub(super) fn wait_for_machine(here: &Session, name: &str, what: &str, cond: impl Fn(&Value) -> bool) -> Value {
    let mut machine = Value::Null;
    wait_until(what, || {
        machine = machines(here).into_iter().find(|m| m["name"] == name).unwrap_or_default();
        cond(&machine)
    });
    machine
}

pub(super) fn in_state(state: &'static str) -> impl Fn(&Value) -> bool {
    move |machine| machine["state"] == state
}

fn with_state<'a>(list: &'a [Value], state: &str) -> Option<&'a Value> {
    list.iter().find(|machine| machine["state"] == state)
}

fn name_of(machine: &Value) -> &str {
    machine["name"].as_str().expect("a name")
}

fn boot(session: &Session) -> Value {
    boot_of(&session.says(&["status", "--json"]))
}

#[rstest]
#[case::with_no_server_there(false)]
#[case::with_its_server_running(true)]
fn a_machine_is_linked_to_its_one_server(#[case] running: bool) {
    let machine = Session::new();
    let before = running.then(|| {
        assert!(machine.cold(&["start-server"]).status.success(), "the server there starts");
        boot(&machine)
    });
    let here = here_with(&[VPS], &[(VPS, &reach(&machine))]);

    let vps = wait_for_machine(&here, VPS, "the machine is linked", in_state("linked"));

    assert_eq!(
        (vps["version"].as_str(), vps["server_version"].as_str()),
        (Some(version::CURRENT), Some(version::CURRENT))
    );
    assert_eq!(machine.servers(), 1);
    assert_eq!(before.unwrap_or_else(|| boot(&machine)), boot(&machine));
}

#[test]
fn a_dropped_link_comes_back_to_the_same_server() {
    let (machine, here) = linked_to_one();
    let (kept, first) = (boot(&machine), ssh_pid_to(&here, VPS));

    let killed = std::process::Command::new("kill").args(["-9", &first]).status().expect("run kill");

    assert!(killed.success(), "kill failed");
    wait_until("a new ssh", || ssh_pid_to(&here, VPS) != first);
    wait_for_machine(&here, VPS, "the machine is linked again", in_state("linked"));
    here.wait_for_file("server.log", "the drop is logged", |log| {
        log.contains("machine unreachable name=vps failed=1 ")
    });
    assert_eq!((boot(&machine), machine.servers()), (kept, 1));
}

#[test]
fn a_machine_without_cornercase_says_so_and_links_on_retry_once_it_is_there() {
    let machine = Session::new();
    let there = folder(&machine, "there");
    let home = folder(&machine, "home");
    let prefix = format!("{} HOME={} PATH={}:/usr/bin:/bin", reach(&machine), home.display(), there.display());
    let here = here_with(&[VPS], &[(VPS, &prefix)]);
    let missing = wait_for_machine(&here, VPS, "cornercase is not found there", in_state("unreachable"));
    assert_eq!(missing["error"], "cornercase was not found on vps: install it there");

    cornercase_in(&there);
    here.says(&["machines", "retry", VPS]);

    wait_for_machine(&here, VPS, "the machine is linked well before its next try", in_state("linked"));
    assert_eq!(machine.servers(), 1);
}

#[test]
fn a_machine_that_is_down_does_not_hold_up_another() {
    let machine = Session::new();
    let here = here_with(&["down", VPS], &[(VPS, &reach(&machine))]);

    let down = wait_for_machine(&here, "down", "ssh fails", in_state("unreachable"));

    assert_eq!(down["error"], "ssh: connect to host down port 22: Connection refused");
    wait_for_machine(&here, VPS, "the other machine is linked", in_state("linked"));
}

#[rstest]
#[case::removing_the_machine(&["machines", "remove", "vps"])]
#[case::changing_its_destination(&["machines", "edit", "vps", "--destination", "pi"])]
#[case::stopping_the_server_here(&["kill-server"])]
fn ending_a_link_leaves_the_server_there_running(#[case] ending: &[&str]) {
    let (machine, here) = linked_to_one();
    let (kept, ssh) = (boot(&machine), ssh_pid_to(&here, VPS));

    here.says(ending);

    wait_until("the link's ssh ends", || gone(&ssh));
    assert_eq!((boot(&machine), machine.servers()), (kept, 1));
}

#[test]
fn editing_a_machine_but_its_destination_keeps_its_link() {
    let (_machine, here) = linked_to_one();
    let ssh = ssh_pid_to(&here, VPS);

    here.says(&["machines", "edit", VPS, "--name", "box", "--icon", "laptop", "--folder", "/srv"]);
    here.says(&["machines", "add", "pi", "pi"]);

    wait_until("the machine added next gets its link", || !ssh_pid_to(&here, "pi").is_empty());
    let kept = wait_for_machine(&here, "box", "the machine is renamed", in_state("linked"));
    assert_eq!((ssh_pid_to(&here, VPS), kept["icon"].as_str(), gone(&ssh)), (ssh, Some("▭"), false));
}

#[test]
fn a_machine_that_is_this_computer_is_not_linked() {
    let here = here_with(&["self"], &[("self", "")]);

    let itself = wait_for_machine(&here, "self", "the link finds this server", in_state("unreachable"));

    assert_eq!(itself["error"], "that is this computer");
    wait_until("its ssh ends", || gone(&ssh_pid_to(&here, "self")));
}

#[test]
fn two_machines_on_one_server_are_linked_once() {
    let machine = Session::new();
    let there = reach(&machine);
    let here = here_with(&[VPS, "box"], &[(VPS, &there), ("box", &there)]);

    let mut list = Vec::new();
    wait_until("one of them is refused", || {
        list = machines(&here);
        with_state(&list, "linked").is_some() && with_state(&list, "unreachable").is_some()
    });

    let (linked, refused) =
        (with_state(&list, "linked").expect("linked"), with_state(&list, "unreachable").expect("refused"));
    assert_eq!(refused["error"], format!("the same server as {}", name_of(linked)));
    wait_until("the refused link's ssh ends", || gone(&ssh_pid_to(&here, name_of(refused))));
    wait_until("the server that lost the lock there ends", || machine.servers() == 1);
}

#[test]
fn an_older_machine_is_updated_there_and_then_linked() {
    let machine = Session::new();
    let old = folder(&machine, "old");
    let script = format!(
        "#!/bin/sh\necho \"$*\" >> '{old}/args'\nif [ -e '{old}/updated' ]; then exec '{bin}' \"$@\"; fi\n\
         if [ \"$1\" = update ]; then : > '{old}/updated'; exit 0; fi\necho 'cornercase-proxy 1 0.0.1'\nexec cat > /dev/null\n",
        old = old.display(),
        bin = bin()
    );
    write_executable(&old.join("cornercase"), &script);
    let here = here_with(&[VPS], &[(VPS, &format!("{} PATH={}:/usr/bin:/bin", reach(&machine), old.display()))]);
    let older = wait_for_machine(&here, VPS, "the machine is older", in_state("older"));
    assert_eq!(older["version"], "0.0.1");

    here.says(&["machines", "update", VPS]);

    wait_for_machine(&here, VPS, "the machine is linked at once", in_state("linked"));
    let ran = std::fs::read_to_string(old.join("args")).expect("the arguments it got");
    assert_eq!(ran.lines().filter(|args| *args != "proxy").collect::<Vec<_>>(), ["update"], "{ran}");
}

#[test]
fn an_update_that_fails_there_is_logged_by_the_machine_s_name_only() {
    let destination = "ana@vps.example.com";
    let machine = Session::new();
    let old = folder(&machine, "old");
    let script = "#!/bin/sh\nif [ \"$1\" = update ]; then echo 'cannot reach vps.example.com port 443' >&2; exit 1; fi\n\
                  echo 'cornercase-proxy 1 0.0.1'\nexec cat > /dev/null\n";
    write_executable(&old.join("cornercase"), script);
    let prefix = format!("{} PATH={}:/usr/bin:/bin", reach(&machine), old.display());
    let here = here_listing(&[(VPS, destination)], &[(destination, &prefix)]);
    wait_for_machine(&here, VPS, "the machine is older", in_state("older"));

    let failed = here.cli(&["machines", "update", VPS]);

    assert!(!failed.status.success(), "{failed:?}");
    here.wait_for_file("server.log", "the failed update is logged", |log| log.contains("update failed"));
    let log = std::fs::read_to_string(here.dir.join("server.log")).expect("read the log");
    assert!(log.contains("cannot reach vps port 443") && !log.contains("vps.example.com"), "{log}");
}

const SHOP: &str = "github.com/acme/web-shop";

fn clone_of(session: &Session, name: &str, url: &str) -> String {
    let dir = folder(session, name);
    git(&dir, &["init", "--quiet"]);
    git(&dir, &["remote", "add", "origin", url]);
    dir.display().to_string()
}

fn status(session: &Session) -> Value {
    serde_json::from_str(&session.says(&["status", "--json"])).expect("a status report")
}

fn project_named<'a>(projects: &'a Value, name: &str) -> &'a Value {
    projects.as_array().and_then(|list| list.iter().find(|p| p["name"] == name)).unwrap_or(&Value::Null)
}

fn sent_with_their_repos(here: &Session, names: &[&str]) -> Value {
    let mut sent = Value::Null;
    wait_until("the machine sends its projects with their repos", || {
        sent = status(here)["machines"][0]["projects"].clone();
        names.iter().all(|name| project_named(&sent, name)["repo"].is_string())
    });
    sent
}

#[test]
fn a_machine_s_clone_is_paired_with_the_local_clone_of_the_same_repo_by_another_url() {
    let (machine, here) = linked_to_one();
    let local = here.says(&["open", &clone_of(&here, "web-shop", "https://github.com/acme/web-shop")]);
    machine.says(&["open", &clone_of(&machine, "web-shop", "git@github.com:acme/web-shop.git")]);
    machine.says(&["open", &clone_of(&machine, "blog", "git@github.com:ana/blog.git")]);
    wait_until("the local clone has its repo", || {
        project_named(&status(&here)["projects"], "web-shop")["repo"].is_string()
    });

    let sent = sent_with_their_repos(&here, &["web-shop", "blog"]);

    let (shop, blog) = (project_named(&sent, "web-shop"), project_named(&sent, "blog"));
    let local: u64 = local.parse().expect("a project id");
    assert_eq!((&shop["repo"], &shop["paired"], &shop["machine"]), (&json!(SHOP), &json!(local), &json!(VPS)));
    assert_eq!((&blog["repo"], &blog["paired"]), (&json!("github.com/ana/blog"), &Value::Null));
    let ours = project_named(&status(&here)["projects"], "web-shop").clone();
    assert_eq!((&ours["repo"], &ours["remotes"][0]["url"]), (&json!(SHOP), &json!("https://github.com/acme/web-shop")));
}

#[test]
fn a_machine_s_workspace_is_shown_from_the_command_line() {
    let (machine, here) = linked_to_one();
    machine.says(&["open", &clone_of(&machine, "blog", "git@github.com:ana/blog.git")]);
    let sent = sent_with_their_repos(&here, &["blog"]);
    let blog = project_named(&sent, "blog");
    let workspace = blog["workspaces"][0]["id"].as_u64().expect("a workspace id").to_string();

    let answer: Value =
        serde_json::from_str(&here.says(&["focus", "--machine", VPS, "--workspace", &workspace, "--json"]))
            .expect("the shown workspace");

    assert_eq!((&answer["machine"], &answer["project"]), (&json!(VPS), &blog["id"]));
    assert_eq!(status(&here)["shown_remote"], answer);
}

fn claude_working_there(machine: &Session, workspace: &str) -> (PathBuf, PathBuf) {
    let (sessions, bin) = claude_asking_on_cue(machine, "claude-there");
    let command = format!("{} {}", bin.join("claude").display(), sessions.display());
    machine.says(&["new-tab", "--workspace", workspace, "--", &command]);
    machine.wait_for_working();
    (sessions, bin)
}

#[test]
fn a_machine_s_agent_shows_under_the_local_project_and_tells_when_it_needs_you() {
    let machine = Session::new();
    assert!(machine.cold(&["start-server"]).status.success(), "the server there starts");
    let here = here_with(&[VPS], &[(VPS, &reach(&machine))]);
    wait_for_machine(&here, VPS, "the machine is linked", in_state("linked"));
    here.says(&["open", &clone_of(&here, "web-shop", "https://github.com/acme/web-shop")]);
    machine.says(&["open", &clone_of(&machine, "web-shop", "git@github.com:acme/web-shop.git")]);
    let sent = sent_with_their_repos(&here, &["web-shop"]);
    let workspace = project_named(&sent, "web-shop")["workspaces"][0]["id"].to_string();
    let (sessions, bin) = claude_working_there(&machine, &workspace);
    wait_until("claude works there", || status(&here)["machines"].to_string().contains(r#""status":"working""#));
    let mut app = Harness::open(Arc::clone(&here), 40, COLS);
    app.wait_for("the machine's project with no clone here", |s| s.contains("☁ vps"));
    app.click(app.position_of("web-shop ("));
    app.wait_for("the machine's workspace under the local project", |s| s.matches("☁ vps").count() == 2);
    let mut notices = here.spawn(&["events", "--notices", "--json"]);
    here.wait_for_file("server.log", "it follows", |log| log.contains("command=events"));
    let mut followed = Followed::of(&mut notices);

    std::fs::write(sessions.join("ask"), "").expect("ask");

    followed.until("the machine's notice", |e| e["event"] == "notice" && e["machine"] == VPS);
    app.wait_for_raw("the desktop is told", |raw| {
        raw.contains("\x1b]777;notify;cornercase;claude on vps needs you in ")
    });
    app.wait_for("a toast says where", |s| s.contains("claude on vps needs you in web-shop"));
    app.wait_for("the tab there says it needs you", |s| s.contains("└ ! "));
    app.click(app.position_of("└ ! "));
    let tabs = project_named(&status(&machine)["projects"], "web-shop")["workspaces"][0]["tabs"].clone();
    let asking = tabs.as_array().and_then(|tabs| tabs.last()).expect("claude's tab there")["id"].clone();
    wait_until("the tab there that needs you is shown", || {
        let shown = status(&here)["shown_remote"].clone();
        (&shown["machine"], &shown["tab"]) == (&json!(VPS), &asking)
    });
    app.wait_for("the card says where it runs, or the tab streams from there", |s| {
        (s.contains("runs on vps") && s.contains("claude is waiting for you"))
            || status(&here)["shown_remote"]["stream"] == "live"
    });
    let _ = std::fs::remove_dir_all(&bin);
}

const PRINTED: &str = "printed-there";

struct ShownThere {
    machine: Arc<Session>,
    here: Arc<Session>,
    app: Harness,
    pane: String,
    link: String,
}

fn echo_of(text: &str) -> String {
    let (head, tail) = text.split_at(text.len() / 2);
    format!("echo {head}\"\"{tail}")
}

fn printed_there(machine: &Session, pane: &str, text: &str) {
    machine.says(&["send", "--pane", pane, "--enter", &echo_of(text)]);
}

fn shown_there() -> ShownThere {
    let (machine, here) = linked_to_one();
    let link = ssh_pid_to(&here, VPS);
    machine.says(&["open", &clone_of(&machine, "blog", "git@github.com:ana/blog.git")]);
    let workspace = project_named(&sent_with_their_repos(&here, &["blog"]), "blog")["workspaces"][0].clone();
    let pane = workspace["tabs"][0]["panes"][0]["id"].to_string();
    let mut app = Harness::open(Arc::clone(&here), ROWS, COLS);
    app.wait_for("the window opens", |s| s.contains("☁ vps"));
    here.says(&["focus", "--machine", VPS, "--workspace", &workspace["id"].to_string()]);
    printed_there(&machine, &pane, PRINTED);
    app.wait_for("what the pane there prints shows here", |s| s.contains(PRINTED));
    ShownThere { machine, here, app, pane, link }
}

fn pane_area(rows: u16, cols: u16) -> String {
    let pane =
        ui::layout_with(Rect::new(0, 0, cols, rows), state::Widths::default(), false, config::Sidebar::default()).pane;
    format!("{}x{}", pane.height, pane.width)
}

fn dim_at(app: &Harness, text: &str) -> bool {
    let at = app.position_of(text);
    app.screen.lock().screen().cell(at.y, at.x).is_some_and(vt100::Cell::dim)
}

#[test]
fn a_machine_s_tab_shows_here_at_the_size_of_the_pane_area() {
    let mut shown = shown_there();
    let before = size_of(&shown.machine, &shown.pane, "before");
    let (rows, cols) = (ROWS + 6, COLS + 20);

    shown.app.resize(rows, cols);

    let mut tries = 0;
    wait_until("the pane there takes the new size", || {
        tries += 1;
        size_of(&shown.machine, &shown.pane, &format!("after{tries}")) == pane_area(rows, cols)
    });
    assert_eq!(before, pane_area(ROWS, COLS));
}

fn stream_lines(shown: &ShownThere) -> Vec<String> {
    ssh_pids_to(&shown.here, VPS).into_iter().filter(|pid| *pid != shown.link && !gone(pid)).collect()
}

fn killed(lines: &[String]) {
    assert!(!lines.is_empty(), "no stream line to kill");
    let killed = std::process::Command::new("kill").arg("-9").args(lines).status().expect("run kill");
    assert!(killed.success(), "kill failed: {lines:?}");
}

#[derive(Debug, Clone, Copy)]
enum Dropped {
    Stream,
    Link,
}

#[rstest]
#[case::its_stream(Dropped::Stream, "reconnecting…", "linked")]
#[case::the_link(Dropped::Link, "reconnecting in", "unreachable")]
fn a_dropped_line_dims_the_tab_under_the_card_until_it_streams_again(
    #[case] dropped: Dropped,
    #[case] card: &str,
    #[case] state: &'static str,
) {
    let mut shown = shown_there();
    hold_ssh_to(&shown.here, VPS);
    let lines = match dropped {
        Dropped::Stream => stream_lines(&shown),
        Dropped::Link => vec![shown.link.clone()],
    };

    killed(&lines);

    shown.app.wait_for("the card over the last screen", |s| s.contains("vps unreachable") && s.contains(card));
    let dimmed = dim_at(&shown.app, PRINTED);
    wait_for_machine(&shown.here, VPS, "the link is as it should be", in_state(state));
    let_ssh_to(&shown.here, VPS);
    if matches!(dropped, Dropped::Link) {
        shown.here.says(&["machines", "retry", VPS]);
    }
    shown.app.wait_for("the tab is live again", |s| !s.contains("vps unreachable") && s.contains(PRINTED));
    printed_there(&shown.machine, &shown.pane, "back-again");
    shown.app.wait_for("what it prints next shows", |s| s.contains("back-again"));
    assert!(dimmed && !dim_at(&shown.app, "back-again"));
}

fn printed_there_by_typing(shown: &mut ShownThere, text: &str) {
    shown.app.send(format!("{}\r", echo_of(text)).as_bytes());
}

#[test]
fn what_is_typed_here_reaches_the_pane_there_and_what_it_prints_shows_here() {
    let mut shown = shown_there();

    printed_there_by_typing(&mut shown, "typed-here");

    let pattern = "^typed-here$";
    shown.machine.says(&["wait", "--pane", &shown.pane, "--text", pattern, "--timeout", "10"]);
    shown.app.wait_for("what the pane there prints shows here", |s| s.contains("typed-here"));
}

#[test]
fn with_its_stream_dropped_what_is_typed_says_so_and_never_reaches_the_pane_there() {
    let mut shown = shown_there();
    hold_ssh_to(&shown.here, VPS);
    killed(&stream_lines(&shown));
    shown.app.wait_for("the card over the last screen", |s| s.contains("reconnecting…"));

    printed_there_by_typing(&mut shown, "lost-keys");

    shown.app.wait_for("the toast", |s| s.contains("nothing was sent"));
    let_ssh_to(&shown.here, VPS);
    shown.app.wait_for("the tab is live again", |s| !s.contains("reconnecting…") && s.contains(PRINTED));
    printed_there_by_typing(&mut shown, "typed-after");
    shown.machine.says(&["wait", "--pane", &shown.pane, "--text", "^typed-after$", "--timeout", "10"]);
    assert!(!shown.machine.says(&["read", "--pane", &shown.pane]).contains("lost"));
}
