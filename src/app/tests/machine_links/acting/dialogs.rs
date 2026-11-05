use super::*;
use crate::app::overlay::{CLOSE_SUBMIT, FORCE_REMOVE_SUBMIT, REMOVE_SUBMIT, UNLOCK_SUBMIT};
use crate::app::worktrees::UNCOMMITTED;
use crate::control::TabInfo;

const ASKED: &str = "Remove the workspace api on vps and delete its worktree folder ~/api? The branch is kept. \
                     Its 2 closed agents are forgotten.";

fn clean() -> Value {
    json!({ "workspace": WORKSPACE, "path": PATH })
}

fn changed() -> Value {
    json!({ "workspace": WORKSPACE, "path": PATH, "changed": true })
}

fn locked() -> Value {
    json!({ "workspace": WORKSPACE, "lock": { "reason": "agent at work", "path": PATH, "pid": 42, "running": true } })
}

fn left() -> Value {
    json!({ "workspace": WORKSPACE, "lock": { "reason": "", "path": PATH, "pid": 42, "running": false } })
}

fn with_roles() -> Value {
    json!({ "workspace": WORKSPACE, "roles": [{ "role": "tester" }, { "role": "reviewer" }], "teardown": true })
}

fn on_the_machine(reply: Reply, worktree: bool) -> Remote {
    let tabs = vec![TabInfo::default()];
    let workspace = WorkspaceInfo { worktree, closed_agents: Some(2), tabs, ..workspace() };
    seeing_in(reply, control::Report { home: Some(PathBuf::from("/home/ana")), ..holding(BOOT, workspace) })
}

fn sent(act: &Act) -> Value {
    serde_json::to_value(act.command(WORKSPACE)).expect("a command")["args"].clone()
}

impl Remote {
    fn ask_rename(&mut self) {
        let aim = self.aim();
        self.rig.app.remote_rename(aim);
    }

    fn press(&mut self, code: KeyCode) {
        send_key(&mut self.rig.app, code, KeyModifiers::NONE);
    }

    fn commands(&self) -> Vec<String> {
        self.asked().iter().map(|asked| asked["command"].as_str().unwrap_or_default().to_string()).collect()
    }

    fn shown(&self) -> ui::Confirm {
        match overlay_shown(&self.rig.app, AREA) {
            ui::Overlay::Confirm(confirm) => confirm,
            _ => panic!("not a confirmation"),
        }
    }

    fn submit(&self) -> Option<&'static str> {
        self.rig.app.overlay.as_ref().map(Overlay::submit_label)
    }
}

mod a_close {
    use super::*;

    #[test]
    fn asks_first_naming_the_machine() {
        let mut remote = on_the_machine(Reply::Agree, false);

        remote.ask_close();

        let asked = "Close the workspace api on vps? Its tab and the programs running in it are stopped. \
                     Its 2 closed agents are forgotten.";
        assert_eq!(confirmation(&remote.rig.app).as_deref(), Some(asked));
        assert_eq!(remote.submit(), Some(CLOSE_SUBMIT));
        assert_eq!(remote.rig.seen("ask"), 0);
    }

    #[test]
    fn closes_it_there_once_confirmed() {
        let mut remote = on_the_machine(Reply::Agree, false);

        remote.ask_close();
        remote.press(KeyCode::Enter);
        remote.heard();

        assert!(remote.rig.app.overlay.is_none());
        assert_eq!(remote.commands(), ["close"]);
        assert_eq!(remote.asked()[0]["args"], sent(&Act::Close));
    }

    #[test]
    fn sends_nothing_when_cancelled() {
        let mut remote = on_the_machine(Reply::Agree, false);

        remote.ask_close();
        remote.press(KeyCode::Esc);

        assert!(remote.rig.app.overlay.is_none());
        assert_eq!(remote.rig.seen("ask"), 0);
    }

    #[test]
    fn opens_nothing_on_a_machine_that_is_not_linked() {
        let mut remote = on_the_machine(Reply::Agree, true);
        remote.falls();

        remote.ask_close();

        remote.sent_nothing_while_down();
    }
}

mod a_rename {
    use super::*;

    #[test]
    fn starts_from_the_name_on_the_machine() {
        let mut remote = on_the_machine(Reply::Agree, false);

        remote.ask_rename();

        let ui::Overlay::Form(form) = overlay_shown(&remote.rig.app, AREA) else { panic!("not a form") };
        assert_eq!((form.title, form.value.text()), ("Rename workspace", "api".to_string()));
        assert_eq!(form.hint, "leave it empty to use the branch name");
    }

    #[rstest]
    #[case::named("web", "web")]
    #[case::trimmed(" web ", "web")]
    #[case::emptied("", "")]
    fn sends_the_name_typed(#[case] typed: &str, #[case] name: &str) {
        let mut remote = on_the_machine(Reply::Agree, false);

        remote.ask_rename();
        for _ in 0.."api".len() {
            remote.press(KeyCode::Backspace);
        }
        type_text(&mut remote.rig.app, typed);
        remote.press(KeyCode::Enter);
        remote.heard();

        assert_eq!(remote.commands(), ["rename"]);
        assert_eq!(remote.asked()[0]["args"], sent(&Act::Rename(name.into())));
    }

    #[test]
    fn sends_nothing_when_cancelled() {
        let mut remote = on_the_machine(Reply::Agree, false);

        remote.ask_rename();
        remote.press(KeyCode::Esc);

        assert!(remote.rig.app.overlay.is_none());
        assert_eq!(remote.rig.seen("ask"), 0);
    }
}

mod a_removal {
    use super::*;

    #[test]
    fn checks_on_the_machine_before_anything_is_removed() {
        let mut remote = on_the_machine(Reply::Found(clean), true);

        remote.ask_close();
        remote.heard();

        assert_eq!(remote.commands(), ["removal-check"]);
        assert_eq!(confirmation(&remote.rig.app).as_deref(), Some(ASKED));
        assert_eq!(remote.submit(), Some(REMOVE_SUBMIT));
    }

    #[rstest]
    #[case::clean(clean, ASKED, None, REMOVE_SUBMIT)]
    #[case::changed(changed, ASKED, Some(UNCOMMITTED), FORCE_REMOVE_SUBMIT)]
    #[case::locked(
        locked,
        "The worktree of api on vps is locked: agent at work. Unlock it and delete its folder? The branch is kept.",
        Some("Process 42 still runs and may be using it."),
        UNLOCK_SUBMIT
    )]
    #[case::left_locked(
        left,
        "The worktree of api on vps is locked. Unlock it and delete its folder? The branch is kept.",
        Some("Process 42 is gone; the lock was left behind."),
        UNLOCK_SUBMIT
    )]
    #[case::with_roles(
        with_roles,
        "Remove the workspace api on vps and delete its worktree folder ~/api, with the worktrees of its 2 roles? \
         Its teardown runs first. The branches are kept. Its 2 closed agents are forgotten.",
        None,
        REMOVE_SUBMIT
    )]
    fn says_what_the_machine_found(
        #[case] found: fn() -> Value,
        #[case] message: &str,
        #[case] note: Option<&str>,
        #[case] submit: &str,
    ) {
        let mut remote = on_the_machine(Reply::Found(found), true);

        remote.ask_close();
        remote.heard();

        let shown = remote.shown();
        assert_eq!((shown.title, shown.message.as_str(), shown.submit), ("Remove workspace", message, submit));
        assert_eq!(shown.note, note.map(|note| ui::Note::Error(note.into())));
    }

    #[rstest]
    #[case::clean(clean, false, false)]
    #[case::changed(changed, true, false)]
    #[case::locked(locked, false, true)]
    fn removes_it_there_as_accepted(#[case] found: fn() -> Value, #[case] force: bool, #[case] unlock: bool) {
        let mut remote = on_the_machine(Reply::Found(found), true);

        remote.ask_close();
        remote.heard();
        remote.press(KeyCode::Enter);
        remote.heard();

        assert_eq!(remote.commands(), ["removal-check", "close"]);
        assert_eq!(remote.asked()[1]["args"], sent(&Act::Remove { force, unlock }));
        assert_eq!(toast(&remote.rig.app), Some("removed api on vps"));
    }

    #[test]
    fn confirmed_during_the_check_goes_on_once_it_is_clean() {
        let mut remote = on_the_machine(Reply::Found(clean), true);

        remote.ask_close();
        remote.press(KeyCode::Enter);
        assert_eq!(remote.shown().note, Some(ui::Note::Busy("checking…")));
        remote.heard();
        remote.heard();

        assert!(remote.rig.app.overlay.is_none());
        assert_eq!(remote.commands(), ["removal-check", "close"]);
        assert_eq!(remote.asked()[1]["args"], sent(&REMOVE));
    }

    #[test]
    fn confirmed_during_the_check_waits_when_it_has_changes() {
        let mut remote = on_the_machine(Reply::Found(changed), true);

        remote.ask_close();
        remote.press(KeyCode::Enter);
        remote.heard();

        assert_eq!(remote.submit(), Some(FORCE_REMOVE_SUBMIT));
        assert_eq!(remote.rig.seen("ask"), 1);
    }

    #[test]
    fn closes_with_a_toast_when_the_check_fails() {
        let mut remote = on_the_machine(Reply::Refuse, true);

        remote.ask_close();
        remote.heard();

        assert!(remote.rig.app.overlay.is_none());
        let told = "could not check api on vps: the worktree on vps is in use; nothing was removed";
        assert_eq!(toast(&remote.rig.app), Some(told));
        assert_eq!(remote.commands(), ["removal-check"]);
    }

    #[test]
    fn sends_nothing_more_when_cancelled_during_the_check() {
        let mut remote = on_the_machine(Reply::Found(clean), true);

        remote.ask_close();
        remote.press(KeyCode::Esc);
        remote.heard();

        assert!(remote.rig.app.overlay.is_none());
        assert_eq!(toast(&remote.rig.app), None);
        assert_eq!(remote.commands(), ["removal-check"]);
    }
}
