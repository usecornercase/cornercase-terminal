use std::borrow::Cow;
use std::ffi::{OsStr, OsString};
use std::fmt::Write;
use std::fs::Metadata;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::sound::{Os, local_machine, on_path};
use crate::os::desktop;
use crate::os::fs::PermissionsExt;
use crate::process;

const REVEAL_WITHIN: Duration = Duration::from_secs(5);
const REPORT_WITHIN: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub(crate) enum Opener {
    Finder,
    Files,
    Explorer,
    Vscode,
    Cursor,
    Zed,
    Sublime,
    Default,
}

impl Opener {
    pub(crate) fn id(self) -> &'static str {
        match self {
            Self::Finder => "finder",
            Self::Files => "files",
            Self::Explorer => "explorer",
            Self::Vscode => "vscode",
            Self::Cursor => "cursor",
            Self::Zed => "zed",
            Self::Sublime => "sublime",
            Self::Default => "default",
        }
    }

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Finder => "Finder",
            Self::Files => "file manager",
            Self::Explorer => "File Explorer",
            Self::Vscode => "VS Code",
            Self::Cursor => "Cursor",
            Self::Zed => "Zed",
            Self::Sublime => "Sublime Text",
            Self::Default => "default app",
        }
    }

    fn editor(self) -> Option<Editor> {
        let (commands, bundle): (&[&str], &str) = match self {
            Self::Vscode => (&["code"], "Visual Studio Code.app"),
            Self::Cursor => (&["cursor"], "Cursor.app"),
            Self::Zed => (&["zeditor", "zed"], "Zed.app"),
            Self::Sublime => (&["subl"], "Sublime Text.app"),
            Self::Finder | Self::Files | Self::Explorer | Self::Default => return None,
        };
        Some(Editor { commands, bundle })
    }

    pub(crate) fn applies(self, kind: Kind) -> bool {
        self != Self::Default || kind == Kind::File
    }
}

struct Editor {
    commands: &'static [&'static str],
    bundle: &'static str,
}

const EDITORS: [Opener; 4] = [Opener::Vscode, Opener::Cursor, Opener::Zed, Opener::Sublime];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Open {
    pub(crate) opener: Opener,
    pub(crate) path: String,
    pub(crate) line: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Browse {
    pub(crate) ip: Option<IpAddr>,
    pub(crate) port: u16,
}

impl Browse {
    pub(crate) fn url(&self) -> String {
        match self.ip {
            None => format!("http://localhost:{}", self.port),
            Some(IpAddr::V4(ip)) => format!("http://{ip}:{}", self.port),
            Some(IpAddr::V6(ip)) => format!("http://[{ip}]:{}", self.port),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Failed {
    pub(crate) opener: Opener,
    pub(crate) reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Folder,
    File,
    Program,
}

impl Kind {
    pub(crate) fn of(path: &Path) -> Option<Self> {
        let meta = path.metadata().ok()?;
        if meta.is_dir() {
            Some(Self::Folder)
        } else if meta.is_file() {
            Some(if program(path, &meta) { Self::Program } else { Self::File })
        } else {
            None
        }
    }
}

fn program(path: &Path, meta: &Metadata) -> bool {
    if cfg!(windows) { has_extension(path, &RUN_WHEN_OPENED) } else { meta.permissions().mode() & 0o111 != 0 }
}

impl Os {
    fn has_desktop(self, set: impl Fn(&str) -> bool) -> bool {
        self != Self::Linux || set("WAYLAND_DISPLAY") || set("DISPLAY")
    }

    fn launcher(self) -> &'static str {
        match self {
            Self::Macos => "open",
            Self::Linux => "xdg-open",
            Self::Windows => "explorer",
        }
    }

    fn file_manager(self) -> Opener {
        match self {
            Self::Macos => Opener::Finder,
            Self::Linux => Opener::Files,
            Self::Windows => Opener::Explorer,
        }
    }

    fn default_app(self) -> Reach {
        if self == Self::Windows { Reach::Shell } else { Reach::Launcher }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Reach {
    Launcher,
    Shell,
    Command(PathBuf),
    App(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Step {
    Run(Vec<OsString>),
    Shell(Shell),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Shell {
    verb: Option<&'static str>,
    file: OsString,
    parameters: Option<OsString>,
}

impl Shell {
    fn open(file: OsString) -> Self {
        Self { verb: None, file, parameters: None }
    }
}

#[derive(Debug, Default, Clone)]
pub(crate) struct Openers {
    found: Vec<(Opener, Reach)>,
    launcher: Option<PathBuf>,
    dbus: Option<PathBuf>,
}

impl Openers {
    pub(crate) fn detect(env: impl Fn(&str) -> Option<String>) -> Self {
        let home = env("HOME").filter(|home| !home.is_empty()).map(PathBuf::from);
        let mut apps = vec![PathBuf::from("/Applications")];
        apps.extend(home.map(|home| home.join("Applications")));
        Self::find(Os::current(), &env, &apps)
    }

    fn find(os: Os, env: &impl Fn(&str) -> Option<String>, apps: &[PathBuf]) -> Self {
        let set = |name: &str| env(name).is_some_and(|value| !value.is_empty());
        if !local_machine(env) || !os.has_desktop(set) {
            return Self::default();
        }
        let path = env("PATH").map(OsString::from);
        let extensions = os.extensions(env("PATHEXT").as_deref());
        let lookup = on_path(path.as_deref(), &extensions);
        let launcher = lookup(os.launcher());
        let mut found = Vec::new();
        if launcher.is_some() {
            found.push((os.file_manager(), Reach::Launcher));
            found.push((Opener::Default, os.default_app()));
        }
        let apps = if launcher.is_some() && os != Os::Windows { apps } else { &[] };
        found.extend(EDITORS.into_iter().filter_map(|opener| Some((opener, editor(os, opener, &lookup, apps)?))));
        found.sort_by_key(|(opener, _)| *opener);
        Self { found, launcher, dbus: lookup("dbus-send") }
    }

    pub(crate) fn ids(&self) -> Vec<Opener> {
        self.found.iter().map(|(opener, _)| *opener).collect()
    }

    fn steps(&self, open: &Open, kind: Option<Kind>) -> Result<Vec<Step>, String> {
        let open = &Open { path: plain(&open.path).into_owned(), ..open.clone() };
        let path = Path::new(&open.path);
        if !path.is_absolute() {
            return Err("the path is not absolute".into());
        }
        let Some((_, reach)) = self.found.iter().find(|(opener, _)| *opener == open.opener) else {
            return Err(format!("{} is not on this computer", open.opener.name()));
        };
        let kind = kind.ok_or("it is gone")?;
        if !open.opener.applies(kind) {
            return Err("the default app opens only plain files".into());
        }
        let target = OsString::from(&open.path);
        let launcher = self.launcher.as_deref().ok_or("there is no launcher")?;
        match (open.opener, reach) {
            (Opener::Default, _) => Ok(self.default_app(target).into_iter().collect()),
            (Opener::Finder, _) => Ok(vec![finder(launcher, path, kind, target)]),
            (Opener::Files, _) if kind == Kind::Folder => Ok(vec![step(launcher, &[target])]),
            (Opener::Files, _) => Ok(self.reveal_steps(open, launcher)),
            (Opener::Explorer, _) => Ok(vec![explorer(launcher, kind, target)]),
            (_, Reach::App(app)) => Ok(vec![step(launcher, &["-a".into(), app.as_os_str().to_os_string(), target])]),
            (opener, Reach::Command(command)) => Ok(vec![command_step(command, opener, open, kind, target)]),
            (_, Reach::Launcher | Reach::Shell) => Err("it has no command".into()),
        }
    }

    fn default_app(&self, target: OsString) -> Option<Step> {
        let launcher = self.launcher.as_deref()?;
        match self.found.iter().find(|(opener, _)| *opener == Opener::Default)? {
            (_, Reach::Shell) => Some(Step::Shell(Shell::open(target))),
            _ => Some(step(launcher, &[target])),
        }
    }

    fn reveal_steps(&self, open: &Open, launcher: &Path) -> Vec<Step> {
        let parent = Path::new(&open.path)
            .parent()
            .map_or_else(|| OsString::from(&open.path), |parent| parent.as_os_str().to_os_string());
        let mut steps = Vec::new();
        if let Some(dbus) = &self.dbus {
            steps.push(step(dbus, &reveal_args(&open.path)));
        }
        steps.push(step(launcher, &[parent]));
        steps
    }

    pub(crate) fn browse(&self, browse: Browse, failed: impl FnOnce(Failed) + Send + 'static) {
        let Some(step) = self.default_app(browse.url().into()) else { return };
        thread::spawn(move || {
            if let Err(reason) = run(&[step]) {
                failed(Failed { opener: Opener::Default, reason });
            }
        });
    }

    pub(crate) fn open(&self, open: &Open, failed: impl FnOnce(Failed) + Send + 'static) {
        if self.found.is_empty() {
            return;
        }
        let (this, open) = (self.clone(), open.clone());
        thread::spawn(move || {
            let path = Path::new(&open.path);
            let kind = path.is_absolute().then(|| Kind::of(path)).flatten();
            let done = this.steps(&open, kind).and_then(|steps| run(&steps));
            if let Err(reason) = done {
                failed(Failed { opener: open.opener, reason });
            }
        });
    }
}

fn editor(os: Os, opener: Opener, lookup: &impl Fn(&str) -> Option<PathBuf>, apps: &[PathBuf]) -> Option<Reach> {
    let editor = opener.editor()?;
    let command = editor
        .commands
        .iter()
        .filter_map(|name| lookup(name).map(|path| (*name, path)))
        .find(|(name, path)| *name != "zed" || os == Os::Windows || in_zed_app(path))
        .map(|(_, path)| Reach::Command(path));
    command.or_else(|| apps.iter().map(|dir| dir.join(editor.bundle)).find(|app| app.is_dir()).map(Reach::App))
}

fn step(program: &Path, args: &[OsString]) -> Step {
    let mut step = vec![program.as_os_str().to_os_string()];
    step.extend_from_slice(args);
    Step::Run(step)
}

fn command_step(command: &Path, opener: Opener, open: &Open, kind: Kind, target: OsString) -> Step {
    let at_line = open.line.filter(|_| kind != Kind::Folder);
    let args = match (at_line, opener) {
        (Some(line), Opener::Vscode | Opener::Cursor) => vec!["-g".into(), format!("{}:{line}", open.path).into()],
        (Some(line), _) => vec![format!("{}:{line}", open.path).into()],
        (None, _) => vec![target],
    };
    step(command, &args)
}

fn finder(launcher: &Path, path: &Path, kind: Kind, target: OsString) -> Step {
    let reveal = kind != Kind::Folder || has_extension(path, &BUNDLES);
    step(launcher, &if reveal { vec!["-R".into(), target] } else { vec![target] })
}

fn explorer(launcher: &Path, kind: Kind, target: OsString) -> Step {
    if kind == Kind::Folder {
        return Step::Shell(Shell { verb: Some("explore"), file: target, parameters: None });
    }
    let mut select = OsString::from("/select,\"");
    select.push(&target);
    select.push("\"");
    Step::Shell(Shell { verb: None, file: launcher.as_os_str().to_os_string(), parameters: Some(select) })
}

fn plain(path: &str) -> Cow<'_, str> {
    if let Some(share) = path.strip_prefix(r"\\?\UNC\") {
        return Cow::Owned(format!(r"\\{share}"));
    }
    match path.strip_prefix(r"\\?\") {
        Some(local) if local.as_bytes().get(1) == Some(&b':') => Cow::Borrowed(local),
        _ => Cow::Borrowed(path),
    }
}

const BUNDLES: [&str; 10] =
    ["app", "appex", "bundle", "framework", "pkg", "playground", "plugin", "prefpane", "xcodeproj", "xcworkspace"];

const RUN_WHEN_OPENED: [&str; 25] = [
    "bat", "cmd", "com", "cpl", "exe", "hta", "jar", "js", "jse", "lnk", "msc", "msi", "msp", "pif", "ps1", "py",
    "pyw", "reg", "scr", "url", "vbe", "vbs", "ws", "wsf", "wsh",
];

fn has_extension(path: &Path, extensions: &[&str]) -> bool {
    path.extension().and_then(OsStr::to_str).is_some_and(|ext| extensions.iter().any(|e| ext.eq_ignore_ascii_case(e)))
}

fn in_zed_app(path: &Path) -> bool {
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    path.components().any(|part| part.as_os_str().eq_ignore_ascii_case("zed.app"))
}

fn reveal_args(path: &str) -> Vec<OsString> {
    [
        "--session",
        "--print-reply",
        "--reply-timeout=5000",
        "--dest=org.freedesktop.FileManager1",
        "/org/freedesktop/FileManager1",
        "org.freedesktop.FileManager1.ShowItems",
    ]
    .iter()
    .map(OsString::from)
    .chain([format!("array:string:{}", uri(path)).into(), "string:".into()])
    .collect()
}

fn uri(path: &str) -> String {
    let mut out = String::from("file://");
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~/".contains(&byte) {
            out.push(char::from(byte));
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

fn run(steps: &[Step]) -> Result<(), String> {
    let Some((last, early)) = steps.split_last() else { return Ok(()) };
    for step in early {
        let Step::Run(step) = step else { continue };
        let Some((program, args)) = step.split_first() else { continue };
        let done = process::output_within(Command::new(program).args(args), REVEAL_WITHIN);
        if done.is_ok_and(|output| output.is_some_and(|output| output.status.success())) {
            return Ok(());
        }
    }
    match last {
        Step::Run(words) => start(words),
        Step::Shell(shell) => {
            desktop::shell_open(shell.verb, &shell.file, shell.parameters.as_deref()).map_err(|err| err.to_string())
        }
    }
}

fn start(words: &[OsString]) -> Result<(), String> {
    let Some((program, args)) = words.split_first() else { return Ok(()) };
    let started = Instant::now();
    let mut command = Command::new(program);
    command.args(args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    let mut child = desktop::detach(&mut command).spawn().map_err(|err| err.to_string())?;
    let status = child.wait().map_err(|err| err.to_string())?;
    if status.success() || started.elapsed() > REPORT_WITHIN {
        return Ok(());
    }
    Err(status.code().map_or_else(|| "it was stopped by a signal".into(), |code| format!("exit status {code}")))
}

#[cfg(test)]
mod tests;
