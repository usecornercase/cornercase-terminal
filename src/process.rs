use std::cell::Cell;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

#[cfg(not(unix))]
use crate::os::process::kill_process;
#[cfg(unix)]
use crate::os::process::kill_process_group;
use crate::os::process::{Grouped, Pid, Signal};
use crate::os::socket::UnixStream;
use crate::text;

pub(crate) use crate::os::process::alive;
#[cfg(windows)]
pub(crate) use imp::listing;
pub(crate) use imp::{
    all, args, children, cpu_time, cwd, env, footprint, name, open_files, parent, peer_uid, start_stamp, started,
};

pub(crate) mod escalation;
pub(crate) mod owners;

const MAX_DESCENDANTS: usize = 256;

const POLL: Duration = Duration::from_millis(20);

const SHELLS: [&str; 17] = [
    "sh", "bash", "zsh", "fish", "dash", "ksh", "mksh", "oksh", "loksh", "yash", "tcsh", "csh", "nu", "elvish",
    "xonsh", "pwsh", "ion",
];

const WINDOWS_SHELLS: [&str; 2] = ["cmd", "powershell"];

const EXE: &str = ".exe";

pub(crate) fn is_shell(name: &str) -> bool {
    let name = program_name(name);
    let windows: &[&str] = if cfg!(windows) { &WINDOWS_SHELLS } else { &[] };
    SHELLS.iter().chain(windows).any(|shell| same_file_name(shell, name))
}

pub(crate) fn program_name(name: &str) -> &str {
    if cfg!(windows) { trim_exe(name) } else { name }
}

pub(crate) fn same_file_name(name: &str, other: &str) -> bool {
    if cfg!(windows) {
        name.chars().flat_map(char::to_lowercase).eq(other.chars().flat_map(char::to_lowercase))
    } else {
        name == other
    }
}

#[cfg(not(windows))]
pub(crate) struct Listing;

#[cfg(not(windows))]
#[expect(clippy::unnecessary_wraps, reason = "Windows reads the process list whole, and that read can fail")]
pub(crate) fn listing() -> io::Result<Listing> {
    Ok(Listing)
}

fn trim_exe(name: &str) -> &str {
    let cut = name.len().saturating_sub(EXE.len());
    match (name.get(..cut), name.get(cut..)) {
        (Some(stem), Some(extension)) if !stem.is_empty() && extension.eq_ignore_ascii_case(EXE) => stem,
        _ => name,
    }
}

pub(crate) fn reap(mut child: Child) {
    thread::spawn(move || child.wait());
}

pub(crate) fn stop(mut child: Child) {
    let _ = child.kill();
    reap(child);
}

pub(crate) fn output_within(command: &mut Command, timeout: Duration) -> io::Result<Option<Output>> {
    let deadline = Instant::now() + timeout;
    let mut child = Grouped::spawn(command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()))?;
    let (stdout, stderr) = (drain(child.stdout.take()), drain(child.stderr.take()));
    let left = || deadline.saturating_duration_since(Instant::now());
    let output = wait_until(&mut child, deadline)?.and_then(|status| {
        let stdout = stdout.recv_timeout(left()).ok()?;
        let stderr = stderr.recv_timeout(left()).ok()?;
        Some(Output { status, stdout, stderr })
    });
    if output.is_none() {
        kill_group(&mut child);
    }
    Ok(output)
}

pub(crate) fn kill_group(child: &mut Grouped) {
    child.signal(Signal::KILL);
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(unix)]
pub(crate) fn signal_group(leader: i32, signal: Signal) {
    if let Some(group) = Pid::from_raw(leader) {
        let _ = kill_process_group(group, signal);
    }
}

#[cfg(not(unix))]
pub(crate) fn signal_group(leader: i32, signal: Signal) {
    let members: Vec<i32> = std::iter::once(leader).chain(descendants(leader)).collect();
    for pid in members.into_iter().filter_map(Pid::from_raw) {
        let _ = kill_process(pid, signal);
    }
}

thread_local! {
    static DEADLINE: Cell<Option<Instant>> = const { Cell::new(None) };
}

pub(crate) fn set_deadline(at: Option<Instant>) {
    DEADLINE.with(|slot| slot.set(at));
}

pub(crate) fn time_left(default: Duration) -> Duration {
    DEADLINE.with(|slot| slot.get().map_or(default, |at| at.saturating_duration_since(Instant::now())))
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

pub(crate) fn wait_until(child: &mut Child, deadline: Instant) -> io::Result<Option<ExitStatus>> {
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

pub(crate) fn running(pid: i32, started: Option<u64>) -> bool {
    alive(pid) && !imp::zombie(pid) && (started.is_none() || start_stamp(pid) == started)
}

pub(crate) fn tree(pid: i32) -> Vec<(i32, i32)> {
    let mut found: Vec<(i32, i32)> = children(pid).into_iter().map(|child| (child, pid)).collect();
    let mut at = 0;
    while at < found.len() && found.len() < MAX_DESCENDANTS {
        let parent = found[at].0;
        found.extend(children(parent).into_iter().map(|child| (child, parent)));
        at += 1;
    }
    found.truncate(MAX_DESCENDANTS);
    found
}

pub(crate) fn descendants(pid: i32) -> Vec<i32> {
    tree(pid).into_iter().map(|(child, _)| child).collect()
}

pub(crate) fn command_line(program: Option<&str>, args: &[String], width: usize) -> String {
    let first = args.first().and_then(|arg| Path::new(arg).file_name()?.to_str());
    let name = program.or(first).unwrap_or("?");
    let line = std::iter::once(name).chain(args.iter().skip(1).map(String::as_str)).collect::<Vec<_>>().join(" ");
    text::truncate_right(&line, width)
}

#[cfg(any(target_os = "linux", target_os = "macos", windows, test))]
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

#[cfg(any(target_os = "linux", test))]
fn stat_field(stat: &str, index: usize) -> Option<&str> {
    stat.rsplit_once(')')?.1.split_whitespace().nth(index)
}

#[cfg(any(target_os = "linux", test))]
fn stat_cpu_time(stat: &str, ticks_per_second: u64) -> Option<Duration> {
    let user: u64 = stat_field(stat, 11)?.parse().ok()?;
    let system: u64 = stat_field(stat, 12)?.parse().ok()?;
    let nanos = user.saturating_add(system).saturating_mul(1_000_000_000) / ticks_per_second.max(1);
    Some(Duration::from_nanos(nanos))
}

#[cfg(any(target_os = "linux", test))]
fn stat_is_zombie(stat: &str) -> bool {
    matches!(stat_field(stat, 0), Some("Z" | "X"))
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

#[cfg(any(windows, test))]
fn split_command_line(line: &str) -> Vec<String> {
    let (program, rest) = match line.strip_prefix('"') {
        Some(quoted) => quoted.split_once('"').unwrap_or((quoted, "")),
        None => line.split_once([' ', '\t']).unwrap_or((line, "")),
    };
    std::iter::once(program.to_string()).chain(arguments(rest)).filter(|arg| !arg.is_empty()).collect()
}

#[cfg(any(windows, test))]
fn arguments(rest: &str) -> Vec<String> {
    let (mut args, mut arg, mut quoted) = (Vec::new(), String::new(), false);
    let mut chars = rest.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => backslashes(&mut chars, &mut arg),
            '"' if quoted && chars.next_if_eq(&'"').is_some() => arg.push('"'),
            '"' => quoted = !quoted,
            ' ' | '\t' if !quoted => args.push(std::mem::take(&mut arg)),
            c => arg.push(c),
        }
    }
    args.push(arg);
    args
}

#[cfg(any(windows, test))]
fn backslashes(chars: &mut std::iter::Peekable<std::str::Chars<'_>>, arg: &mut String) {
    let mut count = 1;
    while chars.next_if_eq(&'\\').is_some() {
        count += 1;
    }
    if chars.peek() != Some(&'"') {
        arg.extend(std::iter::repeat_n('\\', count));
        return;
    }
    arg.extend(std::iter::repeat_n('\\', count / 2));
    if count % 2 == 1 {
        arg.extend(chars.next());
    }
}

#[cfg(any(windows, test))]
fn environment_block(units: &[u16]) -> Vec<(String, String)> {
    let end = units.windows(2).position(|pair| pair == [0, 0]);
    let text = String::from_utf16_lossy(&units[..end.unwrap_or(units.len())]);
    let mut entries: Vec<&str> = text.split('\0').take_while(|entry| !entry.is_empty()).collect();
    if end.is_none() && !text.ends_with('\0') {
        entries.pop();
    }
    vars(entries.into_iter().map(str::as_bytes))
}

#[cfg(any(windows, test))]
const FILETIME_AT_UNIX_EPOCH: u64 = 116_444_736_000_000_000;

#[cfg(any(windows, test))]
const FILETIME_TICKS_PER_MICRO: u64 = 10;

#[cfg(any(windows, test))]
const NANOS_PER_FILETIME_TICK: u64 = 100;

#[cfg(any(windows, test))]
pub(crate) fn filetime_micros(ticks: u64) -> Option<u64> {
    Some(ticks.checked_sub(FILETIME_AT_UNIX_EPOCH)? / FILETIME_TICKS_PER_MICRO)
}

#[cfg(any(windows, test))]
fn filetime_duration(ticks: u64) -> Duration {
    Duration::from_nanos(ticks.saturating_mul(NANOS_PER_FILETIME_TICK))
}

#[cfg(any(windows, test))]
fn still_listed(listed: Option<u64>, now: Option<u64>) -> bool {
    match (listed, now) {
        (Some(listed), Some(now)) => listed == now,
        _ => true,
    }
}

#[cfg(any(windows, test))]
pub(crate) fn born_before(parent: Option<u64>, child: Option<u64>) -> bool {
    match (parent, child) {
        (Some(parent), Some(child)) => parent <= child,
        _ => true,
    }
}

#[cfg(target_os = "linux")]
mod imp {
    use super::{Duration, PathBuf, UnixStream, io, stat_cpu_time, stat_field, stat_is_zombie, status_footprint, vars};

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

    fn stat(pid: i32) -> Option<String> {
        std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()
    }

    pub(crate) fn started(pid: i32) -> Option<u64> {
        stat_field(&stat(pid)?, 19)?.parse().ok()
    }

    pub(crate) fn cpu_time(pid: i32) -> Option<Duration> {
        stat_cpu_time(&stat(pid)?, rustix::param::clock_ticks_per_second())
    }

    pub(crate) fn zombie(pid: i32) -> bool {
        stat(pid).is_none_or(|stat| stat_is_zombie(&stat))
    }

    pub(crate) fn start_stamp(pid: i32) -> Option<u64> {
        started(pid)
    }

    pub(crate) fn parent(pid: i32) -> Option<i32> {
        stat_field(&stat(pid)?, 1)?.parse().ok()
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use std::ffi::{CStr, OsStr, c_int};
    use std::os::fd::AsRawFd;
    use std::os::unix::ffi::OsStrExt;
    use std::ptr::null_mut;
    use std::sync::OnceLock;

    use super::{Duration, PathBuf, UnixStream, io, procargs, procenv};

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

    #[repr(C)]
    struct Timebase {
        numer: u32,
        denom: u32,
    }

    unsafe extern "C" {
        fn mach_timebase_info(info: *mut Timebase) -> c_int;
    }

    fn timebase() -> (u64, u64) {
        static TIMEBASE: OnceLock<(u64, u64)> = OnceLock::new();
        *TIMEBASE.get_or_init(|| {
            let mut info = Timebase { numer: 0, denom: 0 };
            let read = unsafe { mach_timebase_info(&raw mut info) };
            if read == 0 && info.denom != 0 { (u64::from(info.numer), u64::from(info.denom)) } else { (1, 1) }
        })
    }

    pub(crate) fn cpu_time(pid: i32) -> Option<Duration> {
        let mut info: libc::rusage_info_v0 = unsafe { std::mem::zeroed() };
        let read = unsafe { libc::proc_pid_rusage(pid, libc::RUSAGE_INFO_V0, (&raw mut info).cast()) };
        if read != 0 {
            return None;
        }
        let (numer, denom) = timebase();
        let ticks = u128::from(info.ri_user_time) + u128::from(info.ri_system_time);
        let nanos = ticks * u128::from(numer) / u128::from(denom);
        Some(Duration::from_nanos(u64::try_from(nanos).unwrap_or(u64::MAX)))
    }

    pub(crate) fn zombie(pid: i32) -> bool {
        let info: Option<libc::proc_bsdinfo> = pid_info(pid, libc::PROC_PIDTBSDINFO);
        info.is_none_or(|info| info.pbi_status == libc::SZOMB)
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

    pub(crate) fn start_stamp(pid: i32) -> Option<u64> {
        let info: libc::proc_bsdinfo = pid_info(pid, libc::PROC_PIDTBSDINFO)?;
        Some(info.pbi_start_tvsec.saturating_mul(1_000_000).saturating_add(info.pbi_start_tvusec))
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
mod unread {
    use super::{PathBuf, UnixStream, io};

    pub(crate) fn open_files(_pid: i32) -> Vec<PathBuf> {
        Vec::new()
    }

    #[cfg(windows)]
    pub(crate) fn peer_uid(socket: &UnixStream) -> io::Result<u32> {
        crate::os::socket::peer_uid(socket)
    }

    #[cfg(not(windows))]
    pub(crate) fn peer_uid(_socket: &UnixStream) -> io::Result<u32> {
        Err(io::ErrorKind::Unsupported.into())
    }
}

#[cfg(windows)]
mod imp {
    use std::cell::RefCell;
    use std::ffi::OsString;
    use std::mem::offset_of;
    use std::os::windows::ffi::OsStringExt;
    use std::ptr;
    use std::sync::Arc;

    use parking_lot::Mutex;
    use windows_sys::Wdk::System::SystemInformation::{NtQuerySystemInformation, SystemProcessInformation};
    use windows_sys::Wdk::System::Threading::{NtQueryInformationProcess, ProcessBasicInformation};
    use windows_sys::Win32::Foundation::{
        CloseHandle, FILETIME, HANDLE, INVALID_HANDLE_VALUE, NTSTATUS, STATUS_INFO_LENGTH_MISMATCH, UNICODE_STRING,
    };
    use windows_sys::Win32::System::Diagnostics::Debug::ReadProcessMemory;
    use windows_sys::Win32::System::Memory::{MEMORY_BASIC_INFORMATION, VirtualQueryEx};
    use windows_sys::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX};
    use windows_sys::Win32::System::Threading::{
        GetProcessTimes, OpenProcess, PEB, PROCESS_ACCESS_RIGHTS, PROCESS_BASIC_INFORMATION, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_VM_READ, QueryFullProcessImageNameW, RTL_USER_PROCESS_PARAMETERS,
    };
    use windows_sys::Win32::System::WindowsProgramming::SYSTEM_PROCESS_INFORMATION;

    use super::{
        Duration, Instant, PathBuf, alive, born_before, environment_block, filetime_duration, filetime_micros, io,
        program_name, split_command_line, still_listed,
    };

    pub(crate) use super::unread::{open_files, peer_uid};

    const ENVIRONMENT_LIMIT: usize = 1 << 20;
    const IMAGE_PATH_LIMIT: usize = 1024;
    const SNAPSHOT_FOR: Duration = Duration::from_millis(50);
    const LIST_FIRST_SIZE: usize = 1 << 19;
    const LIST_TRIES: usize = 4;
    const CREATE_TIME_IN_RESERVED: usize = 24;
    const NEXT: usize = offset_of!(SYSTEM_PROCESS_INFORMATION, NextEntryOffset);
    const CREATED: usize = offset_of!(SYSTEM_PROCESS_INFORMATION, Reserved1) + CREATE_TIME_IN_RESERVED;
    const NAME_LENGTH: usize = offset_of!(SYSTEM_PROCESS_INFORMATION, ImageName) + offset_of!(UNICODE_STRING, Length);
    const NAME_UNITS: usize = offset_of!(SYSTEM_PROCESS_INFORMATION, ImageName) + offset_of!(UNICODE_STRING, Buffer);
    const PID: usize = offset_of!(SYSTEM_PROCESS_INFORMATION, UniqueProcessId);
    const PARENT: usize = offset_of!(SYSTEM_PROCESS_INFORMATION, Reserved2);

    static SNAPSHOT: Mutex<Option<(Instant, Arc<[Entry]>)>> = Mutex::new(None);

    thread_local! {
        static PINNED: RefCell<Option<Arc<[Entry]>>> = const { RefCell::new(None) };
    }

    struct Owned(HANDLE);

    impl Owned {
        fn new(handle: HANDLE) -> Option<Self> {
            (!handle.is_null() && handle != INVALID_HANDLE_VALUE).then_some(Self(handle))
        }

        fn process(pid: i32, access: PROCESS_ACCESS_RIGHTS) -> Option<Self> {
            let pid = u32::try_from(pid).ok().filter(|pid| *pid != 0)?;
            Self::new(unsafe { OpenProcess(access, 0, pid) })
        }
    }

    impl Drop for Owned {
        fn drop(&mut self) {
            unsafe { CloseHandle(self.0) };
        }
    }

    trait AnyBits: Copy + Default {}

    impl AnyBits for u16 {}
    impl AnyBits for usize {}
    impl AnyBits for Parameters {}

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct Parameters {
        _lengths_and_flags: [u32; 4],
        _console_and_standard_handles: [usize; 5],
        current_directory: UNICODE_STRING,
        _current_directory_handle: usize,
        _dll_path: UNICODE_STRING,
        _image_path: UNICODE_STRING,
        command_line: UNICODE_STRING,
        environment: usize,
    }

    const _: () = assert!(offset_of!(Parameters, command_line) == offset_of!(RTL_USER_PROCESS_PARAMETERS, CommandLine));

    struct Entry {
        pid: i32,
        parent: i32,
        born: Option<u64>,
        name: String,
    }

    impl Entry {
        fn read(bytes: &[u8], base: usize, at: usize) -> Option<Self> {
            let pid = i32::try_from(usize::from_ne_bytes(field(bytes, at + PID)?)).ok()?;
            let parent = i32::try_from(usize::from_ne_bytes(field(bytes, at + PARENT)?)).unwrap_or(0);
            let born = filetime_micros(u64::from_ne_bytes(field(bytes, at + CREATED)?));
            let name = image_name_in(bytes, base, at).unwrap_or_default();
            Some(Self { pid, parent, born, name: program_name(&name).to_string() })
        }

        fn current(&self) -> bool {
            self.pid > 0 && alive(self.pid) && still_listed(self.born, start_stamp(self.pid))
        }
    }

    fn size_u32<T>() -> u32 {
        u32::try_from(size_of::<T>()).unwrap_or(u32::MAX)
    }

    fn field<const N: usize>(bytes: &[u8], at: usize) -> Option<[u8; N]> {
        bytes.get(at..at.checked_add(N)?)?.try_into().ok()
    }

    fn image_name_in(bytes: &[u8], base: usize, at: usize) -> Option<String> {
        let length = usize::from(u16::from_ne_bytes(field(bytes, at + NAME_LENGTH)?));
        let start = usize::from_ne_bytes(field(bytes, at + NAME_UNITS)?).checked_sub(base)?;
        let (pairs, _) = bytes.get(start..start.checked_add(length)?)?.as_chunks::<2>();
        let units: Vec<u16> = pairs.iter().map(|pair| u16::from_ne_bytes(*pair)).collect();
        Some(String::from_utf16_lossy(&units))
    }

    fn as_bytes(words: &[u64]) -> &[u8] {
        unsafe { std::slice::from_raw_parts(words.as_ptr().cast(), size_of_val(words)) }
    }

    fn query(words: &mut [u64]) -> (NTSTATUS, usize) {
        let (length, mut needed) = (u32::try_from(size_of_val(words)).unwrap_or(u32::MAX), 0_u32);
        let into = words.as_mut_ptr().cast();
        let status = unsafe { NtQuerySystemInformation(SystemProcessInformation, into, length, &raw mut needed) };
        (status, usize::try_from(needed).unwrap_or(usize::MAX))
    }

    fn process_list() -> io::Result<Vec<u64>> {
        let mut size = LIST_FIRST_SIZE;
        for _ in 0..LIST_TRIES {
            let mut words = vec![0_u64; size.div_ceil(size_of::<u64>())];
            match query(&mut words) {
                (status, _) if status >= 0 => return Ok(words),
                (STATUS_INFO_LENGTH_MISMATCH, needed) => size = needed.max(size).saturating_mul(2),
                (status, _) => return Err(io::Error::other(format!("could not list the processes: {status:#x}"))),
            }
        }
        Err(io::Error::other("could not list the processes: the list kept growing"))
    }

    fn entries_in(words: &[u64]) -> Vec<Entry> {
        let (bytes, base) = (as_bytes(words), words.as_ptr().addr());
        let mut entries = Vec::new();
        let mut at = Some(0);
        while let Some(start) = at {
            entries.extend(Entry::read(bytes, base, start));
            let next = u32::from_ne_bytes(field(bytes, start + NEXT).unwrap_or_default());
            at = usize::try_from(next).ok().filter(|next| *next > 0).and_then(|next| start.checked_add(next));
        }
        entries
    }

    fn snapshot() -> io::Result<Arc<[Entry]>> {
        let mut last = SNAPSHOT.lock();
        if let Some((at, entries)) = last.as_ref()
            && at.elapsed() < SNAPSHOT_FOR
        {
            return Ok(Arc::clone(entries));
        }
        let entries = entries_in(&process_list()?);
        if entries.is_empty() {
            return Err(io::Error::other("could not list the processes: the list was empty"));
        }
        let entries: Arc<[Entry]> = entries.into();
        *last = Some((Instant::now(), Arc::clone(&entries)));
        Ok(entries)
    }

    fn latest() -> Arc<[Entry]> {
        if let Some(pinned) = PINNED.with(|pinned| pinned.borrow().clone()) {
            return pinned;
        }
        snapshot().unwrap_or_else(|_| {
            SNAPSHOT.lock().as_ref().map_or_else(|| Arc::from(Vec::new()), |(_, entries)| Arc::clone(entries))
        })
    }

    pub(crate) struct Listing(Option<Arc<[Entry]>>);

    impl Drop for Listing {
        fn drop(&mut self) {
            let earlier = self.0.take();
            PINNED.with(|pinned| *pinned.borrow_mut() = earlier);
        }
    }

    pub(crate) fn listing() -> io::Result<Listing> {
        let entries = snapshot()?;
        Ok(Listing(PINNED.with(|pinned| pinned.borrow_mut().replace(entries))))
    }

    fn current(entries: &[Entry], pid: i32) -> Option<&Entry> {
        entries.iter().find(|entry| entry.pid == pid).filter(|entry| entry.current())
    }

    pub(crate) fn all() -> Vec<i32> {
        latest().iter().map(|entry| entry.pid).filter(|pid| *pid > 0).collect()
    }

    pub(crate) fn children(pid: i32) -> Vec<i32> {
        let entries = latest();
        let Some(parent) = current(&entries, pid) else { return Vec::new() };
        let mut pids: Vec<i32> = entries
            .iter()
            .filter(|entry| entry.parent == pid && entry.pid != pid)
            .filter(|entry| born_before(parent.born, entry.born) && entry.current())
            .map(|entry| entry.pid)
            .collect();
        pids.sort_unstable();
        pids
    }

    pub(crate) fn parent(pid: i32) -> Option<i32> {
        let entries = latest();
        let child = current(&entries, pid)?;
        let up = current(&entries, child.parent)?;
        born_before(up.born, child.born).then_some(up.pid)
    }

    pub(crate) fn name(pid: i32) -> Option<String> {
        image_name(pid).or_else(|| current(&latest(), pid).map(|entry| entry.name.clone()))
    }

    fn image_name(pid: i32) -> Option<String> {
        let process = Owned::process(pid, PROCESS_QUERY_LIMITED_INFORMATION)?;
        let mut path = [0_u16; IMAGE_PATH_LIMIT];
        let mut size = u32::try_from(path.len()).ok()?;
        let read =
            unsafe { QueryFullProcessImageNameW(process.0, PROCESS_NAME_WIN32, path.as_mut_ptr(), &raw mut size) };
        if read == 0 {
            return None;
        }
        let path = PathBuf::from(OsString::from_wide(path.get(..usize::try_from(size).ok()?)?));
        Some(program_name(&path.file_name()?.to_string_lossy()).to_string())
    }

    fn read_into<T: AnyBits>(process: &Owned, at: usize, into: &mut [T]) -> bool {
        let size = size_of_val(into);
        let mut done = 0;
        let at = ptr::without_provenance(at);
        let read = size == 0
            || unsafe { ReadProcessMemory(process.0, at, into.as_mut_ptr().cast(), size, &raw mut done) } != 0;
        read && done == size
    }

    fn read<T: AnyBits>(process: &Owned, at: usize) -> Option<T> {
        let mut value = [T::default()];
        read_into(process, at, &mut value).then_some(value[0])
    }

    fn units(process: &Owned, at: usize, count: usize) -> Option<Vec<u16>> {
        let mut units = vec![0_u16; count];
        read_into(process, at, &mut units).then_some(units)
    }

    fn text(process: &Owned, string: UNICODE_STRING) -> Option<Vec<u16>> {
        units(process, string.Buffer.addr(), usize::from(string.Length / 2))
    }

    fn basic_information(process: &Owned) -> Option<PROCESS_BASIC_INFORMATION> {
        let mut basic = PROCESS_BASIC_INFORMATION::default();
        let (into, size) = ((&raw mut basic).cast(), size_u32::<PROCESS_BASIC_INFORMATION>());
        let status =
            unsafe { NtQueryInformationProcess(process.0, ProcessBasicInformation, into, size, ptr::null_mut()) };
        (status >= 0).then_some(basic)
    }

    fn region(process: &Owned, at: usize) -> Option<MEMORY_BASIC_INFORMATION> {
        let mut region = MEMORY_BASIC_INFORMATION::default();
        let size = size_of::<MEMORY_BASIC_INFORMATION>();
        let written = unsafe { VirtualQueryEx(process.0, ptr::without_provenance(at), &raw mut region, size) };
        (written == size).then_some(region)
    }

    fn with_parameters<T>(pid: i32, read_with: impl FnOnce(&Owned, Parameters) -> Option<T>) -> Option<T> {
        let process = Owned::process(pid, PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ)?;
        let peb = basic_information(&process)?.PebBaseAddress.addr();
        let at: usize = read(&process, peb.checked_add(offset_of!(PEB, ProcessParameters))?)?;
        read_with(&process, read(&process, at)?)
    }

    fn environment(process: &Owned, at: usize) -> Option<Vec<u16>> {
        let region = region(process, at)?;
        let end = region.BaseAddress.addr().checked_add(region.RegionSize)?;
        units(process, at, end.checked_sub(at)?.min(ENVIRONMENT_LIMIT) / 2)
    }

    pub(crate) fn args(pid: i32) -> Vec<String> {
        let line = with_parameters(pid, |process, parameters| text(process, parameters.command_line));
        line.map(|line| split_command_line(&String::from_utf16_lossy(&line))).unwrap_or_default()
    }

    pub(crate) fn env(pid: i32) -> Vec<(String, String)> {
        let block = with_parameters(pid, |process, parameters| environment(process, parameters.environment));
        block.map(|block| environment_block(&block)).unwrap_or_default()
    }

    pub(crate) fn cwd(pid: i32) -> Option<PathBuf> {
        let units = with_parameters(pid, |process, parameters| text(process, parameters.current_directory))?;
        let path = PathBuf::from(OsString::from_wide(&units));
        (!units.is_empty()).then(|| path.components().collect())
    }

    pub(crate) fn footprint(pid: i32) -> Option<u64> {
        let process = Owned::process(pid, PROCESS_QUERY_LIMITED_INFORMATION)?;
        let size = size_u32::<PROCESS_MEMORY_COUNTERS_EX>();
        let mut counters = PROCESS_MEMORY_COUNTERS_EX { cb: size, ..PROCESS_MEMORY_COUNTERS_EX::default() };
        let read = unsafe { GetProcessMemoryInfo(process.0, (&raw mut counters).cast(), size) };
        (read != 0).then(|| u64::try_from(counters.PrivateUsage).unwrap_or(u64::MAX))
    }

    fn ticks(time: FILETIME) -> u64 {
        (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime)
    }

    fn times(pid: i32) -> Option<(u64, u64)> {
        let process = Owned::process(pid, PROCESS_QUERY_LIMITED_INFORMATION)?;
        let [mut created, mut exited, mut kernel, mut user] = [FILETIME::default(); 4];
        let read =
            unsafe { GetProcessTimes(process.0, &raw mut created, &raw mut exited, &raw mut kernel, &raw mut user) };
        (read != 0).then(|| (ticks(created), ticks(kernel).saturating_add(ticks(user))))
    }

    pub(crate) fn cpu_time(pid: i32) -> Option<Duration> {
        times(pid).map(|(_, busy)| filetime_duration(busy))
    }

    pub(crate) fn start_stamp(pid: i32) -> Option<u64> {
        times(pid).and_then(|(created, _)| filetime_micros(created))
    }

    pub(crate) fn started(_pid: i32) -> Option<u64> {
        None
    }

    pub(crate) fn zombie(_pid: i32) -> bool {
        false
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
mod imp {
    use super::{Duration, PathBuf};

    pub(crate) use super::unread::{open_files, peer_uid};

    pub(crate) fn all() -> Vec<i32> {
        Vec::new()
    }

    pub(crate) fn children(_pid: i32) -> Vec<i32> {
        Vec::new()
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

    pub(crate) fn cpu_time(_pid: i32) -> Option<Duration> {
        None
    }

    pub(crate) fn zombie(_pid: i32) -> bool {
        true
    }

    pub(crate) fn parent(_pid: i32) -> Option<i32> {
        None
    }

    pub(crate) fn start_stamp(_pid: i32) -> Option<u64> {
        None
    }
}

#[cfg(test)]
mod tests;
