use std::ffi::OsString;
use std::fmt::Write;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::sound::{Os, local_machine, on_path};
use crate::process;

const REVEAL_WITHIN: Duration = Duration::from_secs(5);
const REPORT_WITHIN: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub(crate) enum Opener {
    Finder,
    Files,
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
            Self::Finder | Self::Files | Self::Default => return None,
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
            Some(if meta.permissions().mode() & 0o111 != 0 { Self::Program } else { Self::File })
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Reach {
    Launcher,
    Command(PathBuf),
    App(PathBuf),
}

type Step = Vec<OsString>;

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
        if !local_machine(env) || (os == Os::Linux && !set("WAYLAND_DISPLAY") && !set("DISPLAY")) {
            return Self::default();
        }
        let path = env("PATH").map(OsString::from);
        let lookup = on_path(path.as_deref());
        let launcher = lookup(if os == Os::Macos { "open" } else { "xdg-open" });
        let mut found = Vec::new();
        if launcher.is_some() {
            found.push((if os == Os::Macos { Opener::Finder } else { Opener::Files }, Reach::Launcher));
            found.push((Opener::Default, Reach::Launcher));
        }
        for opener in EDITORS {
            let Some(editor) = opener.editor() else { continue };
            let command = editor
                .commands
                .iter()
                .filter_map(|name| lookup(name).map(|path| (*name, path)))
                .find(|(name, path)| *name != "zed" || in_zed_app(path))
                .map(|(_, path)| Reach::Command(path));
            let bundle = || {
                launcher.as_ref()?;
                apps.iter().map(|dir| dir.join(editor.bundle)).find(|app| app.is_dir()).map(Reach::App)
            };
            if let Some(reach) = command.or_else(bundle) {
                found.push((opener, reach));
            }
        }
        found.sort_by_key(|(opener, _)| *opener);
        Self { found, launcher, dbus: lookup("dbus-send") }
    }

    pub(crate) fn ids(&self) -> Vec<Opener> {
        self.found.iter().map(|(opener, _)| *opener).collect()
    }

    fn steps(&self, open: &Open, kind: Option<Kind>) -> Result<Vec<Step>, String> {
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
        let step = |program: &Path, args: &[OsString]| {
            let mut step = vec![program.as_os_str().to_os_string()];
            step.extend_from_slice(args);
            step
        };
        match (open.opener, reach) {
            (Opener::Default, _) => Ok(vec![step(launcher, &[target])]),
            (Opener::Finder, _) => {
                let reveal = kind != Kind::Folder || is_bundle(path);
                let args = if reveal { vec!["-R".into(), target] } else { vec![target] };
                Ok(vec![step(launcher, &args)])
            }
            (Opener::Files, _) if kind == Kind::Folder => Ok(vec![step(launcher, &[target])]),
            (Opener::Files, _) => {
                let parent = path.parent().map_or_else(|| target.clone(), |parent| parent.as_os_str().to_os_string());
                let mut steps = Vec::new();
                if let Some(dbus) = &self.dbus {
                    steps.push(step(dbus, &reveal_args(&open.path)));
                }
                steps.push(step(launcher, &[parent]));
                Ok(steps)
            }
            (_, Reach::App(app)) => Ok(vec![step(launcher, &["-a".into(), app.as_os_str().to_os_string(), target])]),
            (opener, Reach::Command(command)) => {
                let at_line = open.line.filter(|_| kind != Kind::Folder);
                let args = match (at_line, opener) {
                    (Some(line), Opener::Vscode | Opener::Cursor) => {
                        vec!["-g".into(), format!("{}:{line}", open.path).into()]
                    }
                    (Some(line), _) => vec![format!("{}:{line}", open.path).into()],
                    (None, _) => vec![target],
                };
                Ok(vec![step(command, &args)])
            }
            (_, Reach::Launcher) => Err("it has no command".into()),
        }
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

const BUNDLES: [&str; 10] =
    ["app", "appex", "bundle", "framework", "pkg", "playground", "plugin", "prefpane", "xcodeproj", "xcworkspace"];

fn is_bundle(path: &Path) -> bool {
    path.extension().and_then(|ext| ext.to_str()).is_some_and(|ext| BUNDLES.iter().any(|b| ext.eq_ignore_ascii_case(b)))
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
        let Some((program, args)) = step.split_first() else { continue };
        let done = process::output_within(Command::new(program).args(args), REVEAL_WITHIN);
        if done.is_ok_and(|output| output.is_some_and(|output| output.status.success())) {
            return Ok(());
        }
    }
    let Some((program, args)) = last.split_first() else { return Ok(()) };
    let started = Instant::now();
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .map_err(|err| err.to_string())?;
    let status = child.wait().map_err(|err| err.to_string())?;
    if status.success() || started.elapsed() > REPORT_WITHIN {
        return Ok(());
    }
    Err(status.code().map_or_else(|| "it was stopped by a signal".into(), |code| format!("exit status {code}")))
}

#[cfg(test)]
mod tests;
