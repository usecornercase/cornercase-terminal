use std::io::{self, Read};
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use rustix::io::Errno;
use rustix::process::{Pid, Signal, kill_process_group, test_kill_process};

pub(crate) use imp::{all, args, children, cwd, env, footprint, name, open_files, parent, peer_uid, started};

pub(crate) mod owners;

const MAX_DESCENDANTS: usize = 256;

const POLL: Duration = Duration::from_millis(20);

pub(crate) fn reap(mut child: Child) {
    thread::spawn(move || child.wait());
}

pub(crate) fn stop(mut child: Child) {
    let _ = child.kill();
    reap(child);
}

pub(crate) fn output_within(command: &mut Command, timeout: Duration) -> io::Result<Option<Output>> {
    let deadline = Instant::now() + timeout;
    let mut child =
        command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).process_group(0).spawn()?;
    let group = i32::try_from(child.id()).ok().and_then(Pid::from_raw);
    let (stdout, stderr) = (drain(child.stdout.take()), drain(child.stderr.take()));
    let left = || deadline.saturating_duration_since(Instant::now());
    let output = wait_until(&mut child, deadline)?.and_then(|status| {
        let stdout = stdout.recv_timeout(left()).ok()?;
        let stderr = stderr.recv_timeout(left()).ok()?;
        Some(Output { status, stdout, stderr })
    });
    if output.is_none() {
        if let Some(group) = group {
            let _ = kill_process_group(group, Signal::KILL);
        }
        let _ = child.kill();
        let _ = child.wait();
    }
    Ok(output)
}

fn drain(pipe: Option<impl Read + Send + 'static>) -> Receiver<Vec<u8>> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_end(&mut bytes);
        }
        let _ = tx.send(bytes);
    });
    rx
}

fn wait_until(child: &mut Child, deadline: Instant) -> io::Result<Option<ExitStatus>> {
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status));
        }
        if Instant::now() >= deadline {
            return Ok(None);
        }
        thread::sleep(POLL);
    }
}

pub(crate) fn alive(pid: i32) -> bool {
    Pid::from_raw(pid).is_some_and(|pid| !matches!(test_kill_process(pid), Err(Errno::SRCH)))
}

pub(crate) fn descendants(pid: i32) -> Vec<i32> {
    let mut found = children(pid);
    let mut at = 0;
    while at < found.len() && found.len() < MAX_DESCENDANTS {
        found.extend(children(found[at]));
        at += 1;
    }
    found.truncate(MAX_DESCENDANTS);
    found
}

#[cfg(any(target_os = "linux", target_os = "macos", test))]
fn vars<'a>(entries: impl Iterator<Item = &'a [u8]>) -> Vec<(String, String)> {
    entries
        .filter_map(|entry| {
            let entry = String::from_utf8_lossy(entry);
            let (key, value) = entry.split_once('=')?;
            (!key.is_empty()).then(|| (key.to_string(), value.to_string()))
        })
        .collect()
}

#[cfg(any(target_os = "linux", test))]
fn status_footprint(status: &str) -> Option<u64> {
    let kib = |key: &str| -> Option<u64> {
        status.lines().find_map(|line| line.strip_prefix(key))?.trim().strip_suffix("kB")?.trim().parse().ok()
    };
    Some(kib("RssAnon:")?.saturating_add(kib("VmSwap:").unwrap_or(0)).saturating_mul(1024))
}

#[cfg(any(target_os = "macos", test))]
fn procargs_strings(buf: &[u8]) -> Option<(usize, impl Iterator<Item = &[u8]>)> {
    let (argc, rest) = buf.split_first_chunk::<4>()?;
    let argc = usize::try_from(i32::from_ne_bytes(*argc)).unwrap_or(0);
    let rest = &rest[rest.iter().position(|b| *b == 0).unwrap_or(rest.len())..];
    let rest = &rest[rest.iter().position(|b| *b != 0).unwrap_or(rest.len())..];
    Some((argc, rest.split(|b| *b == 0)))
}

#[cfg(any(target_os = "macos", test))]
fn procargs(buf: &[u8]) -> Vec<String> {
    let Some((argc, strings)) = procargs_strings(buf) else { return Vec::new() };
    strings.take(argc).filter(|a| !a.is_empty()).map(|a| String::from_utf8_lossy(a).into_owned()).collect()
}

#[cfg(any(target_os = "macos", test))]
fn procenv(buf: &[u8]) -> Vec<(String, String)> {
    let Some((argc, strings)) = procargs_strings(buf) else { return Vec::new() };
    vars(strings.skip(argc).take_while(|s| !s.is_empty()))
}

#[cfg(target_os = "linux")]
mod imp {
    use super::{PathBuf, UnixStream, io, status_footprint, vars};

    pub(crate) fn all() -> Vec<i32> {
        std::fs::read_dir("/proc")
            .into_iter()
            .flatten()
            .filter_map(|entry| entry.ok()?.file_name().to_str()?.parse().ok())
            .collect()
    }

    pub(crate) fn children(pid: i32) -> Vec<i32> {
        let mut pids: Vec<i32> = std::fs::read_dir(format!("/proc/{pid}/task"))
            .into_iter()
            .flatten()
            .filter_map(|entry| std::fs::read_to_string(entry.ok()?.path().join("children")).ok())
            .flat_map(|text| text.split_whitespace().filter_map(|p| p.parse().ok()).collect::<Vec<_>>())
            .collect();
        pids.sort_unstable();
        pids.dedup();
        pids
    }

    pub(crate) fn open_files(pid: i32) -> Vec<PathBuf> {
        std::fs::read_dir(format!("/proc/{pid}/fd"))
            .into_iter()
            .flatten()
            .filter_map(|entry| std::fs::read_link(entry.ok()?.path()).ok())
            .collect()
    }

    pub(crate) fn peer_uid(socket: &UnixStream) -> io::Result<u32> {
        Ok(rustix::net::sockopt::socket_peercred(socket)?.uid.as_raw())
    }

    pub(crate) fn cwd(pid: i32) -> Option<PathBuf> {
        std::fs::read_link(format!("/proc/{pid}/cwd")).ok()
    }

    pub(crate) fn name(pid: i32) -> Option<String> {
        let comm = std::fs::read_to_string(format!("/proc/{pid}/comm")).ok()?;
        Some(comm.trim().to_string())
    }

    pub(crate) fn args(pid: i32) -> Vec<String> {
        let cmdline = std::fs::read(format!("/proc/{pid}/cmdline")).unwrap_or_default();
        cmdline.split(|b| *b == 0).filter(|a| !a.is_empty()).map(|a| String::from_utf8_lossy(a).into_owned()).collect()
    }

    pub(crate) fn env(pid: i32) -> Vec<(String, String)> {
        let environ = std::fs::read(format!("/proc/{pid}/environ")).unwrap_or_default();
        vars(environ.split(|b| *b == 0))
    }

    pub(crate) fn footprint(pid: i32) -> Option<u64> {
        status_footprint(&std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?)
    }

    pub(crate) fn started(pid: i32) -> Option<u64> {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        let rest = stat.rsplit_once(')')?.1;
        rest.split_whitespace().nth(19)?.parse().ok()
    }

    pub(crate) fn parent(pid: i32) -> Option<i32> {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        let rest = stat.rsplit_once(')')?.1;
        rest.split_whitespace().nth(1)?.parse().ok()
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use std::ffi::{CStr, OsStr, c_int};
    use std::os::fd::AsRawFd;
    use std::os::unix::ffi::OsStrExt;
    use std::ptr::null_mut;

    use super::{PathBuf, UnixStream, io, procargs, procenv};

    pub(crate) fn all() -> Vec<i32> {
        let count = unsafe { libc::proc_listallpids(null_mut(), 0) };
        listed(usize::try_from(count).unwrap_or(0) + 64, |buffer, size| unsafe { libc::proc_listallpids(buffer, size) })
    }

    pub(crate) fn children(pid: i32) -> Vec<i32> {
        listed(1024, |buffer, size| unsafe { libc::proc_listchildpids(pid, buffer, size) })
    }

    fn listed(capacity: usize, list: impl FnOnce(*mut std::ffi::c_void, c_int) -> c_int) -> Vec<i32> {
        let mut pids = vec![0_i32; capacity];
        let size = c_int::try_from(pids.len() * size_of::<i32>()).unwrap_or(0);
        let read = list(pids.as_mut_ptr().cast(), size);
        pids.truncate(usize::try_from(read).unwrap_or(0).min(capacity));
        pids.retain(|pid| *pid > 0);
        pids
    }

    pub(crate) fn open_files(pid: i32) -> Vec<PathBuf> {
        std::process::Command::new("/usr/sbin/lsof")
            .args(["-n", "-P", "-a", "-p", &pid.to_string(), "-Fn"])
            .output()
            .ok()
            .map(|out| {
                String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .filter_map(|line| line.strip_prefix('n').map(PathBuf::from))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(crate) fn peer_uid(socket: &UnixStream) -> io::Result<u32> {
        let (mut uid, mut gid) = (0, 0);
        if unsafe { libc::getpeereid(socket.as_raw_fd(), &raw mut uid, &raw mut gid) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(uid)
    }

    fn pid_info<T>(pid: i32, flavor: c_int) -> Option<T> {
        let mut info: T = unsafe { std::mem::zeroed() };
        let size = c_int::try_from(size_of::<T>()).ok()?;
        let read = unsafe { libc::proc_pidinfo(pid, flavor, 0, (&raw mut info).cast(), size) };
        (read == size).then_some(info)
    }

    pub(crate) fn cwd(pid: i32) -> Option<PathBuf> {
        let info: libc::proc_vnodepathinfo = pid_info(pid, libc::PROC_PIDVNODEPATHINFO)?;
        let bytes: Vec<u8> = info.pvi_cdir.vip_path.iter().flatten().map(|c| c.cast_unsigned()).collect();
        let path = CStr::from_bytes_until_nul(&bytes).ok()?;
        (!path.is_empty()).then(|| PathBuf::from(OsStr::from_bytes(path.to_bytes())))
    }

    pub(crate) fn footprint(pid: i32) -> Option<u64> {
        let mut info: libc::rusage_info_v0 = unsafe { std::mem::zeroed() };
        let read = unsafe { libc::proc_pid_rusage(pid, libc::RUSAGE_INFO_V0, (&raw mut info).cast()) };
        (read == 0).then_some(info.ri_phys_footprint)
    }

    pub(crate) fn name(pid: i32) -> Option<String> {
        let mut buf = [0_u8; 256];
        let len = unsafe { libc::proc_name(pid, buf.as_mut_ptr().cast(), 256) };
        let len = usize::try_from(len).ok().filter(|len| *len > 0)?;
        Some(String::from_utf8_lossy(&buf[..len]).into_owned())
    }

    fn procargs_buffer(pid: i32) -> Option<Vec<u8>> {
        let mut mib = [libc::CTL_KERN, libc::KERN_PROCARGS2, pid];
        let mut size: libc::size_t = 0;
        let asked = unsafe { libc::sysctl(mib.as_mut_ptr(), 3, null_mut(), &raw mut size, null_mut(), 0) };
        if asked != 0 || size == 0 {
            return None;
        }
        let mut buf = vec![0_u8; size];
        let read = unsafe { libc::sysctl(mib.as_mut_ptr(), 3, buf.as_mut_ptr().cast(), &raw mut size, null_mut(), 0) };
        if read != 0 {
            return None;
        }
        buf.truncate(size);
        Some(buf)
    }

    pub(crate) fn args(pid: i32) -> Vec<String> {
        procargs_buffer(pid).map(|buf| procargs(&buf)).unwrap_or_default()
    }

    pub(crate) fn env(pid: i32) -> Vec<(String, String)> {
        procargs_buffer(pid).map(|buf| procenv(&buf)).unwrap_or_default()
    }

    pub(crate) fn started(_pid: i32) -> Option<u64> {
        None
    }

    pub(crate) fn parent(pid: i32) -> Option<i32> {
        let info: libc::proc_bsdinfo = pid_info(pid, libc::PROC_PIDTBSDINFO)?;
        i32::try_from(info.pbi_ppid).ok()
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
mod imp {
    use super::{PathBuf, UnixStream, io};

    pub(crate) fn all() -> Vec<i32> {
        Vec::new()
    }

    pub(crate) fn children(_pid: i32) -> Vec<i32> {
        Vec::new()
    }

    pub(crate) fn open_files(_pid: i32) -> Vec<PathBuf> {
        Vec::new()
    }

    pub(crate) fn peer_uid(_socket: &UnixStream) -> io::Result<u32> {
        Err(io::ErrorKind::Unsupported.into())
    }

    pub(crate) fn cwd(_pid: i32) -> Option<PathBuf> {
        None
    }

    pub(crate) fn name(_pid: i32) -> Option<String> {
        None
    }

    pub(crate) fn args(_pid: i32) -> Vec<String> {
        Vec::new()
    }

    pub(crate) fn env(_pid: i32) -> Vec<(String, String)> {
        Vec::new()
    }

    pub(crate) fn footprint(_pid: i32) -> Option<u64> {
        None
    }

    pub(crate) fn started(_pid: i32) -> Option<u64> {
        None
    }

    pub(crate) fn parent(_pid: i32) -> Option<i32> {
        None
    }
}

#[cfg(test)]
mod tests;
