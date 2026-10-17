use std::path::{Path, PathBuf};

pub const SOCKET_ENV: &str = "CORNERCASE_SOCKET";
pub(crate) const TODOS_FILE: &str = "todos.json";
const SECRETS_FILE: &str = "secrets.json";
const LOG_FILE: &str = "server.log";
const ISSUE_CACHE_FILE: &str = "issues.json";

pub(crate) fn custom_socket() -> Option<PathBuf> {
    std::env::var_os(SOCKET_ENV).map(PathBuf::from)
}

fn user_file(home_var: &str, fallback: &[&str], name: &str) -> PathBuf {
    let base = std::env::var_os(home_var)
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| fallback.iter().fold(PathBuf::from(home), |p, s| p.join(s))))
        .unwrap_or_else(std::env::temp_dir);
    base.join("cornercase").join(name)
}

pub(crate) fn socket() -> PathBuf {
    if let Some(path) = custom_socket() {
        return path;
    }
    let dir = std::env::var_os("XDG_RUNTIME_DIR").map_or_else(
        || std::env::temp_dir().join(format!("cornercase-{}", rustix::process::getuid().as_raw())),
        |dir| PathBuf::from(dir).join("cornercase"),
    );
    dir.join("server.sock")
}

pub(crate) fn lock(socket: &Path) -> PathBuf {
    socket.with_extension("lock")
}

pub(crate) fn config() -> PathBuf {
    match custom_socket() {
        Some(socket) => socket.with_file_name("config.json"),
        None => user_file("XDG_CONFIG_HOME", &[".config"], "config.json"),
    }
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
    match custom_socket() {
        Some(socket) => socket.with_extension("json"),
        None => user_file("XDG_STATE_HOME", &[".local", "state"], "session.json"),
    }
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
    match custom_socket() {
        Some(socket) => socket.with_extension("log"),
        None => session().with_file_name(LOG_FILE),
    }
}

pub(crate) fn issue_cache(session: &Path) -> PathBuf {
    session.with_file_name(ISSUE_CACHE_FILE)
}

#[cfg(test)]
mod tests;
