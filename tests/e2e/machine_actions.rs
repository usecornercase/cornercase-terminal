use serde_json::{Value, json};

use super::machines::{VPS, in_state, reach, start_listing, wait_for_machine};
use super::*;

const HIGH: u16 = 40;

struct Linked {
    machine: Arc<Session>,
    app: Harness,
}

fn showing(workspace: &str, worktree: bool) -> Linked {
    let machine = Session::new();
    let config = json!({"worktrees_dir": machine.dir.join("worktrees")});
    std::fs::write(machine.dir.join("config.json"), config.to_string()).expect("write the machine's config");
    let here = Session::new();
    start_listing(&here, &[(VPS, VPS)], &[(VPS, &reach(&machine))], json!({}));
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
