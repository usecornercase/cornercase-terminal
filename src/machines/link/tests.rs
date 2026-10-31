use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, TryRecvError};

use rstest::rstest;
use serde_json::{Value, json};

use super::fake::{answer, ended, line, request, respond, serve};
use super::*;
use crate::control::{GroupInfo, Ids, PaneInfo, ProjectInfo, TabInfo, WorkspaceInfo};
use crate::error::Behind;
use crate::host::open::{Browse, Open, Opener};
use crate::host::sound::{Cue, Kind};

const WAIT: Duration = Duration::from_secs(10);
const HERE: &str = "4242-1";
const VERSION: &str = "0.12.225";
const REFUSED: &str = "ssh: connect to host down port 22: Connection refused";

struct Script {
    attempts: Mutex<Receiver<Result<Line>>>,
    tried: Sender<()>,
}

impl Connect for Script {
    fn connect(&self, _: &Killer) -> Result<Line> {
        let _ = self.tried.send(());
        self.attempts
            .lock()
            .recv()
            .unwrap_or_else(|_| Err(Error::Ssh { host: "vps".into(), reason: "no attempt".into() }))
    }
}

struct Ticks {
    waits: Sender<Duration>,
    held: bool,
}

impl Clock for Ticks {
    fn nap(&self, control: &Control, wait: Duration) -> Asked {
        let _ = self.waits.send(wait);
        control.nap(if self.held { wait } else { Duration::ZERO })
    }
}

struct Broken(AtomicBool);

impl Connect for Broken {
    fn connect(&self, _: &Killer) -> Result<Line> {
        assert!(self.0.swap(true, Ordering::SeqCst), "a connector bug");
        Err(refused())
    }
}

struct Tripwire {
    inner: Box<dyn Read + Send>,
    armed: Arc<AtomicBool>,
}

impl Read for Tripwire {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let read = self.inner.read(buf)?;
        assert!(!self.armed.load(Ordering::SeqCst), "a reader bug");
        Ok(read)
    }
}

struct Hanging;

impl Connect for Hanging {
    fn connect(&self, killer: &Killer) -> Result<Line> {
        let mut ssh = std::process::Command::new("/bin/sleep").arg("60").spawn().expect("run sleep");
        killer.arm(&ssh);
        let _ = ssh.wait();
        killer.disarm();
        Err(refused())
    }
}

struct Rig {
    link: Link,
    attempts: Sender<Result<Line>>,
    reports: Receiver<Report>,
    waits: Receiver<Duration>,
    tried: Receiver<()>,
}

impl Rig {
    fn new(id: u64, servers: &Servers) -> Self {
        Self::with(id, servers, Some(false))
    }

    fn held(id: u64, servers: &Servers) -> Self {
        Self::with(id, servers, Some(true))
    }

    fn on_the_wall(id: u64, servers: &Servers) -> Self {
        Self::with(id, servers, None)
    }

    fn with(id: u64, servers: &Servers, ticks: Option<bool>) -> Self {
        let (attempts, script) = mpsc::channel();
        let (tx, reports) = mpsc::channel();
        let (sender, waits) = mpsc::channel();
        let (signal, tried) = mpsc::channel();
        let connector = Box::new(Script { attempts: Mutex::new(script), tried: signal });
        let link = match ticks {
            Some(held) => Link::spawn(id, connector, servers, &tx, Box::new(Ticks { waits: sender, held })),
            None => Link::start(id, connector, servers, &tx),
        };
        Self { link, attempts, reports, waits, tried }
    }

    fn tried(&self, times: usize) {
        for _ in 0..times {
            self.tried.recv_timeout(WAIT).expect("the link connects");
        }
    }

    fn fail(&self, error: Error) {
        self.attempts.send(Err(error)).expect("the link asks for an attempt");
    }

    fn reach(&self, release: &str, why: Option<&str>) -> UnixStream {
        let (line, theirs) = line(release, why);
        self.attempts.send(Ok(line)).expect("the link asks for an attempt");
        theirs
    }

    fn linked(&self, boot: &str) -> UnixStream {
        let mut theirs = self.reach(VERSION, None);
        answer(&mut theirs, boot, VERSION);
        assert_eq!(self.event(), Event::Linked { release: VERSION.into(), server_version: VERSION.into() });
        assert_eq!(request(&mut theirs)["command"], "watch");
        theirs
    }

    fn event(&self) -> Event {
        self.reports.recv_timeout(WAIT).expect("an event from the link").event
    }

    fn waited(&self) -> Duration {
        self.waits.recv_timeout(WAIT).expect("the link waits")
    }
}

fn down(down: Down, failed: u32) -> Event {
    Event::Down { down, failed }
}

fn refused() -> Error {
    Error::Ssh { host: "down".into(), reason: REFUSED.into() }
}

fn behind() -> Error {
    Error::RemoteBehind(Box::new(Behind { host: "vps".into(), there: "0.0.1".into(), update: String::new() }))
}

fn shop(boot: &str) -> control::Report {
    let pane = PaneInfo {
        id: 4,
        agent: Some("claude".into()),
        status: Some("working".into()),
        model: Some("Opus 5.5".into()),
        ..PaneInfo::default()
    };
    let tab = TabInfo { id: 3, name: "agents".into(), panes: vec![pane], ..TabInfo::default() };
    let workspace = WorkspaceInfo {
        id: 2,
        name: "main".into(),
        path: "/home/ana/web-shop".into(),
        branch: Some("main".into()),
        tabs: vec![tab],
        ..WorkspaceInfo::default()
    };
    let project = ProjectInfo {
        id: 1,
        name: "web-shop".into(),
        path: "/home/ana/web-shop".into(),
        workspaces: vec![workspace],
        repo: Some("github.com/acme/web-shop".into()),
        ..ProjectInfo::default()
    };
    control::Report {
        boot: boot.into(),
        home: Some("/home/ana".into()),
        projects: vec![project],
        ..control::Report::default()
    }
}

fn window_messages() -> Vec<ServerMessage> {
    vec![
        ServerMessage::Frame(b"\x1b]52;c;cm0gLXJmIH4K\x07".to_vec()),
        ServerMessage::Rejected("go away".into()),
        ServerMessage::Sound(Cue { kind: Kind::NeedsYou, path: "/tmp/ding.wav".into() }),
        ServerMessage::Open(Open { opener: Opener::Default, path: "/etc/passwd".into(), line: None }),
        ServerMessage::Browse(Browse { ip: None, port: 80 }),
    ]
}

mod waits {
    use super::*;

    #[test]
    fn an_unreachable_machine_is_tried_after_1_2_4_8_then_every_15_seconds() {
        let rig = Rig::new(1, &Servers::new(HERE));
        let mut waits = Vec::new();
        for failed in 1..=6 {
            rig.fail(refused());
            assert_eq!(rig.event(), down(Down::Unreachable(REFUSED.into()), failed));
            waits.push(rig.waited().as_secs());
        }
        assert_eq!(waits, [1, 2, 4, 8, 15, 15]);
    }

    #[rstest]
    #[case::older(behind, Down::Older(Some("0.0.1".into())))]
    #[case::from_before_the_proxy(|| Error::RemoteTooOld("vps".into()), Down::Older(None))]
    #[case::without_cornercase(|| Error::RemoteMissing { host: "vps".into(), option: "" }, Down::Missing)]
    fn a_machine_that_needs_a_person_is_tried_every_minute(#[case] error: fn() -> Error, #[case] expected: Down) {
        let rig = Rig::new(1, &Servers::new(HERE));
        for failed in 1..=2 {
            rig.fail(error());
            assert_eq!(rig.event(), down(expected.clone(), failed));
            assert_eq!(rig.waited(), PERSON);
        }
    }

    #[test]
    fn a_proxy_that_fails_after_its_banner_does_not_reset_them() {
        let rig = Rig::new(1, &Servers::new(HERE));
        for failed in 1..=2 {
            drop(rig.reach(VERSION, Some("could not start a server there")));
            assert_eq!(rig.event(), down(Down::Unreachable("could not start a server there".into()), failed));
        }
        assert_eq!((rig.waited(), rig.waited()), (Duration::from_secs(1), Duration::from_secs(2)));
        drop(rig.linked("vps-1"));
        assert_eq!(rig.event(), down(Down::Unreachable(LOST.into()), 1));
        assert_eq!(rig.waited(), Duration::from_secs(1));
    }

    #[test]
    fn a_link_connects_on_its_own_thread() {
        let rig = Rig::new(1, &Servers::new(HERE));
        assert_eq!(rig.reports.try_recv(), Err(TryRecvError::Empty));
    }
}

mod pokes {
    use super::*;

    #[test]
    fn end_the_wait_of_a_link_that_is_down_and_reset_it() {
        let rig = Rig::held(1, &Servers::new(HERE));
        rig.fail(Error::RemoteTooOld("vps".into()));
        assert_eq!(rig.event(), down(Down::Older(None), 1));
        assert_eq!(rig.waited(), PERSON);
        rig.link.poke();
        rig.fail(refused());
        assert_eq!(rig.event(), down(Down::Unreachable(REFUSED.into()), 1));
    }

    #[test]
    fn while_it_connects_remake_that_connection_once_it_is_up() {
        let rig = Rig::new(1, &Servers::new(HERE));
        rig.tried(1);
        rig.link.poke();
        let mut first = rig.reach(VERSION, None);
        assert!(ended(&mut first));
        drop(rig.linked("vps-1"));
    }

    #[test]
    fn keep_the_server_from_another_machine_while_the_link_is_remade() {
        let servers = Servers::new(HERE);
        let (vps, pi) = (Rig::new(1, &servers), Rig::new(2, &servers));
        let mut first = vps.linked("shared-1");
        let mut refused = pi.reach(VERSION, None);
        answer(&mut refused, "shared-1", VERSION);
        assert_eq!(pi.event(), down(Down::SameServer(1), 1));

        vps.link.poke();
        assert!(ended(&mut first));
        vps.tried(2);
        let mut again = pi.reach(VERSION, None);
        answer(&mut again, "shared-1", VERSION);

        assert_eq!(pi.event(), down(Down::SameServer(1), 2));
        drop(vps.linked("shared-1"));
    }

    #[test]
    fn remake_a_link_that_is_up_without_telling_it_dropped() {
        let rig = Rig::new(1, &Servers::new(HERE));
        let mut first = rig.linked("vps-1");
        rig.link.poke();
        assert!(ended(&mut first));
        drop(rig.linked("vps-1"));
    }
}

mod stopping {
    use super::*;

    #[test]
    fn ends_the_connection_and_the_thread_and_tells_nothing_more() {
        let rig = Rig::on_the_wall(1, &Servers::new(HERE));
        let mut theirs = rig.linked("vps-1");
        let Rig { link, reports, .. } = rig;
        drop(link);
        assert!(ended(&mut theirs));
        assert_eq!(reports.recv_timeout(WAIT), Err(RecvTimeoutError::Disconnected));
    }

    #[test]
    fn while_it_connects_ends_that_connection_unused() {
        let rig = Rig::new(1, &Servers::new(HERE));
        let Rig { link, attempts, reports, .. } = rig;
        drop(link);
        let (line, mut theirs) = line(VERSION, None);
        attempts.send(Ok(line)).expect("the link asks for an attempt");
        assert!(ended(&mut theirs));
        assert_eq!(reports.recv_timeout(WAIT), Err(RecvTimeoutError::Disconnected));
    }

    #[test]
    fn while_ssh_has_not_answered_ends_that_ssh() {
        let (tx, reports) = mpsc::channel::<Report>();
        let link = Link::start(1, Box::new(Hanging), &Servers::new(HERE), &tx);
        drop(tx);
        drop(link);
        assert_eq!(reports.recv_timeout(WAIT), Err(RecvTimeoutError::Disconnected));
    }

    #[test]
    fn a_failure_after_the_link_was_dropped_is_not_told() {
        let rig = Rig::new(7, &Servers::new(HERE));
        let Rig { link, attempts, reports, .. } = rig;
        drop(link);
        attempts.send(Err(refused())).expect("the link asks for an attempt");
        assert_eq!(reports.recv_timeout(WAIT), Err(RecvTimeoutError::Disconnected));
    }
}

mod generations {
    use super::*;

    #[test]
    fn every_event_carries_the_link_s_id() {
        let rig = Rig::new(7, &Servers::new(HERE));
        assert_eq!(rig.link.id(), 7);
        rig.fail(refused());
        assert_eq!(rig.reports.recv_timeout(WAIT).expect("a report").link, 7);
    }
}

mod handshake {
    use super::*;

    #[test]
    fn asks_the_machine_to_start_its_server_then_to_watch_it_on_the_same_connection() {
        let rig = Rig::on_the_wall(1, &Servers::new(HERE));
        let mut theirs = rig.reach(VERSION, None);
        let bare = |asked: &Value| {
            assert!(asked["caller"].is_null() && asked["server"].is_null(), "{asked}");
            assert!(["view", "contract", "boot"].iter().all(|key| asked.get(key).is_none()), "{asked}");
        };
        let asked = request(&mut theirs);
        assert_eq!(asked["command"], "start-server");
        bare(&asked);
        respond(&mut theirs, &Response::Ok(serde_json::to_value(Started::default()).expect("an answer")));
        assert!(matches!(rig.event(), Event::Linked { .. }));
        let watching = request(&mut theirs);
        assert_eq!(watching["command"], "watch");
        bare(&watching);
        drop(rig);
        assert!(ended(&mut theirs));
    }

    #[rstest]
    #[case::refused(Response::Error("no room for a server".into()), "no room for a server")]
    #[case::unreadable(
        Response::Ok(Value::from("ready")),
        "cannot read the answer from there: invalid type: string \"ready\", expected struct Started"
    )]
    fn an_answer_that_is_not_started_is_unreachable(#[case] response: Response, #[case] why: &str) {
        let rig = Rig::new(1, &Servers::new(HERE));
        let mut theirs = rig.reach(VERSION, None);
        request(&mut theirs);
        respond(&mut theirs, &response);
        assert_eq!(rig.event(), down(Down::Unreachable(why.into()), 1));
        assert!(ended(&mut theirs));
    }

    #[test]
    fn a_long_unreadable_answer_is_cut_like_all_it_says() {
        let rig = Rig::new(1, &Servers::new(HERE));
        let mut theirs = rig.reach(VERSION, None);
        request(&mut theirs);
        respond(&mut theirs, &Response::Ok(Value::from("x".repeat(KEPT * 2))));
        let said = "cannot read the answer from there: invalid type: string \"";
        let why = format!("{said}{}", "x".repeat(KEPT - said.len()));
        assert_eq!(rig.event(), down(Down::Unreachable(why), 1));
    }

    #[test]
    fn a_server_that_answers_with_this_one_s_boot_is_this_computer() {
        let rig = Rig::new(1, &Servers::new(HERE));
        let mut theirs = rig.reach(VERSION, None);
        answer(&mut theirs, HERE, VERSION);
        assert_eq!(rig.event(), down(Down::ThisComputer, 1));
        assert_eq!(rig.waited(), PERSON);
        assert!(ended(&mut theirs));
    }

    #[test]
    fn a_server_another_machine_is_linked_to_is_refused_until_it_lets_go() {
        let servers = Servers::new(HERE);
        let (vps, pi) = (Rig::new(1, &servers), Rig::new(2, &servers));
        let first = vps.linked("shared-1");
        let mut second = pi.reach(VERSION, None);
        answer(&mut second, "shared-1", VERSION);
        assert_eq!(pi.event(), down(Down::SameServer(1), 1));
        assert_eq!(pi.waited(), PERSON);
        assert!(ended(&mut second));
        let Rig { link, reports, .. } = vps;
        drop(link);
        assert_eq!(reports.recv_timeout(WAIT), Err(RecvTimeoutError::Disconnected));
        drop(first);
        drop(pi.linked("shared-1"));
    }
}

mod holding {
    use super::*;

    #[rstest]
    #[case::shutdown(ServerMessage::Shutdown, STOPPED)]
    #[case::restart(ServerMessage::Restart(PathBuf::from("/usr/bin/cornercase")), RESTARTING)]
    #[case::detached(ServerMessage::Detached, LET_GO)]
    fn an_ending_ends_only_the_link_which_asks_for_a_server_again(#[case] ending: ServerMessage, #[case] why: &str) {
        let rig = Rig::new(1, &Servers::new(HERE));
        let mut theirs = rig.linked("vps-1");
        protocol::send(&mut theirs, &ending).expect("end the link");
        assert_eq!(rig.event(), down(Down::Unreachable(why.into()), 1));
        assert_eq!(rig.waited(), Duration::from_secs(1));
        let mut again = rig.reach(VERSION, None);
        assert_eq!(request(&mut again)["command"], "start-server");
    }

    #[test]
    fn ignores_everything_but_its_answer_and_the_endings() {
        let rig = Rig::new(1, &Servers::new(HERE));
        let mut theirs = rig.reach(VERSION, None);
        request(&mut theirs);
        let noise = |theirs: &mut UnixStream| {
            for message in window_messages() {
                protocol::send(theirs, &message).expect("send to the link");
            }
            protocol::send(theirs, &99u8).expect("send a message from a newer cornercase");
        };
        noise(&mut theirs);
        respond(&mut theirs, &Response::Ok(serde_json::to_value(Started::default()).expect("an answer")));
        assert!(matches!(rig.event(), Event::Linked { .. }));
        assert_eq!(request(&mut theirs)["command"], "watch");
        noise(&mut theirs);
        serve(&mut theirs, 1, shop("vps-1"));
        assert_eq!(rig.event(), Event::Seen(Box::new(shop("vps-1"))));
        protocol::send(&mut theirs, &ServerMessage::Shutdown).expect("end the link");
        assert_eq!(rig.event(), down(Down::Unreachable(STOPPED.into()), 1));
    }

    #[test]
    fn a_connection_that_ends_says_what_ssh_said() {
        let rig = Rig::new(1, &Servers::new(HERE));
        let mut theirs = rig.reach(VERSION, Some("Connection to vps closed by remote host."));
        answer(&mut theirs, "vps-1", VERSION);
        assert!(matches!(rig.event(), Event::Linked { .. }));
        drop(theirs);
        assert_eq!(rig.event(), down(Down::Unreachable("Connection to vps closed by remote host.".into()), 1));
    }
}

mod watching {
    use super::*;

    #[test]
    fn hands_on_each_snapshot_in_order_with_only_the_machine_s_own_projects() {
        let rig = Rig::new(1, &Servers::new(HERE));
        let mut theirs = rig.linked("vps-1");
        let full = control::Report {
            version: VERSION.into(),
            caller: Some(9),
            shown: Ids { project: Some(1), ..Ids::default() },
            groups: vec![GroupInfo { id: 10, name: "work".into(), ..GroupInfo::default() }],
            ..shop("vps-1")
        };
        let mut first = serde_json::to_value(Snapshot { revision: 1, report: full }).expect("a snapshot");
        first["machines"] = json!([{"name": "pi", "projects": [{"id": 7, "name": "elsewhere"}]}]);
        let mut later = shop("vps-1");
        later.projects[0].workspaces[0].tabs[0].panes[0].status = Some("waiting".into());

        respond(&mut theirs, &Response::Ok(first));
        serve(&mut theirs, 2, later.clone());

        assert_eq!(rig.event(), Event::Seen(Box::new(shop("vps-1"))));
        assert_eq!(rig.event(), Event::Seen(Box::new(later)));
    }

    #[test]
    fn cleans_and_cuts_every_text_from_the_machine_but_keeps_paths_whole() {
        let rig = Rig::new(1, &Servers::new(HERE));
        let mut theirs = rig.linked("vps-1");
        let mut report = shop("vps-1");
        report.projects[0].name = "\u{1b}]52;c;cm0gLXJmIH4K\u{7}web\nshop\u{200b}".into();
        report.projects[0].path = "/home/ana/web  shop\u{200b}\u{1b}[2J/x".repeat(20).into();
        report.projects[0].workspaces[0].path = format!("/home/ana/{}", "x".repeat(10 * 1024)).into();
        report.projects[0].workspaces[0].run_dir = Some("/home/ana/a\tb".into());
        report.home = Some(format!("/home/{}", "a".repeat(KEPT)).into());
        let pane = &mut report.projects[0].workspaces[0].tabs[0].panes[0];
        pane.model = Some("Opus\r\n5.5\u{1b}[2J".into());
        pane.accounts = [("cl\u{7}aude".to_string(), "wo\nrk".to_string())].into();

        serve(&mut theirs, 1, report);

        let Event::Seen(seen) = rig.event() else { panic!("not a snapshot") };
        let project = &seen.projects[0];
        assert_eq!(project.name, "]52;c;cm0gLXJmIH4Kweb shop");
        assert_eq!(project.path, PathBuf::from("/home/ana/web  shop[2J/x".repeat(20)));
        assert_eq!(project.workspaces[0].path.as_os_str().len(), PATH_KEPT);
        assert_eq!(project.workspaces[0].run_dir, Some(PathBuf::from("/home/ana/ab")));
        assert_eq!(seen.home.as_ref().map(|home| home.as_os_str().len()), Some(KEPT + 6));
        let pane = &project.workspaces[0].tabs[0].panes[0];
        assert_eq!(pane.model.as_deref(), Some("Opus 5.5[2J"));
        assert_eq!(pane.accounts, [("claude".to_string(), "wo rk".to_string())].into());
    }

    #[rstest]
    #[case::unreadable(Response::Ok(json!({"boot": "vps-1", "projects": "none"})), "cannot read the answer from there: ")]
    #[case::refused(Response::Error("no \u{1b}watching\nhere".into()), "no watching here")]
    fn a_snapshot_it_cannot_take_cuts_the_link_which_is_tried_again(#[case] answer: Response, #[case] why: &str) {
        let rig = Rig::new(1, &Servers::new(HERE));
        let mut theirs = rig.linked("vps-1");

        respond(&mut theirs, &answer);

        let event = rig.event();
        assert!(
            matches!(&event, Event::Down { down: Down::Unreachable(said), failed: 1 } if said.starts_with(why)),
            "{event:?}"
        );
        assert!(ended(&mut theirs));
        assert_eq!(rig.waited(), Duration::from_secs(1));
        drop(rig.linked("vps-1"));
    }

    #[test]
    fn the_end_of_the_stream_drops_the_link_which_asks_for_a_server_again() {
        let rig = Rig::new(1, &Servers::new(HERE));
        let mut theirs = rig.linked("vps-1");
        serve(&mut theirs, 1, shop("vps-1"));
        assert!(matches!(rig.event(), Event::Seen(_)));

        drop(theirs);

        assert_eq!(rig.event(), down(Down::Unreachable(LOST.into()), 1));
        assert_eq!(rig.waited(), Duration::from_secs(1));
        let mut again = rig.reach(VERSION, None);
        assert_eq!(request(&mut again)["command"], "start-server");
    }
}

mod texts {
    use super::*;

    #[test]
    fn lose_hidden_characters_and_stay_short() {
        assert_eq!(
            clean("\u{1b}[31mPermission denied\u{1b}[0m\u{200b}\n (publickey)"),
            "[31mPermission denied[0m (publickey)"
        );
        assert_eq!(clean(&"x".repeat(KEPT * 2)).chars().count(), KEPT);
    }

    #[test]
    fn from_the_machine_are_cleaned_on_their_way_here() {
        let rig = Rig::new(1, &Servers::new(HERE));
        let mut theirs = rig.reach("0.12.\u{1b}[2J225", Some("\u{1b}[31mConnection reset\u{7}"));
        answer(&mut theirs, "vps-1", "0.12.\u{7}224");
        assert_eq!(rig.event(), Event::Linked { release: "0.12.[2J225".into(), server_version: "0.12.224".into() });
        drop(theirs);
        assert_eq!(rig.event(), down(Down::Unreachable("[31mConnection reset".into()), 1));
        rig.fail(Error::Ssh { host: "vps".into(), reason: "bad\u{1b}]0;title\u{7}".into() });
        assert_eq!(rig.event(), down(Down::Unreachable("bad]0;title".into()), 2));
        let dirty = Behind { host: "vps".into(), there: "0.0.\u{1b}1".into(), update: String::new() };
        rig.fail(Error::RemoteBehind(Box::new(dirty)));
        assert_eq!(rig.event(), down(Down::Older(Some("0.0.1".into())), 3));
    }

    #[rstest]
    #[case::unreachable(Down::Unreachable("Host key verification failed.".into()), Some("Host key verification failed."))]
    #[case::missing(Down::Missing, Some("cornercase was not found on vps: install it there"))]
    #[case::older(Down::Older(Some("0.0.1".into())), None)]
    #[case::this_computer(Down::ThisComputer, Some("that is this computer"))]
    #[case::same_server(Down::SameServer(2), Some("the same server as pi"))]
    fn say_why_a_link_is_down_with_the_machines_names(#[case] down: Down, #[case] why: Option<&str>) {
        assert_eq!(down.reason("vps", Some("pi")).as_deref(), why);
    }

    #[test]
    fn name_another_machine_they_cannot_find() {
        assert_eq!(Down::SameServer(9).reason("vps", None).as_deref(), Some("the same server as another machine"));
    }
}

mod bugs {
    use super::*;

    #[test]
    fn ends_the_connection_it_held_before_its_wait() {
        let rig = Rig::held(1, &Servers::new(HERE));
        let (mut line, mut theirs) = line(VERSION, None);
        theirs.set_read_timeout(Some(WAIT)).expect("a read timeout");
        let armed = Arc::new(AtomicBool::new(false));
        line.reader = Box::new(Tripwire { inner: line.reader, armed: Arc::clone(&armed) });
        rig.attempts.send(Ok(line)).expect("the link asks for an attempt");
        answer(&mut theirs, "vps-1", VERSION);
        assert!(matches!(rig.event(), Event::Linked { .. }));
        assert_eq!(request(&mut theirs)["command"], "watch");

        armed.store(true, Ordering::SeqCst);
        protocol::send(&mut theirs, &ServerMessage::Frame(Vec::new())).expect("send to the link");

        assert_eq!(rig.event(), down(Down::Unreachable(Error::Bug.to_string()), 1));
        assert!(ended(&mut theirs));
    }

    #[test]
    fn a_link_that_hits_one_says_so_and_tries_again_a_minute_later() {
        let (tx, reports) = mpsc::channel::<Report>();
        let (sender, waits) = mpsc::channel();
        let ticks = Box::new(Ticks { waits: sender, held: false });
        let link = Link::spawn(3, Box::new(Broken(AtomicBool::new(false))), &Servers::new(HERE), &tx, ticks);
        let told = || reports.recv_timeout(WAIT).expect("a report");

        assert_eq!(told(), Report { link: 3, event: down(Down::Unreachable(Error::Bug.to_string()), 1) });
        assert_eq!(waits.recv_timeout(WAIT), Ok(PERSON));
        assert_eq!(told(), Report { link: 3, event: down(Down::Unreachable(REFUSED.into()), 1) });
        drop(link);
    }
}
