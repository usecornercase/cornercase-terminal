use super::*;

impl Remote {
    fn forgets(&mut self) {
        serve(&mut self.theirs, 2, control::Report { boot: BOOT.into(), ..control::Report::default() });
        self.rig.until("the workspace is gone", |app| {
            app.machines.held()[0].seen.as_ref().is_some_and(|seen| seen.projects.is_empty())
        });
    }

    fn back(&mut self) {
        wait_until("the link connects again", || !self.ends.lock().is_empty());
        self.theirs = self.ends.lock().remove(0);
        answer(&mut self.theirs, BOOT, version::CURRENT);
        request(&mut self.theirs);
        self.rig.until("the machine is back", |app| app.machines.held()[0].status.state == State::Linked);
    }

    fn busy(&self) -> bool {
        self.rig.app.machines.acting.busy(self.link, WORKSPACE)
    }
}

mod a_dialog {
    use super::*;

    #[test]
    fn closes_once_the_machine_no_longer_lists_its_workspace() {
        let mut remote = seeing(Reply::Agree);
        remote.ask_close();

        remote.forgets();

        assert!(remote.rig.app.overlay.is_none());
        assert_eq!(remote.rig.seen("ask"), 0);
    }

    #[test]
    fn closes_once_the_machine_restarts_and_sends_nothing() {
        let mut remote = seeing(Reply::Agree);
        remote.ask_close();

        remote.restarted();

        assert!(remote.rig.app.overlay.is_none());
        assert_eq!(toast(&remote.rig.app), None);
        assert_eq!(remote.rig.seen("ask"), 0);
    }

    #[test]
    fn closes_once_the_machine_is_taken_out() {
        let mut remote = seeing(Reply::Agree);
        remote.ask_close();

        remote.rig.edit(Vec::new(), Duration::from_secs(4));

        assert!(remote.rig.app.overlay.is_none());
    }

    #[test]
    fn stays_open_on_a_machine_that_falls_and_sends_nothing_once_it_is_back() {
        let mut remote = seeing(Reply::Agree);
        remote.ask_close();

        remote.falls();
        assert!(remote.rig.app.overlay.is_some());
        send_key(&mut remote.rig.app, KeyCode::Enter, KeyModifiers::NONE);
        remote.back();

        remote.sent_nothing_while_down();
    }
}

mod a_removal {
    use super::*;

    #[test]
    fn stops_showing_once_the_machine_no_longer_lists_its_workspace() {
        let mut remote = seeing(Reply::Agree);
        remote.send(REMOVE);
        remote.heard();
        assert!(remote.removing());

        remote.forgets();

        assert!(!remote.busy());
    }

    #[rstest]
    #[case::answered(true)]
    #[case::in_flight(false)]
    fn is_forgotten_once_the_machine_restarts(#[case] answered: bool) {
        let mut remote = seeing(Reply::Agree);
        remote.send(REMOVE);
        if answered {
            remote.heard();
        }

        remote.restarted();

        assert!(!remote.removing());
        assert!(!remote.busy());
    }
}

mod a_tab_s_close {
    use super::*;

    fn without_the_shell() -> control::Report {
        let mut workspace = tabbed();
        workspace.tabs.remove(0);
        holding(BOOT, workspace)
    }

    #[test]
    fn reads_closing_until_the_machine_no_longer_lists_the_tab() {
        let mut remote = with_tabs(Reply::Agree);
        remote.send(Act::CloseTab(SHELL));
        remote.heard();
        assert_eq!(remote.closing(), [true, false]);

        serve(&mut remote.theirs, 2, without_the_shell());
        remote.rig.until("the tab is gone", |app| {
            app.machines.held()[0].seen.as_ref().is_some_and(|seen| seen.projects[0].workspaces[0].tabs.len() == 1)
        });

        assert_eq!(remote.closing(), [false]);
        assert!(!remote.rig.app.machines.acting.busy_tab(remote.link, SHELL));
    }

    #[test]
    fn of_two_tabs_drops_only_the_mark_of_the_one_answered() {
        let mut remote = with_tabs(Reply::Refuse);
        remote.send(Act::CloseTab(SHELL));
        remote.send(Act::RenameTab { tab: AGENT, name: "logs".into() });

        wait_until("both are asked", || remote.asked().len() == 2);
        let (app, rx) = (&mut remote.rig.app, &remote.rig.rx);
        let event = rx.recv_timeout(Duration::from_secs(10)).expect("an answer");
        let AppEvent::Machine(MachineEvent::Acted(acted)) = &event else { panic!("not an answer: {event:?}") };
        let first = format!("{acted:?}").contains("CloseTab");
        app.handle_event(event, AREA).expect("handle event");

        let busy = |tab| app.machines.acting.busy_tab(remote.link, tab);
        assert_eq!((busy(SHELL), busy(AGENT)), (!first, first));
    }

    #[test]
    fn does_not_keep_its_workspace_busy() {
        let mut remote = with_tabs(Reply::Agree);

        remote.send(Act::CloseTab(SHELL));

        assert!(!remote.busy());
        assert!(remote.rig.app.machines.acting.busy_tab(remote.link, SHELL));
        remote.heard();
    }
}

mod a_tab_s_dialog {
    use super::*;

    #[test]
    fn closes_once_the_machine_no_longer_lists_its_tab() {
        let mut remote = with_tabs(Reply::Agree);
        let at = remote.tab_aim(SHELL);
        remote.rig.app.remote_rename_tab(at);

        let mut workspace = tabbed();
        workspace.tabs.remove(0);
        serve(&mut remote.theirs, 2, holding(BOOT, workspace));
        remote.rig.until("the tab is gone", |app| app.overlay.is_none());

        assert_eq!(remote.rig.seen("ask"), 0);
    }

    #[test]
    fn closes_once_the_machine_restarts() {
        let mut remote = with_tabs(Reply::Agree);
        let at = remote.tab_aim(AGENT);
        remote.rig.app.remote_close_tab(at, false);

        remote.restarted();

        assert!(remote.rig.app.overlay.is_none());
        assert_eq!(remote.rig.seen("ask"), 0);
    }
}
