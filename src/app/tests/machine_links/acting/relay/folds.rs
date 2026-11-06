use super::*;
use crate::app::tests::machine_links::seen::agents::{TALL, row_at, section};

const PARENT: u64 = 12;

fn agent(id: u64, parent: Option<u64>, collapsed: bool) -> PaneInfo {
    let (agent, status) = (Some("claude".into()), Some("waiting".into()));
    PaneInfo { id, agent, status, parent, collapsed, ..PaneInfo::default() }
}

fn family(boot: &str, collapsed: bool) -> control::Report {
    let panes = vec![agent(PARENT, None, collapsed), agent(PARENT + 1, Some(PARENT), false)];
    let tabs = vec![TabInfo { id: 2, panes, ..TabInfo::default() }];
    holding(boot, WorkspaceInfo { tabs, ..workspace() })
}

fn folding(reply: Reply, collapsed: bool) -> Remote {
    seeing_in(reply, family(BOOT, collapsed))
}

fn not_found(_: &str) -> Structured {
    Structured::Error(Refusal::new(Code::NotFound, "no pane 12 on vps.lan"))
}

impl Remote {
    fn arrow(&mut self) -> Position {
        section(&mut self.rig.app);
        let row = row_at(&self.rig.app, TALL, 0);
        Position::new(row.x, row.y)
    }

    fn press_arrow(&mut self) -> Position {
        let at = self.arrow();
        mouse_in(&mut self.rig.app, MouseEventKind::Down(MouseButton::Left), at, TALL);
        at
    }

    fn release(&mut self, at: Position) {
        mouse_in(&mut self.rig.app, MouseEventKind::Up(MouseButton::Left), at, TALL);
    }

    fn fold(&mut self) {
        let at = self.press_arrow();
        self.release(at);
    }

    fn told(&mut self) {
        let event = self.held_relay();
        self.rig.app.handle_event(event, AREA).expect("handle event");
    }

    fn folds(&self) -> Vec<Option<bool>> {
        self.rig.app.agents_view().entries.into_iter().map(|entry| entry.fold).collect()
    }

    fn menu_on(&mut self, row: usize) {
        section(&mut self.rig.app);
        let row = row_at(&self.rig.app, TALL, row);
        mouse_in(&mut self.rig.app, MouseEventKind::Down(MouseButton::Right), Position::new(row.x + 6, row.y), TALL);
    }

    fn choose(&mut self, item: usize) {
        let at = menu_item_in(&self.rig.app, item, TALL);
        click_in(&mut self.rig.app, at, TALL);
    }

    fn sends(&mut self, report: control::Report) {
        let wanted = (report.boot.clone(), report.projects.clone());
        serve(&mut self.theirs, 2, report);
        self.rig.until("the snapshot arrives", |app| {
            app.machines.held()[0]
                .seen
                .as_ref()
                .is_some_and(|seen| (&seen.boot, &seen.projects) == (&wanted.0, &wanted.1))
        });
    }
}

fn commands(remote: &Remote) -> Vec<(Value, Value, Value)> {
    let asked = remote.asked().into_iter();
    asked.map(|asked| (asked["command"].clone(), asked["args"].clone(), asked["boot"].clone())).collect()
}

mod the_arrow {
    use super::*;

    #[rstest]
    #[case::open(false, "collapse")]
    #[case::folded(true, "expand")]
    fn asks_the_machine_to_fold_its_pane_there_with_the_boot_of_its_snapshot(
        #[case] collapsed: bool,
        #[case] command: &str,
    ) {
        let mut remote = folding(Reply::Agree, collapsed);

        remote.fold();
        remote.told();

        let item = json!({ "item": { "pane": PARENT } });
        assert_eq!(commands(&remote), [(json!(command), item, json!(BOOT))]);
        assert_eq!((remote.rig.app.shown_remote().is_none(), toast(&remote.rig.app)), (true, None));
    }

    #[test]
    fn leaves_the_row_as_it_was_until_the_next_snapshot() {
        let mut remote = folding(Reply::Agree, false);

        remote.fold();
        remote.told();
        assert_eq!(remote.folds(), [Some(false), None]);
        remote.sends(family(BOOT, true));

        assert_eq!(remote.folds(), [Some(true)]);
    }

    #[test]
    fn pressed_twice_before_the_snapshot_asks_twice() {
        let mut remote = folding(Reply::Agree, false);

        remote.fold();
        remote.fold();
        remote.told();
        remote.told();

        let collapse = (json!("collapse"), json!({ "item": { "pane": PARENT } }), json!(BOOT));
        assert_eq!(commands(&remote), [collapse.clone(), collapse]);
    }

    #[test]
    fn sends_nothing_to_a_machine_that_is_not_linked() {
        let mut remote = folding(Reply::Agree, false);
        remote.falls();

        remote.fold();

        remote.sent_nothing_while_down();
        assert_eq!((remote.rig.app.shown_remote().is_none(), remote.folds()), (true, vec![Some(false), None]));
    }

    #[test]
    fn sends_nothing_once_the_machine_restarted_between_the_press_and_the_release() {
        let mut remote = folding(Reply::Agree, false);
        let at = remote.press_arrow();

        remote.sends(family(RESTARTED, false));
        remote.release(at);

        assert_eq!(toast(&remote.rig.app), Some("vps restarted; nothing was sent"));
        assert_eq!(remote.rig.seen("ask"), 0);
    }

    #[test]
    fn sends_nothing_for_an_agent_the_machine_no_longer_has() {
        let mut remote = folding(Reply::Agree, false);
        let at = remote.press_arrow();

        remote.sends(holding(BOOT, workspace()));
        remote.release(at);

        assert_eq!(toast(&remote.rig.app), Some("that agent is no longer on vps; nothing was sent"));
        assert_eq!(remote.rig.seen("ask"), 0);
    }
}

mod a_refusal {
    use super::*;

    #[rstest]
    #[case::restarted(Reply::Restarted, false, "vps restarted; nothing was changed there")]
    #[case::not_found(Reply::Told(not_found), false, "could not collapse the agent on vps: no pane 12 on vps")]
    #[case::unreachable(
        Reply::Down,
        true,
        "could not expand the agent on vps: ssh: connect to host vps port 22: Connection refused"
    )]
    fn is_told_without_the_destination(#[case] reply: Reply, #[case] collapsed: bool, #[case] told: &str) {
        let mut remote = folding(reply, collapsed);

        remote.fold();
        remote.told();

        assert_eq!(toast(&remote.rig.app), Some(told));
        assert!(remote.rig.app.shown_remote().is_none());
    }
}

mod its_menu {
    use super::*;

    #[rstest]
    #[case::open(false, "collapse")]
    #[case::folded(true, "expand")]
    fn offers_only_to_fold_it_there(#[case] collapsed: bool, #[case] command: &str) {
        let mut remote = folding(Reply::Agree, collapsed);

        remote.menu_on(0);
        assert_eq!((menu_labels(&remote.rig.app), menu_disabled(&remote.rig.app)), (vec![command.into()], vec![false]));
        remote.choose(0);
        remote.told();

        assert_eq!(commands(&remote), [(json!(command), json!({ "item": { "pane": PARENT } }), json!(BOOT))]);
        assert_eq!((remote.rig.app.shown_remote().is_none(), toast(&remote.rig.app)), (true, None));
    }

    #[test]
    fn does_not_open_on_a_row_without_children() {
        let mut remote = folding(Reply::Agree, false);

        remote.menu_on(1);

        assert!(remote.rig.app.overlay.is_none());
    }

    #[test]
    fn of_a_fallen_machine_says_why_and_greys_the_fold() {
        let mut remote = folding(Reply::Agree, false);
        remote.falls();

        remote.menu_on(0);
        let labels: Vec<String> = vec!["vps unreachable".into(), "collapse".into()];
        assert_eq!((menu_labels(&remote.rig.app), menu_disabled(&remote.rig.app)), (labels, vec![true, true]));
        remote.choose(1);

        assert_eq!((remote.rig.app.overlay.is_none(), toast(&remote.rig.app)), (true, None));
        assert_eq!(remote.rig.seen("ask"), 0);
    }

    #[test]
    fn says_nothing_was_sent_once_the_machine_restarted() {
        let mut remote = folding(Reply::Agree, false);
        remote.menu_on(0);

        remote.sends(family(RESTARTED, false));
        remote.choose(0);

        assert_eq!(toast(&remote.rig.app), Some("vps restarted; nothing was sent"));
        assert_eq!(remote.rig.seen("ask"), 0);
    }
}
