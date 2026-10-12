use super::*;

fn entry(name: &str, branch: Option<&str>) -> Entry {
    Entry { name: name.into(), branch: branch.map(str::to_string) }
}

fn picker(selected: Option<usize>) -> Picker {
    Picker {
        title: "New workspace",
        path: "~/projects/".into(),
        filter: String::new(),
        items: vec![entry("..", None), entry("cornercase", Some("main")), entry("notes", None)],
        selected,
        scroll: 0,
        hint: "enter opens ~/projects".into(),
        error: None,
        submit: "open",
        empty: "no folders here",
    }
}

fn with(picker: Picker) -> View<'static> {
    View { overlay: Some(Overlay::Picker(picker)), ..view(&["cornercase"]) }
}

#[test]
fn is_drawn_where_its_area_says() {
    assert_eq!(corners_of_its_area(&with(picker(None))), (String::from("╭"), String::from("╯")));
}

fn item(i: usize) -> Position {
    picker_item(picker_area(AREA), 3, 0, i).as_position()
}

#[test]
fn renders_the_folders_with_their_branch() {
    insta::assert_snapshot!(render(&with(picker(None))).backend());
}

#[test]
fn says_when_nothing_matches() {
    let p = Picker { filter: "zzz".into(), items: Vec::new(), hint: String::new(), ..picker(None) };
    insta::assert_snapshot!(render(&with(p)).backend());
}

#[test]
fn highlights_the_selected_folder() {
    assert_eq!(render(&with(picker(Some(1)))).backend().buffer()[item(1)].bg, DARK_SURFACE);
}

#[test]
fn highlights_the_hovered_folder() {
    let v = View { hover: Some(item(2)), ..with(picker(None)) };
    assert_eq!(render(&v).backend().buffer()[item(2)].bg, DARK_HOVER);
}

#[test]
fn puts_the_cursor_after_the_path() {
    let mut t = render(&with(picker(None)));
    let input = picker_rows(picker_area(AREA))[0];
    let expected = input.x + u16::try_from(INPUT_PROMPT.chars().count() + "~/projects/".len()).expect("width");
    assert_eq!(t.get_cursor_position().expect("cursor"), Position::new(expected, input.y));
}

#[rstest]
#[case::folder(item(1), Some(PickerHit::Item(1)))]
#[case::below_the_last_folder(item(2).offset(ratatui::layout::Offset { x: 0, y: 1 }), None)]
#[case::submit(picker_buttons(picker_area(AREA), "open")[0].as_position(), Some(PickerHit::Submit))]
#[case::cancel(picker_buttons(picker_area(AREA), "open")[1].as_position(), Some(PickerHit::Cancel))]
#[case::the_input(picker_rows(picker_area(AREA))[0].as_position(), None)]
fn maps_clicks(#[case] pos: Position, #[case] expected: Option<PickerHit>) {
    assert_eq!(picker_hit(AREA, "open", 3, 0, pos), expected);
}

#[test]
fn scrolled_rows_map_to_later_folders() {
    let list = picker_list(picker_area(AREA));
    let items = usize::from(list.height) + 5;
    assert_eq!(picker_hit(AREA, "open", items, 2, list.as_position()), Some(PickerHit::Item(2)));
}

#[test]
fn scroll_never_leaves_empty_rows_at_the_bottom() {
    let list = picker_list(picker_area(AREA));
    let items = usize::from(list.height) + 5;
    assert_eq!(picker_hit(AREA, "open", items, 100, list.as_position()), Some(PickerHit::Item(5)));
}

#[test]
fn folders_scrolled_out_have_no_row() {
    assert!(picker_item(picker_area(AREA), 50, 10, 3).is_empty());
}
