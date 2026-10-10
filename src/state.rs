use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::changes::{Mode, Scope};
use crate::issues::People;
use crate::protocol;
use crate::split::Node;
use crate::ui::{GroupEntry, Widths};

pub const VERSION: u32 = 5;
pub const SETTLE: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct State {
    pub version: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<GroupEntry>,
    pub projects: Vec<ProjectState>,
    pub active: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub widths: Option<Widths>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issues: Option<IssuesState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub changes: Option<ChangesState>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub todo: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub files: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangesState {
    #[serde(default)]
    pub open: bool,
    #[serde(default)]
    pub mode: Mode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<Scope>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IssuesState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab: Option<String>,
    #[serde(default)]
    pub closed: bool,
    #[serde(default)]
    pub people: People,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectState {
    pub path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<usize>,
    pub workspaces: Vec<WorkspaceState>,
    #[serde(default)]
    pub active: usize,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub collapsed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceState {
    pub path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default)]
    pub worktree: bool,
    pub tabs: Vec<TabState>,
    #[serde(default)]
    pub active: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub collapsed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TabState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub panes: Vec<PaneState>,
    #[serde(default)]
    pub active: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<Node<usize>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaneState {
    pub cwd: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub right_clicks: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<AgentState>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentState {
    pub kind: String,
    pub conversation: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
}

impl WorkspaceState {
    fn plain(path: PathBuf, cwds: Vec<Option<PathBuf>>, active: usize) -> Self {
        let tabs = cwds.into_iter().map(|cwd| TabState {
            name: None,
            panes: vec![PaneState { cwd, right_clicks: false, agent: None }],
            active: 0,
            layout: None,
        });
        Self { path, name: None, worktree: false, tabs: tabs.collect(), active, base: None, collapsed: false }
    }
}

impl State {
    fn folded(self) -> Self {
        let active = self.active;
        let projects = self.projects.into_iter().enumerate().map(|(i, p)| ProjectState { collapsed: i != active, ..p });
        Self { version: VERSION, projects: projects.collect(), ..self }
    }
}

#[derive(Deserialize)]
struct Versioned {
    version: u32,
}

#[derive(Deserialize)]
struct V2State {
    workspaces: Vec<V2Workspace>,
    active: usize,
}

#[derive(Deserialize)]
struct V2Workspace {
    path: PathBuf,
    #[serde(default)]
    name: Option<String>,
    terminals: Vec<PaneState>,
    #[serde(default)]
    active: usize,
}

impl From<V2State> for State {
    fn from(old: V2State) -> Self {
        let projects = old
            .workspaces
            .into_iter()
            .map(|w| {
                let cwds = w.terminals.into_iter().map(|t| t.cwd).collect();
                let workspace = WorkspaceState::plain(w.path.clone(), cwds, w.active);
                ProjectState {
                    path: w.path,
                    name: w.name,
                    group: None,
                    workspaces: vec![workspace],
                    active: 0,
                    collapsed: false,
                }
            })
            .collect();
        Self {
            version: VERSION,
            groups: Vec::new(),
            projects,
            active: old.active,
            widths: None,
            issues: None,
            changes: None,
            todo: false,
            files: false,
        }
    }
}

#[derive(Deserialize)]
struct V1State {
    terminals: Vec<V1Term>,
    active: usize,
}

#[derive(Deserialize)]
struct V1Term {
    cwd: Option<PathBuf>,
    #[serde(default)]
    name: Option<String>,
}

impl From<V1State> for V2State {
    fn from(old: V1State) -> Self {
        let mut active = 0;
        let mut workspaces = Vec::new();
        for (i, term) in old.terminals.into_iter().enumerate() {
            let Some(cwd) = term.cwd else { continue };
            if i <= old.active {
                active = workspaces.len();
            }
            let terminals = vec![PaneState { cwd: Some(cwd.clone()), right_clicks: false, agent: None }];
            workspaces.push(V2Workspace { path: cwd, name: term.name, terminals, active: 0 });
        }
        Self { workspaces, active }
    }
}

pub fn path() -> PathBuf {
    if let Some(socket) = std::env::var_os(protocol::SOCKET_ENV) {
        return Path::new(&socket).with_extension("json");
    }
    let state_home = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| Path::new(&home).join(".local").join("state")))
        .unwrap_or_else(std::env::temp_dir);
    state_home.join("cornercase").join("session.json")
}

pub fn load(path: &Path) -> Option<State> {
    let text = std::fs::read_to_string(path).ok()?;
    match serde_json::from_str::<Versioned>(&text).ok()?.version {
        VERSION => serde_json::from_str(&text).ok(),
        3 | 4 => serde_json::from_str::<State>(&text).ok().map(State::folded),
        2 => serde_json::from_str::<V2State>(&text).ok().map(|v2| State::from(v2).folded()),
        1 => serde_json::from_str::<V1State>(&text).ok().map(|v1| State::from(V2State::from(v1)).folded()),
        _ => None,
    }
}

pub fn save(path: &Path, value: &impl Serialize) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(value)?)?;
    std::fs::rename(tmp, path)
}

pub fn back_up(path: &Path) -> io::Result<PathBuf> {
    let backup = path.with_extension("json.bak");
    if backup.exists() {
        std::fs::rename(&backup, path.with_extension("json.bak.1"))?;
    }
    std::fs::copy(path, &backup)?;
    Ok(backup)
}

#[derive(Debug)]
pub struct Saver<T = State> {
    path: PathBuf,
    saved: Option<T>,
    pending: Option<(T, Instant)>,
}

impl<T: PartialEq + Serialize> Saver<T> {
    pub fn new(path: PathBuf, saved: Option<T>) -> Self {
        Self { path, saved, pending: None }
    }

    pub fn observe(&mut self, state: T, now: Instant) -> io::Result<bool> {
        if self.saved.as_ref() == Some(&state) {
            self.pending = None;
            return Ok(false);
        }
        match &self.pending {
            Some((pending, since)) if *pending == state => {
                if now.duration_since(*since) >= SETTLE {
                    save(&self.path, &state)?;
                    self.saved = Some(state);
                    self.pending = None;
                    return Ok(true);
                }
            }
            _ => self.pending = Some((state, now)),
        }
        Ok(false)
    }

    pub fn flush(&mut self) -> io::Result<()> {
        if let Some((state, _)) = self.pending.take() {
            save(&self.path, &state)?;
            self.saved = Some(state);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::TempDir;

    fn project(dir: &str) -> ProjectState {
        let workspace = WorkspaceState::plain(PathBuf::from(dir), vec![Some(PathBuf::from(dir))], 0);
        ProjectState {
            path: PathBuf::from(dir),
            name: None,
            group: None,
            workspaces: vec![workspace],
            active: 0,
            collapsed: false,
        }
    }

    fn folded(mut state: State) -> State {
        let active = state.active;
        state.projects.iter_mut().enumerate().for_each(|(i, p)| p.collapsed = i != active);
        state
    }

    fn state(dirs: &[&str]) -> State {
        State {
            version: VERSION,
            groups: Vec::new(),
            projects: dirs.iter().map(|d| project(d)).collect(),
            active: 0,
            widths: None,
            issues: None,
            changes: None,
            todo: false,
            files: false,
        }
    }

    mod file {
        use super::*;

        #[test]
        fn round_trips() {
            let tmp = TempDir::new();
            let path = tmp.path().join("nested").join("session.json");

            save(&path, &state(&["/a", "/b"])).expect("save");

            assert_eq!(load(&path), Some(state(&["/a", "/b"])));
        }

        #[rstest::rstest]
        #[case::column_widths(Widths { projects: 40, workspaces: 20, ..Widths::default() })]
        #[case::stacked_line(Widths { stack: Some(12), ..Widths::default() })]
        fn round_trips_the_widths(#[case] widths: Widths) {
            let tmp = TempDir::new();
            let path = tmp.path().join("session.json");
            let saved = State { widths: Some(widths), ..state(&["/a"]) };

            save(&path, &saved).expect("save");

            assert_eq!(load(&path), Some(saved));
        }

        #[test]
        fn files_without_widths_load_with_none() {
            let tmp = TempDir::new();
            let path = tmp.path().join("session.json");
            std::fs::write(&path, r#"{"version":3,"projects":[],"active":0}"#).expect("write");

            assert_eq!(load(&path).expect("load").widths, None);
        }

        #[test]
        fn widths_saved_before_the_line_load_without_it() {
            let tmp = TempDir::new();
            let path = tmp.path().join("session.json");
            let text = r#"{"version":4,"projects":[],"active":0,"widths":{"projects":40,"workspaces":20}}"#;
            std::fs::write(&path, text).expect("write");

            let widths = load(&path).expect("load").widths;

            assert_eq!(widths, Some(Widths { projects: 40, workspaces: 20, ..Widths::default() }));
        }

        #[test]
        fn round_trips_names_worktrees_and_tabs() {
            let tmp = TempDir::new();
            let path = tmp.path().join("session.json");
            let mut saved = state(&["/a"]);
            saved.projects[0].name = Some("api".into());
            let mut workspace = WorkspaceState::plain("/wt".into(), vec![Some("/wt".into()), None], 1);
            workspace.name = Some("login".into());
            workspace.worktree = true;
            workspace.tabs[0].name = Some("server".into());
            saved.projects[0].workspaces.push(workspace);

            save(&path, &saved).expect("save");

            assert_eq!(load(&path), Some(saved));
        }

        #[test]
        fn round_trips_the_conversation_of_an_agent_pane() {
            let tmp = TempDir::new();
            let path = tmp.path().join("session.json");
            let mut saved = state(&["/a"]);
            let agent = AgentState { kind: "claude".into(), conversation: "4f2c-91".into(), mode: Some("plan".into()) };
            saved.projects[0].workspaces[0].tabs[0].panes[0].agent = Some(agent);

            save(&path, &saved).expect("save");

            assert_eq!(load(&path), Some(saved));
        }

        #[test]
        fn round_trips_groups() {
            let tmp = TempDir::new();
            let path = tmp.path().join("session.json");
            let mut saved = state(&["/a", "/b"]);
            saved.groups = vec![
                GroupEntry { name: "work".into(), icon: '●', colour: 4, collapsed: false },
                GroupEntry { name: "oss".into(), icon: '★', colour: 99, collapsed: true },
            ];
            saved.projects[1].group = Some(1);

            save(&path, &saved).expect("save");

            assert_eq!(load(&path), Some(saved));
        }

        #[test]
        fn round_trips_what_is_folded() {
            let tmp = TempDir::new();
            let path = tmp.path().join("session.json");
            let mut saved = state(&["/a", "/b"]);
            saved.projects[1].collapsed = true;
            saved.projects[0].workspaces[0].collapsed = true;

            save(&path, &saved).expect("save");

            assert_eq!(load(&path), Some(saved));
        }

        #[test]
        fn version_4_files_open_with_only_the_active_project_unfolded() {
            let tmp = TempDir::new();
            let path = tmp.path().join("session.json");
            let v4 = r#"{"version":4,"projects":[{"path":"/a","workspaces":[{"path":"/a","tabs":[{"panes":[{"cwd":"/a"}]}]}]},
                {"path":"/b","workspaces":[{"path":"/b","tabs":[{"panes":[{"cwd":"/b"}]}]}]}],"active":1}"#;
            std::fs::write(&path, v4).expect("write");

            let collapsed: Vec<bool> = load(&path).expect("load").projects.iter().map(|p| p.collapsed).collect();

            assert_eq!(collapsed, [true, false]);
        }

        #[test]
        fn version_3_files_load_with_every_project_ungrouped() {
            let tmp = TempDir::new();
            let path = tmp.path().join("session.json");
            let v3 = r#"{"version":3,"projects":[{"path":"/a","workspaces":[{"path":"/a","tabs":[{"panes":[{"cwd":"/a"}]}]}]},
                {"path":"/b","workspaces":[{"path":"/b","tabs":[{"panes":[{"cwd":"/b"}]}]}]}],"active":1}"#;
            std::fs::write(&path, v3).expect("write");

            assert_eq!(load(&path), Some(folded(State { active: 1, ..state(&["/a", "/b"]) })));
        }

        #[test]
        fn version_2_files_become_one_project_per_workspace() {
            let tmp = TempDir::new();
            let path = tmp.path().join("session.json");
            let v2 = r#"{"version":2,"workspaces":[{"path":"/a","terminals":[{"cwd":"/a"}]},
                {"path":"/b","name":"api","terminals":[{"cwd":"/b"},{"cwd":"/b/src"}],"active":1}],"active":1}"#;
            std::fs::write(&path, v2).expect("write");

            let mut expected = state(&["/a", "/b"]);
            expected.projects[1].name = Some("api".into());
            expected.projects[1].workspaces[0] =
                WorkspaceState::plain("/b".into(), vec![Some("/b".into()), Some("/b/src".into())], 1);
            expected.active = 1;
            assert_eq!(load(&path), Some(folded(expected)));
        }

        #[test]
        fn version_1_files_become_one_project_per_terminal() {
            let tmp = TempDir::new();
            let path = tmp.path().join("session.json");
            let v1 = r#"{"version":1,"terminals":[{"cwd":"/a"},{"cwd":null},{"cwd":"/b","name":"api"}],"active":2}"#;
            std::fs::write(&path, v1).expect("write");

            let mut expected = state(&["/a", "/b"]);
            expected.projects[1].name = Some("api".into());
            expected.active = 1;
            assert_eq!(load(&path), Some(folded(expected)));
        }

        #[test]
        fn missing_file_loads_nothing() {
            let tmp = TempDir::new();
            assert_eq!(load(&tmp.path().join("session.json")), None);
        }

        #[test]
        fn corrupt_file_loads_nothing() {
            let tmp = TempDir::new();
            let path = tmp.path().join("session.json");
            std::fs::write(&path, "{ not json").expect("write");

            assert_eq!(load(&path), None);
        }

        #[test]
        fn other_versions_load_nothing() {
            let tmp = TempDir::new();
            let path = tmp.path().join("session.json");
            save(&path, &State { version: VERSION + 1, ..state(&["/a"]) }).expect("save");

            assert_eq!(load(&path), None);
        }
    }

    mod saver {
        use super::*;

        fn saver(tmp: &TempDir) -> (Saver, PathBuf) {
            let path = tmp.path().join("session.json");
            (Saver::new(path.clone(), None), path)
        }

        #[test]
        fn waits_for_the_state_to_settle() {
            let tmp = TempDir::new();
            let (mut saver, path) = saver(&tmp);
            let t0 = Instant::now();

            saver.observe(state(&["/a"]), t0).expect("observe");
            saver.observe(state(&["/a"]), t0 + SETTLE / 2).expect("observe");

            assert_eq!(load(&path), None);
        }

        #[test]
        fn saves_a_settled_state() {
            let tmp = TempDir::new();
            let (mut saver, path) = saver(&tmp);
            let t0 = Instant::now();

            let first = saver.observe(state(&["/a"]), t0).expect("observe");
            let settled = saver.observe(state(&["/a"]), t0 + SETTLE).expect("observe");

            assert_eq!((first, settled), (false, true));
            assert_eq!(load(&path), Some(state(&["/a"])));
        }

        #[test]
        fn a_change_restarts_the_wait() {
            let tmp = TempDir::new();
            let (mut saver, path) = saver(&tmp);
            let t0 = Instant::now();

            saver.observe(state(&["/a"]), t0).expect("observe");
            saver.observe(state(&[]), t0 + SETTLE / 2).expect("observe");
            saver.observe(state(&[]), t0 + SETTLE).expect("observe");

            assert_eq!(load(&path), None);
        }

        #[test]
        fn does_not_rewrite_an_unchanged_state() {
            let tmp = TempDir::new();
            let path = tmp.path().join("session.json");
            let mut saver = Saver::new(path.clone(), Some(state(&["/a"])));
            let t0 = Instant::now();

            saver.observe(state(&["/a"]), t0).expect("observe");
            saver.observe(state(&["/a"]), t0 + SETTLE).expect("observe");

            assert!(!path.exists());
        }

        #[test]
        fn flushing_saves_a_state_that_has_not_settled_yet() {
            let tmp = TempDir::new();
            let (mut saver, path) = saver(&tmp);

            saver.observe(state(&["/a"]), Instant::now()).expect("observe");
            saver.flush().expect("flush");

            assert_eq!(load(&path), Some(state(&["/a"])));
        }
    }

    mod back_up {
        use super::*;

        #[test]
        fn a_second_copy_keeps_the_first_one() {
            let tmp = TempDir::new();
            let path = tmp.path().join("session.json");
            std::fs::write(&path, "first").expect("write the session");
            back_up(&path).expect("first copy");
            std::fs::write(&path, "second").expect("write the session again");

            let backup = back_up(&path).expect("second copy");

            let read = |name: &str| std::fs::read_to_string(tmp.path().join(name)).ok();
            assert_eq!(backup, tmp.path().join("session.json.bak"));
            assert_eq!(
                (read("session.json.bak"), read("session.json.bak.1")),
                (Some("second".into()), Some("first".into()))
            );
        }
    }
}
