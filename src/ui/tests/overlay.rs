use super::*;
use crate::editor::Editor;

fn form(note: Option<Note>) -> Form {
    Form {
        title: "New worktree",
        label: "branch",
        value: Editor::new("feat/login"),
        hint: "in ~/.cornercase/worktrees/cornercase/feat-login".into(),
        toggle: None,
        note,
        submit: "create",
    }
}

fn with(overlay: Overlay) -> View<'static> {
    View { overlay: Some(overlay), ..view(&["cornercase"]) }
}

#[test]
fn renders_the_menu_where_it_was_opened() {
    let v = with(Overlay::Menu { at: Position::new(4, 4), items: vec!["new worktree".into()], disabled: Vec::new() });
    insta::assert_snapshot!(render(&v).backend());
}

#[test]
fn menu_stays_inside_the_screen() {
    let menu = menu_area(AREA, Position::new(W - 1, H - 1), &["new worktree"]);
    assert_eq!((menu.right(), menu.bottom()), (W, H));
}

#[test]
fn menu_item_is_highlighted_on_hover() {
    let at = Position::new(4, 4);
    let item = menu_item(menu_area(AREA, at, &["new worktree"]), 0).as_position();
    let v = View {
        hover: Some(item),
        ..with(Overlay::Menu { at, items: vec!["new worktree".into()], disabled: Vec::new() })
    };
    assert_eq!(render(&v).backend().buffer()[item].bg, DARK_SURFACE);
}

#[test]
fn a_disabled_item_is_dim_and_does_not_light_on_hover() {
    let at = Position::new(4, 4);
    let labels = ["  home", "stop vim first"];
    let menu = menu_area(AREA, at, &labels);
    let (on, off) = (menu_item(menu, 0).as_position(), menu_item(menu, 1));
    let items = labels.iter().map(ToString::to_string).collect();
    let v = View { hover: Some(off.as_position()), ..with(Overlay::Menu { at, items, disabled: vec![false, true] }) };
    let t = render(&v);
    let text = Position::new(off.x + 2, off.y);
    assert!(t.backend().buffer()[text].modifier.contains(Modifier::DIM));
    assert_ne!(t.backend().buffer()[off.as_position()].bg, DARK_SURFACE);
    assert!(!t.backend().buffer()[Position::new(on.x + 2, on.y)].modifier.contains(Modifier::DIM));
}

#[test]
fn a_menu_leaves_what_is_behind_it_bright() {
    let at = Position::new(4, 4);
    let t = render(&with(Overlay::Menu { at, items: vec!["new worktree".into()], disabled: Vec::new() }));
    assert!(!t.backend().buffer()[areas().title.as_position()].modifier.contains(Modifier::DIM));
}

fn keys_menu() -> keys::Keys {
    keys::Keys {
        title: "Keys".into(),
        items: crate::shortcuts::items(None).iter().map(keys::Item::from).collect(),
        hint: "esc closes · ctrl+] twice types it in the pane".into(),
    }
}

fn keys_entry(label: &str) -> Position {
    let menu = keys_menu();
    let i = menu.items.iter().position(|item| item.label == label).expect("the entry");
    keys::item(keys::area(keys::frame(areas().pane, AREA, &menu), &menu), &menu, i).as_position()
}

#[test]
fn renders_the_keys_menu_at_the_bottom_of_the_pane() {
    insta::assert_snapshot!(render(&with(Overlay::Keys(keys_menu()))).backend());
}

#[test]
fn a_keys_entry_is_highlighted_on_hover() {
    let entry = keys_entry("new tab");
    let v = View { hover: Some(entry), ..with(Overlay::Keys(keys_menu())) };
    assert_eq!(render(&v).backend().buffer()[entry].bg, DARK_SURFACE);
}

#[test]
fn a_keys_entry_that_names_several_keys_is_not_highlighted() {
    let entry = keys_entry("go to tab");
    let v = View { hover: Some(entry), ..with(Overlay::Keys(keys_menu())) };
    assert_eq!(render(&v).backend().buffer()[entry].bg, Color::Reset);
}

#[test]
fn the_keys_menu_leaves_what_is_behind_it_bright() {
    let t = render(&with(Overlay::Keys(keys_menu())));
    assert!(!t.backend().buffer()[areas().title.as_position()].modifier.contains(Modifier::DIM));
}

#[test]
fn buttons_behind_the_menu_are_not_highlighted() {
    let at = Position::new(4, list().y);
    let new = new_project_button(list(), 1, &plain(1));
    let over_new = Position::new(menu_area(AREA, at, &["new worktree"]).x + 2, new.y);
    let v = View {
        hover: Some(over_new),
        ..with(Overlay::Menu { at, items: vec!["new worktree".into()], disabled: Vec::new() })
    };
    assert_eq!(render(&v).backend().buffer()[new.as_position()].bg, Color::Reset);
}

#[test]
fn sidebar_buttons_are_not_highlighted_behind_a_form() {
    let more = areas().more.as_position();
    let v = View { hover: Some(more), ..with(Overlay::Form(form(None))) };
    assert_eq!(render(&v).backend().buffer()[more].bg, Color::Reset);
}

#[test]
fn renders_a_form() {
    insta::assert_snapshot!(render(&with(Overlay::Form(form(None)))).backend());
}

#[test]
fn renders_a_form_error() {
    let v = with(Overlay::Form(form(Some(Note::Error("a branch named 'feat/login' already exists".into())))));
    insta::assert_snapshot!(render(&v).backend());
}

#[test]
fn puts_the_cursor_after_the_input() {
    let mut t = render(&with(Overlay::Form(form(None))));
    let input = form_rows(form_area(AREA))[1];
    let expected = input.x + u16::try_from(INPUT_PROMPT.chars().count() + "feat/login".len()).expect("width");
    assert_eq!(t.get_cursor_position().expect("cursor"), Position::new(expected, input.y));
}

#[test]
fn hides_the_pane_cursor_behind_an_overlay() {
    let snap = screen(b"$ ");
    let v = View { tab: Some(single(snap)), ..with(Overlay::Form(form(Some(Note::Busy("creating…"))))) };
    assert!(!render(&v).backend().cursor_visible());
}

#[rstest]
#[case::submit(0, Some(FormHit::Submit))]
#[case::cancel(1, Some(FormHit::Cancel))]
fn buttons_are_hit(#[case] which: usize, #[case] expected: Option<FormHit>) {
    let pos = form_buttons(form_area(AREA), "create")[which].as_position();
    assert_eq!(form_hit(AREA, "create", None, pos), expected);
}

#[test]
fn the_extra_button_sits_left_of_the_submit_one_and_is_hit() {
    let form = form_area(AREA);
    let [submit, _] = form_buttons(form, "keep");
    let extra = form_extra_button(form, "keep", "close all");
    let hit = form_hit(AREA, "keep", Some("close all"), extra.as_position());
    assert_eq!((extra.right() + 1, extra.y, hit), (submit.x, submit.y, Some(FormHit::Extra)));
}

#[test]
fn a_narrow_form_never_lays_the_extra_button_over_the_submit_one() {
    let narrow = Rect::new(0, 0, 36, 20);
    let form = form_area(narrow);
    let [submit, _] = form_buttons(form, "keep projects");
    let extra = form_extra_button(form, "keep projects", "close projects");
    assert!(extra.right() < submit.x || extra.is_empty());
}

#[test]
fn without_an_extra_button_its_place_is_not_hit() {
    let extra = form_extra_button(form_area(AREA), "keep", "close all");
    assert_eq!(form_hit(AREA, "keep", None, extra.as_position()), None);
}

#[test]
fn the_rest_of_the_form_is_not_a_button() {
    assert_eq!(form_hit(AREA, "create", None, form_area(AREA).as_position()), None);
}
