use super::notifications::active_tab;
use super::*;
use crate::app::control::Asker;
use crate::control::contract::Code;
use crate::control::{Go, OpenView, SetView};

const VIEW_CLIENT: u64 = 40;
const READER: u64 = 41;

fn open(w: &mut Watched, tab: usize, view: &OpenView) -> u64 {
    let id = w.app.projects[0].workspaces[0].tabs[tab].id;
    let open = OpenView { tab: Some(id), ..view.clone() };
    let opened = w.app.view_request(VIEW_CLIENT, &open, Some(AREA)).expect("open a view").expect("a snapshot");
    opened["view"]["id"].as_u64().expect("a view id")
}

fn sized() -> OpenView {
    OpenView { cols: Some(100), rows: Some(30), ..OpenView::default() }
}

fn notices(w: &mut Watched) -> Vec<serde_json::Value> {
    w.app.take_events().into_iter().filter(|(client, _)| *client == READER).fold(
        Vec::new(),
        |mut all, (_, streamed)| {
            if let Streamed::Event(text) = streamed {
                let value: serde_json::Value = serde_json::from_str(&text).expect("json");
                if value["ok"]["event"] == "notice" {
                    all.push(value["ok"].clone());
                }
            }
            all
        },
    )
}

fn listen(w: &mut Watched) {
    w.app.events.subscribe(READER, Vec::new(), false, true);
}

#[rstest]
#[case::shown(true, Status::Idle)]
#[case::hidden(false, Status::Done)]
fn a_view_showing_the_tab_takes_its_done_mark_off(#[case] visible: bool, #[case] wanted: Status) {
    let mut w = Watched::start(agents::Known::Claude, true);
    w.report("idle", 6);
    assert_eq!(status(&w.app, 0, 0), Some(Status::Done));

    open(&mut w, 0, &OpenView { visible: Some(visible), ..sized() });
    tick(&mut w.app);

    assert_eq!(status(&w.app, 0, 0), Some(wanted));
}

#[test]
fn an_agent_a_visible_view_shows_stays_quiet() {
    let mut w = Watched::start(agents::Known::Claude, true);
    open(&mut w, 0, &sized());
    listen(&mut w);

    w.report("waiting", 6);

    assert_eq!(w.told(), (None, Vec::new()));
    assert_eq!(notices(&mut w).len(), 0);
}

#[rstest]
#[case::without_a_size(OpenView::default())]
#[case::inactive(OpenView { active: Some(false), ..sized() })]
#[case::hidden(OpenView { visible: Some(false), ..sized() })]
fn a_view_without_a_size_or_inactive_or_hidden_sees_nothing(#[case] view: OpenView) {
    let mut w = Watched::start(agents::Known::Claude, true);
    open(&mut w, 0, &view);

    w.report("waiting", 6);

    assert!(w.token().is_some());
}

#[test]
fn each_notice_reaches_the_readers_that_asked_once_per_settle() {
    let mut w = Watched::start(agents::Known::Claude, true);
    listen(&mut w);
    w.app.events.subscribe(READER + 1, Vec::new(), false, false);

    w.report("waiting", 6);
    let token = w.token().expect("a token");
    let first = notices(&mut w);
    w.report("waiting", 6);
    let again = notices(&mut w);
    w.report("busy", 1);
    w.report("waiting", 6);
    let last = notices(&mut w);

    assert_eq!(first.len(), 1, "{first:?}");
    assert_eq!(first[0]["status"], "waiting");
    assert_eq!(first[0]["token"], token.as_str());
    assert_eq!(first[0]["message"], format!("claude needs you in {}", w.place()).as_str());
    assert_eq!(first[0]["project"], w.app.projects[0].id);
    assert_eq!(first[0]["workspace"], w.app.projects[0].workspaces[0].id);
    assert_eq!(first[0]["panes"].as_array().map(Vec::len), Some(1));
    assert_eq!(again.len(), 0);
    assert_eq!(last.len(), 1);
}

#[test]
fn a_token_moves_the_asking_view_and_leaves_the_window() {
    let mut w = Watched::start(agents::Known::Claude, true);
    let view = open(&mut w, 1, &sized());
    w.report("waiting", 6);
    let token = w.token().expect("a token");
    let before = w.app.view_projection(view);

    let asker = Asker { caller: None, view: Some(view) };
    w.app.go_request(asker, &Go { token }).expect("go");

    assert_eq!(w.app.view_ids(view).tab, Some(w.app.projects[0].workspaces[0].tabs[0].id));
    assert!(w.app.view_projection(view) > before);
    assert_eq!(active_tab(&w.app), 1);
    assert!(w.token().is_none());
}

#[test]
fn a_token_without_a_view_moves_the_window_like_a_click() {
    let mut w = Watched::start(agents::Known::Claude, true);
    w.report("waiting", 6);
    let token = w.token().expect("a token");

    let asker = Asker { caller: None, view: None };
    w.app.go_request(asker, &Go { token }).expect("go");

    assert_eq!(active_tab(&w.app), 0);
}

#[test]
fn a_token_nobody_gave_or_whose_agent_is_gone_is_not_found() {
    let mut w = Watched::start(agents::Known::Claude, true);
    let asker = || Asker { caller: None, view: None };
    let unknown = w.app.go_request(asker(), &Go { token: "cc-nope-1".into() }).expect_err("unknown");
    assert_eq!(unknown.code, Code::NotFound);

    w.report("waiting", 6);
    let token = w.token().expect("a token");
    let project = w.app.projects[0].id;
    let other = TempDir::new();
    w.app.open_project(other.path().to_path_buf(), AREA).expect("another project");
    w.app.close_project(project);
    let gone = w.app.go_request(asker(), &Go { token }).expect_err("gone");

    assert_eq!(gone.code, Code::NotFound);
    assert!(gone.message.contains("is gone"), "{gone:?}");
}

#[test]
fn with_a_view_open_the_windows_tab_counts_only_while_a_window_is_attached() {
    let mut w = Watched::start(agents::Known::Claude, false);
    open(&mut w, 0, &OpenView::default());
    w.report("waiting", 6);
    assert!(w.token().is_some());

    w.app.window_attached();
    w.report("busy", 1);
    w.app.toast = None;
    w.report("waiting", 6);

    assert!(w.token().is_none());
}

#[test]
fn a_view_set_hidden_notifies_again_and_visible_sees_again() {
    let mut w = Watched::start(agents::Known::Claude, true);
    let view = open(&mut w, 0, &sized());
    w.app.view_set_request(view, &SetView { visible: Some(false), ..SetView::default() }, Some(AREA)).expect("set");
    w.report("waiting", 6);
    assert!(w.token().is_some());

    w.app.view_set_request(view, &SetView { visible: Some(true), ..SetView::default() }, Some(AREA)).expect("set");
    w.app.toast = None;
    w.report("busy", 1);
    w.report("waiting", 6);

    assert!(w.token().is_none());
}
