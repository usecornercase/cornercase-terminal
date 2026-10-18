use std::ffi::OsString;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::text::hex;

pub const SOCKET_ENV: &str = "CORNERCASE_SOCKET";
pub(crate) const TODOS_FILE: &str = "todos.json";
const SECRETS_FILE: &str = "secrets.json";
const LOG_FILE: &str = "server.log";
const ISSUE_CACHE_FILE: &str = "issues.json";
const SOURCE_FILE: &str = "source";
const SOCKET_FILE: &str = "server.sock";
const RELEASE: &str = "cornercase";
const DEV: &str = "cornercase-dev";

type Var<'a> = &'a dyn Fn(&str) -> Option<OsString>;

fn process_var(name: &str) -> Option<OsString> {
    std::env::var_os(name)
}

fn this_tree() -> Option<&'static str> {
    cfg!(debug_assertions).then_some(env!("CARGO_MANIFEST_DIR"))
}

fn this_key() -> Option<String> {
    this_tree().map(dev_key)
}

fn dev_key(tree: &str) -> String {
    hex(&Sha256::digest(tree.as_bytes())[..4])
}

pub(crate) fn dev_source() -> Option<&'static str> {
    this_tree().filter(|_| custom_socket().is_none())
}

pub(crate) fn custom_socket() -> Option<PathBuf> {
    custom(&process_var)
}

fn custom(var: Var) -> Option<PathBuf> {
    var(SOCKET_ENV).map(PathBuf::from)
}

fn absolute(value: Option<OsString>) -> Option<PathBuf> {
    value.map(PathBuf::from).filter(|path| path.is_absolute())
}

fn user_file(var: Var, home_var: &str, fallback: &[&str], folder: &Path, name: &str) -> PathBuf {
    let base = absolute(var(home_var))
        .or_else(|| absolute(var("HOME")).map(|home| fallback.iter().fold(home, |p, s| p.join(s))))
        .unwrap_or_else(std::env::temp_dir);
    base.join(folder).join(name)
}

fn state_folder(dev: Option<&str>) -> PathBuf {
    dev.map_or_else(|| PathBuf::from(RELEASE), |key| Path::new(DEV).join(key))
}

pub(crate) fn socket() -> PathBuf {
    socket_in(&process_var, this_key().as_deref())
}

fn socket_in(var: Var, dev: Option<&str>) -> PathBuf {
    match custom(var) {
        Some(path) => path,
        None => session_in(var, dev).with_file_name(SOCKET_FILE),
    }
}

pub(crate) fn earlier_sockets() -> Vec<PathBuf> {
    if custom_socket().is_some() || this_tree().is_some() {
        return Vec::new();
    }
    earlier_sockets_from(
        &process_var,
        rustix::process::getuid().as_raw(),
        &std::env::temp_dir(),
        cfg!(target_os = "linux"),
    )
}

fn earlier_sockets_from(var: Var, uid: u32, temp: &Path, linux: bool) -> Vec<PathBuf> {
    let runtime = absolute(var("XDG_RUNTIME_DIR")).or_else(|| linux.then(|| PathBuf::from(format!("/run/user/{uid}"))));
    runtime
        .map(|dir| dir.join(RELEASE).join(SOCKET_FILE))
        .into_iter()
        .chain([temp.join(format!("{RELEASE}-{uid}")).join(SOCKET_FILE)])
        .collect()
}

pub(crate) fn lock(socket: &Path) -> PathBuf {
    socket.with_extension("lock")
}

pub(crate) fn config() -> PathBuf {
    config_in(&process_var, this_key().as_deref())
}

fn config_in(var: Var, dev: Option<&str>) -> PathBuf {
    if let Some(socket) = custom(var) {
        return socket.with_file_name("config.json");
    }
    let folder = if dev.is_some() { DEV } else { RELEASE };
    user_file(var, "XDG_CONFIG_HOME", &[".config"], Path::new(folder), "config.json")
}

pub(crate) fn secrets(config_path: &Path) -> PathBuf {
    config_path.with_file_name(SECRETS_FILE)
}

pub(crate) fn flows(config: &Path) -> PathBuf {
    config.with_file_name("flows")
}

pub(crate) fn browsers(config: &Path) -> PathBuf {
    config.with_file_name("browsers.json")
}

pub(crate) fn telegram(config: &Path) -> PathBuf {
    config.with_file_name("telegram.json")
}

pub(crate) fn plugins(config: &Path) -> PathBuf {
    config.with_file_name("plugins.json")
}

pub(crate) fn plugin_config(config: &Path, id: &str) -> PathBuf {
    config.with_file_name("plugins").join("config").join(id)
}

pub(crate) fn plugin_state(session: &Path, id: &str) -> PathBuf {
    session.with_file_name("plugins").join("state").join(id)
}

pub(crate) fn discord(config: &Path) -> PathBuf {
    config.with_file_name("discord.json")
}

pub(crate) fn plugin_installs(session: &Path) -> PathBuf {
    session.with_file_name("plugins").join("installed")
}

pub(crate) fn session() -> PathBuf {
    session_in(&process_var, this_key().as_deref())
}

fn session_in(var: Var, dev: Option<&str>) -> PathBuf {
    match custom(var) {
        Some(socket) => socket.with_extension("json"),
        None => user_file(var, "XDG_STATE_HOME", &[".local", "state"], &state_folder(dev), "session.json"),
    }
}

pub(crate) fn source(session: &Path) -> PathBuf {
    session.with_file_name(SOURCE_FILE)
}

pub(crate) fn todos(state: &Path) -> PathBuf {
    state.with_file_name(TODOS_FILE)
}

pub(crate) fn queue(session: &Path) -> PathBuf {
    if session.file_name().is_some_and(|name| name == "session.json") {
        session.with_file_name("queue.json")
    } else {
        session.with_extension("queue.json")
    }
}

pub(crate) fn runs(session: &Path) -> PathBuf {
    if session.file_name().is_some_and(|name| name == "session.json") {
        session.with_file_name("runs")
    } else {
        session.with_extension("runs")
    }
}

pub(crate) fn stats(session: &Path) -> PathBuf {
    session.with_file_name("stats.json")
}

pub(crate) fn star(session: &Path) -> PathBuf {
    session.with_file_name("star.json")
}

pub(crate) fn log() -> PathBuf {
    log_in(&process_var, this_key().as_deref())
}

fn log_in(var: Var, dev: Option<&str>) -> PathBuf {
    match custom(var) {
        Some(socket) => socket.with_extension("log"),
        None => session_in(var, dev).with_file_name(LOG_FILE),
    }
}

pub(crate) fn issue_cache(session: &Path) -> PathBuf {
    session.with_file_name(ISSUE_CACHE_FILE)
}

#[cfg(test)]
mod tests;
