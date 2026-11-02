use serde_json::{Value, json};

use super::machines::{VPS, in_state, reach, start_listing, wait_for_machine};
use super::*;

const URL: &str = "https://github.com/acme/shop.git";
const VIEW: &str = r#"{"number":7,"title":"Fix the login","state":"OPEN","labels":[],"assignees":[],"author":{"login":"ana"},"updatedAt":"","url":"https://github.com/acme/shop/issues/7","body":"It breaks","comments":[]}"#;

struct OnVps {
    machine: Arc<Session>,
    here: Arc<Session>,
    home: PathBuf,
}

fn on_vps() -> OnVps {
    let machine = Session::new();
    let home = machine.dir.join("home");
    std::fs::create_dir_all(&home).expect("create the machine's home");
    let agent = fake_trusting_agent(&machine.dir);
    let config = json!({
        "worktrees_dir": machine.dir.join("worktrees"),
        "gh": fake_issue_gh(&machine.dir, VIEW),
        "agent": "fake",
        "accept_trust_prompts": true,
        "agent_commands": {"fake": agent, "claude": agent},
    });
    std::fs::write(machine.dir.join("config.json"), config.to_string()).expect("write the machine's config");
    let here = Session::new();
    let config = json!({
        "gh": fake_issue_gh(&here.dir, VIEW),
        "agent": "fake",
        "agent_commands": {"fake": fake_trusting_agent(&here.dir)},
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

    let args = ["flow", "run", "Quick", "--machine", VPS, "--project", &project, "--prompt", "add dark mode"];
    let run = answer(&vps.here.says(&[&args[..], &["--worktree", "dark-mode", "--json"]].concat()));

    assert_eq!(run["machine"], VPS, "{run}");
    let checkout = vps.machine.dir.join("worktrees").join("shop").join("dark-mode");
    wait_until("the worktree is made there", || checkout.is_dir());
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
