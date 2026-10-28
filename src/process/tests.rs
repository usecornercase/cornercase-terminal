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

mod parent {
    use super::*;
    use crate::test_util::Family;

    #[test]
    fn is_the_process_that_started_it() {
        let family = Family::new();
        let child = family.child().expect("a child");
        assert_eq!((parent(family.pid()), parent(child)), (Some(this_pid()), Some(family.pid())));
    }

    #[test]
    fn is_unknown_for_a_process_that_exited() {
        assert_eq!(parent(exited_pid()), None);
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

mod start_stamp {
    use super::*;

    #[test]
    fn stays_the_same_for_a_process_and_differs_for_a_later_one() {
        let ours = start_stamp(this_pid()).expect("a stamp");
        assert_eq!(start_stamp(this_pid()), Some(ours));
        let later = crate::test_util::Running::spawn(Command::new("/bin/sleep").arg("30"));
        assert!(start_stamp(later.pid()).is_some_and(|stamp| stamp > ours));
    }

    #[test]
    fn is_unknown_for_a_process_that_exited() {
        assert_eq!(start_stamp(exited_pid()), None);
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

    fn gives_up_quickly(script: &str) {
        let started = Instant::now();
        let mut command = Command::new("/bin/sh");
        command.args(["-c", script]);

        let output = output_within(&mut command, Duration::from_millis(300)).expect("ran");

        assert!(output.is_none());
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn kills_a_command_that_hangs() {
        gives_up_quickly("exec sleep 30");
    }

    #[test]
    fn a_child_left_holding_the_output_is_stopped_too() {
        let tmp = crate::test_util::TempDir::new();
        let pid_file = tmp.path().join("pid");

        gives_up_quickly(&format!("sleep 30 & echo $! > '{}'; exit 0", pid_file.display()));

        let pid: i32 = std::fs::read_to_string(&pid_file).expect("pid").trim().parse().expect("a pid");
        crate::test_util::wait_until("the child it left is gone", || {
            !alive(pid) || name(pid).is_none_or(|n| n != "sleep")
        });
    }

    #[test]
    fn a_missing_program_is_an_error() {
        let error = output_within(&mut Command::new("/nonexistent/program"), Duration::from_secs(1)).expect_err("none");

        assert_eq!(error.kind(), io::ErrorKind::NotFound);
    }
}

mod stat {
    use super::*;

    const STAT: &str =
        "4210 (npm run (dev) x) S 4200 4210 4200 34816 4210 4194304 1000 0 0 0 150 50 0 0 20 0 1 0 81240 1000 100";

    #[test]
    fn cpu_time_adds_user_and_system_ticks_even_when_the_name_holds_spaces_and_parentheses() {
        assert_eq!(stat_cpu_time(STAT, 100), Some(Duration::from_secs(2)));
    }

    #[test]
    fn the_state_is_the_first_field_after_the_name() {
        assert!(!stat_is_zombie(STAT));
        assert!(stat_is_zombie(&STAT.replace(") S ", ") Z ")));
        assert!(stat_is_zombie(&STAT.replace(") S ", ") X ")));
    }
}

mod cpu_time {
    use super::*;
    use crate::test_util::{Running, wait_until};

    #[test]
    fn grows_for_a_busy_process_by_a_fair_share_of_the_wall_time() {
        let busy = Running::spawn(Command::new("/bin/sh").args(["-c", "while :; do :; done"]));
        let first = Instant::now();
        let before = cpu_time(busy.pid()).expect("cpu time");
        wait_until("the busy loop uses cpu", || {
            cpu_time(busy.pid()).is_some_and(|now| now.saturating_sub(before) > Duration::from_millis(100))
        });
        let wall = first.elapsed();
        let used = cpu_time(busy.pid()).expect("cpu time").saturating_sub(before);
        assert!(used.as_secs_f64() >= wall.as_secs_f64() * 0.25, "{used:?} of {wall:?}");
    }

    #[test]
    fn is_unknown_for_a_process_that_exited() {
        assert_eq!(cpu_time(exited_pid()), None);
    }
}

mod running {
    use super::*;
    use crate::test_util::{Running, wait_until};

    #[test]
    fn is_true_for_a_live_process_with_its_own_start_and_false_for_another_one() {
        let sleeper = Running::spawn(Command::new("/bin/sleep").arg("30"));
        let stamp = start_stamp(sleeper.pid());
        assert!(running(sleeper.pid(), stamp));
        assert!(running(sleeper.pid(), None));
        assert!(!running(sleeper.pid(), stamp.map(|stamp| stamp + 1)));
    }

    #[test]
    fn is_false_for_a_zombie_and_for_a_process_that_was_reaped() {
        let mut child = Command::new("/bin/sh").args(["-c", "exit 0"]).spawn().expect("spawn sh");
        let pid = i32::try_from(child.id()).expect("a pid");
        wait_until("the child is a zombie", || alive(pid) && !running(pid, None));
        assert!(alive(pid));
        child.wait().expect("reap");
        assert!(!running(pid, None));
    }
}

mod tree {
    use super::*;
    use crate::test_util::Family;

    #[test]
    fn pairs_each_descendant_with_its_parent_and_matches_descendants() {
        let family = Family::new();
        let (child, grandchild) = (family.child().expect("a child"), family.grandchild().expect("a grandchild"));
        assert_eq!(tree(family.pid()), [(child, family.pid()), (grandchild, child)]);
        assert_eq!(descendants(family.pid()), [child, grandchild]);
    }
}

mod command_line {
    use super::*;

    #[test]
    fn shows_the_name_then_the_arguments_cut_to_the_width() {
        let args: Vec<String> = ["/usr/bin/node", "vite", "--port", "5173"].iter().map(ToString::to_string).collect();
        assert_eq!(command_line(Some("node"), &args, 40), "node vite --port 5173");
        assert_eq!(command_line(None, &args, 40), "node vite --port 5173");
        assert_eq!(command_line(Some("node"), &args, 10), "node vite…");
    }
}
