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
    #[cfg(unix)]
    use crate::test_util::Family;

    #[cfg(unix)]
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
    #[cfg(unix)]
    use crate::test_util::Family;

    #[cfg(unix)]
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

    #[cfg(unix)]
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

#[cfg(unix)]
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
    #[cfg(unix)]
    use crate::test_util::Sleeper;

    #[cfg(unix)]
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
    use std::time::Duration;
    #[cfg(unix)]
    use std::time::Instant;

    use super::*;

    #[cfg(unix)]
    #[test]
    fn gives_what_the_command_printed_and_its_status() {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "echo out; echo err >&2; exit 3"]);

        let output = output_within(&mut command, Duration::from_secs(5)).expect("ran").expect("in time");

        assert_eq!((output.stdout.as_slice(), output.stderr.as_slice()), (&b"out\n"[..], &b"err\n"[..]));
        assert_eq!(output.status.code(), Some(3));
    }

    #[cfg(unix)]
    fn gives_up_quickly(script: &str) {
        let started = Instant::now();
        let mut command = Command::new("/bin/sh");
        command.args(["-c", script]);

        let output = output_within(&mut command, Duration::from_millis(300)).expect("ran");

        assert!(output.is_none());
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    #[cfg(unix)]
    #[test]
    fn kills_a_command_that_hangs() {
        gives_up_quickly("exec sleep 30");
    }

    #[cfg(unix)]
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
    #[cfg(unix)]
    use crate::test_util::{Running, wait_until};

    #[cfg(unix)]
    #[test]
    fn grows_for_a_busy_process_and_never_past_the_wall_time() {
        let busy = Running::spawn(Command::new("/bin/sh").args(["-c", "while :; do :; done"]));
        let first = Instant::now();
        let before = cpu_time(busy.pid()).expect("cpu time");
        wait_until("the busy loop uses cpu", || {
            cpu_time(busy.pid()).is_some_and(|now| now.saturating_sub(before) > Duration::from_millis(100))
        });
        let wall = first.elapsed();
        let used = cpu_time(busy.pid()).expect("cpu time").saturating_sub(before);
        assert!(used > Duration::from_millis(100) && used <= wall + Duration::from_millis(100), "{used:?} of {wall:?}");
    }

    #[test]
    fn is_unknown_for_a_process_that_exited() {
        assert_eq!(cpu_time(exited_pid()), None);
    }
}

#[cfg(unix)]
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

#[cfg(unix)]
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

mod split_command_line {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::a_quoted_program_running_a_script(
        r#""C:\Program Files\nodejs\node.exe" "C:\npm\node_modules\@anthropic-ai\claude-code\cli.js" --resume"#,
        &[r"C:\Program Files\nodejs\node.exe", r"C:\npm\node_modules\@anthropic-ai\claude-code\cli.js", "--resume"]
    )]
    #[case::an_argument_with_spaces(r#"cmd.exe /d /c "ping -n 30 127.0.0.1""#, &["cmd.exe", "/d", "/c", "ping -n 30 127.0.0.1"])]
    #[case::backslashes_stay_unless_a_quote_follows(
        r#"a.exe C:\dir\ "C:\with space\\" x"#,
        &["a.exe", r"C:\dir\", r"C:\with space\", "x"]
    )]
    #[case::escaped_quotes(r#"a.exe say\"hi\" \\\"q"#, &["a.exe", r#"say"hi""#, r#"\"q"#])]
    #[case::a_doubled_quote_inside_quotes(r#"a.exe "a""b""#, &["a.exe", r#"a"b"#])]
    #[case::empty_arguments_and_extra_whitespace("a.exe  \t\"\" b ", &["a.exe", "b"])]
    #[case::a_program_alone("pwsh.exe", &["pwsh.exe"])]
    #[case::a_quoted_program_keeps_its_backslashes(r#""C:\a\" b"#, &[r"C:\a\", "b"])]
    fn follows_the_rules_of_windows_programs(#[case] line: &str, #[case] expected: &[&str]) {
        assert_eq!(split_command_line(line), expected);
    }
}

mod environment_block {
    use super::*;

    #[test]
    fn ends_at_the_empty_entry_and_skips_the_hidden_drive_folders() {
        let block: Vec<u16> = "=C:=C:\\work\0PATH=C:\\bin;D:\\x\0A=b=c\0\0STALE=1\0".encode_utf16().collect();

        assert_eq!(environment_block(&block), [("PATH".into(), "C:\\bin;D:\\x".into()), ("A".into(), "b=c".into())]);
    }

    #[test]
    fn a_block_cut_inside_a_variable_drops_that_variable() {
        let block: Vec<u16> = "A=1\0B=2\0PATH=C:\\b".encode_utf16().collect();

        assert_eq!(environment_block(&block), [("A".into(), "1".into()), ("B".into(), "2".into())]);
    }

    #[test]
    fn a_block_cut_right_after_a_variable_keeps_it() {
        let block: Vec<u16> = "A=1\0B=2\0".encode_utf16().collect();

        assert_eq!(environment_block(&block), [("A".into(), "1".into()), ("B".into(), "2".into())]);
    }
}

mod filetime {
    use super::*;

    #[test]
    fn a_start_is_microseconds_since_the_unix_epoch() {
        assert_eq!(filetime_micros(FILETIME_AT_UNIX_EPOCH + 25), Some(2));
        assert_eq!(filetime_micros(134_381_276_700_310_730), Some(1_793_654_070_031_073));
        assert_eq!(filetime_micros(FILETIME_AT_UNIX_EPOCH - 1), None);
    }

    #[test]
    fn ten_million_ticks_are_one_second() {
        assert_eq!(filetime_duration(10_000_000), Duration::from_secs(1));
        assert_eq!(filetime_duration(1), Duration::from_nanos(100));
    }
}

mod snapshot_checks {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::the_same_start(Some(500), Some(500), true)]
    #[case::another_start_so_a_reused_pid(Some(500), Some(900), false)]
    #[case::a_start_this_user_may_not_read(Some(500), None, true)]
    #[case::a_kernel_process_with_no_listed_start(None, Some(500), true)]
    fn a_listed_entry_answers_for_a_pid_only_while_it_holds_the_same_process(
        #[case] listed: Option<u64>,
        #[case] now: Option<u64>,
        #[case] expected: bool,
    ) {
        assert_eq!(still_listed(listed, now), expected);
    }

    #[rstest]
    #[case::born_first(Some(400), Some(500), true)]
    #[case::born_together(Some(500), Some(500), true)]
    #[case::born_later_so_a_reused_parent_pid(Some(600), Some(500), false)]
    #[case::a_kernel_parent_with_no_listed_start(None, Some(500), true)]
    fn a_listed_parent_counts_only_if_it_was_born_first(
        #[case] parent: Option<u64>,
        #[case] child: Option<u64>,
        #[case] expected: bool,
    ) {
        assert_eq!(born_before(parent, child), expected);
    }
}

mod program_name {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::lower_case("node.exe", "node")]
    #[case::upper_case("PING.EXE", "PING")]
    #[case::no_extension("node", "node")]
    #[case::only_the_extension(".exe", ".exe")]
    #[case::another_extension("claude.cmd", "claude.cmd")]
    #[case::exe_inside_the_name("a.exe.bak", "a.exe.bak")]
    #[case::a_cut_inside_a_character("ñabc", "ñabc")]
    fn trim_exe_drops_a_last_exe_in_any_case(#[case] name: &str, #[case] expected: &str) {
        assert_eq!(trim_exe(name), expected);
    }

    #[test]
    fn drops_the_exe_only_on_windows() {
        assert_eq!(program_name("node.exe"), if cfg!(windows) { "node" } else { "node.exe" });
    }

    #[test]
    fn cmd_and_powershell_are_shells_only_on_windows() {
        assert_eq!((is_shell("cmd"), is_shell("PowerShell"), is_shell("pwsh")), (cfg!(windows), cfg!(windows), true));
    }

    #[test]
    fn names_in_another_case_are_the_same_file_only_on_windows() {
        assert!(same_file_name("claude", "claude"));
        assert_eq!(same_file_name("CLAUDE", "claude"), cfg!(windows));
        assert_eq!(same_file_name("ÉDITEUR.JS", "éditeur.js"), cfg!(windows));
        assert!(!same_file_name("claude", "codex"));
    }

    #[test]
    fn a_shell_named_with_its_exe_is_one_only_on_windows() {
        let names = ["pwsh.exe", "PowerShell.EXE", "cmd.exe", "nu.exe", "bash.exe", "sh.exe"];
        assert!(names.iter().all(|name| is_shell(name) == cfg!(windows)), "{names:?}");
    }
}

#[cfg(windows)]
mod on_windows {
    use std::process::Stdio;
    use std::ptr;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    use windows_sys::Win32::System::Memory::{
        MEM_COMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_READWRITE, VirtualAlloc, VirtualFree,
    };

    use super::*;
    use crate::test_util::{Running, TempDir, wait_until};

    const SIZE: usize = 64 << 20;
    const HALF: u64 = 32 << 20;

    fn ping(dir: &Path) -> Running {
        Running::spawn(
            Command::new("ping")
                .args(["-n", "30", "127.0.0.1"])
                .current_dir(dir)
                .env("CORNERCASE_PROBE", "a=b")
                .stdout(Stdio::null()),
        )
    }

    #[test]
    fn a_child_is_listed_under_this_process_with_its_name_and_arguments() {
        let dir = TempDir::new();
        let ping = ping(dir.path());
        wait_until("a snapshot lists ping", || children(this_pid()).contains(&ping.pid()));

        assert!(all().contains(&ping.pid()));
        assert_eq!(parent(ping.pid()), Some(this_pid()));
        assert!(name(ping.pid()).is_some_and(|name| name.eq_ignore_ascii_case("ping")), "{:?}", name(ping.pid()));
        let args = args(ping.pid());
        assert!(args.first().is_some_and(|program| program.eq_ignore_ascii_case("ping")), "{args:?}");
        assert_eq!(args[1..], ["-n", "30", "127.0.0.1"]);
    }

    #[test]
    fn a_listing_holds_its_thread_to_one_snapshot_until_it_is_dropped() {
        let dir = TempDir::new();
        let pinned = listing().expect("a listing");
        let ping = ping(dir.path());
        let pid = ping.pid();
        wait_until("another thread sees ping", || {
            thread::spawn(move || children(this_pid()).contains(&pid)).join().expect("the other thread")
        });

        assert!(!children(this_pid()).contains(&pid));
        drop(pinned);
        assert!(children(this_pid()).contains(&pid));
    }

    #[test]
    fn a_child_has_the_folder_and_environment_it_started_with() {
        let dir = TempDir::new();
        let ping = ping(dir.path());

        let folder = cwd(ping.pid()).expect("a folder");
        assert_eq!(folder.canonicalize().expect("canonicalize"), dir.path().canonicalize().expect("canonicalize"));
        assert!(!folder.to_string_lossy().ends_with('\\'), "{}", folder.display());
        let probe = env(ping.pid()).into_iter().find(|(key, _)| key == "CORNERCASE_PROBE");
        assert_eq!(probe, Some(("CORNERCASE_PROBE".into(), "a=b".into())));
    }

    #[test]
    fn a_tree_reaches_the_child_of_a_child() {
        let line = r"%SystemRoot%\System32\ping.exe -n 30 127.0.0.1";
        let cmd = Running::spawn(Command::new("cmd.exe").args(["/d", "/c", line]).stdout(Stdio::null()));
        let find_ping = || {
            children(cmd.pid())
                .into_iter()
                .find(|child| name(*child).is_some_and(|name| name.eq_ignore_ascii_case("ping")))
        };
        wait_until("cmd starts ping", || find_ping().is_some());
        let ping = find_ping().expect("ping");

        let found = (parent(ping), args(cmd.pid()), descendants(cmd.pid()), tree(this_pid()));
        let born = start_stamp(ping);
        let stopped = Command::new("taskkill").args(["/f", "/pid", &ping.to_string()]).stdout(Stdio::null()).status();
        assert!(stopped.as_ref().is_ok_and(ExitStatus::success), "taskkill: {stopped:?}");
        wait_until("ping is gone", || !running(ping, born));

        assert_eq!(found.0, Some(cmd.pid()));
        assert_eq!(found.1[1..], ["/d", "/c", line]);
        assert!(found.2.contains(&ping), "{:?}", found.2);
        assert!(found.3.contains(&(ping, cmd.pid())));
    }

    #[test]
    fn a_child_starts_after_this_process_and_keeps_its_start() {
        let dir = TempDir::new();
        let ping = ping(dir.path());
        let stamp = start_stamp(ping.pid()).expect("a stamp");

        assert!(start_stamp(this_pid()).is_some_and(|ours| ours < stamp));
        assert_eq!(start_stamp(ping.pid()), Some(stamp));
        assert!(running(ping.pid(), Some(stamp)));
        assert!(!running(ping.pid(), Some(stamp + 1)));
        assert_eq!(started(ping.pid()), None);
        assert!(footprint(ping.pid()).is_some_and(|bytes| bytes > 0));
    }

    #[test]
    fn a_child_is_alive_until_it_is_killed() {
        let mut child =
            Command::new("ping").args(["-n", "30", "127.0.0.1"]).stdout(Stdio::null()).spawn().expect("spawn ping");
        let pid = i32::try_from(child.id()).expect("a pid");
        assert!(alive(pid) && running(pid, None));

        child.kill().expect("kill");
        child.wait().expect("wait");
        assert!(!alive(pid) && !running(pid, None));
    }

    #[test]
    fn a_process_whose_start_this_user_may_not_read_keeps_its_place_in_the_tree() {
        let unreadable: Vec<i32> = all().into_iter().filter(|pid| alive(*pid) && start_stamp(*pid).is_none()).collect();
        let placed: Vec<(i32, i32)> = unreadable.iter().filter_map(|pid| Some((*pid, parent(*pid)?))).collect();

        assert!(unreadable.is_empty() || !placed.is_empty(), "none of {unreadable:?} has a parent");
        for (pid, up) in &placed {
            assert!(children(*up).contains(pid) || !alive(*pid), "{pid} under {up}");
        }
        assert!(placed.iter().all(|(pid, _)| name(*pid).is_some() || !alive(*pid)));
    }

    #[test]
    fn a_pid_without_a_process_has_nothing_to_read() {
        let pid = exited_pid();

        assert!(!alive(pid));
        assert_eq!((name(pid), parent(pid), children(pid)), (None, None, Vec::new()));
        assert_eq!((args(pid), env(pid), cwd(pid)), (Vec::<String>::new(), Vec::new(), None));
        assert_eq!((footprint(pid), start_stamp(pid), cpu_time(pid)), (None, None, None));
    }

    #[test]
    fn footprint_counts_memory_committed_but_never_touched() {
        let before = footprint(this_pid()).expect("before");
        let memory = unsafe { VirtualAlloc(ptr::null(), SIZE, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE) };
        assert!(!memory.is_null(), "commit {SIZE} bytes");
        let after = footprint(this_pid()).expect("after");
        unsafe { VirtualFree(memory, 0, MEM_RELEASE) };

        assert!(after.saturating_sub(before) > HALF, "{before} -> {after}");
    }

    #[test]
    fn a_start_stamp_is_microseconds_since_the_unix_epoch() {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).expect("after 1970");
        let ours = start_stamp(this_pid()).map(Duration::from_micros).expect("a stamp");

        let age = now.checked_sub(ours).expect("started before now");
        assert!(age < Duration::from_hours(24), "{ours:?} against {now:?}");
    }

    #[test]
    fn cpu_time_grows_by_the_time_a_busy_thread_takes() {
        let stop = Arc::new(AtomicBool::new(false));
        let busy = Arc::clone(&stop);
        let spinner = thread::spawn(move || {
            while !busy.load(Ordering::Relaxed) {
                std::hint::spin_loop();
            }
        });
        let first = Instant::now();
        let before = cpu_time(this_pid()).expect("cpu time");
        wait_until("the busy thread uses cpu", || {
            cpu_time(this_pid()).is_some_and(|now| now.saturating_sub(before) > Duration::from_millis(100))
        });
        let used = cpu_time(this_pid()).expect("cpu time").saturating_sub(before);
        let wall = first.elapsed();
        stop.store(true, Ordering::Relaxed);
        spinner.join().expect("the busy thread ends");

        let cores = thread::available_parallelism().map_or(1, usize::from);
        let most = wall.as_secs_f64() * f64::from(u32::try_from(cores).unwrap_or(64)) * 2.0 + 0.1;
        assert!(used.as_secs_f64() <= most, "{used:?} in {wall:?} on {cores} cores");
        assert!(used.as_secs_f64() >= wall.as_secs_f64() * 0.25, "{used:?} in {wall:?}");
    }
}
