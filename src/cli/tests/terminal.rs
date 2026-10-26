use rstest::rstest;

use super::*;

#[test]
fn surface_takes_a_tab_and_a_size() {
    let cli = parse(&["surface", "--tab", "3", "--cols", "120", "--rows", "40"]).expect("parse");

    let Some(Command::Control(Control::Surface(surface))) = cli.command else { panic!("not surface") };
    assert_eq!((surface.tab, surface.cols, surface.rows), (Some(3), Some(120), Some(40)));
}

#[rstest]
#[case::a_tab(&["surface", "--tab", "3", "--cols", "120", "--rows", "40", "--patches"])]
#[case::a_view(&["surface", "--view", "4", "--patches"])]
fn surface_can_ask_for_patches(#[case] args: &[&str]) {
    let cli = parse(args).expect("parse");

    let Some(Command::Control(Control::Surface(surface))) = cli.command else { panic!("not surface") };
    assert!(surface.patches);
}

#[test]
fn redraw_names_its_surface() {
    let cli = parse(&["redraw", "--surface", "4"]).expect("parse");

    let Some(Command::Control(Control::Redraw(redraw))) = cli.command else { panic!("not redraw") };
    assert_eq!(redraw.surface, 4);
}

#[test]
fn input_keeps_its_events_in_order() {
    let cli = parse(&["input", "--pane", "3", r#"{"text":"x"}"#, r#"{"key":{"code":"enter"}}"#]).expect("parse");

    let Some(Command::Control(Control::Input(input))) = cli.command else { panic!("not input") };
    assert_eq!(
        (input.pane, input.events),
        (Some(3), vec![r#"{"text":"x"}"#.to_string(), r#"{"key":{"code":"enter"}}"#.to_string()])
    );
}

#[test]
fn input_can_type_into_the_popup_instead_of_a_pane() {
    let cli = parse(&["input", "--popup", r#"{"text":"x"}"#]).expect("parse");

    let Some(Command::Control(Control::Input(input))) = cli.command else { panic!("not input") };
    assert_eq!((input.pane, input.popup), (None, true));
}

#[rstest]
#[case::neither(&["input", r#"{"text":"x"}"#])]
#[case::both(&["input", "--pane", "3", "--popup", r#"{"text":"x"}"#])]
fn input_takes_a_pane_or_the_popup(#[case] args: &[&str]) {
    assert!(parse(args).is_err());
}

#[test]
fn surface_streams_the_popup_without_a_tab_or_a_size() {
    let cli = parse(&["surface", "--popup"]).expect("parse");

    let Some(Command::Control(Control::Surface(surface))) = cli.command else { panic!("not surface") };
    assert!(surface.popup);
}

#[test]
fn surface_takes_no_tab_with_the_popup() {
    assert!(parse(&["surface", "--popup", "--tab", "3", "--cols", "80", "--rows", "24"]).is_err());
}
