use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::agents;
use crate::notify;
use crate::protocol;
use crate::state;
use crate::ui;

pub const DEFAULT_WORKTREES_DIR: &str = "~/.cornercase/worktrees";
pub const DEFAULT_PROMPT: &str = "{url}";
pub const DEFAULT_GH: &str = "gh";
pub const DEFAULT_ISSUE_TABS: [&str; 4] = ["all", "github", "shortcut", "linear"];
pub const DEFAULT_FETCH_MINUTES: u64 = 5;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
#[expect(clippy::struct_excessive_bools, reason = "each switch is its own key in config.json")]
pub struct Config {
    pub worktrees_dir: String,
    pub fetch_minutes: u64,
    pub issue_tabs: Vec<String>,
    pub agent: String,
    pub agent_args: BTreeMap<String, Vec<String>>,
    pub agent_modes: BTreeMap<String, BTreeMap<String, Vec<String>>>,
    pub agent_commands: BTreeMap<String, String>,
    pub prompt: String,
    pub submit: bool,
    pub auto_accept_trust_prompt: bool,
    pub trust_prompt_pattern: String,
    pub gh: String,
    pub sidebar: String,
    pub dim_inactive_panes: bool,
    pub model: bool,
    pub context: bool,
    pub memory: bool,
    pub desktop_notifications: String,
    pub check_updates: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            worktrees_dir: DEFAULT_WORKTREES_DIR.into(),
            fetch_minutes: DEFAULT_FETCH_MINUTES,
            issue_tabs: DEFAULT_ISSUE_TABS.map(String::from).to_vec(),
            agent: agents::AUTO.into(),
            agent_args: BTreeMap::new(),
            agent_modes: BTreeMap::new(),
            agent_commands: BTreeMap::new(),
            prompt: DEFAULT_PROMPT.into(),
            submit: false,
            auto_accept_trust_prompt: true,
            trust_prompt_pattern: agents::DEFAULT_TRUST_PROMPT.into(),
            gh: DEFAULT_GH.into(),
            sidebar: ui::Sidebar::default().id().into(),
            dim_inactive_panes: true,
            model: true,
            context: true,
            memory: false,
            desktop_notifications: notify::AUTO.into(),
            check_updates: true,
        }
    }
}

impl Config {
    pub fn worktrees_dir(&self, home: Option<&Path>) -> PathBuf {
        expand_home(&self.worktrees_dir, home)
    }

    pub fn gh(&self, home: Option<&Path>) -> PathBuf {
        expand_home(&self.gh, home)
    }

    pub fn fetch_every(&self) -> Option<Duration> {
        (self.fetch_minutes > 0).then(|| Duration::from_secs(self.fetch_minutes.saturating_mul(60)))
    }
}

pub fn path() -> PathBuf {
    if let Some(socket) = std::env::var_os(protocol::SOCKET_ENV) {
        return Path::new(&socket).with_file_name("config.json");
    }
    let config_home = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| Path::new(&home).join(".config")))
        .unwrap_or_else(std::env::temp_dir);
    config_home.join("cornercase").join("config.json")
}

pub fn load(path: &Path) -> Config {
    let Some(mut value) = std::fs::read_to_string(path).ok().and_then(|text| serde_json::from_str(&text).ok()) else {
        return Config::default();
    };
    if let Value::Object(keys) = &mut value {
        migrate(keys);
    }
    serde_json::from_value(value).unwrap_or_default()
}

fn migrate(keys: &mut Map<String, Value>) {
    if let Some(old) = keys.remove("context_line") {
        keys.entry("context").or_insert(old);
    }
    if let Some(context) = keys.get("context").cloned() {
        keys.entry("model").or_insert(context);
    }
}

pub fn save(path: &Path, config: &Config) -> io::Result<()> {
    state::save(path, config)
}

pub fn expand_home(path: &str, home: Option<&Path>) -> PathBuf {
    match (path.strip_prefix('~'), home) {
        (Some(""), Some(home)) => home.to_path_buf(),
        (Some(rest), Some(home)) if rest.starts_with('/') => home.join(rest.trim_start_matches('/')),
        _ => PathBuf::from(path),
    }
}

pub fn check_worktrees_dir(input: &str, home: Option<&Path>) -> Result<String, &'static str> {
    let dir = input.trim();
    if dir.is_empty() {
        return Err("the folder is required");
    }
    if !expand_home(dir, home).is_absolute() {
        return Err("use an absolute path or one starting with ~/");
    }
    Ok(dir.to_string())
}

pub fn check_fetch_minutes(input: &str) -> Result<u64, &'static str> {
    input.trim().parse().map_err(|_| "use a whole number of minutes, 0 turns it off")
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::test_util::TempDir;

    mod file {
        use super::*;

        #[test]
        fn round_trips() {
            let tmp = TempDir::new();
            let path = tmp.path().join("nested").join("config.json");
            let config = Config { worktrees_dir: "/srv/worktrees".into(), ..Config::default() };

            save(&path, &config).expect("save");

            assert_eq!(load(&path), config);
        }

        #[test]
        fn missing_file_loads_the_defaults() {
            let tmp = TempDir::new();
            assert_eq!(load(&tmp.path().join("config.json")), Config::default());
        }

        #[test]
        fn corrupt_file_loads_the_defaults() {
            let tmp = TempDir::new();
            let path = tmp.path().join("config.json");
            std::fs::write(&path, "{ not json").expect("write");

            assert_eq!(load(&path), Config::default());
        }

        #[test]
        fn missing_keys_take_their_defaults() {
            let tmp = TempDir::new();
            let path = tmp.path().join("config.json");
            std::fs::write(&path, r#"{"worktrees_dir": "/srv/w"}"#).expect("write");

            assert_eq!(load(&path), Config { worktrees_dir: "/srv/w".into(), ..Config::default() });
        }

        #[test]
        fn the_sidebar_starts_with_projects_on_top() {
            assert_eq!(ui::Sidebar::from_setting(&Config::default().sidebar), ui::Sidebar::ProjectsOnTop);
        }

        #[test]
        fn an_agent_tab_starts_with_its_model_and_context_but_not_its_memory() {
            let c = Config::default();
            assert_eq!((c.model, c.context, c.memory), (true, true, false));
        }

        #[rstest]
        #[case::nothing_set("{}", (true, true))]
        #[case::the_switch_of_0_6_off(r#"{"context_line": false}"#, (false, false))]
        #[case::the_context_off(r#"{"context": false}"#, (false, false))]
        #[case::the_model_set_apart(r#"{"context": false, "model": true}"#, (true, false))]
        #[case::the_context_set_apart(r#"{"model": false}"#, (false, true))]
        #[case::the_new_key_wins(r#"{"context_line": false, "context": true}"#, (true, true))]
        fn a_missing_model_follows_the_context(#[case] file: &str, #[case] expected: (bool, bool)) {
            let tmp = TempDir::new();
            let path = tmp.path().join("config.json");
            std::fs::write(&path, file).expect("write");

            let config = load(&path);

            assert_eq!((config.model, config.context), expected);
        }
    }

    mod expand_home {
        use super::*;

        #[rstest]
        #[case::tilde_alone("~", "/home/ana")]
        #[case::tilde_slash("~/.cornercase/worktrees", "/home/ana/.cornercase/worktrees")]
        #[case::absolute("/srv/worktrees", "/srv/worktrees")]
        #[case::other_users_home("~bob/x", "~bob/x")]
        fn replaces_a_leading_tilde(#[case] input: &str, #[case] expected: &str) {
            assert_eq!(expand_home(input, Some(Path::new("/home/ana"))), PathBuf::from(expected));
        }

        #[test]
        fn keeps_the_tilde_without_a_home() {
            assert_eq!(expand_home("~/x", None), PathBuf::from("~/x"));
        }
    }

    mod check_worktrees_dir {
        use super::*;

        #[rstest]
        #[case::tilde(" ~/worktrees ", Ok("~/worktrees".to_string()))]
        #[case::absolute("/srv/worktrees", Ok("/srv/worktrees".to_string()))]
        #[case::empty("  ", Err("the folder is required"))]
        #[case::relative("worktrees", Err("use an absolute path or one starting with ~/"))]
        fn accepts_only_absolute_folders(#[case] input: &str, #[case] expected: Result<String, &'static str>) {
            assert_eq!(check_worktrees_dir(input, Some(Path::new("/home/ana"))), expected);
        }
    }

    mod check_fetch_minutes {
        use super::*;

        #[rstest]
        #[case::minutes(" 15 ", Ok(15))]
        #[case::off("0", Ok(0))]
        #[case::empty("", Err("use a whole number of minutes, 0 turns it off"))]
        #[case::negative("-1", Err("use a whole number of minutes, 0 turns it off"))]
        #[case::fraction("1.5", Err("use a whole number of minutes, 0 turns it off"))]
        fn accepts_whole_minutes(#[case] input: &str, #[case] expected: Result<u64, &'static str>) {
            assert_eq!(check_fetch_minutes(input), expected);
        }
    }

    mod fetch_every {
        use super::*;

        #[rstest]
        #[case::default(DEFAULT_FETCH_MINUTES, Some(Duration::from_secs(300)))]
        #[case::off(0, None)]
        fn is_the_interval_in_minutes(#[case] fetch_minutes: u64, #[case] expected: Option<Duration>) {
            assert_eq!(Config { fetch_minutes, ..Config::default() }.fetch_every(), expected);
        }
    }
}
