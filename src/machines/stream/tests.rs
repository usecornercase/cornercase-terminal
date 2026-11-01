use std::os::unix::net::UnixStream;
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use serde_json::json;

use super::alarm::{Armed, Bell};
use super::fake::{self, SURFACE, There};
use super::*;
use crate::error::Result;
use crate::machines::link::fake::{ended, line, refused, request};
use crate::protocol::{self, ServerMessage};
use crate::remote::Killer;
use crate::test_util;

pub(super) const WAIT: Duration = Duration::from_secs(10);
pub(super) const BOOT: &str = "4242-7";
pub(super) const VIEW: u64 = 7;
pub(super) const TAB: u64 = 9;
const THERE: There = There { boot: BOOT, view: VIEW, tab: TAB };

struct Script {
    lines: Mutex<Receiver<Line>>,
}

impl Connect for Script {
    fn connect(&self, _: &Killer) -> Result<Line> {
        self.lines.lock().recv().map_err(|_| refused())
    }
}

struct Ticks(Sender<Duration>);

struct Bells(Mutex<Sender<Arc<Armed>>>);

impl Bell for Bells {
    fn arm(&self, armed: Arc<Armed>, ends: Arc<dyn Ends>, _: Duration) {
        let _ = self.0.lock().send(Arc::clone(&armed));
        thread::spawn(move || {
            if armed.wait() {
                ends.cut();
            }
        });
    }
}

impl Clock for Ticks {
    fn nap(&self, control: &Control, wait: Duration) -> Asked {
        let _ = self.0.send(wait);
        control.nap(Duration::ZERO)
    }
}

pub(super) struct Rig {
    pub(super) stream: Stream,
    lines: Sender<Line>,
    reports: Receiver<StreamReport>,
    waits: Receiver<Duration>,
    alarms: Receiver<Arc<Armed>>,
}

impl Rig {
    pub(super) fn new() -> Self {
        let (lines, script) = mpsc::channel();
        let (tx, reports) = mpsc::channel();
        let (napped, waits) = mpsc::channel();
        let (armed, alarms) = mpsc::channel();
        let connector: Arc<dyn Connect> = Arc::new(Script { lines: Mutex::new(script) });
        let wanted = Wanted { tab: TAB, cols: 80, rows: 24 };
        let alarms_here = Alarms::new(Arc::new(Bells(Mutex::new(armed))), WAIT);
        let timing = Timing { clock: Box::new(Ticks(napped)), alarms: alarms_here };
        let stream = Stream::spawn(1, connector, wanted, &tx, timing);
        Self { stream, lines, reports, waits, alarms }
    }

    pub(super) fn alarm(&self) -> Arc<Armed> {
        self.alarms.recv_timeout(WAIT).expect("an alarm is set")
    }

    pub(super) fn reach(&self) -> UnixStream {
        let (line, theirs) = line("0.12.225", None);
        theirs.set_read_timeout(Some(WAIT)).expect("a read timeout");
        self.lines.send(line).expect("the stream connects");
        theirs
    }

    pub(super) fn event(&self) -> StreamEvent {
        self.reports.recv_timeout(WAIT).expect("an event from the stream").event
    }

    fn waited(&self) -> Duration {
        self.waits.recv_timeout(WAIT).expect("the stream waits")
    }

    pub(super) fn viewing(&self) -> UnixStream {
        let mut view = self.reach();
        assert_eq!(request(&mut view)["command"], "view");
        say(&mut view, &view_line(1));
        assert_eq!(self.event(), StreamEvent::Opened { view: VIEW });
        view
    }

    pub(super) fn opened(&self) -> (UnixStream, UnixStream) {
        let view = self.viewing();
        let mut surface = self.reach();
        assert_eq!(request(&mut surface)["command"], "surface");
        (view, surface)
    }
}

pub(super) fn say(theirs: &mut UnixStream, value: &Value) {
    protocol::send(theirs, &ServerMessage::Response(value.to_string())).expect("answer the stream");
}

fn view_line(projection: u64) -> Value {
    THERE.view_line(projection)
}

pub(super) fn complete(revision: u64, projection: u64, text: &str) -> Value {
    THERE.complete(revision, projection, text)
}

fn patch(revision: u64, base: u64, text: &str) -> Value {
    THERE.patch(revision, base, text)
}

pub(super) fn shown(event: StreamEvent) -> String {
    match event {
        StreamEvent::Frame(screen) => fake::text(&screen),
        other => panic!("not a frame: {other:?}"),
    }
}

fn live(rig: &Rig) -> (UnixStream, UnixStream) {
    let (view, mut surface) = rig.opened();
    say(&mut surface, &complete(1, 1, "hello"));
    rig.event();
    (view, surface)
}

fn no_surface(orders: &mut UnixStream) {
    say(orders, &json!({"error": {"code": "not-found", "message": "no surface 3"}}));
}

fn lost() -> StreamEvent {
    StreamEvent::Down("the link ended".into())
}

mod opening {
    use super::*;

    #[test]
    fn opens_a_visible_active_view_on_the_tab_at_its_size() {
        let rig = Rig::new();
        let mut view = rig.reach();

        let asked = request(&mut view);

        assert_eq!(asked["command"], "view");
        assert_eq!(
            asked["args"],
            json!({"cols": 80, "rows": 24, "active": true, "tab": TAB, "visible": true, "workspace": null, "project": null})
        );
        assert_eq!(asked.get("view"), None);
    }

    #[test]
    fn then_streams_the_view_s_surface_with_patches() {
        let rig = Rig::new();
        let _view = rig.viewing();
        let mut surface = rig.reach();

        let asked = request(&mut surface);

        assert_eq!(
            (&asked["command"], &asked["view"], &asked["boot"]),
            (&json!("surface"), &json!(VIEW), &json!(BOOT))
        );
        assert_eq!((&asked["args"]["patches"], &asked["args"]["tab"]), (&json!(true), &Value::Null));
    }

    #[test]
    fn its_reports_carry_its_id() {
        let rig = Rig::new();
        let mut view = rig.reach();
        request(&mut view);

        say(&mut view, &view_line(1));

        let report = rig.reports.recv_timeout(WAIT).expect("a report");
        assert_eq!((report.stream, report.event), (rig.stream.id(), StreamEvent::Opened { view: VIEW }));
    }

    #[test]
    fn a_size_too_big_for_a_view_is_cut_to_the_largest() {
        let (lines, script) = mpsc::channel();
        let (tx, _reports) = mpsc::channel::<StreamReport>();
        let connector = Box::new(Script { lines: Mutex::new(script) });
        let _stream = Stream::start(1, connector, Wanted { tab: TAB, cols: 5000, rows: 0 }, &tx);
        let (line, mut view) = line("0.12.225", None);
        lines.send(line).expect("the stream connects");

        let asked = request(&mut view);

        assert_eq!((&asked["args"]["cols"], &asked["args"]["rows"]), (&json!(1000), &json!(1)));
    }

    #[test]
    fn a_refused_view_tells_why_and_tries_again_later() {
        let rig = Rig::new();
        let mut view = rig.reach();
        request(&mut view);

        say(&mut view, &json!({"error": "no tab 9\u{1b}[2J"}));

        assert_eq!(rig.event(), StreamEvent::Down("no tab 9[2J".into()));
        assert_eq!(rig.waited(), remote::backoff(1));
    }
}

mod frames {
    use super::*;

    #[test]
    fn a_complete_frame_and_a_patch_arrive_as_frames() {
        let rig = Rig::new();
        let (_view, mut surface) = rig.opened();

        say(&mut surface, &complete(1, 1, "hello"));
        let first = shown(rig.event());
        say(&mut surface, &patch(2, 1, "world"));

        assert_eq!((first.as_str(), shown(rig.event()).as_str()), ("hello", "world"));
    }

    #[test]
    fn a_frame_of_an_older_projection_is_not_shown() {
        let rig = Rig::new();
        let (_view, mut surface) = rig.opened();
        say(&mut surface, &complete(1, 2, "new"));
        rig.event();

        say(&mut surface, &complete(2, 1, "old"));
        say(&mut surface, &complete(3, 2, "newer"));

        assert_eq!(shown(rig.event()), "newer");
    }

    #[test]
    fn the_view_s_lines_raise_the_projection_a_frame_needs() {
        let rig = Rig::new();
        let (mut view, mut surface) = rig.opened();

        say(&mut view, &view_line(3));
        test_util::wait_until("the view's line is read", || rig.stream.shared.fence.lock().projection == 3);
        say(&mut surface, &complete(1, 2, "old"));
        say(&mut surface, &complete(2, 3, "new"));

        assert_eq!(shown(rig.event()), "new");
    }

    #[test]
    fn a_projection_from_another_view_holds_back_no_frame() {
        let rig = Rig::new();
        let (_view, mut surface) = rig.opened();

        rig.stream.shared.seen(VIEW - 1, 9);
        say(&mut surface, &complete(1, 1, "new"));

        assert_eq!(shown(rig.event()), "new");
    }

    #[test]
    fn a_frame_of_another_server_run_is_not_shown() {
        let rig = Rig::new();
        let (_view, mut surface) = rig.opened();
        let mut other = complete(1, 1, "other");
        other["ok"]["boot"] = json!("9999-1");

        say(&mut surface, &other);
        say(&mut surface, &complete(2, 1, "ours"));

        assert_eq!(shown(rig.event()), "ours");
    }

    #[test]
    fn a_patch_that_does_not_fit_asks_for_a_redraw_once() {
        let rig = Rig::new();
        let (_view, mut surface) = live(&rig);

        say(&mut surface, &patch(5, 4, "lost"));
        say(&mut surface, &patch(6, 5, "lost too"));
        let mut orders = rig.reach();
        let asked = request(&mut orders);
        say(&mut orders, &json!({"ok": {"surface": SURFACE, "tab": TAB}}));
        say(&mut surface, &complete(7, 1, "again"));

        assert_eq!((&asked["command"], &asked["args"]), (&json!("redraw"), &json!({"surface": SURFACE})));
        assert_eq!((&asked["boot"], asked.get("view")), (&json!(BOOT), None));
        assert_eq!(shown(rig.event()), "again");
    }

    #[test]
    fn a_refused_redraw_opens_the_view_and_its_surface_again() {
        let rig = Rig::new();
        let (mut view, mut surface) = live(&rig);
        say(&mut surface, &patch(5, 4, "lost"));
        let mut orders = rig.reach();
        request(&mut orders);

        no_surface(&mut orders);

        assert!(ended(&mut surface) && ended(&mut view));
        assert_eq!((rig.event(), rig.waited()), (StreamEvent::Down(REDRAWN.into()), remote::backoff(1)));
        assert_eq!(request(&mut rig.reach())["command"], "view");
    }

    #[test]
    fn a_redraw_refused_again_before_a_frame_waits_longer() {
        let rig = Rig::new();
        let (_view, mut surface) = live(&rig);
        say(&mut surface, &patch(5, 4, "lost"));
        let mut orders = rig.reach();
        request(&mut orders);
        no_surface(&mut orders);
        rig.event();
        rig.waited();

        let (_view, mut surface) = rig.opened();
        say(&mut surface, &patch(6, 5, "lost"));
        request(&mut orders);
        no_surface(&mut orders);

        assert_eq!((rig.event(), rig.waited()), (StreamEvent::Down(REDRAWN.into()), remote::backoff(2)));
    }

    #[test]
    fn a_frame_that_cannot_be_read_ends_the_stream() {
        let rig = Rig::new();
        let (mut view, mut surface) = rig.opened();

        say(&mut surface, &json!({"ok": 7}));

        assert_eq!(rig.event(), StreamEvent::Down(UNREADABLE.into()));
        assert!(ended(&mut view));
    }

    #[test]
    fn a_refused_surface_tells_the_message_cleaned() {
        let rig = Rig::new();
        let (_view, mut surface) = rig.opened();

        say(&mut surface, &json!({"error": {"code": "failed", "message": "view 7 streams nothing\u{7} now"}}));

        assert_eq!(rig.event(), StreamEvent::Down("view 7 streams nothing now".into()));
    }
}

mod ending {
    use super::*;

    #[test]
    fn the_surface_ending_tells_down_and_opens_both_again() {
        let rig = Rig::new();
        let (mut view, surface) = live(&rig);

        drop(surface);

        assert_eq!(rig.event(), lost());
        assert_eq!(rig.waited(), remote::backoff(1));
        assert!(ended(&mut view));
        assert_eq!(request(&mut rig.reach())["command"], "view");
    }

    #[test]
    fn the_view_ending_ends_its_surface_too() {
        let rig = Rig::new();
        let (view, mut surface) = rig.opened();

        drop(view);

        assert!(ended(&mut surface));
        assert_eq!(rig.event(), lost());
    }

    #[test]
    fn failing_again_before_a_frame_waits_longer() {
        let rig = Rig::new();
        let (_view, surface) = rig.opened();
        drop(surface);
        rig.event();
        rig.waited();

        let (_view, surface) = rig.opened();
        drop(surface);

        assert_eq!(rig.event(), lost());
        assert_eq!(rig.waited(), remote::backoff(2));
    }

    #[test]
    fn dropping_the_stream_closes_the_view_and_the_surface() {
        let rig = Rig::new();
        let (mut view, mut surface) = rig.opened();

        drop(rig.stream);

        assert!(ended(&mut view) && ended(&mut surface));
    }
}

mod silence {
    use super::*;

    #[test]
    fn a_view_that_never_answers_is_down_in_time_and_tried_again() {
        let rig = Rig::new();
        let mut view = rig.reach();
        request(&mut view);

        rig.alarm().ring();

        assert_eq!(rig.event(), StreamEvent::Down(LATE.into()));
        assert!(ended(&mut view));
        assert_eq!(request(&mut rig.reach())["command"], "view");
    }

    #[test]
    fn a_surface_that_never_sends_a_frame_is_down_in_time() {
        let rig = Rig::new();
        let (_view, mut surface) = rig.opened();

        rig.alarm().ring();

        assert_eq!(rig.event(), StreamEvent::Down(LATE.into()));
        assert!(ended(&mut surface));
    }

    #[test]
    fn a_stream_that_shows_frames_is_not_cut() {
        let rig = Rig::new();
        let (_view, mut surface) = rig.opened();
        let opening = rig.alarm();

        say(&mut surface, &complete(1, 1, "hello"));
        rig.event();

        opening.ring();
        say(&mut surface, &complete(2, 1, "still"));
        assert_eq!(shown(rig.event()), "still");
    }
}
