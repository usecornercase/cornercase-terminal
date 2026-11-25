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
    fn lists_this_computer_then_each_machine_in_order_then_add_and_the_memory_a_start_needs() {
        let s = with(&[
            (machine("vps", "vps.lan"), State::Linked),
            (machine("pi", "pi.lan"), State::Older),
            (machine("mini", "mini.local"), State::Unreachable),
        ]);

        let rows = [
            Row::Here,
            Row::Machine(0),
            Row::Machine(1),
            Row::UpdateMachine(1),
            Row::Machine(2),
            Row::AddMachine,
            Row::DefaultMemory,
        ];
        assert_eq!(s.rows(), rows);
    }

    #[test]
    fn with_no_machine_lists_this_computer_and_add() {
        assert_eq!(with(&[]).rows(), [Row::Here, Row::AddMachine, Row::DefaultMemory]);
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

        assert_eq!(removals, [None, Some(Action::RemoveMachine(0)), None, Some(Action::RemoveMachine(1)), None, None]);
    }

    #[test]
    fn backspace_removes_the_selected_machine() {
        let mut s = two();
        go_to(&mut s, &Row::Machine(1));

        assert_eq!(press(&mut s, KeyCode::Backspace), Action::RemoveMachine(1));
    }
}

mod the_toggle {
    use super::*;

    fn three() -> Settings {
        let mut s = with(&[
            (machine("vps", "vps.lan"), State::Older),
            (Machine { shown: false, ..machine("pi", "pi.lan") }, State::Connecting),
        ]);
        s.config.machines.push(machine("bad name", "bad.lan"));
        s
    }

    #[test]
    fn is_ticked_for_a_shown_machine_and_not_for_a_hidden_one_and_absent_elsewhere() {
        let s = three();

        let toggles: Vec<Option<bool>> = s.rows().iter().map(|row| s.row_text(row).toggle).collect();

        assert_eq!(toggles, [None, Some(true), None, Some(false), None, None, None]);
    }

    #[test]
    fn flips_what_the_machine_shows() {
        let s = three();

        let flips: Vec<Option<Action>> = (0..s.rows().len()).map(|i| s.toggling(i)).collect();

        let expected =
            [None, Some(Action::ShowMachine(0, false)), None, Some(Action::ShowMachine(1, true)), None, None, None];
        assert_eq!(flips, expected);
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
    fn otherwise_says_what_enter_the_toggle_and_the_cross_do() {
        let mut s = with(&[(machine("vps", "vps.lan"), State::Linked)]);
        go_to(&mut s, &Row::Machine(0));

        assert_eq!(s.hint(), "enter edits a machine · ✓ hides it here · × removes it; its server keeps running there");
    }

    #[test]
    fn leaves_the_toggle_out_on_an_update_row_which_has_none() {
        let mut s = with(&[(machine("pi", "pi.lan"), State::Older)]);
        go_to(&mut s, &Row::UpdateMachine(0));

        assert_eq!(s.hint(), "enter edits a machine · × removes it, and its server keeps running there");
    }
}

mod a_machine_that_is_not_shown {
    use super::*;

    fn hidden() -> Machine {
        Machine { shown: false, ..machine("devbox", "me@10.0.0.5") }
    }

    #[test]
    fn is_grey_says_so_and_has_no_update_row() {
        let s = with(&[(hidden(), State::Connecting)]);

        assert_eq!(s.rows(), [Row::Here, Row::Machine(0), Row::AddMachine, Row::DefaultMemory]);
        let row = s.row_text(&Row::Machine(0));
        assert_eq!((row.note.as_str(), row.mark, row.dangerous), ("hidden", Some(State::Connecting), false));
    }

    #[test]
    fn has_a_hint_that_says_how_to_link_it() {
        let mut s = with(&[(hidden(), State::Connecting)]);
        go_to(&mut s, &Row::Machine(0));

        assert_eq!(
            s.hint(),
            "hidden: nothing is linked and its server keeps running there; its toggle shows it here again"
        );
    }
}

mod the_first_row_while_the_window_is_on_another_machine {
    use super::*;

    #[test]
    fn names_the_machine_and_says_whose_list_this_is() {
        let mut s = with(&[]);
        s.on = Some("vps".into());

        assert_eq!(text(&s, &Row::Here).0, "  ▭ vps");
        go_to(&mut s, &Row::Here);
        assert!(s.hint().starts_with("vps runs the server this window is on"), "{}", s.hint());
    }
}

mod the_memory_a_start_needs {
    use super::*;

    #[test]
    fn shows_the_default_and_what_it_is_for() {
        let s = with(&[]);

        let (label, value, note, _) = text(&s, &Row::DefaultMemory);

        assert_eq!((label.as_str(), value.as_str()), ("a start needs", "4GB"));
        assert!(note.contains("auto and the queue"), "{note}");
    }

    #[test]
    fn is_edited_and_saved_as_typed() {
        let mut s = with(&[]);
        go_to(&mut s, &Row::DefaultMemory);
        press(&mut s, KeyCode::Enter);
        s.edit.as_mut().expect("editing").input = "6 GB".into();

        assert_eq!(saved(press(&mut s, KeyCode::Enter)).default_memory, "6 GB");
        assert_eq!(s.notice.as_deref(), Some("a start that names no memory needs 6 GB"));
    }

    #[test]
    fn an_amount_it_cannot_read_is_refused() {
        let mut s = with(&[]);
        go_to(&mut s, &Row::DefaultMemory);
        press(&mut s, KeyCode::Enter);
        s.edit.as_mut().expect("editing").input = "lots".into();

        assert_eq!(press(&mut s, KeyCode::Enter), Action::None);
        assert!(s.edit.as_ref().is_some_and(|edit| edit.error.is_some()));
    }
}

mod this_computer {
    use super::*;
    use crate::machines::memory::MemoryInfo;
    use crate::text::GB;

    #[test]
    fn says_how_much_memory_is_free() {
        let mut s = with(&[]);
        s.here_memory = Some(MemoryInfo { total: 16 * GB, available: 5 * GB, read_at: None });

        let (_, _, note, _) = text(&s, &Row::Here);

        assert_eq!(note, format!("{} · 5 GB free of 16 GB", version::CURRENT));
    }

    #[test]
    fn says_only_its_version_while_its_memory_is_unknown() {
        let (_, _, note, _) = text(&with(&[]), &Row::Here);

        assert_eq!(note, version::CURRENT);
    }
}
