use std::net::Shutdown;

use rstest::rstest;
use serde_json::{Value, json};

use super::*;
use crate::app::machines::acting::Aim;
use crate::control::contract::{Code, Refusal, Structured};
use crate::control::{ProjectInfo, WorkspaceInfo};
use crate::machines::act::Act;
use crate::machines::link::fake::{request, serve};
use crate::protocol::ServerMessage;

const BOOT: &str = "vps-1";
const RESTARTED: &str = "vps-2";
const WORKSPACE: u64 = 7;
const PATH: &str = "/home/ana/api";
const REMOVE: Act = Act::Remove { force: false, unlock: false };

#[derive(Clone, Copy)]
enum Reply {
    Agree,
    Warn,
    Refuse,
    Restarted,
    Down,
    Found(fn() -> Value),
}

impl Reply {
    fn to(self, asked: &Value) -> Structured {
        let refuse = |code, message: String| Structured::Error(Refusal::new(code, message));
        match (self, asked["boot"].as_str()) {
            (Self::Refuse, _) => refuse(Code::Failed, "the worktree on vps.lan is in use".into()),
            (Self::Warn, _) => Structured::Ok(json!({ "warning": "exit status 1: no route to vps.lan" })),
            (Self::Found(removal), _) if asked["command"] == "removal-check" => Structured::Ok(removal()),
            (Self::Restarted, Some(boot)) if boot != RESTARTED => {
                refuse(Code::StaleBoot, format!("this request is for the cornercase server started as {boot}"))
            }
            _ => Structured::Ok(json!({})),
        }
    }
}

type Asked = Arc<Mutex<Vec<Value>>>;

struct Answering {
    reply: Reply,
    asked: Asked,
    seen: Seen,
}

impl Connect for Answering {
    fn connect(&self, _: &Killer) -> Result<Line> {
        self.seen.note("ask".into());
        if matches!(self.reply, Reply::Down) {
            let reason = "ssh: connect to host vps.lan port 22: Connection refused".into();
            return Err(Error::Ssh { host: "vps.lan".into(), reason });
        }
        let (line, mut theirs) = line(version::CURRENT, None);
        let (reply, asked) = (self.reply, Arc::clone(&self.asked));
        std::thread::spawn(move || {
            let question = request(&mut theirs);
            let text = serde_json::to_string(&reply.to(&question)).expect("an answer");
            asked.lock().push(question);
            protocol::send(&mut theirs, &ServerMessage::Response(text)).expect("answer the question");
        });
        Ok(line)
    }
}

struct Remote {
    rig: Rig,
    theirs: UnixStream,
    asked: Asked,
    link: u64,
    ends: Ends,
}

fn workspace() -> WorkspaceInfo {
    WorkspaceInfo { id: WORKSPACE, name: "api".into(), path: PathBuf::from(PATH), ..WorkspaceInfo::default() }
}

fn holding(boot: &str, workspace: WorkspaceInfo) -> control::Report {
    let project = ProjectInfo {
        id: 1,
        name: "api".into(),
        path: PathBuf::from(PATH),
        workspaces: vec![workspace],
        ..Default::default()
    };
    control::Report { boot: boot.into(), projects: vec![project], ..control::Report::default() }
}

fn snapshot(boot: &str) -> control::Report {
    holding(boot, workspace())
}

fn seeing(reply: Reply) -> Remote {
    seeing_in(reply, snapshot(BOOT))
}

fn seeing_in(reply: Reply, report: control::Report) -> Remote {
    let mut rig = Rig::new(Vec::new(), There::Missing);
    let (ends, asked) = (Ends::default(), Asked::default());
    let (watched, given, seen) = (Arc::clone(&ends), Arc::clone(&asked), rig.seen.clone());
    rig.app.machines.set_connector(Box::new(move |destination| -> Box<dyn Connect> {
        if seen.count("link") > 0 {
            return Box::new(Answering { reply, asked: Arc::clone(&given), seen: seen.clone() });
        }
        seen.note("link".into());
        let there = There::Watched(Arc::clone(&watched));
        Box::new(Fake { destination: destination.into(), there, seen: seen.clone() })
    }));
    rig.edit(vec![machine("vps", "vps.lan")], Duration::from_secs(2));
    wait_until("the link connects", || !ends.lock().is_empty());
    let mut theirs = ends.lock().remove(0);
    answer(&mut theirs, BOOT, version::CURRENT);
    request(&mut theirs);
    serve(&mut theirs, 1, report);
    rig.until("the snapshot arrives", |app| app.machines.held()[0].seen.is_some());
    let link = rig.links()[0];
    Remote { rig, theirs, asked, link, ends }
}

impl Remote {
    fn aim(&self) -> Aim {
        Aim { link: self.link, era: self.rig.app.machines.held()[0].era, workspace: WORKSPACE }
    }

    fn ask_close(&mut self) {
        let aim = self.aim();
        self.rig.app.remote_close(aim);
    }

    fn falls(&mut self) {
        self.theirs.shutdown(Shutdown::Both).expect("cut the link");
        self.rig.until("the machine falls", |app| app.machines.held()[0].status.state != State::Linked);
    }

    fn send(&mut self, act: Act) {
        let aim = self.aim();
        self.rig.app.send_act(aim, act);
    }

    fn heard(&mut self) {
        let (app, rx) = (&mut self.rig.app, &self.rig.rx);
        wait_until("the machine answers", || {
            let mut acted = false;
            while let Ok(event) = rx.try_recv() {
                acted |= matches!(event, AppEvent::Machine(MachineEvent::Acted(_)));
                app.handle_event(event, AREA).expect("handle event");
            }
            acted
        });
    }

    fn asked(&self) -> Vec<Value> {
        self.asked.lock().clone()
    }

    fn sent_nothing_while_down(&self) {
        assert!(self.rig.app.overlay.is_none());
        assert_eq!(toast(&self.rig.app), Some("vps unreachable; nothing was sent"));
        assert_eq!(self.rig.seen("ask"), 0);
    }

    fn removing(&self) -> bool {
        let app = &self.rig.app;
        let remote = app.remote_projects();
        app.alone_entries(&remote).flatten().any(|entry| entry.removing)
    }

    fn restarted(&mut self) {
        serve(&mut self.theirs, 2, snapshot(RESTARTED));
        self.rig.until("the new boot arrives", |app| {
            app.machines.held()[0].seen.as_ref().is_some_and(|seen| seen.boot == RESTARTED)
        });
    }
}

mod an_act {
    use super::*;

    #[rstest]
    #[case::rename(Act::Rename("web".into()), "rename")]
    #[case::check(Act::Check, "removal-check")]
    #[case::close(Act::Close, "close")]
    #[case::remove(Act::Remove { force: true, unlock: true }, "close")]
    fn reaches_the_machine_with_the_boot_of_its_snapshot(#[case] act: Act, #[case] command: &str) {
        let mut remote = seeing(Reply::Agree);
        let args = serde_json::to_value(act.command(WORKSPACE)).expect("a command")["args"].clone();

        remote.send(act);
        remote.heard();

        let asked = remote.asked();
        let [asked] = &asked[..] else { panic!("one question, not {asked:?}") };
        assert_eq!((asked["command"].as_str(), asked["boot"].as_str()), (Some(command), Some(BOOT)));
        assert_eq!(asked["args"], args);
    }

    #[test]
    fn waits_for_the_one_in_flight_on_its_workspace() {
        let mut remote = seeing(Reply::Agree);

        remote.send(Act::Rename("web".into()));
        remote.send(Act::Close);
        assert_eq!(toast(&remote.rig.app), Some("api on vps is still waiting for its last action; nothing was sent"));
        remote.heard();

        assert_eq!(remote.rig.seen("ask"), 1);
        assert_eq!(remote.asked()[0]["command"], "rename");
    }

    #[test]
    fn is_not_sent_to_a_machine_that_is_not_linked() {
        let mut remote = seeing(Reply::Agree);
        let aim = remote.aim();
        remote.falls();

        remote.rig.app.send_act(aim, Act::Close);

        assert_eq!(toast(&remote.rig.app), Some("vps unreachable; nothing was sent"));
        assert_eq!(remote.rig.seen("ask"), 0);
        assert!(!remote.removing());
    }

    #[test]
    fn is_not_sent_once_the_machine_restarted() {
        let mut remote = seeing(Reply::Agree);
        let aim = remote.aim();

        remote.restarted();
        remote.rig.app.send_act(aim, Act::Rename("web".into()));

        assert_eq!(toast(&remote.rig.app), Some("vps restarted; nothing was sent"));
        assert_eq!(remote.rig.seen("ask"), 0);
        assert_eq!(remote.aim().era, aim.era + 1);
    }

    #[test]
    fn keeps_its_aim_while_the_machine_keeps_its_boot() {
        let mut remote = seeing(Reply::Agree);
        let aim = remote.aim();

        serve(&mut remote.theirs, 2, snapshot(BOOT));
        remote.rig.until("the next snapshot arrives", |app| app.machines.held()[0].seen.is_some());
        remote.rig.app.send_act(aim, Act::Close);
        remote.heard();

        assert_eq!(remote.aim(), aim);
        assert_eq!(remote.rig.seen("ask"), 1);
    }

    #[test]
    fn is_not_sent_for_a_workspace_the_machine_no_longer_has() {
        let mut remote = seeing(Reply::Agree);
        let aim = Aim { workspace: WORKSPACE + 1, ..remote.aim() };

        remote.rig.app.send_act(aim, Act::Close);

        assert_eq!(toast(&remote.rig.app), Some("that workspace is no longer on vps; nothing was sent"));
        assert_eq!(remote.rig.seen("ask"), 0);
    }
}

mod an_answer {
    use super::*;

    #[rstest]
    #[case::renamed(Act::Rename("web".into()), Reply::Agree, None)]
    #[case::closed(Act::Close, Reply::Agree, None)]
    #[case::checked(Act::Check, Reply::Agree, None)]
    #[case::refused(REMOVE, Reply::Refuse, Some("could not remove api on vps: the worktree on vps is in use"))]
    #[case::unreachable(
        Act::Rename("web".into()),
        Reply::Down,
        Some("could not rename api on vps: ssh: connect to host vps port 22: Connection refused"),
    )]
    fn is_told_without_the_destination(#[case] act: Act, #[case] reply: Reply, #[case] told: Option<&str>) {
        let mut remote = seeing(reply);

        remote.send(act);
        remote.heard();

        assert_eq!(toast(&remote.rig.app), told);
    }

    #[rstest]
    #[case::removed(Reply::Agree, "removed api on vps", ui::ToastIcon::Check)]
    #[case::warned(Reply::Warn, "removed api on vps, cleanup failed in part, see server.log", ui::ToastIcon::Bug)]
    fn to_a_removal_is_told_as_a_removal_here(#[case] reply: Reply, #[case] told: &str, #[case] icon: ui::ToastIcon) {
        let mut remote = seeing(reply);

        remote.send(REMOVE);
        remote.heard();

        let toast = remote.rig.app.messages.toast.as_ref().expect("a toast");
        assert_eq!((toast.message.as_str(), toast.icon), (told, icon));
    }

    #[test]
    fn from_a_machine_that_restarted_says_nothing_was_changed() {
        let mut remote = seeing(Reply::Restarted);

        remote.send(REMOVE);
        remote.heard();

        assert_eq!(toast(&remote.rig.app), Some("vps restarted; nothing was changed there"));
        assert!(!remote.removing());
    }
}

mod the_row {
    use super::*;

    #[rstest]
    #[case::closing(Act::Close, true)]
    #[case::removing(REMOVE, true)]
    #[case::renaming(Act::Rename("web".into()), false)]
    #[case::checking(Act::Check, false)]
    fn is_removing_while_the_machine_closes_it(#[case] act: Act, #[case] removing: bool) {
        let mut remote = seeing(Reply::Agree);
        assert!(!remote.removing());

        remote.send(act);

        assert_eq!(remote.removing(), removing);
    }

    #[test]
    fn stays_removing_after_a_removal_until_the_machine_drops_it() {
        let mut remote = seeing(Reply::Agree);

        remote.send(REMOVE);
        remote.heard();

        assert!(remote.removing());
    }

    #[test]
    fn comes_back_when_the_machine_refuses() {
        let mut remote = seeing(Reply::Refuse);

        remote.send(REMOVE);
        remote.heard();
        assert!(!remote.removing());
        remote.send(Act::Close);
        remote.heard();

        assert_eq!(remote.rig.seen("ask"), 2);
    }
}

mod dialogs;
mod following;
mod menus;
