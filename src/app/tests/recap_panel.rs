#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use super::*;
use crate::app::recap_panel::Overrides;
use crate::recap::journal::{Entry, Event, Pointer, WorkspacePlace, files};
use crate::recap::{Choice, Named, Place, RECAP_VERSION, Recap, store};
use crate::test_util::{fake_program, fixture};
use crate::time::Stamp;

const OK: &str = fixture!("claude/2.1.318/recap-ok.jsonl");
const FAILED: &str = fixture!("claude/2.1.318/recap-failed.jsonl");

pub(super) struct Recaps {
    pub(super) app: App,
    pub(super) rx: Receiver<AppEvent>,
    state: TempDir,
    pub(super) claude: PathBuf,
    _bin: TempDir,
}

fn at(ago: Duration) -> String {
    Stamp(SystemTime::now() - ago).to_string()
}

fn transcript(dir: &Path, prompt_at: &str) -> PathBuf {
    let path = dir.join("c1.jsonl");
    let text = format!(
        r#"{{"type":"user","origin":{{"kind":"human"}},"timestamp":"{prompt_at}","message":{{"content":[{{"type":"text","text":"add a rate limit"}}]}}}}"#
    );
    std::fs::write(&path, text + "\n").expect("write the transcript");
    path
}

fn entry(event: Event, ago: Duration, app: &App, record: &Path) -> Entry {
    let project = &app.projects[0];
    let workspace = &project.workspaces[0];
    let mut entry = Entry::blank("claude");
    entry.at = at(ago);
    entry.event = event;
    entry.conversation = Some("c1".into());
    entry.record = Some(Pointer::Claude(record.to_path_buf()));
    entry.project = Some(Named { label: "api".into(), path: project.path.display().to_string() });
    entry.workspace = Some(WorkspacePlace {
        label: "default".into(),
        branch: None,
        path: workspace.path.display().to_string(),
        worktree: false,
    });
    entry.tab = Some("claude".into());
    entry
}

pub(super) fn recaps(prompted: Duration) -> Recaps {
    let (mut app, rx) = app();
    let (state, bin) = (TempDir::new(), TempDir::new());
    let claude = fake_program(bin.path(), "claude");
    std::fs::write(claude.with_extension("out"), OK).expect("canned output");
    app.config.agent_commands.insert("claude".into(), claude.display().to_string());
    app.config.recap_agent = "claude".into();
    app.set_recap_places(&state.path().join("journal"), &state.path().join("recap"));
    let record = transcript(state.path(), &at(prompted));
    let found = [entry(Event::Prompt, prompted, &app, &record)];
    files::append(&state.path().join("journal"), &found).expect("append");
    Recaps { app, rx, state, claude, _bin: bin }
}

impl Recaps {
    fn settle(&mut self, choice: Choice) {
        let Self { app, rx, .. } = self;
        wait_until("the recap run ends", || {
            drain(app, rx);
            !app.panels.recap.run(choice).running
        });
    }

    pub(super) fn runs(&self) -> usize {
        std::fs::read_to_string(self.claude.with_extension("argv")).map_or(0, |text| text.lines().count())
    }

    fn text(&self, choice: Choice) -> Option<String> {
        self.app.panels.recap.run(choice).last.as_ref().map(|recap| recap.text.clone())
    }
}

#[test]
fn opening_the_panel_writes_the_recap_of_now_and_keeps_it_for_half_an_hour() {
    let mut r = recaps(Duration::from_secs(600));
    r.app.toggle_recap();
    assert!(r.app.panel_shown());
    r.settle(Choice::Now);
    assert_eq!(r.text(Choice::Now).as_deref(), Some("hello"));
    assert_eq!(r.runs(), 1);
    r.app.toggle_recap();
    r.app.toggle_recap();
    r.settle(Choice::Now);
    assert_eq!(r.runs(), 1);
    r.app.retry_recap();
    r.settle(Choice::Now);
    assert_eq!(r.runs(), 2);
}

#[test]
fn the_result_is_saved_in_the_choice_folder_and_nothing_else_is_written() {
    let mut r = recaps(Duration::from_secs(600));
    r.app.toggle_recap();
    r.settle(Choice::Now);
    let folder = r.state.path().join("recap").join("now");
    let mut names: Vec<String> = std::fs::read_dir(&folder)
        .expect("the folder")
        .flatten()
        .map(|item| item.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(names, ["dossier.md", "recap.json", "recap.md"]);
    assert_eq!(store::load(&r.state.path().join("recap"), Choice::Now).map(|recap| recap.text), Some("hello".into()));
    let dossier = std::fs::read_to_string(folder.join("dossier.md")).expect("dossier");
    assert!(dossier.contains("add a rate limit"), "{dossier}");
}

#[test]
fn yesterday_is_written_once_for_its_day() {
    let mut r = recaps(Duration::from_hours(48));
    r.app.show_recap(Choice::Yesterday);
    r.settle(Choice::Yesterday);
    assert_eq!(r.text(Choice::Yesterday).as_deref(), Some("hello"));
    r.app.show_recap(Choice::Yesterday);
    r.settle(Choice::Yesterday);
    assert_eq!(r.runs(), 1);
}

#[test]
fn nothing_to_sum_up_runs_no_agent_and_says_why() {
    let mut r = recaps(Duration::from_secs(600));
    r.app.show_recap(Choice::Yesterday);
    r.settle(Choice::Yesterday);
    assert_eq!(r.runs(), 0);
    assert_eq!(r.app.panels.recap.run(Choice::Yesterday).empty, Some("No agent activity in the last 30 days."));
}

#[test]
fn a_failure_keeps_the_previous_text_and_retry_runs_again() {
    let mut r = recaps(Duration::from_secs(600));
    r.app.toggle_recap();
    r.settle(Choice::Now);
    std::fs::write(r.claude.with_extension("out"), FAILED).expect("a failing agent");
    r.app.retry_recap();
    r.settle(Choice::Now);
    let run = r.app.panels.recap.run(Choice::Now);
    assert_eq!(run.error.as_deref(), Some("Not logged in · Please run /login"));
    assert_eq!(r.text(Choice::Now).as_deref(), Some("hello"));
    std::fs::write(r.claude.with_extension("out"), OK).expect("an agent that works");
    r.app.retry_recap();
    r.settle(Choice::Now);
    assert!(r.app.panels.recap.run(Choice::Now).error.is_none());
}

#[test]
fn a_second_ask_while_it_runs_only_shows_the_progress() {
    let mut r = recaps(Duration::from_secs(600));
    std::fs::write(r.claude.with_extension("hold"), "").expect("hold");
    r.app.toggle_recap();
    assert!(r.app.panels.recap.run(Choice::Now).running);
    assert!(!r.app.ask_recap(Choice::Now, true, &Overrides::default(), None));
    std::fs::remove_file(r.claude.with_extension("hold")).expect("release");
    r.settle(Choice::Now);
    assert_eq!(r.runs(), 1);
}

#[test]
fn a_place_that_is_open_is_gone_to_and_a_closed_one_is_said_to_be_closed() {
    let mut r = recaps(Duration::from_secs(600));
    r.app.toggle_recap();
    r.settle(Choice::Now);
    r.app.panels.recap.run_mut(Choice::Now).last = Some(Recap {
        text: "- [c1] asked\n- [c2] asked".into(),
        places: [("c1".to_string(), place_of(&r.app, true)), ("c2".to_string(), place_of(&r.app, false))].into(),
        ..Recap::default()
    });
    r.app.recap_go("c1");
    assert_eq!(toast(&r.app), None);
    r.app.recap_go("c2");
    assert_eq!(toast(&r.app), Some("gone › old is closed"));
}

fn place_of(app: &App, open: bool) -> Place {
    let project = &app.projects[0];
    let named = |label: &str, path: String| Some(Named { label: label.into(), path });
    if open {
        Place {
            kind: "conversation".into(),
            project: named("api", project.path.display().to_string()),
            workspace: named("default", project.workspaces[0].path.display().to_string()),
            ..Place::default()
        }
    } else {
        Place {
            kind: "conversation".into(),
            project: named("gone", "/nonexistent/project".into()),
            workspace: named("old", "/nonexistent/project".into()),
            ..Place::default()
        }
    }
}

#[test]
fn the_footer_button_the_icon_and_the_key_open_the_panel() {
    let (mut app, _rx) = app();
    app.screen.drawn = app.layout(AREA);
    let button = ui::recap_button(app.screen.drawn.worktrees);
    assert!(!button.is_empty(), "{:?}", app.screen.drawn.worktrees);
    click(&mut app, button.as_position());
    assert_eq!(app.screen.side, Some(Side::Recap));
    click(&mut app, button.as_position());
    assert_eq!(app.screen.side, None);
}

pub(super) fn waiting_place(status: Option<&str>) -> Place {
    Place {
        kind: "conversation".into(),
        agent: Some("claude".into()),
        conversation: Some("gone".into()),
        status: status.map(str::to_string),
        ..Place::default()
    }
}

pub(super) fn cached_now(places: std::collections::BTreeMap<String, Place>) -> Recap {
    Recap {
        v: RECAP_VERSION,
        range: "now".into(),
        written_at: crate::time::now(),
        text: "## Needs you\n- [c1] asks which cache".into(),
        places,
        ..Recap::default()
    }
}

#[test]
fn a_cached_now_that_names_a_closed_conversation_is_written_again_when_shown() {
    let mut r = recaps(Duration::from_secs(600));
    r.app.panels.recap.run_mut(Choice::Now).last =
        Some(cached_now([("c1".to_string(), waiting_place(Some("waiting")))].into()));
    assert_eq!(r.app.recap_outdated(Choice::Now).ids, ["c1".to_string()].into());
    let view = r.app.recap_view(AREA);
    assert!(view.changed.contains("c1") && view.outdated);
    r.app.show_recap(Choice::Now);
    r.settle(Choice::Now);
    assert_eq!(r.runs(), 1);
    assert_eq!(r.text(Choice::Now).as_deref(), Some("hello"));
    assert!(!r.app.recap_outdated(Choice::Now).any());
}

#[test]
fn a_cached_now_that_still_matches_is_shown_without_a_run() {
    let mut r = recaps(Duration::from_secs(600));
    r.app.panels.recap.run_mut(Choice::Now).last = Some(cached_now([("c1".to_string(), waiting_place(None))].into()));
    r.app.show_recap(Choice::Now);
    r.settle(Choice::Now);
    assert_eq!(r.runs(), 0);
}
