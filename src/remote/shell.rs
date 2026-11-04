use serde::{Deserialize, Serialize};

use super::{CANNOT_EXECUTE, NOT_FOUND};
use crate::agents::{STOP_PARSING, powershell_quoted, splits};
use crate::protocol::TIED_ENV;

pub(super) const PROBE: &str = r#"echo cornercase-shell-"x"-$true-$env:OS"#;
const HEARD: &str = "cornercase-shell-";
const HEARD_CMD: &str = r#""x"-$true-$env:OS"#;
const HEARD_POWERSHELL: &str = "x-True-Windows_NT";
const PROGRAM: &str = "cornercase";
const FOLDERS: [&str; 2] = [r"\.local\bin", r"\.cargo\bin"];
const WHERE: &str = r#""%SystemRoot%\System32\where.exe" /q"#;
const CMD_PATH: &str = r#"%PATH:"=%"#;
const CMD_BARE: &str = r"-_./\:,+=@";
const CMD_READS: [char; 3] = ['"', '%', '!'];
const HOME_CMD: &str = "%USERPROFILE%";
const NOT_FOUND_ERROR: &str = "System.Management.Automation.CommandNotFoundException";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum RemoteShell {
    #[serde(rename = "sh")]
    Posix,
    #[serde(rename = "cmd")]
    Cmd,
    #[serde(rename = "powershell")]
    PowerShell,
}

impl RemoteShell {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Posix => "sh",
            Self::Cmd => "cmd.exe",
            Self::PowerShell => "PowerShell",
        }
    }

    pub(super) fn windows(self) -> bool {
        self != Self::Posix
    }
}

pub(super) fn heard(line: &str) -> Option<RemoteShell> {
    match line.trim().strip_prefix(HEARD)? {
        HEARD_CMD => Some(RemoteShell::Cmd),
        HEARD_POWERSHELL => Some(RemoteShell::PowerShell),
        _ => Some(RemoteShell::Posix),
    }
}

pub(super) fn windows_program(given: &str) -> bool {
    let mut chars = given.chars();
    let drive = chars.next().is_some_and(|c| c.is_ascii_alphabetic()) && chars.next() == Some(':');
    drive || given.starts_with(r"\\") || given.starts_with(r"~\")
}

struct Program {
    home: bool,
    path: String,
}

impl Program {
    fn of(given: Option<&str>) -> Result<Self, String> {
        let given = given.unwrap_or(PROGRAM);
        if given.is_empty() {
            return Err("the command to run there is empty".into());
        }
        Ok(match given.strip_prefix('~').filter(|rest| rest.starts_with(['\\', '/'])) {
            Some(rest) => Self { home: true, path: rest.replace('/', "\\") },
            None => Self { home: false, path: given.replace('/', "\\") },
        })
    }
}

pub(super) fn cmd_line(given: Option<&str>, args: &[&str], tied: bool) -> Result<String, String> {
    let program = Program::of(given)?;
    cmd_checked(&program.path)?;
    let mut words = vec![cmd_program(&program)];
    for arg in args {
        cmd_checked(arg)?;
        words.push(cmd_word(arg));
    }
    let folders: Vec<String> = FOLDERS.iter().map(|folder| format!(";{HOME_CMD}{folder}")).collect();
    let tie = if tied { format!("set \"{TIED_ENV}=1\" && ") } else { String::new() };
    Ok(format!(
        "set \"PATH={CMD_PATH}{}\" && {tie}({WHERE} {} || exit {NOT_FOUND}) && {}",
        folders.concat(),
        cmd_found(&program),
        words.join(" ")
    ))
}

fn cmd_checked(word: &str) -> Result<(), String> {
    match word.chars().find(|c| CMD_READS.contains(c) || c.is_control()) {
        Some(c) if c.is_control() => Err(format!("`{}` holds a control character", word.escape_debug())),
        Some(c) => Err(format!("`{word}` holds a `{c}`, which cmd.exe would read as its own")),
        None => Ok(()),
    }
}

fn cmd_word(word: &str) -> String {
    if !word.is_empty() && word.chars().all(|c| c.is_ascii_alphanumeric() || CMD_BARE.contains(c)) {
        return word.to_string();
    }
    cmd_quoted(word)
}

fn cmd_quoted(text: &str) -> String {
    let trailing = text.len() - text.trim_end_matches('\\').len();
    format!("\"{text}{}\"", "\\".repeat(trailing))
}

fn cmd_program(program: &Program) -> String {
    let home = if program.home { HOME_CMD } else { "" };
    cmd_quoted(&format!("{home}{}", program.path))
}

fn cmd_found(program: &Program) -> String {
    let home = if program.home { HOME_CMD } else { "" };
    match program.path.rfind('\\') {
        Some(at) => format!("\"{home}{}:{}\"", &program.path[..=at], &program.path[at + 1..]),
        None => cmd_quoted(&program.path),
    }
}

pub(super) fn powershell_line(given: Option<&str>, args: &[&str], tied: bool) -> Result<String, String> {
    let program = Program::of(given)?;
    let mut words = vec![powershell_program(&program)?];
    for arg in args {
        words.push(powershell_word(arg)?);
    }
    let folders: Vec<String> = FOLDERS.iter().map(|folder| format!(" + ';' + $env:USERPROFILE + '{folder}'")).collect();
    let tie = if tied { format!(" $env:{TIED_ENV} = '1';") } else { String::new() };
    Ok(format!(
        "$env:PATH = $env:PATH{};{tie} try {{ & {} }} catch [{NOT_FOUND_ERROR}] {{ exit {NOT_FOUND} }} \
         catch {{ [Console]::Error.WriteLine($_); exit {CANNOT_EXECUTE} }}; exit $LASTEXITCODE",
        folders.concat(),
        words.join(" ")
    ))
}

fn powershell_program(program: &Program) -> Result<String, String> {
    powershell_checked(&program.path)?;
    let quoted = powershell_quoted(&program.path);
    Ok(if program.home { format!("($env:USERPROFILE + {quoted})") } else { quoted })
}

fn powershell_word(word: &str) -> Result<String, String> {
    if word.is_empty() {
        return Err("Windows PowerShell 5.1 drops an empty argument".into());
    }
    if word == STOP_PARSING || splits(word) {
        return Err(format!("Windows PowerShell 5.1 would pass `{word}` on in pieces"));
    }
    powershell_checked(word)?;
    Ok(powershell_quoted(word))
}

fn powershell_checked(word: &str) -> Result<(), String> {
    if word.chars().any(char::is_control) {
        return Err(format!("`{}` holds a control character", word.escape_debug()));
    }
    if word.contains("  ") {
        return Err(format!("`{word}` holds two spaces in a row, which a terminal there passes on as one"));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
