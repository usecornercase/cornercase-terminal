use std::path::Path;

use crate::config;
use crate::text;

pub(crate) use imp::{default, found};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Found {
    pub(crate) value: String,
    pub(crate) note: String,
}

pub(crate) fn resolve(program: &str, home: Option<&Path>) -> Result<String, String> {
    let program = unquoted(program);
    if program.is_empty() {
        return Err("no program was given".into());
    }
    if program.chars().any(text::hidden) {
        return Err("it holds a character that cannot be seen".into());
    }
    imp::resolve(&config::expand_home(program, home).to_string_lossy())
}

fn unquoted(text: &str) -> &str {
    let text = text.trim();
    text.strip_prefix('"').and_then(|text| text.strip_suffix('"')).unwrap_or(text)
}

#[cfg(unix)]
mod imp {
    use std::ffi::OsStr;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};

    use super::Found;
    use crate::process;
    use crate::process::program;

    const SHELLS_FILE: &str = "/etc/shells";
    const NOT_FOUND: &str = "it was not found on the PATH";
    const NO_PROGRAM: &str = "there is no program there";
    const RELATIVE: &str = "use a full path, one starting with ~/, or a name on the PATH";

    pub(crate) fn default() -> String {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into())
    }

    pub(crate) fn found() -> Vec<Found> {
        listed(&std::fs::read_to_string(SHELLS_FILE).unwrap_or_default())
    }

    pub(super) fn listed(text: &str) -> Vec<Found> {
        let mut seen: Vec<PathBuf> = Vec::new();
        let mut shells = Vec::new();
        for line in text.lines().map(str::trim).filter(|line| !line.is_empty() && !line.starts_with('#')) {
            let path = Path::new(line);
            let name = path.file_name().and_then(OsStr::to_str).unwrap_or_default();
            if !path.is_absolute() || !executable(path) || !process::is_shell(name) {
                continue;
            }
            let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
            if !seen.contains(&canonical) {
                seen.push(canonical);
                shells.push(Found { value: line.to_string(), note: name.to_string() });
            }
        }
        shells
    }

    pub(super) fn resolve(program: &str) -> Result<String, String> {
        resolve_on(program, std::env::var_os("PATH").as_deref())
    }

    pub(super) fn resolve_on(program: &str, path: Option<&OsStr>) -> Result<String, String> {
        if !program.contains('/') {
            let found = program::on_path(false, path, &[])(program);
            return found.map(|found| found.to_string_lossy().into_owned()).ok_or_else(|| NOT_FOUND.into());
        }
        let given = Path::new(program);
        if !given.is_absolute() {
            return Err(RELATIVE.into());
        }
        if executable(given) { Ok(program.to_string()) } else { Err(NO_PROGRAM.into()) }
    }

    fn executable(path: &Path) -> bool {
        path.metadata().is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
    }
}

#[cfg(windows)]
mod imp {
    use std::ffi::{OsStr, OsString};
    use std::path::{Path, PathBuf};

    use super::{Found, unquoted};
    use crate::log;
    use crate::os::fs::reaches_network;
    use crate::script;

    const SHELLS: [&str; 2] = ["pwsh.exe", "powershell.exe"];
    const CMD: &str = "cmd.exe";
    const EXE: &str = ".exe";
    const PROGRAMS: [&str; 2] = ["exe", "com"];
    const GIT_BASH: &str = r"bin\bash.exe";
    const RELATIVE: &str = "use a full path or a name on the PATH";
    const NETWORK: &str = "it is on the network, which would hold up every new pane";
    const NOT_EXE: &str = "Windows starts only an .exe or a .com";
    const WSL: &str = "WSL runs Linux, whose folders and programs cornercase cannot follow from Windows";

    pub(super) struct Env<'a> {
        pub(super) path: Option<&'a OsStr>,
        pub(super) comspec: Option<&'a str>,
        pub(super) program_files: Option<&'a OsStr>,
        pub(super) local_app_data: Option<&'a OsStr>,
    }

    pub(crate) fn default() -> String {
        let path = std::env::var_os("PATH");
        let given = std::env::var("SHELL").ok().filter(|shell| !unquoted(shell).is_empty());
        let started = given.as_deref().and_then(|shell| startable(shell, path.as_deref()));
        if let (Some(shell), None) = (&given, &started) {
            log::warning!("server", "SHELL cannot be started on Windows", shell = shell);
        }
        started.unwrap_or_else(|| fallback(path.as_deref(), std::env::var("ComSpec").ok().as_deref()))
    }

    pub(crate) fn found() -> Vec<Found> {
        let path = std::env::var_os("PATH");
        let comspec = std::env::var("ComSpec").ok();
        let program_files = std::env::var_os("ProgramFiles");
        let local_app_data = std::env::var_os("LOCALAPPDATA");
        found_in(&Env {
            path: path.as_deref(),
            comspec: comspec.as_deref(),
            program_files: program_files.as_deref(),
            local_app_data: local_app_data.as_deref(),
        })
    }

    pub(super) fn found_in(env: &Env) -> Vec<Found> {
        let named = |value: &str, label: &str, program: &str| {
            on_path(program, env.path)
                .map(|full| Found { value: value.to_string(), note: format!("{label} · {}", full.to_string_lossy()) })
        };
        let cmd = || {
            let full = on_path(CMD, env.path).or_else(|| program(Path::new(unquoted(env.comspec?))))?;
            let value =
                if on_path(CMD, env.path).is_some() { "cmd".to_string() } else { full.to_string_lossy().into_owned() };
            Some(Found { value, note: format!("Command Prompt · {}", full.to_string_lossy()) })
        };
        [named("pwsh", "PowerShell 7", SHELLS[0]), named("powershell", "Windows PowerShell", SHELLS[1]), cmd()]
            .into_iter()
            .flatten()
            .chain(git_bash(env))
            .collect()
    }

    fn git_bash(env: &Env) -> Option<Found> {
        let from_git = on_path("git", env.path).and_then(|git| script::git_root(&git));
        let usual = |base: Option<&OsStr>, below: &str| {
            let root = Path::new(base?).join(below);
            root.join(script::GIT_MARK).is_file().then_some(root)
        };
        let roots = [from_git, usual(env.program_files, "Git"), usual(env.local_app_data, r"Programs\Git")];
        roots.into_iter().flatten().find_map(|root| {
            let bash = root.join(GIT_BASH);
            runnable(&bash).then(|| Found { value: bash.to_string_lossy().into_owned(), note: "Git Bash".into() })
        })
    }

    pub(super) fn resolve(program: &str) -> Result<String, String> {
        let system = crate::os::desktop::windows_folder();
        resolve_on(program, std::env::var_os("PATH").as_deref(), system.as_deref())
    }

    pub(super) fn resolve_on(program: &str, path: Option<&OsStr>, system: Option<&Path>) -> Result<String, String> {
        if reaches_network(program) {
            return Err(NETWORK.into());
        }
        let found = startable(program, path).ok_or_else(|| why_not(program))?;
        if is_wsl(Path::new(&found), system) { Err(WSL.into()) } else { Ok(found) }
    }

    fn why_not(program: &str) -> String {
        let given = Path::new(program);
        if !given.is_absolute() && program.contains(['/', '\\', ':']) {
            RELATIVE.into()
        } else if given.is_absolute() && runnable(given) && !program_extension(given) {
            NOT_EXE.into()
        } else if given.is_absolute() {
            "there is no program there".into()
        } else {
            "it was not found on the PATH".into()
        }
    }

    fn program_extension(path: &Path) -> bool {
        path.extension().is_some_and(|ext| PROGRAMS.iter().any(|program| ext.eq_ignore_ascii_case(program)))
    }

    fn lexical(path: &Path) -> PathBuf {
        let mut kept = PathBuf::new();
        for part in path.components() {
            match part {
                std::path::Component::ParentDir => {
                    kept.pop();
                }
                std::path::Component::CurDir => {}
                other => kept.push(other),
            }
        }
        kept
    }

    fn is_wsl(found: &Path, system: Option<&Path>) -> bool {
        let found = lexical(found);
        let found = found.as_path();
        let stem = found.file_stem().map(|stem| stem.to_string_lossy().to_lowercase());
        match stem.as_deref() {
            Some("wsl") => true,
            Some("bash") => {
                let own = found.parent().map(|dir| dir.to_string_lossy().to_lowercase());
                let wsl = system.map(|system| lexical(&system.join("System32")).to_string_lossy().to_lowercase());
                own.is_some() && own == wsl
            }
            _ => false,
        }
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

    fn on_path(name: &str, path: Option<&OsStr>) -> Option<PathBuf> {
        std::env::split_paths(path?)
            .filter(|dir| dir.is_absolute() && !reaches_network(&dir.to_string_lossy()))
            .find_map(|dir| program(&dir.join(name)))
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
