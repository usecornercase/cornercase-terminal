use super::*;
use crate::app::tests::issue_list::FAKE_AGENT;

struct Asking {
    app: App,
    rx: Receiver<AppEvent>,
    repo: TempDir,
    _dirs: (TempDir, TempDir),
}

impl Asking {
    fn new() -> Self {
        let (repo, worktrees, config) = (git_repo(&[("README", "hi")]), TempDir::new(), TempDir::new());
        let agent = config.path().join("agent");
        write_executable(&agent, FAKE_AGENT);
        let settings = Config {
            worktrees_dir: worktrees.path().display().to_string(),
            agent_commands: [("fake".to_string(), agent.display().to_string())].into(),
            ..Config::default()
        };
        let (app, rx) = app_in(repo.path(), write_config(config.path(), &settings));
        Self { app, rx, repo, _dirs: (worktrees, config) }
    }

    fn started(&mut self, worktree: Option<&str>) -> Done {
        ask(&mut self.app, None, placed(worktree));
        done(answered(&mut self.app, &self.rx, "the tab is open"))
    }

    fn asks_you(&mut self, pane: u64) {
        refreshing(&mut self.app, &self.rx, "the agent asks whether to trust the folder", |app| {
            app.agents.launches.list().iter().any(|launch| launch.term == pane && launch.waits_for_you())
        });
    }
}

fn placed(worktree: Option<&str>) -> Command {
    Command::Start(start(worktree))
}

fn start(worktree: Option<&str>) -> wire::Start {
    wire::Start {
        agent: Some("fake".into()),
        prompt: Some("fix the login".into()),
        worktree: worktree.map(str::to_string),
        placed: true,
        ..wire::Start::default()
    }
}

#[test]
fn answers_with_the_new_pane_and_the_agent_still_gets_its_prompt_once_you_trust_the_folder() {
    let mut asking = Asking::new();

    let pane = asking.started(None).ids.pane.expect("the pane");

    asking.asks_you(pane);
    let term = asking.app.projects.pane_by_mut(pane).expect("the pane");
    assert!(term.write(b"yes\r"), "the pane takes the answer");
    let got = asking.repo.path().join("got");
    refreshing(&mut asking.app, &asking.rx, "the prompt is typed", |_| got.exists());
    assert_eq!(std::fs::read_to_string(&got).expect("the prompt"), "fix the login");
}

#[test]
fn in_a_new_worktree_answers_once_the_worktree_opens() {
    let mut asking = Asking::new();

    let started = asking.started(Some("feat/login"));

    let at = asking.app.projects.workspace_index(started.ids.workspace.expect("the workspace")).expect("it");
    assert!(asking.app.projects.workspace(at).worktree);
    asking.asks_you(started.ids.pane.expect("the pane"));
}

#[rstest::rstest]
#[case::wait(wire::Start { wait: true, ..start(None) })]
#[case::timeout(wire::Start { timeout: Some(30.0), ..start(None) })]
fn takes_no_wait(#[case] start: wire::Start) {
    let mut asking = Asking::new();
    let tabs = asking.app.projects[0].workspaces[0].tabs.len();

    let refused = error(now(&mut asking.app, None, Command::Start(start)));

    assert!(refused.contains("`placed` answers once the tab is open"), "{refused}");
    assert_eq!(asking.app.projects[0].workspaces[0].tabs.len(), tabs);
}
