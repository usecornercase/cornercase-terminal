pub(crate) use imp::default;

#[cfg(unix)]
mod imp {
    pub(crate) fn default() -> String {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into())
    }
}

#[cfg(windows)]
mod imp {
    use std::ffi::{OsStr, OsString};
    use std::path::{Path, PathBuf};

    use crate::log;

    const SHELLS: [&str; 2] = ["pwsh.exe", "powershell.exe"];
    const CMD: &str = "cmd.exe";
    const EXE: &str = ".exe";

    pub(crate) fn default() -> String {
        let path = std::env::var_os("PATH");
        let given = std::env::var("SHELL").ok().filter(|shell| !shell.is_empty());
        let started = given.as_deref().and_then(|shell| startable(shell, path.as_deref()));
        if let (Some(shell), None) = (&given, &started) {
            log::warning!("server", "SHELL cannot be started on Windows", shell = shell);
        }
        started.unwrap_or_else(|| fallback(path.as_deref(), std::env::var("ComSpec").ok()))
    }

    pub(super) fn startable(shell: &str, path: Option<&OsStr>) -> Option<String> {
        let given = Path::new(shell);
        let found = if given.is_absolute() {
            with_exe(given)
        } else if shell.contains(['/', '\\', ':']) {
            None
        } else {
            on_path(shell, path)
        };
        found.map(|found| found.to_string_lossy().into_owned())
    }

    pub(super) fn fallback(path: Option<&OsStr>, comspec: Option<String>) -> String {
        SHELLS
            .iter()
            .find_map(|name| on_path(name, path))
            .map(|found| found.to_string_lossy().into_owned())
            .or(comspec.filter(|comspec| !comspec.is_empty()))
            .unwrap_or_else(|| CMD.to_string())
    }

    fn on_path(name: &str, path: Option<&OsStr>) -> Option<PathBuf> {
        std::env::split_paths(path?).filter(|dir| dir.is_absolute()).find_map(|dir| with_exe(&dir.join(name)))
    }

    fn with_exe(path: &Path) -> Option<PathBuf> {
        let mut named: OsString = path.as_os_str().to_owned();
        named.push(EXE);
        [path.to_path_buf(), PathBuf::from(named)].into_iter().find(|candidate| runnable(candidate))
    }

    fn runnable(path: &Path) -> bool {
        std::fs::metadata(path).or_else(|_| std::fs::symlink_metadata(path)).is_ok_and(|meta| meta.is_file())
    }
}

#[cfg(test)]
mod tests;
