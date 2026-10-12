use super::*;

#[test]
fn plain_keys_go_to_the_shell() {
    let (mut app, _rx) = app();
    send_key(&mut app, KeyCode::Char('c'), KeyModifiers::NONE);
    assert_eq!(app.projects.len(), 1);
}

#[test]
fn ctrl_b_has_no_special_meaning() {
    let (mut app, _rx) = app();
    send_key(&mut app, KeyCode::Char('b'), KeyModifiers::CONTROL);
    send_key(&mut app, KeyCode::Char('c'), KeyModifiers::NONE);
    assert_eq!(app.projects.len(), 1);
}
