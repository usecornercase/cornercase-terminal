use super::*;

fn saved(s: &Setup) -> Config {
    config::load(&s.config.path().join("config.json"))
}

fn names(app: &App) -> Vec<String> {
    dialog(app).flows.iter().map(|saved| saved.flow.name.clone()).collect()
}

fn press_row(app: &mut App, i: usize, area: Rect) {
    let at = row_rect(app, area, i).as_position();
    mouse_in(app, MouseEventKind::Down(MouseButton::Left), at, area);
}

fn drag_to(app: &mut App, i: usize, area: Rect) {
    let at = row_rect(app, area, i).as_position();
    mouse_in(app, MouseEventKind::Drag(MouseButton::Left), at, area);
}

fn drop_on(app: &mut App, i: usize, area: Rect) {
    let at = row_rect(app, area, i).as_position();
    mouse_in(app, MouseEventKind::Up(MouseButton::Left), at, area);
}

fn drag_in(app: &mut App, from: usize, to: usize, area: Rect) {
    press_row(app, from, area);
    drag_to(app, to, area);
    drop_on(app, to, area);
}

fn confirm(s: &mut Setup, label: &str) {
    let [submit, _] = ui::form_buttons(ui::form_area(WIDE), label);
    click_in(&mut s.app, submit.as_position(), WIDE);
}

fn copy_of_full_issue_as_default() -> Setup {
    let mut s = settings_tab(false);
    duplicate_full_issue(&mut s, WIDE);
    button(&mut s.app, WIDE, dialog::DONE);
    button(&mut s.app, WIDE, dialog::SET_DEFAULT);
    s
}

mod ordering {
    use super::*;

    #[test]
    fn dragging_a_row_writes_flow_order_and_the_list_follows() {
        let mut s = settings_tab(false);
        let before = names(&s.app);
        let last = before.len() - 1;
        drag_in(&mut s.app, last, 0, WIDE);
        let mut expected = before.clone();
        expected.rotate_right(1);
        assert_eq!(names(&s.app), expected);
        assert_eq!(s.app.config.flow_order, expected);
        assert_eq!(saved(&s).flow_order, expected);
        assert_eq!(dialog(&s.app).cursor, 0);
        assert!(dialog(&s.app).edit.is_none());
    }

    #[test]
    fn esc_during_a_drag_moves_nothing_and_keeps_the_dialog_open() {
        let mut s = settings_tab(false);
        press_row(&mut s.app, 2, WIDE);
        drag_to(&mut s.app, 0, WIDE);
        key_in(&mut s.app, KeyCode::Esc, WIDE);
        drop_on(&mut s.app, 0, WIDE);
        assert_eq!(s.app.config.flow_order, Vec::<String>::new());
        assert!(dialog(&s.app).drag.is_none());
        assert!(matches!(s.app.overlay, Some(Overlay::Flows { .. })));
    }

    #[test]
    fn a_click_on_the_selected_flow_still_opens_it() {
        let mut s = settings_tab(false);
        click_list_row(&mut s.app, WIDE, 2);
        assert!(dialog(&s.app).edit.is_none());
        assert_eq!(dialog(&s.app).cursor, 2);
        click_list_row(&mut s.app, WIDE, 2);
        assert!(dialog(&s.app).edit.is_some());
    }

    #[test]
    fn ordering_in_one_project_keeps_another_projects_flow_in_place() {
        let mut s = setup(true);
        let alpha = crate::flows::Flow {
            name: "Alpha".into(),
            ..crate::flows::builtin::starter(&[Known::Claude], Some(Known::Claude))
        };
        let first = s.project.as_ref().expect("a project").path().to_path_buf();
        store::save(&store::project_dir(&first), &alpha, None).expect("save");
        let other = TempDir::new();
        s.app.open_project(other.path().to_path_buf(), WIDE).expect("open the second project");
        s.app.change_config(|config| config.flow_order = vec!["Alpha".into(), "Quick".into()]).expect("write");
        let second = s.app.projects.last().expect("a project").id;
        s.app.open_flows(Some(second), None);
        let quick = names(&s.app).iter().position(|name| name == "Quick").expect("Quick is listed");
        drag_in(&mut s.app, quick, 0, WIDE);
        assert_eq!(&s.app.config.flow_order[..2], ["Alpha", "Quick"]);
    }
}

mod default {
    use super::*;

    #[test]
    fn set_default_writes_default_flow_and_moves_the_tag() {
        let mut s = settings_tab(false);
        select_flow(&mut s.app, WIDE, "Quick");
        button(&mut s.app, WIDE, dialog::SET_DEFAULT);
        assert_eq!((s.app.config.default_flow.as_str(), saved(&s).default_flow.as_str()), ("Quick", "Quick"));
        assert_eq!(dialog(&s.app).default.as_deref(), Some("Quick"));
        select_flow(&mut s.app, WIDE, "Arena");
        button(&mut s.app, WIDE, dialog::SET_DEFAULT);
        assert_eq!(saved(&s).default_flow, "Arena");
        let notes: Vec<(String, bool)> =
            dialog(&s.app).list_rows().into_iter().map(|row| (row.label, row.note.starts_with("default"))).collect();
        assert!(notes.iter().all(|(label, tagged)| *tagged == (label == "Arena")), "{notes:?}");
    }

    #[test]
    fn renaming_the_default_flow_moves_the_default_and_its_place() {
        let mut s = copy_of_full_issue_as_default();
        s.app.change_config(|config| config.flow_order = vec!["Full issue copy".into()]).expect("write");
        s.app.refresh_flows();
        click_list_row(&mut s.app, WIDE, 0);
        assert!(dialog(&s.app).edit.is_some());
        click_row(&mut s.app, WIDE, Row::Name);
        for _ in 0.."copy".len() {
            key_in(&mut s.app, KeyCode::Backspace, WIDE);
        }
        type_in(&mut s.app, "again", WIDE);
        button(&mut s.app, WIDE, dialog::SAVE);
        button(&mut s.app, WIDE, dialog::SAVE);
        let config = saved(&s);
        assert_eq!(config.default_flow, "Full issue again");
        assert_eq!(config.flow_order, ["Full issue again"]);
    }

    fn edited_quick_as_default() -> Setup {
        let mut s = settings_tab(false);
        let quick = crate::flows::Flow {
            name: "Quick".into(),
            ..crate::flows::builtin::starter(&[Known::Claude], Some(Known::Claude))
        };
        store::save(&user_dir(&s), &quick, None).expect("save");
        s.app.refresh_flows();
        select_flow(&mut s.app, WIDE, "Quick");
        button(&mut s.app, WIDE, dialog::SET_DEFAULT);
        s
    }

    #[test]
    fn restoring_the_default_built_in_keeps_the_default() {
        let mut s = edited_quick_as_default();
        button(&mut s.app, WIDE, dialog::RESTORE);
        confirm(&mut s, dialog::RESTORE);
        assert_eq!(files(&user_dir(&s)), Vec::<String>::new());
        assert_eq!(saved(&s).default_flow, "Quick");
        assert_eq!(dialog(&s.app).default.as_deref(), Some("Quick"));
    }

    #[test]
    fn deleting_your_version_of_the_default_built_in_hides_it_and_clears_the_default() {
        let mut s = edited_quick_as_default();
        button(&mut s.app, WIDE, dialog::DELETE);
        confirm(&mut s, dialog::DELETE);
        let config = saved(&s);
        assert_eq!(
            (config.default_flow.as_str(), config.hidden_flows.as_slice()),
            ("", ["Quick".to_string()].as_slice())
        );
    }

    #[test]
    fn renaming_the_flow_the_default_did_not_resolve_to_leaves_the_default() {
        let mut s = setup(true);
        let twin = crate::flows::Flow {
            name: "Twin".into(),
            ..crate::flows::builtin::starter(&[Known::Claude], Some(Known::Claude))
        };
        let project = store::project_dir(s.project.as_ref().expect("a project").path());
        let theirs = store::save(&project, &twin, None).expect("save");
        store::save(&user_dir(&s), &twin, None).expect("save");
        s.app.change_config(|config| config.default_flow = "Twin".into()).expect("write");
        let renamed = crate::flows::Flow { name: "Twin two".into(), ..twin };
        s.app.save_flow_file(&project, &renamed, Some(&theirs)).expect("rename");
        assert_eq!(saved(&s).default_flow, "Twin");
    }

    #[test]
    fn deleting_the_default_flow_clears_it() {
        let mut s = copy_of_full_issue_as_default();
        assert_eq!(saved(&s).default_flow, "Full issue copy");
        button(&mut s.app, WIDE, dialog::DELETE);
        confirm(&mut s, dialog::DELETE);
        assert_eq!(saved(&s).default_flow, "");
        assert_eq!(dialog(&s.app).default, None);
    }

    #[test]
    fn hiding_the_default_built_in_clears_it() {
        let mut s = settings_tab(false);
        select_flow(&mut s.app, WIDE, "Arena");
        button(&mut s.app, WIDE, dialog::SET_DEFAULT);
        button(&mut s.app, WIDE, dialog::DELETE);
        confirm(&mut s, dialog::DELETE);
        let config = saved(&s);
        assert_eq!(
            (config.default_flow.as_str(), config.hidden_flows.as_slice()),
            ("", ["Arena".to_string()].as_slice())
        );
    }
}

mod pages {
    use super::*;

    #[test]
    fn the_list_with_the_default_tag() {
        let mut s = settings_tab(false);
        select_flow(&mut s.app, WIDE, "Quick");
        button(&mut s.app, WIDE, dialog::SET_DEFAULT);
        insta::assert_snapshot!("the_list_with_the_default_tag", shown(&mut s.app, WIDE));
    }

    #[test]
    fn a_flow_dragged_over_the_list() {
        let mut s = settings_tab(false);
        press_row(&mut s.app, 4, WIDE);
        drag_to(&mut s.app, 1, WIDE);
        insta::assert_snapshot!("a_flow_dragged_over_the_list", shown(&mut s.app, WIDE));
    }
}
