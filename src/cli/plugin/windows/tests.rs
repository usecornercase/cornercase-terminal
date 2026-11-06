use std::path::{Path, PathBuf};
use std::time::Duration;

use rstest::rstest;

use super::*;
use crate::os::process::{Pid, Signal, kill_process};
use crate::plugins::manifest::FILE;
use crate::plugins::{Kind, Places, Registry, supervise};
use crate::process;
use crate::test_util::{BrokenPipe, TempDir, fake_program, wait_until, wait_until_within};

const STOPS_AND_STARTS: Duration = Duration::from_secs(20);

fn seen(rows: &[(&str, &str, Seen)]) -> BTreeMap<String, Vec<(String, Seen)>> {
    let mut notes: BTreeMap<String, Vec<(String, Seen)>> = BTreeMap::new();
    for (plugin, id, seen) in rows {
        notes.entry((*plugin).into()).or_default().push(((*id).into(), *seen));
    }
    notes
}

fn running(pid: u32) -> Seen {
    Seen::Running { pid }
}

fn change(plugin: &str, entry: ServiceInfo) -> Change {
    Change { plugin: plugin.into(), entry }
}

mod changes {
    use super::*;

    #[test]
    fn the_first_look_tells_each_entry_in_order() {
        let now = Instant::now();
        let notes = seen(&[("b.one", "panel", running(3)), ("a.one", "x", running(1)), ("a.one", "y", running(2))]);

        let changes = Known::default().changes(&notes, now);

        let said: Vec<_> = changes.iter().map(|c| (c.plugin.as_str(), c.entry.id.as_str())).collect();
        assert_eq!(said, [("a.one", "x"), ("a.one", "y"), ("b.one", "panel")]);
    }

    #[test]
    fn nothing_new_tells_nothing() {
        let (mut known, now) = (Known::default(), Instant::now());
        let notes = seen(&[("a.one", "x", running(1)), ("a.one", "y", Seen::Stopped { until: now })]);
        known.changes(&notes, now);

        assert_eq!(known.changes(&notes, now + Duration::from_secs(1)), []);
    }

    #[test]
    fn a_new_pid_is_a_change_even_when_the_stop_was_missed() {
        let (mut known, now) = (Known::default(), Instant::now());
        known.changes(&seen(&[("a.one", "x", running(1))]), now);

        let changes = known.changes(&seen(&[("a.one", "x", running(2))]), now);

        assert_eq!(changes, [change("a.one", ServiceInfo::seen("x", running(2), now))]);
    }

    #[test]
    fn a_stop_says_when_it_starts_again() {
        let (mut known, now) = (Known::default(), Instant::now());
        known.changes(&seen(&[("a.one", "x", running(1))]), now);
        let until = now + Duration::from_millis(1500);

        let changes = known.changes(&seen(&[("a.one", "x", Seen::Stopped { until })]), now);

        assert_eq!((changes[0].entry.state.as_str(), changes[0].entry.pid), ("stopped", None));
        assert_eq!(changes[0].entry.restart_in, Some(2));
    }

    #[test]
    fn another_failed_start_is_another_change() {
        let (mut known, now) = (Known::default(), Instant::now());
        let (first, second) = (now + Duration::from_secs(1), now + Duration::from_secs(5));
        known.changes(&seen(&[("a.one", "x", Seen::Stopped { until: first })]), now);

        let changes = known.changes(&seen(&[("a.one", "x", Seen::Stopped { until: second })]), now);

        assert_eq!(changes[0].entry.restart_in, Some(6));
    }

    #[test]
    fn an_entry_no_longer_wanted_stops_with_no_restart() {
        let (mut known, now) = (Known::default(), Instant::now());
        known.changes(&seen(&[("a.one", "x", running(1)), ("a.one", "y", running(2))]), now);

        let changes = known.changes(&seen(&[("a.one", "y", running(2))]), now);

        assert_eq!(changes, [change("a.one", ServiceInfo::named("x", "stopped"))]);
        assert_eq!(known.changes(&seen(&[("a.one", "y", running(2))]), now), []);
    }
}

mod lines {
    use super::*;

    #[rstest]
    #[case::running(ServiceInfo::seen("panel", running(42), Instant::now()), "acme.one panel: running, pid 42")]
    #[case::waiting(
        ServiceInfo::seen("panel", Seen::Stopped { until: Instant::now() }, Instant::now() + Duration::from_secs(1)),
        "acme.one panel: stopped, restarts in 1s"
    )]
    #[case::gone(ServiceInfo::named("panel", "stopped"), "acme.one panel: stopped")]
    fn a_person_reads_words(#[case] entry: ServiceInfo, #[case] line: &str) {
        assert_eq!(change("acme.one", entry).line(false), line);
    }

    #[rstest]
    #[case::running(
        ServiceInfo::seen("panel", running(42), Instant::now()),
        r#"{"plugin":"acme.one","id":"panel","state":"running","pid":42,"restart_in":null}"#
    )]
    #[case::gone(
        ServiceInfo::named("panel", "stopped"),
        r#"{"plugin":"acme.one","id":"panel","state":"stopped","pid":null,"restart_in":null}"#
    )]
    fn a_program_reads_one_object_per_line(#[case] entry: ServiceInfo, #[case] line: &str) {
        assert_eq!(change("acme.one", entry).line(true), line);
    }

    #[test]
    fn invisible_characters_never_reach_the_terminal_and_keep_their_values() {
        let line = change("notes\u{202e}", ServiceInfo::named("panel", "stopped"));

        assert!(line.line(false).starts_with("notes\\u{202e} panel"), "{}", line.line(false));
        let json = line.line(true);
        assert!(json.contains("notes\\u202e") && !json.chars().any(crate::text::hidden), "{json}");
        let read: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        assert_eq!(read["plugin"], "notes\u{202e}");
    }
}

mod reporting {
    use super::*;

    fn reported(json: bool, steps: Vec<BTreeMap<String, Vec<(String, Seen)>>>) -> String {
        let (notes, mut out) = (Notes::default(), Vec::new());
        let mut steps = steps.into_iter();
        let next = || match steps.next() {
            Some(step) => {
                *notes.lock() = step;
                false
            }
            None => true,
        };
        report(&notes, json, &mut out, next).expect("reported");
        String::from_utf8(out).expect("text")
    }

    #[test]
    fn says_what_it_does_then_each_change_until_its_reader_is_gone() {
        let steps = vec![seen(&[("a.one", "x", running(1))]), seen(&[("a.one", "x", running(1))]), seen(&[])];

        assert_eq!(reported(false, steps), format!("{KEEPING}\na.one x: running, pid 1\na.one x: stopped\n"));
    }

    #[test]
    fn with_json_prints_only_the_changes() {
        let said = reported(true, vec![seen(&[("a.one", "x", running(1))])]);

        assert_eq!(said, "{\"plugin\":\"a.one\",\"id\":\"x\",\"state\":\"running\",\"pid\":1,\"restart_in\":null}\n");
    }

    #[test]
    fn with_no_plugin_prints_nothing_and_keeps_going() {
        assert_eq!(reported(true, vec![seen(&[]), seen(&[]), seen(&[])]), "");
    }

    #[rstest]
    #[case::the_first_line(false)]
    #[case::a_change(true)]
    fn ends_quietly_when_a_write_finds_the_reader_gone(#[case] json: bool) {
        let notes = Notes::default();
        *notes.lock() = seen(&[("a.one", "x", running(1))]);

        report(&notes, json, &mut BrokenPipe, || panic!("a write finds it first")).expect("no error");
    }
}

struct Panel {
    _home: TempDir,
    _root: TempDir,
    places: Places,
}

impl Panel {
    fn on() -> Self {
        let (home, root) = (TempDir::new(), TempDir::new());
        let program = program_in(root.path());
        std::fs::write(program.with_extension("hold"), "").expect("keep it running");
        let name = program.file_name().and_then(|name| name.to_str()).expect("its name");
        let manifest = format!(
            "id = \"acme.panel\"\nname = \"Panel\"\nversion = \"1\"\ndescription = \"A panel\"\n\
             [[window]]\nid = \"panel\"\ncommand = [\"./{name}\"]\n"
        );
        std::fs::write(root.path().join(FILE), manifest).expect("manifest");
        let places = Places::new(&home.path().join("config.json"), &home.path().join("session.json"));
        let mut registry = Registry::load(&places.registry);
        registry.link(root.path()).expect("link");
        registry.save().expect("save");
        let panel = Self { _home: home, _root: root, places };
        panel.switch(true);
        panel
    }

    fn switch(&self, on: bool) {
        let mut registry = Registry::load(&self.places.registry);
        registry.set_enabled("acme.panel", on).expect("switch");
        registry.save().expect("save");
    }

    fn running(&self) -> Vec<i32> {
        supervise::running(&self.places, "acme.panel").expect("the running files")
    }

    fn supervise(&self) -> Supervisor {
        Supervisor::start(Kind::Window, self.places.clone(), Vec::new())
    }
}

fn program_in(root: &Path) -> PathBuf {
    let built = TempDir::new();
    let fake = fake_program(built.path(), "panel");
    let program = root.join(fake.file_name().expect("its name"));
    std::fs::copy(&fake, &program).expect("a copy with the folder's permissions");
    program
}

struct Telling {
    notes: Notes,
    known: Known,
}

impl Telling {
    fn of(supervisor: &Supervisor) -> Self {
        Self { notes: supervisor.notes(), known: Known::default() }
    }

    fn until(&mut self, what: &str, state: &str) -> Vec<ServiceInfo> {
        let mut told = Vec::new();
        wait_until_within(what, STOPS_AND_STARTS, || {
            told.extend(self.known.changes(&self.notes.lock(), Instant::now()).into_iter().map(|change| change.entry));
            told.last().is_some_and(|entry| entry.state == state)
        });
        told
    }
}

fn last(told: &[ServiceInfo]) -> &ServiceInfo {
    told.last().expect("what it waited for")
}

fn pid_of(entry: &ServiceInfo) -> i32 {
    entry.pid.and_then(|pid| i32::try_from(pid).ok()).expect("a pid")
}

fn assert_stopped(panel: &Panel, pid: i32) {
    assert!(!process::alive(pid));
    assert_eq!(panel.running(), Vec::<i32>::new());
}

mod keeping {
    use super::*;

    fn kept(signalled: bool, reporting: impl FnOnce(&Notes) -> Result<()> + Send + 'static) -> Result<bool> {
        let panel = Panel::on();
        let supervisor = panel.supervise();
        wait_until_within("it runs", STOPS_AND_STARTS, || !panel.running().is_empty());
        let pid = panel.running()[0];
        let (tell, ended) = mpsc::channel();
        if signalled {
            tell.send(Ok(())).expect("the signal");
        }

        let kept = keep(supervisor, tell, &ended, reporting);

        assert_stopped(&panel, pid);
        kept
    }

    #[test]
    fn a_signal_stops_what_runs_even_while_a_write_is_stuck() {
        let (_stays_stuck, stuck) = mpsc::channel::<()>();

        let ended = kept(true, move |_| {
            let _ = stuck.recv();
            Ok(())
        });

        assert_eq!(ended.ok(), Some(true));
    }

    #[test]
    fn the_report_ending_stops_what_runs() {
        assert_eq!(kept(false, |_| Ok(())).ok(), Some(true));
    }

    #[rstest]
    #[case::that_fails(false, "cannot write")]
    #[case::that_panics(true, "a bug")]
    fn a_report_that_breaks_stops_what_runs_and_says_so(#[case] panics: bool, #[case] says: &str) {
        let ended = kept(false, move |_| {
            assert!(!panics, "a report that panics");
            Err(Error::Control("cannot write".into()))
        });

        let error = ended.expect_err("its error").to_string();
        assert!(error.contains(says), "{error}");
    }
}

mod a_real_process {
    use super::*;

    #[test]
    fn runs_is_killed_starts_again_and_stops_with_the_supervisor() {
        let panel = Panel::on();
        let supervisor = panel.supervise();
        let mut telling = Telling::of(&supervisor);

        let told = telling.until("it runs", "running");
        let first = last(&told);
        assert_eq!(first.id, "panel");
        assert_eq!(panel.running(), [pid_of(first)]);

        let pid = Pid::from_raw(pid_of(first)).expect("a pid");
        kill_process(pid, Signal::KILL).expect("killed");
        let after = telling.until("it runs again", "running");
        let stops = |entry: &&ServiceInfo| entry.state == "stopped";
        assert!(after.iter().filter(stops).all(|stop| stop.restart_in.is_some_and(|s| s >= 1)), "{after:?}");
        let again = last(&after);
        assert_ne!(again.pid, first.pid);

        drop(supervisor);

        assert_stopped(&panel, pid_of(again));
    }

    #[test]
    fn turning_the_plugin_off_tells_it_will_not_start_again() {
        let panel = Panel::on();
        let supervisor = panel.supervise();
        let mut telling = Telling::of(&supervisor);
        let told = telling.until("it runs", "running");

        panel.switch(false);
        let stopped = telling.until("it stopped", "stopped");

        assert_eq!(last(&stopped), &ServiceInfo::named("panel", "stopped"));
        wait_until("it is gone", || !process::alive(pid_of(last(&told))));
        assert_eq!(panel.running(), Vec::<i32>::new());
    }

    #[test]
    fn two_supervisors_run_two_copies() {
        let panel = Panel::on();
        let (one, two) = (panel.supervise(), panel.supervise());
        wait_until_within("both run", STOPS_AND_STARTS, || panel.running().len() == 2);

        drop(one);

        assert_eq!(panel.running().len(), 1);
        drop(two);
        assert_eq!(panel.running(), Vec::<i32>::new());
    }

    #[test]
    fn the_note_counts_this_command_among_where_it_runs() {
        let panel = Panel::on();
        let plugin = Registry::load(&panel.places.registry).plugins().remove(0);

        assert_eq!(plugin.note(None), "A panel · its window process runs in local windows and `plugin windows`");
    }
}
