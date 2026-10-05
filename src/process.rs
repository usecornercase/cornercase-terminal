use std::io;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;

use rustix::io::Errno;
use rustix::process::{Pid, test_kill_process};

pub use imp::{args, cwd, env, name, peer_uid};

pub fn alive(pid: i32) -> bool {
    Pid::from_raw(pid).is_some_and(|pid| !matches!(test_kill_process(pid), Err(Errno::SRCH)))
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
    use super::{PathBuf, UnixStream, io, vars};

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
}

#[cfg(target_os = "macos")]
mod imp {
    use std::ffi::{CStr, OsStr, c_int};
    use std::os::fd::AsRawFd;
    use std::os::unix::ffi::OsStrExt;
    use std::ptr::null_mut;

    use super::{PathBuf, UnixStream, io, procargs, procenv};

    pub fn peer_uid(socket: &UnixStream) -> io::Result<u32> {
        let (mut uid, mut gid) = (0, 0);
        if unsafe { libc::getpeereid(socket.as_raw_fd(), &raw mut uid, &raw mut gid) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(uid)
    }

    pub fn cwd(pid: i32) -> Option<PathBuf> {
        let mut info: libc::proc_vnodepathinfo = unsafe { std::mem::zeroed() };
        let size = c_int::try_from(size_of::<libc::proc_vnodepathinfo>()).ok()?;
        let read = unsafe { libc::proc_pidinfo(pid, libc::PROC_PIDVNODEPATHINFO, 0, (&raw mut info).cast(), size) };
        if read != size {
            return None;
        }
        let bytes: Vec<u8> = info.pvi_cdir.vip_path.iter().flatten().map(|c| c.cast_unsigned()).collect();
        let path = CStr::from_bytes_until_nul(&bytes).ok()?;
        (!path.is_empty()).then(|| PathBuf::from(OsStr::from_bytes(path.to_bytes())))
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
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    fn me() -> i32 {
        i32::try_from(std::process::id()).expect("pid fits in i32")
    }

    fn gone() -> i32 {
        let mut child = std::process::Command::new("/bin/sh").arg("-c").arg("exit 0").spawn().expect("spawn sh");
        child.wait().expect("wait for sh");
        i32::try_from(child.id()).expect("pid fits in i32")
    }

    mod alive {
        use super::*;

        #[test]
        fn is_true_for_this_process() {
            assert!(alive(me()));
        }

        #[test]
        fn is_false_for_a_process_that_exited() {
            assert!(!alive(gone()));
        }
    }

    mod this_process {
        use super::*;

        #[test]
        fn has_the_test_binary_as_its_first_argument() {
            let first = args(me()).into_iter().next().expect("an argument");
            let exe = std::env::current_exe().expect("current exe");

            assert_eq!(Path::new(&first).file_name(), exe.file_name());
        }

        #[test]
        fn has_the_current_dir_as_its_cwd() {
            let here = std::env::current_dir().expect("current dir").canonicalize().expect("canonicalize");

            assert_eq!(cwd(me()).map(|p| p.canonicalize().expect("canonicalize")), Some(here));
        }

        #[test]
        fn has_a_name() {
            assert!(name(me()).is_some_and(|n| !n.is_empty()));
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
            assert_eq!(env(gone()), Vec::new());
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
}
