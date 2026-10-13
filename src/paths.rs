use std::path::{Path, PathBuf};

pub const SOCKET_ENV: &str = "CORNERCASE_SOCKET";
pub const TODOS_FILE: &str = "todos.json";
const SECRETS_FILE: &str = "secrets.json";
const LOG_FILE: &str = "server.log";
const ISSUE_CACHE_FILE: &str = "issues.json";

pub fn socket() -> PathBuf {
    if let Some(path) = std::env::var_os(SOCKET_ENV) {
        return path.into();
    }
    let dir = std::env::var_os("XDG_RUNTIME_DIR").map_or_else(
        || std::env::temp_dir().join(format!("cornercase-{}", rustix::process::getuid().as_raw())),
        |dir| PathBuf::from(dir).join("cornercase"),
    );
    dir.join("server.sock")
}

pub fn lock(socket: &Path) -> PathBuf {
    socket.with_extension("lock")
}

pub fn config() -> PathBuf {
    if let Some(socket) = std::env::var_os(SOCKET_ENV) {
        return Path::new(&socket).with_file_name("config.json");
    }
    let config_home = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| Path::new(&home).join(".config")))
        .unwrap_or_else(std::env::temp_dir);
    config_home.join("cornercase").join("config.json")
}

pub fn secrets(config_path: &Path) -> PathBuf {
    config_path.with_file_name(SECRETS_FILE)
}

pub fn browsers(config: &Path) -> PathBuf {
    config.with_file_name("browsers.json")
}

pub fn session() -> PathBuf {
    if let Some(socket) = std::env::var_os(SOCKET_ENV) {
        return Path::new(&socket).with_extension("json");
    }
    let state_home = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| Path::new(&home).join(".local").join("state")))
        .unwrap_or_else(std::env::temp_dir);
    state_home.join("cornercase").join("session.json")
}

pub fn todos(state: &Path) -> PathBuf {
    state.with_file_name(TODOS_FILE)
}

pub fn stats(session: &Path) -> PathBuf {
    session.with_file_name("stats.json")
}

pub fn log() -> PathBuf {
    match std::env::var_os(SOCKET_ENV) {
        Some(socket) => Path::new(&socket).with_extension("log"),
        None => session().with_file_name(LOG_FILE),
    }
}

pub fn issue_cache(session: &Path) -> PathBuf {
    session.with_file_name(ISSUE_CACHE_FILE)
}

#[cfg(test)]
mod tests;
