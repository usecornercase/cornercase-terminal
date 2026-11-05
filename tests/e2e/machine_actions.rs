use serde_json::{Value, json};

use super::machines::{VPS, in_state, reach, start_listing, status, wait_for_machine};
use super::*;

const HIGH: u16 = 40;

struct Linked {
    machine: Arc<Session>,
    app: Harness,
}

fn showing(workspace: &str, worktree: bool) -> Linked {
    showing_at(workspace, worktree, VPS, "")
}

fn showing_at(workspace: &str, worktree: bool, destination: &str, teardown: &str) -> Linked {
    let machine = Session::new();
    let mut config = json!({"worktrees_dir": machine.dir.join("worktrees"), "projects": {}});
    if !teardown.is_empty() {
        config["projects"][machine.dir.join("api").display().to_string()] = json!({"teardown": teardown});
    }
    std::fs::write(machine.dir.join("config.json"), config.to_string()).expect("write the machine's config");
    let here = Session::new();
    start_listing(&here, &[(VPS, destination)], &[(destination, &reach(&machine))], json!({}));
    wait_for_machine(&here, VPS, "the machine is linked", in_state("linked"));
    let repo = repository(&machine.dir, "api").display().to_string();
    let project = machine.says(&["open", &repo]);
    let kind: &[&str] = if worktree { &["--worktree"] } else { &[] };
    machine.says(&[&["new-workspace", workspace, "--project", &project], kind].concat());
    let mut app = Harness::open(here, HIGH, COLS);
    app.wait_for("the machine's project", |s| s.contains(&entry("api")) && s.contains("☁ vps"));
    app.click(app.position_of(&entry("api")));
    let linked = Linked { machine, app };
    wait_until("the machine's workspace shows", || linked.row(workspace).is_some());
    linked
}

fn column(app: &Harness, y: u16) -> String {
    let list = workspaces_list();
    app.row(y).chars().skip(usize::from(list.x)).take(usize::from(list.width)).collect()
}

fn names(machine: &Session) -> Vec<String> {
    let status: Value = serde_json::from_str(&machine.says(&["status", "--json"])).expect("a status report");
    let projects = status["projects"].as_array().cloned().unwrap_or_default();
    let workspaces = projects.iter().flat_map(|p| p["workspaces"].as_array().cloned().unwrap_or_default());
    workspaces.filter_map(|w| w["name"].as_str().map(String::from)).collect()
}

impl Linked {
    fn row(&self, name: &str) -> Option<Rect> {
        let list = workspaces_list();
        let y = (list.y..HIGH).find(|&y| column(&self.app, y).contains(name))?;
        Some(Rect { y, height: 1, ..list })
    }

    fn close(&mut self, name: &str) {
        let row = self.row(name).expect("the workspace's row");
        self.app.click(ui::row_close_button(row, 1).as_position());
    }

    fn menu(&mut self, name: &str) {
        let row = self.row(name).expect("the workspace's row");
        let (x, y) = (row.x + 4, row.y + 1);
        self.app.send(format!("\x1b[<35;{x};{y}M").as_bytes());
        let mut at = None;
        wait_until("its buttons show on hover", || {
            at = column(&self.app, row.y).chars().position(|c| c == '⋯');
            at.is_some()
        });
        let x = u16::try_from(at.expect("the menu button")).expect("a column");
        self.app.click(Position::new(row.x + x, row.y));
    }

    fn until_gone(&self, name: &str) {
        wait_until("the machine no longer lists it", || !names(&self.machine).iter().any(|n| n == name));
        wait_until("its row goes here", || self.row(name).is_none());
    }
}

#[test]
fn a_machine_s_plain_workspace_is_closed_there_from_its_close_button() {
    let mut linked = showing("notes", false);
    assert!(names(&linked.machine).iter().any(|n| n == "notes"));

    linked.close("notes");
    linked.app.wait_for("it asks first", |s| s.contains("Close workspace") && s.contains("on vps"));
    linked.app.send(b"\r");

    linked.until_gone("notes");
}

#[test]
fn a_machine_s_worktree_is_removed_there_from_its_close_button() {
    let mut linked = showing("e2e/gone", true);
    let checkout = linked.machine.dir.join("worktrees").join("api").join("e2e-gone");
    assert!(checkout.is_dir(), "no checkout at {}", checkout.display());

    linked.close("e2e/gone");
    linked.app.wait_for("the machine checks it first", |s| s.contains("Remove workspace") && s.contains("on vps"));
    linked.app.send(b"\r");

    linked.app.wait_for("the toast says it is gone", |s| s.contains("removed e2e/gone on vps"));
    linked.until_gone("e2e/gone");
    assert!(!checkout.exists(), "the checkout is still there");
}

#[test]
fn a_machine_s_teardown_warning_is_logged_here_whole_and_without_its_destination() {
    let said = format!("no route to vps.lan {}", "x".repeat(120));
    let mut linked = showing_at("e2e/gone", true, "vps.lan", &format!("echo '{said}'; exit 1"));

    linked.close("e2e/gone");
    linked.app.wait_for("the machine checks it first", |s| s.contains("Remove workspace") && s.contains("on vps"));
    linked.app.send(b"\r");

    linked.app.wait_for("the toast points at the log", |s| s.contains("cleanup failed in part"));
    let logged = format!("exit status 1: no route to vps {}", "x".repeat(120));
    linked.app.session.wait_for_file("server.log", "the warning is logged", |log| log.contains(&logged));
    let log = std::fs::read_to_string(linked.app.session.dir.join("server.log")).expect("read the log");
    let line = log.lines().find(|line| line.contains(&logged)).expect("the warning's line");
    assert!(line.contains("workspace=e2e/gone machine=vps") && !line.contains("vps.lan"), "{line}");
}

#[test]
fn a_machine_s_workspace_is_renamed_there_from_its_menu() {
    let mut linked = showing("notes", false);

    linked.menu("notes");
    linked.app.wait_for("the menu opens", |s| s.contains("rename workspace"));
    let item = linked.app.position_of("rename workspace");
    linked.app.click(item);
    linked.app.wait_for("the form opens", |s| s.contains("Rename workspace"));
    linked.app.send(&[0x7f; 64]);
    linked.app.send(b"docs\r");

    wait_until("the machine has the new name", || names(&linked.machine).iter().any(|n| n == "docs"));
    wait_until("the row shows it here", || linked.row("docs").is_some());
    linked.until_gone("notes");
}

fn claude_with_a_child(machine: &Session) -> u64 {
    let sessions = machine.claude_dir().join("sessions");
    std::fs::create_dir_all(&sessions).expect("create the sessions folder");
    let claude = temp_dir("claude-with-a-child").join("claude");
    write_executable(
        &claude,
        "#!/bin/sh\nprintf '{\"pid\":%s,\"status\":\"waiting\"}' $$ > \"$1/$$.json\"\n\
         if [ -n \"$2\" ]; then \"$2\" new-tab -- \"$0 $1\"; fi\nread answer\n",
    );
    let repo = repository(&machine.dir, "api").display().to_string();
    let project: u64 = machine.says(&["open", &repo]).parse().expect("a project id");
    let projects = status(machine)["projects"].clone();
    let opened = projects.as_array().and_then(|list| list.iter().find(|p| p["id"] == project));
    let workspace = opened.expect("the opened project")["workspaces"][0]["id"].to_string();
    let command = format!("{} {} {}", claude.display(), sessions.display(), bin());
    machine.says(&["new-tab", "--workspace", &workspace, "--", &command]);
    let mut parent = None;
    wait_until("claude and its child work there", || {
        let agents = agents_of(machine);
        parent = agents.iter().find_map(|pane| pane["parent"].as_u64());
        agents.len() == 2 && parent.is_some()
    });
    parent.expect("the parent")
}

fn agents_of(machine: &Session) -> Vec<Value> {
    let status = status(machine);
    let workspaces = status["projects"].as_array().into_iter().flatten().filter_map(|p| p["workspaces"].as_array());
    let panes =
        workspaces.flatten().filter_map(|w| w["tabs"].as_array()).flatten().filter_map(|t| t["panes"].as_array());
    panes.flatten().filter(|pane| pane["agent"] == "claude").cloned().collect()
}

fn folded(machine: &Session, pane: u64) -> Option<bool> {
    let agents = agents_of(machine);
    let found = agents.iter().find(|agent| agent["id"] == pane)?;
    Some(found["collapsed"].as_bool().unwrap_or(false))
}

fn arrow(app: &Harness, fold: char) -> Option<Position> {
    (0..HIGH).find_map(|y| {
        let row = app.row(y);
        let x = row.contains("☁ vps").then(|| row.chars().position(|c| c == fold)).flatten()?;
        Some(Position::new(u16::try_from(x).ok()?, y))
    })
}

#[test]
fn a_machine_s_agent_is_folded_there_from_its_arrow_in_the_agents_section() {
    let machine = Session::new();
    assert!(machine.cold(&["start-server"]).status.success(), "the server there starts");
    let here = Session::new();
    start_listing(&here, &[(VPS, VPS)], &[(VPS, &reach(&machine))], json!({"agents_section": true}));
    wait_for_machine(&here, VPS, "the machine is linked", in_state("linked"));
    let parent = claude_with_a_child(&machine);
    let mut app = Harness::open(Arc::clone(&here), HIGH, COLS);
    wait_until("its row shows its arrow here", || arrow(&app, '▾').is_some());

    app.click(arrow(&app, '▾').expect("the open arrow"));

    wait_until("the machine folds it", || folded(&machine, parent) == Some(true));
    wait_until("the row shows it folded here", || arrow(&app, '▸').is_some());
    here.wait_for_file("server.log", "the relay is logged", |log| log.contains("verb=collapse"));
    app.click(arrow(&app, '▸').expect("the folded arrow"));
    wait_until("the machine unfolds it", || folded(&machine, parent) == Some(false));
    wait_until("the row shows it open here", || arrow(&app, '▾').is_some());
    assert!(!app.text().contains("could not"), "{}", app.text());
}
