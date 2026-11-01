use rstest::rstest;

use super::*;
use crate::agents::activity::Status;
use crate::agents::claude::fake::FakeClaude;
use crate::app::agent_watch::WATCH_AGENTS_EVERY;
use crate::app::control::Asker;
use crate::app::machines::RemoteItem;
use crate::app::tests::control_requests::told;
use crate::control::contract::Code;
use crate::control::{Go, Ids, PaneInfo, What};
use crate::host::sound::Kind;
use crate::project::Tab;

const NEEDS_YOU: &str = "claude on vps needs you in web-shop › main";

fn loud(seeing: &mut Seeing) {
    seeing.rig.app.config.sound = true;
    seeing.rig.app.config.sound_subagents = true;
}

fn claude_is(seeing: &mut Seeing, revision: u64, status: &str, edit: impl Fn(&mut PaneInfo)) {
    let mut report = with_agent(worked(report(&[("web-shop", Some(SHOP)), ("blog", None)])));
    let pane = &mut report.projects[0].workspaces[0].tabs[0].panes[0];
    pane.status = Some(status.into());
    edit(pane);
    serve(&mut seeing.theirs, revision, report);
    seeing.rig.until("claude's state arrives", |app| {
        let seen = app.machines.held()[0].seen.as_ref();
        seen.is_some_and(|seen| seen.projects[0].workspaces[0].tabs[0].panes[0].status.as_deref() == Some(status))
    });
}

fn heard(seeing: &mut Seeing) -> (Option<String>, Vec<Kind>) {
    let sounds = seeing.rig.app.take_sounds().into_iter().map(|cue| cue.kind).collect();
    (toast(&seeing.rig.app).map(str::to_string), sounds)
}

#[rstest]
#[case::waiting("waiting", NEEDS_YOU, Kind::NeedsYou)]
#[case::failed("failed", "claude on vps failed in web-shop › main", Kind::NeedsYou)]
#[case::done("done", "claude on vps finished in web-shop › main", Kind::Finished)]
fn an_agent_there_that_stops_working_is_told_with_its_sound(
    #[case] status: &str,
    #[case] message: &str,
    #[case] sound: Kind,
) {
    let mut seeing = seeing_claude();
    loud(&mut seeing);

    claude_is(&mut seeing, 3, status, |_| {});

    assert_eq!(heard(&mut seeing), (Some(message.to_string()), vec![sound]));
}

#[rstest]
#[case::once_it_is_picked(true, "waiting", false)]
#[case::from_a_shell_in_the_background(false, "waiting", true)]
#[case::back_to_idle(false, "idle", false)]
fn nothing_is_told(#[case] picked: bool, #[case] status: &str, #[case] background: bool) {
    let mut seeing = seeing_claude();
    loud(&mut seeing);
    if picked {
        assert!(seeing.rig.app.select_remote("vps", RemoteItem::Workspace(10)));
    }

    claude_is(&mut seeing, 3, status, |pane| pane.background_shell = background);

    assert_eq!(heard(&mut seeing), (None, Vec::new()));
}

#[test]
fn nothing_is_told_from_the_first_snapshot_once_the_link_is_back() {
    let mut seeing = seeing_claude();
    loud(&mut seeing);
    let ends = Arc::clone(&seeing.ends);
    drop(std::mem::replace(&mut seeing.theirs, UnixStream::pair().expect("a pair").0));
    seeing.rig.until("the machine falls", |app| app.machines.held()[0].status.state == State::Unreachable);
    seeing.theirs = linked(&ends);

    claude_is(&mut seeing, 1, "waiting", |_| {});

    assert_eq!(heard(&mut seeing), (None, Vec::new()));
}

#[rstest]
#[case::with_its_sound_off(false, true, None)]
#[case::from_a_subagent_with_their_sound_off(true, false, None)]
#[case::from_a_subagent_with_their_sound_on(true, true, Some(Kind::NeedsYou))]
fn the_sound_follows_the_settings(#[case] sound: bool, #[case] subagents: bool, #[case] played: Option<Kind>) {
    let mut seeing = seeing_claude();
    seeing.rig.app.config.sound = sound;
    seeing.rig.app.config.sound_subagents = subagents;

    claude_is(&mut seeing, 3, "waiting", |pane| pane.parent = Some(1));

    assert_eq!(heard(&mut seeing), (Some(NEEDS_YOU.to_string()), played.into_iter().collect()));
}

#[test]
fn the_notice_event_names_the_machine_and_no_local_ids() {
    let mut seeing = seeing_claude();
    seeing.rig.app.clients.events.subscribe(9, Vec::new(), false, true);

    claude_is(&mut seeing, 3, "waiting", |_| {});

    let streamed: Vec<_> = seeing.rig.app.take_events().into_iter().map(|(_, streamed)| streamed).collect();
    let events = told(&streamed);
    let [(What::Notice { machine, message, panes, .. }, ids)] = &events[..] else { panic!("one notice: {events:?}") };
    assert_eq!(
        (machine.as_deref(), message.as_str(), panes, ids),
        (Some("vps"), NEEDS_YOU, &vec![12], &Ids::default())
    );
}

#[test]
fn nothing_is_told_once_another_agent_runs_in_the_pane() {
    let mut seeing = seeing_claude();
    loud(&mut seeing);

    claude_is(&mut seeing, 3, "done", |pane| pane.agent = Some("codex".into()));

    assert_eq!(heard(&mut seeing), (None, Vec::new()));
}

fn told_waiting() -> (Seeing, String) {
    let mut seeing = seeing_claude();
    claude_is(&mut seeing, 3, "waiting", |_| {});
    let token = seeing.rig.app.messages.toast.as_ref().and_then(|toast| toast.go.clone()).expect("a go button");
    (seeing, token)
}

#[test]
fn go_picks_the_machine_s_workspace_out_of_full_screen_and_the_menu() {
    let (mut seeing, token) = told_waiting();
    seeing.rig.app.panels.files.set_full_screen(true);
    seeing.rig.app.screen.nav = Some(ui::Nav::Workspaces);

    seeing.rig.app.notification_clicked(&token);

    let app = &seeing.rig.app;
    assert_eq!((picked(app), toast(app)), (Some((10, Some(13))), None));
    assert_eq!((app.panels.files.full_screen(), app.screen.nav), (false, None));
}

#[test]
fn go_finds_the_machine_once_it_is_renamed() {
    let (mut seeing, token) = told_waiting();
    seeing.rig.edit(vec![machine("box", "vps.lan")], Duration::from_secs(2));
    assert_eq!(seeing.rig.app.machines.held()[0].name, "box");

    seeing.rig.app.notification_clicked(&token);

    assert_eq!(picked(&seeing.rig.app), Some((10, Some(13))));
}

#[test]
fn go_for_a_view_is_refused_and_picks_nothing() {
    let (mut seeing, token) = told_waiting();

    let refused = seeing.rig.app.go_request(Asker { caller: None, view: Some(1) }, &Go { token }).expect_err("refused");

    assert_eq!((refused.code, picked(&seeing.rig.app)), (Code::Malformed, None));
    assert!(refused.message.contains("drop --view"), "{}", refused.message);
}

fn watch_until(rig: &mut Rig, what: &str, done: impl Fn(&App) -> bool) {
    let (app, rx) = (&mut rig.app, &rig.rx);
    wait_until(what, || {
        drain(app, rx);
        let now = app.agents.watched().map_or_else(Instant::now, |at| at + WATCH_AGENTS_EVERY);
        app.watch_agents(now);
        done(app)
    });
}

#[test]
fn an_agent_here_behind_the_machine_s_workspace_is_told() {
    let Seeing { mut rig, .. } = seeing_two();
    let claude = FakeClaude::new();
    rig.app.agents.set_claude_dir(claude.dir());
    type_line(&mut rig.app, &claude.command_line(""));
    let working = |app: &App| app.tab().and_then(Tab::status);
    watch_until(&mut rig, "claude works here", |app| working(app) == Some(Status::Working));
    assert!(rig.app.select_remote("vps", RemoteItem::Workspace(10)));

    claude.signal("finish");
    watch_until(&mut rig, "claude's turn is told", |app| toast(app).is_some());

    assert!(toast(&rig.app).is_some_and(|told| told.starts_with("claude finished in ")), "{:?}", toast(&rig.app));
}
