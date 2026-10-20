use super::*;
use crate::accounts::{self, Accounts};
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
    let t = setup.app.push_tab(workspace_at(&setup.app, 0, 0), AREA, None).expect("a new tab").index();
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
    assert!(setup.app.agents.launches.list().is_empty());
    let toast = setup.app.messages.toast.as_ref().map(|toast| toast.message.clone()).unwrap_or_default();
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
    let toast = setup.app.messages.toast.as_ref().map(|toast| toast.message.clone()).unwrap_or_default();
    assert_eq!(toast, "the codex account `work` is gone: its conversation does not resume");
}

mod respawning_a_pane {
    use super::*;

    fn tab(setup: &Setup) -> &Tab {
        &setup.app.projects[0].workspaces[0].tabs[0]
    }

    fn told_about_panes(app: &mut App) -> Vec<String> {
        app.observe().0.into_iter().map(|note| note.message).filter(|m| m.starts_with("pane ")).collect()
    }

    #[test]
    fn keeps_its_id_place_and_parent_with_a_new_shell_on_the_new_account() {
        let mut setup = Setup::new();
        setup.add(Known::Claude, "work");
        let personal = setup.add(Known::Claude, "personal");
        let codex = setup.add(Known::Codex, "cx");
        setup.use_profile("Trabajo", &[("claude", "work"), ("codex", "cx")]);
        setup.open();
        let pane = setup.pane(0).id;
        let split = setup.app.split_pane(pane, Dir::Right, AREA, false).expect("split").expect("a new pane");
        let term = setup.app.projects.pane_by_mut(pane).expect("the pane");
        (term.parent, term.collapsed) = (Some(split), true);
        let (old_pid, layout) = (setup.pane(0).shell_pid(), tab(&setup).layout.clone());
        told_about_panes(&mut setup.app);
        let accounts = Accounts::of(
            &setup.root(),
            &[("claude".to_string(), "personal".to_string()), ("codex".to_string(), "cx".to_string())]
                .into_iter()
                .collect(),
        );

        let cwd = setup.project.path().to_path_buf();
        assert!(setup.app.respawn_in(pane, cwd, accounts).expect("respawn"));

        let tab = tab(&setup);
        let term = tab.pane_by(pane).expect("the same pane id");
        assert_eq!((tab.panes[0].id, &tab.layout), (pane, &layout));
        assert_eq!((term.parent, term.collapsed, term.generation()), (Some(split), true, 1));
        assert_ne!(term.shell_pid(), old_pid);
        assert_eq!(term.accounts().name(Known::Claude), Some("personal"));
        assert_eq!(told_about_panes(&mut setup.app), Vec::<String>::new());
        let env = setup.env_of(pane);
        assert_eq!(value(&env, "CLAUDE_CONFIG_DIR"), personal.to_str());
        assert_eq!(value(&env, "CODEX_HOME"), codex.to_str());
    }

    #[test]
    fn on_the_agent_s_own_login_has_no_account_variable() {
        let mut setup = Setup::new();
        setup.add(Known::Claude, "work");
        setup.use_profile("Trabajo", &[("claude", "work")]);
        setup.open();
        let pane = setup.pane(0).id;

        let cwd = setup.project.path().to_path_buf();
        assert!(setup.app.respawn_in(pane, cwd, Accounts::default()).expect("respawn"));

        assert!(setup.pane(0).accounts().is_empty());
        let env = setup.env_of(pane);
        assert_eq!(value(&env, "CLAUDE_CONFIG_DIR"), None);
    }

    #[test]
    fn the_old_shell_s_exit_and_output_never_reach_the_new_one() {
        let mut setup = Setup::new();
        setup.open();
        let pane = setup.pane(0).id;
        let cwd = setup.project.path().to_path_buf();
        assert!(setup.app.respawn_in(pane, cwd, Accounts::default()).expect("respawn"));

        setup.app.handle_event(AppEvent::Output(pane, 0, b"stale-output".to_vec()), AREA).expect("output");
        setup.app.handle_event(AppEvent::Exited(pane, 0), AREA).expect("exited");

        let term = setup.app.projects.pane_by(pane).expect("the pane is still open");
        assert!(!term.emulator.screen_text().unwrap_or_default().contains("stale-output"));
        setup.app.handle_event(AppEvent::Exited(pane, 1), AREA).expect("exited");
        assert!(setup.app.projects.pane_by(pane).is_none());
    }

    #[test]
    fn a_pane_that_is_gone_is_not_respawned() {
        let mut setup = Setup::new();
        setup.open();
        let cwd = setup.project.path().to_path_buf();

        assert!(!setup.app.respawn_in(u64::MAX, cwd, Accounts::default()).expect("respawn"));
    }
}

mod a_new_tab {
    use super::*;
    use crate::control::AccountUse;
    use crate::project::Workspace;

    fn claude(term: &Term) -> Option<&str> {
        term.accounts().name(Known::Claude)
    }

    fn on_work_then_home(setup: &mut Setup) {
        setup.add(Known::Claude, "work");
        setup.add(Known::Claude, "home");
        setup.use_profile("Trabajo", &[("claude", "work")]);
        setup.open();
        setup.app.config.profile = String::new();
        setup.app.config.active_accounts.insert("claude".into(), "home".into());
    }

    #[test]
    fn keeps_the_accounts_of_the_active_pane_whatever_the_selection_says() {
        let mut setup = Setup::new();
        on_work_then_home(&mut setup);

        setup.app.add_tab(workspace_at(&setup.app, 0, 0), AREA).expect("a new tab");

        let new = setup.pane(1).id;
        assert_eq!(claude(setup.pane(1)), Some("work"));
        let env = setup.env_of(new);
        assert_eq!(value(&env, "CLAUDE_CONFIG_DIR"), setup.root().join("claude").join("work").to_str());
    }

    #[test]
    fn from_the_tree_keeps_the_accounts_of_its_own_workspace() {
        let mut setup = Setup::new();
        on_work_then_home(&mut setup);
        let other = TempDir::new();
        setup.app.open_project(other.path().to_path_buf(), AREA).expect("open another project");
        assert_eq!((setup.app.active, claude(tab_term(&setup.app, 0, 0))), (1, Some("home")));

        setup.app.add_tab_row(0, 0, AREA).expect("a new tab");

        let at = tab_at(&setup.app, 0, 0, 1);
        assert_eq!(setup.app.projects.tab(at).pane().and_then(claude), Some("work"));
    }

    #[test]
    fn whose_account_is_gone_uses_the_agent_s_own_login() {
        let mut setup = Setup::new();
        on_work_then_home(&mut setup);
        std::fs::remove_dir_all(setup.root().join("claude").join("work")).expect("remove the account");

        setup.app.add_tab(workspace_at(&setup.app, 0, 0), AREA).expect("a new tab");

        assert_eq!(claude(setup.pane(1)), None);
    }

    #[test]
    fn in_a_workspace_without_tabs_uses_the_selection() {
        let mut setup = Setup::new();
        on_work_then_home(&mut setup);
        let id = setup.app.take_id();
        let path = setup.project.path().to_path_buf();
        let at = setup.app.projects.push_workspace(0, Workspace::new(id, path, None, false));

        setup.app.add_tab(at, AREA).expect("a new tab");

        let tab = tab_at(&setup.app, 0, at.index(), 0);
        assert_eq!(setup.app.projects.tab(tab).pane().and_then(claude), Some("home"));
    }

    #[test]
    fn changing_the_selection_leaves_the_open_panes_alone() {
        let mut setup = Setup::new();
        on_work_then_home(&mut setup);
        let pid = setup.pane(0).shell_pid();

        let wanted = AccountUse { provider: "claude".into(), account: String::new() };
        setup.app.account_use_request(&wanted).expect("use the system login");

        assert_eq!(setup.app.config.selected_accounts().get("claude"), None);
        assert_eq!((setup.pane(0).shell_pid(), claude(setup.pane(0))), (pid, Some("work")));
        assert!(setup.app.messages.toast.is_none());
    }
}
