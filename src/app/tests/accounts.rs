use super::*;
use crate::accounts;
use crate::agents::{AgentState, Known};
use crate::project::split::Dir;
use crate::state::{PaneState, ProjectState, State, WorkspaceState};

struct Setup {
    app: App,
    _rx: Receiver<AppEvent>,
    config: TempDir,
    project: TempDir,
}

impl Setup {
    fn new() -> Self {
        let config = TempDir::new();
        let project = TempDir::new();
        let (mut app, rx) = new_app(config.path().join("config.json"));
        app.set_accounts_dir(&config.path().join("accounts"));
        Self { app, _rx: rx, config, project }
    }

    fn root(&self) -> PathBuf {
        self.config.path().join("accounts")
    }

    fn add(&self, kind: Known, name: &str) -> PathBuf {
        accounts::add(&self.root(), kind, name, None).expect("add the account")
    }

    fn use_profile(&mut self, name: &str, chosen: &[(&str, &str)]) {
        let chosen = chosen.iter().map(|(p, a)| ((*p).to_string(), (*a).to_string())).collect();
        self.app.config.profiles.insert(name.to_string(), chosen);
        self.app.config.profile = name.to_string();
    }

    fn open(&mut self) {
        self.app.open_project(self.project.path().to_path_buf(), AREA).expect("open the project");
    }

    fn pane(&self, t: usize) -> &Term {
        tab_term(&self.app, 0, t)
    }

    fn env_of(&mut self, pane: u64) -> Vec<String> {
        let out = self.project.path().join(format!("env-{pane}.txt"));
        let term = self.app.projects.pane_by_mut(pane).expect("the pane");
        assert!(term.write(format!("env > env-{pane}.tmp && mv env-{pane}.tmp env-{pane}.txt\n").as_bytes()));
        wait_until("the shell wrote its environment", || out.exists());
        let written = std::fs::read_to_string(&out).expect("read env");
        std::fs::remove_file(&out).expect("remove the env file");
        written.lines().map(str::to_owned).collect()
    }
}

fn restore_pane(setup: &mut Setup, pane: PaneState) {
    let path = setup.project.path().to_path_buf();
    let mut workspace = WorkspaceState::plain(path.clone(), vec![None], 0);
    workspace.tabs[0].panes = vec![pane];
    let project =
        ProjectState { path, name: None, group: None, workspaces: vec![workspace], active: 0, collapsed: false };
    let saved = State { version: state::VERSION, projects: vec![project], ..State::default() };
    setup.app.config.resume_agents = true;
    setup.app.restore(&saved, AREA);
}

fn value<'a>(env: &'a [String], key: &str) -> Option<&'a str> {
    env.iter().find_map(|line| line.strip_prefix(key)?.strip_prefix('='))
}

#[test]
fn a_new_tab_gets_the_active_profile_s_account() {
    let mut setup = Setup::new();
    let work = setup.add(Known::Claude, "work");
    setup.use_profile("Trabajo", &[("claude", "work")]);
    setup.open();
    let pane = setup.pane(0).id;

    let env = setup.env_of(pane);

    assert_eq!(value(&env, "CLAUDE_CONFIG_DIR"), work.to_str());
    assert_eq!(value(&env, "CLAUDE_SECURESTORAGE_CONFIG_DIR"), None);
    assert_eq!(setup.pane(0).accounts().name(Known::Claude), Some("work"));
}

#[test]
fn switching_to_system_changes_new_tabs_only() {
    let mut setup = Setup::new();
    let work = setup.add(Known::Claude, "work");
    setup.use_profile("Trabajo", &[("claude", "work")]);
    setup.open();
    let old = setup.pane(0).id;

    setup.app.config.profile = String::new();
    let t = setup.app.push_tab(0, 0, AREA, None).expect("a new tab");
    let new = setup.pane(t).id;

    let (old_env, new_env) = (setup.env_of(old), setup.env_of(new));
    assert_eq!(value(&old_env, "CLAUDE_CONFIG_DIR"), work.to_str());
    assert_ne!(value(&new_env, "CLAUDE_CONFIG_DIR"), work.to_str());
    assert!(setup.pane(t).accounts().is_empty());
}

#[test]
fn a_split_keeps_the_accounts_of_its_pane() {
    let mut setup = Setup::new();
    let work = setup.add(Known::Codex, "work");
    setup.use_profile("Trabajo", &[("codex", "work")]);
    setup.open();
    let source = setup.pane(0).id;
    setup.app.config.profile = String::new();

    let split = setup.app.split_pane(source, Dir::Right, AREA, false).expect("split").expect("a new pane");

    let env = setup.env_of(split);
    assert_eq!(value(&env, "CODEX_HOME"), work.to_str());
    let term = setup.app.projects.pane_by_mut(split).expect("the split pane");
    assert_eq!(term.accounts().name(Known::Codex), Some("work"));
}

#[test]
fn an_account_whose_folder_is_gone_is_left_out() {
    let mut setup = Setup::new();
    let work = setup.add(Known::Claude, "work");
    setup.use_profile("Trabajo", &[("claude", "work"), ("codex", "missing")]);
    std::fs::remove_dir_all(&work).expect("remove the account");
    setup.open();

    assert!(setup.pane(0).accounts().is_empty());
}

#[test]
fn a_pane_whose_account_is_gone_comes_back_as_a_shell() {
    let mut setup = Setup::new();
    let agent =
        AgentState { kind: "claude".into(), conversation: "s1".into(), mode: None, account: Some("work".into()) };
    let pane = PaneState {
        agent: Some(agent),
        accounts: [("claude".to_string(), "work".to_string())].into_iter().collect(),
        ..PaneState::at(Some(setup.project.path().to_path_buf()))
    };
    restore_pane(&mut setup, pane);

    let term = setup.pane(0);
    let id = term.id;
    assert!(term.accounts().is_empty());
    assert!(term.resume.is_none());
    assert!(setup.app.agents.launches.is_empty());
    let toast = setup.app.toast.as_ref().map(|toast| toast.message.clone()).unwrap_or_default();
    assert!(toast.contains("`work`"), "{toast}");
    let env = setup.env_of(id);
    assert_eq!(value(&env, "CLAUDE_CONFIG_DIR"), None);
}

#[test]
fn the_toast_names_the_account_of_the_agent_that_does_not_resume() {
    let mut setup = Setup::new();
    let agent =
        AgentState { kind: "codex".into(), conversation: "s1".into(), mode: None, account: Some("work".into()) };
    let accounts = [("claude", "home"), ("codex", "work")];
    let pane = PaneState {
        agent: Some(agent),
        accounts: accounts.iter().map(|(p, a)| ((*p).to_string(), (*a).to_string())).collect(),
        ..PaneState::at(Some(setup.project.path().to_path_buf()))
    };

    restore_pane(&mut setup, pane);

    assert!(setup.pane(0).resume.is_none());
    let toast = setup.app.toast.as_ref().map(|toast| toast.message.clone()).unwrap_or_default();
    assert_eq!(toast, "the codex account `work` is gone: its conversation does not resume");
}
