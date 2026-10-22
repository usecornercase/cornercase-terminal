use super::*;

fn arena() -> Flow {
    builtin::flows(&Known::ALL, Some(Known::Claude)).into_iter().find(|flow| flow.name == "Arena").expect("arena")
}

fn with_default(name: &str) -> Dialog {
    let mut d = dialog();
    d.default = Some(name.into());
    d
}

fn names(d: &Dialog) -> Vec<String> {
    d.flows.iter().map(|saved| saved.flow.name.clone()).collect()
}

fn listing(d: &Dialog, flows: Vec<Saved>) -> Listing {
    Listing {
        flows,
        hidden: d.hidden.clone(),
        broken: d.broken.clone(),
        builtins: d.builtins.clone(),
        default: d.default.clone(),
    }
}

mod the_default_flow {
    use super::*;

    #[test]
    fn set_default_names_the_selected_flow() {
        let mut d = dialog();
        d.select(2);
        assert_eq!(d.press(SET_DEFAULT), Action::Default("Quick".into()));
    }

    #[test]
    fn set_default_on_the_default_writes_nothing_and_says_so() {
        let mut d = with_default("Quick");
        d.select(2);
        assert_eq!(d.press(SET_DEFAULT), Action::None);
        assert_eq!(d.hint(), "Quick is already the default: the start dialog opens on it");
    }

    #[test]
    fn every_listed_flow_offers_set_default_but_not_files_or_deleted_built_ins() {
        let mut d = dialog();
        d.hidden = vec![arena()];
        d.show_hidden = true;
        for i in 0..d.entries() {
            d.select(i);
            let offered = d.buttons(ROOM).contains(&SET_DEFAULT);
            assert_eq!(offered, i < d.flows.len(), "entry {i}");
        }
    }

    #[test]
    fn the_default_flow_is_tagged_before_edited_or_this_project() {
        let mut d = with_default("Mine");
        d.flows[mine()].origin = Origin::Project(PathBuf::from("/p/mine.toml"));
        let rows = d.list_rows();
        assert!(rows[mine()].note.starts_with("default · this project · "), "{}", rows[mine()].note);
        assert!(!rows[0].note.contains("default"), "{}", rows[0].note);
    }

    #[test]
    fn a_red_default_still_says_default() {
        let mut d = with_default("Mine");
        d.flows[mine()].flow.stages.clear();
        let row = &d.list_rows()[mine()];
        assert_eq!((row.red, row.note.as_str()), (true, "default"));
    }
}

mod dragging {
    use super::*;

    #[test]
    fn grabbing_selects_and_a_release_without_moving_opens_only_a_selected_flow() {
        let mut d = dialog();
        d.grab(2);
        assert_eq!(d.cursor, 2);
        assert_eq!(d.release(), Action::None);
        assert!(d.edit.is_none());
        d.grab(2);
        assert_eq!(d.release(), Action::None);
        assert!(d.edit.is_some());
    }

    #[test]
    fn dropping_above_a_flow_orders_every_flow_with_it_there() {
        let mut d = dialog();
        d.grab(4);
        d.drag_over(Some(1));
        assert_eq!(d.drag.and_then(|drag| drag.landing), Some(1));
        let Action::Order(order) = d.release() else { panic!("not ordered") };
        assert_eq!(order, ["Full issue", "Mine", "Cross review", "Quick", "Bug, test first"]);
    }

    #[test]
    fn dropping_below_a_flow_or_below_the_list_moves_it_after_that_flow() {
        let mut d = dialog();
        d.grab(0);
        d.drag_over(Some(3));
        let Action::Order(order) = d.release() else { panic!("not ordered") };
        assert_eq!(order, ["Cross review", "Quick", "Bug, test first", "Full issue", "Mine"]);
        d.grab(0);
        d.drag_over(Some(9));
        let Action::Order(order) = d.release() else { panic!("not ordered") };
        assert_eq!(order, ["Cross review", "Quick", "Bug, test first", "Mine", "Full issue"]);
    }

    #[test]
    fn a_landing_at_its_own_place_moves_nothing() {
        let mut d = dialog();
        d.grab(4);
        d.drag_over(Some(5));
        assert_eq!(d.drag.and_then(|drag| drag.landing), None);
        assert_eq!(d.release(), Action::None);
        assert!(d.edit.is_none());
        d.grab(2);
        d.drag_over(Some(0));
        d.drag_over(Some(2));
        assert_eq!(d.release(), Action::None);
    }

    #[test]
    fn leaving_the_list_shows_no_landing() {
        let mut d = dialog();
        d.grab(2);
        d.drag_over(Some(0));
        d.drag_over(None);
        assert_eq!(d.drag.map(|drag| (drag.moved, drag.landing)), Some((true, None)));
        assert_eq!(d.hint(), "release to move Quick there · esc puts it back");
    }

    #[test]
    fn esc_cancels_a_drag_and_keeps_the_dialog() {
        let mut d = dialog();
        d.grab(2);
        d.drag_over(Some(0));
        assert_eq!(key(&mut d, KeyCode::Esc), Action::None);
        assert!(d.drag.is_none());
        assert_eq!(key(&mut d, KeyCode::Esc), Action::Close);
    }

    #[test]
    fn a_new_listing_cancels_a_drag_and_keeps_the_selected_flow() {
        let mut d = dialog();
        d.grab(2);
        let mut flows = d.flows.clone();
        flows.reverse();
        let next = listing(&d, flows);
        d.set_listing(next);
        assert!(d.drag.is_none());
        assert_eq!(names(&d)[d.cursor], "Quick");
    }

    #[test]
    fn a_wheel_turn_does_not_move_the_cursor_while_a_flow_is_held() {
        let mut d = dialog();
        d.grab(2);
        d.wheel(1, ROOM);
        assert_eq!(d.cursor, 2);
    }
}
