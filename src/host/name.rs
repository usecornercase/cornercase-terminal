use std::sync::OnceLock;

pub(crate) fn computer() -> Option<String> {
    static NAME: OnceLock<Option<String>> = OnceLock::new();
    NAME.get_or_init(imp::read).clone()
}

fn machine_name(computer: Option<String>, hostname: &str) -> Option<String> {
    let computer = computer.map(|name| name.trim().to_string()).filter(|name| !name.is_empty());
    let hostname = hostname.trim();
    let hostname = hostname.strip_suffix(".local").unwrap_or(hostname).trim();
    computer.or_else(|| (!hostname.is_empty()).then(|| hostname.to_string()))
}

#[cfg(target_os = "macos")]
mod imp {
    use std::process::Command;
    use std::time::Duration;

    use super::machine_name;
    use crate::process;

    const WITHIN: Duration = Duration::from_secs(2);

    fn run(program: &str, args: &[&str]) -> Option<String> {
        let output = process::output_within(Command::new(program).args(args), WITHIN).ok()??;
        output.status.success().then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    pub(super) fn read() -> Option<String> {
        let computer = run("/usr/sbin/scutil", &["--get", "ComputerName"]).filter(|name| !name.is_empty());
        let hostname = if computer.is_some() { String::new() } else { run("/bin/hostname", &[]).unwrap_or_default() };
        machine_name(computer, &hostname)
    }
}

#[cfg(target_os = "linux")]
mod imp {
    use super::machine_name;

    pub(super) fn read() -> Option<String> {
        let hostname = std::fs::read_to_string("/proc/sys/kernel/hostname").unwrap_or_default();
        machine_name(None, &hostname)
    }
}

#[cfg(windows)]
mod imp {
    use super::machine_name;

    pub(super) fn read() -> Option<String> {
        machine_name(std::env::var("COMPUTERNAME").ok(), "")
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
mod imp {
    pub(super) fn read() -> Option<String> {
        None
    }
}

#[cfg(test)]
mod tests;
