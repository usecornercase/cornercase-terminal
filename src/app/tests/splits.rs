use super::*;

fn two_pane_positions() -> (App, Receiver<AppEvent>, u64, u64, Vec<Rect>) {
    let (app, rx) = split_right();
    let [left, right] = ids(&app)[..] else { panic!("two panes") };
    let r = rects(&app);
    (app, rx, left, right, r)
}

fn pane() -> Rect {
    areas().pane
}

fn inside(r: Rect) -> Position {
    Position::new(r.x + 1, r.y + 1)
}

fn tab(app: &App) -> &Tab {
    app.tab().expect("an active tab")
}

fn rects(app: &App) -> Vec<Rect> {
    tab(app).layout.panes(pane()).into_iter().map(|(_, r)| r).collect()
}

fn split(app: &mut App, at: Position, label: &str) {
    right_click(app, at);
    pick(app, label);
}

fn split_right() -> (App, Receiver<AppEvent>) {
    let (mut app, rx) = app();
    split(&mut app, inside(pane()), "split right");
    (app, rx)
}

fn divider(app: &App) -> split::Divider {
    tab(app).layout.dividers(pane()).remove(0)
}

#[test]
fn hovering_a_divider_asks_for_a_resize_pointer_across_it() {
    let (mut right, _rx) = split_right();
    let (mut down, _rx2) = app();
    split(&mut down, inside(pane()), "split down");
    let line = |app: &App| {
        let r = divider(app).line;
        Position::new(r.x, r.y + r.height / 2)
    };

    let on = |app: &mut App| {
        let at = line(app);
        mouse(app, MouseEventKind::Moved, at);
        app.pointer(AREA)
    };

    assert_eq!((on(&mut right), on(&mut down)), (pointer::Shape::Columns, pointer::Shape::Rows));
}

#[test]
fn the_pointer_follows_a_divider_drag_until_release() {
    let (mut app, _rx) = split_right();
    let from = divider(&app).line.as_position();
    press(&mut app, from);
    let away = Position::new(pane().x, from.y + 3);
    let dragging = pointer_after_drag(&mut app, away);

    assert_eq!((dragging, app.pointer(AREA)), (pointer::Shape::Columns, pointer::Shape::Default));
}

#[test]
fn esc_ends_a_divider_drag() {
    let (mut app, _rx) = split_right();
    let from = divider(&app).line.as_position();
    press(&mut app, from);
    mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), Position::new(pane().x, from.y));

    send_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);

    assert_eq!((app.drag.is_none(), app.pointer(AREA)), (true, pointer::Shape::Default));
}

#[test]
fn a_right_click_in_a_pane_opens_its_menu() {
    let (mut app, _rx) = app();
    right_click(&mut app, inside(pane()));
    assert_eq!(menu_labels(&app), ["split right", "split down", "send right-clicks to the pane", "close pane"]);
}

#[test]
fn split_right_puts_a_new_pane_beside_it() {
    let (app, _rx) = split_right();
    let room = pane().width - 2;
    let left = room.div_ceil(2);
    assert_eq!(
        rects(&app),
        [Rect { width: left, ..pane() }, Rect { x: pane().x + left + 2, width: room - left, ..pane() }]
    );
}

#[test]
fn split_down_puts_a_new_pane_below_it() {
    let (mut app, _rx) = app();
    split(&mut app, inside(pane()), "split down");
    let room = pane().height - 1;
    let top = room.div_ceil(2);
    assert_eq!(
        rects(&app),
        [Rect { height: top, ..pane() }, Rect { y: pane().y + top + 1, height: room - top, ..pane() }]
    );
}

#[test]
fn the_new_pane_becomes_the_active_one() {
    let (app, _rx) = split_right();
    assert_eq!(tab(&app).active, 1);
}

#[test]
fn each_terminal_gets_the_size_of_its_pane() {
    let (app, _rx) = split_right();
    let sizes: Vec<(u16, u16)> = tab(&app).panes.iter().map(|t| t.emulator.size().expect("size")).collect();
    let expected: Vec<(u16, u16)> = rects(&app).iter().map(|r| (r.height, r.width)).collect();
    assert_eq!(sizes, expected);
}

#[test]
fn the_new_pane_opens_in_the_folder_of_the_split_one() {
    let (mut app, _rx, dirs) = app_with(1);
    split(&mut app, inside(pane()), "split right");
    let new = tab(&app).panes[1].cwd();
    assert_eq!(new, Some(canonical(&dirs[0])));
}

#[test]
fn a_split_that_would_not_fit_is_not_offered() {
    let (mut app, _rx) = split_right();
    let at = inside(rects(&app)[1]);
    right_click(&mut app, at);
    assert_eq!(menu_labels(&app), ["split down", "send right-clicks to the pane", "close pane"]);
}

#[test]
fn a_click_on_another_pane_makes_it_active() {
    let (mut app, _rx) = split_right();
    let at = inside(rects(&app)[0]);
    click(&mut app, at);
    assert_eq!((tab(&app).active, app.drag), (0, None));
}

#[test]
fn a_split_tab_keeps_the_name_of_its_first_pane_whatever_has_the_focus() {
    let (mut app, rx) = split_right();
    type_line(&mut app, "sleep 30");
    pump_until(&mut app, &rx, "sleep runs", |a| {
        a.term().and_then(|t| t.program(&a.config)).as_deref() == Some("sleep")
    });
    let first = tab(&app).label(&app.config);

    let left = inside(rects(&app)[0]);
    click(&mut app, left);

    assert!(is_sh(&first) && tab(&app).label(&app.config) == first, "{first}");
}

#[test]
fn a_split_tab_counts_its_other_panes() {
    let (mut app, _rx) = split_right();
    let left = inside(rects(&app)[0]);
    split(&mut app, left, "split down");
    assert_eq!(app.tab_entry(tab(&app)).others, 2);
}

fn pane_text(app: &mut App, i: usize) -> String {
    let term = &mut app.tab_mut().expect("a tab").panes[i];
    term.emulator.snapshot().map(|s| s.contents()).unwrap_or_default()
}

#[test]
fn keys_go_to_the_active_pane() {
    let (mut app, rx) = split_right();
    type_line(&mut app, "echo split-\"\"works");
    wait_until("the new pane runs the command", || {
        drain(&mut app, &rx);
        pane_text(&mut app, 1).contains("split-works")
    });
    assert!(!pane_text(&mut app, 0).contains("split-works"));
}

#[test]
fn closing_a_pane_gives_its_space_back() {
    let (mut app, rx) = split_right();
    let first = tab(&app).panes[0].id;
    let at = inside(rects(&app)[1]);
    split(&mut app, at, "close pane");
    pump_until(&mut app, &rx, "the pane is gone", |app| tab(app).panes.len() == 1);
    assert_eq!((&tab(&app).layout, tab(&app).active), (&split::Node::Leaf(first), 0));
}

#[test]
fn exiting_the_shell_closes_its_pane() {
    let (mut app, rx) = split_right();
    type_line(&mut app, "exit");
    pump_until(&mut app, &rx, "the pane is gone", |app| tab(app).panes.len() == 1);
    assert_eq!(rects(&app), [pane()]);
}

#[test]
fn a_program_that_wants_the_mouse_still_gets_the_menu() {
    let (mut app, _rx) = app();
    app.term_mut().expect("a pane").feed(b"\x1b[?1000h");
    right_click(&mut app, inside(pane()));
    assert_eq!(menu_labels(&app).last().map(String::as_str), Some(PaneAction::Close.label()));
}

#[test]
fn right_clicks_can_go_to_the_program_instead() {
    let (mut app, _rx) = app();
    app.term_mut().expect("a pane").feed(b"\x1b[?1000h");
    split(&mut app, inside(pane()), "send right-clicks to the pane");

    right_click(&mut app, inside(pane()));

    assert!(app.overlay.is_none());
}

#[test]
fn the_menu_offers_the_way_back() {
    let (mut app, _rx) = app();
    app.term_mut().expect("a pane").feed(b"\x1b[?1000h");
    split(&mut app, inside(pane()), "send right-clicks to the pane");
    app.term_mut().expect("a pane").feed(b"\x1b[?1000l");

    right_click(&mut app, inside(pane()));

    assert!(menu_labels(&app).iter().any(|l| l == "use this menu on right-click"));
}

fn drag_pane(app: &mut App, from: Position, to: Position) {
    mouse_down(app, MouseButton::Right, from);
    mouse(app, MouseEventKind::Drag(MouseButton::Right), to);
    mouse(app, MouseEventKind::Up(MouseButton::Right), to);
}

fn middle(r: Rect) -> Position {
    Position::new(r.x + r.width / 2, r.y + r.height / 2)
}

fn bottom(r: Rect) -> Position {
    Position::new(r.x + r.width / 2, r.bottom() - 1)
}

fn ids(app: &App) -> Vec<u64> {
    tab(app).panes.iter().map(|t| t.id).collect()
}

#[test]
fn a_pane_dragged_with_the_right_button_onto_the_bottom_of_another_goes_below_it() {
    let (mut app, _rx, left, right, r) = two_pane_positions();

    drag_pane(&mut app, inside(r[0]), bottom(r[1]));

    let mut expected = split::Node::Leaf(right);
    expected.split(right, Dir::Down, left);
    assert_eq!((&tab(&app).layout, tab(&app).active, app.overlay.is_none()), (&expected, 0, true));
}

#[test]
fn moved_panes_keep_their_shells_and_get_their_new_size() {
    let (mut app, _rx) = split_right();
    let before = ids(&app);
    let r = rects(&app);

    drag_pane(&mut app, inside(r[0]), bottom(r[1]));

    let sizes: Vec<(u64, (u16, u16))> =
        tab(&app).panes.iter().map(|t| (t.id, t.emulator.size().expect("size"))).collect();
    let mut expected: Vec<(u64, (u16, u16))> =
        tab(&app).layout.panes(pane()).into_iter().map(|(id, r)| (id, (r.height, r.width))).collect();
    expected.sort_by_key(|(id, _)| before.iter().position(|b| b == id));
    assert_eq!((ids(&app), sizes), (before, expected));
}

#[test]
fn a_pane_dropped_in_the_middle_of_another_swaps_with_it() {
    let (mut app, _rx, left, right, r) = two_pane_positions();

    drag_pane(&mut app, inside(r[0]), middle(r[1]));

    assert_eq!(tab(&app).layout.ids(), [right, left]);
}

#[test]
fn the_landing_shows_while_dragging() {
    let (mut app, _rx) = split_right();
    let r = rects(&app);

    mouse_down(&mut app, MouseButton::Right, inside(r[0]));
    mouse(&mut app, MouseEventKind::Drag(MouseButton::Right), middle(r[1]));

    assert_eq!(app.pane_landing(pane()), Some((r[1], split::Place::Swap)));
}

#[test]
fn esc_cancels_a_pane_drag() {
    let (mut app, _rx) = split_right();
    let layout = tab(&app).layout.clone();
    let r = rects(&app);
    let to = bottom(r[1]);

    mouse_down(&mut app, MouseButton::Right, inside(r[0]));
    mouse(&mut app, MouseEventKind::Drag(MouseButton::Right), to);
    send_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    mouse(&mut app, MouseEventKind::Up(MouseButton::Right), to);

    assert_eq!((&tab(&app).layout, app.overlay.is_none()), (&layout, true));
}

#[test]
fn a_drag_back_onto_the_same_pane_changes_nothing_and_opens_no_menu() {
    let (mut app, _rx) = split_right();
    let layout = tab(&app).layout.clone();
    let r = rects(&app);
    let from = inside(r[0]);

    mouse_down(&mut app, MouseButton::Right, from);
    mouse(&mut app, MouseEventKind::Drag(MouseButton::Right), middle(r[1]));
    mouse(&mut app, MouseEventKind::Drag(MouseButton::Right), from);
    mouse(&mut app, MouseEventKind::Up(MouseButton::Right), from);

    assert_eq!((&tab(&app).layout, app.overlay.is_none()), (&layout, true));
}

#[test]
fn a_move_that_would_not_fit_lands_nowhere() {
    let (mut app, _rx) = split_right();
    let r = rects(&app);
    split(&mut app, inside(r[1]), "split down");
    let [_, top, bottom_pane] = ids(&app)[..] else { panic!("three panes") };
    let narrow = Rect { width: 30, ..pane() };
    let right_edge = |area: Rect| {
        let r = tab(&app).rect(area, bottom_pane).expect("the bottom pane");
        Position::new(r.right() - 1, r.y + r.height / 2)
    };

    let fits = tab(&app).landing(pane(), top, right_edge(pane())).map(|l| l.place);
    let squeezed = tab(&app).landing(narrow, top, right_edge(narrow)).map(|l| l.place);

    assert_eq!((fits, squeezed), (Some(split::Place::Right), None));
}

#[test]
fn dragging_the_divider_moves_it() {
    let (mut app, _rx) = split_right();
    let line = divider(&app).line;

    drag(&mut app, Position::new(line.x, line.y + 2), Position::new(line.x - 5, line.y + 2));

    assert_eq!((divider(&app).line.x, app.drag.is_none()), (line.x - 5, true));
}

#[test]
fn the_terminals_follow_the_divider() {
    let (mut app, _rx) = split_right();
    let line = divider(&app).line;
    drag(&mut app, Position::new(line.x, line.y), Position::new(line.x - 5, line.y));

    app.resize(AREA);

    let left = rects(&app)[0];
    assert_eq!(tab(&app).panes[0].emulator.size().expect("size"), (left.height, left.width));
}

#[test]
fn a_double_click_on_the_divider_splits_in_half_again() {
    let (mut app, _rx) = split_right();
    let line = divider(&app).line;
    drag(&mut app, Position::new(line.x, line.y), Position::new(line.x - 5, line.y));
    let moved = divider(&app).line.as_position();

    click(&mut app, moved);
    click(&mut app, moved);

    assert_eq!(divider(&app).line.x, line.x);
}

fn split_down_twice() -> (App, Receiver<AppEvent>) {
    let (mut app, rx) = app();
    split(&mut app, inside(pane()), "split down");
    let bottom = rects(&app)[1];
    split(&mut app, inside(bottom), "split down");
    (app, rx)
}

const LOW: Rect = Rect { height: 8, ..AREA };

fn sizes(app: &App) -> Vec<(u16, u16)> {
    tab(app).panes.iter().map(|t| t.emulator.size().expect("a size")).collect()
}

#[test]
fn dragging_the_outer_divider_to_the_edge_keeps_every_pane() {
    let (mut app, _rx) = split_down_twice();
    let line = divider(&app).line;

    drag(&mut app, Position::new(line.x + 1, line.y), Position::new(line.x + 1, pane().bottom() - 1));

    assert!(rects(&app).iter().all(|r| r.height >= split::MIN_ROWS), "{:?}", rects(&app));
}

#[test]
fn a_client_too_small_for_every_pane_shows_only_the_active_one() {
    let (mut app, _rx) = split_down_twice();
    let before = sizes(&app);
    let area = app.layout(LOW).pane;

    app.resize(LOW);

    let active = tab(&app).pane().expect("an active pane").id;
    assert_eq!(
        (tab(&app).shown(area), sizes(&app)),
        (vec![(active, area)], vec![before[0], before[1], (area.height, area.width)])
    );
}

#[test]
fn a_pane_shown_alone_for_lack_of_room_cannot_be_split() {
    let (mut app, _rx) = split_down_twice();
    let at = inside(app.layout(LOW).pane);

    mouse_in(&mut app, MouseEventKind::Down(MouseButton::Right), at, LOW);
    mouse_in(&mut app, MouseEventKind::Up(MouseButton::Right), at, LOW);

    assert_eq!(menu_labels(&app), ["send right-clicks to the pane", "close pane"]);
}

#[rstest::rstest]
#[case::no_width(Rect { x: 10, y: 5, width: 0, height: 4 })]
#[case::no_height(Rect { x: 10, y: 5, width: 4, height: 0 })]
fn a_pane_with_no_room_has_no_cell_under_the_mouse(#[case] rect: Rect) {
    let ev =
        MouseEvent { kind: MouseEventKind::Drag(MouseButton::Left), column: 12, row: 6, modifiers: KeyModifiers::NONE };

    assert_eq!(pane_cell(rect, ev), None);
}

#[test]
fn a_selection_stops_at_the_edge_of_its_pane() {
    let (mut app, _rx) = split_right();
    let left = rects(&app)[0];
    click(&mut app, inside(left));
    app.tab_mut().expect("a tab").panes[0].feed(b"\x1b[2J\x1b[Hhello world, this line is long");

    drag(&mut app, left.as_position(), Position::new(left.right() + 5, left.y));

    assert_eq!(app.take_host_writes(), [clipboard::osc52("hello world, this li")]);
}

#[test]
fn the_layout_is_saved() {
    let (app, _rx) = split_right();
    let saved = &app.state().projects[0].workspaces[0].tabs[0];
    let half = split::Node::Split {
        dir: Dir::Right,
        ratio: split::HALF,
        first: Box::new(split::Node::Leaf(0)),
        second: Box::new(split::Node::Leaf(1)),
    };
    assert_eq!((saved.panes.len(), saved.layout.as_ref()), (2, Some(&half)));
}

#[test]
fn restore_brings_back_the_layout_and_the_right_clicks() {
    let (mut app, _rx) = split_right();
    let at = inside(rects(&app)[1]);
    split(&mut app, at, "send right-clicks to the pane");
    let at = inside(rects(&app)[0]);
    split(&mut app, at, "split down");
    let saved = app.state();
    let (mut restored, _rx2) = empty_app();

    restored.restore(&saved, AREA);

    let tab = tab(&restored);
    let right_clicks: Vec<bool> = tab.panes.iter().map(|t| tab.right_clicks_to_pane(t.id)).collect();
    assert_eq!(
        (
            tab.layout.panes(pane()).len(),
            restored.state().projects[0].workspaces[0].tabs[0].layout.clone(),
            right_clicks
        ),
        (3, saved.projects[0].workspaces[0].tabs[0].layout.clone(), vec![false, true, false])
    );
}

#[test]
fn a_tab_saved_without_a_layout_puts_its_panes_side_by_side() {
    let dir = TempDir::new();
    let (mut app, _rx) = empty_app();
    let pane_state = PaneState {
        cwd: None,
        right_clicks: false,
        agent: None,
        done: false,
        pending: false,
        id: None,
        parent: None,
        collapsed: false,
        accounts: std::collections::BTreeMap::new(),
    };
    let tab = TabState { name: None, panes: vec![pane_state.clone(), pane_state], active: 1, layout: None };
    let workspace = WorkspaceState {
        path: dir.path().to_path_buf(),
        name: None,
        worktree: false,
        tabs: vec![tab],
        active: 0,
        base: None,
        collapsed: false,
        run: None,
        role_worktrees: Vec::new(),
        run_dirs: Vec::new(),
    };
    let project = ProjectState {
        path: dir.path().to_path_buf(),
        name: None,
        group: None,
        workspaces: vec![workspace],
        active: 0,
        collapsed: false,
    };
    let saved = State {
        version: state::VERSION,
        groups: Vec::new(),
        projects: vec![project],
        active: 0,
        widths: None,
        issues: None,
        changes: None,
        todo: false,
        files: false,
        flow: false,
    };

    app.restore(&saved, AREA);

    assert_eq!(
        (rects(&app).len(), tab_term(&app, 0, 0).id, app.tab().map(|t| t.active)),
        (2, app.tab().expect("tab").panes[1].id, Some(1))
    );
}
