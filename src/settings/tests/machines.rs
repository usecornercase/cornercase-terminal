use rstest::rstest;

use super::*;
use crate::machines::{Info, Machine, machine};
use crate::version;

fn info(machine: &Machine, state: State) -> Info {
    let version = matches!(state, State::Linked | State::Older).then(|| "0.12.200".to_string());
    Info { state, server_version: version.clone(), version, ..Info::of(machine, None, None, None) }
}

fn with(machines: &[(Machine, State)]) -> Settings {
    let mut s = settings();
    s.config.machines = machines.iter().map(|(machine, _)| machine.clone()).collect();
    s.machines = machines.iter().map(|(machine, state)| info(machine, *state)).collect();
    s.open_page(Page::Machines);
    s
}

fn text(s: &Settings, row: &Row) -> (String, String, String, Option<State>) {
    let text = s.row_text(row);
    (text.label, text.value, text.note, text.mark)
}

fn place(s: &Settings, row: &Row) -> (String, String) {
    let text = s.row_text(row);
    (text.value, text.tail)
}

mod the_page {
    use super::*;

    #[test]
    fn is_the_last_tab_after_plugins() {
        assert_eq!((Page::ALL.len(), Page::ALL.last()), (9, Some(&Page::Machines)));
        assert_eq!((Page::Plugins.step(1), Page::Machines.step(1)), (Page::Machines, Page::Worktrees));
    }

    #[test]
    fn lists_this_computer_then_each_machine_in_order_then_add() {
        let s = with(&[
            (machine("vps", "vps.lan"), State::Linked),
            (machine("pi", "pi.lan"), State::Older),
            (machine("mini", "mini.local"), State::Unreachable),
        ]);

        let rows =
            [Row::Here, Row::Machine(0), Row::Machine(1), Row::UpdateMachine(1), Row::Machine(2), Row::AddMachine];
        assert_eq!(s.rows(), rows);
    }

    #[test]
    fn with_no_machine_lists_this_computer_and_add() {
        assert_eq!(with(&[]).rows(), [Row::Here, Row::AddMachine]);
    }
}

mod rows {
    use super::*;

    #[test]
    fn this_computer_shows_its_version_and_offers_nothing() {
        let mut s = with(&[]);

        assert_eq!(text(&s, &Row::Here), ("  ▭ this computer".into(), String::new(), version::CURRENT.into(), None));
        assert_eq!((s.activate(), s.notice.clone(), s.removal(0)), (Action::None, None, None));
    }

    #[test]
    fn a_machine_shows_its_icon_name_destination_folder_and_state() {
        let mini = Machine { icon: "▣".into(), folder: "/srv/code".into(), ..machine("mini", "mini.local") };
        let s = with(&[(machine("vps", "miguel@vps.example.com"), State::Linked), (mini, State::Unreachable)]);

        assert_eq!(
            [text(&s, &Row::Machine(0)), text(&s, &Row::Machine(1))],
            [
                ("☁ vps".into(), "miguel@vps.example.com".into(), "linked · 0.12.200".into(), Some(State::Linked)),
                (
                    "▣ mini".into(),
                    "mini.local            ".into(),
                    "unreachable · not seen yet".into(),
                    Some(State::Unreachable)
                ),
            ]
        );
        assert_eq!(
            [place(&s, &Row::Machine(0)).1, place(&s, &Row::Machine(1)).1],
            ["~/code   ".to_string(), "/srv/code".into()]
        );
    }

    #[test]
    fn a_long_destination_is_whole_and_a_long_folder_is_cut_at_its_start() {
        let long = Machine {
            destination: "deploy@build-server-number-four.example.com".into(),
            folder: "/home/deploy/projects/clients/acme".into(),
            ..machine("build", "")
        };
        let s = with(&[(long, State::Connecting)]);

        assert_eq!(
            place(&s, &Row::Machine(0)),
            ("deploy@build-server-number-four.example.com".into(), "…y/projects/clients/acme".into())
        );
    }

    #[test]
    fn an_unknown_icon_shows_as_the_server_one_and_stays_in_the_file() {
        let s = with(&[(Machine { icon: "🖥".into(), ..machine("box", "box.lan") }, State::Connecting)]);

        assert_eq!((text(&s, &Row::Machine(0)).0.as_str(), s.config.machines[0].icon.as_str()), ("☁ box", "🖥"));
    }

    #[test]
    fn an_entry_with_a_problem_says_it_in_red_with_the_unreachable_mark() {
        let mut s = settings();
        s.config.machines = vec![machine("my vps", "vps.lan")];
        s.open_page(Page::Machines);

        let row = s.row_text(&Row::Machine(0));
        let problem = "use letters, digits, `.`, `_` and `-`, at most 20 characters";
        assert_eq!((row.note.as_str(), row.mark, row.dangerous), (problem, Some(State::Unreachable), true));
    }

    #[rstest]
    #[case::connecting(State::Connecting, '◌')]
    #[case::linked(State::Linked, '●')]
    #[case::unreachable(State::Unreachable, '○')]
    #[case::older(State::Older, '◐')]
    fn are_marked_by_their_state(#[case] state: State, #[case] mark: char) {
        let s = with(&[(machine("vps", "vps.lan"), state)]);

        assert_eq!(s.row_text(&Row::Machine(0)).mark.map(State::mark), Some(mark));
    }

    #[test]
    fn the_update_row_says_while_it_updates() {
        let mut s = with(&[(machine("pi", "pi.lan"), State::Older)]);
        assert!(!text(&s, &Row::UpdateMachine(0)).2.contains("updating"));

        s.machines[0].updating = true;

        assert_eq!(text(&s, &Row::UpdateMachine(0)), ("    update…".into(), String::new(), "updating…".into(), None));
    }
}

mod acting {
    use super::*;

    fn two() -> Settings {
        with(&[(machine("vps", "vps.lan"), State::Older), (machine("pi", "pi.lan"), State::Linked)])
    }

    #[rstest]
    #[case::a_machine(Row::Machine(1), Action::EditMachine(1))]
    #[case::its_update(Row::UpdateMachine(0), Action::UpdateMachine(0))]
    #[case::add(Row::AddMachine, Action::AddMachine)]
    fn enter_opens_what_the_row_says(#[case] row: Row, #[case] action: Action) {
        let mut s = two();
        go_to(&mut s, &row);

        assert_eq!(press(&mut s, KeyCode::Enter), action);
    }

    #[test]
    fn only_a_machine_has_a_cross() {
        let s = two();

        let removals: Vec<Option<Action>> = (0..s.rows().len()).map(|i| s.removal(i)).collect();

        assert_eq!(removals, [None, Some(Action::RemoveMachine(0)), None, Some(Action::RemoveMachine(1)), None]);
    }

    #[test]
    fn backspace_removes_the_selected_machine() {
        let mut s = two();
        go_to(&mut s, &Row::Machine(1));

        assert_eq!(press(&mut s, KeyCode::Backspace), Action::RemoveMachine(1));
    }
}

mod selection {
    use super::*;

    fn turned(s: &mut Settings, state: State) {
        let infos = s.config.machines.iter().map(|machine| info(machine, state)).collect();
        s.take_machines(infos);
    }

    #[test]
    fn stays_on_its_row_when_an_update_row_comes_above_it() {
        let mut s = with(&[(machine("vps", "vps.lan"), State::Linked)]);
        go_to(&mut s, &Row::AddMachine);

        turned(&mut s, State::Older);

        assert_eq!(s.row(), Some(Row::AddMachine));
    }

    #[test]
    fn goes_to_the_machine_when_its_update_row_is_gone() {
        let mut s = with(&[(machine("vps", "vps.lan"), State::Older), (machine("pi", "pi.lan"), State::Older)]);
        go_to(&mut s, &Row::UpdateMachine(0));

        turned(&mut s, State::Linked);

        assert_eq!(s.row(), Some(Row::Machine(0)));
    }
}

mod the_hint {
    use super::*;

    #[test]
    fn says_why_the_selected_machine_is_not_linked() {
        let mut s = with(&[(machine("pi", "pi.lan"), State::Unreachable)]);
        s.machines[0].error = Some("ssh: connect to host pi.lan port 22: Connection refused".into());
        go_to(&mut s, &Row::Machine(0));

        assert_eq!(s.hint(), "ssh: connect to host pi.lan port 22: Connection refused");
    }

    #[test]
    fn otherwise_says_what_enter_and_the_cross_do() {
        let mut s = with(&[(machine("vps", "vps.lan"), State::Linked)]);
        go_to(&mut s, &Row::Machine(0));

        assert_eq!(s.hint(), "enter edits a machine · × removes it, and its server keeps running there");
    }
}
