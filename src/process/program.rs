pub(crate) mod shim;

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::os::fs::PermissionsExt;

const RUNNABLE: [&str; 4] = [".com", ".exe", ".bat", ".cmd"];
const SCRIPT: &str = ".ps1";
const POWERSHELLS: [&str; 2] = ["pwsh", "powershell"];

pub(crate) fn extensions(pathext: Option<&str>) -> Vec<String> {
    let listed = pathext.unwrap_or_default().split(';').map(|extension| extension.trim().to_lowercase());
    let runnable: Vec<String> = listed.filter(|extension| RUNNABLE.contains(&extension.as_str())).collect();
    if runnable.is_empty() { RUNNABLE.map(String::from).to_vec() } else { runnable }
}

pub(crate) fn on_path<'a>(
    windows: bool,
    path: Option<&'a OsStr>,
    extensions: &'a [String],
) -> impl Fn(&str) -> Option<PathBuf> + 'a {
    move |name| {
        std::env::split_paths(path?)
            .filter(|dir| dir.is_absolute())
            .find_map(|dir| runnable(windows, &dir.join(name), extensions))
    }
}

pub(crate) fn locate(program: &str) -> Option<PathBuf> {
    find(cfg!(windows), program, std::env::var_os("PATH").as_deref(), &own_extensions())
}

pub(crate) fn command(program: &str) -> Command {
    Command::new(startable(program, std::env::var_os("PATH").as_deref(), &own_extensions()))
}

pub(crate) fn is_powershell(name: &str) -> bool {
    let name = super::program_name(name);
    POWERSHELLS.iter().any(|shell| super::same_file_name(shell, name))
}

pub(crate) fn powershell_runs(program: &str) -> Option<PathBuf> {
    powershell_finds(program, std::env::var_os("PATH").as_deref(), std::env::var("PATHEXT").ok().as_deref())
}

fn powershell_finds(program: &str, path: Option<&OsStr>, pathext: Option<&str>) -> Option<PathBuf> {
    let extensions: Vec<String> = std::iter::once(SCRIPT.to_string()).chain(extensions(pathext)).collect();
    find(true, program, path, &extensions)
}

fn find(windows: bool, program: &str, path: Option<&OsStr>, extensions: &[String]) -> Option<PathBuf> {
    if program.chars().any(std::path::is_separator) {
        return runnable(windows, Path::new(program), extensions);
    }
    on_path(windows, path, extensions)(program)
}

fn startable(program: &str, path: Option<&OsStr>, extensions: &[String]) -> OsString {
    if extensions.is_empty() {
        return program.into();
    }
    find(cfg!(windows), program, path, extensions).map_or_else(|| program.into(), PathBuf::into_os_string)
}

fn own_extensions() -> Vec<String> {
    if cfg!(windows) { extensions(std::env::var("PATHEXT").ok().as_deref()) } else { Vec::new() }
}

fn runnable(windows: bool, path: &Path, extensions: &[String]) -> Option<PathBuf> {
    with_extensions(path, extensions).into_iter().find(|candidate| executable(windows, candidate))
}

fn with_extensions(path: &Path, extensions: &[String]) -> Vec<PathBuf> {
    let extension = path.extension().and_then(OsStr::to_str).map(|ext| format!(".{ext}").to_lowercase());
    if extensions.is_empty() || extension.is_some_and(|ext| extensions.contains(&ext)) {
        return vec![path.to_path_buf()];
    }
    let named = |extension: &String| {
        let mut name = path.as_os_str().to_owned();
        name.push(extension);
        PathBuf::from(name)
    };
    extensions.iter().map(named).collect()
}

fn executable(windows: bool, path: &Path) -> bool {
    path.metadata().is_ok_and(|m| m.is_file() && (windows || m.permissions().mode() & 0o111 != 0))
}

#[cfg(test)]
mod tests;
