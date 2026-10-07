use std::io;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;

use rustix::io::Errno;
use rustix::process::{Pid, test_kill_process};

pub use imp::{all, args, children, cwd, env, footprint, name, open_files, peer_uid};

const MAX_DESCENDANTS: usize = 256;

pub fn alive(pid: i32) -> bool {
    Pid::from_raw(pid).is_some_and(|pid| !matches!(test_kill_process(pid), Err(Errno::SRCH)))
}

pub fn descendants(pid: i32) -> Vec<i32> {
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

    pub fn all() -> Vec<i32> {
        std::fs::read_dir("/proc")
            .into_iter()
            .flatten()
            .filter_map(|entry| entry.ok()?.file_name().to_str()?.parse().ok())
            .collect()
    }

    pub fn children(pid: i32) -> Vec<i32> {
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

    pub fn open_files(pid: i32) -> Vec<PathBuf> {
        std::fs::read_dir(format!("/proc/{pid}/fd"))
            .into_iter()
            .flatten()
            .filter_map(|entry| std::fs::read_link(entry.ok()?.path()).ok())
            .collect()
    }

    pub fn peer_uid(socket: &UnixStream) -> io::Result<u32> {
        Ok(rustix::net::sockopt::socket_peercred(socket)?.uid.as_raw())
    }

    pub fn cwd(pid: i32) -> Option<PathBuf> {
        std::fs::read_link(format!("/proc/{pid}/cwd")).ok()
    }

    pub fn name(pid: i32) -> Option<String> {
        let comm = std::fs::read_to_string(format!("/proc/{pid}/comm")).ok()?;
        Some(comm.trim().to_string())
    }

    pub fn args(pid: i32) -> Vec<String> {
        let cmdline = std::fs::read(format!("/proc/{pid}/cmdline")).unwrap_or_default();
        cmdline.split(|b| *b == 0).filter(|a| !a.is_empty()).map(|a| String::from_utf8_lossy(a).into_owned()).collect()
    }

    pub fn env(pid: i32) -> Vec<(String, String)> {
        let environ = std::fs::read(format!("/proc/{pid}/environ")).unwrap_or_default();
        vars(environ.split(|b| *b == 0))
    }

    pub fn footprint(pid: i32) -> Option<u64> {
        status_footprint(&std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?)
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use std::ffi::{CStr, OsStr, c_int};
    use std::os::fd::AsRawFd;
    use std::os::unix::ffi::OsStrExt;
    use std::ptr::null_mut;

    use super::{PathBuf, UnixStream, io, procargs, procenv};

    pub fn all() -> Vec<i32> {
        let count = unsafe { libc::proc_listallpids(null_mut(), 0) };
        listed(usize::try_from(count).unwrap_or(0) + 64, |buffer, size| unsafe { libc::proc_listallpids(buffer, size) })
    }

    pub fn children(pid: i32) -> Vec<i32> {
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

    pub fn open_files(pid: i32) -> Vec<PathBuf> {
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

    pub fn peer_uid(socket: &UnixStream) -> io::Result<u32> {
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

    pub fn cwd(pid: i32) -> Option<PathBuf> {
        let info: libc::proc_vnodepathinfo = pid_info(pid, libc::PROC_PIDVNODEPATHINFO)?;
        let bytes: Vec<u8> = info.pvi_cdir.vip_path.iter().flatten().map(|c| c.cast_unsigned()).collect();
        let path = CStr::from_bytes_until_nul(&bytes).ok()?;
        (!path.is_empty()).then(|| PathBuf::from(OsStr::from_bytes(path.to_bytes())))
    }

    pub fn footprint(pid: i32) -> Option<u64> {
        let mut info: libc::rusage_info_v0 = unsafe { std::mem::zeroed() };
        let read = unsafe { libc::proc_pid_rusage(pid, libc::RUSAGE_INFO_V0, (&raw mut info).cast()) };
        (read == 0).then_some(info.ri_phys_footprint)
    }

    pub fn name(pid: i32) -> Option<String> {
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

    pub fn args(pid: i32) -> Vec<String> {
        procargs_buffer(pid).map(|buf| procargs(&buf)).unwrap_or_default()
    }

    pub fn env(pid: i32) -> Vec<(String, String)> {
        procargs_buffer(pid).map(|buf| procenv(&buf)).unwrap_or_default()
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
mod imp {
    use super::{PathBuf, UnixStream, io};

    pub fn all() -> Vec<i32> {
        Vec::new()
    }

    pub fn children(_pid: i32) -> Vec<i32> {
        Vec::new()
    }

    pub fn open_files(_pid: i32) -> Vec<PathBuf> {
        Vec::new()
    }

    pub fn peer_uid(_socket: &UnixStream) -> io::Result<u32> {
        Err(io::ErrorKind::Unsupported.into())
    }

    pub fn cwd(_pid: i32) -> Option<PathBuf> {
        None
    }

    pub fn name(_pid: i32) -> Option<String> {
        None
    }

    pub fn args(_pid: i32) -> Vec<String> {
        Vec::new()
    }

    pub fn env(_pid: i32) -> Vec<(String, String)> {
        Vec::new()
    }

    pub fn footprint(_pid: i32) -> Option<u64> {
        None
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::test_util::{exited_pid, this_pid};

    mod alive {
        use super::*;

        #[test]
        fn is_true_for_this_process() {
            assert!(alive(this_pid()));
        }

        #[test]
        fn is_false_for_a_process_that_exited() {
            assert!(!alive(exited_pid()));
        }
    }

    mod descendants {
        use super::*;
        use crate::test_util::Family;

        #[test]
        fn reach_the_children_of_children() {
            let family = Family::new();
            let expected = [family.child().expect("a child"), family.grandchild().expect("a grandchild")];

            assert_eq!(descendants(family.pid()), expected);
        }

        #[test]
        fn are_none_for_a_process_that_exited() {
            assert_eq!(descendants(exited_pid()), Vec::<i32>::new());
        }
    }

    mod footprint {
        use std::ptr::null_mut;

        use rustix::mm::{MapFlags, ProtFlags, mmap, munmap};

        use super::*;
        use crate::test_util::TempDir;

        const SIZE: usize = 64 << 20;
        const HALF: u64 = 32 << 20;

        #[test]
        fn is_some_memory_for_this_process() {
            assert!(footprint(this_pid()).is_some_and(|bytes| bytes > 0));
        }

        #[test]
        fn counts_memory_the_process_writes() {
            let before = footprint(this_pid()).expect("before");

            let written = std::hint::black_box(vec![1_u8; SIZE]);
            let after = footprint(this_pid()).expect("after");
            drop(written);

            assert!(after.saturating_sub(before) > HALF, "{before} -> {after}");
        }

        #[test]
        fn leaves_out_a_file_the_process_only_reads() {
            let dir = TempDir::new();
            let path = dir.path().join("mapped");
            std::fs::write(&path, vec![1_u8; SIZE]).expect("write the file");
            let file = std::fs::File::open(&path).expect("open the file");
            let before = footprint(this_pid()).expect("before");

            let map = unsafe { mmap(null_mut(), SIZE, ProtFlags::READ, MapFlags::SHARED, &file, 0) }.expect("map");
            let read: usize = (0..SIZE)
                .step_by(4096)
                .map(|at| usize::from(unsafe { map.cast::<u8>().add(at).read_volatile() }))
                .sum();
            let after = footprint(this_pid()).expect("after");
            unsafe { munmap(map, SIZE) }.expect("unmap");

            assert_eq!(read, SIZE / 4096);
            assert!(after.saturating_sub(before) < HALF, "{before} -> {after}");
        }

        #[test]
        fn is_none_for_a_process_that_exited() {
            assert_eq!(footprint(exited_pid()), None);
        }
    }

    mod this_process {
        use super::*;

        #[test]
        fn has_the_test_binary_as_its_first_argument() {
            let first = args(this_pid()).into_iter().next().expect("an argument");
            let exe = std::env::current_exe().expect("current exe");

            assert_eq!(Path::new(&first).file_name(), exe.file_name());
        }

        #[test]
        fn has_the_current_dir_as_its_cwd() {
            let here = std::env::current_dir().expect("current dir").canonicalize().expect("canonicalize");

            assert_eq!(cwd(this_pid()).map(|p| p.canonicalize().expect("canonicalize")), Some(here));
        }

        #[test]
        fn has_a_name() {
            assert!(name(this_pid()).is_some_and(|n| !n.is_empty()));
        }
    }

    mod env {
        use super::*;
        use crate::test_util::Sleeper;

        #[test]
        fn is_what_the_process_started_with() {
            let sleeper = Sleeper::with_env(&[("CORNERCASE_PROBE", "a=b")]);

            let found = env(sleeper.pid()).into_iter().find(|(key, _)| key == "CORNERCASE_PROBE");

            assert_eq!(found, Some(("CORNERCASE_PROBE".into(), "a=b".into())));
        }

        #[test]
        fn is_empty_for_a_process_that_is_gone() {
            assert_eq!(env(exited_pid()), Vec::new());
        }
    }

    mod procargs {
        use super::*;

        fn buffer(argc: i32, rest: &[u8]) -> Vec<u8> {
            [argc.to_ne_bytes().as_slice(), rest].concat()
        }

        #[test]
        fn skips_the_exec_path_and_its_padding() {
            let buf = buffer(2, b"/usr/bin/node\0\0\0\0node\0gemini\0");

            assert_eq!(procargs(&buf), ["node", "gemini"]);
        }

        #[test]
        fn stops_before_the_environment() {
            let buf = buffer(1, b"/bin/zsh\0\0-zsh\0HOME=/Users/a\0PATH=/bin\0");

            assert_eq!(procargs(&buf), ["-zsh"]);
        }

        #[test]
        fn is_empty_for_a_short_buffer() {
            assert_eq!(procargs(&[1, 0]), Vec::<String>::new());
        }

        #[test]
        fn the_environment_follows_the_arguments() {
            let buf = buffer(1, b"/bin/zsh\0\0-zsh\0HOME=/Users/a\0PATH=/bin\0\0ptr_munge=\0");

            assert_eq!(procenv(&buf), [("HOME".into(), "/Users/a".into()), ("PATH".into(), "/bin".into())]);
        }

        #[test]
        fn arguments_that_look_like_variables_are_not_the_environment() {
            let buf = buffer(2, b"/usr/bin/env\0\0env\0A=1\0B=2\0");

            assert_eq!(procenv(&buf), [("B".to_string(), "2".to_string())]);
        }
    }

    mod status {
        use rstest::rstest;

        use super::*;

        #[rstest]
        #[case::anonymous_memory_and_swap("RssAnon:\t     128 kB\nRssFile:\t    1664 kB\nVmSwap:\t      64 kB\n", Some(192 * 1024))]
        #[case::no_swap_line("Name:\tsleep\nRssAnon:\t     128 kB\n", Some(128 * 1024))]
        #[case::no_anonymous_memory_line("Name:\tkworker\nState:\tI (idle)\n", None)]
        #[case::a_value_that_is_not_a_number("RssAnon:\t     lots kB\n", None)]
        fn footprint_is_anonymous_memory_plus_swap(#[case] text: &str, #[case] expected: Option<u64>) {
            assert_eq!(status_footprint(text), expected);
        }
    }
}
