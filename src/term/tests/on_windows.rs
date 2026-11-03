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
    let env = [("PATH", system_path()), ("PROMPT", "$G".into()), ("CORNERCASE_PROBE", "from-cornercase".into())]
        .map(|(key, value)| (key.to_string(), value));
    spawn_cmd_with(&env, cwd)
}

fn spawn_cmd_with(env: &[(String, String)], cwd: Option<PathBuf>) -> (Term, Receiver<Event>) {
    let (tx, rx) = mpsc::channel();
    let args = ["/d".to_string()];
    let theme = HostTheme::default();
    let opts =
        SpawnOptions { id: 1, shell: "cmd.exe", args: &args, env, remove: &[], rows: 24, cols: 80, cwd, theme: &theme };
    let mut term = Term::spawn(opts, tx).expect("spawn cmd.exe");
    term.became_shell();
    (term, rx)
}

fn in_folder(term: &mut Term, rx: &Receiver<Event>, folder: &Path) -> bool {
    let _ = contents(term, rx);
    term.cwd().and_then(|cwd| cwd.canonicalize().ok()).as_deref() == Some(folder)
}

fn typed(term: &mut Term, rx: &Receiver<Event>, line: &str, shows: &str) {
    term.write(format!("{line}\r").as_bytes());
    until_shown(term, rx, shows);
}

fn until_shown(term: &mut Term, rx: &Receiver<Event>, text: &str) {
    wait_until_within(text, SLOW, || contents(term, rx).contains(text));
}

fn single_quoted(path: &Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', "''"))
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

        wait_until("cmd reports its folder", || in_folder(&mut term, &rx, &folder));
    }

    #[test]
    fn starts_with_the_environment_of_the_process_that_started_it() {
        let (mut term, rx) = spawn_cmd_with(&[("PROMPT".into(), "$G".into())], None);
        let own = std::env::var("PATH").expect("PATH");
        let shell = term.shell_pid().expect("the shell's pid");
        let path = |pid| process::env(pid).into_iter().find(|(key, _)| key.eq_ignore_ascii_case("PATH"));

        wait_until("the shell's environment can be read", || {
            let _ = contents(&mut term, &rx);
            path(shell).is_some()
        });

        assert_eq!(path(shell).map(|(_, value)| value), Some(own));
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
}

mod mouse {
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    use rstest::rstest;

    use super::*;
    use crate::term::mouse::{self, MouseEncoding};

    const LEFT: MouseButton = MouseButton::Left;
    const READER: &str = crate::test_util::fixture!("conpty/mouse.cs");
    const REFUSED: &str = "cc-refused";

    fn start_reader(term: &mut Term, rx: &Receiver<Event>, dir: &TempDir, call: &str) {
        let source = dir.path().join("mouse.cs");
        std::fs::write(&source, READER).expect("write the reader");
        let path = single_quoted(&source);
        let compile = format!("Add-Type -Path {path} -ErrorAction Stop; [CcMouse]::{call}");
        let refused = "'cc-re' + 'fused: ' + $_";
        let line =
            format!("powershell -NoProfile -NonInteractive -Command \"try {{ {compile} }} catch {{ {refused} }}\"");
        term.write(format!("{line}\r").as_bytes());
        let mut shown = String::new();
        wait_until_within("the reader starts", SLOW, || {
            shown = contents(term, rx);
            shown.contains("cc-ready") || shown.contains(REFUSED)
        });
        let refusal = shown.lines().find(|line| line.contains(REFUSED));
        assert!(refusal.is_none(), "PowerShell did not run the reader: {refusal:?}");
    }

    fn asks_for(term: &mut Term, rx: &Receiver<Event>, mode: MouseMode) -> bool {
        let _ = contents(term, rx);
        term.emulator.mouse_mode() == mode
    }

    fn click(term: &mut Term, kind: MouseEventKind, col: u16) -> Vec<u8> {
        let ev = MouseEvent { kind, column: col, row: 4, modifiers: KeyModifiers::NONE };
        let (mode, encoding) = (term.emulator.mouse_mode(), term.emulator.mouse_encoding());
        let bytes = mouse::encode(ev, col, 4, mode, encoding).expect("the program wants the click");
        term.write(&bytes);
        bytes
    }

    fn read_as(bytes: &[u8]) -> String {
        let units: Vec<String> =
            String::from_utf8_lossy(bytes).chars().map(|unit| format!("{:x}", u32::from(unit))).collect();
        format!("cc-read {}", units.join(" "))
    }

    #[rstest]
    #[case::sgr("1000;1006", MouseEncoding::Sgr, 2)]
    #[case::x10_at_its_last_column("1000", MouseEncoding::Default, 94)]
    #[case::utf8_past_it("1000;1005", MouseEncoding::Utf8, 199)]
    fn a_program_reading_vt_input_gets_its_mouse_mode_through_and_reads_a_click_as_sent(
        #[case] modes: &str,
        #[case] encoding: MouseEncoding,
        #[case] col: u16,
    ) {
        let dir = TempDir::new();
        let (mut term, rx) = spawn_cmd(None);
        start_reader(&mut term, &rx, &dir, &format!("Vt('{modes}')"));
        wait_until("the mouse mode arrives", || asks_for(&mut term, &rx, MouseMode::PressRelease));
        assert_eq!(term.emulator.mouse_encoding(), encoding);

        let mut sent = click(&mut term, MouseEventKind::Down(LEFT), col);
        sent.extend(click(&mut term, MouseEventKind::Up(LEFT), col));
        term.write(b"q");

        until_shown(&mut term, &rx, &read_as(&sent));
        wait_until("the mouse mode ends", || asks_for(&mut term, &rx, MouseMode::None));
    }

    #[test]
    fn a_byte_past_0x7f_reaches_the_program_as_a_replacement_character() {
        let dir = TempDir::new();
        let (mut term, rx) = spawn_cmd(None);
        start_reader(&mut term, &rx, &dir, "Vt('1000')");

        term.write(b"\x1b[M \x80%q");

        until_shown(&mut term, &rx, "cc-read 1b 5b 4d 20 fffd 25");
    }

    #[test]
    fn a_program_asking_the_console_for_mouse_input_gets_a_click_as_a_mouse_record() {
        let dir = TempDir::new();
        let (mut term, rx) = spawn_cmd(None);
        start_reader(&mut term, &rx, &dir, "Records()");
        wait_until("the console asks for the mouse", || asks_for(&mut term, &rx, MouseMode::AnyMotion));
        assert_eq!(term.emulator.mouse_encoding(), MouseEncoding::Sgr);

        click(&mut term, MouseEventKind::Down(LEFT), 2);

        until_shown(&mut term, &rx, "cc-click 2,4");
        wait_until("the console lets the mouse go", || asks_for(&mut term, &rx, MouseMode::None));
    }

    #[test]
    fn a_mouse_mode_written_without_vt_input_stays_in_the_console() {
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
    fn is_the_deepest_shell_with_its_folder_through_nested_shells() {
        let dir = TempDir::new();
        let folder = dir.path().canonicalize().expect("canonicalize");
        let (mut term, rx) = spawn_cmd(None);
        term.write(b"cmd /d\r");
        wait_until_within("a second cmd is in the foreground", SLOW, || {
            program_is(&mut term, &rx, "cmd") && term.foreground_pid() != term.shell_pid()
        });

        typed(&mut term, &rx, &format!("cd /d \"{}\" & echo cc-mo^ved", dir.path().display()), "cc-moved");

        wait_until("the folder is the second cmd's", || in_folder(&mut term, &rx, &folder));
        assert!(term.shell_in_foreground());
    }

    #[test]
    fn is_powershell_in_the_folder_its_prompt_reports() {
        let dir = TempDir::new();
        let folder = dir.path().canonicalize().expect("canonicalize");
        let (mut term, rx) = spawn_cmd(None);
        typed(&mut term, &rx, "powershell -NoProfile", "PS ");
        let prompt = r"function prompt { [char]27 + ']9;9;' + $PWD.ProviderPath + [char]27 + '\' + 'cc-p' + 's> ' }";
        typed(&mut term, &rx, prompt, "cc-ps>");

        term.write(format!("Set-Location {}\r", single_quoted(dir.path())).as_bytes());

        wait_until_within("the reported folder", SLOW, || in_folder(&mut term, &rx, &folder));
        assert!(term.shell_in_foreground());
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

mod reported_folder {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::osc_7_with_a_host("file://box/C:/Users/me%20too", Some("C:/Users/me too"))]
    #[case::osc_7_without_a_host("file:///C:/Users/me", Some("C:/Users/me"))]
    #[case::conemu_quoted(r#""C:\Users\me""#, Some(r"C:\Users\me"))]
    #[case::conemu_plain(r"C:\Users\me", Some(r"C:\Users\me"))]
    #[case::a_unix_folder("file://box/home/me", None)]
    #[case::a_relative_folder(r"Users\me", None)]
    #[case::nothing_reported("", None)]
    fn is_a_windows_folder_from_osc_7_or_9_9(#[case] reported: &str, #[case] expected: Option<&str>) {
        assert_eq!(imp::folder_from(reported), expected.map(PathBuf::from));
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
    fn a_pane_dropped_with_the_cursor_request_unanswered_closes_even_under_openconsole() {
        let (term, rx) = spawn_cmd(None);
        let _ = until_asked(&rx);

        drop(term);

        let closed = std::iter::from_fn(|| rx.recv_timeout(SLOW).ok()).any(|ev| matches!(ev, Event::Exited(1, 0)));
        assert!(closed, "no Exited event");
    }
}
