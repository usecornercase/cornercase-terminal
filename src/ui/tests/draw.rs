use super::*;

#[test]
fn renders_sidebar_with_active_entry() {
    let v = View { active: 1, ..view(&["cornercase", "tmp"]) };
    insta::assert_snapshot!(render(&v).backend());
}

#[test]
fn shows_the_workspace_count_next_to_the_name() {
    let mut v = View { active: 1, ..view(&["cornercase", "api", "tmp"]) };
    v.projects[0].workspaces = 3;
    v.projects[2].workspaces = 0;
    insta::assert_snapshot!(render(&v).backend());
}

#[test]
fn a_long_name_is_cut_before_the_count() {
    let v = View {
        projects: vec![ProjectEntry { name: "a".repeat(40), workspaces: 12, group: None, status: None }],
        ..view(&[])
    };
    let row: String =
        (0..list().width).map(|x| render(&v).backend().buffer()[(x, list().y)].symbol().to_string()).collect();
    assert!(row.contains("aaa… (12)"), "{row:?}");
}

#[test]
fn truncates_long_names() {
    let v = view(&["a-folder-with-a-really-long-name-that-does-not-fit"]);
    insta::assert_snapshot!(render(&v).backend());
}

#[test]
fn without_projects_the_pane_shows_the_logo() {
    insta::assert_snapshot!(render_sized(&view(&[]), W, 24).backend());
}

#[test]
fn a_pane_too_short_for_the_logo_keeps_the_words() {
    let t = render_sized(&view(&[]), W, 8);
    let text: String = t.backend().buffer().content().iter().map(ratatui::buffer::Cell::symbol).collect();
    assert_eq!((text.contains(TAGLINE), text.contains('█')), (true, false));
}

#[test]
fn renders_active_screen_in_pane() {
    let snap = screen(b"$ echo hello\r\nhello\r\n$ ");
    let v = View { tab: Some(single(snap)), ..view(&["~"]) };
    insta::assert_snapshot!(render(&v).backend());
}

#[test]
fn places_cursor_where_the_inner_terminal_has_it() {
    let snap = screen(b"$ echo hello\r\nhello\r\n$ ");
    let v = View { tab: Some(single(snap)), ..view(&["~"]) };
    let mut t = render(&v);
    assert_eq!(t.get_cursor_position().expect("cursor"), Position::new(areas().pane.x + 2, 2));
}
