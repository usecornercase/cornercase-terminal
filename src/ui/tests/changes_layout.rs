use super::*;

const BIG: Rect = Rect { x: 0, y: 0, width: 160, height: 30 };

#[test]
fn the_panel_sits_right_of_the_pane() {
    let a = layout_with(BIG, Widths::default(), true, Sidebar::SideBySide);
    assert_eq!(
        (a.changes_border.x, a.changes.x, a.changes.right(), a.pane.right()),
        (a.pane.right(), a.pane.right() + 1, BIG.right(), BIG.width - Widths::default().changes_width(160))
    );
}

#[test]
fn the_panel_border_drags() {
    let a = layout_with(BIG, Widths::default(), true, Sidebar::SideBySide);
    assert_eq!(a.border_hit(a.changes_border.as_position()), Some(Border::Changes));
}

#[test]
fn a_compact_panel_covers_the_screen_below_the_bar() {
    let small = Rect { width: 80, ..BIG };
    let a = layout_with(small, Widths::default(), true, Sidebar::SideBySide);
    assert_eq!(
        (a.changes, a.changes_button.right(), a.files_button.right(), a.todo_button.right()),
        (a.pane, a.files_button.x, a.todo_button.x, a.search_button.x)
    );
}

fn with_panel() -> View<'static> {
    let tints = crate::changes::Tints::of(&HostTheme::default());
    let panel = changes::View {
        scope: crate::changes::Scope::Project,
        project: "shop".into(),
        owners: Vec::new(),
        headers: Vec::new(),
        mode: crate::changes::Mode::Uncommitted,
        base: None,
        body: changes::Body::Loading,
        folded: Vec::new(),
        viewed: Vec::new(),
        gaps: HashMap::new(),
        scroll: 0,
        live: false,
        light: false,
        muted: Color::DarkGray,
        tints,
        filter: None,
    };
    View {
        has_project: true,
        issues: true,
        workspaces: vec![WorkspaceEntry { name: "main".into(), tabs: vec!["zsh".into()], behind: 0, removing: false }],
        active_tab: Some(0),
        changes: Some(panel),
        changes_button: Some(ChangesButton { label: "Changes 3".into(), open: true }),
        ..view(&["shop"])
    }
}

#[test]
fn draws_the_panel_and_its_button() {
    insta::assert_snapshot!(render_sized(&with_panel(), 140, 16).backend());
}

#[test]
fn hovering_the_changes_button_leaves_issues_unlit() {
    let r = changes_button(layout(Rect::new(0, 0, 140, 16), Widths::default()).issues, "Changes 3");
    let t = render_sized(&View { hover: Some(r.as_position()), ..with_panel() }, 140, 16);
    let issues_x = layout(Rect::new(0, 0, 140, 16), Widths::default()).issues.x + 2;
    let buffer = t.backend().buffer();
    assert_eq!((buffer[(issues_x, r.y)].bg, buffer[(r.x + 1, r.y)].bg), (Color::Reset, Color::Cyan));
}

fn with_todo() -> View<'static> {
    let item = |id: u64, text: &str, done: bool| todo::Item { id, text: text.into(), done, editing: None };
    let panel = todo::View {
        items: vec![
            item(1, "fix the login bug in safari", false),
            item(2, "renew the domain", false),
            item(3, "write docs", true),
        ],
        scroll: 0,
        adding: None,
        light: false,
        muted: Color::DarkGray,
        drag: None,
    };
    View { changes: None, todo: Some(panel), todo_pending: true, ..with_panel() }
}

fn pending(pending: bool, dot: bool) -> View<'static> {
    View { changes: None, todo_pending: pending, todo_dot: dot, ..with_panel() }
}

#[test]
fn todo_shows_a_dot_while_items_are_pending() {
    insta::assert_snapshot!(render_sized(&pending(true, true), 140, 18).backend());
}

#[test]
fn todo_shows_no_dot_when_nothing_is_pending() {
    insta::assert_snapshot!(render_sized(&pending(false, true), 140, 18).backend());
}

#[test]
fn todo_shows_no_dot_when_it_is_switched_off() {
    insta::assert_snapshot!(render_sized(&pending(true, false), 140, 18).backend());
}

#[test]
fn the_compact_todo_button_shows_a_dot_while_items_are_pending() {
    insta::assert_snapshot!(render_sized(&pending(true, true), 80, 18).backend());
}

#[test]
fn the_compact_todo_button_shows_no_dot_when_nothing_is_pending() {
    insta::assert_snapshot!(render_sized(&pending(false, true), 80, 18).backend());
}

#[test]
fn the_dot_is_muted_and_left_of_the_button() {
    let a = layout(Rect::new(0, 0, 140, 18), Widths::default());
    let r = a.todo_button;
    let with = render_sized(&pending(true, true), 140, 18);
    let without = render_sized(&pending(false, true), 140, 18);
    let row = Rect { x: r.x - 2, width: r.width + 2, ..r };
    let dot = with.backend().buffer()[(r.x - 1, r.y)].clone();
    assert_eq!(
        (row_text(&with, row), row_text(&without, row), dot.fg),
        (" ● TODO ".to_string(), "   TODO ".to_string(), Color::DarkGray)
    );
}

#[test]
fn the_button_stays_right_aligned_with_changes() {
    let a = layout(Rect::new(0, 0, 140, 18), Widths::default());
    assert_eq!((a.todo_button.right(), a.todo_button.width), (changes_button(a.issues, "Changes").right(), 6));
}

#[test]
fn a_narrow_footer_leaves_out_the_dot() {
    let widths = Widths { workspaces: MIN_COLUMN_WIDTH, ..Widths::default() };
    let a = layout(Rect::new(0, 0, 140, 18), widths);
    assert_eq!(a.todo_dot(), None);
}

#[test]
fn draws_the_todo_panel_and_its_button() {
    insta::assert_snapshot!(render_sized(&with_todo(), 140, 18).backend());
}

#[test]
fn a_compact_todo_panel_covers_the_screen_below_the_bar() {
    insta::assert_snapshot!(render_sized(&with_todo(), 80, 18).backend());
}

#[test]
fn the_bar_has_a_changes_button_in_compact_mode() {
    let v = View {
        changes: None,
        changes_button: Some(ChangesButton { label: "Changes".into(), open: false }),
        ..with_panel()
    };
    let t = render_sized(&v, 80, 16);
    let r = layout(Rect::new(0, 0, 80, 16), Widths::default()).changes_button;
    assert_eq!(row_text(&t, Rect { y: r.y + 1, height: 1, ..r }).trim(), "±");
}
