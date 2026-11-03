use std::ffi::OsString;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;

use crate::os::ffi;
use crate::process::{self, program};

const SH: &str = "/bin/sh";
const GIT_SHELLS: [&str; 2] = [r"usr\bin\sh.exe", r"bin\sh.exe"];
const GIT_LEVELS: usize = 4;
const EXEC_PATH_WITHIN: Duration = Duration::from_secs(5);
const SCRIPT_ENV: &str = "CORNERCASE_SCRIPT";
const ARGS_ENV: &str = "CORNERCASE_ARGS";
const ADAPTER: &str = r#"eval "set -- $CORNERCASE_ARGS"; cornercase_script=$CORNERCASE_SCRIPT; unset CORNERCASE_ARGS CORNERCASE_SCRIPT; eval "$cornercase_script""#;
const EXEC: &str = r#"exec "$@""#;
const PWSH: &str = "pwsh";
const WINDOWS_POWERSHELL: &str = "powershell.exe";
const STRICT: &str = "$ErrorActionPreference = 'Stop'\n$PSNativeCommandUseErrorActionPreference = $true\n";
const QUOTES: [char; 5] = ['\'', '\u{2018}', '\u{2019}', '\u{201a}', '\u{201b}'];
const SHEBANG: [u8; 2] = *b"#!";
const NOT_RUNNABLE: &str = "it is a script, and on Windows cornercase runs one only through Git for Windows' sh, \
                            which was not found";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Shell {
    Sh(Sh),
    PowerShell(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Sh {
    pub(crate) program: String,
    pub(crate) git: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Line {
    pub(crate) program: String,
    pub(crate) args: Vec<String>,
    pub(crate) env: Vec<(String, String)>,
}

impl Line {
    pub(crate) fn command(&self) -> Command {
        let mut command = Command::new(&self.program);
        command.args(&self.args).envs(self.env.iter().map(|(key, value)| (key, value)));
        command
    }

    pub(crate) fn os_env(&self) -> impl Iterator<Item = (OsString, OsString)> + '_ {
        self.env.iter().map(|(key, value)| (key.into(), value.into()))
    }

    pub(crate) fn on_git_path(mut self, sh: &Sh) -> Self {
        let folder = Path::new(&sh.program).parent().filter(|_| sh.git);
        let path = folder.and_then(|folder| {
            let inherited = std::env::var_os("PATH").unwrap_or_default();
            std::env::join_paths(std::iter::once(folder.to_path_buf()).chain(std::env::split_paths(&inherited))).ok()
        });
        self.env.extend(path.map(|path| ("PATH".to_string(), path.to_string_lossy().into_owned())));
        self
    }
}

impl Shell {
    pub(crate) fn here() -> &'static Self {
        static HERE: OnceLock<Shell> = OnceLock::new();
        HERE.get_or_init(|| {
            if cfg!(windows) { Self::pick(git_sh().as_deref(), program::locate(PWSH).as_deref()) } else { Self::sh() }
        })
    }

    pub(crate) fn sh() -> Self {
        Self::Sh(Sh { program: SH.into(), git: false })
    }

    pub(crate) fn pick(git_sh: Option<&Path>, pwsh: Option<&Path>) -> Self {
        match git_sh {
            Some(sh) => Self::Sh(Sh { program: sh.to_string_lossy().into_owned(), git: true }),
            None => {
                Self::PowerShell(pwsh.map_or_else(|| WINDOWS_POWERSHELL.into(), |pwsh| pwsh.to_string_lossy().into()))
            }
        }
    }

    pub(crate) fn run(&self, script: &str) -> Line {
        match self {
            Self::Sh(sh) => sh.line("-ec", script, &[]).on_git_path(sh),
            Self::PowerShell(program) => Line {
                program: program.clone(),
                args: vec!["-NoProfile".into(), "-Command".into(), strict(script)],
                env: Vec::new(),
            },
        }
    }

    pub(crate) fn interpreted(&self, program: &Path, args: &[String], log: Option<&Path>) -> Option<Line> {
        let Self::Sh(sh) = self else { return None };
        if !sh.git || !shebang(program) {
            return None;
        }
        let script = ffi::plain(program).to_string_lossy().into_owned();
        let positional: Vec<String> = ["sh".to_string(), script].into_iter().chain(args.iter().cloned()).collect();
        let text = log.map_or_else(
            || EXEC.to_string(),
            |log| format!("{EXEC} >>{} 2>&1", sh_quote(&ffi::plain(log).to_string_lossy())),
        );
        Some(sh.line("-c", &text, &positional).on_git_path(sh))
    }

    pub(crate) fn refuses(&self, program: &Path) -> Option<&'static str> {
        (matches!(self, Self::PowerShell(_)) && shebang(program)).then_some(NOT_RUNNABLE)
    }
}

impl Sh {
    pub(crate) fn line(&self, flags: &str, text: &str, positional: &[String]) -> Line {
        let program = self.program.clone();
        if !self.git {
            let args = [flags, text].into_iter().map(String::from).chain(positional.iter().cloned()).collect();
            return Line { program, args, env: Vec::new() };
        }
        let (name, rest) = positional.split_first().map_or(("sh", &[][..]), |(name, rest)| (name.as_str(), rest));
        let env = vec![(SCRIPT_ENV.into(), text.into()), (ARGS_ENV.into(), sh_quoted(rest))];
        Line { program, args: vec![flags.into(), ADAPTER.into(), name.into()], env }
    }
}

pub(crate) fn strict(script: &str) -> String {
    format!("{STRICT}{script}")
}

pub(crate) fn encoded(script: &str) -> String {
    let bytes: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    STANDARD.encode(bytes)
}

pub(crate) fn literal(text: &str) -> String {
    let mut quoted = String::with_capacity(text.len() + 2);
    quoted.push('\'');
    for c in text.chars() {
        quoted.push(c);
        if QUOTES.contains(&c) {
            quoted.push(c);
        }
    }
    quoted.push('\'');
    quoted
}

fn sh_quoted(args: &[String]) -> String {
    let quoted: Vec<String> = args.iter().map(|arg| sh_quote(arg)).collect();
    quoted.join(" ")
}

fn sh_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

pub(crate) fn git_sh() -> Option<PathBuf> {
    let git = program::locate("git");
    git.as_deref().and_then(Path::parent).and_then(beside_git).or_else(|| exec_path().as_deref().and_then(beside_git))
}

fn beside_git(folder: &Path) -> Option<PathBuf> {
    let mut candidates = folder.ancestors().take(GIT_LEVELS).flat_map(|dir| GIT_SHELLS.map(|sh| dir.join(sh)));
    candidates.find(|candidate| candidate.is_file())
}

fn exec_path() -> Option<PathBuf> {
    let output = process::output_within(program::command("git").arg("--exec-path"), EXEC_PATH_WITHIN).ok()??;
    let text = String::from_utf8(output.stdout).ok().filter(|_| output.status.success())?;
    Some(PathBuf::from(text.trim())).filter(|dir| dir.is_absolute())
}

fn shebang(program: &Path) -> bool {
    let mut start = [0; 2];
    std::fs::File::open(program).and_then(|mut file| file.read_exact(&mut start)).is_ok_and(|()| start == SHEBANG)
}

#[cfg(test)]
mod tests;
