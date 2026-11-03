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
    const PROGRAMS: [&str; 2] = ["exe", "com"];

    pub(crate) fn default() -> String {
        let path = std::env::var_os("PATH");
        let given = std::env::var("SHELL").ok().filter(|shell| !unquoted(shell).is_empty());
        let started = given.as_deref().and_then(|shell| startable(shell, path.as_deref()));
        if let (Some(shell), None) = (&given, &started) {
            log::warning!("server", "SHELL cannot be started on Windows", shell = shell);
        }
        started.unwrap_or_else(|| fallback(path.as_deref(), std::env::var("ComSpec").ok().as_deref()))
    }

    pub(super) fn startable(shell: &str, path: Option<&OsStr>) -> Option<String> {
        let shell = unquoted(shell);
        let given = Path::new(shell);
        let found = if given.is_absolute() {
            program(given)
        } else if shell.contains(['/', '\\', ':']) {
            None
        } else {
            on_path(shell, path)
        };
        found.map(|found| found.to_string_lossy().into_owned())
    }

    pub(super) fn fallback(path: Option<&OsStr>, comspec: Option<&str>) -> String {
        let interpreter = || comspec.map(unquoted).and_then(|comspec| program(Path::new(comspec)));
        SHELLS
            .iter()
            .find_map(|name| on_path(name, path))
            .or_else(interpreter)
            .map_or_else(|| CMD.to_string(), |found| found.to_string_lossy().into_owned())
    }

    fn unquoted(text: &str) -> &str {
        let text = text.trim();
        text.strip_prefix('"').and_then(|text| text.strip_suffix('"')).unwrap_or(text)
    }

    fn on_path(name: &str, path: Option<&OsStr>) -> Option<PathBuf> {
        std::env::split_paths(path?).filter(|dir| dir.is_absolute()).find_map(|dir| program(&dir.join(name)))
    }

    fn program(path: &Path) -> Option<PathBuf> {
        let started =
            path.extension().is_some_and(|ext| PROGRAMS.iter().any(|program| ext.eq_ignore_ascii_case(program)));
        let candidate = if started {
            path.to_path_buf()
        } else {
            let mut named: OsString = path.as_os_str().to_owned();
            named.push(EXE);
            PathBuf::from(named)
        };
        runnable(&candidate).then_some(candidate)
    }

    fn runnable(path: &Path) -> bool {
        std::fs::metadata(path).or_else(|_| std::fs::symlink_metadata(path)).is_ok_and(|meta| meta.is_file())
    }
}

#[cfg(test)]
mod tests;
