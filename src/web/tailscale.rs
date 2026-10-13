use std::fmt;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::Value;

pub(crate) const HTTPS_PORT: u16 = 8443;
pub(crate) const ADMIN_DNS: &str = "https://login.tailscale.com/admin/dns";
const APP_BINARY: &str = "/Applications/Tailscale.app/Contents/MacOS/Tailscale";
const TIMEOUT: Duration = Duration::from_secs(15);
const POLL: Duration = Duration::from_millis(20);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Node {
    pub(crate) host: String,
    pub(crate) login: String,
}

impl Node {
    pub(crate) fn url(&self) -> String {
        format!("https://{}:{HTTPS_PORT}", self.host)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Problem {
    NotInstalled,
    NotConnected(String),
    NoHttps,
    Failed(String),
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotInstalled => write!(f, "tailscale is not installed on this computer"),
            Self::NotConnected(state) => write!(f, "tailscale is not connected ({state}): open it and log in"),
            Self::NoHttps => write!(f, "turn on HTTPS certificates for your tailnet at {ADMIN_DNS}"),
            Self::Failed(why) => write!(f, "tailscale: {why}"),
        }
    }
}

pub(crate) fn binary() -> Option<PathBuf> {
    let on_path = std::env::var_os("PATH")
        .into_iter()
        .flat_map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .map(|dir| dir.join("tailscale"))
        .find(|candidate| candidate.is_file());
    on_path.or_else(|| Some(PathBuf::from(APP_BINARY)).filter(|app| app.is_file()))
}

pub(crate) fn start(program: &Path, local_port: u16) -> Result<Node, Problem> {
    let node = status(program)?;
    let target = format!("http://127.0.0.1:{local_port}");
    let https = format!("--https={HTTPS_PORT}");
    run(program, &["serve", "--bg", "--yes", &https, &target])?;
    Ok(node)
}

pub(crate) fn stop(program: &Path) -> Result<(), Problem> {
    let https = format!("--https={HTTPS_PORT}");
    run(program, &["serve", &https, "off"]).map(|_| ())
}

pub(crate) fn status(program: &Path) -> Result<Node, Problem> {
    let output = run(program, &["status", "--json"])?;
    parse_status(&String::from_utf8_lossy(&output.stdout))
}

pub(crate) fn parse_status(text: &str) -> Result<Node, Problem> {
    let status: Value =
        serde_json::from_str(text).map_err(|e| Problem::Failed(format!("unexpected status output: {e}")))?;
    let state = status["BackendState"].as_str().unwrap_or("unknown");
    if state != "Running" {
        return Err(Problem::NotConnected(state.into()));
    }
    let host = status["Self"]["DNSName"].as_str().unwrap_or_default().trim_end_matches('.').to_string();
    if host.is_empty() {
        return Err(Problem::Failed("this computer has no tailnet name; turn on MagicDNS".into()));
    }
    let certified =
        status["CertDomains"].as_array().is_some_and(|domains| domains.iter().any(|d| d.as_str() == Some(&host)));
    if !certified {
        return Err(Problem::NoHttps);
    }
    let user = status["Self"]["UserID"].as_u64().map(|id| id.to_string()).unwrap_or_default();
    let login = status["User"][&user]["LoginName"].as_str().unwrap_or_default().to_string();
    if login.is_empty() {
        return Err(Problem::Failed("cannot tell which account owns this computer".into()));
    }
    Ok(Node { host, login })
}

fn drain(pipe: Option<impl Read + Send + 'static>) -> thread::JoinHandle<Vec<u8>> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_end(&mut bytes);
        }
        bytes
    })
}

fn wait(child: &mut Child, timeout: Duration) -> io::Result<Option<std::process::ExitStatus>> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status));
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(None);
        }
        thread::sleep(POLL);
    }
}

fn run(program: &Path, args: &[&str]) -> Result<Output, Problem> {
    let spawned =
        Command::new(program).args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn();
    let mut child = match spawned {
        Ok(child) => child,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Err(Problem::NotInstalled),
        Err(e) => return Err(Problem::Failed(e.to_string())),
    };
    let (stdout, stderr) = (drain(child.stdout.take()), drain(child.stderr.take()));
    let status = wait(&mut child, TIMEOUT).map_err(|e| Problem::Failed(e.to_string()))?;
    let output = Output {
        status: status.ok_or_else(|| Problem::Failed(format!("no answer within {} s", TIMEOUT.as_secs())))?,
        stdout: stdout.join().unwrap_or_default(),
        stderr: stderr.join().unwrap_or_default(),
    };
    if output.status.success() {
        return Ok(output);
    }
    let said = String::from_utf8_lossy(&output.stderr);
    let last = said.lines().map(str::trim).rfind(|line| !line.is_empty()).map(String::from);
    Err(Problem::Failed(last.unwrap_or_else(|| output.status.to_string())))
}

#[cfg(test)]
mod tests;
