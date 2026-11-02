use super::*;
use crate::app::machines::RemoteItem;
use crate::app::machines::relay::{Asking, Then};
use crate::control::NewTab;
use crate::log::{Job, Level};
use crate::machines::send::Send;

const CLIENT: u64 = 3;
const KEY: u64 = 900;
const NEW: u64 = 8;

fn new_tab() -> Send {
    Send::NewTab(NewTab { workspace: Some(WORKSPACE), focus: true, ..NewTab::default() })
}

fn with_new(boot: &str) -> control::Report {
    let mut report = snapshot(boot);
    let web = WorkspaceInfo { id: NEW, name: "web".into(), path: PathBuf::from("/home/ana/web"), ..workspace() };
    report.projects[0].workspaces.push(web);
    report
}

fn relayed(event: &AppEvent) -> Option<std::result::Result<Value, Refusal>> {
    let AppEvent::Machine(MachineEvent::Relayed(relayed)) = event else { return None };
    Some(relayed.result().clone())
}

impl Remote {
    fn relay(&mut self, send: Send) -> std::result::Result<Value, (Code, String)> {
        self.rig.app.pend_reading(CLIENT, KEY);
        self.rig.app.send_to_machine("vps", send, Then::Client(KEY)).expect("relayed");
        let mut result = None;
        let (app, rx) = (&mut self.rig.app, &self.rig.rx);
        wait_until("the machine answers", || {
            while let Ok(event) = rx.try_recv() {
                result = result.take().or_else(|| relayed(&event));
                app.handle_event(event, AREA).expect("handle event");
            }
            result.is_some()
        });
        result.expect("a result").map_err(|refusal| (refusal.code, refusal.message))
    }

    fn answers(&mut self) -> Vec<(u64, Value)> {
        let answers = self.rig.app.take_answers().into_iter();
        answers.map(|(client, text)| (client, serde_json::from_str(&text).expect("an answer"))).collect()
    }

    fn refused(&mut self, name: &str) -> (Code, String) {
        let refusal = self.rig.app.send_to_machine(name, new_tab(), Then::Client(KEY)).expect_err("refused");
        (refusal.code, refusal.message)
    }

    fn selected(&self) -> Option<u64> {
        self.rig.app.shown_remote().map(|shown| shown.workspace)
    }

    fn awaits(&mut self, boot: &str, workspace: u64) {
        let link = self.link;
        self.rig.app.await_remote(link, boot.into(), RemoteItem::Workspace(workspace));
    }

    fn serves(&mut self, revision: u64, report: control::Report) {
        let boot = report.boot.clone();
        let count = report.projects[0].workspaces.len();
        serve(&mut self.theirs, revision, report);
        self.rig.until("the snapshot arrives", |app| {
            let seen = app.machines.held()[0].seen.as_deref();
            seen.is_some_and(|seen| seen.boot == boot && seen.projects[0].workspaces.len() == count)
        });
    }
}

mod a_relay {
    use super::*;

    #[test]
    fn answers_the_client_with_what_the_machine_said() {
        let mut remote = seeing(Reply::Agree);

        assert_eq!(remote.relay(new_tab()), Ok(json!({})));
        assert_eq!(remote.answers(), [(CLIENT, json!({ "ok": {} }))]);
    }

    #[test]
    fn tells_the_client_why_the_machine_refused() {
        let mut remote = seeing(Reply::Refuse);

        remote.relay(new_tab()).expect_err("a refusal");

        assert_eq!(remote.answers(), [(CLIENT, json!({ "error": "the worktree on vps is in use" }))]);
    }

    #[test]
    fn reaches_the_machine_with_the_boot_of_its_snapshot_and_without_the_focus() {
        let mut remote = seeing(Reply::Agree);

        remote.relay(new_tab()).expect("an answer");

        let asked = remote.asked();
        let [asked] = &asked[..] else { panic!("one question, not {asked:?}") };
        assert_eq!((asked["command"].as_str(), asked["boot"].as_str()), (Some("new-tab"), Some(BOOT)));
        assert_eq!(
            (asked["args"]["workspace"].as_u64(), asked["args"]["focus"].as_bool()),
            (Some(WORKSPACE), Some(false))
        );
    }

    #[rstest]
    #[case::refused(Reply::Refuse, Code::Failed, "the worktree on vps is in use")]
    #[case::restarted(Reply::Restarted, Code::StaleBoot, "this request is for the cornercase server started as vps-1")]
    fn keeps_the_machines_code_without_the_destination(#[case] reply: Reply, #[case] code: Code, #[case] why: &str) {
        let mut remote = seeing(reply);

        assert_eq!(remote.relay(new_tab()), Err((code, why.into())));
    }

    #[test]
    fn says_a_machine_that_cannot_be_reached_failed_without_its_destination() {
        let mut remote = seeing(Reply::Down);

        let (code, why) = remote.relay(new_tab()).expect_err("a failure");

        assert_eq!(code, Code::Failed);
        assert!(why.contains("vps") && !why.contains("vps.lan"), "{why}");
    }

    #[test]
    fn refuses_a_machine_that_is_not_listed() {
        let mut remote = seeing(Reply::Agree);

        let (code, why) = remote.refused("box");

        assert_eq!((code, why.contains("box")), (Code::NotFound, true));
        assert_eq!(remote.rig.seen("ask"), 0);
    }

    #[test]
    fn refuses_a_machine_that_is_not_linked() {
        let mut remote = seeing(Reply::Agree);
        remote.falls();

        assert_eq!(remote.refused("VPS"), (Code::Unavailable, "vps unreachable".into()));
        assert_eq!(remote.rig.seen("ask"), 0);
    }
}

mod an_answer {
    use super::*;

    #[rstest]
    #[case::late("it did not answer in time")]
    #[case::unreachable("ssh: connect to host vps.lan port 22: Connection refused")]
    fn that_never_came_fails_without_the_destination(#[case] why: &str) {
        let asking = Asking {
            machine: "vps".into(),
            destination: "ana@vps.lan".into(),
            boot: None,
            timeout: Duration::from_secs(1),
        };
        let mut job = Job::new(Level::Info, "machines", "relay");

        let heard = asking.heard(Err(why.into()), &mut job).expect_err("a failure");

        assert_eq!((heard.code, heard.message), (Code::Failed, why.replace("vps.lan", "vps")));
    }
}

mod an_awaited_selection {
    use super::*;

    #[test]
    fn is_made_when_the_next_snapshot_lists_it() {
        let mut remote = seeing(Reply::Agree);

        remote.awaits(BOOT, NEW);
        assert_eq!(remote.selected(), None);
        remote.serves(2, with_new(BOOT));

        assert_eq!(remote.selected(), Some(NEW));
    }

    #[test]
    fn is_made_at_once_when_the_snapshot_already_lists_it() {
        let mut remote = seeing(Reply::Agree);

        remote.awaits(BOOT, WORKSPACE);

        assert_eq!(remote.selected(), Some(WORKSPACE));
    }

    #[test]
    fn is_dropped_when_something_else_is_selected() {
        let mut remote = seeing(Reply::Agree);

        remote.awaits(BOOT, NEW);
        assert!(remote.rig.app.select_remote("vps", RemoteItem::Workspace(WORKSPACE)));
        remote.serves(2, with_new(BOOT));

        assert_eq!(remote.selected(), Some(WORKSPACE));
    }

    #[test]
    fn is_dropped_when_the_machine_restarts() {
        let mut remote = seeing(Reply::Agree);

        remote.awaits(BOOT, NEW);
        remote.serves(2, with_new(RESTARTED));

        assert_eq!(remote.selected(), None);
    }

    #[test]
    fn is_dropped_once_it_waited_too_long() {
        let mut remote = seeing(Reply::Agree);

        remote.awaits(BOOT, NEW);
        remote.rig.refresh(Duration::from_secs(300));
        remote.serves(2, with_new(BOOT));

        assert_eq!(remote.selected(), None);
    }

    #[test]
    fn waits_past_a_refresh_before_its_time() {
        let mut remote = seeing(Reply::Agree);

        remote.awaits(BOOT, NEW);
        remote.rig.refresh(Duration::ZERO);
        remote.serves(2, with_new(BOOT));

        assert_eq!(remote.selected(), Some(NEW));
    }
}

mod requests;
