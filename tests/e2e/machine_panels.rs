use serde_json::json;

use super::machines::{
    VPS, in_state, project_named, reach, sent_with_their_repos, start_listing, status, wait_for_machine,
};
use super::*;

struct Shown {
    machine: Arc<Session>,
    app: Harness,
}

fn clone_with_notes(session: &Session, url: &str, notes: &str) -> String {
    let repo = session.dir.join("api");
    std::fs::create_dir_all(&repo).expect("create the clone's folder");
    git(&repo, &["init", "--quiet"]);
    git(&repo, &["remote", "add", "origin", url]);
    std::fs::write(repo.join("notes.txt"), "one\ntwo\n").expect("write the notes");
    git(&repo, &["add", "notes.txt"]);
    git(&repo, &["commit", "--quiet", "-m", "init"]);
    std::fs::write(repo.join("notes.txt"), notes).expect("edit the notes");
    repo.display().to_string()
}

fn gate(machine: &Session) -> String {
    let gate = machine.dir.join("gate");
    let down = machine.dir.join("down");
    write_executable(&gate, &format!("#!/bin/sh\n[ -e '{}' ] && exit 255\nexec \"$@\"\n", down.display()));
    format!("{} {}", gate.display(), reach(machine))
}

fn showing() -> Shown {
    let machine = Session::new();
    let here = Session::new();
    start_listing(&here, &[(VPS, VPS)], &[(VPS, &gate(&machine))], json!({}));
    wait_for_machine(&here, VPS, "the machine is linked", in_state("linked"));
    here.says(&["open", &clone_with_notes(&here, "https://github.com/acme/api", "one\ntwo\n")]);
    machine.says(&["open", &clone_with_notes(&machine, "git@github.com:acme/api.git", "one\nTWO\n")]);
    let sent = sent_with_their_repos(&here, &["api"]);
    let workspace = project_named(&sent, "api")["workspaces"][0]["id"].clone();
    let mut app = Harness::open(Arc::clone(&here), ROWS, COLS);
    app.wait_for("the local clone", |s| s.contains(&entry("api")));

    here.says(&["focus", "--machine", VPS, "--workspace", &workspace.to_string()]);

    wait_until("the machine's workspace streams here", || {
        let shown = status(&here)["shown_remote"].clone();
        (&shown["machine"], &shown["workspace"], &shown["stream"]) == (&json!(VPS), &workspace, &json!("live"))
    });
    Shown { machine, app }
}

impl Shown {
    fn changes(&mut self) {
        let button = self.app.position_of("Changes");
        self.app.click(button);
        self.app.wait_for("the panel lists the file changed there", |s| {
            s.contains("Uncommitted") && s.contains("notes.txt") && s.contains("Changes 1")
        });
    }
}

#[test]
fn a_machine_s_changes_show_the_file_changed_there() {
    let mut shown = showing();

    shown.changes();

    shown.app.wait_for("its counts come from there", |s| s.contains("+1") && s.contains("−1"));
}

#[test]
fn a_machine_s_files_show_its_tree_and_open_a_file_from_there() {
    let mut shown = showing();
    let button = shown.app.position_of("Files");
    shown.app.click(button);
    shown.app.wait_for("the tree from there", |s| s.contains("notes.txt"));

    let file = shown.app.position_of("notes.txt");
    shown.app.click(file);

    shown.app.wait_for("the text from there", |s| s.contains("TWO") && !s.contains("two"));
}

#[test]
fn a_machine_that_goes_down_leaves_its_changes_listed_and_says_so() {
    let mut shown = showing();
    shown.changes();

    std::fs::write(shown.machine.dir.join("down"), "").expect("refuse the next ssh");
    shown.machine.says(&["kill-server"]);

    shown.app.wait_for("the panel says the machine is unreachable", |s| {
        s.contains("vps unreachable") && s.contains("notes.txt")
    });
}
