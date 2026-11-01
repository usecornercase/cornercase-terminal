use rstest::rstest;
use serde_json::{Value, json};

use super::*;
use crate::app::machines::acting::Aim;
use crate::app::machines::{Flow, RemoteItem};
use crate::control::contract::Structured;
use crate::control::{Item, StreamState};
use crate::machines::act::Act;
use crate::machines::link::fake::ended;
use crate::machines::stream::fake::{self, There};
use crate::protocol::ServerMessage;

const VIEW: u64 = 7;
const THERE: There = There { boot: BOOT, view: VIEW, tab: 13 };
const WIDE: Rect = Rect { x: 0, y: 0, width: 160, height: 40 };

fn reach(ends: &Ends) -> UnixStream {
    wait_until("the stream connects", || !ends.lock().is_empty());
    let theirs = ends.lock().remove(0);
    theirs.set_read_timeout(Some(Duration::from_secs(10))).expect("a read timeout");
    theirs
}

fn say(theirs: &mut UnixStream, value: &Value) {
    protocol::send(theirs, &ServerMessage::Response(value.to_string())).expect("answer the stream");
}

fn windowed() -> Seeing {
    let mut seeing = seeing_two();
    seeing.rig.app.window_attached();
    seeing.rig.app.resize(AREA);
    seeing
}

struct Streaming {
    seeing: Seeing,
    view: UnixStream,
    surface: UnixStream,
}

fn streaming() -> Streaming {
    let mut seeing = windowed();
    assert!(seeing.rig.app.select_remote("vps", RemoteItem::Workspace(10)));
    let mut view = reach(&seeing.ends);
    assert_eq!(request(&mut view)["command"], "view");
    say(&mut view, &THERE.view_line(1));
    let mut surface = reach(&seeing.ends);
    assert_eq!(request(&mut surface)["command"], "surface");
    Streaming { seeing, view, surface }
}

fn live(streaming: &mut Streaming, text: &str) {
    say(&mut streaming.surface, &THERE.complete(1, 1, text));
    streaming.seeing.rig.until("the frame arrives", |app| shown(app).as_deref() == Some(text));
}

fn stream_fallen() -> (Seeing, UnixStream) {
    let mut streaming = streaming();
    live(&mut streaming, "hello");
    let Streaming { mut seeing, view, surface } = streaming;
    drop(surface);
    seeing.rig.until("the stream falls", |app| matches!(flow(app), Some(Flow::Down(_))));
    (seeing, view)
}

fn fall(seeing: &mut Seeing) {
    drop(std::mem::replace(&mut seeing.theirs, UnixStream::pair().expect("a pair").0));
    seeing.rig.until("the machine falls", |app| app.machines.held()[0].status.state == State::Unreachable);
}

fn flow(app: &App) -> Option<Flow> {
    app.watched().map(|watched| watched.flow.clone())
}

fn shown(app: &App) -> Option<String> {
    app.watched()?.screen.as_deref().map(fake::text)
}

fn size(area: Rect) -> (Value, Value) {
    (json!(area.width), json!(area.height))
}

fn asked_size(asked: &Value) -> (Value, Value) {
    (asked["args"]["cols"].clone(), asked["args"]["rows"].clone())
}

mod opening {
    use super::*;

    #[test]
    fn a_workspace_there_opens_a_view_on_its_active_tab_at_the_pane_area_s_size() {
        let mut seeing = windowed();

        assert!(seeing.rig.app.select_remote("vps", RemoteItem::Workspace(10)));

        let asked = request(&mut reach(&seeing.ends));
        assert_eq!((&asked["command"], &asked["args"]["tab"]), (&json!("view"), &json!(13)));
        assert_eq!(asked_size(&asked), size(seeing.rig.app.layout(AREA).pane));
        assert_eq!(flow(&seeing.rig.app), Some(Flow::Connecting));
    }

    #[test]
    fn its_frames_reach_the_app_once_the_view_s_surface_streams() {
        let mut streaming = streaming();
        assert_eq!(shown(&streaming.seeing.rig.app), None);

        live(&mut streaming, "hello");

        assert_eq!(flow(&streaming.seeing.rig.app), Some(Flow::Live));
    }

    #[test]
    fn nothing_opens_without_a_window() {
        let mut seeing = seeing_two();
        seeing.rig.app.resize(AREA);

        assert!(seeing.rig.app.select_remote("vps", RemoteItem::Workspace(10)));
        seeing.rig.refresh(Duration::from_secs(1));

        assert!(seeing.rig.app.watched().is_none());
        assert_eq!(seeing.rig.seen("start vps.lan"), 1);
    }
}

mod moving {
    use super::*;

    #[test]
    fn another_tab_of_the_workspace_moves_the_view_without_a_new_stream() {
        let mut streaming = streaming();

        assert!(streaming.seeing.rig.app.select_remote("vps", RemoteItem::Tab(11)));

        let asked = request(&mut reach(&streaming.seeing.ends));
        assert_eq!((&asked["command"], &asked["view"]), (&json!("focus"), &json!(VIEW)));
        assert_eq!(asked["args"]["item"], serde_json::to_value(Item::Tab(11)).expect("an item"));
        assert_eq!(streaming.seeing.rig.seen("start vps.lan"), 2);
    }

    #[test]
    fn another_workspace_ends_the_stream_and_opens_one_on_its_tab() {
        let Streaming { mut seeing, mut view, mut surface } = streaming();

        assert!(seeing.rig.app.select_remote("vps", RemoteItem::Workspace(20)));

        assert!(ended(&mut view) && ended(&mut surface));
        assert_eq!(request(&mut reach(&seeing.ends))["args"]["tab"], json!(23));
    }
}

mod ending {
    use super::*;

    #[derive(Debug, Clone, Copy)]
    enum Away {
        Here,
        Removed,
        WindowGone,
        Stopped,
    }

    #[rstest]
    #[case::picking_something_here(Away::Here)]
    #[case::taking_the_machine_out(Away::Removed)]
    #[case::the_last_window_going(Away::WindowGone)]
    #[case::stopping_the_server(Away::Stopped)]
    fn ends_its_lines(#[case] away: Away) {
        let Streaming { mut seeing, mut view, mut surface } = streaming();

        match away {
            Away::Here => seeing.rig.app.leave_remote(),
            Away::Removed => seeing.rig.edit(Vec::new(), Duration::from_secs(2)),
            Away::WindowGone => seeing.rig.app.window_gone(),
            Away::Stopped => seeing.rig.app.stop_background(),
        }

        assert!(ended(&mut view) && ended(&mut surface));
        assert!(seeing.rig.app.watched().is_none());
    }

    #[test]
    fn a_fallen_link_ends_it_and_keeps_the_last_frame() {
        let mut streaming = streaming();
        live(&mut streaming, "hello");
        let Streaming { mut seeing, mut view, mut surface } = streaming;

        fall(&mut seeing);

        assert!(ended(&mut view) && ended(&mut surface));
        assert!(matches!(flow(&seeing.rig.app), Some(Flow::Down(_))));
        assert_eq!(shown(&seeing.rig.app).as_deref(), Some("hello"));
    }

    #[test]
    fn the_link_back_opens_it_again_on_the_same_tab() {
        let Streaming { mut seeing, view, surface } = streaming();
        fall(&mut seeing);
        drop((view, surface));

        seeing.theirs = linked(&seeing.ends);
        serve(&mut seeing.theirs, 1, worked(report(&[("web-shop", Some(SHOP)), ("blog", None)])));
        seeing.rig.until("the machine is seen again", |app| app.machines.held()[0].watching);

        let asked = request(&mut reach(&seeing.ends));
        assert_eq!((&asked["command"], &asked["args"]["tab"]), (&json!("view"), &json!(13)));
        assert_eq!(flow(&seeing.rig.app), Some(Flow::Connecting));
    }

    #[rstest]
    #[case::closing_it(Act::Close)]
    #[case::removing_its_worktree(Act::Remove { force: false, unlock: false })]
    fn acting_on_it_from_its_row_ends_its_lines_once_the_machine_drops_it(#[case] act: Act) {
        let Streaming { mut seeing, mut view, mut surface } = streaming();
        let era = seeing.rig.app.machines.held()[0].era;

        seeing.rig.app.send_act(Aim { link: seeing.rig.links()[0], era, workspace: 10 }, act);
        let mut acted = reach(&seeing.ends);
        assert_eq!(request(&mut acted)["command"], "close");
        let done = serde_json::to_string(&Structured::Ok(json!({}))).expect("an answer");
        protocol::send(&mut acted, &ServerMessage::Response(done)).expect("answer the act");
        let mut dropped = worked(report(&[("web-shop", Some(SHOP)), ("blog", None)]));
        dropped.projects[0].workspaces.clear();
        serve(&mut seeing.theirs, 2, dropped);
        seeing.rig.until("the workspace leaves", |app| app.shown_remote().is_none());

        assert!(ended(&mut view) && ended(&mut surface));
        assert!(seeing.rig.app.watched().is_none());
    }

    #[test]
    fn a_fallen_stream_is_down_and_keeps_the_last_frame() {
        let (seeing, _view) = stream_fallen();

        assert_eq!(shown(&seeing.rig.app).as_deref(), Some("hello"));
    }
}

mod drawn {
    use super::*;

    fn card(app: &App) -> Option<ui::RemoteCard> {
        app.remote_card(Instant::now())
    }

    fn tab_down(app: &App) -> Option<bool> {
        app.remote_screen().map(|tab| tab.down)
    }

    #[test]
    fn before_a_frame_is_the_machine_s_card_alone() {
        let streaming = streaming();
        let app = &streaming.seeing.rig.app;

        assert_eq!((tab_down(app), card(app).map(|card| card.down)), (None, Some(None)));
    }

    #[test]
    fn a_live_frame_shows_without_the_card() {
        let mut streaming = streaming();

        live(&mut streaming, "hello");

        let app = &streaming.seeing.rig.app;
        assert_eq!((tab_down(app), card(app).map(|card| card.down)), (Some(false), None));
    }

    #[test]
    fn a_fallen_link_dims_the_frame_under_its_countdown() {
        let mut streaming = streaming();
        live(&mut streaming, "hello");

        fall(&mut streaming.seeing);

        let app = &streaming.seeing.rig.app;
        assert_eq!(tab_down(app), Some(true));
        let down = card(app).and_then(|card| card.down);
        assert!(matches!(down, Some(ui::Retry::In(_))), "{down:?}");
    }

    #[test]
    fn a_stream_fallen_before_a_frame_shows_the_card_reconnecting() {
        let Streaming { mut seeing, view: _view, surface } = streaming();

        drop(surface);

        seeing.rig.until("the stream falls", |app| matches!(flow(app), Some(Flow::Down(_))));
        assert_eq!(card(&seeing.rig.app).map(|card| card.down), Some(Some(ui::Retry::Now)));
    }

    #[test]
    fn a_fallen_stream_dims_the_frame_under_a_card_that_reconnects() {
        let (seeing, _view) = stream_fallen();

        let app = &seeing.rig.app;
        assert_eq!((tab_down(app), card(app).map(|card| card.down)), (Some(true), Some(Some(ui::Retry::Now))));
    }
}

mod in_status {
    use super::*;
    use crate::app::events::Streamed;
    use crate::control::{Command, Request, Response, Snapshot, Watch};

    fn stream(app: &App) -> Option<StreamState> {
        app.report(None).shown_remote?.stream
    }

    fn streamed(snapshot: Snapshot) -> Option<StreamState> {
        snapshot.report.shown_remote?.stream
    }

    fn watch(app: &mut App) -> Option<StreamState> {
        app.clients.events.snapshots.set_every(Duration::ZERO);
        let command = Command::Watch(Watch::default());
        let asked = Request { caller: None, server: None, contract: None, boot: None, view: None, command };
        app.request(1, &serde_json::to_string(&asked).expect("a request"), None, Instant::now());
        let answers = app.take_answers();
        let Some(Ok(Response::Ok(first))) = answers.first().map(|(_, text)| serde_json::from_str(text)) else {
            panic!("no snapshot: {answers:?}")
        };
        streamed(serde_json::from_value(first).expect("a snapshot"))
    }

    fn published(app: &mut App) -> Vec<Option<StreamState>> {
        let snapshot = |text: &str| match serde_json::from_str(text) {
            Ok(Response::Ok(value)) => serde_json::from_value(value).expect("a snapshot"),
            other => panic!("not a snapshot: {other:?}"),
        };
        let events = app.take_events().into_iter();
        events
            .filter_map(|(_, sent)| if let Streamed::Snapshot(text) = sent { Some(snapshot(&text)) } else { None })
            .map(streamed)
            .collect()
    }

    #[test]
    fn tells_a_watcher_once_the_first_frame_makes_it_live() {
        let mut streaming = streaming();
        let app = &mut streaming.seeing.rig.app;
        let before = watch(app);
        app.trace_refresh();
        published(app);

        live(&mut streaming, "hello");
        let app = &mut streaming.seeing.rig.app;
        app.trace_refresh();

        assert_eq!(
            (before, published(app).last().copied().flatten()),
            (Some(StreamState::Connecting), Some(StreamState::Live))
        );
    }

    #[test]
    fn says_connecting_then_live() {
        let mut streaming = streaming();
        let before = stream(&streaming.seeing.rig.app);

        live(&mut streaming, "hello");

        let now = stream(&streaming.seeing.rig.app);
        assert_eq!((before, now), (Some(StreamState::Connecting), Some(StreamState::Live)));
    }

    #[test]
    fn says_down_once_the_link_falls() {
        let mut streaming = streaming();
        live(&mut streaming, "hello");

        fall(&mut streaming.seeing);

        assert_eq!(stream(&streaming.seeing.rig.app), Some(StreamState::Down));
    }

    #[test]
    fn leaves_it_out_without_a_window() {
        let mut seeing = seeing_two();
        seeing.rig.app.resize(AREA);

        assert!(seeing.rig.app.select_remote("vps", RemoteItem::Workspace(10)));

        let written = serde_json::to_value(seeing.rig.app.report(None)).expect("json");
        let shown = &written["shown_remote"];
        assert!(shown.is_object() && shown.get("stream").is_none(), "{written}");
    }
}

mod sizes {
    use super::*;

    #[test]
    fn follow_the_pane_area_with_one_view_set_per_change() {
        let mut streaming = streaming();
        let app = &mut streaming.seeing.rig.app;
        let wide = size(app.layout(WIDE).pane);
        app.resize(WIDE);
        let mut orders = reach(&streaming.seeing.ends);
        let first = request(&mut orders);
        say(&mut orders, &json!({"ok": {"id": VIEW, "projection": 2}}));

        let app = &mut streaming.seeing.rig.app;
        app.resize(WIDE);
        app.show_side(Some(Side::Todo));
        let narrow = size(app.layout(WIDE).pane);
        app.resize(WIDE);
        let next = request(&mut orders);

        assert_eq!((&first["command"], asked_size(&first)), (&json!("view-set"), wide));
        assert_eq!(asked_size(&next), narrow);
        assert_ne!(asked_size(&first), narrow);
    }
}

mod server_events {
    use std::sync::Arc;

    use super::*;
    use crate::app::machines::MachineEvent;
    use crate::machines::stream::screen::RemoteScreen;
    use crate::machines::stream::{StreamEvent, StreamReport};

    fn stream(event: StreamEvent) -> AppEvent {
        AppEvent::Machine(MachineEvent::Stream(StreamReport { stream: 1, event }))
    }

    #[rstest]
    #[case::a_frame(stream(StreamEvent::Frame(Arc::new(RemoteScreen::default()))), true)]
    #[case::an_opened_view(stream(StreamEvent::Opened { view: VIEW }), false)]
    #[case::a_fall(stream(StreamEvent::Down("gone".into())), false)]
    #[case::other_machine_news(AppEvent::Machine(MachineEvent::Stats { link: 1, result: Err("x".into()) }), false)]
    #[case::exit(AppEvent::Exited(1, 0), false)]
    fn only_a_frame_is_drawn_like_a_pane_s_output(#[case] event: AppEvent, #[case] frame: bool) {
        assert_eq!(event.remote_frame(), frame);
    }
}

mod mouse;
mod typing;
