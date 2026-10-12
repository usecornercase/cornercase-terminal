use super::*;

#[test]
fn only_the_panes_of_the_tab_on_screen() {
    let (mut app, _rx) = app();
    let first = app.term().expect("a pane").id;
    app.add_tab(0, 0, AREA).expect("add a tab");
    let second = app.term().expect("the new pane").id;

    assert_eq!((app.shows(first), app.shows(second), app.shows(u64::MAX)), (false, true, false));
}
