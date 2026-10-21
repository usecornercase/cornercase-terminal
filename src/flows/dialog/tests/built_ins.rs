use super::*;

fn arena() -> Flow {
    builtin::flows(&Known::ALL, Some(Known::Claude)).into_iter().find(|flow| flow.name == "Arena").expect("arena")
}

fn with_deleted() -> Dialog {
    let mut d = dialog();
    d.hidden = vec![arena()];
    d
}

fn toggle() -> usize {
    6
}

fn settings_dialog() -> Dialog {
    let mut d = dialog();
    d.in_settings = true;
    d
}

#[test]
fn deleting_a_built_in_asks_then_hides_it() {
    let mut d = dialog();
    assert_eq!(d.press(DELETE), Action::None);
    assert_eq!(d.confirm, Some(Confirm::Hide(0)));
    assert_eq!(d.confirm_text().map(|(title, ..)| title), Some("Delete flow"));
    assert_eq!(d.confirmed(), Action::Hide { name: "Full issue".into(), file: None });
}

#[test]
fn deleting_an_edited_built_in_deletes_your_file_and_hides_it() {
    for via_key in [false, true] {
        let mut d = dialog();
        d.flows[mine()].flow.name = "Full issue".into();
        d.select(mine());
        if via_key {
            key(&mut d, KeyCode::Delete);
        } else {
            d.press(DELETE);
        }
        assert_eq!(
            d.confirmed(),
            Action::Hide { name: "Full issue".into(), file: Some(PathBuf::from("/flows/mine.toml")) }
        );
    }
}

#[test]
fn the_deleted_entry_comes_last_and_folds() {
    let mut d = with_deleted();
    let row = d.list_rows().pop().expect("a row");
    assert_eq!((d.entries(), row.label.as_str(), row.muted), (toggle() + 1, "▸ deleted built-ins (1)", true));
    d.select(toggle());
    assert_eq!(d.buttons(ROOM), [IMPORT, NEW, DONE]);
    key(&mut d, KeyCode::Enter);
    let rows = d.list_rows();
    assert_eq!((d.entries(), rows[toggle()].label.as_str()), (toggle() + 2, "▾ deleted built-ins (1)"));
    let deleted = &rows[toggle() + 1];
    assert_eq!(
        (deleted.label.as_str(), deleted.muted, deleted.note.starts_with("deleted · ")),
        ("  Arena", true, true)
    );
    d.select(toggle() + 1);
    assert!(matches!(d.preview(), Some(Preview::Flow(flow)) if flow.name == "Arena"));
    assert_eq!(d.buttons(ROOM), [RESTORE_BUILT_IN, IMPORT, NEW, DONE]);
    assert_eq!(
        [key(&mut d, KeyCode::Enter), d.press(RESTORE_BUILT_IN)],
        [Action::Unhide("Arena".into()), Action::Unhide("Arena".into())]
    );
}

#[test]
fn a_click_on_the_deleted_entry_folds_it_and_a_second_click_on_a_deleted_flow_restores_it() {
    let mut d = with_deleted();
    d.hit(Hit::Row(toggle()), ROOM);
    assert_eq!(d.entries(), toggle() + 2);
    d.hit(Hit::Row(toggle() + 1), ROOM);
    assert_eq!(d.hit(Hit::Row(toggle() + 1), ROOM), Action::Unhide("Arena".into()));
}

#[test]
fn the_deleted_entry_explains_itself_in_place_of_a_diagram() {
    let mut d = with_deleted();
    d.select(toggle());
    let Some(Preview::Note(note)) = d.preview() else { panic!("not a note") };
    assert!(note.starts_with("1 built-in flow is deleted"), "{note}");
}

#[test]
fn the_list_forgets_the_deleted_entry_once_nothing_is_deleted() {
    let mut d = with_deleted();
    d.select(toggle());
    d.toggle_deleted();
    d.set_listing(Listing::default());
    assert!(!d.show_hidden);
    assert_eq!(d.entries(), 0);
}

#[test]
fn tab_and_the_arrows_step_through_the_settings_tabs() {
    let mut d = settings_dialog();
    assert_eq!(key(&mut d, KeyCode::Tab), Action::StepTab(1));
    assert_eq!(key(&mut d, KeyCode::Right), Action::StepTab(1));
    assert_eq!(key(&mut d, KeyCode::BackTab), Action::StepTab(-1));
    assert_eq!(key(&mut d, KeyCode::Left), Action::StepTab(-1));
    assert_eq!(key(&mut d, KeyCode::Down), Action::None);
    assert_eq!(d.cursor, 1);
}

#[test]
fn outside_the_settings_tab_moves_between_flows() {
    let mut d = dialog();
    assert_eq!(key(&mut d, KeyCode::Tab), Action::None);
    assert_eq!(d.cursor, 1);
}

#[test]
fn a_click_on_a_settings_tab_is_passed_on() {
    let mut d = settings_dialog();
    assert_eq!(d.hit(Hit::Tab(4), ROOM), Action::Tab(4));
    assert_eq!(dialog().hit(Hit::Tab(4), ROOM), Action::None);
}

#[test]
fn the_list_is_titled_settings_and_ends_with_share_restart_and_done() {
    let mut d = settings_dialog();
    assert_eq!(d.title(), "Settings");
    assert!(d.settings_tabs());
    assert_eq!(
        d.buttons(ROOM),
        [EDIT, DUPLICATE, EXPORT, DELETE, IMPORT, NEW, crate::settings::SHARE, crate::settings::RESTART, DONE]
    );
    assert_eq!(d.press(crate::settings::RESTART), Action::Restart);
    assert_eq!(d.press(crate::settings::SHARE), Action::Share);
    d.open(mine());
    assert_eq!(d.title(), "Flows › Mine");
    assert!(!d.settings_tabs());
}

#[test]
fn a_narrow_row_keeps_delete_restore_default_and_done() {
    let mut d = settings_dialog();
    d.flows[mine()].flow.name = "Full issue".into();
    d.select(mine());
    let buttons = d.buttons(Room { wide: false, width: 82, ..ROOM });
    assert_eq!(buttons.last(), Some(&DONE));
    assert!(buttons.contains(&DELETE) && buttons.contains(&RESTORE));
    assert!(row_width(&buttons) <= 82, "{buttons:?}");
}
