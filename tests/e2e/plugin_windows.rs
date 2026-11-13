use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command, Stdio};

use rustix::fs::{OFlags, fcntl_setfl};
use serde_json::json;

use super::commands::Followed;
use super::plugins::alive;
use super::*;

const MANIFEST: &str = r#"id = "acme.panel"
name = "Panel"
version = "0.1.0"

[[window]]
id = "panel"
command = ["./panel"]
"#;
const KEEPING: &str = "keeping the plugins' window processes running until this command ends";

struct Panel {
    session: Arc<Session>,
    folder: PathBuf,
}

impl Panel {
    fn folder(test: &str) -> PathBuf {
        let folder = temp_dir_named(&format!("ccwindows-{test}-{}", std::process::id()));
        std::fs::write(folder.join("cornercase-plugin.toml"), MANIFEST).expect("manifest");
        let script = "#!/bin/sh\necho \"$CORNERCASE_BIN\" >> \"$CORNERCASE_PLUGIN_STATE/bins\"\n\
                      echo $$ >> \"$CORNERCASE_PLUGIN_STATE/pids\"\nexec sleep 60\n";
        write_executable(&folder.join("panel"), script);
        folder
    }

    fn on(test: &str) -> Self {
        let (session, folder) = (Session::new(), Self::folder(test));
        session.says(&["plugin", "link", &folder.display().to_string()]);
        session.says(&["plugin", "enable", "acme.panel"]);
        Self { session, folder }
    }

    fn state(&self) -> PathBuf {
        self.session.dir.join("plugins/state/acme.panel")
    }

    fn pids(&self) -> Vec<i32> {
        let written = std::fs::read_to_string(self.state().join("pids")).unwrap_or_default();
        written.lines().filter_map(|line| line.parse().ok()).collect()
    }

    fn running_files(&self) -> Vec<String> {
        let Ok(files) = std::fs::read_dir(self.state().join("running")) else { return Vec::new() };
        files.flatten().map(|file| file.file_name().to_string_lossy().into_owned()).collect()
    }

    fn keeper(&self) -> (Child, Followed) {
        let mut keeper = self.session.spawn(&["plugin", "windows", "--json"]);
        let lines = Followed::of(&mut keeper);
        (keeper, lines)
    }

    fn runs(&self, lines: &mut Followed, what: &str) -> i32 {
        let line = lines.next(what);
        assert_eq!(
            (&line["plugin"], &line["id"], &line["state"]),
            (&json!("acme.panel"), &json!("panel"), &json!("running"))
        );
        assert!(line["restart_in"].is_null(), "{line}");
        let pid = i32::try_from(line["pid"].as_u64().expect("a pid")).expect("a pid");
        wait_until("the process wrote its pid", || self.pids().last() == Some(&pid));
        pid
    }
}

impl Drop for Panel {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.folder);
    }
}

fn sent(pid: &impl ToString, name: &str) -> bool {
    Command::new("kill").args([name, &pid.to_string()]).status().is_ok_and(|status| status.success())
}

fn signal(pid: &impl ToString, name: &str) {
    assert!(sent(pid, name), "kill {name} failed");
}

struct Keeper(Option<Child>);

impl Keeper {
    fn spawn(command: &mut Command) -> Self {
        Self(Some(command.spawn().expect("run cornercase")))
    }

    fn stop(mut self) {
        let keeper = self.0.take().expect("the keeper runs");
        signal(&keeper.id(), "-TERM");
        ended_well(keeper);
    }
}

impl Drop for Keeper {
    fn drop(&mut self) {
        if let Some(keeper) = self.0.take() {
            sent(&keeper.id(), "-TERM");
            finish_within(keeper, TIMEOUT);
        }
    }
}

fn ended_well(keeper: Child) {
    let ended = finish_within(keeper, TIMEOUT);
    assert_eq!((ended.status.code(), ended.stderr.as_slice()), (Some(0), &b""[..]), "{ended:?}");
}

fn assert_stopped(panel: &Panel, pid: i32) {
    assert!(!alive(pid));
    assert_eq!(panel.running_files(), Vec::<String>::new());
}

fn starts_again(panel: &Panel, lines: &mut Followed, killed: i32) -> i32 {
    lines.until("it starts again", |line| line["state"] == "running" && line["pid"] != json!(killed));
    let says_when = |line: &serde_json::Value| {
        line["state"] != "stopped" || line["restart_in"].as_u64().is_some_and(|seconds| seconds >= 1)
    };
    assert!(lines.seen.iter().all(says_when), "{:?}", lines.seen);
    let again = lines.seen.last().and_then(|line| i32::try_from(line["pid"].as_u64()?).ok()).expect("its pid");
    wait_until("the new process wrote its pid", || panel.pids().last() == Some(&again));
    again
}

fn assert_the_log_tells_two_starts_and_the_stop(panel: &Panel) {
    let log = std::fs::read_to_string(panel.state().join("plugin.log")).expect("the plugin's log");
    assert_eq!(log.matches("window process started").count(), 2, "{log}");
    assert!(log.contains("window process stopped"), "{log}");
}

#[test]
fn a_client_keeps_a_window_process_running_with_no_server_until_the_command_ends() {
    let panel = Panel::on("reproduction");
    assert_eq!(panel.pids(), Vec::<i32>::new(), "nothing runs before it");

    let (keeper, mut lines) = panel.keeper();
    let pid = panel.runs(&mut lines, "the process appears");
    assert!(alive(pid));
    assert_eq!(panel.running_files(), [pid.to_string()]);

    signal(&pid, "-KILL");
    let again = starts_again(&panel, &mut lines, pid);
    assert_eq!(panel.running_files(), [again.to_string()]);

    signal(&keeper.id(), "-TERM");
    ended_well(keeper);
    assert_stopped(&panel, again);
    assert_the_log_tells_two_starts_and_the_stop(&panel);
    assert!(!panel.session.socket().exists(), "no server was started");
}

#[rstest]
#[case::term("-TERM")]
#[case::interrupt("-INT")]
#[case::hangup("-HUP")]
#[case::quit("-QUIT")]
fn a_signal_stops_what_it_runs(#[case] name: &str) {
    let panel = Panel::on(&format!("signal{name}"));
    let (keeper, mut lines) = panel.keeper();
    let pid = panel.runs(&mut lines, "the process appears");

    signal(&keeper.id(), name);

    ended_well(keeper);
    assert_stopped(&panel, pid);
}

#[test]
fn closing_its_output_stops_what_it_runs() {
    let panel = Panel::on("reader-leaves");
    let mut keeper = panel.session.spawn(&["plugin", "windows", "--json"]);
    let mut out = BufReader::new(keeper.stdout.take().expect("its output"));
    let mut line = String::new();
    out.read_line(&mut line).expect("a line");
    wait_until("the process wrote its pid", || panel.pids().len() == 1);
    let pid = panel.pids()[0];

    drop(out);

    ended_well(keeper);
    assert_stopped(&panel, pid);
}

#[test]
fn a_reader_that_stopped_reading_does_not_hold_up_the_end() {
    let panel = Panel::on("stuck-reader");
    let (reader, writer) = full_pipe();
    let mut command = panel.session.command();
    command.args(["plugin", "windows"]).stdout(writer).stderr(Stdio::piped());
    let keeper = command.spawn().expect("run cornercase");
    wait_until("the process wrote its pid", || panel.pids().len() == 1);
    let pid = panel.pids()[0];

    signal(&keeper.id(), "-TERM");

    ended_well(keeper);
    assert_stopped(&panel, pid);
    drop(reader);
}

fn full_pipe() -> (std::io::PipeReader, std::io::PipeWriter) {
    let (reader, mut writer) = std::io::pipe().expect("a pipe");
    fcntl_setfl(&writer, OFlags::NONBLOCK).expect("non-blocking");
    for size in [4096, 1] {
        while writer.write(&vec![b'x'; size]).is_ok() {}
    }
    fcntl_setfl(&writer, OFlags::empty()).expect("blocking again");
    (reader, writer)
}

#[test]
fn with_no_plugin_it_keeps_running_and_follows_the_plugins() {
    let session = Session::new();
    let registry = session.dir.join("plugins.json");
    std::fs::write(&registry, "{").expect("a list it cannot read");
    let folder = Panel::folder("later");
    let link = ["plugin", "link", &folder.display().to_string()];
    assert!(refused(&session.cli(&link), 1, "will not write over it"), "the list is the one commands read");
    let panel = Panel { session, folder };
    let (mut keeper, mut lines) = panel.keeper();

    std::fs::remove_file(&registry).expect("fixed");
    panel.session.says(&link);
    panel.session.says(&["plugin", "enable", "acme.panel"]);
    let first = panel.runs(&mut lines, "the plugin turned on runs");

    panel.session.says(&["plugin", "disable", "acme.panel"]);
    let off = lines.next("the plugin turned off stops");
    assert_eq!((&off["state"], &off["pid"], &off["restart_in"]), (&json!("stopped"), &json!(null), &json!(null)));
    wait_until("the process is gone", || !alive(first));

    panel.session.says(&["plugin", "enable", "acme.panel"]);
    let second = panel.runs(&mut lines, "turned on again it runs again");
    panel.session.says(&["plugin", "remove", "acme.panel"]);
    assert_stopped(&panel, second);
    assert!(keeper.try_wait().expect("check it").is_none(), "it keeps running");

    signal(&keeper.id(), "-TERM");
    ended_well(keeper);
}

#[test]
fn two_clients_run_two_copies() {
    let panel = Panel::on("two-clients");
    let (one, mut one_lines) = panel.keeper();
    let first = panel.runs(&mut one_lines, "the first copy runs");
    let (two, mut two_lines) = panel.keeper();
    let second = panel.runs(&mut two_lines, "the second copy runs");
    assert_ne!(first, second);

    signal(&one.id(), "-TERM");
    ended_well(one);

    assert!(!alive(first) && alive(second));
    assert_eq!(panel.running_files(), [second.to_string()]);
    signal(&two.id(), "-TERM");
    ended_well(two);
    assert_stopped(&panel, second);
}

fn put_this_build_at(bin: &Path) {
    let new = bin.with_extension("new");
    let copied = Command::new("cp").arg(env!("CARGO_BIN_EXE_cornercase")).arg(&new).status().expect("run cp");
    assert!(copied.success(), "copy this build");
    std::fs::rename(&new, bin).expect("over what runs there, as an update does");
}

#[test]
fn what_starts_after_an_update_gets_a_binary_that_runs() {
    let panel = Panel::on("update");
    let bin = panel.folder.join("cornercase");
    put_this_build_at(&bin);
    let mut command = panel.session.command_of(&bin);
    command.args(["plugin", "windows", "--json"]).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut keeper = command.spawn().expect("run the copy");
    let mut lines = Followed::of(&mut keeper);
    let pid = panel.runs(&mut lines, "the process appears");

    put_this_build_at(&bin);
    signal(&pid, "-KILL");
    let again = starts_again(&panel, &mut lines, pid);

    let told = std::fs::read_to_string(panel.state().join("bins")).expect("the paths it got");
    let got = told.lines().last().expect("the last path");
    assert_eq!(got, bin.display().to_string());
    assert!(Command::new(got).arg("--version").status().is_ok_and(|ran| ran.success()), "{got}");
    signal(&keeper.id(), "-TERM");
    ended_well(keeper);
    assert_stopped(&panel, again);
}

#[test]
fn without_json_it_says_each_change_in_words() {
    let panel = Panel::on("words");
    let path = panel.folder.join("said");
    let mut command = panel.session.command();
    command.args(["plugin", "windows"]).stderr(Stdio::piped());
    command.stdout(std::fs::File::create(&path).expect("a file to say it in"));
    let keeper = Keeper::spawn(&mut command);
    let said = || std::fs::read_to_string(&path).unwrap_or_default();
    wait_until("it says the process runs", || said().contains("panel: running") && panel.pids().len() == 1);
    let pid = panel.pids()[0];
    assert_eq!(said(), format!("{KEEPING}\nacme.panel panel: running, pid {pid}\n"));

    panel.session.says(&["plugin", "disable", "acme.panel"]);

    wait_until("it says the process stopped", || said().contains("panel: stopped"));
    assert!(said().ends_with(&format!("pid {pid}\nacme.panel panel: stopped\n")), "{}", said());
    keeper.stop();
}
