use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::*;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn proc(pid: i32, depth: usize, name: &str, cpu: f64) -> ProcRow {
    ProcRow {
        pid,
        started: Some(u64::try_from(pid).unwrap_or(0) * 10),
        depth,
        name: name.into(),
        command: format!("{name} --serve"),
        cpu,
        memory: 1 << 20,
        stoppable: depth > 0,
        agent: None,
    }
}

fn pane(id: u64, order: usize, place: &str, cpu: f64, memory: u64) -> PaneRow {
    PaneRow {
        pane: Some(id),
        order,
        place: place.into(),
        program: "npm".into(),
        cpu,
        memory,
        procs: vec![
            proc(100 + i32::try_from(id).unwrap_or(0), 0, "zsh", 0.0),
            proc(200 + i32::try_from(id).unwrap_or(0), 1, "node", cpu),
        ],
    }
}

fn own() -> PaneRow {
    let mut row = pane(0, usize::MAX, "cornercase", 0.5, 10);
    row.pane = None;
    row.procs.iter_mut().for_each(|proc| proc.stoppable = false);
    row
}

fn loaded() -> Dialog {
    let mut dialog = Dialog::new(1, Sort::Cpu);
    let rows = vec![
        pane(1, 0, "web › main › dev", 10.0, 300),
        pane(2, 1, "api › main › server", 90.0, 100),
        pane(3, 2, "docs › main › zsh", 90.0, 900),
    ];
    dialog.listed(Ok((rows, Some(own()))), false);
    dialog
}

fn order(dialog: &Dialog) -> Vec<Option<u64>> {
    dialog.rows.iter().map(|row| row.pane).collect()
}

mod sorting {
    use super::*;

    #[test]
    fn cpu_puts_the_busiest_first_and_keeps_the_sidebar_order_for_ties() {
        assert_eq!(order(&loaded()), [Some(2), Some(3), Some(1)]);
    }

    #[test]
    fn memory_and_name_reorder_the_rows() {
        let mut dialog = loaded();
        dialog.click(Hit::Sort(Sort::Memory));
        assert_eq!(order(&dialog), [Some(3), Some(1), Some(2)]);
        dialog.click(Hit::Sort(Sort::Name));
        assert_eq!(order(&dialog), [Some(2), Some(3), Some(1)]);
    }

    #[test]
    fn tab_cycles_the_sort() {
        let mut dialog = loaded();
        dialog.key(key(KeyCode::Tab), 10);
        assert_eq!(dialog.sort, Sort::Memory);
        dialog.key(key(KeyCode::Tab), 10);
        dialog.key(key(KeyCode::Tab), 10);
        assert_eq!(dialog.sort, Sort::Cpu);
    }

    #[test]
    fn the_cornercase_row_always_comes_last() {
        let dialog = loaded();
        assert_eq!(dialog.visible().last(), Some(&Line::Pane(3)));
    }
}

mod keeping_the_order {
    use super::*;

    #[test]
    fn existing_rows_keep_their_places_new_ones_come_last_and_gone_ones_leave() {
        let mut dialog = loaded();
        let rows = vec![pane(1, 0, "web", 99.0, 1), pane(4, 3, "new", 50.0, 1), pane(2, 1, "api", 1.0, 1)];
        dialog.listed(Ok((rows, None)), true);
        assert_eq!(order(&dialog), [Some(2), Some(1), Some(4)]);
        assert_eq!(dialog.rows[1].cpu, 99.0);
    }

    #[test]
    fn without_it_the_rows_sort_again() {
        let mut dialog = loaded();
        let rows = vec![pane(1, 0, "web", 99.0, 1), pane(2, 1, "api", 1.0, 1)];
        dialog.listed(Ok((rows, None)), false);
        assert_eq!(order(&dialog), [Some(1), Some(2)]);
    }

    #[test]
    fn the_selection_follows_its_row_across_a_sort() {
        let mut dialog = loaded();
        dialog.click(Hit::Row(2));
        assert_eq!(dialog.selected, Some(Key::Pane(1)));
        let rows = vec![pane(1, 0, "web", 99.0, 1), pane(2, 1, "api", 1.0, 1), pane(3, 2, "docs", 2.0, 1)];
        dialog.listed(Ok((rows, None)), false);
        let lines = dialog.visible();
        assert_eq!(dialog.selected_line(&lines), Some(0));
    }
}

mod totals {
    use super::*;

    #[test]
    fn add_every_row_and_the_cornercase_one() {
        let totals = loaded().totals();
        assert_eq!((totals.panes, totals.processes, totals.memory), (3, 8, 1310));
        assert!((totals.cpu - 190.5).abs() < 1e-9);
    }
}

mod filtering {
    use super::*;

    fn typed(dialog: &mut Dialog, text: &str) {
        for c in text.chars() {
            dialog.key(key(KeyCode::Char(c)), 10);
        }
    }

    #[test]
    fn matches_the_place_and_hides_the_cornercase_row() {
        let mut dialog = loaded();
        typed(&mut dialog, "API");
        assert_eq!(dialog.visible(), [Line::Pane(0)]);
        assert_eq!(dialog.selected, Some(Key::Pane(2)));
    }

    #[test]
    fn a_pane_matched_through_a_process_shows_expanded() {
        let mut dialog = loaded();
        dialog.rows[0].procs[1].command = "esbuild --watch".into();
        typed(&mut dialog, "esbuild");
        let key = dialog.rows[0].pane;
        assert_eq!(key, Some(2));
        assert_eq!(dialog.visible(), [Line::Pane(0), Line::Proc(0, 0), Line::Proc(0, 1)]);
    }

    #[test]
    fn nothing_left_is_an_empty_list_and_esc_clears_before_closing() {
        let mut dialog = loaded();
        typed(&mut dialog, "zzz");
        assert_eq!(dialog.visible(), Vec::<Line>::new());
        assert_eq!(dialog.key(key(KeyCode::Esc), 10), Action::None);
        assert_eq!(dialog.filter, "");
        assert_eq!(dialog.key(key(KeyCode::Esc), 10), Action::Close);
    }

    #[test]
    fn backspace_and_paste_edit_the_filter() {
        let mut dialog = loaded();
        dialog.paste("web\nsecond line");
        assert_eq!(dialog.filter, "web");
        dialog.key(key(KeyCode::Backspace), 10);
        assert_eq!(dialog.filter, "we");
    }

    #[test]
    fn control_keys_are_not_typed() {
        let mut dialog = loaded();
        dialog.key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL), 10);
        assert_eq!(dialog.filter, "");
    }
}

mod moving {
    use super::*;

    #[test]
    fn arrows_walk_the_visible_lines_and_right_and_left_fold_a_row() {
        let mut dialog = loaded();
        assert_eq!(dialog.selected, Some(Key::Pane(2)));
        dialog.key(key(KeyCode::Right), 10);
        assert_eq!(dialog.visible().len(), 6);
        dialog.key(key(KeyCode::Down), 10);
        assert_eq!(dialog.selected, Some(Key::Process(102, Some(1020))));
        dialog.key(key(KeyCode::Left), 10);
        assert_eq!(dialog.selected, Some(Key::Pane(2)));
        dialog.key(key(KeyCode::Left), 10);
        assert_eq!(dialog.visible().len(), 4);
    }

    #[test]
    fn a_click_on_a_pane_row_selects_and_toggles_it() {
        let mut dialog = loaded();
        dialog.click(Hit::Row(1));
        assert!(dialog.expanded.contains(&Key::Pane(3)));
        dialog.click(Hit::Row(1));
        assert!(dialog.expanded.is_empty());
    }
}

mod acting {
    use super::*;

    fn expanded() -> Dialog {
        let mut dialog = loaded();
        dialog.key(key(KeyCode::Right), 10);
        dialog
    }

    #[test]
    fn enter_on_a_pane_row_goes_to_the_pane_and_done_closes() {
        let mut dialog = loaded();
        assert_eq!(dialog.key(key(KeyCode::Enter), 10), Action::Go(2));
        assert_eq!(dialog.click(Hit::Done), Action::Close);
    }

    #[test]
    fn the_row_buttons_go_and_close_name_their_pane() {
        let mut dialog = loaded();
        assert_eq!(dialog.click(Hit::Go(1)), Action::Go(3));
        assert_eq!(dialog.click(Hit::Close(2)), Action::ClosePane(1));
    }

    #[test]
    fn the_cornercase_row_has_no_pane_to_go_to_or_close() {
        let mut dialog = loaded();
        assert_eq!(dialog.click(Hit::Go(3)), Action::None);
        assert_eq!(dialog.click(Hit::Close(3)), Action::None);
    }

    #[test]
    fn the_shell_line_offers_no_stop() {
        let mut dialog = expanded();
        assert_eq!(dialog.click(Hit::StopRow(1)), Action::None);
        assert_eq!(dialog.step, Step::List);
    }

    #[test]
    fn a_stop_asks_first_and_nothing_stops_until_the_confirmation() {
        let mut dialog = expanded();
        assert_eq!(dialog.click(Hit::StopRow(2)), Action::None);
        let Step::Confirm(target) = &dialog.step else { panic!("a confirmation: {:?}", dialog.step) };
        assert_eq!((target.group.root.pid, target.place.as_str()), (202, "api › main › server"));
        assert_eq!(dialog.key(key(KeyCode::Esc), 10), Action::None);
        assert_eq!(dialog.step, Step::List);
        dialog.click(Hit::StopRow(2));
        assert!(matches!(dialog.click(Hit::Stop), Action::Stop(group) if group.root.pid == 202));
        assert_eq!(dialog.step, Step::List);
    }

    #[test]
    fn the_confirmation_names_what_it_captured_and_keeps_it_across_a_refresh() {
        let mut dialog = expanded();
        dialog.rows[0].procs.push(proc(300, 2, "esbuild", 1.0));
        dialog.click(Hit::StopRow(2));
        let rows = vec![pane(9, 0, "other", 99.0, 1)];
        dialog.listed(Ok((rows, None)), false);
        let Action::Stop(group) = dialog.key(key(KeyCode::Enter), 10) else { panic!("a stop") };
        assert_eq!((group.root.pid, group.members.iter().map(|m| m.pid).collect::<Vec<_>>()), (202, vec![300]));
    }

    #[test]
    fn enter_on_a_process_line_opens_its_confirmation() {
        let mut dialog = expanded();
        dialog.key(key(KeyCode::Down), 10);
        dialog.key(key(KeyCode::Down), 10);
        dialog.key(key(KeyCode::Enter), 10);
        assert!(matches!(dialog.step, Step::Confirm(_)));
    }

    #[test]
    fn the_confirmation_ignores_clicks_on_the_list() {
        let mut dialog = expanded();
        dialog.click(Hit::StopRow(2));
        assert_eq!(dialog.click(Hit::Go(0)), Action::None);
        assert_eq!(dialog.click(Hit::Back), Action::None);
        assert_eq!(dialog.step, Step::List);
    }
}
