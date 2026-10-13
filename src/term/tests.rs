use std::assert_matches;
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use rstest::rstest;

use super::*;
use crate::test_util::{TempDir, is_sh, wait_until};

const RECV_TIMEOUT: Duration = Duration::from_secs(5);

fn spawn_sh_in(cwd: Option<PathBuf>) -> (Term, Receiver<Event>) {
    spawn_with(&[], cwd)
}

fn spawn_with(args: &[String], cwd: Option<PathBuf>) -> (Term, Receiver<Event>) {
    let (tx, rx) = mpsc::channel();
    let opts =
        SpawnOptions { id: 1, shell: "/bin/sh", args, env: &[], rows: 24, cols: 80, cwd, theme: &HostTheme::default() };
    (Term::spawn(opts, tx).expect("spawn /bin/sh"), rx)
}

fn spawn_sh() -> (Term, Receiver<Event>) {
    spawn_sh_in(None)
}

fn contents(term: &mut Term, rx: &Receiver<Event>) -> String {
    while let Ok(ev) = rx.try_recv() {
        if let Event::Output(_, bytes) = ev {
            term.feed(&bytes);
        }
    }
    term.emulator.snapshot().expect("snapshot").contents()
}

mod scrollback {
    use super::*;

    #[rstest]
    #[case::narrow(80, 13_000)]
    #[case::wide(250, 4_000)]
    fn a_pane_keeps_thousands_of_lines(#[case] cols: u16, #[case] at_least: usize) {
        let mut emu =
            Emulator::new(50, cols, SCROLLBACK_BYTES, &HostTheme::default(), Box::new(|_| {})).expect("emulator");
        let text: Vec<String> = (1..=20_000).map(|i| format!("line-{i}")).collect();
        emu.feed(text.join("\r\n").as_bytes());

        let kept = emu.last_lines(20_000).expect("text");

        assert!(kept.lines().count() >= at_least, "kept {} lines from {:?}", kept.lines().count(), kept.lines().next());
    }
}

mod shell_in_foreground {
    use super::*;

    #[test]
    fn an_idle_shell_is() {
        let (term, _rx) = spawn_sh();
        wait_until("the shell is at its prompt", || term.shell_in_foreground());
    }

    #[test]
    fn a_pane_started_with_a_command_never_is() {
        let (term, _rx) = spawn_with(&["-c".into(), "exec sleep 30".into()], None);
        wait_until("sleep runs", || term.foreground_pid().and_then(process::name).as_deref() == Some("sleep"));

        assert!(!term.shell_in_foreground());
    }
}

mod spawn {
    use super::*;

    #[test]
    fn returns_error_when_shell_does_not_exist() {
        let (tx, _rx) = mpsc::channel::<Event>();
        let opts = SpawnOptions {
            id: 1,
            shell: "/nonexistent/shell",
            args: &[],
            env: &[],
            rows: 24,
            cols: 80,
            cwd: None,
            theme: &HostTheme::default(),
        };

        let result = Term::spawn(opts, tx);

        assert_matches!(result, Err(Error::SpawnShell { .. }));
    }

    #[test]
    fn starts_in_requested_dir() {
        let tmp = std::env::temp_dir().canonicalize().expect("temp dir");
        let (term, _rx) = spawn_sh_in(Some(tmp.clone()));

        wait_until("shell starts in requested dir", || term.cwd().as_ref() == Some(&tmp));
    }
}

mod shell_pid {
    use super::*;

    #[test]
    fn is_the_parent_of_the_program_in_the_foreground() {
        let (mut term, _rx) = spawn_sh();

        term.write(b"/bin/sleep 30\r");

        wait_until("sleep runs under the shell", || {
            term.shell_pid()
                .zip(term.foreground_pid())
                .is_some_and(|(shell, pid)| process::children(shell).contains(&pid))
        });
        term.write(b"\x03");
    }
}

mod output {
    use super::*;

    #[test]
    fn is_parsed_into_the_screen() {
        let (mut term, rx) = spawn_sh();

        term.write(b"echo hello-$((1+1))\r");

        wait_until("output appears on screen", || contents(&mut term, &rx).contains("hello-2"));
    }

    #[test]
    fn sends_output_event() {
        let (mut term, rx) = spawn_sh();

        term.write(b"echo x\r");

        assert_matches!(rx.recv_timeout(RECV_TIMEOUT), Ok(Event::Output(1, _)));
    }
}

mod writing {
    use super::*;

    struct Stuck(Receiver<()>);

    impl Write for Stuck {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            let _ = self.0.recv();
            Err(std::io::Error::other("released"))
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn a_terminal_that_stops_taking_input_gets_no_more_than_its_queue() {
        let (release, stuck) = mpsc::channel();
        let writer = spawn_writer(Box::new(Stuck(stuck)));

        let taken = [write_to(&writer, &vec![b'x'; MAX_QUEUED]), write_to(&writer, b"y")];
        drop(release);

        assert_eq!(taken, [true, false]);
    }

    #[test]
    fn a_paste_cannot_end_itself_early() {
        assert_eq!(bracketed("ls\x1b[201~\rrm -rf x"), "\x1b[200~ls\rrm -rf x\x1b[201~");
    }

    #[test]
    fn a_program_that_does_not_read_never_blocks_the_writer() {
        let (mut term, _rx) = spawn_sh();
        term.write(b"stty -echo; sleep 2; cat > /dev/null\r");
        wait_until("sleep runs", || term.program(&Config::default()).as_deref() == Some("sleep"));
        let lines = format!("{}\n", "x".repeat(63)).repeat(16 * 1024);

        let started = Instant::now();
        term.write(lines.as_bytes());

        assert!(started.elapsed() < Duration::from_millis(500), "the write took {:?}", started.elapsed());
    }
}

mod replies {
    use super::*;

    #[test]
    fn reach_the_program() {
        let (mut term, rx) = spawn_sh();

        term.write(b"stty -icanon -echo; printf '\\033[5n'; head -c 4 | tr '\\033' E; stty sane\r");

        wait_until("program reads the status reply", || contents(&mut term, &rx).contains("E[0n"));
    }
}

mod cwd {
    use super::*;

    #[test]
    fn follows_cd() {
        let (mut term, _rx) = spawn_sh();

        term.write(b"cd /\r");

        wait_until("cwd follows cd", || term.cwd() == Some(PathBuf::from("/")));
    }
}

mod program {
    use super::*;

    fn label(name: &str, argv: &[&str]) -> String {
        let argv: Vec<String> = argv.iter().map(|a| (*a).to_string()).collect();
        program(&Config::default(), name, &argv)
    }

    #[rstest]
    #[case::codex_through_npm("node-MainThread", &["node", "/usr/local/bin/codex"], "codex")]
    #[case::codex_by_its_script("node", &["node", "/usr/lib/node_modules/@openai/codex/bin/codex.js"], "codex")]
    #[case::a_native_agent("claude", &["/home/a/.local/bin/claude", "--resume"], "claude")]
    #[case::a_renamed_agent("cursor-agent", &["cursor-agent"], "cursor")]
    #[case::a_script_without_extension("node", &["node", "/usr/bin/vite", "--port", "3000"], "vite")]
    #[case::a_flag_before_the_script("node", &["node", "--inspect", "server.mjs"], "server")]
    #[case::a_python_script("python3", &["python3", "-u", "./tools/sync.py"], "sync")]
    #[case::a_python_module("python3", &["python3", "-m", "http.server"], "python3")]
    #[case::inline_code("ruby", &["ruby", "-e", "puts 'a/b'"], "ruby")]
    #[case::inline_code_naming_a_file("node", &["node", "-e", "require('./lib/foo.js')"], "node")]
    #[case::python_code_naming_a_file("python3", &["python3", "-c", "import./tools/sync.py"], "python3")]
    #[case::a_bare_interpreter("node", &["node"], "node")]
    #[case::the_node_thread_name("node-MainThread", &["node"], "node")]
    #[case::another_program("nvim", &["nvim", "src/main.rs"], "nvim")]
    #[case::a_program_with_an_agent_argument("less", &["less", "claude"], "less")]
    fn names_the_program(#[case] name: &str, #[case] argv: &[&str], #[case] expected: &str) {
        assert_eq!(label(name, argv), expected);
    }

    #[test]
    fn is_the_shell_when_idle() {
        let (term, _rx) = spawn_sh();

        wait_until("shows the shell", || term.program(&Config::default()).as_deref().is_some_and(is_sh));
    }

    #[test]
    fn is_the_foreground_program_while_it_runs() {
        let (mut term, _rx) = spawn_sh();

        term.write(b"sleep 30\r");

        wait_until("shows the foreground program", || term.program(&Config::default()).as_deref() == Some("sleep"));
    }

    #[test]
    fn falls_back_to_shell_when_program_ends() {
        let (mut term, _rx) = spawn_sh();
        term.write(b"sleep 30\r");
        wait_until("sleep starts", || term.program(&Config::default()).as_deref() == Some("sleep"));

        term.write(&[0x03]);

        wait_until("back to the shell", || term.program(&Config::default()).as_deref().is_some_and(is_sh));
    }

    #[test]
    fn is_the_script_an_interpreter_runs() {
        let dir = TempDir::new();
        let node = dir.path().join("node");
        let status = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg("cp /bin/bash \"$1\"")
            .arg("sh")
            .arg(&node)
            .status()
            .expect("run cp");
        assert!(status.success(), "copy bash");
        std::fs::create_dir(dir.path().join("bin")).expect("create bin");
        std::fs::write(dir.path().join("bin/tool.js"), "read line\n").expect("write script");
        let (mut term, _rx) = spawn_sh_in(Some(dir.path().to_path_buf()));

        term.write(b"./node bin/tool.js\r");

        wait_until("shows the script", || term.program(&Config::default()).as_deref() == Some("tool"));
    }
}

mod current_job {
    use super::*;

    #[test]
    fn is_none_at_the_prompt_and_the_program_while_it_runs() {
        let (mut term, _rx) = spawn_sh();
        wait_until("the shell is at its prompt", || term.shell_in_foreground());
        let idle = term.current_job(&Config::default());

        term.write(b"sleep 30\r");

        wait_until("sleep runs", || term.current_job(&Config::default()).is_some_and(|job| job.program == "sleep"));
        let job = term.current_job(&Config::default()).expect("a job");
        assert_eq!((idle, Some(job.pid)), (None, term.foreground_pid()));
    }
}

mod resize {
    use super::*;

    #[test]
    fn updates_the_emulator_size() {
        let (mut term, _rx) = spawn_sh();

        term.resize(10, 50);

        assert_eq!(term.emulator.size().expect("size"), (10, 50));
    }

    #[test]
    fn reaches_the_program() {
        let (mut term, rx) = spawn_sh();

        term.resize(10, 50);
        term.write(b"stty size\r");

        wait_until("stty sees the new size", || contents(&mut term, &rx).contains("10 50"));
    }
}

mod kill {
    use super::*;

    #[test]
    fn sends_exited_event() {
        let (mut term, rx) = spawn_sh();

        term.kill();

        let exited = std::iter::from_fn(|| rx.recv_timeout(RECV_TIMEOUT).ok()).any(|ev| matches!(ev, Event::Exited(1)));
        assert!(exited, "no Exited(1) event received");
    }
}
