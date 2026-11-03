use super::*;
use crate::agents::activity::Activity;
use crate::term::mouse::MouseMode;
use crate::test_util::{TempDir, wait_until, wait_until_within};

const SLOW: Duration = Duration::from_secs(30);
const PING: &str = "ping -n 30 127.0.0.1";
const CURSOR_REQUEST: &[u8] = b"\x1b[6n";

fn system_path() -> String {
    let root = std::env::var("SystemRoot").expect("SystemRoot");
    format!(r"{root}\System32;{root};{root}\System32\WindowsPowerShell\v1.0")
}

fn spawn_cmd(cwd: Option<PathBuf>) -> (Term, Receiver<Event>) {
    let (tx, rx) = mpsc::channel();
    let env = [("PATH", system_path()), ("PROMPT", "$G".into()), ("CORNERCASE_PROBE", "from-cornercase".into())]
        .map(|(key, value)| (key.to_string(), value));
    let args = ["/d".to_string()];
    let theme = HostTheme::default();
    let opts = SpawnOptions {
        id: 1,
        shell: "cmd.exe",
        args: &args,
        env: &env,
        remove: &[],
        rows: 24,
        cols: 80,
        cwd,
        theme: &theme,
    };
    let mut term = Term::spawn(opts, tx).expect("spawn cmd.exe");
    term.became_shell();
    (term, rx)
}

fn typed(term: &mut Term, rx: &Receiver<Event>, line: &str, shows: &str) {
    term.write(format!("{line}\r").as_bytes());
    wait_until_within(shows, SLOW, || contents(term, rx).contains(shows));
}

fn titled(term: &mut Term, rx: &Receiver<Event>, title: &str) -> bool {
    let _ = contents(term, rx);
    term.emulator.title() == title
}

fn program_is(term: &mut Term, rx: &Receiver<Event>, name: &str) -> bool {
    let _ = contents(term, rx);
    term.program(&Config::default()).is_some_and(|program| process::same_file_name(&program, name))
}

fn runs_ping(pid: i32) -> bool {
    process::descendants(pid)
        .into_iter()
        .any(|child| process::name(child).is_some_and(|name| process::same_file_name(&name, "ping")))
}

fn until_asked(rx: &Receiver<Event>) -> Vec<u8> {
    let mut first = Vec::new();
    while !first.windows(CURSOR_REQUEST.len()).any(|bytes| bytes == CURSOR_REQUEST) {
        let Ok(Event::Output(_, _, bytes)) = rx.recv_timeout(SLOW) else { break };
        first.extend_from_slice(&bytes);
    }
    first
}

fn nothing_but_sequences(bytes: &[u8]) -> bool {
    let mut parts = bytes.split(|byte| *byte == 0x1b);
    let sequence = |part: &[u8]| match part {
        [b'[', parameters @ .., end] if end.is_ascii_alphabetic() => {
            parameters.iter().all(|byte| byte.is_ascii_digit() || b"?;".contains(byte))
        }
        _ => false,
    };
    parts.next().is_some_and(<[u8]>::is_empty) && parts.all(sequence)
}

fn exited(term: &mut Term, rx: &Receiver<Event>) -> bool {
    let deadline = Instant::now() + SLOW;
    while let Some(left) = deadline.checked_duration_since(Instant::now()) {
        match rx.recv_timeout(left) {
            Ok(Event::Output(_, _, bytes)) => term.feed(&bytes),
            Ok(Event::Exited(..)) => return true,
            Err(_) => return false,
        }
    }
    false
}

mod pane {
    use super::*;

    #[test]
    fn the_console_asks_for_the_cursor_before_the_shell_shows_anything() {
        let (_term, rx) = spawn_cmd(None);

        let first = until_asked(&rx);

        assert!(first.ends_with(CURSOR_REQUEST) && nothing_but_sequences(&first), "{first:?}");
    }

    #[test]
    fn runs_what_is_typed_with_the_variables_cornercase_sets() {
        let (mut term, rx) = spawn_cmd(None);

        typed(&mut term, &rx, "echo cc-mark^er %CORNERCASE_PROBE%", "cc-marker from-cornercase");
    }

    #[test]
    fn starts_in_the_folder_it_was_given() {
        let dir = TempDir::new();
        let (mut term, rx) = spawn_cmd(Some(dir.path().to_path_buf()));
        let folder = dir.path().canonicalize().expect("canonicalize");

        wait_until("cmd reports its folder", || {
            let _ = contents(&mut term, &rx);
            term.cwd().and_then(|cwd| cwd.canonicalize().ok()) == Some(folder.clone())
        });
    }

    #[test]
    fn starts_at_its_size_and_follows_a_resize() {
        let (mut term, rx) = spawn_cmd(None);
        let size = "powershell -NoProfile -NonInteractive -Command \"'cc-size-{0}x{1}' -f [Console]::WindowHeight,[Console]::WindowWidth\"";
        typed(&mut term, &rx, size, "cc-size-24x80");

        term.resize(10, 50);

        typed(&mut term, &rx, &format!("cls & {size}"), "cc-size-10x50");
        assert_eq!(term.emulator.size().expect("size"), (10, 50));
    }
}

mod console_sequences {
    use super::*;

    #[test]
    fn a_title_set_through_the_console_reaches_the_emulator() {
        let (mut term, rx) = spawn_cmd(None);

        term.write("title cc-\u{2733} console\r".as_bytes());

        wait_until("the title arrives", || titled(&mut term, &rx, "cc-\u{2733} console"));
    }

    #[test]
    fn a_title_a_program_writes_reaches_the_emulator() {
        let (mut term, rx) = spawn_cmd(None);

        term.write("prompt $E]2;cc-\u{2733} osc$E\\$G\r".as_bytes());

        wait_until("the title arrives", || titled(&mut term, &rx, "cc-\u{2733} osc"));
    }

    #[test]
    fn a_link_keeps_its_address() {
        let (mut term, rx) = spawn_cmd(None);

        typed(&mut term, &rx, "prompt $E]8;;https://example.com/cc$E\\cc-link$E]8;;$E\\$G", "cc-link>");

        let links = term.emulator.surface_snapshot().expect("snapshot").links;
        assert!(links.iter().any(|link| link == "https://example.com/cc"), "{links:?}");
    }

    #[test]
    fn a_status_report_and_a_copy_reach_the_emulator() {
        let (mut term, rx) = spawn_cmd(None);

        typed(&mut term, &rx, "prompt $E]7501;state=working$E\\$E]52;c;Y2MtY29weQ==$E\\cc-osc$G", "cc-osc>");

        assert!(matches!(term.emulator.program_activity(), Some(Activity::Working)));
        assert!(term.emulator.take_copied().iter().any(|copied| copied == "cc-copy"));
    }

    #[test]
    fn a_mouse_mode_a_program_asks_for_stays_in_the_console() {
        let (mut term, rx) = spawn_cmd(None);

        typed(&mut term, &rx, "prompt $E[?1000h$E[?1006hcc-mouse$G", "cc-mouse>");

        assert!(matches!(term.emulator.mouse_mode(), MouseMode::None));
    }
}

mod foreground {
    use super::*;

    #[test]
    fn is_the_shell_at_its_prompt() {
        let (mut term, rx) = spawn_cmd(None);

        wait_until("cmd is at its prompt", || program_is(&mut term, &rx, "cmd"));

        assert_eq!((term.shell_in_foreground(), term.current_job(&Config::default())), (true, None));
    }

    #[test]
    fn is_the_program_the_shell_runs_until_it_stops() {
        let (mut term, rx) = spawn_cmd(None);
        term.write(format!("{PING}\r").as_bytes());
        wait_until_within("ping runs", SLOW, || program_is(&mut term, &rx, "ping"));
        let job = term.current_job(&Config::default()).expect("a job");
        assert_eq!((Some(job.pid), term.shell_in_foreground()), (term.foreground_pid(), false));

        term.write(b"\x03");

        wait_until_within("back to cmd", SLOW, || program_is(&mut term, &rx, "cmd"));
    }

    #[test]
    fn looks_through_a_nested_shell() {
        let (mut term, rx) = spawn_cmd(None);

        term.write(format!("cmd /d /c {PING}\r").as_bytes());

        wait_until_within("ping runs under a second cmd", SLOW, || program_is(&mut term, &rx, "ping"));
    }

    #[test]
    fn is_the_first_program_that_is_not_a_shell() {
        let (mut term, rx) = spawn_cmd(None);

        term.write(format!("forfiles /p %SystemRoot%\\System32 /m cmd.exe /c \"cmd /d /c {PING}\"\r").as_bytes());

        wait_until_within("forfiles runs ping", SLOW, || {
            program_is(&mut term, &rx, "forfiles") && term.foreground_pid().is_some_and(runs_ping)
        });
    }
}

mod exit {
    use super::*;

    #[test]
    fn a_shell_that_exits_says_so_with_its_status() {
        let (mut term, rx) = spawn_cmd(None);

        term.write(b"exit 3\r");

        assert!(exited(&mut term, &rx), "no Exited event");
        assert_eq!(term.exit_status().as_deref(), Some("3"));
    }

    #[test]
    fn a_pane_dropped_while_the_console_waits_for_the_cursor_still_closes() {
        let (term, rx) = spawn_cmd(None);
        let _ = until_asked(&rx);

        drop(term);

        let closed = std::iter::from_fn(|| rx.recv_timeout(SLOW).ok()).any(|ev| matches!(ev, Event::Exited(1, 0)));
        assert!(closed, "no Exited event");
    }
}
