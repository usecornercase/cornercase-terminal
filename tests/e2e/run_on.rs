use serde_json::{Value, json};

use super::machines::{VPS, in_state, reach, start_listing, wait_for_machine};
use super::*;

const URL: &str = "https://github.com/acme/shop.git";

struct OnVps {
    machine: Arc<Session>,
    here: Arc<Session>,
}

fn on_vps() -> OnVps {
    let machine = Session::new();
    let home = machine.dir.join("home");
    std::fs::create_dir_all(&home).expect("create the machine's home");
    let home = home.canonicalize().expect("canonicalize the machine's home");
    let agent = fake_agent_writing(&machine.dir, ">>");
    let config = json!({
        "worktrees_dir": machine.dir.join("worktrees"),
        "agent": "fake",
        "accept_trust_prompts": true,
        "agent_commands": {"fake": agent, "claude": agent},
    });
    std::fs::write(machine.dir.join("config.json"), config.to_string()).expect("write the machine's config");
    let here = Session::new();
    let config = json!({
        "agent": "fake",
        "agent_commands": {"fake": fake_trusting_agent(&here.dir)},
    });
    let reached = format!("{} HOME={}", reach(&machine), home.display());
    start_listing(&here, &[(VPS, VPS)], &[(VPS, &reached)], config);
    wait_for_machine(&here, VPS, "the machine is linked", in_state("linked"));
    OnVps { machine, here }
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

    let args = ["flow", "run", "Quick", "--machine", VPS, "--project", &project, "--prompt", "add dark mode"];
    let run = answer(&vps.here.says(&[&args[..], &["--worktree", "dark-mode", "--json"]].concat()));

    assert_eq!(run["machine"], VPS, "{run}");
    let members = run["members"].as_array().cloned().unwrap_or_default();
    let mut roles: Vec<&str> = members.iter().filter_map(|member| member["role"].as_str()).collect();
    roles.sort_unstable();
    assert_eq!(roles, ["coder", "orchestrator", "reviewer"], "{run}");
    let orchestrator = members.iter().find(|member| member["role"] == "orchestrator").expect("the orchestrator");
    let pane = orchestrator["pane"].to_string();
    let got = vps.machine.dir.join("worktrees").join("shop").join("dark-mode").join("got");
    let deadline = Instant::now() + TIMEOUT;
    while !std::fs::read_to_string(&got).unwrap_or_default().contains("add dark mode") {
        assert!(Instant::now() < deadline, "the orchestrator there never got the task\n{}", vps.why(&pane));
        thread::sleep(POLL);
    }
    let branched = |project: &Value| workspaces(project).iter().any(|w| w["branch"] == "dark-mode");
    vps.wait_there("here lists the run's workspace under vps", branched);
}
