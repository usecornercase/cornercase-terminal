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

mod started {
    use super::*;

    #[cfg(target_os = "linux")]
    #[test]
    fn is_field_22_of_stat() {
        let stat = std::fs::read_to_string("/proc/self/stat").expect("stat");
        let rest = stat.rsplit_once(')').expect("comm").1;
        let field: u64 = rest.split_whitespace().nth(19).expect("start time").parse().expect("number");
        assert_eq!(started(this_pid()), Some(field));
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn is_unknown_without_a_proc_stat() {
        assert_eq!(started(this_pid()), None);
    }
}

mod footprint {
    use std::ptr::null_mut;

    use rustix::mm::{MapFlags, ProtFlags, mmap, mmap_anonymous, munmap};

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
        let map = unsafe { mmap_anonymous(null_mut(), SIZE, ProtFlags::READ | ProtFlags::WRITE, MapFlags::PRIVATE) }
            .expect("map");
        let before = footprint(this_pid()).expect("before");

        for at in (0..SIZE).step_by(4096) {
            unsafe { map.cast::<u8>().add(at).write_volatile(1) };
        }
        let after = footprint(this_pid()).expect("after");
        unsafe { munmap(map, SIZE) }.expect("unmap");

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
        let read: usize =
            (0..SIZE).step_by(4096).map(|at| usize::from(unsafe { map.cast::<u8>().add(at).read_volatile() })).sum();
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

mod output_within {
    use std::process::Command;
    use std::time::{Duration, Instant};

    use super::*;

    #[test]
    fn gives_what_the_command_printed_and_its_status() {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "echo out; echo err >&2; exit 3"]);

        let output = output_within(&mut command, Duration::from_secs(5)).expect("ran").expect("in time");

        assert_eq!((output.stdout.as_slice(), output.stderr.as_slice()), (&b"out\n"[..], &b"err\n"[..]));
        assert_eq!(output.status.code(), Some(3));
    }

    #[test]
    fn kills_a_command_that_hangs() {
        let started = Instant::now();
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "exec sleep 30"]);

        let output = output_within(&mut command, Duration::from_millis(300)).expect("ran");

        assert!(output.is_none());
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn a_missing_program_is_an_error() {
        let error = output_within(&mut Command::new("/nonexistent/program"), Duration::from_secs(1)).expect_err("none");

        assert_eq!(error.kind(), io::ErrorKind::NotFound);
    }
}
