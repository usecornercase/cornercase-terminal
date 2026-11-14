use serde_json::{Value, json};

use super::machines::{VPS, in_state, reach, start_listing, wait_for_machine};
use super::remote::on_ssh_path;
use super::*;

const URL: &str = "https://github.com/acme/shop.git";
const ANOTHER_RUN: &str = "1-1";
const VIEW: &str = r#"{"number":7,"title":"Fix the login","state":"OPEN","labels":[],"assignees":[],"author":{"login":"ana"},"updatedAt":"","url":"https://github.com/acme/shop/issues/7","body":"It breaks","comments":[]}"#;

struct OnVps {
    machine: Arc<Session>,
    here: Arc<Session>,
    home: PathBuf,
}

fn on_vps() -> OnVps {
    on_vps_accepting_trust(true)
}

fn on_vps_accepting_trust(accepts: bool) -> OnVps {
    let machine = Session::new();
    let home = machine.dir.join("home");
    std::fs::create_dir_all(&home).expect("create the machine's home");
    let home = home.canonicalize().expect("canonicalize the machine's home");
    let agent = fake_agent_writing(&machine.dir, ">>");
    let config = json!({
        "worktrees_dir": machine.dir.join("worktrees"),
        "gh": fake_issue_gh(&machine.dir, VIEW),
        "agent": "fake",
        "accept_trust_prompts": accepts,
        "agent_commands": agent_commands(&[("fake", &agent), ("claude", &agent)]),
    });
    std::fs::write(machine.dir.join("config.json"), config.to_string()).expect("write the machine's config");
    let here = Session::new();
    let trusting = fake_trusting_agent(&here.dir);
    let config = json!({
        "gh": fake_issue_gh(&here.dir, VIEW),
        "agent": "fake",
        "agent_commands": agent_commands(&[("fake", &trusting)]),
    });
    let reached = format!("{} HOME={}", reach(&machine), home.display());
    start_listing(&here, &[(VPS, VPS)], &[(VPS, &reached)], config);
    wait_for_machine(&here, VPS, "the machine is linked", in_state("linked"));
    OnVps { machine, here, home }
}

fn shop(dir: &std::path::Path) -> PathBuf {
    let repo = repository(dir, "shop");
    git(&repo, &["remote", "add", "origin", URL]);
    repo
}

fn workspaces(project: &Value) -> Vec<Value> {
    project["workspaces"].as_array().cloned().unwrap_or_default()
}

fn has_tab(project: &Value, tab: &Value) -> bool {
    let tabs = |workspace: &Value| workspace["tabs"].as_array().cloned().unwrap_or_default();
    workspaces(project).iter().any(|workspace| tabs(workspace).iter().any(|shown| shown["id"] == *tab))
}

fn answer(text: &str) -> Value {
    serde_json::from_str(text).expect("a JSON answer")
}

impl OnVps {
    fn put_codex_on_the_path(&self) {
        on_ssh_path(&self.here.dir, "codex", "#!/bin/sh\nexit 1\n");
    }

    fn shop_there(&self) -> (PathBuf, String) {
        let repo = shop(&self.machine.dir);
        let project = self.machine.says(&["open", &repo.display().to_string()]);
        self.wait_there("the machine's project shows here", |project| project["repo"] == "github.com/acme/shop");
        (repo, project)
    }

    fn there(&self) -> Vec<Value> {
        let status = answer(&self.here.says(&["status", "--json"]));
        let machines = status["machines"].as_array().cloned().unwrap_or_default();
        let vps = machines.into_iter().find(|machine| machine["name"] == VPS).unwrap_or_default();
        vps["projects"].as_array().cloned().unwrap_or_default()
    }

    fn wait_there(&self, what: &str, cond: impl Fn(&Value) -> bool) {
        wait_until(what, || self.there().iter().any(&cond));
    }

    fn boot_there(&self) -> String {
        let status = answer(&self.here.says(&["status", "--json"]));
        let machines = status["machines"].as_array().cloned().unwrap_or_default();
        let vps = machines.into_iter().find(|machine| machine["name"] == VPS).unwrap_or_default();
        vps["boot"].as_str().unwrap_or_default().to_string()
    }

    fn rows_there(&self) -> Vec<(Value, Value)> {
        let projects = self.there();
        projects.iter().flat_map(workspaces).map(|w| (w["id"].clone(), w["name"].clone())).collect()
    }

    fn window_on(&self, repo: &std::path::Path) -> Harness {
        let mut app = Harness::open(Arc::clone(&self.here), ROWS, COLS);
        app.wait_for("app starts with one terminal", |s| s.contains(&first_entry()));
        self.here.says(&["open", &repo.display().to_string()]);
        let known = |project: &Value| project["name"] == "shop" && project["repo"] == "github.com/acme/shop";
        wait_until("its identity is read", || {
            answer(&self.here.says(&["status", "--json"]))["projects"]
                .as_array()
                .is_some_and(|all| all.iter().any(known))
        });
        app.wait_for("the repo opens", |s| s.contains(&entry("shop")));
        app.click(app.position_of(&entry("shop")));
        app
    }

    fn shown_tab(&self) -> Option<Value> {
        let shown = answer(&self.here.says(&["status", "--json"]))["shown_remote"].clone();
        (shown["machine"] == VPS).then(|| shown["tab"].clone())
    }

    fn why(&self, pane: &str) -> String {
        let folder = self.machine.dir.join("worktrees").display().to_string();
        let listing = std::process::Command::new("ls").args(["-laR", &folder]).output().expect("run ls");
        let screen = self.machine.cli(&["read", "--pane", pane, "--lines", "40"]);
        let log = std::fs::read_to_string(self.machine.dir.join("server.log")).unwrap_or_default();
        let tail = log.lines().skip(log.lines().count().saturating_sub(40)).collect::<Vec<_>>().join("\n");
        let text = |bytes: &[u8]| String::from_utf8_lossy(bytes).into_owned();
        format!(
            "--- worktrees there ---\n{}\n--- the orchestrator's screen ---\n{}{}\n--- server.log there ---\n{tail}",
            text(&listing.stdout),
            text(&screen.stdout),
            text(&screen.stderr),
        )
    }
}

#[test]
fn an_agent_started_on_a_machine_runs_there_and_shows_under_it_here() {
    let vps = on_vps();
    let (repo, project) = vps.shop_there();

    let args = ["start", "--machine", VPS, "--project", &project, "--prompt", "hello", "--json"];
    let started = answer(&vps.here.says(&args));

    assert_eq!(started["machine"], VPS, "{started}");
    let got = repo.join("got").display().to_string();
    vps.machine.wait_for_file(&got, "the agent there gets the prompt", |text| text == "hello");
    vps.wait_there("here lists its tab under vps", |project| has_tab(project, &started["tab"]));
}

#[test]
fn a_workspace_made_on_a_machine_is_listed_under_it_here() {
    let vps = on_vps();
    let (_, project) = vps.shop_there();

    let made = vps.here.says(&["new-workspace", "notes", "--machine", VPS, "--project", &project]);
    let made: u64 = made.parse().expect("the new workspace's id");

    let named = |project: &Value| workspaces(project).iter().any(|w| w["name"] == "notes" && w["id"] == made);
    vps.wait_there("here lists the workspace under vps", named);
}

#[test]
fn a_flow_run_on_a_machine_starts_its_team_in_a_worktree_there() {
    let vps = on_vps();
    let (_, project) = vps.shop_there();
    vps.put_codex_on_the_path();

    let args = ["flow", "run", "Quick", "--machine", VPS, "--project", &project, "--prompt", "add dark mode"];
    let run = answer(&vps.here.says(&[&args[..], &["--worktree", "dark-mode", "--json"]].concat()));

    assert_eq!(run["machine"], VPS, "{run}");
    let members = run["members"].as_array().cloned().unwrap_or_default();
    let mut roles: Vec<&str> = members.iter().filter_map(|member| member["role"].as_str()).collect();
    roles.sort_unstable();
    assert_eq!(roles, ["coder", "orchestrator", "reviewer"], "{run}");
    let agents: Vec<&str> = members.iter().filter_map(|member| member["agent"].as_str()).collect();
    assert_eq!(agents, ["claude"; 3], "a role there runs an agent this test does not fake: {run}");
    let orchestrator = members.iter().find(|member| member["role"] == "orchestrator").expect("the orchestrator");
    let pane = orchestrator["pane"].to_string();
    let folder = PathBuf::from(run["run_dir"].as_str().expect("the run's folder"));
    assert!(!folder.starts_with(&vps.here.dir), "the folder is on the machine, not here: {run}");
    assert_eq!(std::fs::read_to_string(folder.join("task.md")).expect("task.md there"), "add dark mode");
    let got = vps.machine.dir.join("worktrees").join("shop").join("dark-mode").join("got");
    let deadline = Instant::now() + TIMEOUT;
    while !std::fs::read_to_string(&got).unwrap_or_default().contains("add dark mode") {
        assert!(Instant::now() < deadline, "the orchestrator there never got the task\n{}", vps.why(&pane));
        thread::sleep(POLL);
    }
    let branched = |project: &Value| workspaces(project).iter().any(|w| w["branch"] == "dark-mode");
    vps.wait_there("here lists the run's workspace under vps", branched);
}

#[test]
fn a_project_cloned_from_new_workspace_gets_only_the_ticked_files() {
    let vps = on_vps();
    let repo = shop(&vps.here.dir);
    std::fs::write(repo.join(".gitignore"), ".env\nnotes.txt\n").expect("write .gitignore");
    git(&repo, &["add", ".gitignore"]);
    git(&repo, &["commit", "--quiet", "-m", "ignore"]);
    std::fs::write(repo.join(".env"), "TOKEN=1\n").expect("write .env");
    std::fs::write(repo.join("notes.txt"), "mine\n").expect("write notes.txt");
    let bare = vps.machine.dir.join("shop.git");
    git(&vps.machine.dir, &["clone", "--bare", "--quiet", &repo.display().to_string(), "shop.git"]);
    let rewrite = format!("[url \"{}\"]\n\tinsteadOf = {URL}\n", bare.display());
    std::fs::write(vps.home.join(".gitconfig"), rewrite).expect("write the machine's git config");
    let mut app = vps.window_on(&repo);

    let at = ui::new_workspace_button(workspaces_list(), 1, &tab_lines(&[1])).as_position();
    app.click(at);
    app.wait_for("the new menu opens", |s| s.contains("new workspace") && s.contains("start a flow…"));
    app.pick(at, &["new workspace", "start a flow…"], 0);
    app.wait_for("the form offers the machine", |s| s.contains("run on") && s.contains("○ ☁ vps"));
    app.click(app.position_of("○ ☁ vps"));
    app.wait_for("the project is not there yet", |s| s.contains("not on vps · clone…"));
    app.send(b"notes\r");
    app.wait_for("the clone dialog lists the ignored files", |s| {
        s.contains("Clone shop on vps") && s.contains("[ ] .env") && s.contains("[ ] notes.txt")
    });
    app.click(app.position_of("[ ] .env"));
    app.wait_for("the file is ticked", |s| s.contains("[x] .env"));
    app.send(b"\r");

    app.wait_for("back in New workspace with the clone there", |s| s.contains("on vps, in ~/code/shop"));
    let clone = vps.home.join("code").join("shop");
    assert_eq!(std::fs::read_to_string(clone.join(".env")).ok().as_deref(), Some("TOKEN=1\n"));
    assert!(!clone.join("notes.txt").exists(), "an unticked file was copied");
    app.send(b"\r");
    let made = |project: &Value| workspaces(project).iter().any(|w| w["name"] == "notes");
    vps.wait_there("the workspace is made there", made);
}

#[test]
fn an_issue_started_on_a_machine_shows_its_tab_in_this_window() {
    let vps = on_vps();
    vps.shop_there();
    let repo = shop(&vps.here.dir);
    let mut app = vps.window_on(&repo);

    app.click(areas().issues.as_position());
    app.wait_for("the issues show", |s| s.contains("#7") && s.contains("Fix the login"));
    app.send(b"\r");
    app.wait_for("the issue shows", |s| s.contains("It breaks") && s.contains("run on…"));
    app.click(app.position_of("run on…"));
    app.wait_for("the machines are offered", |s| s.contains("this computer") && s.contains("vps"));
    app.send(b"vps\r");
    app.wait_for("it starts on the machine", |s| s.contains("in its own worktree on vps, in "));
    app.click(app.position_of(" start "));
    app.wait_for("start asks for the branch", |s| s.contains("› branch    issue-7-fix-the-login"));
    app.send(b"\r");

    let checkout = vps.machine.dir.join("worktrees").join("shop").join("issue-7-fix-the-login");
    let got = checkout.join("got").display().to_string();
    vps.machine
        .wait_for_file(&got, "the agent there gets the issue", |text| text == "https://github.com/acme/shop/issues/7");
    wait_until("the window shows the machine's tab", || vps.shown_tab().is_some_and(|tab| tab.is_u64()));
}

struct Asking<'a> {
    commands: &'a Session,
    agents: &'a Session,
    place: &'a [&'a str],
    repo: &'a std::path::Path,
}

impl Asking<'_> {
    fn start(&self, args: &[&str]) -> std::process::Child {
        self.commands.spawn(&[&["start", "--json"], self.place, args].concat())
    }

    fn asks_trust(&self, pane: &str) -> bool {
        self.agents.says(&["read", "--pane", pane]).contains("Do you trust")
    }

    fn first_pane_asking_trust(&self) -> String {
        let mut asking = None;
        wait_until("the agent of the plain start asks to trust the folder", || {
            asking = panes(self.agents.report()).map(|pane| pane.id.to_string()).find(|pane| self.asks_trust(pane));
            asking.is_some()
        });
        asking.expect("the pane that asks")
    }

    fn trust_and_see_the_prompt(&self, pane: &str, prompt: &str) {
        self.agents.says(&["keys", "--pane", pane, "enter"]);
        let got = self.repo.join("got").display().to_string();
        self.agents.wait_for_file(&got, "the agent gets its prompt once it is trusted", |text| text.ends_with(prompt));
    }

    fn placed_answers_before_a_plain_start_held_on_its_question(&self) -> Value {
        let mut plain = self.start(&["--prompt", "plain"]);
        let held = self.first_pane_asking_trust();

        let started = answered(self.start(&["--prompt", "placed", "--placed"]));

        let pane = started["pane"].to_string();
        assert_ne!(pane, held, "{started}");
        wait_until("the agent of the placed start asks too", || self.asks_trust(&pane));
        assert!(plain.try_wait().expect("the plain start").is_none(), "the plain start answered with a question up");
        self.trust_and_see_the_prompt(&pane, "placed");
        self.trust_and_see_the_prompt(&held, "plain");
        let plain = answered(plain);
        assert_eq!(plain["pane"].to_string(), held, "{plain}");
        let ids = |answer: &Value| answer.as_object().map(|ids| ids.keys().cloned().collect::<Vec<_>>());
        assert_eq!(ids(&started), ids(&plain), "the same ids");
        started
    }
}

fn answered(child: std::process::Child) -> Value {
    let out = finish_within(child, TIMEOUT);
    assert!(out.status.success(), "{out:?}");
    serde_json::from_slice(&out.stdout).expect("a JSON answer")
}

#[test]
fn a_placed_start_answers_while_its_agent_asks_and_a_plain_one_holds_its_answer() {
    let vps = on_vps_accepting_trust(false);
    let repo = shop(&vps.here.dir);
    let project = vps.here.says(&["open", &repo.display().to_string()]);

    let place = ["--project", &project];
    let asking = Asking { commands: &vps.here, agents: &vps.here, place: &place, repo: &repo };
    let started = asking.placed_answers_before_a_plain_start_held_on_its_question();

    assert_eq!(started["machine"], Value::Null, "{started}");
}

#[test]
fn a_placed_start_on_a_machine_answers_while_the_agent_there_asks_and_a_plain_one_holds_its_answer() {
    let vps = on_vps_accepting_trust(false);
    let (repo, project) = vps.shop_there();

    let place = ["--machine", VPS, "--project", &project];
    let asking = Asking { commands: &vps.here, agents: &vps.machine, place: &place, repo: &repo };
    let started = asking.placed_answers_before_a_plain_start_held_on_its_question();

    assert_eq!(started["machine"], VPS, "{started}");
}

fn refused_as_another_run(out: &std::process::Output, boot: &str) {
    let stderr = String::from_utf8_lossy(&out.stderr);
    let prefix = format!("error: this request is for the cornercase server started as {boot} on {VPS}");
    assert_eq!(out.status.code(), Some(4), "{out:?}");
    assert!(stderr.starts_with(&prefix), "{out:?}");
    assert!(out.stdout.is_empty(), "{out:?}");
}

#[test]
fn work_aimed_at_another_run_of_a_machine_starts_and_shows_nothing_there() {
    let vps = on_vps();
    let (_, project) = vps.shop_there();
    let workspace = vps.there().iter().flat_map(workspaces).map(|w| w["id"].to_string()).next().expect("a workspace");
    let before = vps.rows_there();
    let aimed = ["--machine", VPS, "--boot", ANOTHER_RUN];

    let asked: [Vec<&str>; 4] = [
        ["start", "--project", &project, "--prompt", "hello"].into_iter().chain(aimed).collect(),
        ["new-workspace", "notes", "--project", &project].into_iter().chain(aimed).collect(),
        ["flow", "run", "Quick", "--project", &project, "--prompt", "x"].into_iter().chain(aimed).collect(),
        ["focus", "--workspace", &workspace].into_iter().chain(aimed).collect(),
    ];
    for args in &asked {
        refused_as_another_run(&vps.here.cli(args), ANOTHER_RUN);
    }

    assert_eq!(vps.rows_there(), before);
    assert_eq!(vps.shown_tab(), None);
    let boot = vps.boot_there();
    let made = vps.here.says(&["new-workspace", "notes", "--machine", VPS, "--project", &project, "--boot", &boot]);
    let made: u64 = made.parse().expect("the new workspace's id");
    vps.wait_there("here lists the workspace under vps", |project| {
        workspaces(project).iter().any(|w| w["name"] == "notes" && w["id"] == made)
    });
    let shown = vps.here.says(&["focus", "--machine", VPS, "--workspace", &workspace, "--boot", &boot, "--json"]);
    assert_eq!(answer(&shown)["machine"], VPS, "{shown}");
}

#[test]
fn a_workspace_aimed_before_the_machine_restarted_is_refused_after_it() {
    let vps = on_vps();
    let (repo, project) = vps.shop_there();
    let old = vps.boot_there();

    vps.machine.says(&["restart", "--yes"]);
    wait_until("the machine's server is another run", || {
        let boot = vps.boot_there();
        !boot.is_empty() && boot != old
    });

    let args = ["new-workspace", "notes", "--machine", VPS, "--project", &project];
    refused_as_another_run(&vps.here.cli(&[&args[..], &["--boot", &old]].concat()), &old);
    let project = vps.machine.says(&["open", &repo.display().to_string()]);
    let args = ["new-workspace", "notes", "--machine", VPS, "--project", &project];
    assert!(!vps.rows_there().iter().any(|(_, name)| name == "notes"));
    let new = vps.boot_there();
    vps.here.says(&[&args[..], &["--boot", &new]].concat());
    vps.wait_there("the workspace is made there", |project| workspaces(project).iter().any(|w| w["name"] == "notes"));
}

#[test]
fn a_refusal_from_a_machine_names_it_in_the_command_it_gives() {
    let vps = on_vps();
    let (_, project) = vps.shop_there();

    let flow = ["flow", "run", "Nope", "--machine", VPS, "--project", &project, "--prompt", "x"];
    let out = vps.here.cli(&flow);
    assert!(refused(&out, 1, "(`cornercase flow list` there)"), "{out:?}");

    let start = ["start", "--machine", VPS, "--project", "999", "--prompt", "x"];
    let out = vps.here.cli(&start);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("`cornercase status`") && !stderr.contains("--machine vps"), "{out:?}");
    assert!(!stderr.contains("`cornercase status` there"), "{out:?}");
}
